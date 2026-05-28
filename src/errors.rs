//! Centralized error types for reuse across the CLI.
//!
//! Uses miette for rich diagnostic output and thiserror for ergonomic error derivation.

use miette::Diagnostic;
use thiserror::Error;

/// HTTP and network-related errors.
#[derive(Error, Debug, Diagnostic)]
pub enum HttpError {
    #[error("HTTP request failed: {0}")]
    #[diagnostic(code(ana::http::request_failed))]
    Request(#[from] reqwest::Error),

    #[error("HTTP middleware error: {0}")]
    #[diagnostic(code(ana::http::middleware))]
    Middleware(String),

    #[error("HTTP client build error: {0}")]
    #[diagnostic(code(ana::http::client_build))]
    ClientBuild(String),
}

impl From<reqwest_middleware::Error> for HttpError {
    fn from(e: reqwest_middleware::Error) -> Self {
        HttpError::Middleware(e.to_string())
    }
}

/// I/O and filesystem errors.
#[derive(Error, Debug, Diagnostic)]
pub enum IoError {
    #[error("I/O error: {0}")]
    #[diagnostic(code(ana::io::general))]
    General(#[from] std::io::Error),

    #[error("Failed to read file: {path}")]
    #[diagnostic(code(ana::io::read_file))]
    ReadFile {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to write file: {path}")]
    #[diagnostic(code(ana::io::write_file))]
    WriteFile {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Path not found: {0}")]
    #[diagnostic(code(ana::io::not_found))]
    NotFound(String),
}

/// Configuration and parsing errors.
#[derive(Error, Debug, Diagnostic)]
pub enum ConfigError {
    #[error("Failed to parse configuration: {0}")]
    #[diagnostic(code(ana::config::parse))]
    Parse(String),

    #[error("Invalid configuration value: {key} = {value}")]
    #[diagnostic(code(ana::config::invalid_value))]
    InvalidValue { key: String, value: String },

    #[error("Missing required configuration: {0}")]
    #[diagnostic(code(ana::config::missing))]
    Missing(String),
}

/// Version and update errors.
#[derive(Error, Debug, Diagnostic, PartialEq)]
pub enum UpdateError {
    #[error("HTTP error: {0}")]
    #[diagnostic(code(ana::update::http))]
    Http(String),

    #[error("I/O error: {0}")]
    #[diagnostic(code(ana::update::io))]
    Io(String),

    #[error("Failed to parse version: {0}")]
    #[diagnostic(code(ana::update::version_parse))]
    VersionParse(String),

    #[error("No release asset found for platform: {0}")]
    #[diagnostic(code(ana::update::asset_not_found))]
    AssetNotFound(String),

    #[error("Unsupported platform: {0}")]
    #[diagnostic(code(ana::update::unsupported_platform))]
    UnsupportedPlatform(String),
}

impl From<reqwest::Error> for UpdateError {
    fn from(e: reqwest::Error) -> Self {
        UpdateError::Http(e.to_string())
    }
}

impl From<reqwest_middleware::Error> for UpdateError {
    fn from(e: reqwest_middleware::Error) -> Self {
        UpdateError::Http(e.to_string())
    }
}

/// QR code generation errors.
#[derive(Error, Debug, Diagnostic, Clone)]
pub enum QrError {
    #[error("URL too long ({0} bytes), max 134 chars")]
    #[diagnostic(
        code(ana::qr::too_long),
        help("Shorten the URL to 134 characters or fewer")
    )]
    TooLong(usize),

    #[error("URL contains non-Latin-1 characters")]
    #[diagnostic(
        code(ana::qr::invalid_byte),
        help("URL must contain only ASCII or Latin-1 characters")
    )]
    InvalidByte,
}

/// Tool installation and management errors.
#[derive(Error, Debug, Diagnostic)]
pub enum ToolError {
    #[error("Tool not found: {0}")]
    #[diagnostic(code(ana::tool::not_found))]
    NotFound(String),

    #[error("Tool already installed: {0}")]
    #[diagnostic(code(ana::tool::already_installed))]
    AlreadyInstalled(String),

    #[error("Failed to install tool: {name}")]
    #[diagnostic(code(ana::tool::install_failed))]
    InstallFailed {
        name: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Failed to uninstall tool: {name}")]
    #[diagnostic(code(ana::tool::uninstall_failed))]
    UninstallFailed {
        name: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Command execution failed: {0}")]
    #[diagnostic(code(ana::tool::command_failed))]
    CommandFailed(String),
}

/// Error when Outerbounds is not configured.
#[derive(Error, Debug, Diagnostic)]
#[error("Outerbounds is not configured.")]
#[diagnostic(
    code(ana::outerbounds::not_configured),
    help(
        "To configure, visit your Outerbounds instance and follow the instructions for local setup.\n\
         You will need to run: outerbounds configure <token>"
    )
)]
pub struct OuterboundsNotConfiguredError;

/// Error when tool management is unavailable (conda-package build).
#[cfg(feature = "conda-package")]
#[derive(Error, Debug, Diagnostic)]
#[error("Tool management is not available in the conda package.")]
#[diagnostic(
    code(ana::tool::conda_package),
    help(
        "When installed as a conda package, tools are managed by conda.\n\
         To use `ana tool install/uninstall`, install ana standalone:\n\
         \n\
         curl -fsSL https://anaconda.sh | bash"
    )
)]
pub struct ToolManagementUnavailableError;

/// Error when anaconda-mcp is not installed (conda-package build).
#[cfg(feature = "conda-package")]
#[derive(Error, Debug, Diagnostic)]
#[error("The mcp subcommand requires anaconda-mcp to be installed.")]
#[diagnostic(
    code(ana::mcp::not_installed),
    help("Install it with:\n\n    conda install anaconda-mcp")
)]
pub struct AnacondaMcpNotInstalledError;

/// Error when self-update is unavailable.
#[cfg(not(feature = "self-update"))]
#[derive(Error, Debug, Diagnostic)]
#[error("Self-update is not available in this build.")]
#[diagnostic(
    code(ana::self_update::unavailable),
    help("If installed via conda, update with:\n\n    conda update ana-cli")
)]
pub struct SelfUpdateUnavailableError;

/// Authentication errors (re-exported from auth module for convenience).
pub use crate::auth::errors::AuthError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_error_display() {
        let err = HttpError::Middleware("connection refused".to_string());
        assert_eq!(err.to_string(), "HTTP middleware error: connection refused");
    }

    #[test]
    fn test_update_error_display() {
        let err = UpdateError::VersionParse("invalid".to_string());
        assert_eq!(err.to_string(), "Failed to parse version: invalid");

        let err = UpdateError::AssetNotFound("linux-riscv64".to_string());
        assert_eq!(
            err.to_string(),
            "No release asset found for platform: linux-riscv64"
        );
    }

    #[test]
    fn test_qr_error_display() {
        let err = QrError::TooLong(200);
        assert_eq!(err.to_string(), "URL too long (200 bytes), max 134 chars");

        let err = QrError::InvalidByte;
        assert_eq!(err.to_string(), "URL contains non-Latin-1 characters");
    }

    #[test]
    fn test_config_error_display() {
        let err = ConfigError::Missing("api_key".to_string());
        assert_eq!(err.to_string(), "Missing required configuration: api_key");

        let err = ConfigError::InvalidValue {
            key: "timeout".to_string(),
            value: "not_a_number".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Invalid configuration value: timeout = not_a_number"
        );
    }

    #[test]
    fn test_tool_error_display() {
        let err = ToolError::NotFound("conda".to_string());
        assert_eq!(err.to_string(), "Tool not found: conda");

        let err = ToolError::AlreadyInstalled("pixi".to_string());
        assert_eq!(err.to_string(), "Tool already installed: pixi");
    }
}
