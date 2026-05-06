//! Authentication error types.

use miette::Diagnostic;
use thiserror::Error;

#[derive(Error, Debug, Diagnostic)]
pub enum AuthError {
    #[error("HTTP request failed: {0}")]
    #[diagnostic(code(ana::auth::request))]
    Request(#[from] reqwest::Error),

    #[error("HTTP middleware error: {0}")]
    #[diagnostic(code(ana::auth::middleware))]
    Middleware(String),

    #[error("Authorization failed: {0}")]
    #[diagnostic(code(ana::auth::authorization))]
    Authorization(String),

    #[error("Authorization timed out")]
    #[diagnostic(code(ana::auth::timeout))]
    Timeout,

    #[error("Missing endpoint in OpenID configuration: {0}")]
    #[diagnostic(code(ana::auth::missing_endpoint))]
    MissingEndpoint(String),

    #[error("Keyring error: {0}")]
    #[diagnostic(code(ana::auth::keyring))]
    Keyring(String),

    #[error("Invalid API key: {0}")]
    #[diagnostic(code(ana::auth::invalid_api_key))]
    InvalidApiKey(String),

    #[error("Not logged in")]
    #[diagnostic(
        code(ana::auth::not_logged_in),
        help("Run `ana login` to authenticate")
    )]
    NotLoggedIn,
}

impl From<reqwest_middleware::Error> for AuthError {
    fn from(e: reqwest_middleware::Error) -> Self {
        AuthError::Middleware(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_error_display() {
        let err = AuthError::Timeout;
        assert_eq!(err.to_string(), "Authorization timed out");

        let err = AuthError::Authorization("test error".to_string());
        assert_eq!(err.to_string(), "Authorization failed: test error");

        let err = AuthError::MissingEndpoint("device_authorization_endpoint".to_string());
        assert_eq!(
            err.to_string(),
            "Missing endpoint in OpenID configuration: device_authorization_endpoint"
        );

        let err = AuthError::Keyring("file not found".to_string());
        assert_eq!(err.to_string(), "Keyring error: file not found");
    }
}
