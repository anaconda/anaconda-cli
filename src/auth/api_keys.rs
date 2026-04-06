//! API key management.

use serde::{Deserialize, Serialize};

use super::actions::ApiClient;
use super::errors::AuthError;
use crate::VERSION;

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

/// Create a new API key using the access token.
pub async fn create_api_key(client: &ApiClient, access_token: &str) -> Result<String, AuthError> {
    let payload = CreateApiKeyRequest {
        scopes: vec![
            "cloud:read".to_string(),
            "cloud:write".to_string(),
            "repo:read".to_string(),
        ],
        tags: vec![format!("ana-cli/v{}", VERSION)],
    };

    // TODO: AAU token header is normally added here in anaconda-auth
    let response = client
        .send(
            client
                .post_builder("/api/auth/api-keys")
                .bearer_auth(access_token)
                .json(&payload),
        )
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        tracing::error!(%status, %body, "failed to create API key");
        return Err(AuthError::Authorization("Failed to create API key".into()));
    }

    let response: ApiKeyResponse = response.json().await?;
    Ok(response.api_key)
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
