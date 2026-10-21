//! JSON output of `ana package info` and `ana forum search`.
//!
//! Mirrors bigbend `apps/remote-mcp/remote_mcp/mcp/schemas.py`: same field
//! names, order, and nesting. Fields Python marks `exclude=True` (deferred
//! data such as `github_stars`) are not declared here. The contract tests at
//! the bottom of this file pin the exact key sets.
//!
//! Optional fields serialize as explicit `null` rather than being omitted
//! (never use `skip_serializing_if`), so an agent can tell "unavailable"
//! apart from "not part of the response". The error shape lives in
//! `errors.rs`.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DownloadCount {
    pub total: Option<i64>,
    pub channel: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CveSummary {
    pub count: Option<u64>,
    pub critical: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildVariant {
    pub label: String,
    pub selector: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PackageInfoResponse {
    pub name: String,
    pub version: Option<String>,
    pub channel: String,
    pub platforms: Vec<String>,
    pub python_versions: Option<Vec<String>>,
    pub license: Option<String>,
    pub notable_constraints: Option<Vec<String>>,
    pub build_variants: Option<Vec<BuildVariant>>,
    pub summary: Option<String>,
    pub doc_url: Option<String>,
    pub download_count: DownloadCount,
    pub cve_summary: CveSummary,
    pub policy_status: Option<String>,
    pub policy_detail: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ForumBlurb {
    pub username: String,
    pub blurb: String,
    pub post_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ForumTopic {
    pub id: i64,
    pub title: String,
    /// Absolute, same-origin with the configured forum base URL, or null.
    pub url: Option<String>,
    pub category: String,
    pub posts_count: i64,
    pub reply_count: i64,
    pub views: i64,
    pub created_at: Option<String>,
    pub last_posted_at: Option<String>,
    pub blurbs: Vec<ForumBlurb>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ForumPagination {
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ForumSearchResponse {
    pub topics: Vec<ForumTopic>,
    pub total: i64,
    pub pagination: ForumPagination,
    pub notes: Vec<String>,
}

/// Flattened policy assigned to a channel (`ana package policy`). Mirrors the
/// dict `org_config` builds: `name` first, then `blocks`, then `overrides`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AssignedPolicy {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocks: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overrides: Option<Vec<serde_json::Value>>,
}

/// One channel in an org's configuration (`ana package policy`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChannelConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub channel_type: String,
    pub privacy: String,
    pub url: String,
    /// Flattened assigned policy, or null when the channel has none.
    pub assigned_policy: Option<AssignedPolicy>,
}

/// `ana package policy` response: org channels and their applied rules.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrgConfigResponse {
    pub org_name: String,
    pub channels: Vec<ChannelConfig>,
    pub notes: Vec<String>,
}

/// Ambiguous org: the caller must pick one. Serialized as a structured error.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrgChoiceRequired {
    pub error: &'static str,
    pub message: String,
    pub available_orgs: Vec<String>,
}

/// A single CVE record in the `ana package security` response.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CveDetail {
    pub id: String,
    pub score: Option<f64>,
    pub severity: String,
    pub status: String,
    pub status_on_platform: Option<String>,
    pub description: Option<String>,
    pub fix_version: Option<String>,
    pub fix_spec: Option<String>,
    pub platforms_affected: Option<Vec<String>>,
    pub platforms_cleared: Option<Vec<String>>,
    pub cvss_vector: Option<String>,
    pub cwe: Option<Vec<String>>,
    pub analyst_comment: Option<String>,
    pub curation_date: Option<String>,
    pub references: Option<std::collections::BTreeMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PackageSecurityPagination {
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
}

/// `ana package security` response.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PackageSecurityResponse {
    pub name: String,
    pub version: String,
    pub channel: String,
    pub total_cves: i64,
    pub pagination: PackageSecurityPagination,
    pub cves: Vec<CveDetail>,
    pub cleared_cve_count: i64,
    pub notes: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Contract tests: each expected string is what Python's
    // `model_dump_json()` emits for the same model in
    // `remote_mcp/mcp/schemas.py`, so key names, key order, nesting, and
    // explicit nulls are all pinned.

    #[test]
    fn package_info_contract() {
        let response = PackageInfoResponse {
            name: "numpy".into(),
            version: None,
            channel: "main".into(),
            platforms: vec![],
            python_versions: None,
            license: None,
            notable_constraints: None,
            build_variants: None,
            summary: None,
            doc_url: None,
            download_count: DownloadCount {
                total: None,
                channel: None,
            },
            cve_summary: CveSummary {
                count: None,
                critical: None,
            },
            policy_status: None,
            policy_detail: None,
            notes: vec![],
        };
        assert_eq!(
            serde_json::to_string(&response).unwrap(),
            concat!(
                r#"{"name":"numpy","version":null,"channel":"main","platforms":[],"#,
                r#""python_versions":null,"license":null,"notable_constraints":null,"#,
                r#""build_variants":null,"summary":null,"doc_url":null,"#,
                r#""download_count":{"total":null,"channel":null},"#,
                r#""cve_summary":{"count":null,"critical":null},"#,
                r#""policy_status":null,"policy_detail":null,"notes":[]}"#,
            )
        );
        let variant = BuildVariant {
            label: "cpu".into(),
            selector: "pytorch=*=cpu*".into(),
        };
        assert_eq!(
            serde_json::to_value(&variant).unwrap(),
            json!({"label": "cpu", "selector": "pytorch=*=cpu*"})
        );
    }

    #[test]
    fn forum_search_contract() {
        let response = ForumSearchResponse {
            topics: vec![ForumTopic {
                id: 1,
                title: "t".into(),
                url: None,
                category: "c".into(),
                posts_count: 1,
                reply_count: 0,
                views: 2,
                created_at: None,
                last_posted_at: None,
                blurbs: vec![ForumBlurb {
                    username: "u".into(),
                    blurb: "b".into(),
                    post_number: 1,
                }],
            }],
            total: 1,
            pagination: ForumPagination {
                page: 1,
                page_size: 10,
                total_pages: 1,
            },
            notes: vec![],
        };
        assert_eq!(
            serde_json::to_string(&response).unwrap(),
            concat!(
                r#"{"topics":[{"id":1,"title":"t","url":null,"category":"c","#,
                r#""posts_count":1,"reply_count":0,"views":2,"created_at":null,"#,
                r#""last_posted_at":null,"blurbs":[{"username":"u","blurb":"b","#,
                r#""post_number":1}]}],"total":1,"#,
                r#""pagination":{"page":1,"page_size":10,"total_pages":1},"notes":[]}"#,
            )
        );
    }

    #[test]
    fn org_config_contract() {
        let response = OrgConfigResponse {
            org_name: "acme".into(),
            channels: vec![
                ChannelConfig {
                    name: "main".into(),
                    channel_type: "anaconda".into(),
                    privacy: "public".into(),
                    url: "https://repo.anaconda.cloud/repo/acme/main".into(),
                    assigned_policy: None,
                },
                ChannelConfig {
                    name: "finance".into(),
                    channel_type: "hosted".into(),
                    privacy: "private".into(),
                    url: "https://repo.anaconda.cloud/repo/acme/finance".into(),
                    assigned_policy: Some(AssignedPolicy {
                        name: "finance_packages".into(),
                        blocks: Some(vec![json!({"facet": "cve_score", "match": "> 8"})]),
                        overrides: None,
                    }),
                },
            ],
            notes: vec![],
        };
        assert_eq!(
            serde_json::to_string(&response).unwrap(),
            concat!(
                r#"{"org_name":"acme","channels":["#,
                r#"{"name":"main","type":"anaconda","privacy":"public","#,
                r#""url":"https://repo.anaconda.cloud/repo/acme/main","assigned_policy":null},"#,
                r#"{"name":"finance","type":"hosted","privacy":"private","#,
                r#""url":"https://repo.anaconda.cloud/repo/acme/finance","#,
                r#""assigned_policy":{"name":"finance_packages","#,
                r#""blocks":[{"facet":"cve_score","match":"> 8"}]}}],"#,
                r#""notes":[]}"#,
            )
        );
    }

    #[test]
    fn org_choice_required_contract() {
        let choice = OrgChoiceRequired {
            error: "org_choice_required",
            message: "Pick one.".into(),
            available_orgs: vec!["org-a".into(), "org-b".into()],
        };
        assert_eq!(
            serde_json::to_string(&choice).unwrap(),
            r#"{"error":"org_choice_required","message":"Pick one.","available_orgs":["org-a","org-b"]}"#
        );
    }

    #[test]
    fn package_security_contract() {
        let response = PackageSecurityResponse {
            name: "requests".into(),
            version: "2.32.3".into(),
            channel: "main".into(),
            total_cves: 1,
            pagination: PackageSecurityPagination {
                page: 1,
                page_size: 20,
                total_pages: 1,
            },
            cves: vec![CveDetail {
                id: "CVE-2026-25645".into(),
                score: Some(4.4),
                severity: "medium".into(),
                status: "active".into(),
                status_on_platform: Some("linux-64".into()),
                description: Some("desc".into()),
                fix_version: Some("2.33.1".into()),
                fix_spec: Some("requests>=2.33.0".into()),
                platforms_affected: Some(vec!["linux-64".into()]),
                platforms_cleared: Some(vec![]),
                cvss_vector: Some("CVSS:3.1/AV:L/AC:H/PR:L/UI:R/S:U/C:N/I:H/A:N".into()),
                cwe: Some(vec!["CWE-345".into()]),
                analyst_comment: Some("fixed in 2.33.0".into()),
                curation_date: Some("2026-06-19T00:00:00".into()),
                references: Some([("R1".into(), "https://nvd.nist.gov".into())].into()),
            }],
            cleared_cve_count: 0,
            notes: vec![],
        };
        assert_eq!(
            serde_json::to_string(&response).unwrap(),
            concat!(
                r#"{"name":"requests","version":"2.32.3","channel":"main","total_cves":1,"#,
                r#""pagination":{"page":1,"page_size":20,"total_pages":1},"cves":[{"id":"CVE-2026-25645","#,
                r#""score":4.4,"severity":"medium","status":"active","status_on_platform":"linux-64","#,
                r#""description":"desc","fix_version":"2.33.1","fix_spec":"requests>=2.33.0","#,
                r#""platforms_affected":["linux-64"],"platforms_cleared":[],"#,
                r#""cvss_vector":"CVSS:3.1/AV:L/AC:H/PR:L/UI:R/S:U/C:N/I:H/A:N","#,
                r#""cwe":["CWE-345"],"analyst_comment":"fixed in 2.33.0","#,
                r#""curation_date":"2026-06-19T00:00:00","references":{"R1":"https://nvd.nist.gov"}}],"#,
                r#""cleared_cve_count":0,"notes":[]}"#,
            )
        );
    }
}
