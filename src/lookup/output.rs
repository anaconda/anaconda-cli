//! Output for `ana package` and `ana forum`: JSON for agents and scripts, text for people.
//!
//! JSON is used when stdout is not a terminal (an agent's shell, a pipe) or
//! when `--json` is passed. Errors follow the same rule: in JSON mode they are
//! printed to stdout as a `ToolError` object and the command exits non-zero
//! without a second, human-oriented diagnostic.

use std::io::IsTerminal;

use serde::Serialize;

use super::errors::{SERIALIZATION_ERROR_JSON, ToolError};
use crate::errors::LookupError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Json,
    Text,
}

impl OutputMode {
    /// JSON when forced or when stdout is not a terminal.
    ///
    /// The terminal check is the standard library's `IsTerminal` (stable
    /// since Rust 1.70), which replaced the `atty` and `is-terminal` crates;
    /// it is what `console` and the rest of the CLI use too. Only the
    /// JSON-vs-text decision is ours.
    pub fn detect(force_json: bool) -> Self {
        Self::select(force_json, std::io::stdout().is_terminal())
    }

    fn select(force_json: bool, stdout_is_terminal: bool) -> Self {
        if force_json || !stdout_is_terminal {
            Self::Json
        } else {
            Self::Text
        }
    }
}

/// Print a command result in the given mode.
///
/// Generic so one function serves every command's response type (each is a
/// distinct `Serialize` struct with its own text renderer); the alternative is
/// a copy of this function per command.
pub fn emit<T: Serialize>(
    mode: OutputMode,
    result: Result<T, ToolError>,
    render: fn(&T) -> String,
) -> miette::Result<()> {
    match (mode, result) {
        (OutputMode::Json, Ok(value)) => match serde_json::to_string_pretty(&value) {
            Ok(json) => {
                println!("{json}");
                Ok(())
            }
            Err(e) => {
                tracing::error!(error = %e, "failed to serialize result");
                println!("{SERIALIZATION_ERROR_JSON}");
                Err(LookupError::Reported("internal_error".to_string()).into())
            }
        },
        (OutputMode::Text, Ok(value)) => {
            print!("{}", render(&value));
            Ok(())
        }
        (mode, Err(error)) => Err(emit_error(mode, error)),
    }
}

/// Turn a tool error into the command's error, printing it first in JSON mode.
pub fn emit_error(mode: OutputMode, error: ToolError) -> miette::Report {
    match mode {
        OutputMode::Json => {
            let json = serde_json::to_string_pretty(&error)
                .unwrap_or_else(|_| SERIALIZATION_ERROR_JSON.to_string());
            println!("{json}");
            LookupError::Reported(format!("{}: {}", error.error, error.message)).into()
        }
        OutputMode::Text => LookupError::from(error).into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_when_forced_or_piped() {
        assert_eq!(OutputMode::select(true, true), OutputMode::Json);
        assert_eq!(OutputMode::select(false, false), OutputMode::Json);
        assert_eq!(OutputMode::select(true, false), OutputMode::Json);
        assert_eq!(OutputMode::select(false, true), OutputMode::Text);
    }

    #[test]
    fn json_errors_are_reported_markers() {
        let report = emit_error(OutputMode::Json, ToolError::unauthenticated());
        assert!(matches!(
            report.downcast_ref::<LookupError>(),
            Some(LookupError::Reported(_))
        ));
    }

    #[test]
    fn text_errors_are_diagnostics_with_help() {
        let report = emit_error(OutputMode::Text, ToolError::unauthenticated());
        match report.downcast_ref::<LookupError>() {
            Some(LookupError::Tool { code, help, .. }) => {
                assert_eq!(*code, "unauthenticated");
                assert_eq!(help.as_deref(), Some("Run `ana login`, then retry."));
            }
            _ => panic!("expected LookupError::Tool"),
        }
        let report = emit_error(
            OutputMode::Text,
            ToolError::new("package_not_found", "missing"),
        );
        assert!(matches!(
            report.downcast_ref::<LookupError>(),
            Some(LookupError::Tool { help: None, .. })
        ));
    }
}
