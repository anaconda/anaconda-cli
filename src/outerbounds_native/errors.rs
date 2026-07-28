use std::path::PathBuf;

use miette::Diagnostic;
use thiserror::Error;

#[derive(Error, Debug, Diagnostic)]
pub enum ConfigError {
    #[error("Config file not found: {path}")]
    #[diagnostic(code(ana::ob::config_not_found))]
    NotFound { path: PathBuf },

    #[error("Failed to read config file: {path}")]
    #[diagnostic(code(ana::ob::config_read_failed))]
    ReadFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to parse config file {path}: {reason}")]
    #[diagnostic(code(ana::ob::config_parse_failed))]
    ParseFailed { path: PathBuf, reason: String },

    #[error("Failed to write config file: {path}")]
    #[diagnostic(code(ana::ob::config_write_failed))]
    WriteFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to decode configuration string")]
    #[diagnostic(
        code(ana::ob::decode_failed),
        help("Ensure the config string is valid base64+zlib compressed JSON")
    )]
    DecodeFailed(String),

    #[error("Missing required config key: {key}")]
    #[diagnostic(code(ana::ob::missing_key))]
    MissingKey { key: String },

    #[error("Invalid config type: {config_type}")]
    #[diagnostic(code(ana::ob::invalid_config_type))]
    InvalidConfigType { config_type: String },

    #[error("Failed to fetch remote config from {url}")]
    #[diagnostic(code(ana::ob::remote_fetch_failed))]
    RemoteFetchFailed {
        url: String,
        #[source]
        source: reqwest_middleware::Error,
    },

    #[error("OBP_CONFIG_DIR is set to {path} but no ob_config.json exists there")]
    #[diagnostic(code(ana::ob::obp_config_dir_missing))]
    ObpConfigDirMissing { path: PathBuf },

    #[error("ob_config.json exists but missing required key: {key}")]
    #[diagnostic(code(ana::ob::ob_config_missing_key))]
    ObConfigMissingKey { key: String },
}

#[derive(Error, Debug, Diagnostic)]
pub enum ServicePrincipalError {
    #[error("Missing --name")]
    #[diagnostic(
        code(ana::ob::sp::missing_name),
        help("Provide it on the command line or via --from-obproject-toml")
    )]
    MissingName,

    #[error("Missing --deployment-domain")]
    #[diagnostic(
        code(ana::ob::sp::missing_domain),
        help("Provide it on the command line or via --from-obproject-toml")
    )]
    MissingDeploymentDomain,

    #[error("No JWT token provided")]
    #[diagnostic(
        code(ana::ob::sp::no_jwt),
        help("Provide --jwt-token or use --github-actions in CI")
    )]
    NoJwtToken,

    #[error("GitHub Actions environment not detected")]
    #[diagnostic(
        code(ana::ob::sp::not_gha),
        help(
            "Ensure ACTIONS_ID_TOKEN_REQUEST_TOKEN and ACTIONS_ID_TOKEN_REQUEST_URL are set, and 'id-token: write' permission is granted"
        )
    )]
    NotGitHubActions,

    #[error("Failed to fetch GitHub Actions JWT")]
    #[diagnostic(code(ana::ob::sp::gha_jwt_failed))]
    GitHubActionsJwtFailed {
        #[source]
        source: reqwest_middleware::Error,
    },

    #[error("Failed to exchange JWT for origin token")]
    #[diagnostic(code(ana::ob::sp::origin_token_failed))]
    OriginTokenFailed {
        #[source]
        source: reqwest_middleware::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_principal_error_messages() {
        assert_eq!(
            ServicePrincipalError::MissingName.to_string(),
            "Missing --name"
        );
        assert_eq!(
            ServicePrincipalError::MissingDeploymentDomain.to_string(),
            "Missing --deployment-domain"
        );
        assert_eq!(
            ServicePrincipalError::NoJwtToken.to_string(),
            "No JWT token provided"
        );
        assert_eq!(
            ServicePrincipalError::NotGitHubActions.to_string(),
            "GitHub Actions environment not detected"
        );
    }

    #[test]
    fn test_config_error_messages() {
        let err = ConfigError::NotFound {
            path: PathBuf::from("/tmp/config.json"),
        };
        assert!(err.to_string().contains("/tmp/config.json"));

        let err = ConfigError::MissingKey {
            key: "API_KEY".to_string(),
        };
        assert!(err.to_string().contains("API_KEY"));

        let err = ConfigError::DecodeFailed("bad input".to_string());
        assert!(err.to_string().contains("decode"));
    }
}
