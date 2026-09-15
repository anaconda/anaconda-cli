//! Terms of Service acceptance for Anaconda MCP.
//!
//! Acceptance state is stored in `~/.anaconda/config.toml` under `[plugin.mcp]`
//! (overridable via `ANACONDA_CONFIG_TOML`), shared with other Anaconda tools.
//! The `ANACONDA_MCP_ACCEPTED_TERMS` and `ANACONDA_MCP_ACCEPTED_TERMS_VERSION`
//! environment variables override the file on read.

use std::io::IsTerminal;
use std::path::PathBuf;

use miette::{IntoDiagnostic, miette};

use super::commands::McpTermsCommands;
use crate::context::CommandContext;
use crate::errors::McpError;
use crate::input::prompt_yes_no;
use crate::ui::status;
use crate::ui::styles::UiColor;

/// Version of the Terms of Service that users must accept.
pub const CURRENT_TOS_VERSION: &str = "2026-05-27";

pub const TERMS_OF_SERVICE: &str = "\
# Anaconda MCP Terms of Service

Anaconda MCP is a beta product covered by Beta Terms

https://www.anaconda.com/legal/terms/mcpbeta

By entering 'y' below, I agree to the Beta Terms. To the extent
these terms differ from any other agreement with Anaconda,
these Beta Terms control.

This product is not intended for production use.
";

/// Acceptance state for the Terms of Service.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct TermsState {
    /// `None` = not yet responded, `Some(false)` = declined, `Some(true)` = accepted.
    pub accepted: Option<bool>,
    pub accepted_version: Option<String>,
}

impl TermsState {
    /// True when terms are accepted at the current version.
    pub fn is_current(&self) -> bool {
        self.accepted == Some(true) && self.accepted_version.as_deref() == Some(CURRENT_TOS_VERSION)
    }
}

/// Path to the shared Anaconda config file.
pub fn config_path() -> PathBuf {
    if let Ok(p) = std::env::var("ANACONDA_CONFIG_TOML")
        && !p.is_empty()
    {
        return PathBuf::from(p);
    }
    crate::paths::home_dir()
        .join(".anaconda")
        .join("config.toml")
}

fn parse_env_bool(val: &str) -> Option<bool> {
    match val.trim().to_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "y" | "t" => Some(true),
        "0" | "false" | "no" | "off" | "n" | "f" => Some(false),
        _ => None,
    }
}

fn read_state_from_file() -> TermsState {
    let Ok(content) = std::fs::read_to_string(config_path()) else {
        return TermsState::default();
    };
    let Ok(doc) = content.parse::<toml_edit::DocumentMut>() else {
        return TermsState::default();
    };
    let mcp = doc.get("plugin").and_then(|p| p.get("mcp"));
    let accepted = mcp
        .and_then(|m| m.get("accepted_terms"))
        .and_then(|i| i.as_bool());
    let accepted_version = mcp
        .and_then(|m| m.get("accepted_terms_version"))
        .and_then(|i| i.as_str())
        .map(String::from);
    TermsState {
        accepted,
        accepted_version,
    }
}

/// Read the acceptance state, with environment variables overriding the file.
pub fn read_state() -> TermsState {
    let mut state = read_state_from_file();
    if let Ok(v) = std::env::var("ANACONDA_MCP_ACCEPTED_TERMS")
        && let Some(b) = parse_env_bool(&v)
    {
        state.accepted = Some(b);
    }
    if let Ok(v) = std::env::var("ANACONDA_MCP_ACCEPTED_TERMS_VERSION")
        && !v.is_empty()
    {
        state.accepted_version = Some(v);
    }
    state
}

/// Persist acceptance (or decline) to the config file, preserving other content.
pub fn persist_acceptance(accepted: bool) -> miette::Result<()> {
    let path = config_path();
    let mut doc = match std::fs::read_to_string(&path) {
        Ok(content) => content
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| miette!("Failed to parse {}: {}", path.display(), e))?,
        Err(_) => toml_edit::DocumentMut::new(),
    };

    let plugin = super::clients::ensure_toml_table(doc.as_table_mut(), "plugin");
    let plugin_table = plugin
        .as_table_mut()
        .ok_or_else(|| miette!("{}: [plugin] is not a table", path.display()))?;
    let mcp = super::clients::ensure_toml_table(plugin_table, "mcp");
    mcp["accepted_terms"] = toml_edit::value(accepted);
    if accepted {
        mcp["accepted_terms_version"] = toml_edit::value(CURRENT_TOS_VERSION);
    } else if let Some(table) = mcp.as_table_like_mut() {
        table.remove("accepted_terms_version");
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).into_diagnostic()?;
    }
    std::fs::write(&path, doc.to_string()).into_diagnostic()?;
    Ok(())
}

/// Verify terms acceptance before running an MCP subcommand.
///
/// On a TTY, prompts interactively instead of erroring.
pub fn ensure_accepted(ctx: &mut CommandContext) -> miette::Result<()> {
    if read_state().is_current() {
        return Ok(());
    }

    if !std::io::stdout().is_terminal() {
        return Err(McpError::TermsNotAccepted.into());
    }

    print_terms();
    let accepted = prompt_yes_no("Do you accept the Beta Terms?", false);
    persist_acceptance(accepted)?;

    if !accepted {
        return Err(McpError::TermsDeclined.into());
    }

    status::blank_line();
    let consent = prompt_yes_no(
        "(Optional) May Anaconda contact you about your experience using Anaconda MCP?",
        false,
    );
    if consent {
        ctx.telemetry.add("mcp_contact_consent", true);
    }
    Ok(())
}

/// Run a `terms` subcommand.
pub fn run(
    ctx: &mut CommandContext,
    command: Option<McpTermsCommands>,
    json: bool,
) -> miette::Result<()> {
    match command {
        None => show(json),
        Some(McpTermsCommands::Status { json }) => status(json),
        Some(McpTermsCommands::Accept { json, consent }) => accept(ctx, json, consent),
    }
}

/// Print the Terms of Service with light styling (bold heading, highlighted URL).
fn print_terms() {
    status::blank_line();
    for line in TERMS_OF_SERVICE.trim_end().lines() {
        if let Some(heading) = line.strip_prefix("# ") {
            eprintln!("{}", UiColor::BoxText.apply_bold(heading));
        } else if line.starts_with("https://") {
            eprintln!("{}", status::highlight(line));
        } else {
            eprintln!("{line}");
        }
    }
    status::blank_line();
}

fn show(json: bool) -> miette::Result<()> {
    if json {
        let out = serde_json::json!({
            "terms": TERMS_OF_SERVICE,
            "version": CURRENT_TOS_VERSION,
        });
        println!("{}", serde_json::to_string_pretty(&out).into_diagnostic()?);
    } else {
        print_terms();
    }
    Ok(())
}

fn status(json: bool) -> miette::Result<()> {
    let state = read_state();
    let accepted = state.accepted == Some(true);
    let needs_reaccept = accepted && state.accepted_version.as_deref() != Some(CURRENT_TOS_VERSION);

    if json {
        let out = serde_json::json!({
            "accepted": accepted,
            "accepted_version": state.accepted_version,
            "current_version": CURRENT_TOS_VERSION,
            "needs_reaccept": needs_reaccept,
        });
        println!("{}", serde_json::to_string_pretty(&out).into_diagnostic()?);
        if !accepted || needs_reaccept {
            std::process::exit(1);
        }
        return Ok(());
    }

    if !accepted {
        let label = if state.accepted == Some(false) {
            "declined"
        } else {
            "not yet responded"
        };
        status::error(&format!("Terms of Service: {label}"));
        std::process::exit(1);
    }

    if needs_reaccept {
        status::warn(&format!(
            "Terms of Service: accepted (version {}), but current version is {}",
            state.accepted_version.as_deref().unwrap_or("unknown"),
            CURRENT_TOS_VERSION
        ));
        status::tip("run `ana mcp terms accept` to re-accept");
        std::process::exit(1);
    }

    status::success("Terms of Service: accepted");
    Ok(())
}

fn accept(ctx: &mut CommandContext, json: bool, consent: bool) -> miette::Result<()> {
    let already_current = read_state().is_current();

    if !already_current {
        persist_acceptance(true)?;
    }

    if consent {
        ctx.telemetry.add("mcp_contact_consent", true);
    }

    if json {
        let out = serde_json::json!({
            "accepted": true,
            "accepted_version": CURRENT_TOS_VERSION,
            "previously_accepted": already_current,
        });
        println!("{}", serde_json::to_string_pretty(&out).into_diagnostic()?);
        return Ok(());
    }

    if already_current {
        status::info("Terms of Service have already been accepted.");
    } else {
        status::success("Terms of Service accepted.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn with_config(path: &std::path::Path, f: impl FnOnce()) {
        temp_env::with_vars(
            [
                (
                    "ANACONDA_CONFIG_TOML",
                    Some(path.to_string_lossy().as_ref()),
                ),
                ("ANACONDA_MCP_ACCEPTED_TERMS", None::<&str>),
                ("ANACONDA_MCP_ACCEPTED_TERMS_VERSION", None::<&str>),
            ],
            f,
        );
    }

    #[test]
    fn test_parse_env_bool() {
        assert_eq!(parse_env_bool("true"), Some(true));
        assert_eq!(parse_env_bool("0"), Some(false));
        assert_eq!(parse_env_bool("yes"), Some(true));
        assert_eq!(parse_env_bool("garbage"), None);
    }

    #[test]
    #[serial(env)]
    fn test_read_state_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        with_config(&dir.path().join("config.toml"), || {
            assert_eq!(read_state(), TermsState::default());
        });
    }

    #[test]
    #[serial(env)]
    fn test_persist_and_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        with_config(&path, || {
            persist_acceptance(true).unwrap();
            let state = read_state();
            assert_eq!(state.accepted, Some(true));
            assert_eq!(state.accepted_version.as_deref(), Some(CURRENT_TOS_VERSION));
            assert!(state.is_current());

            persist_acceptance(false).unwrap();
            let state = read_state();
            assert_eq!(state.accepted, Some(false));
            assert_eq!(state.accepted_version, None);
            assert!(!state.is_current());
        });
    }

    #[test]
    #[serial(env)]
    fn test_persist_preserves_existing_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "# comment\n[plugin.auth]\napi_key = \"secret\"\n").unwrap();
        with_config(&path, || {
            persist_acceptance(true).unwrap();
            let content = std::fs::read_to_string(&path).unwrap();
            assert!(content.contains("# comment"));
            assert!(content.contains("api_key = \"secret\""));
            assert!(content.contains("accepted_terms = true"));
        });
    }

    #[test]
    #[serial(env)]
    fn test_env_overrides_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        with_config(&path, || {
            persist_acceptance(false).unwrap();
            temp_env::with_vars(
                [
                    ("ANACONDA_MCP_ACCEPTED_TERMS", Some("true")),
                    (
                        "ANACONDA_MCP_ACCEPTED_TERMS_VERSION",
                        Some(CURRENT_TOS_VERSION),
                    ),
                ],
                || {
                    assert!(read_state().is_current());
                },
            );
            assert!(!read_state().is_current());
        });
    }

    #[test]
    #[serial(env)]
    fn test_stale_version_needs_reaccept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        with_config(&path, || {
            persist_acceptance(true).unwrap();
            temp_env::with_var(
                "ANACONDA_MCP_ACCEPTED_TERMS_VERSION",
                Some("1999-01-01"),
                || {
                    let state = read_state();
                    assert_eq!(state.accepted, Some(true));
                    assert!(!state.is_current());
                },
            );
        });
    }
}
