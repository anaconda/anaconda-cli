//! OAuth 2.0 Device Authorization Grant (RFC 8628) implementation.

use std::thread;
use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;

use super::api_keys::create_api_key;
use crate::config::Config;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

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
}

/// OpenID Connect discovery document.
#[derive(Debug, Deserialize)]
struct OpenIdConfig {
    device_authorization_endpoint: Option<String>,
    token_endpoint: String,
}

/// Response from the device authorization endpoint.
#[derive(Debug, Deserialize)]
struct DeviceAuthResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    expires_in: u64,
    interval: Option<u64>,
}

/// Response from the token endpoint.
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
}

/// Error response from the token endpoint during polling.
#[derive(Debug, Deserialize)]
struct TokenErrorResponse {
    error: String,
    error_description: Option<String>,
}

/// Perform the device authorization flow.
pub fn login() -> Result<(), AuthError> {
    let config = Config::load();
    let client = reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()?;

    // TODO(mattkram): Better handling for common exceptions like SSL cert, etc.
    // Fetch OpenID configuration
    let openid_config: OpenIdConfig = client.get(&config.well_known_url()).send()?.json()?;

    let device_auth_endpoint = openid_config
        .device_authorization_endpoint
        .ok_or_else(|| AuthError::MissingEndpoint("device_authorization_endpoint".to_string()))?;

    // Request device authorization
    let device_response: DeviceAuthResponse = client
        .post(&device_auth_endpoint)
        .form(&[
            ("client_id", config.client_id.as_str()),
            ("scope", "openid profile email"),
        ])
        .send()?
        .json()?;

    // Display instructions to user
    println!("To authenticate, visit:");
    println!();
    if let Some(ref uri) = device_response.verification_uri_complete {
        println!("  {}", uri);
    } else {
        println!("  {}", device_response.verification_uri);
        println!();
        println!("And enter the code: {}", device_response.user_code);
    }
    println!();

    // Open browser if configured
    if config.open_browser {
        let uri = device_response
            .verification_uri_complete
            .as_ref()
            .unwrap_or(&device_response.verification_uri);
        if let Err(e) = webbrowser::open(uri) {
            eprintln!("Could not open browser: {}", e);
        }
    }

    // TODO: Spinner?
    println!("Waiting for authentication...");

    // Poll for token
    let interval = Duration::from_secs(device_response.interval.unwrap_or(5));
    let timeout = Duration::from_secs(device_response.expires_in);
    let start = std::time::Instant::now();

    loop {
        if start.elapsed() > timeout {
            return Err(AuthError::Timeout);
        }

        thread::sleep(interval);

        let response = client
            .post(&openid_config.token_endpoint)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", &device_response.device_code),
                ("client_id", &config.client_id),
            ])
            .send()?;

        if response.status().is_success() {
            let token: TokenResponse = response.json()?;
            println!();
            println!("Successfully authenticated!");

            // Create API key
            println!("Creating API key...");
            let api_key = create_api_key(&client, &config, &token.access_token)?;
            println!();
            println!("API Key: {}", api_key);
            // TODO: Store API key securely
            return Ok(());
        }

        let error: TokenErrorResponse = response.json()?;
        match error.error.as_str() {
            "authorization_pending" => continue,
            "slow_down" => {
                thread::sleep(Duration::from_secs(5));
                continue;
            }
            "expired_token" => return Err(AuthError::Timeout),
            "access_denied" => {
                return Err(AuthError::Authorization(
                    "Access denied by user".to_string(),
                ));
            }
            _ => {
                return Err(AuthError::Authorization(
                    error
                        .error_description
                        .unwrap_or_else(|| error.error.clone()),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_openid_config_deserialize() {
        let config: OpenIdConfig = serde_json::from_value(json!({
            "device_authorization_endpoint": "https://example.com/device",
            "token_endpoint": "https://example.com/token"
        }))
        .unwrap();
        assert_eq!(
            config.device_authorization_endpoint,
            Some("https://example.com/device".to_string())
        );
        assert_eq!(config.token_endpoint, "https://example.com/token");
    }

    #[test]
    fn test_openid_config_without_device_endpoint() {
        let config: OpenIdConfig = serde_json::from_value(json!({
            "token_endpoint": "https://example.com/token"
        }))
        .unwrap();
        assert!(config.device_authorization_endpoint.is_none());
    }

    #[test]
    fn test_device_auth_response_deserialize() {
        let response: DeviceAuthResponse = serde_json::from_value(json!({
            "device_code": "abc123",
            "user_code": "ABCD-1234",
            "verification_uri": "https://example.com/verify",
            "verification_uri_complete": "https://example.com/verify?code=ABCD-1234",
            "expires_in": 600,
            "interval": 5
        }))
        .unwrap();
        assert_eq!(response.device_code, "abc123");
        assert_eq!(response.user_code, "ABCD-1234");
        assert_eq!(response.verification_uri, "https://example.com/verify");
        assert_eq!(response.expires_in, 600);
        assert_eq!(response.interval, Some(5));
    }

    #[test]
    fn test_device_auth_response_minimal() {
        let response: DeviceAuthResponse = serde_json::from_value(json!({
            "device_code": "abc",
            "user_code": "XYZ",
            "verification_uri": "https://example.com",
            "expires_in": 300
        }))
        .unwrap();
        assert!(response.verification_uri_complete.is_none());
        assert!(response.interval.is_none());
    }

    #[test]
    fn test_token_error_response_deserialize() {
        let response: TokenErrorResponse = serde_json::from_value(json!({
            "error": "authorization_pending"
        }))
        .unwrap();
        assert_eq!(response.error, "authorization_pending");
        assert!(response.error_description.is_none());
    }

    #[test]
    fn test_token_error_response_with_description() {
        let response: TokenErrorResponse = serde_json::from_value(json!({
            "error": "access_denied",
            "error_description": "User denied access"
        }))
        .unwrap();
        assert_eq!(response.error, "access_denied");
        assert_eq!(
            response.error_description,
            Some("User denied access".to_string())
        );
    }

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
    }
}
