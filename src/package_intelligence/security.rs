//! `ana package security`: detailed CVE information for a package.
//!
//! Ported from bigbend `apps/remote-mcp/remote_mcp/services/package_security.py`.
//! Two subscriber-path calls — `/cves` for full detail (reviews) and
//! `/files?version=` for per-platform status — merged into one CVE list,
//! sorted by severity and paginated 20 to a page.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::str::FromStr;

use rattler_conda_types::{MatchSpec, PackageNameMatcher, ParseStrictness, Version, VersionSpec};
use serde_json::Value;

use super::errors::ToolError;
use super::org::{fetch_artifact_files_for_version, fetch_artifacts_listing, fetch_cve_detail};
use super::schemas::{CveDetail, PackageSecurityPagination, PackageSecurityResponse};
use crate::context::CommandContext;

pub const PAGE_SIZE: i64 = 20;
const MAX_DESCRIPTION_CHARS: usize = 200;
const MAX_COMMENT_CHARS: usize = 300;
const TRUNCATION_MARKER: &str = " [truncated]";
const NVD_CWE_PREFIX: &str = "NVD-CWE-";

fn status_rank(status: &str) -> u8 {
    match status {
        "active" => 3,
        "reported" => 2,
        "cleared" => 1,
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// Port of `_truncate`.
fn truncate(text: Option<&str>, max_chars: usize) -> Option<String> {
    let text = text?;
    if text.len() <= max_chars {
        return Some(text.to_string());
    }
    let mut cut: String = text.chars().take(max_chars).collect();
    cut.push_str(TRUNCATION_MARKER);
    Some(cut)
}

/// Port of `_get_latest_anaconda_review`: pick the latest `source=anaconda`
/// review, falling back to the latest of any source.
fn get_latest_anaconda_review<'a>(reviews: &'a [Value]) -> Option<&'a Value> {
    let date_of = |r: &'a Value| r.get("date").and_then(Value::as_str).unwrap_or("");
    let is_anaconda = |r: &Value| r.get("source").and_then(Value::as_str) == Some("anaconda");
    reviews
        .iter()
        .filter(|r| is_anaconda(r))
        .max_by_key(|r| date_of(r))
        .or_else(|| reviews.iter().max_by_key(|r| date_of(r)))
}

/// Port of `_build_file_cve_status`: `{cve_id: {subdir: status}}`, worst
/// status winning when several builds share a subdir.
fn build_file_cve_status(files: Option<&Value>) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut result: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let Some(files) = files else {
        return result;
    };
    for item in files.get("items").and_then(Value::as_array).into_iter().flatten() {
        let Some(subdir) = item
            .get("ckey")
            .and_then(Value::as_str)
            .and_then(|ckey| ckey.split('/').next())
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let metadata = item.get("metadata").unwrap_or(&Value::Null);
        for cve in metadata.get("cves").and_then(Value::as_array).into_iter().flatten() {
            let Some(cve_id) = cve.get("id").and_then(Value::as_str) else {
                continue;
            };
            let status = cve.get("status").and_then(Value::as_str).unwrap_or("");
            let per_subdir = result.entry(cve_id.to_string()).or_default();
            let entry = per_subdir
                .entry(subdir.to_string())
                .or_insert_with(|| "cleared".to_string());
            if status_rank(status) > status_rank(entry.as_str()) {
                *entry = status.to_string();
            }
        }
    }
    result
}

/// Port of `_resolve_fix`: find the minimum available version `>= user_version`
/// that satisfies one of the cleared matchspecs.
fn resolve_fix(
    cleared_specs: &[&str],
    pkg_name: &str,
    available_versions: &[String],
    user_version: &str,
) -> (Option<String>, Option<String>) {
    let Ok(user_vo) = Version::from_str(user_version) else {
        return (None, None);
    };
    let mut sorted: Vec<(String, Version)> = Vec::with_capacity(available_versions.len());
    for v in available_versions {
        match Version::from_str(v) {
            Ok(vo) => sorted.push((v.clone(), vo)),
            Err(_) => return (None, None),
        }
    }
    sorted.sort_by(|a, b| a.1.cmp(&b.1));

    let mut best: Option<(Version, String, String)> = None;
    for spec_str in cleared_specs {
        let Ok(ms) = MatchSpec::from_str(spec_str, ParseStrictness::Lenient) else {
            continue;
        };
        let name = match &ms.name {
            PackageNameMatcher::Exact(name) => name.as_source(),
            _ => continue,
        };
        if name != pkg_name {
            continue;
        }
        let Some(ver_spec) = ms.version.as_ref() else {
            return (Some(user_version.to_string()), Some(spec_str.to_string()));
        };
        if matches!(ver_spec, VersionSpec::Any) {
            return (Some(user_version.to_string()), Some(spec_str.to_string()));
        }
        for (v, vo) in &sorted {
            if !ver_spec.matches(vo) {
                continue;
            }
            if vo < &user_vo {
                continue;
            }
            if best.as_ref().is_none_or(|(best_vo, _, _)| vo < best_vo) {
                best = Some((vo.clone(), v.clone(), spec_str.to_string()));
            }
            break;
        }
    }

    best.map(|(_, v, spec)| (Some(v), Some(spec)))
        .unwrap_or((None, None))
}

/// Port of `_extract_cvss_vector`: v4 > v3 > v2.
fn extract_cvss_vector(cve_item: &Value) -> Option<String> {
    for (version_key, inner_key) in [("cvssv4", "cvssV4"), ("cvssv3", "cvssV3"), ("cvssv2", "cvssV2")] {
        if let Some(vector) = cve_item
            .get(version_key)
            .and_then(|blob| blob.get(inner_key))
            .and_then(|inner| inner.get("vectorString"))
            .and_then(Value::as_str)
        {
            return Some(vector.to_string());
        }
    }
    None
}

/// Port of `_extract_severity`.
fn extract_severity(cve_item: &Value) -> String {
    for (version_key, inner_key) in [("cvssv4", "cvssV4"), ("cvssv3", "cvssV3")] {
        if let Some(severity) = cve_item
            .get(version_key)
            .and_then(|blob| blob.get(inner_key))
            .and_then(|inner| inner.get("baseSeverity"))
            .and_then(Value::as_str)
        {
            return severity.to_ascii_lowercase();
        }
    }
    "unknown".to_string()
}

/// Port of `_extract_cwes`.
fn extract_cwes(cve_item: &Value) -> Option<Vec<String>> {
    let mut cwes: Vec<String> = cve_item
        .get("cwes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    if cwes.is_empty()
        && let Some(nvdv2) = cve_item.get("nvdv2").and_then(Value::as_object)
    {
        for weakness in nvdv2
            .get("weaknesses")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            for desc in weakness
                .get("description")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(value) = desc.get("value").and_then(Value::as_str)
                    && !value.is_empty()
                {
                    cwes.push(value.to_string());
                }
            }
        }
    }
    let filtered: Vec<String> = cwes
        .into_iter()
        .filter(|c| !c.starts_with(NVD_CWE_PREFIX))
        .collect();
    (!filtered.is_empty()).then_some(filtered)
}

/// Port of `_build_cve_detail`. Returns `None` for curated CVEs that are fully
/// cleared on every platform.
fn build_cve_detail(
    cve_item: &Value,
    file_cve_status: &BTreeMap<String, BTreeMap<String, String>>,
    platform: Option<&str>,
    pkg_name: &str,
    available_versions: &[String],
    user_version: &str,
) -> Option<CveDetail> {
    let cve_id = cve_item.get("id").and_then(Value::as_str).unwrap_or("");
    let reviews = cve_item.get("reviews").and_then(Value::as_array);
    let has_review = reviews.is_some_and(|r| !r.is_empty());
    let review = reviews
        .filter(|r| !r.is_empty())
        .and_then(|r| get_latest_anaconda_review(r));

    let subdir_statuses = file_cve_status.get(cve_id);

    let (platforms_affected, platforms_cleared, overall_status, status_on_platform) =
        if let (true, Some(subdir_statuses)) = (has_review, subdir_statuses) {
            let affected: Vec<String> = subdir_statuses
                .iter()
                .filter(|(_, st)| st.as_str() == "active")
                .map(|(s, _)| s.clone())
                .collect();
            let cleared: Vec<String> = subdir_statuses
                .iter()
                .filter(|(_, st)| st.as_str() == "cleared")
                .map(|(s, _)| s.clone())
                .collect();
            if affected.is_empty() {
                return None;
            }
            let status_on_platform = platform.and_then(|p| subdir_statuses.get(p).cloned());
            (
                Some(affected),
                Some(cleared),
                "active",
                status_on_platform,
            )
        } else if has_review {
            (None, None, "active", None)
        } else {
            (None, None, "reported", None)
        };

    let (fix_version, fix_spec) = match review {
        Some(review) if review.get("cleared").and_then(Value::as_array).is_some_and(|c| !c.is_empty()) && !available_versions.is_empty() => {
            let cleared: Vec<&str> = review
                .get("cleared")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            resolve_fix(&cleared, pkg_name, available_versions, user_version)
        }
        _ => (None, None),
    };

    Some(CveDetail {
        id: cve_id.to_string(),
        score: cve_item.get("score").and_then(Value::as_f64),
        severity: extract_severity(cve_item),
        status: overall_status.to_string(),
        status_on_platform,
        description: truncate(cve_item.get("description").and_then(Value::as_str), MAX_DESCRIPTION_CHARS),
        fix_version,
        fix_spec,
        platforms_affected,
        platforms_cleared,
        cvss_vector: extract_cvss_vector(cve_item),
        cwe: extract_cwes(cve_item),
        analyst_comment: review
            .and_then(|r| r.get("comment"))
            .and_then(Value::as_str)
            .and_then(|c| truncate(Some(c), MAX_COMMENT_CHARS)),
        curation_date: review.and_then(|r| r.get("date")).and_then(Value::as_str).map(str::to_string),
        references: review
            .and_then(|r| r.get("references"))
            .and_then(Value::as_object)
            .map(|refs| {
                refs.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            }),
    })
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Command entry: check credentials, resolve the org, then fetch security.
/// Port of the `package_security` tool's org-resolution wrapper.
pub async fn run_security(
    ctx: &CommandContext,
    name: Option<String>,
    version: Option<String>,
    channel: Option<String>,
    platform: Option<String>,
    page: i64,
    org: Option<String>,
) -> Result<PackageSecurityResponse, ToolError> {
    if !super::has_credentials(&ctx.config) {
        return Err(ToolError::unauthenticated());
    }

    let name = name.unwrap_or_default();
    let version = version.unwrap_or_default();
    let channel = channel.unwrap_or_default();
    for (value, field) in [(&name, "package name"), (&channel, "channel")] {
        if !super::package_info::is_safe_segment(value) {
            return Err(ToolError::invalid_request(format!(
                "{field} {}",
                super::package_info::SEGMENT_RULE
            )));
        }
    }
    if version.trim().is_empty() {
        return Err(ToolError::invalid_request("version is required"));
    }

    let passport = super::org::fetch_whoami(ctx).await.ok_or_else(|| {
        ToolError::new(
            "upstream_error",
            "Could not retrieve your organization memberships. Try again later.",
        )
    })?;
    let org_name = match super::org::resolve_org(&passport, org.as_deref()) {
        super::org::OrgResolution::Resolved(org) => org.name,
        super::org::OrgResolution::None => {
            return Err(ToolError::new(
                "subscription_required",
                "You need a business subscription to view package security. \
                 Available to subscribers only.",
            ));
        }
        super::org::OrgResolution::Choice(choice) => {
            return Err(ToolError::org_choice(choice.message, choice.available_orgs));
        }
        super::org::OrgResolution::Error(error) => return Err(error),
    };

    package_security(
        ctx,
        &org_name,
        &channel,
        &name,
        &version,
        platform.as_deref(),
        page,
    )
    .await
}

/// Port of `get_package_security`. `org_name` and `channel` are already
/// validated (non-blank, safe segments) by the command layer.
pub async fn package_security(
    ctx: &CommandContext,
    org_name: &str,
    channel: &str,
    name: &str,
    version: &str,
    platform: Option<&str>,
    page: i64,
) -> Result<PackageSecurityResponse, ToolError> {
    // Call 1: full CVE detail.
    let cves_data = fetch_cve_detail(ctx, org_name, channel, name).await.ok_or_else(|| {
        ToolError::new(
            "search_unavailable",
            "Could not retrieve CVE data. Try again later.",
        )
    })?;

    // Available versions, for fix resolution and version validation.
    let mut available_versions: Vec<String> = Vec::new();
    if let Some(listing) = fetch_artifacts_listing(ctx, org_name, channel, name).await
        && let Some(first) = listing.get("items").and_then(Value::as_array).and_then(|i| i.first())
    {
        available_versions = first
            .get("available_versions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
    }

    if !available_versions.is_empty() && !available_versions.iter().any(|v| v == version) {
        let shown: Vec<&str> = available_versions.iter().take(10).map(String::as_str).collect();
        let more = available_versions.len().saturating_sub(shown.len());
        let mut version_list = shown.join(", ");
        if more > 0 {
            version_list.push_str(&format!(" (and {more} more)"));
        }
        return Err(ToolError::new(
            "version_not_found",
            format!(
                "Version {version:?} does not exist for package {name:?} in channel \
                 {channel:?}. Available versions: {version_list}"
            ),
        )
        .with_name(name));
    }

    // Call 2: per-platform status. Graceful: no files means no platform status.
    let files_data = fetch_artifact_files_for_version(ctx, org_name, channel, name, version).await;
    let file_cve_status = build_file_cve_status(files_data.as_ref());

    // Merge.
    let mut all_cves: Vec<CveDetail> = Vec::new();
    let mut cleared_count: i64 = 0;
    for cve_item in cves_data.get("items").and_then(Value::as_array).into_iter().flatten() {
        let is_reported = !cve_item
            .get("reviews")
            .and_then(Value::as_array)
            .is_some_and(|r| !r.is_empty());
        let cve_id = cve_item.get("id").and_then(Value::as_str).unwrap_or("");
        if is_reported && !file_cve_status.is_empty() && !file_cve_status.contains_key(cve_id) {
            continue;
        }
        match build_cve_detail(
            cve_item,
            &file_cve_status,
            platform,
            name,
            &available_versions,
            version,
        ) {
            Some(detail) => all_cves.push(detail),
            None => cleared_count += 1,
        }
    }

    // Sort by score descending, nulls last.
    all_cves.sort_by(|a, b| match (a.score, b.score) {
        (Some(x), Some(y)) => y.partial_cmp(&x).unwrap_or(Ordering::Equal),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    });

    // Paginate.
    let total_cves = all_cves.len() as i64;
    let total_pages = ((total_cves + PAGE_SIZE - 1) / PAGE_SIZE).max(1);
    let page = page.clamp(1, total_pages);
    let start = ((page - 1) * PAGE_SIZE) as usize;
    let page_cves = all_cves
        .into_iter()
        .skip(start)
        .take(PAGE_SIZE as usize)
        .collect::<Vec<_>>();

    let mut notes = Vec::new();
    if total_pages > 1 && page < total_pages {
        notes.push(format!(
            "Showing page {page} of {total_pages} ({} of {total_cves} CVEs). Results sorted by \
             severity. Call with page={} for more.",
            page_cves.len(),
            page + 1
        ));
    }

    Ok(PackageSecurityResponse {
        name: name.to_string(),
        version: version.to_string(),
        channel: channel.to_string(),
        total_cves,
        pagination: PackageSecurityPagination {
            page,
            page_size: PAGE_SIZE,
            total_pages,
        },
        cves: page_cves,
        cleared_cve_count: cleared_count,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn file_item(subdir: &str, build: &str, cves: Value) -> Value {
        json!({
            "name": "requests",
            "ckey": format!("{subdir}/requests-2.32.3-{build}.tar.bz2"),
            "metadata": {"repodata_record.json": {"subdir": subdir, "build": build}, "cves": cves},
        })
    }

    #[test]
    fn truncates_with_marker() {
        assert_eq!(truncate(None, 200), None);
        assert_eq!(truncate(Some("hello"), 200).as_deref(), Some("hello"));
        let long = "a".repeat(250);
        let result = truncate(Some(&long), 200).unwrap();
        assert_eq!(result.len(), 200 + TRUNCATION_MARKER.len());
        assert!(result.ends_with(TRUNCATION_MARKER));
    }

    #[test]
    fn picks_latest_anaconda_review() {
        let reviews = json!([
            {"source":"anaconda","date":"2020-04-20T00:00:00","cleared":[]},
            {"source":"anaconda","date":"2021-08-10T00:00:00","cleared":["requests>=2.20.0"]},
            {"source":"anaconda","date":"2019-01-15T00:00:00","cleared":["requests>=9.9.9"]},
        ]);
        let arr = reviews.as_array().unwrap();
        let review = get_latest_anaconda_review(arr).unwrap();
        assert_eq!(review["date"], "2021-08-10T00:00:00");
        // Non-anaconda source skipped when anaconda exists.
        let mixed = json!([
            {"source":"conda_forge","date":"2026-12-01T00:00:00"},
            {"source":"anaconda","date":"2026-06-01T00:00:00"},
        ]);
        let review = get_latest_anaconda_review(mixed.as_array().unwrap()).unwrap();
        assert_eq!(review["source"], "anaconda");
    }

    #[test]
    fn builds_file_cve_status_with_worst_wins() {
        let files = json!({"items": [
            file_item("linux-64", "b1", json!([{"id":"CVE-TEST","status":"cleared"}])),
            file_item("linux-64", "b2", json!([{"id":"CVE-TEST","status":"active"}])),
            file_item("osx-arm64", "b1", json!([{"id":"CVE-TEST","status":"cleared"}])),
        ]});
        let status = build_file_cve_status(Some(&files));
        assert_eq!(status["CVE-TEST"]["linux-64"], "active");
        assert_eq!(status["CVE-TEST"]["osx-arm64"], "cleared");
        assert_eq!(build_file_cve_status(None), BTreeMap::new());
    }

    #[test]
    fn resolves_fix() {
        let avail = ["6.5.7", "6.5.5", "6.4.2"]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            resolve_fix(&["tornado>=6.5.5"], "tornado", &avail, "6.4.2"),
            (Some("6.5.5".into()), Some("tornado>=6.5.5".into()))
        );
        // Skips versions below the user's, and wrong package names.
        let avail = ["3.0.22".to_string(), "3.0.20".to_string()];
        assert_eq!(
            resolve_fix(&["python>=3.7.2", "openssl>=3.0.20"], "openssl", &avail, "3.0.19"),
            (Some("3.0.20".into()), Some("openssl>=3.0.20".into()))
        );
        // Bare name returns the user's version.
        let avail = ["3.5.8".to_string(), "3.0.22".to_string()];
        assert_eq!(
            resolve_fix(&["openssl"], "openssl", &avail, "3.0.22"),
            (Some("3.0.22".into()), Some("openssl".into()))
        );
        // Malformed versions degrade to None.
        let avail = ["6.5.7".to_string()];
        assert_eq!(
            resolve_fix(&["tornado>=6.5.5"], "tornado", &avail, "not-a-version!!"),
            (None, None)
        );
    }

    #[test]
    fn extracts_severity_and_cwes() {
        let cve = json!({
            "cvssv3": {"cvssV3": {"baseSeverity": "MEDIUM", "vectorString": "CVSS:3.1/..."}},
            "cwes": ["CWE-345", "NVD-CWE-no"],
        });
        assert_eq!(extract_severity(&cve), "medium");
        assert_eq!(
            extract_cvss_vector(&cve).as_deref(),
            Some("CVSS:3.1/...")
        );
        assert_eq!(extract_cwes(&cve), Some(vec!["CWE-345".to_string()]));
        assert_eq!(extract_severity(&json!({})), "unknown");
        assert_eq!(extract_cwes(&json!({})), None);
    }

    #[test]
    fn build_cve_detail_reported_and_cleared() {
        let files = json!({"items": [
            file_item("linux-64", "b", json!([
                {"id":"CVE-ACTIVE","status":"active"},
                {"id":"CVE-CLEARED","status":"cleared"},
            ])),
        ]});
        let status = build_file_cve_status(Some(&files));

        let active = json!({
            "id": "CVE-ACTIVE",
            "score": 4.4,
            "reviews": [{"source":"anaconda","date":"2026-06-19T00:00:00","cleared":["requests>=2.33.0"],"comment":"c","references":{"R1":"https://x"}}],
        });
        let detail = build_cve_detail(&active, &status, Some("linux-64"), "requests", &["2.33.1".into()], "2.32.3").unwrap();
        assert_eq!(detail.status, "active");
        assert_eq!(detail.status_on_platform.as_deref(), Some("active"));
        assert_eq!(detail.fix_version.as_deref(), Some("2.33.1"));
        assert_eq!(
            detail.platforms_affected,
            Some(vec!["linux-64".to_string()])
        );

        // Fully cleared on all platforms → None.
        let cleared = json!({
            "id": "CVE-CLEARED",
            "score": 7.5,
            "reviews": [{"source":"anaconda","date":"2021-01-01T00:00:00","cleared":[]}],
        });
        assert!(build_cve_detail(&cleared, &status, None, "requests", &[], "2.32.3").is_none());

        // Reported CVE has no file status or fix.
        let reported = json!({"id":"CVE-REPORTED","score":6.1,"reviews":[]});
        let detail = build_cve_detail(&reported, &status, Some("linux-64"), "requests", &[], "2.32.3").unwrap();
        assert_eq!(detail.status, "reported");
        assert_eq!(detail.fix_spec, None);
        assert_eq!(detail.status_on_platform, None);
    }
}
