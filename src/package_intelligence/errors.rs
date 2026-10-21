//! Errors returned by the package intelligence commands.
//!
//! `ToolError` is the structured error agents parse: in JSON mode it is
//! printed to stdout as `{"error", "message", "name"}` and the command exits
//! non-zero. In a terminal it becomes a `PackageIntelligenceError::Tool`
//! diagnostic. `PackageIntelligenceError` itself lives in `src/errors.rs`
//! because `cli.rs` matches on it (CLAUDE.md: errors shared across modules go
//! there).

use serde::Serialize;

use crate::errors::PackageIntelligenceError;

/// Serialized `ToolError` used when a result cannot be encoded. Kept as a
/// literal so the fallback cannot itself fail; a test checks it matches.
pub const SERIALIZATION_ERROR_JSON: &str = r#"{"error":"internal_error","message":"The tool result could not be serialized. Try again later.","name":null}"#;

/// A structured command error. Field names and codes mirror the remote MCP
/// service's `ToolError` so the Anaconda Package Intelligence skill can
/// branch on them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolError {
    /// Machine-readable code, e.g. `package_not_found`.
    pub error: &'static str,
    pub message: String,
    /// Echo of the package name the caller supplied, when relevant; `null`
    /// otherwise.
    pub name: Option<String>,
    /// Candidate organizations for `org_choice_required`; omitted otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_orgs: Option<Vec<String>>,
}

impl ToolError {
    pub fn new(error: &'static str, message: impl Into<String>) -> Self {
        Self {
            error,
            message: message.into(),
            name: None,
            available_orgs: None,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn org_choice(message: impl Into<String>, available_orgs: Vec<String>) -> Self {
        Self {
            error: "org_choice_required",
            message: message.into(),
            name: None,
            available_orgs: Some(available_orgs),
        }
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

    /// Follow-up hint shown under the message in a terminal.
    pub fn help(&self) -> Option<&'static str> {
        match self.error {
            "unauthenticated" => Some("Run `ana login`, then retry."),
            "search_unavailable" => {
                Some("The service may be temporarily unavailable. Try again later.")
            }
            _ => None,
        }
    }
}

impl From<ToolError> for PackageIntelligenceError {
    /// The terminal (non-JSON) form of a tool error.
    fn from(error: ToolError) -> Self {
        PackageIntelligenceError::Tool {
            help: error.help().map(str::to_string),
            code: error.error,
            message: error.message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Contract: what Python's `model_dump_json()` emits for the remote
    // service's `ToolError`, including the explicit `"name": null`.
    #[test]
    fn tool_error_contract() {
        assert_eq!(
            serde_json::to_string(&ToolError::invalid_request("bad")).unwrap(),
            r#"{"error":"invalid_request","message":"bad","name":null}"#
        );
        assert_eq!(
            serde_json::to_string(&ToolError::new("package_not_found", "missing").with_name("np"))
                .unwrap(),
            r#"{"error":"package_not_found","message":"missing","name":"np"}"#
        );
    }

    #[test]
    fn org_choice_error_contract() {
        assert_eq!(
            serde_json::to_string(&ToolError::org_choice("Pick one.", vec!["a".into(), "b".into()]))
                .unwrap(),
            r#"{"error":"org_choice_required","message":"Pick one.","name":null,"available_orgs":["a","b"]}"#
        );
    }

    #[test]
    fn serialization_error_json_matches_tool_error() {
        let expected = ToolError::new(
            "internal_error",
            "The tool result could not be serialized. Try again later.",
        );
        assert_eq!(
            SERIALIZATION_ERROR_JSON,
            serde_json::to_string(&expected).unwrap()
        );
    }

    #[test]
    fn help_hints() {
        assert_eq!(
            ToolError::unauthenticated().help(),
            Some("Run `ana login`, then retry.")
        );
        assert!(
            ToolError::new("search_unavailable", "x")
                .help()
                .unwrap()
                .contains("Try again later")
        );
        assert_eq!(ToolError::new("package_not_found", "x").help(), None);
    }

    #[test]
    fn converts_to_terminal_diagnostic() {
        match PackageIntelligenceError::from(ToolError::unauthenticated()) {
            PackageIntelligenceError::Tool {
                code,
                message,
                help,
            } => {
                assert_eq!(code, "unauthenticated");
                assert!(message.contains("ana login"));
                assert_eq!(help.as_deref(), Some("Run `ana login`, then retry."));
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
