use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Metaflow config stored in ~/.metaflowconfig/config.json (or config_{profile}.json)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetaflowConfig {
    // Authentication
    #[serde(
        rename = "METAFLOW_SERVICE_AUTH_KEY",
        skip_serializing_if = "Option::is_none"
    )]
    pub service_auth_key: Option<String>,

    // OBP servers
    #[serde(rename = "OBP_API_SERVER", skip_serializing_if = "Option::is_none")]
    pub obp_api_server: Option<String>,

    #[serde(rename = "OBP_AUTH_SERVER", skip_serializing_if = "Option::is_none")]
    pub obp_auth_server: Option<String>,

    #[serde(
        rename = "OBP_METAFLOW_CONFIG_URL",
        skip_serializing_if = "Option::is_none"
    )]
    pub obp_metaflow_config_url: Option<String>,

    // Metaflow service
    #[serde(
        rename = "METAFLOW_SERVICE_URL",
        skip_serializing_if = "Option::is_none"
    )]
    pub service_url: Option<String>,

    #[serde(rename = "METAFLOW_UI_URL", skip_serializing_if = "Option::is_none")]
    pub ui_url: Option<String>,

    // Datastore
    #[serde(
        rename = "METAFLOW_DEFAULT_DATASTORE",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_datastore: Option<String>,

    #[serde(
        rename = "METAFLOW_DATASTORE_SYSROOT_S3",
        skip_serializing_if = "Option::is_none"
    )]
    pub datastore_sysroot_s3: Option<String>,

    #[serde(
        rename = "METAFLOW_DATATOOLS_S3ROOT",
        skip_serializing_if = "Option::is_none"
    )]
    pub datatools_s3root: Option<String>,

    // Kubernetes
    #[serde(
        rename = "METAFLOW_KUBERNETES_NAMESPACE",
        skip_serializing_if = "Option::is_none"
    )]
    pub kubernetes_namespace: Option<String>,

    #[serde(
        rename = "METAFLOW_KUBERNETES_SANDBOX_INIT_SCRIPT",
        skip_serializing_if = "Option::is_none"
    )]
    pub kubernetes_sandbox_init_script: Option<String>,

    // Defaults
    #[serde(
        rename = "METAFLOW_DEFAULT_METADATA",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_metadata: Option<String>,

    #[serde(
        rename = "METAFLOW_DEFAULT_AWS_CLIENT_PROVIDER",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_aws_client_provider: Option<String>,

    // Perimeter (sometimes included in metaflow config)
    #[serde(rename = "OBP_PERIMETER", skip_serializing_if = "Option::is_none")]
    pub obp_perimeter: Option<String>,

    // Forward compatibility: capture any unknown fields
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

impl MetaflowConfig {
    /// Get the OBP API server URL with https:// prefix
    pub fn obp_api_server_url(&self) -> Option<String> {
        self.obp_api_server.as_ref().map(|s| sanitize_url(s))
    }

    /// Get the OBP auth server URL with https:// prefix
    pub fn obp_auth_server_url(&self) -> Option<String> {
        self.obp_auth_server.as_ref().map(|s| sanitize_url(s))
    }
}

/// OB-specific config stored in ~/.metaflowconfig/ob_config.json (or ob_config_{profile}.json)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ObConfig {
    #[serde(
        rename = "OB_CURRENT_PERIMETER",
        skip_serializing_if = "Option::is_none"
    )]
    pub current_perimeter: Option<String>,

    #[serde(
        rename = "OB_CURRENT_PERIMETER_MF_CONFIG_URL",
        skip_serializing_if = "Option::is_none"
    )]
    pub perimeter_config_url: Option<String>,

    /// Legacy key for backwards compatibility with workstations
    #[serde(
        rename = "OB_CURRENT_PERIMETER_URL",
        skip_serializing_if = "Option::is_none"
    )]
    pub perimeter_url_legacy: Option<String>,

    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

impl ObConfig {
    /// Get the perimeter config URL, checking both current and legacy keys
    pub fn config_url(&self) -> Option<&str> {
        self.perimeter_config_url
            .as_deref()
            .or(self.perimeter_url_legacy.as_deref())
    }
}

/// Resolved configuration after potentially fetching from remote URL
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub metaflow: MetaflowConfig,
    pub ob: Option<ObConfig>,
    pub profile: Option<String>,
    /// The URL the config was fetched from, if any
    pub source_url: Option<String>,
}

/// Ensure URL has https:// prefix and no trailing slash
fn sanitize_url(url: &str) -> String {
    let url = if url.starts_with("https://") || url.starts_with("http://") {
        url.to_string()
    } else {
        format!("https://{}", url)
    };
    url.trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metaflow_config_serde_roundtrip() {
        let config = MetaflowConfig {
            service_auth_key: Some("secret-token".into()),
            obp_api_server: Some("api.example.com".into()),
            obp_auth_server: Some("auth.example.com".into()),
            default_datastore: Some("s3".into()),
            ..Default::default()
        };

        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: MetaflowConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.service_auth_key, Some("secret-token".into()));
        assert_eq!(parsed.obp_api_server, Some("api.example.com".into()));
        assert_eq!(parsed.default_datastore, Some("s3".into()));
    }

    #[test]
    fn metaflow_config_preserves_unknown_fields() {
        let json = r#"{
            "METAFLOW_SERVICE_AUTH_KEY": "token",
            "SOME_FUTURE_KEY": "future-value",
            "ANOTHER_KEY": 123
        }"#;

        let config: MetaflowConfig = serde_json::from_str(json).unwrap();

        assert_eq!(config.service_auth_key, Some("token".into()));
        assert_eq!(
            config.extra.get("SOME_FUTURE_KEY").and_then(|v| v.as_str()),
            Some("future-value")
        );
        assert_eq!(
            config.extra.get("ANOTHER_KEY").and_then(|v| v.as_i64()),
            Some(123)
        );

        // Roundtrip preserves unknown fields
        let serialized = serde_json::to_string(&config).unwrap();
        assert!(serialized.contains("SOME_FUTURE_KEY"));
        assert!(serialized.contains("ANOTHER_KEY"));
    }

    #[test]
    fn ob_config_url_precedence() {
        // New key takes precedence
        let config = ObConfig {
            current_perimeter: Some("prod".into()),
            perimeter_config_url: Some("https://new.url".into()),
            perimeter_url_legacy: Some("https://old.url".into()),
            extra: HashMap::new(),
        };
        assert_eq!(config.config_url(), Some("https://new.url"));

        // Falls back to legacy
        let config = ObConfig {
            current_perimeter: Some("prod".into()),
            perimeter_config_url: None,
            perimeter_url_legacy: Some("https://old.url".into()),
            extra: HashMap::new(),
        };
        assert_eq!(config.config_url(), Some("https://old.url"));

        // None if neither set
        let config = ObConfig::default();
        assert!(config.config_url().is_none());
    }

    #[test]
    fn sanitize_url_adds_https() {
        assert_eq!(sanitize_url("api.example.com"), "https://api.example.com");
        assert_eq!(
            sanitize_url("https://api.example.com"),
            "https://api.example.com"
        );
        assert_eq!(
            sanitize_url("http://api.example.com"),
            "http://api.example.com"
        );
    }

    #[test]
    fn sanitize_url_removes_trailing_slash() {
        assert_eq!(sanitize_url("api.example.com/"), "https://api.example.com");
        assert_eq!(
            sanitize_url("https://api.example.com/"),
            "https://api.example.com"
        );
    }

    #[test]
    fn obp_server_url_helpers() {
        let config = MetaflowConfig {
            obp_api_server: Some("api.example.com".into()),
            obp_auth_server: Some("https://auth.example.com/".into()),
            ..Default::default()
        };

        assert_eq!(
            config.obp_api_server_url(),
            Some("https://api.example.com".into())
        );
        assert_eq!(
            config.obp_auth_server_url(),
            Some("https://auth.example.com".into())
        );
    }
}
