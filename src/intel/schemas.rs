//! JSON output of the `ana intel` commands.
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
}
