use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use base64::prelude::*;
use flate2::read::ZlibDecoder;
use serde::Deserialize;
use serde_json::Value;

use crate::outerbounds_native::errors::ConfigError;

use super::reader::{default_config_dir, default_ob_config_dir};
use super::types::{MetaflowConfig, ObConfig};

/// Type of config encoding
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigType {
    /// Config values are inline in the encoded string
    Inline,
    /// Config should be fetched from AWS Secrets Manager
    AwsSecretsManager { arn: String, region: String },
}

/// Decoded configuration from a base64+zlib magic string
#[derive(Debug, Clone)]
pub struct DecodedConfig {
    pub config_type: ConfigType,
    pub config: MetaflowConfig,
    pub perimeter: Option<String>,
}

/// Intermediate struct for parsing the raw decoded JSON
#[derive(Debug, Deserialize)]
struct RawDecodedConfig {
    #[serde(rename = "OB_CONFIG_TYPE")]
    config_type: Option<String>,

    #[serde(rename = "OBP_PERIMETER")]
    perimeter: Option<String>,

    #[serde(rename = "OBP_METAFLOW_CONFIG_URL")]
    metaflow_config_url: Option<String>,

    #[serde(rename = "METAFLOW_SERVICE_AUTH_KEY")]
    service_auth_key: Option<String>,

    // AWS Secrets Manager fields
    #[serde(rename = "AWS_SECRETS_MANAGER_SECRET_ARN")]
    aws_secret_arn: Option<String>,

    #[serde(rename = "AWS_SECRETS_MANAGER_REGION")]
    aws_region: Option<String>,

    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

/// Decode a base64+zlib compressed config string.
///
/// The string may have a prefix like "ob-1.0:" which is stripped before decoding.
pub fn decode_config(encoded: &str) -> Result<DecodedConfig, ConfigError> {
    // Strip prefix if present (e.g., "ob-1.0:base64data")
    let encoded = encoded.split(':').next_back().unwrap_or(encoded);

    // Base64 decode
    let compressed = BASE64_STANDARD
        .decode(encoded.trim())
        .map_err(|e| ConfigError::DecodeFailed(format!("base64 decode failed: {}", e)))?;

    // Zlib decompress
    let mut decoder = ZlibDecoder::new(&compressed[..]);
    let mut json_bytes = Vec::new();
    std::io::Read::read_to_end(&mut decoder, &mut json_bytes)
        .map_err(|e| ConfigError::DecodeFailed(format!("zlib decompress failed: {}", e)))?;

    // Parse JSON
    let raw: RawDecodedConfig = serde_json::from_slice(&json_bytes)
        .map_err(|e| ConfigError::DecodeFailed(format!("JSON parse failed: {}", e)))?;

    // Determine config type
    let config_type = match raw.config_type.as_deref() {
        Some("aws-secrets-manager") => {
            let arn = raw.aws_secret_arn.ok_or_else(|| {
                ConfigError::DecodeFailed(
                    "AWS_SECRETS_MANAGER_SECRET_ARN required for aws-secrets-manager config type"
                        .to_string(),
                )
            })?;
            let region = raw.aws_region.ok_or_else(|| {
                ConfigError::DecodeFailed(
                    "AWS_SECRETS_MANAGER_REGION required for aws-secrets-manager config type"
                        .to_string(),
                )
            })?;
            ConfigType::AwsSecretsManager { arn, region }
        }
        Some("inline") | None => ConfigType::Inline,
        Some(other) => {
            return Err(ConfigError::InvalidConfigType {
                config_type: other.to_string(),
            })
        }
    };

    // Build MetaflowConfig from the decoded data
    let config = if raw.metaflow_config_url.is_some() {
        // If URL is present, only keep URL and auth key
        MetaflowConfig {
            obp_metaflow_config_url: raw.metaflow_config_url,
            service_auth_key: raw.service_auth_key,
            ..Default::default()
        }
    } else {
        // Rebuild full config from all fields
        let mut extra = raw.extra;

        // Add known fields back to extra for serialization into MetaflowConfig
        if let Some(key) = raw.service_auth_key {
            extra.insert("METAFLOW_SERVICE_AUTH_KEY".to_string(), Value::String(key));
        }
        if let Some(url) = raw.metaflow_config_url {
            extra.insert("OBP_METAFLOW_CONFIG_URL".to_string(), Value::String(url));
        }

        serde_json::from_value(Value::Object(extra.into_iter().collect()))
            .map_err(|e| ConfigError::DecodeFailed(format!("Failed to build config: {}", e)))?
    };

    Ok(DecodedConfig {
        config_type,
        config,
        perimeter: raw.perimeter,
    })
}

/// Encode a config to base64+zlib format (for testing/roundtrip verification)
pub fn encode_config(config: &HashMap<String, Value>) -> Result<String, ConfigError> {
    let json = serde_json::to_vec(config)
        .map_err(|e| ConfigError::DecodeFailed(format!("JSON serialize failed: {}", e)))?;

    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(&json)
        .map_err(|e| ConfigError::DecodeFailed(format!("zlib compress failed: {}", e)))?;
    let compressed = encoder
        .finish()
        .map_err(|e| ConfigError::DecodeFailed(format!("zlib finish failed: {}", e)))?;

    Ok(BASE64_STANDARD.encode(compressed))
}

/// Write decoded config to the filesystem.
///
/// Returns the path to the written config file.
pub fn write_config(
    decoded: &DecodedConfig,
    profile: Option<&str>,
    config_dir_override: Option<&Path>,
) -> Result<PathBuf, ConfigError> {
    let base_dir = config_dir_override
        .map(PathBuf::from)
        .unwrap_or_else(default_config_dir);

    // Ensure directory exists
    fs::create_dir_all(&base_dir).map_err(|e| ConfigError::WriteFailed {
        path: base_dir.clone(),
        source: e,
    })?;

    // Determine config file path
    let filename = match profile {
        Some(p) if !p.is_empty() => format!("config_{}.json", p),
        _ => "config.json".to_string(),
    };
    let config_path = base_dir.join(&filename);

    // Write metaflow config
    let json = serde_json::to_string_pretty(&decoded.config)
        .map_err(|e| ConfigError::DecodeFailed(format!("JSON serialize failed: {}", e)))?;

    fs::write(&config_path, json).map_err(|e| ConfigError::WriteFailed {
        path: config_path.clone(),
        source: e,
    })?;

    // Write ob_config.json if perimeter and URL are present
    if let (Some(perimeter), Some(url)) = (
        &decoded.perimeter,
        decoded.config.obp_metaflow_config_url.as_ref(),
    ) {
        let ob_dir = default_ob_config_dir();
        fs::create_dir_all(&ob_dir).map_err(|e| ConfigError::WriteFailed {
            path: ob_dir.clone(),
            source: e,
        })?;

        let ob_filename = match profile {
            Some(p) if !p.is_empty() => format!("ob_config_{}.json", p),
            _ => "ob_config.json".to_string(),
        };
        let ob_config_path = ob_dir.join(&ob_filename);

        let ob_config = ObConfig {
            current_perimeter: Some(perimeter.clone()),
            perimeter_config_url: Some(url.clone()),
            perimeter_url_legacy: None,
            extra: HashMap::new(),
        };

        let ob_json = serde_json::to_string_pretty(&ob_config)
            .map_err(|e| ConfigError::DecodeFailed(format!("JSON serialize failed: {}", e)))?;

        fs::write(&ob_config_path, ob_json).map_err(|e| ConfigError::WriteFailed {
            path: ob_config_path,
            source: e,
        })?;
    }

    Ok(config_path)
}

/// Check if a config file already exists for the given profile.
pub fn config_exists(profile: Option<&str>, config_dir_override: Option<&Path>) -> bool {
    let base_dir = config_dir_override
        .map(PathBuf::from)
        .unwrap_or_else(default_config_dir);

    let filename = match profile {
        Some(p) if !p.is_empty() => format!("config_{}.json", p),
        _ => "config.json".to_string(),
    };

    base_dir.join(filename).exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_temp_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    fn make_test_config() -> HashMap<String, Value> {
        let mut config = HashMap::new();
        config.insert(
            "METAFLOW_SERVICE_AUTH_KEY".to_string(),
            Value::String("test-token".to_string()),
        );
        config.insert(
            "OBP_API_SERVER".to_string(),
            Value::String("api.example.com".to_string()),
        );
        config
    }

    /// Sample config from Python test suite (test_config_writer.py)
    fn sample_metaflow_config() -> HashMap<String, Value> {
        serde_json::from_str(
            r#"{
                "METAFLOW_ARGO_EVENTS_EVENT": "metaflow-event",
                "METAFLOW_ARGO_EVENTS_EVENT_BUS": "default",
                "METAFLOW_ARGO_EVENTS_EVENT_SOURCE": "argo-events-webhook",
                "METAFLOW_ARGO_EVENTS_WEBHOOK_URL": "http://argo-events-webhook-eventsource-svc.jobs-default:12000/metaflow-event",
                "METAFLOW_ARGO_WORKFLOWS_ENV_VARS_TO_SKIP": "METAFLOW_SERVICE_HEADERS",
                "METAFLOW_ARGO_WORKFLOWS_KUBERNETES_SECRETS": "argo-workflows-default-service-principal-credentials",
                "METAFLOW_AWS_SECRETS_MANAGER_DEFAULT_REGION": "us-west-2",
                "METAFLOW_DATASTORE_SYSROOT_S3": "s3://obp-4bh26o-metaflow/metaflow",
                "METAFLOW_DATATOOLS_S3ROOT": "s3://obp-4bh26o-metaflow/data",
                "METAFLOW_DEFAULT_AWS_CLIENT_PROVIDER": "obp",
                "METAFLOW_DEFAULT_CONTAINER_REGISTRY": "public.ecr.aws/docker/library",
                "METAFLOW_DEFAULT_DATASTORE": "s3",
                "METAFLOW_DEFAULT_METADATA": "service",
                "METAFLOW_DEFAULT_SECRETS_BACKEND_TYPE": "aws-secrets-manager",
                "METAFLOW_KUBERNETES_CONTAINER_IMAGE": "006988687827.dkr.ecr.us-west-2.amazonaws.com/obptask-python:master-39509478-1675983838",
                "METAFLOW_KUBERNETES_NAMESPACE": "jobs-default",
                "METAFLOW_KUBERNETES_SANDBOX_INIT_SCRIPT": "eval $(curl https://outerbounds-public.s3.us-west-2.amazonaws.com/platform/kubernetes_auth_shim.py | OBP_AUTH_SERVER=auth.dev-latest.outerbounds.xyz python3 )",
                "METAFLOW_OTEL_ENDPOINT": "https://tracing.dev-latest.outerbounds.xyz/v1/traces",
                "METAFLOW_SERVICE_AUTH_KEY": "<REDACTED>",
                "METAFLOW_SERVICE_URL": "https://metadata.dev-latest.outerbounds.xyz/",
                "METAFLOW_UI_URL": "https://ui.dev-latest.outerbounds.xyz/",
                "METAFLOW_USER": "jackie@outerbounds.co",
                "OBP_AUTH_SERVER": "auth.dev-latest.outerbounds.xyz",
                "OBP_API_SERVER": "api.dev-latest.outerbounds.xyz"
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let original = make_test_config();
        let encoded = encode_config(&original).unwrap();
        let decoded = decode_config(&encoded).unwrap();

        assert_eq!(
            decoded.config.service_auth_key,
            Some("test-token".to_string())
        );
        assert_eq!(
            decoded.config.obp_api_server,
            Some("api.example.com".to_string())
        );
    }

    #[test]
    fn test_decode_strips_prefix() {
        let original = make_test_config();
        let encoded = encode_config(&original).unwrap();
        let prefixed = format!("ob-1.0:{}", encoded);

        let decoded = decode_config(&prefixed).unwrap();
        assert_eq!(
            decoded.config.service_auth_key,
            Some("test-token".to_string())
        );
    }

    #[test]
    fn test_decode_inline_config_type() {
        let mut config = make_test_config();
        config.insert(
            "OB_CONFIG_TYPE".to_string(),
            Value::String("inline".to_string()),
        );

        let encoded = encode_config(&config).unwrap();
        let decoded = decode_config(&encoded).unwrap();

        assert_eq!(decoded.config_type, ConfigType::Inline);
    }

    #[test]
    fn test_decode_default_config_type_is_inline() {
        let config = make_test_config();
        let encoded = encode_config(&config).unwrap();
        let decoded = decode_config(&encoded).unwrap();

        assert_eq!(decoded.config_type, ConfigType::Inline);
    }

    #[test]
    fn test_decode_aws_secrets_manager_config_type() {
        let mut config = HashMap::new();
        config.insert(
            "OB_CONFIG_TYPE".to_string(),
            Value::String("aws-secrets-manager".to_string()),
        );
        config.insert(
            "AWS_SECRETS_MANAGER_SECRET_ARN".to_string(),
            Value::String(
                "arn:aws:secretsmanager:us-east-1:123456789:secret:my-secret".to_string(),
            ),
        );
        config.insert(
            "AWS_SECRETS_MANAGER_REGION".to_string(),
            Value::String("us-east-1".to_string()),
        );

        let encoded = encode_config(&config).unwrap();
        let decoded = decode_config(&encoded).unwrap();

        assert!(matches!(
            decoded.config_type,
            ConfigType::AwsSecretsManager { .. }
        ));
        if let ConfigType::AwsSecretsManager { arn, region } = decoded.config_type {
            assert!(arn.contains("my-secret"));
            assert_eq!(region, "us-east-1");
        }
    }

    #[test]
    fn test_decode_aws_secrets_manager_missing_arn() {
        let mut config = HashMap::new();
        config.insert(
            "OB_CONFIG_TYPE".to_string(),
            Value::String("aws-secrets-manager".to_string()),
        );
        config.insert(
            "AWS_SECRETS_MANAGER_REGION".to_string(),
            Value::String("us-east-1".to_string()),
        );

        let encoded = encode_config(&config).unwrap();
        let result = decode_config(&encoded);

        assert!(matches!(result, Err(ConfigError::DecodeFailed(_))));
    }

    #[test]
    fn test_decode_invalid_config_type() {
        let mut config = HashMap::new();
        config.insert(
            "OB_CONFIG_TYPE".to_string(),
            Value::String("unknown-type".to_string()),
        );

        let encoded = encode_config(&config).unwrap();
        let result = decode_config(&encoded);

        assert!(matches!(result, Err(ConfigError::InvalidConfigType { .. })));
    }

    #[test]
    fn test_decode_with_perimeter() {
        let mut config = make_test_config();
        config.insert(
            "OBP_PERIMETER".to_string(),
            Value::String("production".to_string()),
        );

        let encoded = encode_config(&config).unwrap();
        let decoded = decode_config(&encoded).unwrap();

        assert_eq!(decoded.perimeter, Some("production".to_string()));
    }

    #[test]
    fn test_decode_with_metaflow_config_url() {
        let mut config = HashMap::new();
        config.insert(
            "METAFLOW_SERVICE_AUTH_KEY".to_string(),
            Value::String("token".to_string()),
        );
        config.insert(
            "OBP_METAFLOW_CONFIG_URL".to_string(),
            Value::String("https://example.com/config".to_string()),
        );
        config.insert(
            "OBP_API_SERVER".to_string(),
            Value::String("should-be-ignored".to_string()),
        );

        let encoded = encode_config(&config).unwrap();
        let decoded = decode_config(&encoded).unwrap();

        // When URL is present, only URL and auth key should be kept
        assert_eq!(decoded.config.service_auth_key, Some("token".to_string()));
        assert_eq!(
            decoded.config.obp_metaflow_config_url,
            Some("https://example.com/config".to_string())
        );
        // Other fields should not be present
        assert!(decoded.config.obp_api_server.is_none());
    }

    #[test]
    fn test_decode_invalid_base64() {
        let result = decode_config("not-valid-base64!!!");
        assert!(matches!(result, Err(ConfigError::DecodeFailed(_))));
    }

    #[test]
    fn test_decode_invalid_zlib() {
        let invalid = BASE64_STANDARD.encode(b"not compressed data");
        let result = decode_config(&invalid);
        assert!(matches!(result, Err(ConfigError::DecodeFailed(_))));
    }

    #[test]
    fn test_write_config_creates_file() {
        let tmp = setup_temp_dir();
        let decoded = DecodedConfig {
            config_type: ConfigType::Inline,
            config: MetaflowConfig {
                service_auth_key: Some("test-token".to_string()),
                obp_api_server: Some("api.example.com".to_string()),
                ..Default::default()
            },
            perimeter: None,
        };

        let path = write_config(&decoded, None, Some(tmp.path())).unwrap();

        assert!(path.exists());
        assert_eq!(path, tmp.path().join("config.json"));

        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("test-token"));
        assert!(contents.contains("api.example.com"));
    }

    #[test]
    fn test_write_config_with_profile() {
        let tmp = setup_temp_dir();
        let decoded = DecodedConfig {
            config_type: ConfigType::Inline,
            config: MetaflowConfig {
                service_auth_key: Some("prod-token".to_string()),
                ..Default::default()
            },
            perimeter: None,
        };

        let path = write_config(&decoded, Some("prod"), Some(tmp.path())).unwrap();

        assert_eq!(path, tmp.path().join("config_prod.json"));
        assert!(path.exists());
    }

    #[test]
    fn test_write_config_creates_ob_config_when_perimeter_and_url() {
        let tmp = setup_temp_dir();

        temp_env::with_vars(
            [
                ("METAFLOW_HOME", Some(tmp.path().to_str().unwrap())),
                ("OBP_CONFIG_DIR", None::<&str>),
            ],
            || {
                let decoded = DecodedConfig {
                    config_type: ConfigType::Inline,
                    config: MetaflowConfig {
                        service_auth_key: Some("token".to_string()),
                        obp_metaflow_config_url: Some("https://example.com/config".to_string()),
                        ..Default::default()
                    },
                    perimeter: Some("production".to_string()),
                };

                write_config(&decoded, None, Some(tmp.path())).unwrap();

                // Check ob_config.json was created
                let ob_config_path = tmp.path().join("ob_config.json");
                assert!(ob_config_path.exists());

                let ob_contents = fs::read_to_string(&ob_config_path).unwrap();
                assert!(ob_contents.contains("production"));
                assert!(ob_contents.contains("https://example.com/config"));
            },
        );
    }

    #[test]
    fn test_config_exists() {
        let tmp = setup_temp_dir();

        assert!(!config_exists(None, Some(tmp.path())));

        fs::write(tmp.path().join("config.json"), "{}").unwrap();
        assert!(config_exists(None, Some(tmp.path())));

        assert!(!config_exists(Some("prod"), Some(tmp.path())));

        fs::write(tmp.path().join("config_prod.json"), "{}").unwrap();
        assert!(config_exists(Some("prod"), Some(tmp.path())));
    }

    // Tests matching Python test suite (test_config_writer.py)

    #[test]
    fn test_serialization_roundtrip_matches_python() {
        // From Python: test_serialization - tests various data types
        let test_cases: Vec<HashMap<String, Value>> = vec![
            HashMap::new(),
            {
                let mut m = HashMap::new();
                m.insert("KEY1".to_string(), Value::String("VALUE1".to_string()));
                m.insert("KEY2".to_string(), Value::Number(2.into()));
                m
            },
        ];

        for original in test_cases {
            let encoded = encode_config(&original).unwrap();
            let decoded = decode_config(&encoded).unwrap();

            // Check all original keys are present in decoded config
            for (key, value) in &original {
                let decoded_value = decoded
                    .config
                    .extra
                    .get(key)
                    .expect(&format!("Missing key: {}", key));
                assert_eq!(decoded_value, value, "Mismatch for key: {}", key);
            }
        }
    }

    #[test]
    fn test_metaflow_config_inline_matches_python() {
        // From Python: test_metaflow_config_inline
        let config = sample_metaflow_config();
        let encoded = encode_config(&config).unwrap();
        let decoded = decode_config(&encoded).unwrap();

        // Verify known fields
        assert_eq!(
            decoded.config.service_auth_key,
            Some("<REDACTED>".to_string())
        );
        assert_eq!(
            decoded.config.obp_api_server,
            Some("api.dev-latest.outerbounds.xyz".to_string())
        );
        assert_eq!(
            decoded.config.obp_auth_server,
            Some("auth.dev-latest.outerbounds.xyz".to_string())
        );
        assert_eq!(
            decoded.config.service_url,
            Some("https://metadata.dev-latest.outerbounds.xyz/".to_string())
        );
        assert_eq!(
            decoded.config.ui_url,
            Some("https://ui.dev-latest.outerbounds.xyz/".to_string())
        );
        assert_eq!(decoded.config.default_datastore, Some("s3".to_string()));
        assert_eq!(decoded.config.default_metadata, Some("service".to_string()));
        assert_eq!(
            decoded.config.kubernetes_namespace,
            Some("jobs-default".to_string())
        );
        assert_eq!(
            decoded.config.datastore_sysroot_s3,
            Some("s3://obp-4bh26o-metaflow/metaflow".to_string())
        );
        assert_eq!(
            decoded.config.datatools_s3root,
            Some("s3://obp-4bh26o-metaflow/data".to_string())
        );
        assert_eq!(
            decoded.config.default_aws_client_provider,
            Some("obp".to_string())
        );

        // Verify extra fields are preserved
        assert_eq!(
            decoded
                .config
                .extra
                .get("METAFLOW_ARGO_EVENTS_EVENT")
                .and_then(|v| v.as_str()),
            Some("metaflow-event")
        );
        assert_eq!(
            decoded
                .config
                .extra
                .get("METAFLOW_USER")
                .and_then(|v| v.as_str()),
            Some("jackie@outerbounds.co")
        );
    }

    #[test]
    fn test_prefixed_config_matches_python() {
        // From Python: test_aws_secrets_manager uses "not-a-secret:" prefix
        let config = sample_metaflow_config();
        let encoded = encode_config(&config).unwrap();
        let prefixed = format!("not-a-secret:{}", encoded);

        let decoded = decode_config(&prefixed).unwrap();
        assert_eq!(
            decoded.config.service_auth_key,
            Some("<REDACTED>".to_string())
        );
    }

    #[test]
    fn test_aws_secrets_manager_config_type_matches_python() {
        // From Python: test_aws_secrets_manager - tests aws-secrets-manager config type parsing
        // Note: We don't test actual AWS calls, just the config type detection
        let mut config = HashMap::new();
        config.insert(
            "OB_CONFIG_TYPE".to_string(),
            Value::String("aws-secrets-manager".to_string()),
        );
        config.insert(
            "AWS_SECRETS_MANAGER_REGION".to_string(),
            Value::String("us-west-2".to_string()),
        );
        config.insert(
            "AWS_SECRETS_MANAGER_SECRET_ARN".to_string(),
            Value::String(
                "arn:aws:secretsmanager:us-west-2:123456789012:secret:unique-boo".to_string(),
            ),
        );

        let encoded = encode_config(&config).unwrap();
        let decoded = decode_config(&encoded).unwrap();

        match decoded.config_type {
            ConfigType::AwsSecretsManager { arn, region } => {
                assert_eq!(region, "us-west-2");
                assert!(arn.contains("unique-boo"));
            }
            _ => panic!("Expected AwsSecretsManager config type"),
        }
    }

    #[test]
    fn test_decode_python_generated_string() {
        // This string was generated by Python using the same algorithm:
        // json.dumps(config) -> zlib.compress() -> base64.b64encode()
        // Config: {
        //   "METAFLOW_SERVICE_AUTH_KEY": "test-key-12345",
        //   "OBP_API_SERVER": "api.test.outerbounds.xyz",
        //   "OBP_AUTH_SERVER": "auth.test.outerbounds.xyz"
        // }
        let python_encoded = "eJyrVvJ1DXF08/EPjw92DQrzdHaNdwwN8Yj3do1UslJQKkktLtHNTq3UNTQyNjFV0lFQ8ncKiHcM8ASrdg0CqUksyNQDqdPLLy1JLUrKL81LKdarqKyCqwaZh6S8tCQDu/paANrTLQM=";

        let decoded = decode_config(python_encoded).unwrap();

        assert_eq!(
            decoded.config.service_auth_key,
            Some("test-key-12345".to_string())
        );
        assert_eq!(
            decoded.config.obp_api_server,
            Some("api.test.outerbounds.xyz".to_string())
        );
        assert_eq!(
            decoded.config.obp_auth_server,
            Some("auth.test.outerbounds.xyz".to_string())
        );
    }
}
