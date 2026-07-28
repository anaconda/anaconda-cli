use std::collections::HashMap;
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, SystemTime};

use figment::Figment;
use figment::providers::{Format, Json, Serialized};
use serde::{Deserialize, Serialize};

use crate::context::CommandContext;
use crate::outerbounds_native::errors::ConfigError;

use super::types::{MetaflowConfig, ObConfig, ResolvedConfig};

/// Default TTL for cached remote configs (1 hour)
const CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// In-memory cache for remote metaflow configs, keyed by URL
static REMOTE_CONFIG_CACHE: LazyLock<Mutex<HashMap<String, MetaflowConfig>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Cached config with timestamp for TTL checking
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedConfig {
    config: MetaflowConfig,
    #[serde(with = "system_time_serde")]
    cached_at: SystemTime,
}

mod system_time_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    pub fn serialize<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let duration = time.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
        duration.as_secs().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(UNIX_EPOCH + Duration::from_secs(secs))
    }
}

impl CachedConfig {
    fn new(config: MetaflowConfig) -> Self {
        Self {
            config,
            cached_at: SystemTime::now(),
        }
    }

    fn is_expired(&self) -> bool {
        self.cached_at
            .elapsed()
            .map(|elapsed| elapsed > CACHE_TTL)
            .unwrap_or(true)
    }
}

/// Get the cache directory for remote configs
fn cache_dir(config_dir: &Path) -> PathBuf {
    config_dir.join(".cache")
}

/// Generate a cache filename from a URL
fn cache_filename(url: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut hasher);
    format!("remote_config_{:x}.json", hasher.finish())
}

/// Read cached config from disk if it exists and is not expired
fn read_disk_cache(config_dir: &Path, url: &str) -> Option<MetaflowConfig> {
    let cache_path = cache_dir(config_dir).join(cache_filename(url));

    let content = fs::read_to_string(&cache_path).ok()?;
    let cached: CachedConfig = serde_json::from_str(&content).ok()?;

    if cached.is_expired() {
        // Clean up expired cache file
        let _ = fs::remove_file(&cache_path);
        return None;
    }

    Some(cached.config)
}

/// Write config to disk cache
fn write_disk_cache(config_dir: &Path, url: &str, config: &MetaflowConfig) {
    let cache_path = cache_dir(config_dir).join(cache_filename(url));

    // Ensure cache directory exists
    if let Err(e) = fs::create_dir_all(cache_dir(config_dir)) {
        log::debug!("Failed to create cache directory: {}", e);
        return;
    }

    let cached = CachedConfig::new(config.clone());
    match serde_json::to_string_pretty(&cached) {
        Ok(content) => {
            if let Err(e) = fs::write(&cache_path, content) {
                log::debug!("Failed to write config cache: {}", e);
            }
        }
        Err(e) => {
            log::debug!("Failed to serialize config for cache: {}", e);
        }
    }
}

/// Get the default metaflow config directory path.
/// Respects METAFLOW_HOME env var, defaults to ~/.metaflowconfig
pub fn default_config_dir() -> PathBuf {
    env::var("METAFLOW_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".metaflowconfig")
        })
}

/// Get the default OB config directory path.
/// Respects OBP_CONFIG_DIR env var, falls back to metaflow config dir
pub fn default_ob_config_dir() -> PathBuf {
    env::var("OBP_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_config_dir())
}

/// Get the current profile from METAFLOW_PROFILE env var
pub fn current_profile() -> Option<String> {
    env::var("METAFLOW_PROFILE").ok().filter(|s| !s.is_empty())
}

/// Get the metaflow config file path for a given profile
pub fn metaflow_config_path(config_dir: &Path, profile: Option<&str>) -> PathBuf {
    let filename = match profile {
        Some(p) if !p.is_empty() => format!("config_{}.json", p),
        _ => "config.json".to_string(),
    };
    config_dir.join(filename)
}

/// Get the OB config file path for a given profile
pub fn ob_config_path(config_dir: &Path, profile: Option<&str>) -> PathBuf {
    let filename = match profile {
        Some(p) if !p.is_empty() => format!("ob_config_{}.json", p),
        _ => "ob_config.json".to_string(),
    };
    config_dir.join(filename)
}

/// Read metaflow config from filesystem.
pub fn read_metaflow_config(
    config_dir: &Path,
    profile: Option<&str>,
) -> Result<MetaflowConfig, ConfigError> {
    let path = metaflow_config_path(config_dir, profile);

    if !path.exists() {
        return Err(ConfigError::NotFound { path });
    }

    Figment::new()
        .merge(Serialized::defaults(MetaflowConfig::default()))
        .merge(Json::file(&path))
        .extract()
        .map_err(|e| ConfigError::ParseFailed {
            path,
            reason: e.to_string(),
        })
}

/// Read OB config from filesystem.
/// Returns Ok(None) if file doesn't exist (unless OBP_CONFIG_DIR is explicitly set).
pub fn read_ob_config(
    config_dir: &Path,
    profile: Option<&str>,
) -> Result<Option<ObConfig>, ConfigError> {
    let path = ob_config_path(config_dir, profile);

    if !path.exists() {
        if env::var("OBP_CONFIG_DIR").is_ok() {
            return Err(ConfigError::ObpConfigDirMissing {
                path: config_dir.to_path_buf(),
            });
        }
        return Ok(None);
    }

    let config: ObConfig = Figment::new()
        .merge(Json::file(&path))
        .extract()
        .map_err(|e| ConfigError::ParseFailed {
            path,
            reason: e.to_string(),
        })?;

    // Validate required key if file exists
    if config.config_url().is_none() {
        return Err(ConfigError::ObConfigMissingKey {
            key: "OB_CURRENT_PERIMETER_MF_CONFIG_URL".to_string(),
        });
    }

    Ok(Some(config))
}

/// Fetch remote config from URL using auth token.
async fn fetch_remote_config(
    ctx: &CommandContext,
    url: &str,
    auth_key: &str,
) -> Result<MetaflowConfig, ConfigError> {
    let response = ctx
        .unauthenticated_client(std::time::Duration::from_secs(30))
        .get(url)
        .header("x-api-key", auth_key)
        .send()
        .await
        .map_err(|e| ConfigError::RemoteFetchFailed {
            url: url.to_string(),
            source: e,
        })?;

    if !response.status().is_success() {
        return Err(ConfigError::RemoteFetchFailed {
            url: url.to_string(),
            source: reqwest_middleware::Error::from(response.error_for_status().unwrap_err()),
        });
    }

    #[derive(serde::Deserialize)]
    struct RemoteConfigResponse {
        config: MetaflowConfig,
    }

    let remote: RemoteConfigResponse =
        response
            .json()
            .await
            .map_err(|e| ConfigError::RemoteFetchFailed {
                url: url.to_string(),
                source: reqwest_middleware::Error::from(e),
            })?;

    Ok(remote.config)
}

/// Initialize config, fetching from remote URL if configured.
///
/// Resolution order:
/// 1. Read local metaflow config from filesystem
/// 2. Check ob_config.json for perimeter-specific URL
/// 3. If URL found (in ob_config or metaflow config), fetch remote config
/// 4. Cache remote config by URL
pub async fn init_config(
    ctx: &CommandContext,
    config_dir: &Path,
    profile: Option<&str>,
) -> Result<ResolvedConfig, ConfigError> {
    let local_config = read_metaflow_config(config_dir, profile)?;
    let ob_config = read_ob_config(config_dir, profile)?;

    // Determine if we need to fetch remote config
    let remote_url = ob_config
        .as_ref()
        .and_then(|ob| ob.config_url())
        .or(local_config.obp_metaflow_config_url.as_deref())
        .map(String::from);

    let (metaflow, source_url) =
        match remote_url {
            Some(url) => {
                // Check in-memory cache first
                {
                    let cache = REMOTE_CONFIG_CACHE.lock().unwrap();
                    if let Some(cached) = cache.get(&url) {
                        return Ok(ResolvedConfig {
                            metaflow: cached.clone(),
                            ob: ob_config,
                            profile: profile.map(String::from),
                            source_url: Some(url),
                        });
                    }
                }

                // Check disk cache (with TTL)
                if let Some(cached) = read_disk_cache(config_dir, &url) {
                    // Populate in-memory cache
                    {
                        let mut cache = REMOTE_CONFIG_CACHE.lock().unwrap();
                        cache.insert(url.clone(), cached.clone());
                    }
                    return Ok(ResolvedConfig {
                        metaflow: cached,
                        ob: ob_config,
                        profile: profile.map(String::from),
                        source_url: Some(url),
                    });
                }

                // Need auth key to fetch remote config
                let auth_key = local_config.service_auth_key.as_ref().ok_or_else(|| {
                    ConfigError::MissingKey {
                        key: "METAFLOW_SERVICE_AUTH_KEY".to_string(),
                    }
                })?;

                let mut remote_config = fetch_remote_config(ctx, &url, auth_key).await?;

                // Preserve the config URL in the resolved config
                remote_config.obp_metaflow_config_url = Some(url.clone());

                // Cache to disk
                write_disk_cache(config_dir, &url, &remote_config);

                // Cache in memory
                {
                    let mut cache = REMOTE_CONFIG_CACHE.lock().unwrap();
                    cache.insert(url.clone(), remote_config.clone());
                }

                (remote_config, Some(url))
            }
            None => (local_config, None),
        };

    Ok(ResolvedConfig {
        metaflow,
        ob: ob_config,
        profile: profile.map(String::from),
        source_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::config::Config;
    use crate::http::Client;

    fn setup_temp_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    fn test_config(keyring_path: PathBuf, domain: &str) -> Config {
        Config {
            domain: domain.to_string(),
            client_id: "test-client".to_string(),
            ssl_verify: true,
            open_browser: false,
            keyring_path,
            use_https: true,
            metrics_endpoint: "https://metrics.example.com".to_string(),
            metrics_public_endpoint: "https://public.metrics.example.com".to_string(),
            metrics_export_interval_ms: 1000,
            metrics_console_exporter: false,
            metrics_skip_internet_check: true,
            include_prereleases: false,
            pip_index_url: "https://example.com/simple".to_string(),
            self_update_url: Some("https://example.com".to_string()),
            auto_update_tools: None,
            #[cfg(feature = "diagnostics")]
            sentry_disabled: true,
            #[cfg(feature = "diagnostics")]
            sentry_environment: "test".to_string(),
        }
    }

    fn setup_test_context(mock_server: &MockServer) -> (CommandContext, TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let keyring_path = dir.path().join("keyring");
        let config = test_config(keyring_path, "test.example.com");

        let client = Client::new(reqwest::Client::builder(), mock_server.uri()).unwrap();
        let ctx = CommandContext::with_client(config, client);

        (ctx, dir)
    }

    #[test]
    fn test_metaflow_config_path_default_profile() {
        let dir = PathBuf::from("/mf");
        assert_eq!(
            metaflow_config_path(&dir, None),
            PathBuf::from("/mf/config.json")
        );
        assert_eq!(
            metaflow_config_path(&dir, Some("")),
            PathBuf::from("/mf/config.json")
        );
    }

    #[test]
    fn test_metaflow_config_path_named_profile() {
        let dir = PathBuf::from("/mf");
        assert_eq!(
            metaflow_config_path(&dir, Some("prod")),
            PathBuf::from("/mf/config_prod.json")
        );
    }

    #[test]
    fn test_ob_config_path_default_profile() {
        let dir = PathBuf::from("/mf");
        assert_eq!(
            ob_config_path(&dir, None),
            PathBuf::from("/mf/ob_config.json")
        );
    }

    #[test]
    fn test_ob_config_path_named_profile() {
        let dir = PathBuf::from("/mf");
        assert_eq!(
            ob_config_path(&dir, Some("staging")),
            PathBuf::from("/mf/ob_config_staging.json")
        );
    }

    #[test]
    fn test_read_metaflow_config_not_found() {
        let tmp = setup_temp_dir();
        let result = read_metaflow_config(tmp.path(), None);
        assert!(matches!(result, Err(ConfigError::NotFound { .. })));
    }

    #[test]
    fn test_read_metaflow_config_success() {
        let tmp = setup_temp_dir();
        let config_path = tmp.path().join("config.json");
        fs::write(
            &config_path,
            r#"{
                "METAFLOW_SERVICE_AUTH_KEY": "test-token",
                "OBP_API_SERVER": "api.example.com"
            }"#,
        )
        .unwrap();

        let config = read_metaflow_config(tmp.path(), None).unwrap();
        assert_eq!(config.service_auth_key, Some("test-token".to_string()));
        assert_eq!(config.obp_api_server, Some("api.example.com".to_string()));
    }

    #[test]
    fn test_read_metaflow_config_with_profile() {
        let tmp = setup_temp_dir();
        let config_path = tmp.path().join("config_prod.json");
        fs::write(
            &config_path,
            r#"{"METAFLOW_SERVICE_AUTH_KEY": "prod-token"}"#,
        )
        .unwrap();

        let config = read_metaflow_config(tmp.path(), Some("prod")).unwrap();
        assert_eq!(config.service_auth_key, Some("prod-token".to_string()));
    }

    #[test]
    fn test_read_metaflow_config_preserves_unknown_fields() {
        let tmp = setup_temp_dir();
        let config_path = tmp.path().join("config.json");
        fs::write(
            &config_path,
            r#"{
                "METAFLOW_SERVICE_AUTH_KEY": "token",
                "SOME_FUTURE_KEY": "future-value"
            }"#,
        )
        .unwrap();

        let config = read_metaflow_config(tmp.path(), None).unwrap();
        assert_eq!(
            config.extra.get("SOME_FUTURE_KEY").and_then(|v| v.as_str()),
            Some("future-value")
        );
    }

    #[test]
    fn test_read_ob_config_not_found_ok() {
        let tmp = setup_temp_dir();
        let result = read_ob_config(tmp.path(), None).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_read_ob_config_success() {
        let tmp = setup_temp_dir();
        let config_path = tmp.path().join("ob_config.json");
        fs::write(
            &config_path,
            r#"{
                "OB_CURRENT_PERIMETER": "prod",
                "OB_CURRENT_PERIMETER_MF_CONFIG_URL": "https://example.com/config"
            }"#,
        )
        .unwrap();

        let config = read_ob_config(tmp.path(), None).unwrap().unwrap();
        assert_eq!(config.current_perimeter, Some("prod".to_string()));
        assert_eq!(config.config_url(), Some("https://example.com/config"));
    }

    #[test]
    fn test_read_ob_config_legacy_url_key() {
        let tmp = setup_temp_dir();
        let config_path = tmp.path().join("ob_config.json");
        fs::write(
            &config_path,
            r#"{
                "OB_CURRENT_PERIMETER": "prod",
                "OB_CURRENT_PERIMETER_URL": "https://legacy.example.com/config"
            }"#,
        )
        .unwrap();

        let config = read_ob_config(tmp.path(), None).unwrap().unwrap();
        assert_eq!(
            config.config_url(),
            Some("https://legacy.example.com/config")
        );
    }

    #[tokio::test]
    async fn test_fetch_remote_config_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/config"))
            .and(header("x-api-key", "test-auth-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "config": {
                    "METAFLOW_SERVICE_AUTH_KEY": "remote-token",
                    "OBP_API_SERVER": "remote-api.example.com"
                }
            })))
            .mount(&mock_server)
            .await;

        let (ctx, _dir) = setup_test_context(&mock_server);
        let url = format!("{}/config", mock_server.uri());

        let config = fetch_remote_config(&ctx, &url, "test-auth-key")
            .await
            .unwrap();

        assert_eq!(config.service_auth_key, Some("remote-token".to_string()));
        assert_eq!(
            config.obp_api_server,
            Some("remote-api.example.com".to_string())
        );
    }

    #[tokio::test]
    async fn test_fetch_remote_config_unauthorized() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/config"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&mock_server)
            .await;

        let (ctx, _dir) = setup_test_context(&mock_server);
        let url = format!("{}/config", mock_server.uri());

        let result = fetch_remote_config(&ctx, &url, "bad-key").await;

        assert!(matches!(result, Err(ConfigError::RemoteFetchFailed { .. })));
    }

    #[tokio::test]
    async fn test_init_config_local_only() {
        let mock_server = MockServer::start().await;
        let (ctx, dir) = setup_test_context(&mock_server);

        let config_path = dir.path().join("config.json");
        fs::write(
            &config_path,
            r#"{"METAFLOW_SERVICE_AUTH_KEY": "local-token"}"#,
        )
        .unwrap();

        let resolved = init_config(&ctx, dir.path(), None).await.unwrap();

        assert_eq!(
            resolved.metaflow.service_auth_key,
            Some("local-token".to_string())
        );
        assert!(resolved.ob.is_none());
        assert!(resolved.source_url.is_none());
    }

    #[tokio::test]
    async fn test_init_config_fetches_from_ob_config_url() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/remote-config"))
            .and(header("x-api-key", "local-auth"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "config": {
                    "METAFLOW_SERVICE_AUTH_KEY": "remote-token",
                    "OBP_API_SERVER": "remote-api.example.com"
                }
            })))
            .mount(&mock_server)
            .await;

        let (ctx, dir) = setup_test_context(&mock_server);

        let metaflow_path = dir.path().join("config.json");
        fs::write(
            &metaflow_path,
            r#"{"METAFLOW_SERVICE_AUTH_KEY": "local-auth"}"#,
        )
        .unwrap();

        let ob_path = dir.path().join("ob_config.json");
        let remote_url = format!("{}/remote-config", mock_server.uri());
        fs::write(
            &ob_path,
            serde_json::json!({
                "OB_CURRENT_PERIMETER": "prod",
                "OB_CURRENT_PERIMETER_MF_CONFIG_URL": remote_url
            })
            .to_string(),
        )
        .unwrap();

        let resolved = init_config(&ctx, dir.path(), None).await.unwrap();

        assert_eq!(
            resolved.metaflow.service_auth_key,
            Some("remote-token".to_string())
        );
        assert_eq!(
            resolved.metaflow.obp_api_server,
            Some("remote-api.example.com".to_string())
        );
        assert!(resolved.ob.is_some());
        assert!(resolved.source_url.is_some());
    }

    #[tokio::test]
    async fn test_init_config_uses_cache() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/cached-config"))
            .and(header("x-api-key", "local-auth"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "config": {
                    "METAFLOW_SERVICE_AUTH_KEY": "cached-token"
                }
            })))
            .expect(1) // Should only be called once
            .mount(&mock_server)
            .await;

        let (ctx, dir) = setup_test_context(&mock_server);

        let metaflow_path = dir.path().join("config.json");
        fs::write(
            &metaflow_path,
            r#"{"METAFLOW_SERVICE_AUTH_KEY": "local-auth"}"#,
        )
        .unwrap();

        let ob_path = dir.path().join("ob_config.json");
        let remote_url = format!("{}/cached-config", mock_server.uri());
        fs::write(
            &ob_path,
            serde_json::json!({
                "OB_CURRENT_PERIMETER": "prod",
                "OB_CURRENT_PERIMETER_MF_CONFIG_URL": remote_url
            })
            .to_string(),
        )
        .unwrap();

        // First call - hits the server
        let resolved1 = init_config(&ctx, dir.path(), None).await.unwrap();
        assert_eq!(
            resolved1.metaflow.service_auth_key,
            Some("cached-token".to_string())
        );

        // Second call - should use cache (mock expects only 1 call)
        let resolved2 = init_config(&ctx, dir.path(), None).await.unwrap();
        assert_eq!(
            resolved2.metaflow.service_auth_key,
            Some("cached-token".to_string())
        );
    }

    #[tokio::test]
    async fn test_init_config_missing_auth_key() {
        let mock_server = MockServer::start().await;
        let (ctx, dir) = setup_test_context(&mock_server);

        // Local config without auth key
        let metaflow_path = dir.path().join("config.json");
        fs::write(&metaflow_path, r#"{"OBP_API_SERVER": "api.example.com"}"#).unwrap();

        // OB config with remote URL - will fail because no auth key
        let ob_path = dir.path().join("ob_config.json");
        fs::write(
            &ob_path,
            r#"{
                "OB_CURRENT_PERIMETER": "prod",
                "OB_CURRENT_PERIMETER_MF_CONFIG_URL": "https://example.com/config"
            }"#,
        )
        .unwrap();

        let result = init_config(&ctx, dir.path(), None).await;

        assert!(matches!(result, Err(ConfigError::MissingKey { .. })));
    }

    #[tokio::test]
    async fn test_resolved_config_fields() {
        let mock_server = MockServer::start().await;
        let (ctx, dir) = setup_test_context(&mock_server);

        let config_path = dir.path().join("config_staging.json");
        fs::write(
            &config_path,
            r#"{"METAFLOW_SERVICE_AUTH_KEY": "staging-token"}"#,
        )
        .unwrap();

        let resolved = init_config(&ctx, dir.path(), Some("staging"))
            .await
            .unwrap();

        assert_eq!(resolved.profile, Some("staging".to_string()));
        assert_eq!(
            resolved.metaflow.service_auth_key,
            Some("staging-token".to_string())
        );
    }

    #[test]
    fn test_default_config_dir_returns_path() {
        let dir = default_config_dir();
        // Should end with .metaflowconfig (unless METAFLOW_HOME is set)
        let dir_str = dir.to_string_lossy();
        assert!(
            dir_str.ends_with(".metaflowconfig") || env::var("METAFLOW_HOME").is_ok(),
            "Expected path ending in .metaflowconfig, got: {}",
            dir_str
        );
    }

    #[test]
    fn test_default_ob_config_dir_returns_path() {
        let dir = default_ob_config_dir();
        // Should be a valid path
        assert!(!dir.to_string_lossy().is_empty());
        // If OBP_CONFIG_DIR not set, should match default_config_dir
        if env::var("OBP_CONFIG_DIR").is_err() {
            assert_eq!(dir, default_config_dir());
        }
    }

    #[test]
    fn test_current_profile_returns_option() {
        let profile = current_profile();
        // Should return None if METAFLOW_PROFILE not set, or Some if set
        if let Ok(val) = env::var("METAFLOW_PROFILE") {
            if val.is_empty() {
                assert!(profile.is_none());
            } else {
                assert_eq!(profile, Some(val));
            }
        } else {
            assert!(profile.is_none());
        }
    }

    #[test]
    fn test_cache_filename_deterministic() {
        let url = "https://example.com/config";
        let filename1 = cache_filename(url);
        let filename2 = cache_filename(url);
        assert_eq!(filename1, filename2);
        assert!(filename1.starts_with("remote_config_"));
        assert!(filename1.ends_with(".json"));
    }

    #[test]
    fn test_cache_filename_different_urls() {
        let filename1 = cache_filename("https://example.com/config1");
        let filename2 = cache_filename("https://example.com/config2");
        assert_ne!(filename1, filename2);
    }

    #[test]
    fn test_disk_cache_roundtrip() {
        let tmp = setup_temp_dir();
        let url = "https://example.com/test-config";
        let config = MetaflowConfig {
            service_auth_key: Some("test-key".into()),
            obp_api_server: Some("api.example.com".into()),
            ..Default::default()
        };

        // Write to cache
        write_disk_cache(tmp.path(), url, &config);

        // Read from cache
        let cached = read_disk_cache(tmp.path(), url);
        assert!(cached.is_some());

        let cached = cached.unwrap();
        assert_eq!(cached.service_auth_key, Some("test-key".into()));
        assert_eq!(cached.obp_api_server, Some("api.example.com".into()));
    }

    #[test]
    fn test_disk_cache_missing_returns_none() {
        let tmp = setup_temp_dir();
        let cached = read_disk_cache(tmp.path(), "https://nonexistent.com/config");
        assert!(cached.is_none());
    }

    #[test]
    fn test_cached_config_not_expired() {
        let config = MetaflowConfig::default();
        let cached = CachedConfig::new(config);
        assert!(!cached.is_expired());
    }

    #[test]
    fn test_cached_config_expired() {
        let config = MetaflowConfig::default();
        let cached = CachedConfig {
            config,
            cached_at: SystemTime::now() - Duration::from_secs(2 * 60 * 60), // 2 hours ago
        };
        assert!(cached.is_expired());
    }
}
