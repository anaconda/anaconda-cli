//! Wire types returned by the MCP tools.
//!
//! Field names are part of the contract the Anaconda Package Intelligence
//! skill (`src/mcp/skill.rs`) relies on. Optional fields serialize as
//! explicit `null` rather than being omitted, so an agent can tell
//! "unavailable" apart from "not part of the response".

use serde::Serialize;

/// A tool-level error, returned with `isError: true`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolError {
    pub error: &'static str,
    pub message: String,
    /// Echo of the package name the caller supplied, when relevant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ToolError {
    pub fn new(error: &'static str, message: impl Into<String>) -> Self {
        Self {
            error,
            message: message.into(),
            name: None,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new("invalid_request", message)
    }

    pub fn unauthenticated() -> Self {
        Self::new(
            "unauthenticated",
            "Not logged in to Anaconda, or the stored credentials were rejected. \
             Run `ana login` in a terminal, then retry.",
        )
    }
}

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

    #[test]
    fn tool_error_omits_name_when_absent() {
        let value = serde_json::to_value(ToolError::invalid_request("bad")).unwrap();
        assert_eq!(value, json!({"error": "invalid_request", "message": "bad"}));
    }

    #[test]
    fn tool_error_includes_name_when_present() {
        let value =
            serde_json::to_value(ToolError::new("package_not_found", "missing").with_name("numpy"))
                .unwrap();
        assert_eq!(value["name"], "numpy");
    }

    #[test]
    fn package_info_serializes_nulls_explicitly() {
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
        let value = serde_json::to_value(&response).unwrap();
        let object = value.as_object().unwrap();
        for key in [
            "version",
            "python_versions",
            "license",
            "notable_constraints",
            "build_variants",
            "summary",
            "doc_url",
            "policy_status",
            "policy_detail",
        ] {
            assert!(object.contains_key(key), "missing key {key}");
            assert!(object[key].is_null(), "{key} should be null");
        }
        assert!(value["cve_summary"]["count"].is_null());
    }
}
