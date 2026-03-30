//! Authentication actions (login, logout, whoami).

use std::thread;
use std::time::Duration;

use serde::Deserialize;

use super::api_keys::create_api_key;
use super::errors::AuthError;
use super::keyring::{delete_api_key, get_api_key, save_api_key};
use crate::config::Config;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// HTTP client with configuration and optional authentication.
pub struct ApiClient {
    client: reqwest::blocking::Client,
    config: Config,
    api_key: Option<String>,
}

impl ApiClient {
    /// Create a new API client, loading credentials from the keyring if available.
    pub fn new() -> Result<Self, AuthError> {
        let config = Config::load();
        let api_key = get_api_key(&config)?;
        let client = reqwest::blocking::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()?;

        Ok(Self {
            client,
            config,
            api_key,
        })
    }

    /// Check if the client has valid credentials.
    pub fn is_authenticated(&self) -> bool {
        self.api_key.is_some()
    }

    /// Get the configured domain.
    pub fn domain(&self) -> &str {
        &self.config.domain
    }

    /// Make an authenticated GET request to an API endpoint.
    pub fn get(&self, path: &str) -> Result<reqwest::blocking::Response, AuthError> {
        let url = format!("{}{}", self.config.base_url(), path);
        let mut request = self.client.get(&url);

        if let Some(ref api_key) = self.api_key {
            request = request.bearer_auth(api_key);
        }

        Ok(request.send()?)
    }

    /// Get the underlying HTTP client for custom requests.
    pub fn raw_client(&self) -> &reqwest::blocking::Client {
        &self.client
    }

    /// Get the configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }
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
    let api_client = ApiClient::new()?;
    let client = api_client.raw_client();
    let config = api_client.config();

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

            // Save to keyring
            save_api_key(&config, &api_key)?;
            println!("API key saved to {}", config.keyring_path.display());
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

/// Log out by removing the API key for the current domain.
pub fn logout() -> Result<(), AuthError> {
    let config = Config::load();
    delete_api_key(&config)?;
    println!("Logged out from {}", config.domain);
    Ok(())
}

/// Display the API key for the current domain.
pub fn show_api_key() -> Result<(), AuthError> {
    let config = Config::load();

    match get_api_key(&config)? {
        Some(key) => println!("{}", key),
        None => {
            println!("Not logged in to {}", config.domain);
            println!("Run `ana login` to authenticate.");
        }
    }

    Ok(())
}

/// Display information about the logged-in user.
pub fn whoami() -> Result<(), AuthError> {
    let client = ApiClient::new()?;

    if !client.is_authenticated() {
        println!("Not logged in to {}", client.domain());
        println!("Run `ana login` to authenticate.");
        return Ok(());
    }

    let response = client.get("/api/account")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(AuthError::Authorization(format!(
            "Failed to get account info: {} - {}",
            status, body
        )));
    }

    let data: serde_json::Value = response.json()?;
    let pretty = serde_json::to_string_pretty(&data).unwrap_or_default();

    println!("Your info ({}):", client.domain());
    println!("{}", pretty);

    Ok(())
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
}
