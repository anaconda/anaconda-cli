//! JSON response types for authentication APIs.

use serde::Deserialize;

/// OpenID Connect discovery document.
#[derive(Debug, Deserialize)]
pub struct OpenIdConfig {
    pub device_authorization_endpoint: Option<String>,
    pub token_endpoint: String,
}

/// Response from the device authorization endpoint.
#[derive(Debug, Deserialize)]
pub struct DeviceAuthResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    pub interval: Option<u64>,
}

/// Response from the token endpoint.
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
}

/// Error response from the token endpoint during polling.
#[derive(Debug, Deserialize)]
pub struct TokenErrorResponse {
    pub error: String,
    pub error_description: Option<String>,
}

/// Account information from the API.
#[derive(Debug, Deserialize)]
pub struct AccountResponse {
    pub user: Option<UserInfo>,
}

/// User information nested in account response.
#[derive(Debug, Deserialize)]
pub struct UserInfo {
    pub username: Option<String>,
    pub email: Option<String>,
}
