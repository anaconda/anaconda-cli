//! `package_info`: pre-install intelligence for a conda package.
//!
//! Ported from bigbend `apps/remote-mcp/remote_mcp/services/package_info.py`
//! (free-tier path). Calls the public package routes:
//!
//! 1. `GET /api/ce/packages/{channel}/artifacts/conda/{name}`, once per
//!    default channel, in parallel, to find the best channel.
//! 2. `GET …/{name}/files?version={v}&limit=200` for build-level detail.
//!
//! Every response is parsed tolerantly as `serde_json::Value`: a missing or
//! oddly typed field degrades to `null` or a note, never to a hard failure.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use futures_util::future::join_all;
use reqwest::StatusCode;
use serde_json::Value;

use super::schemas::{BuildVariant, CveSummary, DownloadCount, PackageInfoResponse, ToolError};
use super::state::{REQUEST_TIMEOUT, ServerState};

pub const DEFAULT_CHANNELS: &[&str] = &["main", "main-x", "msys2"];
const DEPRECATED_CHANNELS: &[&str] = &["r"];
const CONDA_RECORD_KEYS: &[&str] = &["repodata_record.json", "index.json"];
pub const MAX_PACKAGE_NAME_LENGTH: usize = 256;
pub const MAX_FILTER_VALUE_LENGTH: usize = 256;
const FILES_PAGE_LIMIT: u64 = 200;
/// Upper bound for the follow-up `/files` request for one version.
const MAX_FILES_LIMIT: u64 = 1000;
const CRITICAL_CVSS_THRESHOLD: f64 = 9.0;
const ACTIONABLE_CVE_STATUSES: &[&str] = &["active", "reported"];

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Tool arguments after trimming; all are exact-match filters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedArguments {
    pub name: String,
    pub version: Option<String>,
    pub channel: Option<String>,
    pub platform: Option<String>,
    pub python_version: Option<String>,
}

pub fn validate_arguments(
    name: &str,
    version: Option<&str>,
    channel: Option<&str>,
    platform: Option<&str>,
    python_version: Option<&str>,
) -> Result<ValidatedArguments, ToolError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ToolError::invalid_request(
            "name must be a non-empty, non-whitespace string",
        ));
    }
    if name.chars().count() > MAX_PACKAGE_NAME_LENGTH {
        return Err(ToolError::invalid_request(format!(
            "name must be no longer than {MAX_PACKAGE_NAME_LENGTH} characters"
        )));
    }
    let checked = |field: &str, value: Option<&str>| -> Result<Option<String>, ToolError> {
        let Some(value) = value else {
            return Ok(None);
        };
        let stripped = value.trim();
        if stripped.chars().count() > MAX_FILTER_VALUE_LENGTH {
            return Err(ToolError::invalid_request(format!(
                "{field} must be no longer than {MAX_FILTER_VALUE_LENGTH} characters"
            )));
        }
        // A whitespace-only filter would be an empty exact-match term that can
        // never match, which reads back as a missing package.
        if stripped.is_empty() {
            return Err(ToolError::invalid_request(format!(
                "{field} must not be empty or whitespace when provided"
            )));
        }
        Ok(Some(stripped.to_string()))
    };
    Ok(ValidatedArguments {
        name: name.to_string(),
        version: checked("version", version)?,
        channel: checked("channel", channel)?,
        platform: checked("platform", platform)?,
        python_version: checked("python_version", python_version)?,
    })
}

/// Reject anything that is not a plain conda name segment before it is
/// interpolated into a request path.
pub fn checked_segment<'a>(value: &'a str, field: &str) -> Result<&'a str, ToolError> {
    let safe = !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        && value != "."
        && value != "..";
    if safe {
        Ok(value)
    } else {
        Err(ToolError::invalid_request(format!(
            "{field} may only contain letters, digits, '.', '_' and '-', and may not be '.' or '..'"
        )))
    }
}

// ---------------------------------------------------------------------------
// Versions
// ---------------------------------------------------------------------------

/// Compare conda versions with `VersionOrder` semantics, falling back to a
/// plain string comparison when either side does not parse.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    use rattler_conda_types::Version;
    match (Version::from_str(a), Version::from_str(b)) {
        (Ok(a), Ok(b)) => a.cmp(&b),
        _ => a.cmp(b),
    }
}

// ---------------------------------------------------------------------------
// Pure extraction helpers over `/files` items
// ---------------------------------------------------------------------------

fn str_at<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

/// The repodata record carried by a `/files` item, or an empty object.
fn conda_record(item: &Value) -> &Value {
    static EMPTY: Value = Value::Null;
    let metadata = item.get("metadata");
    CONDA_RECORD_KEYS
        .iter()
        .filter_map(|key| metadata.and_then(|m| m.get(*key)))
        .find(|record| record.as_object().is_some_and(|o| !o.is_empty()))
        .unwrap_or(&EMPTY)
}

fn depends(item: &Value) -> impl Iterator<Item = &str> {
    conda_record(item)
        .get("depends")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

/// Parse `python[ ][>=|==|=][ ]X.Y…` and return `X.Y`.
fn python_major_minor(dep: &str) -> Option<&str> {
    let rest = dep.strip_prefix("python")?;
    let rest = rest.trim_start();
    let rest = rest
        .strip_prefix(">=")
        .or_else(|| rest.strip_prefix("=="))
        .or_else(|| rest.strip_prefix('='))
        .unwrap_or(rest)
        .trim_start();
    let major_len = rest.bytes().take_while(u8::is_ascii_digit).count();
    if major_len == 0 || rest.as_bytes().get(major_len) != Some(&b'.') {
        return None;
    }
    let minor_len = rest[major_len + 1..]
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    (minor_len > 0).then(|| &rest[..major_len + 1 + minor_len])
}

fn numeric_key(version: &str) -> Vec<u64> {
    version
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

/// Python versions supported by the builds.
///
/// Per-version builds (`python >=3.12,<3.13.0a0`) yield `["3.11", "3.12"]`.
/// noarch builds (`python >=3.10`) yield the raw constraint `[">=3.10"]` so the
/// agent knows the package is not pinned to one version.
pub fn extract_python_versions(items: &[Value]) -> Option<Vec<String>> {
    let mut found = BTreeSet::new();
    let mut has_upper_bound = false;
    let mut raw_constraints = BTreeSet::new();
    for dep in items.iter().flat_map(depends) {
        let dep = dep.trim();
        let Some(version) = python_major_minor(dep) else {
            continue;
        };
        found.insert(version.to_string());
        if dep.contains('<') {
            has_upper_bound = true;
        } else if let Some((_, spec)) = dep.split_once(char::is_whitespace) {
            let spec = spec.trim();
            if !spec.is_empty() {
                raw_constraints.insert(spec.to_string());
            }
        }
    }
    if found.is_empty() {
        return None;
    }
    if !has_upper_bound && !raw_constraints.is_empty() {
        return Some(raw_constraints.into_iter().collect());
    }
    let mut versions: Vec<String> = found.into_iter().collect();
    versions.sort_by_key(|v| numeric_key(v));
    Some(versions)
}

/// First `>=X` lower bound in a dependency spec.
fn lower_bound(dep: &str) -> Option<&str> {
    let start = dep.find(">=")? + 2;
    let rest = &dep[start..];
    let end = rest
        .find(|c: char| c.is_whitespace() || c == ',' || c == '<')
        .unwrap_or(rest.len());
    (end > 0).then(|| &rest[..end])
}

/// Lower-bounded dependency constraints, one line per dependency. Virtual
/// packages (`__*`) and python (see `python_versions`) are skipped.
pub fn extract_notable_constraints(items: &[Value]) -> Option<Vec<String>> {
    let mut bounds: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for dep in items.iter().flat_map(depends) {
        let dep = dep.trim();
        let Some(name) = dep.split_whitespace().next() else {
            continue;
        };
        if name.starts_with("__") || name == "python" {
            continue;
        }
        if let Some(bound) = lower_bound(dep) {
            bounds.entry(name).or_default().insert(bound);
        }
    }
    if bounds.is_empty() {
        return None;
    }
    Some(
        bounds
            .into_iter()
            .map(|(name, versions)| {
                let versions: Vec<_> = versions.into_iter().collect();
                match versions.as_slice() {
                    [only] => format!("{name} >={only}"),
                    [first, .., last] => {
                        format!("{name} >={first} to >={last} (varies by python version)")
                    }
                    [] => unreachable!("entries always hold at least one bound"),
                }
            })
            .collect(),
    )
}

/// Actionable (active/reported) CVE counts, de-duplicated by id across builds.
/// Both counts are null when no item carries CVE metadata at all.
pub fn extract_cve_summary(items: &[Value]) -> CveSummary {
    let mut seen = BTreeSet::new();
    let mut actionable = 0;
    let mut critical = 0;
    let mut has_metadata = false;
    for item in items {
        let Some(cves) = item.get("metadata").and_then(|m| m.get("cves")) else {
            continue;
        };
        if cves.is_null() {
            continue;
        }
        has_metadata = true;
        for cve in cves.as_array().into_iter().flatten() {
            let Some(id) = str_at(cve, "id").filter(|id| !id.is_empty()) else {
                continue;
            };
            if !seen.insert(id.to_string()) {
                continue;
            }
            let status = str_at(cve, "status").unwrap_or_default();
            if !ACTIONABLE_CVE_STATUSES.contains(&status) {
                continue;
            }
            actionable += 1;
            if cve
                .get("score")
                .and_then(Value::as_f64)
                .is_some_and(|score| score >= CRITICAL_CVSS_THRESHOLD)
            {
                critical += 1;
            }
        }
    }
    if has_metadata {
        CveSummary {
            count: Some(actionable),
            critical: Some(critical),
        }
    } else {
        CveSummary {
            count: None,
            critical: None,
        }
    }
}

/// Strip a trailing `_?h<hex{6,}>_<digits>` build hash suffix.
fn strip_build_hash(build: &str) -> &str {
    let bytes = build.as_bytes();
    let mut i = bytes.len();
    while i > 0 && bytes[i - 1].is_ascii_digit() {
        i -= 1;
    }
    if i == bytes.len() || i == 0 || bytes[i - 1] != b'_' {
        return build;
    }
    i -= 1;
    let hex_end = i;
    while i > 0 && matches!(bytes[i - 1], b'0'..=b'9' | b'a'..=b'f') {
        i -= 1;
    }
    if hex_end - i < 6 || i == 0 || bytes[i - 1] != b'h' {
        return build;
    }
    i -= 1;
    if i > 0 && bytes[i - 1] == b'_' {
        i -= 1;
    }
    &build[..i]
}

/// Remove every `_?py\d{2,3}` occurrence.
fn strip_python_tags(label: &str) -> String {
    let bytes = label.as_bytes();
    let mut out = String::with_capacity(label.len());
    let mut i = 0;
    let python_tag_len = |at: usize| -> usize {
        if !label[at..].starts_with("py") {
            return 0;
        }
        let digits = bytes[at + 2..]
            .iter()
            .take(3)
            .take_while(|b| b.is_ascii_digit())
            .count();
        if digits >= 2 { 2 + digits } else { 0 }
    };
    while i < bytes.len() {
        let with_underscore = if bytes[i] == b'_' {
            python_tag_len(i + 1)
        } else {
            0
        };
        if with_underscore > 0 {
            i += 1 + with_underscore;
            continue;
        }
        let plain = python_tag_len(i);
        if plain > 0 {
            i += plain;
            continue;
        }
        let ch = label[i..]
            .chars()
            .next()
            .expect("index is on a char boundary");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Distinct build variants, from build-string prefixes (`cpu_mkl`) or BLAS
/// metapackage dependencies (`blas * openblas`). `None` when all builds
/// share one variant.
pub fn extract_build_variants(items: &[Value], package_name: &str) -> Option<Vec<BuildVariant>> {
    let mut build_labels = BTreeSet::new();
    let mut blas_labels = BTreeSet::new();
    for item in items {
        let record = conda_record(item);
        if let Some(build) = str_at(record, "build").filter(|b| !b.is_empty()) {
            let label = strip_python_tags(strip_build_hash(build));
            let label = label.trim_end_matches('_');
            if !label.is_empty() {
                build_labels.insert(label.to_string());
            }
        }
        for dep in depends(item) {
            let parts: Vec<&str> = dep.split_whitespace().collect();
            if let ["blas", _, variant] = parts.as_slice() {
                blas_labels.insert((*variant).to_string());
            }
        }
    }
    if build_labels.len() > 1 {
        return Some(
            build_labels
                .into_iter()
                .map(|label| BuildVariant {
                    selector: format!("{package_name}=*={label}*"),
                    label,
                })
                .collect(),
        );
    }
    if blas_labels.len() > 1 {
        return Some(
            blas_labels
                .into_iter()
                .map(|label| BuildVariant {
                    selector: format!("blas=*={label}"),
                    label,
                })
                .collect(),
        );
    }
    None
}

/// Keep a publisher-supplied documentation URL only if it is plain http(s)
/// with a real host and no control characters or backslashes.
pub fn safe_doc_url(doc_url: &str) -> Option<String> {
    if doc_url.is_empty() || doc_url.chars().any(|c| c == '\\' || c.is_control()) {
        return None;
    }
    let parsed = url::Url::parse(doc_url).ok()?;
    (matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some())
        .then(|| doc_url.to_string())
}

fn subdirs_from_items(items: &[Value]) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| {
            item.get("metadata")
                .and_then(|m| m.get("repodata_record.json"))
                .and_then(|r| str_at(r, "subdir"))
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn artifact_version(meta: &Value) -> Option<&str> {
    meta.get("metadata").and_then(|m| str_at(m, "version"))
}

// ---------------------------------------------------------------------------
// Channel resolution
// ---------------------------------------------------------------------------

/// The channel whose metadata will be used, plus every other channel that
/// also carries the package (newest first).
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelResolution {
    pub resolved_channel: String,
    pub artifact_meta: Value,
    pub other_channels: Vec<(String, Value)>,
}

/// Pick the best channel from parallel lookups.
///
/// `results` holds one `(channel, metadata)` per channel that carries the
/// package. An explicitly requested channel is never silently swapped for
/// another; without one, `main` is preferred and the newest alternative is
/// used when `main` lacks the package.
pub fn resolve_channel(
    preferred_channel: Option<&str>,
    results: Vec<(String, Value)>,
    any_responded: bool,
    query_name: &str,
    echo_name: &str,
) -> Result<ChannelResolution, ToolError> {
    let preferred = preferred_channel.unwrap_or("main");
    let mut preferred_meta = None;
    let mut others = Vec::new();
    for (channel, meta) in results {
        if channel == preferred {
            preferred_meta = Some(meta);
        } else {
            others.push((channel, meta));
        }
    }
    others.sort_by(|(_, a), (_, b)| {
        compare_versions(
            artifact_version(b).unwrap_or_default(),
            artifact_version(a).unwrap_or_default(),
        )
    });
    let not_deprecated = |ch: &str| !DEPRECATED_CHANNELS.contains(&ch);

    if let Some(meta) = preferred_meta {
        return Ok(ChannelResolution {
            resolved_channel: preferred.to_string(),
            artifact_meta: meta,
            other_channels: others
                .into_iter()
                .filter(|(ch, _)| not_deprecated(ch))
                .collect(),
        });
    }

    // Nothing responded at all (so nothing was found): an outage, not a
    // missing package. Checked before the explicit-channel branch so a named
    // channel is not misreported as `package_not_found` while the service is
    // down.
    if !any_responded {
        return Err(ToolError::new(
            "search_unavailable",
            format!("Could not reach the primary channel ({preferred}). Try again later."),
        )
        .with_name(echo_name));
    }

    if let Some(requested) = preferred_channel {
        let available: Vec<&str> = others
            .iter()
            .map(|(ch, _)| ch.as_str())
            .filter(|ch| not_deprecated(ch))
            .collect();
        let hint = if available.is_empty() {
            String::new()
        } else {
            format!(" Available in: {}.", available.join(", "))
        };
        return Err(ToolError::new(
            "package_not_found",
            format!("No package matching {query_name:?} was found in channel {requested:?}.{hint}"),
        )
        .with_name(echo_name));
    }

    if !others.is_empty() {
        let (best_channel, best_meta) = others.remove(0);
        return Ok(ChannelResolution {
            resolved_channel: best_channel,
            artifact_meta: best_meta,
            other_channels: others
                .into_iter()
                .filter(|(ch, _)| not_deprecated(ch))
                .collect(),
        });
    }

    Err(ToolError::new(
        "package_not_found",
        format!("No package matching {query_name:?} was found in any configured channel."),
    )
    .with_name(echo_name))
}

// ---------------------------------------------------------------------------
// Response assembly
// ---------------------------------------------------------------------------

/// Build the response from the resolved channel and the `/files` payload
/// (`None` when that request failed).
pub fn assemble(
    args: &ValidatedArguments,
    echo_name: &str,
    resolution: &ChannelResolution,
    files: Option<&Value>,
) -> Result<PackageInfoResponse, ToolError> {
    let channel = resolution.resolved_channel.as_str();
    let meta = &resolution.artifact_meta;
    let metadata = meta.get("metadata").unwrap_or(&Value::Null);
    let latest_version = str_at(metadata, "version");
    let resolved_version = args.version.as_deref().or(latest_version);
    let items: &[Value] = files
        .and_then(|f| f.get("items"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let total_files = files
        .and_then(|f| f.get("total_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let mut notes: Vec<String> = resolution
        .other_channels
        .iter()
        .map(|(ch, m)| {
            format!(
                "Also available on: {ch} ({})",
                artifact_version(m).unwrap_or_default()
            )
        })
        .collect();

    if let Some(requested) = &args.version {
        if files.is_none() {
            notes.push("Could not retrieve build file details for this version.".to_string());
        } else if items.is_empty() && total_files == 0 {
            let available = string_list(meta.get("available_versions"));
            let hint = if available.is_empty() {
                latest_version
                    .map(|latest| format!(" Latest available version: {latest}"))
                    .unwrap_or_default()
            } else {
                let mut hint = format!(
                    " Available versions: {}",
                    available
                        .iter()
                        .take(10)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                if available.len() > 10 {
                    hint.push_str(&format!(" (and {} more)", available.len() - 10));
                }
                hint
            };
            return Err(ToolError::new(
                "version_not_found",
                format!(
                    "Version {requested:?} does not exist for package {:?} in channel \
                     {channel:?}.{hint}",
                    args.name
                ),
            )
            .with_name(echo_name));
        }
    }

    if !items.is_empty() && total_files > items.len() as u64 {
        notes.push(format!(
            "Build details cover {} of {total_files} files; platform, Python, and dependency \
             information may be incomplete.",
            items.len()
        ));
    }

    let platforms = {
        let from_items = subdirs_from_items(items);
        if from_items.is_empty() {
            string_list(metadata.get("subdirs"))
        } else {
            from_items
        }
    };
    let python_versions = extract_python_versions(items);

    if DEPRECATED_CHANNELS.contains(&channel) {
        notes.insert(
            0,
            format!("Channel '{channel}' is deprecated. Packages are no longer updated."),
        );
    }
    if let Some(latest) = latest_version
        && resolved_version != Some(latest)
    {
        notes.push(format!(
            "A newer version ({latest}) is available in {channel}."
        ));
    }
    if let Some(platform) = &args.platform
        && !platforms.contains(platform)
    {
        notes.push(format!(
            "Not available on {platform}. Available platforms: {}.",
            platforms.join(", ")
        ));
    }
    if let (Some(python), Some(supported)) = (&args.python_version, &python_versions)
        && !supported.contains(python)
        && !supported.iter().any(|v| v.starts_with(">="))
    {
        notes.push(format!(
            "Not available for Python {python}. Available: {}.",
            supported.join(", ")
        ));
    }

    let total_downloads = meta.get("download_count").and_then(Value::as_i64);
    Ok(PackageInfoResponse {
        name: args.name.clone(),
        version: resolved_version.map(str::to_string),
        channel: channel.to_string(),
        platforms,
        python_versions,
        license: str_at(metadata, "license").map(str::to_string),
        notable_constraints: extract_notable_constraints(items),
        build_variants: extract_build_variants(items, &args.name),
        summary: str_at(metadata, "summary").map(str::to_string),
        doc_url: str_at(metadata, "doc_url").and_then(safe_doc_url),
        download_count: DownloadCount {
            total: total_downloads,
            channel: total_downloads.map(|_| channel.to_string()),
        },
        cve_summary: extract_cve_summary(items),
        policy_status: None,
        policy_detail: None,
        notes,
    })
}

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------

/// Outcome of a single channel lookup.
enum Lookup {
    Found(String, Value),
    Missing,
    Unauthorized,
    Failed,
}

async fn lookup_channel(state: &ServerState, channel: &str, name: &str) -> Lookup {
    let path = format!("/api/ce/packages/{channel}/artifacts/conda/{name}");
    let response = match state
        .client
        .get(&path)
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
    {
        Ok(response) => response,
        Err(e) => {
            tracing::warn!(channel, error = %e, "package lookup failed");
            return Lookup::Failed;
        }
    };
    match response.status() {
        StatusCode::OK => match response.json::<Value>().await {
            Ok(meta) => Lookup::Found(channel.to_string(), meta),
            Err(e) => {
                tracing::warn!(channel, error = %e, "package lookup returned invalid JSON");
                Lookup::Failed
            }
        },
        StatusCode::UNAUTHORIZED => Lookup::Unauthorized,
        _ => Lookup::Missing,
    }
}

/// Fetch all `/files` items for one version. `None` on any failure.
async fn fetch_files(
    state: &ServerState,
    channel: &str,
    name: &str,
    version: Option<&str>,
) -> Option<Value> {
    let path = format!("/api/ce/packages/{channel}/artifacts/conda/{name}/files");
    let request = |limit: u64| {
        let mut query = vec![("limit", limit.to_string())];
        if let Some(version) = version {
            query.push(("version", version.to_string()));
        }
        state
            .client
            .get(&path)
            .query(&query)
            .timeout(REQUEST_TIMEOUT)
            .send()
    };
    let read = |result: reqwest_middleware::Result<reqwest::Response>| async {
        match result {
            Ok(response) if response.status() == StatusCode::OK => {
                response.json::<Value>().await.ok()
            }
            Ok(response) => {
                tracing::warn!(channel, status = %response.status(), "files lookup failed");
                None
            }
            Err(e) => {
                tracing::warn!(channel, error = %e, "files lookup failed");
                None
            }
        }
    };
    let first = read(request(FILES_PAGE_LIMIT).await).await?;
    let total = first
        .get("total_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    // Without a version the listing spans every release, so the first page
    // is a sample, not something to fetch in full.
    if total <= FILES_PAGE_LIMIT || version.is_none() {
        return Some(first);
    }
    // Large variant matrices (e.g. pytorch) exceed one page. Fetch more, but
    // never an unbounded amount based on a server-supplied count; `assemble`
    // notes when the result is still partial. Keep the first page if the
    // larger request fails.
    let limit = total.min(MAX_FILES_LIMIT);
    read(request(limit).await).await.or(Some(first))
}

/// Tool entry point.
pub async fn package_info(
    state: &ServerState,
    name: &str,
    version: Option<&str>,
    channel: Option<&str>,
    platform: Option<&str>,
    python_version: Option<&str>,
) -> Result<PackageInfoResponse, ToolError> {
    let args = validate_arguments(name, version, channel, platform, python_version)?;
    if !state.has_credentials() {
        return Err(ToolError::unauthenticated());
    }

    let checked_name =
        checked_segment(&args.name, "package_name").map_err(|e| e.with_name(name))?;
    let preferred = args.channel.as_deref().unwrap_or("main");
    let mut channels = vec![preferred];
    channels.extend(
        DEFAULT_CHANNELS
            .iter()
            .copied()
            .filter(|ch| *ch != preferred),
    );
    for ch in &channels {
        checked_segment(ch, "channel").map_err(|e| e.with_name(name))?;
    }

    let lookups = join_all(
        channels
            .iter()
            .map(|ch| lookup_channel(state, ch, checked_name)),
    )
    .await;

    let mut found = Vec::new();
    let mut any_responded = false;
    for lookup in lookups {
        match lookup {
            Lookup::Found(ch, meta) => {
                any_responded = true;
                found.push((ch, meta));
            }
            Lookup::Missing => any_responded = true,
            Lookup::Unauthorized => return Err(ToolError::unauthenticated()),
            Lookup::Failed => {}
        }
    }

    let resolution = resolve_channel(
        args.channel.as_deref(),
        found,
        any_responded,
        &args.name,
        name,
    )?;
    let resolved_version = args
        .version
        .as_deref()
        .or_else(|| artifact_version(&resolution.artifact_meta))
        .map(str::to_string);
    let files = fetch_files(
        state,
        &resolution.resolved_channel,
        checked_name,
        resolved_version.as_deref(),
    )
    .await;

    assemble(&args, name, &resolution, files.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(subdir: &str, build: &str, depends: &[&str]) -> Value {
        json!({
            "metadata": {
                "repodata_record.json": {
                    "subdir": subdir,
                    "build": build,
                    "depends": depends,
                }
            }
        })
    }

    fn numpy_meta(version: &str) -> Value {
        json!({
            "name": "numpy",
            "download_count": 1234,
            "available_versions": ["2.4.1", "2.3.0"],
            "metadata": {
                "version": version,
                "subdirs": ["linux-64", "osx-arm64"],
                "license": "BSD-3-Clause",
                "summary": "Array processing.",
                "doc_url": "https://numpy.org/doc/stable/",
            },
        })
    }

    fn args(name: &str) -> ValidatedArguments {
        validate_arguments(name, None, None, None, None).unwrap()
    }

    #[test]
    fn validates_name() {
        for bad in ["", "   "] {
            assert_eq!(
                validate_arguments(bad, None, None, None, None)
                    .unwrap_err()
                    .error,
                "invalid_request"
            );
        }
        assert!(validate_arguments(&"x".repeat(257), None, None, None, None).is_err());
        let padded = format!("  {}  ", "x".repeat(257));
        assert!(validate_arguments(&padded, None, None, None, None).is_err());
        assert_eq!(
            validate_arguments("  numpy  ", None, None, None, None)
                .unwrap()
                .name,
            "numpy"
        );
    }

    #[test]
    fn rejects_whitespace_and_oversized_filters() {
        assert!(validate_arguments("numpy", Some("  "), None, None, None).is_err());
        assert!(validate_arguments("numpy", None, Some(""), None, None).is_err());
        let long = "x".repeat(257);
        assert!(validate_arguments("numpy", None, None, Some(&long), None).is_err());
        let ok = validate_arguments("numpy", Some(" 2.0 "), None, None, None).unwrap();
        assert_eq!(ok.version.as_deref(), Some("2.0"));
    }

    #[test]
    fn checked_segment_rejects_traversal() {
        for bad in ["..", ".", "a/b", "a b", "", "a%2f"] {
            assert!(checked_segment(bad, "name").is_err(), "{bad}");
        }
        for good in ["numpy", "scikit-learn", "python_abi", "ca-certificates.1"] {
            assert!(checked_segment(good, "name").is_ok(), "{good}");
        }
    }

    #[test]
    fn compares_conda_versions() {
        assert_eq!(compare_versions("1.10", "1.9"), Ordering::Greater);
        assert_eq!(compare_versions("1.0.post1", "1.0"), Ordering::Greater);
        assert_eq!(compare_versions("1.0.dev1", "1.0"), Ordering::Less);
        assert_eq!(compare_versions("2.0", "2.0.0"), Ordering::Equal);
    }

    #[test]
    fn python_versions_per_version_builds() {
        let items = [
            item("linux-64", "py311h1_0", &["python >=3.11,<3.12.0a0"]),
            item("linux-64", "py310h1_0", &["python >=3.10,<3.11.0a0"]),
            item("linux-64", "py312h1_0", &["python >=3.12,<3.13.0a0"]),
        ];
        assert_eq!(
            extract_python_versions(&items).unwrap(),
            ["3.10", "3.11", "3.12"]
        );
    }

    #[test]
    fn python_versions_noarch() {
        let items = [item("noarch", "pyhd8ed1ab_0", &["python >=3.10"])];
        assert_eq!(extract_python_versions(&items).unwrap(), [">=3.10"]);
    }

    #[test]
    fn python_versions_ignores_similar_names() {
        let items = [item(
            "linux-64",
            "b",
            &["python_abi 3.12.* *_cp312", "pythonnet 3.0"],
        )];
        assert_eq!(extract_python_versions(&items), None);
    }

    #[test]
    fn notable_constraints() {
        let items = [
            item(
                "linux-64",
                "a",
                &["numpy >=1.22.4,<2", "__glibc >=2.17", "python >=3.10,<3.11"],
            ),
            item("linux-64", "b", &["numpy >=1.23.2,<2", "tzdata >=2022a"]),
        ];
        assert_eq!(
            extract_notable_constraints(&items).unwrap(),
            [
                "numpy >=1.22.4 to >=1.23.2 (varies by python version)",
                "tzdata >=2022a",
            ]
        );
    }

    #[test]
    fn cve_summary_counts_actionable_unique() {
        let items = [
            json!({"metadata": {"cves": [
                {"id": "CVE-1", "status": "active", "score": 9.8},
                {"id": "CVE-2", "status": "cleared", "score": 9.9},
                {"id": "CVE-3", "status": "reported", "score": 5.0},
            ]}}),
            json!({"metadata": {"cves": [{"id": "CVE-1", "status": "active", "score": 9.8}]}}),
        ];
        assert_eq!(
            extract_cve_summary(&items),
            CveSummary {
                count: Some(2),
                critical: Some(1)
            }
        );
        assert_eq!(
            extract_cve_summary(&[json!({"metadata": {}})]),
            CveSummary {
                count: None,
                critical: None
            }
        );
        assert_eq!(
            extract_cve_summary(&[json!({"metadata": {"cves": []}})]),
            CveSummary {
                count: Some(0),
                critical: Some(0)
            }
        );
    }

    #[test]
    fn build_label_stripping() {
        assert_eq!(
            strip_build_hash("cpu_mkl_py312h0452e45_104"),
            "cpu_mkl_py312"
        );
        assert_eq!(strip_build_hash("py312h8456e4e_0"), "py312");
        assert_eq!(strip_build_hash("h1234_0"), "h1234_0");
        assert_eq!(
            strip_build_hash("cuda118_py310_h0123456_1"),
            "cuda118_py310"
        );
        assert_eq!(strip_python_tags("cpu_mkl_py312"), "cpu_mkl");
        assert_eq!(strip_python_tags("py312"), "");
        assert_eq!(strip_python_tags("cuda118_py310"), "cuda118");
    }

    #[test]
    fn build_variants_from_build_strings() {
        let items = [
            item("linux-64", "cpu_mkl_py312h0452e45_104", &[]),
            item("linux-64", "cuda126_py312h0452e45_104", &[]),
            item("linux-64", "cpu_mkl_py311h0452e45_104", &[]),
        ];
        assert_eq!(
            extract_build_variants(&items, "pytorch").unwrap(),
            [
                BuildVariant {
                    label: "cpu_mkl".into(),
                    selector: "pytorch=*=cpu_mkl*".into()
                },
                BuildVariant {
                    label: "cuda126".into(),
                    selector: "pytorch=*=cuda126*".into()
                },
            ]
        );
    }

    #[test]
    fn build_variants_from_blas() {
        let items = [
            item("linux-64", "py312h8456e4e_0", &["blas 1.0 mkl"]),
            item("linux-64", "py312h8456e4e_1", &["blas * openblas"]),
        ];
        let variants = extract_build_variants(&items, "numpy").unwrap();
        assert_eq!(variants[0].selector, "blas=*=mkl");
        assert_eq!(variants[1].label, "openblas");
    }

    #[test]
    fn build_variants_none_when_uniform() {
        let items = [
            item("linux-64", "py312h8456e4e_0", &[]),
            item("osx-arm64", "py311h8456e4e_0", &[]),
        ];
        assert_eq!(extract_build_variants(&items, "numpy"), None);
    }

    #[test]
    fn doc_url_safety() {
        assert!(safe_doc_url("https://numpy.org/doc").is_some());
        for bad in [
            "javascript:alert(1)",
            "data:text/html,x",
            "file:///etc/passwd",
            "http:/\\evil.example",
            "https://a.example/\nx",
            "",
        ] {
            assert_eq!(safe_doc_url(bad), None, "{bad}");
        }
    }

    #[test]
    fn resolve_prefers_main() {
        let res = resolve_channel(
            None,
            vec![
                ("main-x".into(), numpy_meta("2.5.0")),
                ("main".into(), numpy_meta("2.4.1")),
            ],
            true,
            "numpy",
            "numpy",
        )
        .unwrap();
        assert_eq!(res.resolved_channel, "main");
        assert_eq!(res.other_channels[0].0, "main-x");
    }

    #[test]
    fn resolve_falls_through_to_newest_alternative() {
        let res = resolve_channel(
            None,
            vec![
                ("msys2".into(), numpy_meta("1.0")),
                ("main-x".into(), numpy_meta("2.0")),
            ],
            true,
            "numpy",
            "numpy",
        )
        .unwrap();
        assert_eq!(res.resolved_channel, "main-x");
        assert_eq!(res.other_channels.len(), 1);
    }

    #[test]
    fn resolve_never_switches_explicit_channel() {
        let err = resolve_channel(
            Some("main"),
            vec![("main-x".into(), numpy_meta("2.0"))],
            true,
            "numpy",
            " numpy ",
        )
        .unwrap_err();
        assert_eq!(err.error, "package_not_found");
        assert!(err.message.contains("Available in: main-x."));
        assert_eq!(err.name.as_deref(), Some(" numpy "));
    }

    #[test]
    fn resolve_reports_unreachable_vs_missing() {
        let err = resolve_channel(None, vec![], false, "numpy", "numpy").unwrap_err();
        assert_eq!(err.error, "search_unavailable");
        let err = resolve_channel(None, vec![], true, "numpy", "numpy").unwrap_err();
        assert_eq!(err.error, "package_not_found");
    }

    #[test]
    fn resolve_explicit_channel_outage_is_search_unavailable() {
        let err = resolve_channel(Some("main-x"), vec![], false, "numpy", "numpy").unwrap_err();
        assert_eq!(err.error, "search_unavailable");
        assert!(err.message.contains("(main-x)"));
        let err = resolve_channel(Some("main-x"), vec![], true, "numpy", "numpy").unwrap_err();
        assert_eq!(err.error, "package_not_found");
    }

    fn resolution(meta: Value) -> ChannelResolution {
        ChannelResolution {
            resolved_channel: "main".into(),
            artifact_meta: meta,
            other_channels: vec![("main-x".into(), numpy_meta("2.5.0"))],
        }
    }

    #[test]
    fn assemble_full_response() {
        let files = json!({"total_count": 2, "items": [
            item("linux-64", "py312h8456e4e_0", &["python >=3.12,<3.13.0a0"]),
            item("osx-arm64", "py312h8456e4e_0", &["python >=3.12,<3.13.0a0"]),
        ]});
        let response = assemble(
            &args("numpy"),
            "numpy",
            &resolution(numpy_meta("2.4.1")),
            Some(&files),
        )
        .unwrap();
        assert_eq!(response.version.as_deref(), Some("2.4.1"));
        assert_eq!(response.platforms, ["linux-64", "osx-arm64"]);
        assert_eq!(response.python_versions.unwrap(), ["3.12"]);
        assert_eq!(response.download_count.total, Some(1234));
        assert_eq!(response.download_count.channel.as_deref(), Some("main"));
        assert_eq!(response.notes, ["Also available on: main-x (2.5.0)"]);
        assert_eq!(response.cve_summary.count, None);
    }

    #[test]
    fn assemble_notes_older_version_platform_and_python() {
        let mut a =
            validate_arguments("numpy", Some("2.3.0"), None, Some("win-arm64"), Some("3.9"))
                .unwrap();
        a.channel = None;
        let files = json!({"total_count": 1, "items": [
            item("linux-64", "py312h8456e4e_0", &["python >=3.12,<3.13.0a0"]),
        ]});
        let response =
            assemble(&a, "numpy", &resolution(numpy_meta("2.4.1")), Some(&files)).unwrap();
        assert_eq!(response.version.as_deref(), Some("2.3.0"));
        assert!(
            response
                .notes
                .contains(&"A newer version (2.4.1) is available in main.".to_string())
        );
        assert!(
            response.notes.contains(
                &"Not available on win-arm64. Available platforms: linux-64.".to_string()
            )
        );
        assert!(
            response
                .notes
                .contains(&"Not available for Python 3.9. Available: 3.12.".to_string())
        );
    }

    #[test]
    fn assemble_version_not_found() {
        let a = validate_arguments("numpy", Some("9.9"), None, None, None).unwrap();
        let files = json!({"total_count": 0, "items": []});
        let err =
            assemble(&a, "numpy", &resolution(numpy_meta("2.4.1")), Some(&files)).unwrap_err();
        assert_eq!(err.error, "version_not_found");
        assert!(err.message.contains("Available versions: 2.4.1, 2.3.0"));
    }

    #[test]
    fn assemble_files_unavailable_note() {
        let a = validate_arguments("numpy", Some("2.3.0"), None, None, None).unwrap();
        let response = assemble(&a, "numpy", &resolution(numpy_meta("2.4.1")), None).unwrap();
        assert!(
            response
                .notes
                .contains(&"Could not retrieve build file details for this version.".to_string())
        );
        assert_eq!(response.platforms, ["linux-64", "osx-arm64"]);
    }

    #[test]
    fn assemble_notes_partial_build_details() {
        let files = json!({"total_count": 5000, "items": [
            item("linux-64", "py312h8456e4e_0", &[]),
        ]});
        let response = assemble(
            &args("numpy"),
            "numpy",
            &resolution(numpy_meta("2.4.1")),
            Some(&files),
        )
        .unwrap();
        assert!(
            response.notes.contains(
                &"Build details cover 1 of 5000 files; platform, Python, and dependency \
              information may be incomplete."
                    .to_string()
            )
        );
    }

    #[test]
    fn assemble_deprecated_channel_note_first() {
        let mut res = resolution(numpy_meta("2.4.1"));
        res.resolved_channel = "r".into();
        let response = assemble(&args("numpy"), "numpy", &res, None).unwrap();
        assert!(response.notes[0].contains("Channel 'r' is deprecated"));
    }
}
