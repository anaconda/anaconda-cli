//! API key management.

use base64::Engine;
use base64::prelude::BASE64_URL_SAFE_NO_PAD;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use super::errors::AuthError;
use crate::VERSION;
use crate::http::Client;

/// Request body for creating a v1 API key.
#[derive(Debug, Serialize)]
struct CreateApiKeyRequest {
    scopes: Vec<String>,
    tags: Vec<String>,
}

/// Request body for creating a v2 API key.
#[derive(Debug, Serialize)]
struct CreateApiKeyV2Request {
    name: String,
    scopes: Vec<String>,
    tags: Vec<String>,
    expires_at: String,
}

/// Response from the v1 API key creation endpoint.
#[derive(Debug, Deserialize)]
struct ApiKeyResponse {
    api_key: String,
}

/// Response from the v2 API key creation endpoint.
#[derive(Debug, Deserialize)]
struct ApiKeyV2Response {
    api_key: String,
    key: ApiKeyV2Metadata,
}

/// Metadata for a v2 API key.
#[derive(Debug, Deserialize)]
struct ApiKeyV2Metadata {
    expires_at: Option<String>,
}

/// JWT payload containing expiration timestamp.
#[derive(Debug, Deserialize)]
struct JwtPayload {
    exp: i64,
}

/// Extract expiration date from a JWT token.
///
/// Returns the expiration as a YYYY-MM-DD string, or None if parsing fails.
pub fn get_expiration(api_key: &str) -> Option<String> {
    // JWT format: header.payload.signature
    let parts: Vec<&str> = api_key.split('.').collect();
    if parts.len() != 3 {
        return None;
    }

    // Decode the payload (middle part) - JWT uses base64url encoding
    let payload_bytes = BASE64_URL_SAFE_NO_PAD.decode(parts[1]).ok()?;
    let payload: JwtPayload = serde_json::from_slice(&payload_bytes).ok()?;

    // Convert Unix timestamp to date string
    let datetime = chrono::DateTime::from_timestamp(payload.exp, 0)?;
    Some(datetime.format("%Y-%m-%d").to_string())
}

/// Validate that a string is a valid API key (v1 JWT or v2 opaque token).
///
/// Returns true if the token is either:
/// - A v2 key (starts with "ak_" prefix)
/// - A v1 JWT (3 parts, decodable payload)
pub fn is_valid_api_key(api_key: &str) -> bool {
    // V2 keys start with "ak_" prefix
    if api_key.starts_with("ak_") {
        return api_key.len() > 3;
    }

    // V1 keys are JWTs
    let parts: Vec<&str> = api_key.split('.').collect();
    if parts.len() != 3 {
        return false;
    }

    // Try to decode the payload
    BASE64_URL_SAFE_NO_PAD.decode(parts[1]).is_ok()
}

/// Result of API key creation, includes expiration for display.
pub struct ApiKeyCreationResult {
    pub api_key: String,
    pub expires_at: Option<String>,
}

/// Create a new v1 API key using the access token.
pub async fn create_api_key(client: &Client, access_token: &str) -> Result<ApiKeyCreationResult, AuthError> {
    let payload = CreateApiKeyRequest {
        scopes: vec![
            "cloud:read".to_string(),
            "cloud:write".to_string(),
            "repo:read".to_string(),
        ],
        tags: vec![format!("ana-cli/v{}", VERSION)],
    };

    let response = client
        .post("/api/auth/api-keys")
        .bearer_auth(access_token)
        .json(&payload)
        .send()
        .await?;

    if response.status() != reqwest::StatusCode::CREATED {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        tracing::error!("Failed to create API key: {} - {}", status, body);
        return Err(AuthError::Authorization(format!(
            "Failed to create API key: {} - {}",
            status, body
        )));
    }

    let response: ApiKeyResponse = response.json().await?;
    Ok(ApiKeyCreationResult {
        api_key: response.api_key,
        expires_at: None, // V1 expiration is parsed from JWT later
    })
}

/// Create a new v2 API key using the access token.
pub async fn create_api_key_v2(client: &Client, access_token: &str) -> Result<ApiKeyCreationResult, AuthError> {
    let now = Utc::now();
    let expires_at = now + Duration::days(365);
    let name = format!("ana-cli {}", now.format("%Y-%m-%d"));

    let payload = CreateApiKeyV2Request {
        name,
        scopes: vec![
            "cloud:read".to_string(),
            "cloud:write".to_string(),
            "repo:read".to_string(),
        ],
        tags: vec![format!("ana-cli/v{}", VERSION)],
        expires_at: expires_at.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    };

    let response = client
        .post("/api/auth/api-keys/v2")
        .bearer_auth(access_token)
        .json(&payload)
        .send()
        .await?;

    if response.status() != reqwest::StatusCode::CREATED {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        tracing::error!("Failed to create v2 API key: {} - {}", status, body);
        return Err(AuthError::Authorization(format!(
            "Failed to create v2 API key: {} - {}",
            status, body
        )));
    }

    let response: ApiKeyV2Response = response.json().await?;

    // Parse expiration date from response (format: 2026-12-31T23:59:59Z)
    let expires_at_date = response.key.expires_at.as_ref().and_then(|s| {
        s.get(..10).map(|d| d.to_string())
    });

    Ok(ApiKeyCreationResult {
        api_key: response.api_key,
        expires_at: expires_at_date,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_create_api_key_request_serialize() {
        let request = CreateApiKeyRequest {
            scopes: vec!["cloud:read".to_string()],
            tags: vec!["test".to_string()],
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["scopes"], json!(["cloud:read"]));
        assert_eq!(json["tags"], json!(["test"]));
    }

    #[test]
    fn test_api_key_response_deserialize() {
        let response: ApiKeyResponse = serde_json::from_value(json!({
            "api_key": "ak-1234567890abcdef"
        }))
        .unwrap();
        assert_eq!(response.api_key, "ak-1234567890abcdef");
    }
}
