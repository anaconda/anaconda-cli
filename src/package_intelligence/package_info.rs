//! `ana package info`: pre-install intelligence for a conda package.
//!
//! Ported from bigbend `apps/remote-mcp/remote_mcp/services/package_info.py`
//! (free-tier path, `_package_info_from_ce_routes`); each helper names the
//! Python function it ports. Calls the public package routes:
//!
//! 1. `GET /api/ce/packages/{channel}/artifacts/conda/{name}`, once per
//!    default channel, in parallel, to find the best channel.
//! 2. `GET …/{name}/files?version={v}&limit=200` for build-level detail.
//!
//! Every response is parsed tolerantly as `serde_json::Value`: a missing or
//! oddly typed field degrades to `null` or a note, never to a hard failure.
//! Conda versions and dependency specs are parsed with `rattler_conda_types`;
//! build strings, which conda treats as opaque, use Python's regexes.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::sync::LazyLock;

use futures_util::future::join_all;
use rattler_conda_types::version_spec::{EqualityOperator, RangeOperator, StrictRangeOperator};
use rattler_conda_types::{MatchSpec, PackageNameMatcher, ParseStrictness, Version, VersionSpec};
use regex::Regex;
use reqwest::StatusCode;
use serde_json::Value;

use super::REQUEST_TIMEOUT;
use super::errors::ToolError;
use super::org;
use super::schemas::{BuildVariant, CveSummary, DownloadCount, PackageInfoResponse};
use crate::context::CommandContext;

pub const DEFAULT_CHANNELS: &[&str] = &["main", "main-x", "msys2"];
const DEPRECATED_CHANNELS: &[&str] = &["r"];
const CONDA_RECORD_KEYS: &[&str] = &["repodata_record.json", "index.json"];
const FILES_PAGE_LIMIT: u64 = 200;
/// Upper bound for the follow-up `/files` request for one version.
const MAX_FILES_LIMIT: u64 = 1000;
const CRITICAL_CVSS_THRESHOLD: f64 = 9.0;
const ACTIONABLE_CVE_STATUSES: &[&str] = &["active", "reported"];

// ---------------------------------------------------------------------------
// Arguments
// ---------------------------------------------------------------------------

/// What to look up. Values arrive already trimmed and validated by the clap
/// value parsers in `commands.rs`; all are exact-match filters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageQuery {
    pub name: String,
    pub version: Option<String>,
    pub channel: Option<String>,
    pub platform: Option<String>,
    pub python_version: Option<String>,
    pub org: Option<String>,
}

/// Rule enforced by [`is_safe_segment`], phrased for error messages.
pub const SEGMENT_RULE: &str =
    "may only contain letters, digits, '.', '_' and '-', and may not be '.' or '..'";

/// True for a plain conda name segment, safe to interpolate into a request
/// path (no `/`, `%`, whitespace, or `.`/`..` traversal).
pub fn is_safe_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        && value != "."
        && value != ".."
}

/// Guard applied right before a value is put into a request path. clap
/// already rejects unsafe names; this keeps the URL safe for any caller.
fn check_segment(value: &str, field: &str) -> Result<(), ToolError> {
    if is_safe_segment(value) {
        Ok(())
    } else {
        Err(ToolError::invalid_request(format!(
            "{field} {SEGMENT_RULE}"
        )))
    }
}

// ---------------------------------------------------------------------------
// Versions
// ---------------------------------------------------------------------------

/// Port of `compare_versions` (`domain/package_models.py`): conda
/// `VersionOrder` semantics via rattler's `Version`, falling back to a plain
/// string comparison when either side does not parse.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    match (Version::from_str(a), Version::from_str(b)) {
        (Ok(a), Ok(b)) => a.cmp(&b),
        _ => a.cmp(b),
    }
}

// ---------------------------------------------------------------------------
// Pure extraction helpers over `/files` items
// ---------------------------------------------------------------------------

/// String value of `key` in a JSON object, if present and a string.
///
/// The `'a` says the returned `&str` borrows from `value` (not from `key`):
/// with two reference inputs Rust cannot infer which one the result points
/// into, so it must be spelled out. No copy is made.
fn str_at<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

/// Port of `_conda_record_from_item`: the repodata record carried by a
/// `/files` item, or `null`. `repodata_record.json` is preferred; an empty
/// record falls through to `index.json`.
fn conda_record(item: &Value) -> &Value {
    static EMPTY: Value = Value::Null;
    let metadata = item.get("metadata");
    CONDA_RECORD_KEYS
        .iter()
        .filter_map(|key| metadata.and_then(|m| m.get(*key)))
        .find(|record| record.as_object().is_some_and(|o| !o.is_empty()))
        .unwrap_or(&EMPTY)
}

/// Raw dependency strings of one item, from its conda record's `depends`
/// array. Missing records, a missing or non-array `depends`, and non-string
/// entries yield nothing.
fn depends(item: &Value) -> impl Iterator<Item = &str> {
    conda_record(item)
        .get("depends")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

/// A dependency string parsed by rattler as a conda match spec. Python uses
/// regexes here; a real parser handles `3.12.*`, `|` alternatives, and
/// spacing correctly. `None` if it does not parse or does not name a single
/// exact package (globs, regex names).
fn parse_dependency(raw: &str) -> Option<MatchSpec> {
    let spec = MatchSpec::from_str(raw.trim(), ParseStrictness::Lenient).ok()?;
    matches!(spec.name, PackageNameMatcher::Exact(_)).then_some(spec)
}

/// The exact package name of a parsed dependency, as written.
fn package_name(spec: &MatchSpec) -> Option<&str> {
    match &spec.name {
        PackageNameMatcher::Exact(name) => Some(name.as_source()),
        _ => None,
    }
}

/// Every dependency of every item, parsed. Unparseable strings are skipped.
fn parsed_depends(items: &[Value]) -> impl Iterator<Item = MatchSpec> {
    items.iter().flat_map(depends).filter_map(parse_dependency)
}

/// The constraint text after the package name, exactly as written: `>=3.10`
/// for `python >=3.10`, `>=3.10 *_cpython` when a build string follows.
/// `None` when the dependency is a bare name. Used for noarch Python
/// constraints, which are reported verbatim like the remote service does.
fn raw_constraint(raw: &str) -> Option<&str> {
    raw.trim()
        .split_once(char::is_whitespace)
        .map(|(_, rest)| rest.trim())
        .filter(|rest| !rest.is_empty())
}

/// Leaf constraints of a version spec, in source order.
fn leaves(spec: &VersionSpec) -> Vec<&VersionSpec> {
    match spec {
        VersionSpec::Group(_, specs) => specs.iter().flat_map(leaves).collect(),
        leaf => vec![leaf],
    }
}

/// `X.Y` of the first constraint that pins a minimum or exact Python
/// version (`>=3.12`, `==3.12`, `3.12`, `3.12.*`). Matches Python's
/// `^python\s*(?:>=|==|=)?\s*(\d+\.\d+)`, which only looks at the first one.
fn python_major_minor(spec: &VersionSpec) -> Option<String> {
    let version = match leaves(spec).first()? {
        VersionSpec::Range(RangeOperator::GreaterEquals, v)
        | VersionSpec::Exact(EqualityOperator::Equals, v) => v,
        VersionSpec::StrictRange(StrictRangeOperator::StartsWith, v) => &v.0,
        _ => return None,
    };
    let (major, minor) = version.as_major_minor()?;
    Some(format!("{major}.{minor}"))
}

fn has_upper_bound(spec: &VersionSpec) -> bool {
    leaves(spec).iter().any(|leaf| {
        matches!(
            leaf,
            VersionSpec::Range(RangeOperator::Less | RangeOperator::LessEquals, _)
        )
    })
}

/// First `>=X` lower bound in a version spec.
fn lower_bound(spec: &VersionSpec) -> Option<String> {
    leaves(spec).into_iter().find_map(|leaf| match leaf {
        VersionSpec::Range(RangeOperator::GreaterEquals, v) => Some(v.to_string()),
        _ => None,
    })
}

/// Port of `_extract_python_versions`: Python versions supported by the
/// builds.
///
/// Per-version builds (`python >=3.12,<3.13.0a0`) yield `["3.11", "3.12"]`.
/// noarch builds (`python >=3.10`) yield the raw constraint `[">=3.10"]` so the
/// agent knows the package is not pinned to one version.
pub fn extract_python_versions(items: &[Value]) -> Option<Vec<String>> {
    let mut found = BTreeSet::new();
    let mut any_upper_bound = false;
    let mut raw_constraints = BTreeSet::new();
    for raw in items.iter().flat_map(depends) {
        let Some(spec) = parse_dependency(raw) else {
            continue;
        };
        if package_name(&spec) != Some("python") {
            continue;
        }
        let Some(version) = spec.version.as_ref() else {
            continue;
        };
        let Some(major_minor) = python_major_minor(version) else {
            continue;
        };
        found.insert(major_minor);
        if has_upper_bound(version) {
            any_upper_bound = true;
        } else if let Some(constraint) = raw_constraint(raw) {
            raw_constraints.insert(constraint.to_string());
        }
    }
    if found.is_empty() {
        return None;
    }
    if !any_upper_bound && !raw_constraints.is_empty() {
        return Some(raw_constraints.into_iter().collect());
    }
    let mut versions: Vec<String> = found.into_iter().collect();
    versions.sort_by(|a, b| compare_versions(a, b));
    Some(versions)
}

/// Port of `_extract_notable_constraints`: lower-bounded dependency
/// constraints, one line per dependency. Virtual packages (`__*`) and python
/// (see `python_versions`) are skipped.
///
/// Unlike Python, which sorts bounds as strings (`2.10.0` before `2.9.0`),
/// a range is ordered by conda version so it reads lowest to highest.
pub fn extract_notable_constraints(items: &[Value]) -> Option<Vec<String>> {
    let mut bounds: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for spec in parsed_depends(items) {
        let Some(name) = package_name(&spec) else {
            continue;
        };
        if name.starts_with("__") || name == "python" {
            continue;
        }
        if let Some(bound) = spec.version.as_ref().and_then(lower_bound) {
            bounds.entry(name.to_string()).or_default().insert(bound);
        }
    }
    if bounds.is_empty() {
        return None;
    }
    Some(
        bounds
            .into_iter()
            .map(|(name, versions)| {
                let mut versions: Vec<_> = versions.into_iter().collect();
                versions.sort_by(|a, b| compare_versions(a, b));
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

/// Port of `_extract_cve_summary`: actionable (active/reported) CVE counts,
/// de-duplicated by id across builds. Both counts are null when no item
/// carries CVE metadata at all.
pub fn extract_cve_summary(items: &[Value]) -> CveSummary {
    let cve_lists: Vec<&Value> = items
        .iter()
        .filter_map(|item| item.get("metadata")?.get("cves"))
        .filter(|cves| !cves.is_null())
        .collect();
    if cve_lists.is_empty() {
        return CveSummary {
            count: None,
            critical: None,
        };
    }
    let mut seen = BTreeSet::new();
    let actionable: Vec<&Value> = cve_lists
        .iter()
        .flat_map(|cves| cves.as_array().into_iter().flatten())
        // First occurrence of each id wins, whatever its status.
        .filter(|cve| {
            str_at(cve, "id")
                .filter(|id| !id.is_empty())
                .is_some_and(|id| seen.insert(id.to_string()))
        })
        .filter(|cve| ACTIONABLE_CVE_STATUSES.contains(&str_at(cve, "status").unwrap_or_default()))
        .collect();
    let critical = actionable
        .iter()
        .filter(|cve| {
            cve.get("score")
                .and_then(Value::as_f64)
                .is_some_and(|score| score >= CRITICAL_CVSS_THRESHOLD)
        })
        .count();
    CveSummary {
        count: Some(actionable.len() as u64),
        critical: Some(critical as u64),
    }
}

// The same patterns as Python's `_extract_build_variants`. They are constant,
// so compiling them cannot fail at runtime.
static BUILD_HASH_SUFFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"_?h[0-9a-f]{6,}_\d+$").expect("valid regex"));
static PYTHON_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"_?py\d{2,3}").expect("valid regex"));
static BLAS_DEPENDENCY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^blas\s+\S+\s+(\S+)$").expect("valid regex"));

/// Variant label of a build string: `cpu_mkl_py312h0452e45_104` -> `cpu_mkl`.
/// conda treats build strings as opaque, so rattler has nothing for this.
fn build_label(build: &str) -> String {
    let without_hash = BUILD_HASH_SUFFIX.replace(build, "");
    let without_python = PYTHON_TAG.replace_all(&without_hash, "");
    without_python.trim_end_matches('_').to_string()
}

/// Port of `_extract_build_variants`: distinct build variants, from
/// build-string prefixes (`cpu_mkl`) or BLAS metapackage dependencies
/// (`blas * openblas`). `None` when all builds share one variant.
pub fn extract_build_variants(items: &[Value], package_name: &str) -> Option<Vec<BuildVariant>> {
    let build_labels: BTreeSet<String> = items
        .iter()
        .filter_map(|item| str_at(conda_record(item), "build"))
        .filter(|build| !build.is_empty())
        .map(build_label)
        .filter(|label| !label.is_empty())
        .collect();
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
    let blas_labels: BTreeSet<String> = items
        .iter()
        .flat_map(depends)
        .filter_map(|dep| BLAS_DEPENDENCY.captures(dep))
        .map(|captures| captures[1].to_string())
        .collect();
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

/// Port of `_safe_doc_url`: keep a publisher-supplied documentation URL only
/// if it is plain http(s) with a real host and no control characters or
/// backslashes.
pub fn safe_doc_url(doc_url: &str) -> Option<String> {
    if doc_url.is_empty() || doc_url.chars().any(|c| c == '\\' || c.is_control()) {
        return None;
    }
    let parsed = url::Url::parse(doc_url).ok()?;
    (matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some())
        .then(|| doc_url.to_string())
}

/// Port of `_subdirs_from_items`.
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

/// Port of `_resolve_channel`: pick the best channel from parallel lookups.
///
/// `results` holds one `(channel, metadata)` per channel that carries the
/// package. An explicitly requested channel is never silently swapped for
/// another; without one, `main` is preferred and the newest alternative is
/// used when `main` lacks the package.
pub fn resolve_channel(
    preferred_channel: Option<&str>,
    results: Vec<(String, Value)>,
    any_responded: bool,
    name: &str,
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
        .with_name(name));
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
            format!("No package matching {name:?} was found in channel {requested:?}.{hint}"),
        )
        .with_name(name));
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
        format!("No package matching {name:?} was found in any configured channel."),
    )
    .with_name(name))
}

// ---------------------------------------------------------------------------
// Response assembly
// ---------------------------------------------------------------------------

/// True when an item is `removed_by_policy`.
fn removed_by_policy(item: &Value) -> bool {
    item.get("removed_by_policy")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Port of `_derive_policy`: summarize policy disposition for the subscriber
/// path. Returns `(policy_status, policy_detail)`.
fn derive_policy(items: &[Value]) -> (Option<String>, Option<String>) {
    let removed: Vec<&Value> = items.iter().filter(|i| removed_by_policy(i)).collect();
    if removed.len() == items.len() {
        let reasons: std::collections::BTreeSet<String> = removed
            .iter()
            .filter_map(|i| i.get("removed_reason").and_then(Value::as_str))
            .filter(|r| !r.is_empty())
            .map(str::to_string)
            .collect();
        let detail = if reasons.is_empty() {
            "Version blocked by policy.".to_string()
        } else {
            reasons.into_iter().collect::<Vec<_>>().join("; ")
        };
        (Some("blocked".to_string()), Some(detail))
    } else if !removed.is_empty() {
        (
            Some("available".to_string()),
            Some(format!(
                "{} build(s) removed by policy; {} available",
                removed.len(),
                items.len() - removed.len()
            )),
        )
    } else {
        (Some("available".to_string()), None)
    }
}

/// Port of `_resolve_version_and_assemble` and `_assemble_response`: build the
/// response from the resolved channel and the `/files` payload (`None` when
/// that request failed). `has_policy` selects the subscriber behavior
/// (policy fields and policy-filtered build details).
pub fn assemble(
    args: &PackageQuery,
    resolution: &ChannelResolution,
    files: Option<&Value>,
    has_policy: bool,
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

    // On the subscriber path, build-level details (platforms, Python,
    // constraints, variants) come from policy-available builds only; CVE
    // counts still cover every build that came back.
    let available_items: Vec<Value> = if has_policy {
        items
            .iter()
            .filter(|item| !removed_by_policy(item))
            .cloned()
            .collect()
    } else {
        items.to_vec()
    };

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
            .with_name(&args.name));
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
        if has_policy && !items.is_empty() {
            if !available_items.is_empty() {
                let from_available = subdirs_from_items(&available_items);
                if from_available.is_empty() {
                    string_list(metadata.get("subdirs"))
                } else {
                    from_available
                }
            } else {
                let from_items = subdirs_from_items(items);
                if from_items.is_empty() {
                    string_list(metadata.get("subdirs"))
                } else {
                    from_items
                }
            }
        } else {
            let from_items = subdirs_from_items(items);
            if from_items.is_empty() {
                string_list(metadata.get("subdirs"))
            } else {
                from_items
            }
        }
    };
    let python_versions = extract_python_versions(&available_items);

    let (policy_status, policy_detail) = if has_policy && !items.is_empty() {
        derive_policy(items)
    } else if has_policy {
        (
            None,
            resolved_version.map(|version| {
                format!("No build files returned for version {version}.")
            }),
        )
    } else {
        (None, None)
    };

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
        notable_constraints: extract_notable_constraints(&available_items),
        build_variants: extract_build_variants(&available_items, &args.name),
        summary: str_at(metadata, "summary").map(str::to_string),
        doc_url: str_at(metadata, "doc_url").and_then(safe_doc_url),
        download_count: DownloadCount {
            total: total_downloads,
            channel: total_downloads.map(|_| channel.to_string()),
        },
        cve_summary: extract_cve_summary(items),
        policy_status,
        policy_detail,
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
    Forbidden,
    Failed,
}

async fn lookup_channel(ctx: &CommandContext, channel: &str, name: &str) -> Lookup {
    let path = format!("/api/ce/packages/{channel}/artifacts/conda/{name}");
    let response = match ctx
        .client()
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
    ctx: &CommandContext,
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
        ctx.client()
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

/// Entry point for `ana package info`. Port of `_package_info_from_ce_routes`
/// (free path) plus the subscriber path (`--org`).
pub async fn package_info(
    ctx: &CommandContext,
    args: &PackageQuery,
) -> Result<PackageInfoResponse, ToolError> {
    if args.org.is_some() {
        return package_info_with_org(ctx, args).await;
    }
    package_info_free(ctx, args).await
}

/// Free-tier path (CE routes), unchanged behavior.
async fn package_info_free(
    ctx: &CommandContext,
    args: &PackageQuery,
) -> Result<PackageInfoResponse, ToolError> {
    if !super::has_credentials(&ctx.config) {
        return Err(ToolError::unauthenticated());
    }

    check_segment(&args.name, "package name").map_err(|e| e.with_name(&args.name))?;
    let preferred = args.channel.as_deref().unwrap_or("main");
    check_segment(preferred, "channel").map_err(|e| e.with_name(&args.name))?;
    let mut channels = vec![preferred];
    channels.extend(
        DEFAULT_CHANNELS
            .iter()
            .copied()
            .filter(|ch| *ch != preferred),
    );

    let lookups = join_all(
        channels
            .iter()
            .map(|ch| lookup_channel(ctx, ch, &args.name)),
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
            Lookup::Forbidden => {}
            Lookup::Failed => {}
        }
    }

    let resolution = resolve_channel(args.channel.as_deref(), found, any_responded, &args.name)?;
    let resolved_version = args
        .version
        .as_deref()
        .or_else(|| artifact_version(&resolution.artifact_meta))
        .map(str::to_string);
    let files = fetch_files(
        ctx,
        &resolution.resolved_channel,
        &args.name,
        resolved_version.as_deref(),
    )
    .await;

    assemble(args, &resolution, files.as_ref(), false)
}

/// Subscriber path: resolve the org from the whoami passport, look the package
/// up through the org's channels, and fold in policy disposition.
async fn package_info_with_org(
    ctx: &CommandContext,
    args: &PackageQuery,
) -> Result<PackageInfoResponse, ToolError> {
    if !super::has_credentials(&ctx.config) {
        return Err(ToolError::unauthenticated());
    }

    let org_name = args.org.as_deref().expect("org is set");
    check_segment(&args.name, "package name").map_err(|e| e.with_name(&args.name))?;

    let passport = org::fetch_whoami(ctx).await.ok_or_else(|| {
        ToolError::new(
            "upstream_error",
            "Could not retrieve your organization memberships. Try again later.",
        )
    })?;
    let resolved_org = match org::resolve_org(&passport, Some(org_name)) {
        org::OrgResolution::Resolved(org) => org,
        org::OrgResolution::Error(error) => return Err(error.with_name(&args.name)),
        org::OrgResolution::Choice(choice) => {
            return Err(ToolError::org_choice(choice.message, choice.available_orgs));
        }
        org::OrgResolution::None => {
            return Err(ToolError::new(
                "no_subscription",
                format!("Organization '{org_name}' does not have a business subscription."),
            )
            .with_name(&args.name));
        }
    };

    // Fail-closed on explicit channel requests: validate against the org's
    // configured channels (fail-open when the channel list is unavailable).
    if let Some(channel) = args.channel.as_deref() {
        check_segment(channel, "channel").map_err(|e| e.with_name(&args.name))?;
        if let Some(channels) = org::fetch_org_channels(ctx, &resolved_org.name).await {
            let names: Vec<String> = channels
                .iter()
                .map(|c| org::strip_org_prefix(&resolved_org.name, &c.name))
                .collect();
            if !names.is_empty() && !names.iter().any(|n| n == channel) {
                return Err(ToolError::new(
                    "invalid_channel",
                    format!(
                        "Channel '{channel}' is not available in organization '{}'. Available: {}.",
                        resolved_org.name,
                        names.join(", ")
                    ),
                )
                .with_name(&args.name));
            }
        }
    }

    let preferred = args.channel.as_deref().unwrap_or("main");
    let mut channels = vec![preferred];
    channels.extend(
        DEFAULT_CHANNELS
            .iter()
            .copied()
            .filter(|ch| *ch != preferred),
    );

    let lookups = join_all(
        channels
            .iter()
            .map(|ch| lookup_subscriber_channel(ctx, &resolved_org.name, ch, &args.name)),
    )
    .await;

    let mut found = Vec::new();
    let mut any_responded = false;
    let mut forbidden = false;
    for lookup in lookups {
        match lookup {
            Lookup::Found(ch, meta) => {
                any_responded = true;
                found.push((ch, meta));
            }
            Lookup::Missing => any_responded = true,
            Lookup::Forbidden => forbidden = true,
            Lookup::Unauthorized => return Err(ToolError::unauthenticated()),
            Lookup::Failed => {}
        }
    }

    if forbidden {
        // The org does not actually grant us the subscriber endpoints: fall
        // back to the public view and say so.
        let mut response = package_info_free(ctx, args).await?;
        response.notes.insert(
            0,
            "Organization channel access is unavailable; showing public package data.".to_string(),
        );
        return Ok(response);
    }

    let resolution = resolve_channel(args.channel.as_deref(), found, any_responded, &args.name)?;
    let resolved_version = args
        .version
        .as_deref()
        .or_else(|| artifact_version(&resolution.artifact_meta))
        .map(str::to_string);
    let files = org::fetch_artifact_files_for_version(
        ctx,
        &resolved_org.name,
        &resolution.resolved_channel,
        &args.name,
        resolved_version.as_deref().unwrap_or(""),
    )
    .await;

    assemble(args, &resolution, files.as_ref(), true)
}

/// Look up a package through the subscriber channel path, distinguishing a
/// 403 (org access denied) so the caller can fall back to the public view.
async fn lookup_subscriber_channel(
    ctx: &CommandContext,
    org_name: &str,
    channel: &str,
    name: &str,
) -> Lookup {
    let path = org::build_artifacts_path(org_name, channel, "");
    let response = match ctx
        .client()
        .get(&path)
        .query(&[("only_names", name)])
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
    {
        Ok(response) => response,
        Err(e) => {
            tracing::warn!(channel, error = %e, "subscriber package lookup failed");
            return Lookup::Failed;
        }
    };
    match response.status() {
        StatusCode::OK => {
            let meta = match response.json::<Value>().await.ok() {
                Some(data) => {
                    // The subscriber listing is paginated; the item list holds
                    // the artifact meta under `items[0]`.
                    match data
                        .get("items")
                        .and_then(Value::as_array)
                        .and_then(|items| items.first())
                        .cloned()
                    {
                        Some(item) => item,
                        None => return Lookup::Missing,
                    }
                }
                None => return Lookup::Failed,
            };
            Lookup::Found(channel.to_string(), meta)
        }
        StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED => {
            if response.status() == StatusCode::UNAUTHORIZED {
                Lookup::Unauthorized
            } else {
                Lookup::Forbidden
            }
        }
        StatusCode::NOT_FOUND => Lookup::Missing,
        _ => Lookup::Missing,
    }
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

    fn query(
        name: &str,
        version: Option<&str>,
        platform: Option<&str>,
        python_version: Option<&str>,
    ) -> PackageQuery {
        PackageQuery {
            name: name.into(),
            version: version.map(String::from),
            channel: None,
            platform: platform.map(String::from),
            python_version: python_version.map(String::from),
            org: None,
        }
    }

    fn args(name: &str) -> PackageQuery {
        query(name, None, None, None)
    }

    #[test]
    fn safe_segments() {
        for bad in [
            "..",
            ".",
            "a/b",
            "a b",
            "",
            "a%2f",
            "a\\b",
            "numpy?x=1",
            "é",
        ] {
            assert!(!is_safe_segment(bad), "{bad}");
        }
        for good in [
            "numpy",
            "scikit-learn",
            "python_abi",
            "ca-certificates.1",
            "_r-mutex",
        ] {
            assert!(is_safe_segment(good), "{good}");
        }
        let err = check_segment("../etc", "package name").unwrap_err();
        assert_eq!(err.error, "invalid_request");
        assert!(err.message.starts_with("package name may only contain"));
    }

    #[test]
    fn str_at_reads_only_string_values() {
        let value = json!({"a": "x", "n": 1, "z": null});
        assert_eq!(str_at(&value, "a"), Some("x"));
        assert_eq!(str_at(&value, "n"), None);
        assert_eq!(str_at(&value, "z"), None);
        assert_eq!(str_at(&value, "missing"), None);
        assert_eq!(str_at(&json!(["a"]), "a"), None);
    }

    // conda_record: which record of a `/files` item is used.

    #[test]
    fn conda_record_prefers_repodata_record() {
        let item = json!({"metadata": {
            "repodata_record.json": {"build": "from_repodata"},
            "index.json": {"build": "from_index"},
        }});
        assert_eq!(conda_record(&item)["build"], "from_repodata");
    }

    #[test]
    fn conda_record_falls_back_to_index_json() {
        let missing = json!({"metadata": {"index.json": {"build": "from_index"}}});
        assert_eq!(conda_record(&missing)["build"], "from_index");
        // An empty or non-object repodata record does not count.
        for repodata in [json!({}), json!(null), json!("x"), json!([1])] {
            let item = json!({"metadata": {
                "repodata_record.json": repodata,
                "index.json": {"build": "from_index"},
            }});
            assert_eq!(conda_record(&item)["build"], "from_index", "{item}");
        }
    }

    #[test]
    fn conda_record_is_null_when_absent() {
        for item in [
            json!({}),
            json!({"metadata": null}),
            json!({"metadata": {}}),
            json!({"metadata": {"repodata_record.json": {}, "index.json": {}}}),
            json!({"metadata": {"cves": []}}),
            json!("not an object"),
            json!(null),
        ] {
            assert!(conda_record(&item).is_null(), "{item}");
        }
    }

    // depends: raw dependency strings of an item.

    #[test]
    fn depends_reads_record_strings() {
        let item = item("linux-64", "b", &["python >=3.12", "numpy >=1.22"]);
        assert_eq!(
            depends(&item).collect::<Vec<_>>(),
            ["python >=3.12", "numpy >=1.22"]
        );
        // From index.json when there is no repodata record.
        let item = json!({"metadata": {"index.json": {"depends": ["libgcc >=14"]}}});
        assert_eq!(depends(&item).collect::<Vec<_>>(), ["libgcc >=14"]);
    }

    #[test]
    fn depends_skips_non_strings_and_bad_shapes() {
        let mixed = json!({"metadata": {"repodata_record.json": {
            "depends": ["ok >=1", 42, null, {"x": 1}, ["nested"], "also-ok"],
        }}});
        assert_eq!(depends(&mixed).collect::<Vec<_>>(), ["ok >=1", "also-ok"]);
        for item in [
            json!({"metadata": {"repodata_record.json": {"build": "b"}}}),
            json!({"metadata": {"repodata_record.json": {"depends": "python"}}}),
            json!({"metadata": {"repodata_record.json": {"depends": null}}}),
            json!({"metadata": {"repodata_record.json": {"depends": []}}}),
            json!({}),
        ] {
            assert_eq!(depends(&item).count(), 0, "{item}");
        }
    }

    #[test]
    fn depends_keeps_raw_spacing() {
        // Trimming is left to the consumers (Python's BLAS regex is applied
        // to the untrimmed string).
        let item = item("linux-64", "b", &["  blas * mkl  "]);
        assert_eq!(depends(&item).collect::<Vec<_>>(), ["  blas * mkl  "]);
    }

    // Dependencies parsed with rattler's MatchSpec.

    #[test]
    fn parse_dependency_extracts_exact_names() {
        for (raw, name) in [
            ("python >=3.12,<3.13.0a0", "python"),
            ("  numpy >=1.22  ", "numpy"),
            ("python_abi 3.12.* *_cp312", "python_abi"),
            ("__glibc >=2.17", "__glibc"),
            ("libgcc", "libgcc"),
        ] {
            let spec = parse_dependency(raw).unwrap_or_else(|| panic!("{raw} should parse"));
            assert_eq!(package_name(&spec), Some(name), "{raw}");
        }
        let bare = parse_dependency("libgcc").unwrap();
        assert!(bare.version.is_none());
    }

    #[test]
    fn parse_dependency_rejects_unusable_specs() {
        for raw in ["", "   ", "numpy >=>=1", "nump* >=1"] {
            assert!(parse_dependency(raw).is_none(), "{raw:?}");
        }
    }

    // raw_constraint: the verbatim text after the package name.

    #[test]
    fn raw_constraint_returns_text_after_name() {
        for (raw, expected) in [
            ("python >=3.10", Some(">=3.10")),
            ("python   >=3.10  ", Some(">=3.10")),
            ("  python >=3.10", Some(">=3.10")),
            ("python\t>=3.10", Some(">=3.10")),
            ("python >=3.10 *_cpython", Some(">=3.10 *_cpython")),
            ("python 3.12.*", Some("3.12.*")),
            ("python >=3.12,<3.13.0a0", Some(">=3.12,<3.13.0a0")),
            ("python", None),
            ("python   ", None),
            ("", None),
        ] {
            assert_eq!(raw_constraint(raw), expected, "{raw:?}");
        }
    }

    #[test]
    fn version_spec_helpers() {
        let spec = |raw: &str| parse_dependency(raw).unwrap().version.unwrap();
        assert_eq!(
            python_major_minor(&spec("python >=3.12,<3.13.0a0")).as_deref(),
            Some("3.12")
        );
        assert_eq!(
            python_major_minor(&spec("python 3.11.*")).as_deref(),
            Some("3.11")
        );
        assert_eq!(python_major_minor(&spec("python <3.13")), None);
        assert!(has_upper_bound(&spec("python >=3.12,<3.13.0a0")));
        assert!(has_upper_bound(&spec("python >=3.12,<=3.13")));
        assert!(!has_upper_bound(&spec("python >=3.12")));
        assert_eq!(
            lower_bound(&spec("numpy >=1.22.4,<2")).as_deref(),
            Some("1.22.4")
        );
        assert_eq!(lower_bound(&spec("numpy <2")), None);
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
    fn python_versions_from_other_spec_forms() {
        // Exact, starts-with, and `==` pins are treated like `>=` minimums,
        // as Python's `(?:>=|==|=)?` regex does; `>` and `<` alone are not.
        for (dep, expected) in [
            ("python 3.12.*", Some(vec!["3.12.*"])),
            ("python ==3.11.4", Some(vec!["==3.11.4"])),
            ("  python >=3.9,<3.10.0a0  ", Some(vec!["3.9"])),
            ("python >3.9", None),
            ("python <3.13", None),
            ("python", None),
        ] {
            let items = [item("linux-64", "b", &[dep])];
            let expected = expected.map(|v| v.into_iter().map(String::from).collect::<Vec<_>>());
            assert_eq!(extract_python_versions(&items), expected, "{dep}");
        }
    }

    #[test]
    fn python_versions_sort_numerically() {
        let items = [
            item("linux-64", "a", &["python >=3.10,<3.11.0a0"]),
            item("linux-64", "b", &["python >=3.9,<3.10.0a0"]),
        ];
        assert_eq!(extract_python_versions(&items).unwrap(), ["3.9", "3.10"]);
    }

    #[test]
    fn notable_constraint_ranges_use_version_order() {
        let items = [
            item("linux-64", "a", &["magma >=2.10.0"]),
            item("linux-64", "b", &["magma >=2.9.0"]),
        ];
        assert_eq!(
            extract_notable_constraints(&items).unwrap(),
            ["magma >=2.9.0 to >=2.10.0 (varies by python version)"]
        );
    }

    #[test]
    fn dependency_parsing_skips_unparseable_specs() {
        let items = [item(
            "linux-64",
            "a",
            &["", "numpy >=1.22", "not a valid spec >=>= ???"],
        )];
        assert_eq!(
            extract_notable_constraints(&items).unwrap(),
            ["numpy >=1.22"]
        );
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
        for (build, label) in [
            ("cpu_mkl_py312h0452e45_104", "cpu_mkl"),
            ("py312h8456e4e_0", ""),
            ("cuda118_py310_h0123456_1", "cuda118"),
            ("cuda126_py312h0452e45_104", "cuda126"),
            // Hash shorter than six hex digits: not a hash suffix.
            ("h1234_0", "h1234_0"),
            // Same results as Python's regexes, quirks included: noarch
            // builds keep `py`, and `\d{2,3}` is greedy.
            ("pyhd8ed1ab_0", "py"),
            ("py2713_0", "3_0"),
        ] {
            assert_eq!(build_label(build), label, "{build}");
        }
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
        )
        .unwrap_err();
        assert_eq!(err.error, "package_not_found");
        assert!(err.message.contains("Available in: main-x."));
        assert_eq!(err.name.as_deref(), Some("numpy"));
    }

    #[test]
    fn resolve_reports_unreachable_vs_missing() {
        let err = resolve_channel(None, vec![], false, "numpy").unwrap_err();
        assert_eq!(err.error, "search_unavailable");
        let err = resolve_channel(None, vec![], true, "numpy").unwrap_err();
        assert_eq!(err.error, "package_not_found");
    }

    #[test]
    fn resolve_explicit_channel_outage_is_search_unavailable() {
        let err = resolve_channel(Some("main-x"), vec![], false, "numpy").unwrap_err();
        assert_eq!(err.error, "search_unavailable");
        assert!(err.message.contains("(main-x)"));
        let err = resolve_channel(Some("main-x"), vec![], true, "numpy").unwrap_err();
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
            &resolution(numpy_meta("2.4.1")),
            Some(&files),
            false,
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
        let a = query("numpy", Some("2.3.0"), Some("win-arm64"), Some("3.9"));
        let files = json!({"total_count": 1, "items": [
            item("linux-64", "py312h8456e4e_0", &["python >=3.12,<3.13.0a0"]),
        ]});
        let response = assemble(&a, &resolution(numpy_meta("2.4.1")), Some(&files), false).unwrap();
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
        let a = query("numpy", Some("9.9"), None, None);
        let files = json!({"total_count": 0, "items": []});
        let err = assemble(&a, &resolution(numpy_meta("2.4.1")), Some(&files), false).unwrap_err();
        assert_eq!(err.error, "version_not_found");
        assert!(err.message.contains("Available versions: 2.4.1, 2.3.0"));
    }

    #[test]
    fn assemble_files_unavailable_note() {
        let a = query("numpy", Some("2.3.0"), None, None);
        let response = assemble(&a, &resolution(numpy_meta("2.4.1")), None, false).unwrap();
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
            &resolution(numpy_meta("2.4.1")),
            Some(&files),
            false,
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
        let response = assemble(&args("numpy"), &res, None, false).unwrap();
        assert!(response.notes[0].contains("Channel 'r' is deprecated"));
    }
}
