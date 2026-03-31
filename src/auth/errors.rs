//! Authentication error types.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Authorization failed: {0}")]
    Authorization(String),

    #[error("Authorization timed out")]
    Timeout,

    #[error("Missing endpoint in OpenID configuration: {0}")]
    MissingEndpoint(String),

    #[error("Keyring error: {0}")]
    Keyring(String),

    #[error("Invalid API key")]
    InvalidKey,
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
