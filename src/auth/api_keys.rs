//! API key management.

use base64::Engine;
use base64::prelude::BASE64_URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};

use super::errors::AuthError;
use crate::VERSION;
use crate::config::Config;

/// Request body for creating an API key.
#[derive(Debug, Serialize)]
struct CreateApiKeyRequest {
    scopes: Vec<String>,
    tags: Vec<String>,
}

/// Response from the API key creation endpoint.
#[derive(Debug, Deserialize)]
struct ApiKeyResponse {
    api_key: String,
}

/// JWT payload containing expiration timestamp.
#[derive(Debug, Deserialize)]
struct JwtPayload {
    exp: i64,
}

/// Result of creating an API key.
pub struct ApiKeyResult {
    pub api_key: String,
    pub expires_at: Option<String>,
}

/// Extract expiration date from a JWT token.
///
/// Returns the expiration as a YYYY-MM-DD string, or None if parsing fails.
fn extract_jwt_expiration(token: &str) -> Option<String> {
    // JWT format: header.payload.signature
    let parts: Vec<&str> = token.split('.').collect();
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

/// Create a new API key using the access token.
pub async fn create_api_key(
    client: &reqwest_middleware::ClientWithMiddleware,
    config: &Config,
    access_token: &str,
) -> Result<ApiKeyResult, AuthError> {
    let url = format!("{}/api/auth/api-keys", config.base_url());
    let payload = CreateApiKeyRequest {
        scopes: vec![
            "cloud:read".to_string(),
            "cloud:write".to_string(),
            "repo:read".to_string(),
        ],
        tags: vec![format!("ana-cli/v{}", VERSION)],
    };

    let response = client
        .post(&url)
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
    let expires_at = extract_jwt_expiration(&response.api_key);

    Ok(ApiKeyResult {
        api_key: response.api_key,
        expires_at,
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
