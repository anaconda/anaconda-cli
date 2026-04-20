//! Authentication actions (login, logout, whoami).

use std::time::Duration;

use serde::Deserialize;
use tokio::time::sleep;

use super::api_keys::create_api_key;
use super::errors::AuthError;
use super::keyring::{delete_api_key, get_api_key, save_api_key};
use crate::config::Config;
use crate::http::{Client, bearer_header, build_client};
use crate::input::KeyListener;
use crate::ui::status;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Print a QR code to the terminal with indentation.
fn print_qr(qr: &str) {
    status::blank_line();
    for line in qr.lines() {
        eprintln!("    {}", line);
    }
    status::blank_line();
}

/// HTTP client with configuration and optional authentication.
pub struct ApiClient {
    inner: Client,
    api_key: Option<String>,
    domain: String,
}

impl ApiClient {
    /// Create a new API client, loading credentials from the keyring if available.
    pub fn new() -> Result<Self, AuthError> {
        let config = Config::load();
        let api_key = get_api_key(&config)?;

        let mut builder = reqwest::Client::builder().timeout(REQUEST_TIMEOUT);
        if let Some(ref key) = api_key {
            builder = builder.default_headers(bearer_header(key));
        }

        let client = Client::new(builder, config.base_url())?;

        Ok(Self {
            inner: client,
            api_key,
            domain: config.domain,
        })
    }

    /// Check if the client has valid credentials.
    pub fn is_authenticated(&self) -> bool {
        self.api_key.is_some()
    }

    /// Get the configured domain.
    pub fn domain(&self) -> &str {
        &self.domain
    }

    /// GET request.
    pub fn get(&self, url: &str) -> reqwest_middleware::RequestBuilder {
        self.inner.get(url)
    }

    /// POST request.
    #[allow(dead_code)]
    pub fn post(&self, url: &str) -> reqwest_middleware::RequestBuilder {
        self.inner.post(url)
    }

    /// PUT request.
    #[allow(dead_code)]
    pub fn put(&self, url: &str) -> reqwest_middleware::RequestBuilder {
        self.inner.put(url)
    }

    /// PATCH request.
    #[allow(dead_code)]
    pub fn patch(&self, url: &str) -> reqwest_middleware::RequestBuilder {
        self.inner.patch(url)
    }

    /// DELETE request.
    #[allow(dead_code)]
    pub fn delete(&self, url: &str) -> reqwest_middleware::RequestBuilder {
        self.inner.delete(url)
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

/// Account information from the API.
#[derive(Debug, Deserialize)]
struct AccountInfo {
    email: Option<String>,
    user: Option<UserInfo>,
}

/// User information nested in account response.
#[derive(Debug, Deserialize)]
struct UserInfo {
    username: Option<String>,
}

/// API key information from the API.
#[derive(Debug, Deserialize)]
struct ApiKeyInfo {
    expires_at: Option<String>,
}

/// Print logged-in user status line.
///
/// Example: `✓ Logged in as kford@anaconda.com (anaconda)`
fn print_logged_in_status(email: &str, org: &str) {
    status::success(&format!(
        "Logged in as {} ({})",
        status::highlight(email),
        org
    ));
}

/// Calculate days from today until a date string (YYYY-MM-DD format).
fn days_until_date(date_str: &str) -> Option<i64> {
    let expires = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()?;
    let today = chrono::Utc::now().date_naive();
    Some((expires - today).num_days())
}

/// Print token expiration info.
///
/// Example: `  expires    2026-04-09 (365 days)`
fn print_expiration(expires_at: &str) {
    // Parse the expiration date and calculate days remaining
    if let Some(days_remaining) = days_until_date(&expires_at[..10]) {
        let days_str = if days_remaining == 1 {
            "1 day".to_string()
        } else {
            format!("{} days", days_remaining)
        };
        eprintln!(
            "  {}{}{}",
            status::dim("expires    "),
            status::highlight(&expires_at[..10]),
            status::dim(&format!(" ({days_str})"))
        );
    }
}

/// Combined login information for display.
struct LoginInfo {
    email: String,
    org: String,
    expires_at: Option<String>,
}

/// Fetch login info (account + API key expiration) for display after login.
async fn fetch_login_info(config: &Config) -> Result<LoginInfo, AuthError> {
    let client = ApiClient::new()?;

    // Fetch account info
    let account_response = client.get("/api/account").send().await?;
    let account: AccountInfo = account_response.json().await?;

    let email = account
        .email
        .or_else(|| account.user.and_then(|u| u.username))
        .unwrap_or_else(|| "unknown".to_string());

    // Fetch API key info for expiration
    let keys_response = client.get("/api/auth/api-keys").send().await?;
    let keys: Vec<ApiKeyInfo> = keys_response.json().await.unwrap_or_default();
    let expires_at = keys.first().and_then(|k| k.expires_at.clone());

    Ok(LoginInfo {
        email,
        org: config.domain.clone(),
        expires_at,
    })
}

/// Perform the device authorization flow.
pub async fn login() -> Result<(), AuthError> {
    // We use a new, unauthenticated client instead of ApiClient, since
    // login by definition happens first. It ends up being simpler to do
    // this, at least for now, because the auth flow needs to follow direct
    // URLs from openid-configuration etc.
    let config = Config::load();

    let client = build_client(reqwest::Client::builder().timeout(REQUEST_TIMEOUT))?;

    // Fetch OpenID configuration
    let openid_config: OpenIdConfig = client
        .get(&config.well_known_url())
        .send()
        .await?
        .json()
        .await?;

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
        .send()
        .await?
        .json()
        .await?;

    // Display instructions to user
    let display_uri = device_response
        .verification_uri_complete
        .as_deref()
        .unwrap_or(&device_response.verification_uri);

    // Try to open browser first — this determines whether we show QR immediately
    let browser_opened = if config.open_browser {
        let uri = device_response
            .verification_uri_complete
            .as_ref()
            .unwrap_or(&device_response.verification_uri);
        webbrowser::open(uri).is_ok()
    } else {
        false
    };

    // Pre-generate the QR code string
    let qr_output = crate::qr::qr_to_terminal(display_uri, 1, true).ok();

    // Listen for 'q' keypress in a background thread (for on-demand QR).
    // KeyListener handles terminal state restoration and Ctrl+C.
    let listen_for_q = browser_opened && qr_output.is_some();
    let key_listener = if listen_for_q {
        KeyListener::spawn(&['q'])
    } else {
        None
    };

    let mut qr_shown = false;
    if browser_opened {
        // Browser opened — clean layout, offer QR on demand
        status::info(&format!(
            "Opening {} in your browser...",
            status::highlight(&config.domain)
        ));
        status::blank_line();
        status::info("To authenticate, visit:");
        status::blank_line();
        eprintln!("  {}", status::highlight(display_uri));
        if device_response.verification_uri_complete.is_none() {
            status::blank_line();
            status::info(&format!(
                "And enter the code: {}",
                status::highlight(&device_response.user_code)
            ));
        }
        status::blank_line();
        if qr_output.is_some() {
            status::waiting("Waiting for authentication... (press q for QR code)");
        } else {
            status::waiting("Waiting for authentication...");
        }
    } else {
        status::info("To authenticate, scan the QR code or visit:");
        status::blank_line();
        eprintln!("  {}", status::highlight(display_uri));
        if device_response.verification_uri_complete.is_none() {
            status::blank_line();
            status::info(&format!(
                "And enter the code: {}",
                status::highlight(&device_response.user_code)
            ));
        }
        status::blank_line();
        status::waiting("Waiting for authentication...");

        // No browser — show QR code immediately
        if let Some(ref qr) = qr_output {
            print_qr(qr);
            qr_shown = true;
        }
    }

    // Poll for token
    let interval = Duration::from_secs(device_response.interval.unwrap_or(5));
    let timeout = Duration::from_secs(device_response.expires_in);
    let start = std::time::Instant::now();

    loop {
        if start.elapsed() > timeout {
            tracing::error!("Authentication timed out");
            return Err(AuthError::Timeout);
        }

        // Check for 'q' keypress while waiting
        let sleep_until = std::time::Instant::now() + interval;
        while std::time::Instant::now() < sleep_until {
            if !qr_shown {
                if let Some(ref listener) = key_listener {
                    if listener.try_recv().is_some() {
                        if let Some(ref qr) = qr_output {
                            print_qr(qr);
                            qr_shown = true;
                        }
                    }
                }
            }
            sleep(Duration::from_millis(100)).await;
        }

        let response = client
            .post(&openid_config.token_endpoint)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", &device_response.device_code),
                ("client_id", &config.client_id),
            ])
            .send()
            .await?;

        if response.status().is_success() {
            let token: TokenResponse = response.json().await?;
            status::blank_line();
            status::success("Authentication complete");

            // Create API key
            let api_key = create_api_key(&client, &config, &token.access_token).await?;

            // Save to keyring
            save_api_key(&config, &api_key)?;
            status::success("Token stored in keyring");

            // Fetch and display user info
            if let Ok(login_info) = fetch_login_info(&config).await {
                print_logged_in_status(&login_info.email, &login_info.org);
                if let Some(ref expires_at) = login_info.expires_at {
                    print_expiration(expires_at);
                }
            }

            return Ok(());
        }

        let error: TokenErrorResponse = response.json().await?;
        match error.error.as_str() {
            "authorization_pending" => continue,
            "slow_down" => {
                sleep(Duration::from_secs(5)).await;
                continue;
            }
            "expired_token" => {
                tracing::error!("Token expired during authentication");
                return Err(AuthError::Timeout);
            }
            "access_denied" => {
                tracing::error!("Access denied by user");
                return Err(AuthError::Authorization(
                    "Access denied by user".to_string(),
                ));
            }
            _ => {
                let msg = error
                    .error_description
                    .unwrap_or_else(|| error.error.clone());
                tracing::error!("Authorization error: {}", msg);
                return Err(AuthError::Authorization(msg));
            }
        }
    }
}

/// Log out by removing the API key for the current domain.
pub fn logout() -> Result<(), AuthError> {
    let config = Config::load();
    delete_api_key(&config)?;
    status::success(&format!(
        "Logged out of {}",
        status::highlight(&config.domain)
    ));
    status::success("Token removed from system keyring");
    status::warn(&format!(
        "To fully revoke your token visit {}",
        status::highlight(&format!("{}/settings/tokens", config.domain))
    ));
    Ok(())
}

/// Display the API key for the current domain.
pub fn show_api_key() -> Result<(), AuthError> {
    let config = Config::load();

    match get_api_key(&config)? {
        Some(key) => println!("{}", key),
        None => {
            status::error("not logged in");
            status::info(&format!(
                "Run {} to authenticate.",
                status::highlight("ana login")
            ));
        }
    }

    Ok(())
}

/// Display information about the logged-in user.
pub async fn whoami() -> Result<(), AuthError> {
    let client = ApiClient::new()?;

    if !client.is_authenticated() {
        status::error("not logged in");
        status::info(&format!(
            "Run {} to authenticate.",
            status::highlight("ana login")
        ));
        return Ok(());
    }

    let response = client.get("/api/account").send().await?;

    if !response.status().is_success() {
        let resp_status = response.status();
        let body = response.text().await.unwrap_or_default();
        tracing::error!("Failed to get account info: {} - {}", resp_status, body);
        return Err(AuthError::Authorization(format!(
            "Failed to get account info: {} - {}",
            resp_status, body
        )));
    }

    let data: serde_json::Value = response.json().await?;
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
