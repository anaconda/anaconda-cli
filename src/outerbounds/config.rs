//! Metaflow/Outerbounds configuration loading.
//!
//! Loads configuration from `~/.metaflowconfig/` (or `$METAFLOW_HOME`):
//! - `config.json` - Main config with API keys and URLs
//! - `config_{profile}.json` - Profile-specific config
//! - `ob_config.json` - Current perimeter selection
//!
//! Environment variables:
//! - `METAFLOW_HOME` - Override the config directory (default: `~/.metaflowconfig`)
//! - `OBP_CONFIG_DIR` - Override the OB config directory (for `ob_config.json`)
//! - `METAFLOW_PROFILE` - Select a named profile

use std::fs;
use std::path::PathBuf;

use miette::miette;
use serde::Deserialize;

/// Returns the metaflow config directory path.
/// Uses `METAFLOW_HOME` if set, otherwise `~/.metaflowconfig`.
pub fn metaflow_home() -> Option<PathBuf> {
    std::env::var("METAFLOW_HOME")
        .map(PathBuf::from)
        .ok()
        .or_else(|| dirs::home_dir().map(|h| h.join(".metaflowconfig")))
}

/// Returns the OB config directory path.
/// Uses `OBP_CONFIG_DIR` if set, otherwise falls back to `metaflow_home()`.
pub fn ob_config_dir() -> Option<PathBuf> {
    std::env::var("OBP_CONFIG_DIR")
        .map(|p| {
            let path = PathBuf::from(&p);
            if p.starts_with("~/") {
                if let Some(home) = dirs::home_dir() {
                    return home.join(p.strip_prefix("~/").unwrap());
                }
            }
            path
        })
        .ok()
        .or_else(metaflow_home)
}

/// Returns the current profile from `METAFLOW_PROFILE` environment variable.
pub fn current_profile() -> Option<String> {
    std::env::var("METAFLOW_PROFILE").ok().filter(|s| !s.is_empty())
}

/// Main metaflow config from `config.json`.
///
/// Only includes fields actually used by the CLI. The config file may contain
/// many more fields which are ignored (serde default behavior).
#[derive(Debug, Default, Deserialize)]
pub struct MetaflowConfig {
    /// API key for authenticating with Outerbounds services.
    #[serde(rename = "METAFLOW_SERVICE_AUTH_KEY")]
    pub api_key: Option<String>,

    /// Outerbounds Platform API server URL.
    #[serde(rename = "OBP_API_SERVER")]
    pub api_server: Option<String>,
}

impl MetaflowConfig {
    /// Load config from the default location, returning None if not found.
    /// Respects `METAFLOW_PROFILE` for profile-specific configs.
    pub fn load() -> Option<Self> {
        Self::load_with_profile(current_profile().as_deref())
    }

    /// Load config for a specific profile.
    pub fn load_with_profile(profile: Option<&str>) -> Option<Self> {
        let home = metaflow_home()?;
        let filename = match profile {
            Some(p) if !p.is_empty() => format!("config_{}.json", p),
            _ => "config.json".to_string(),
        };
        let path = home.join(filename);
        let contents = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&contents).ok()
    }

    /// Load config with detailed error messages.
    /// Respects `METAFLOW_PROFILE` for profile-specific configs.
    pub fn load_strict() -> miette::Result<Self> {
        Self::load_strict_with_profile(current_profile().as_deref())
    }

    /// Load config for a specific profile with detailed error messages.
    pub fn load_strict_with_profile(profile: Option<&str>) -> miette::Result<Self> {
        let home = metaflow_home().ok_or_else(|| miette!("Could not determine home directory"))?;
        let filename = match profile {
            Some(p) if !p.is_empty() => format!("config_{}.json", p),
            _ => "config.json".to_string(),
        };
        let path = home.join(&filename);

        if !path.exists() {
            return Err(miette!("Config file not found: {}", path.display()));
        }

        let contents =
            fs::read_to_string(&path).map_err(|e| miette!("Failed to read config: {}", e))?;

        if contents.trim().is_empty() {
            return Err(miette!("Config file is empty: {}", path.display()));
        }

        serde_json::from_str(&contents).map_err(|e| miette!("Failed to parse config: {}", e))
    }
}

/// Outerbounds perimeter config from `ob_config.json`.
#[derive(Debug, Default, Deserialize)]
pub struct ObPerimeterConfig {
    #[serde(rename = "OB_CURRENT_PERIMETER")]
    pub current_perimeter: Option<String>,

    #[serde(rename = "OB_CURRENT_PERIMETER_MF_CONFIG_URL")]
    pub config_url: Option<String>,

    /// Legacy key for backwards compatibility with old workstations.
    #[serde(rename = "OB_CURRENT_PERIMETER_URL")]
    pub config_url_legacy: Option<String>,
}

impl ObPerimeterConfig {
    /// Load perimeter config from the default location.
    /// Uses `OBP_CONFIG_DIR` if set, otherwise `METAFLOW_HOME`, otherwise `~/.metaflowconfig`.
    /// Respects `METAFLOW_PROFILE` for profile-specific configs.
    pub fn load() -> Option<Self> {
        Self::load_with_profile(current_profile().as_deref())
    }

    /// Load perimeter config for a specific profile.
    pub fn load_with_profile(profile: Option<&str>) -> Option<Self> {
        let dir = ob_config_dir()?;
        let filename = match profile {
            Some(p) if !p.is_empty() => format!("ob_config_{}.json", p),
            _ => "ob_config.json".to_string(),
        };
        let path = dir.join(filename);
        let contents = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&contents).ok()
    }

    /// Get the config URL, checking both the current and legacy keys.
    pub fn get_config_url(&self) -> Option<&str> {
        self.config_url
            .as_deref()
            .or(self.config_url_legacy.as_deref())
    }

    /// Extract the platform domain from the config URL.
    /// e.g., `https://api.example.obp.outerbounds.com/v1/...` -> `example.obp.outerbounds.com`
    pub fn platform_domain(&self) -> Option<String> {
        let url = self.get_config_url()?;
        let url = url.strip_prefix("https://api.")?;
        let domain = url.split('/').next()?;
        Some(domain.to_string())
    }
}

/// Combined Outerbounds configuration loaded from all sources.
#[derive(Debug, Clone)]
pub struct OuterboundsConfig {
    pub api_key: String,
    pub api_server: String,
    pub perimeter: String,
}

impl OuterboundsConfig {
    /// Load configuration from all sources, returning an error if required fields are missing.
    /// Respects `METAFLOW_PROFILE` for profile-specific configs.
    pub fn load() -> miette::Result<Self> {
        Self::load_with_profile(current_profile().as_deref())
    }

    /// Load configuration for a specific profile.
    pub fn load_with_profile(profile: Option<&str>) -> miette::Result<Self> {
        let mf_config = MetaflowConfig::load_with_profile(profile).ok_or_else(|| {
            match profile {
                Some(p) if !p.is_empty() => miette!(
                    "Profile config not found: config_{}.json. Run `outerbounds configure` first.",
                    p
                ),
                _ => miette!("Metaflow config not found. Run `outerbounds configure` first."),
            }
        })?;

        let ob_config = ObPerimeterConfig::load_with_profile(profile);

        let api_key = mf_config
            .api_key
            .ok_or_else(|| miette!("Missing METAFLOW_SERVICE_AUTH_KEY in config"))?;

        let api_server = mf_config
            .api_server
            .ok_or_else(|| miette!("Missing OBP_API_SERVER in config"))?;

        let perimeter = ob_config
            .as_ref()
            .and_then(|c| c.current_perimeter.clone())
            .unwrap_or_else(|| "default".to_string());

        Ok(Self {
            api_key,
            api_server: sanitize_url(&api_server),
            perimeter,
        })
    }

    /// Get the API base URL for the current perimeter.
    pub fn api_url(&self, path: &str) -> String {
        format!(
            "{}/v1/perimeters/{}/{}",
            self.api_server.trim_end_matches('/'),
            self.perimeter,
            path.trim_start_matches('/')
        )
    }

    /// Get the API base URL (without perimeter path) for endpoints like `/v1/me/perimeters`.
    pub fn api_base_url(&self) -> &str {
        &self.api_server
    }
}

/// Sanitize a URL by ensuring it has `https://` prefix and no trailing slash.
/// Matches the Python implementation's `get_sanitized_url_from_config`.
pub fn sanitize_url(url: &str) -> String {
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
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    /// Run a test with clean environment variables.
    fn with_clean_env<F, R>(home: &Path, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let home_str = home.to_str().unwrap();
        temp_env::with_vars(
            [
                ("HOME", Some(home_str)),
                ("METAFLOW_HOME", None::<&str>),
                ("OBP_CONFIG_DIR", None::<&str>),
                ("METAFLOW_PROFILE", None::<&str>),
            ],
            f,
        )
    }

    /// Create a metaflow config file.
    fn create_config(dir: &Path, api_key: &str, api_server: &str) {
        let config_dir = dir.join(".metaflowconfig");
        fs::create_dir_all(&config_dir).unwrap();
        create_config_in_dir(&config_dir, None, api_key, api_server);
    }

    /// Create a config file in a specific directory.
    fn create_config_in_dir(dir: &Path, profile: Option<&str>, api_key: &str, api_server: &str) {
        fs::create_dir_all(dir).unwrap();
        let filename = match profile {
            Some(p) => format!("config_{}.json", p),
            None => "config.json".to_string(),
        };
        let config = serde_json::json!({
            "METAFLOW_SERVICE_AUTH_KEY": api_key,
            "OBP_API_SERVER": api_server,
            "METAFLOW_SERVICE_URL": "https://service.example.com"
        });
        fs::write(dir.join(filename), config.to_string()).unwrap();
    }

    /// Create an ob_config file.
    fn create_ob_config(dir: &Path, perimeter: &str, config_url: &str) {
        create_ob_config_with_profile(dir, None, perimeter, config_url, false);
    }

    /// Create an ob_config file with profile support and legacy key option.
    fn create_ob_config_with_profile(
        dir: &Path,
        profile: Option<&str>,
        perimeter: &str,
        config_url: &str,
        use_legacy_key: bool,
    ) {
        let config_dir = dir.join(".metaflowconfig");
        fs::create_dir_all(&config_dir).unwrap();
        let filename = match profile {
            Some(p) => format!("ob_config_{}.json", p),
            None => "ob_config.json".to_string(),
        };
        let url_key = if use_legacy_key {
            "OB_CURRENT_PERIMETER_URL"
        } else {
            "OB_CURRENT_PERIMETER_MF_CONFIG_URL"
        };
        let config = serde_json::json!({
            "OB_CURRENT_PERIMETER": perimeter,
            url_key: config_url
        });
        fs::write(config_dir.join(filename), config.to_string()).unwrap();
    }

    // =========================================================================
    // metaflow_home() tests
    // =========================================================================

    #[test]
    fn test_metaflow_home_default() {
        let tmp = TempDir::new().unwrap();
        with_clean_env(tmp.path(), || {
            let home = metaflow_home().unwrap();
            assert_eq!(home, tmp.path().join(".metaflowconfig"));
        });
    }

    #[test]
    fn test_metaflow_home_env_override() {
        let tmp = TempDir::new().unwrap();
        let custom_path = tmp.path().join("custom_config");
        fs::create_dir_all(&custom_path).unwrap();

        temp_env::with_vars(
            [
                ("HOME", Some(tmp.path().to_str().unwrap())),
                ("METAFLOW_HOME", Some(custom_path.to_str().unwrap())),
            ],
            || {
                let home = metaflow_home().unwrap();
                assert_eq!(home, custom_path);
            },
        );
    }

    // =========================================================================
    // ob_config_dir() tests
    // =========================================================================

    #[test]
    fn test_ob_config_dir_default() {
        let tmp = TempDir::new().unwrap();
        with_clean_env(tmp.path(), || {
            let dir = ob_config_dir().unwrap();
            assert_eq!(dir, tmp.path().join(".metaflowconfig"));
        });
    }

    #[test]
    fn test_ob_config_dir_obp_config_dir_override() {
        let tmp = TempDir::new().unwrap();
        let custom_path = tmp.path().join("obp_config");
        fs::create_dir_all(&custom_path).unwrap();

        temp_env::with_vars(
            [
                ("HOME", Some(tmp.path().to_str().unwrap())),
                ("METAFLOW_HOME", None::<&str>),
                ("OBP_CONFIG_DIR", Some(custom_path.to_str().unwrap())),
            ],
            || {
                let dir = ob_config_dir().unwrap();
                assert_eq!(dir, custom_path);
            },
        );
    }

    #[test]
    fn test_ob_config_dir_prefers_obp_over_metaflow() {
        let tmp = TempDir::new().unwrap();
        let mf_path = tmp.path().join("metaflow_home");
        let obp_path = tmp.path().join("obp_config");
        fs::create_dir_all(&mf_path).unwrap();
        fs::create_dir_all(&obp_path).unwrap();

        temp_env::with_vars(
            [
                ("HOME", Some(tmp.path().to_str().unwrap())),
                ("METAFLOW_HOME", Some(mf_path.to_str().unwrap())),
                ("OBP_CONFIG_DIR", Some(obp_path.to_str().unwrap())),
            ],
            || {
                let dir = ob_config_dir().unwrap();
                assert_eq!(dir, obp_path, "OBP_CONFIG_DIR should take precedence");
            },
        );
    }

    // =========================================================================
    // current_profile() tests
    // =========================================================================

    #[test]
    fn test_current_profile_none() {
        temp_env::with_var("METAFLOW_PROFILE", None::<&str>, || {
            assert!(current_profile().is_none());
        });
    }

    #[test]
    fn test_current_profile_empty_string() {
        temp_env::with_var("METAFLOW_PROFILE", Some(""), || {
            assert!(current_profile().is_none());
        });
    }

    #[test]
    fn test_current_profile_set() {
        temp_env::with_var("METAFLOW_PROFILE", Some("production"), || {
            assert_eq!(current_profile(), Some("production".to_string()));
        });
    }

    // =========================================================================
    // MetaflowConfig tests
    // =========================================================================

    #[test]
    fn test_metaflow_config_load_basic() {
        let tmp = TempDir::new().unwrap();
        create_config(tmp.path(), "test-key", "https://api.example.com");

        with_clean_env(tmp.path(), || {
            let config = MetaflowConfig::load().unwrap();
            assert_eq!(config.api_key, Some("test-key".to_string()));
            assert_eq!(
                config.api_server,
                Some("https://api.example.com".to_string())
            );
        });
    }

    #[test]
    fn test_metaflow_config_load_missing() {
        let tmp = TempDir::new().unwrap();

        with_clean_env(tmp.path(), || {
            let config = MetaflowConfig::load();
            assert!(config.is_none());
        });
    }

    #[test]
    fn test_metaflow_config_load_with_profile() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().join(".metaflowconfig");
        create_config_in_dir(&config_dir, Some("staging"), "staging-key", "https://api.staging.com");

        with_clean_env(tmp.path(), || {
            let config = MetaflowConfig::load_with_profile(Some("staging")).unwrap();
            assert_eq!(config.api_key, Some("staging-key".to_string()));
            assert_eq!(
                config.api_server,
                Some("https://api.staging.com".to_string())
            );
        });
    }

    #[test]
    fn test_metaflow_config_load_respects_env_profile() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().join(".metaflowconfig");
        create_config_in_dir(&config_dir, None, "default-key", "https://api.default.com");
        create_config_in_dir(&config_dir, Some("prod"), "prod-key", "https://api.prod.com");

        temp_env::with_vars(
            [
                ("HOME", Some(tmp.path().to_str().unwrap())),
                ("METAFLOW_HOME", None::<&str>),
                ("OBP_CONFIG_DIR", None::<&str>),
                ("METAFLOW_PROFILE", Some("prod")),
            ],
            || {
                let config = MetaflowConfig::load().unwrap();
                assert_eq!(config.api_key, Some("prod-key".to_string()));
            },
        );
    }

    #[test]
    fn test_metaflow_config_load_strict_missing_file() {
        let tmp = TempDir::new().unwrap();

        with_clean_env(tmp.path(), || {
            let result = MetaflowConfig::load_strict();
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(err.contains("not found"), "Error: {}", err);
        });
    }

    #[test]
    fn test_metaflow_config_load_strict_empty_file() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().join(".metaflowconfig");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(config_dir.join("config.json"), "").unwrap();

        with_clean_env(tmp.path(), || {
            let result = MetaflowConfig::load_strict();
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(err.contains("empty"), "Error: {}", err);
        });
    }

    #[test]
    fn test_metaflow_config_load_strict_invalid_json() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().join(".metaflowconfig");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(config_dir.join("config.json"), "{ invalid json }").unwrap();

        with_clean_env(tmp.path(), || {
            let result = MetaflowConfig::load_strict();
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(err.contains("parse"), "Error: {}", err);
        });
    }

    #[test]
    fn test_metaflow_config_load_strict_whitespace_only() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().join(".metaflowconfig");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(config_dir.join("config.json"), "   \n\t  ").unwrap();

        with_clean_env(tmp.path(), || {
            let result = MetaflowConfig::load_strict();
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(err.contains("empty"), "Error: {}", err);
        });
    }

    #[test]
    fn test_metaflow_config_partial_fields() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().join(".metaflowconfig");
        fs::create_dir_all(&config_dir).unwrap();

        let config = serde_json::json!({
            "METAFLOW_SERVICE_AUTH_KEY": "only-key"
        });
        fs::write(config_dir.join("config.json"), config.to_string()).unwrap();

        with_clean_env(tmp.path(), || {
            let config = MetaflowConfig::load().unwrap();
            assert_eq!(config.api_key, Some("only-key".to_string()));
            assert_eq!(config.api_server, None);
        });
    }

    // =========================================================================
    // ObPerimeterConfig tests
    // =========================================================================

    #[test]
    fn test_ob_perimeter_config_load_basic() {
        let tmp = TempDir::new().unwrap();
        create_ob_config(
            tmp.path(),
            "production",
            "https://api.prod.obp.outerbounds.com/v1/perimeters/production/metaflowconfigs/default",
        );

        with_clean_env(tmp.path(), || {
            let config = ObPerimeterConfig::load().unwrap();
            assert_eq!(config.current_perimeter, Some("production".to_string()));
            assert_eq!(
                config.get_config_url(),
                Some("https://api.prod.obp.outerbounds.com/v1/perimeters/production/metaflowconfigs/default")
            );
        });
    }

    #[test]
    fn test_ob_perimeter_config_load_missing() {
        let tmp = TempDir::new().unwrap();

        with_clean_env(tmp.path(), || {
            let config = ObPerimeterConfig::load();
            assert!(config.is_none());
        });
    }

    #[test]
    fn test_ob_perimeter_config_legacy_key() {
        let tmp = TempDir::new().unwrap();
        create_ob_config_with_profile(
            tmp.path(),
            None,
            "legacy-perimeter",
            "https://api.legacy.com/v1/config",
            true, // use legacy key
        );

        with_clean_env(tmp.path(), || {
            let config = ObPerimeterConfig::load().unwrap();
            assert_eq!(
                config.current_perimeter,
                Some("legacy-perimeter".to_string())
            );
            assert_eq!(
                config.get_config_url(),
                Some("https://api.legacy.com/v1/config")
            );
            assert!(config.config_url.is_none(), "New key should be None");
            assert!(
                config.config_url_legacy.is_some(),
                "Legacy key should be Some"
            );
        });
    }

    #[test]
    fn test_ob_perimeter_config_prefers_new_key() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().join(".metaflowconfig");
        fs::create_dir_all(&config_dir).unwrap();

        let config = serde_json::json!({
            "OB_CURRENT_PERIMETER": "test",
            "OB_CURRENT_PERIMETER_MF_CONFIG_URL": "https://new.url",
            "OB_CURRENT_PERIMETER_URL": "https://legacy.url"
        });
        fs::write(config_dir.join("ob_config.json"), config.to_string()).unwrap();

        with_clean_env(tmp.path(), || {
            let config = ObPerimeterConfig::load().unwrap();
            assert_eq!(config.get_config_url(), Some("https://new.url"));
        });
    }

    #[test]
    fn test_ob_perimeter_config_with_profile() {
        let tmp = TempDir::new().unwrap();
        create_ob_config_with_profile(
            tmp.path(),
            Some("staging"),
            "staging-perimeter",
            "https://api.staging.com/config",
            false,
        );

        with_clean_env(tmp.path(), || {
            let config = ObPerimeterConfig::load_with_profile(Some("staging")).unwrap();
            assert_eq!(
                config.current_perimeter,
                Some("staging-perimeter".to_string())
            );
        });
    }

    #[test]
    fn test_ob_perimeter_config_respects_obp_config_dir() {
        let tmp = TempDir::new().unwrap();
        let obp_dir = tmp.path().join("obp_special");
        fs::create_dir_all(&obp_dir).unwrap();

        let config = serde_json::json!({
            "OB_CURRENT_PERIMETER": "special-perimeter",
            "OB_CURRENT_PERIMETER_MF_CONFIG_URL": "https://special.url"
        });
        fs::write(obp_dir.join("ob_config.json"), config.to_string()).unwrap();

        temp_env::with_vars(
            [
                ("HOME", Some(tmp.path().to_str().unwrap())),
                ("METAFLOW_HOME", None::<&str>),
                ("OBP_CONFIG_DIR", Some(obp_dir.to_str().unwrap())),
                ("METAFLOW_PROFILE", None::<&str>),
            ],
            || {
                let config = ObPerimeterConfig::load().unwrap();
                assert_eq!(
                    config.current_perimeter,
                    Some("special-perimeter".to_string())
                );
            },
        );
    }

    #[test]
    fn test_platform_domain_extraction() {
        let tmp = TempDir::new().unwrap();
        create_ob_config(
            tmp.path(),
            "prod",
            "https://api.merced.obp.outerbounds.com/v1/perimeters/default/metaflowconfigs/default",
        );

        with_clean_env(tmp.path(), || {
            let config = ObPerimeterConfig::load().unwrap();
            assert_eq!(
                config.platform_domain(),
                Some("merced.obp.outerbounds.com".to_string())
            );
        });
    }

    #[test]
    fn test_platform_domain_invalid_url() {
        let config = ObPerimeterConfig {
            current_perimeter: Some("test".to_string()),
            config_url: Some("invalid-url".to_string()),
            config_url_legacy: None,
        };
        assert!(config.platform_domain().is_none());
    }

    // =========================================================================
    // OuterboundsConfig tests
    // =========================================================================

    #[test]
    fn test_outerbounds_config_load_basic() {
        let tmp = TempDir::new().unwrap();
        create_config(tmp.path(), "my-api-key", "https://api.example.com");
        create_ob_config(tmp.path(), "staging", "https://api.staging.com/v1/...");

        with_clean_env(tmp.path(), || {
            let config = OuterboundsConfig::load().unwrap();
            assert_eq!(config.api_key, "my-api-key");
            assert_eq!(config.api_server, "https://api.example.com");
            assert_eq!(config.perimeter, "staging");
        });
    }

    #[test]
    fn test_outerbounds_config_default_perimeter() {
        let tmp = TempDir::new().unwrap();
        create_config(tmp.path(), "my-api-key", "https://api.example.com");

        with_clean_env(tmp.path(), || {
            let config = OuterboundsConfig::load().unwrap();
            assert_eq!(config.perimeter, "default");
        });
    }

    #[test]
    fn test_outerbounds_config_missing_api_key() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().join(".metaflowconfig");
        fs::create_dir_all(&config_dir).unwrap();

        let config = serde_json::json!({
            "OBP_API_SERVER": "https://api.example.com"
        });
        fs::write(config_dir.join("config.json"), config.to_string()).unwrap();

        with_clean_env(tmp.path(), || {
            let result = OuterboundsConfig::load();
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains("METAFLOW_SERVICE_AUTH_KEY"),
                "Error: {}",
                err
            );
        });
    }

    #[test]
    fn test_outerbounds_config_missing_api_server() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().join(".metaflowconfig");
        fs::create_dir_all(&config_dir).unwrap();

        let config = serde_json::json!({
            "METAFLOW_SERVICE_AUTH_KEY": "my-key"
        });
        fs::write(config_dir.join("config.json"), config.to_string()).unwrap();

        with_clean_env(tmp.path(), || {
            let result = OuterboundsConfig::load();
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(err.contains("OBP_API_SERVER"), "Error: {}", err);
        });
    }

    #[test]
    fn test_outerbounds_config_with_profile() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().join(".metaflowconfig");
        create_config_in_dir(&config_dir, Some("prod"), "prod-key", "https://api.prod.com");
        create_ob_config_with_profile(
            tmp.path(),
            Some("prod"),
            "prod-perimeter",
            "https://url",
            false,
        );

        with_clean_env(tmp.path(), || {
            let config = OuterboundsConfig::load_with_profile(Some("prod")).unwrap();
            assert_eq!(config.api_key, "prod-key");
            assert_eq!(config.api_server, "https://api.prod.com");
            assert_eq!(config.perimeter, "prod-perimeter");
        });
    }

    // =========================================================================
    // URL sanitization tests
    // =========================================================================

    #[test]
    fn test_sanitize_url_with_https() {
        assert_eq!(
            sanitize_url("https://api.example.com"),
            "https://api.example.com"
        );
    }

    #[test]
    fn test_sanitize_url_with_http() {
        assert_eq!(
            sanitize_url("http://api.example.com"),
            "http://api.example.com"
        );
    }

    #[test]
    fn test_sanitize_url_without_scheme() {
        assert_eq!(
            sanitize_url("api.example.com"),
            "https://api.example.com"
        );
    }

    #[test]
    fn test_sanitize_url_removes_trailing_slash() {
        assert_eq!(
            sanitize_url("https://api.example.com/"),
            "https://api.example.com"
        );
        assert_eq!(
            sanitize_url("api.example.com/"),
            "https://api.example.com"
        );
    }

    #[test]
    fn test_sanitize_url_multiple_trailing_slashes() {
        assert_eq!(
            sanitize_url("https://api.example.com///"),
            "https://api.example.com"
        );
    }

    #[test]
    fn test_outerbounds_config_sanitizes_api_server() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().join(".metaflowconfig");
        fs::create_dir_all(&config_dir).unwrap();

        let config = serde_json::json!({
            "METAFLOW_SERVICE_AUTH_KEY": "key",
            "OBP_API_SERVER": "api.example.com/"
        });
        fs::write(config_dir.join("config.json"), config.to_string()).unwrap();

        with_clean_env(tmp.path(), || {
            let config = OuterboundsConfig::load().unwrap();
            assert_eq!(config.api_server, "https://api.example.com");
        });
    }

    // =========================================================================
    // api_url() tests
    // =========================================================================

    #[test]
    fn test_api_url_basic() {
        let config = OuterboundsConfig {
            api_key: "key".to_string(),
            api_server: "https://api.example.com".to_string(),
            perimeter: "prod".to_string(),
        };

        assert_eq!(
            config.api_url("capsules"),
            "https://api.example.com/v1/perimeters/prod/capsules"
        );
    }

    #[test]
    fn test_api_url_with_leading_slash() {
        let config = OuterboundsConfig {
            api_key: "key".to_string(),
            api_server: "https://api.example.com".to_string(),
            perimeter: "prod".to_string(),
        };

        assert_eq!(
            config.api_url("/capsules/123"),
            "https://api.example.com/v1/perimeters/prod/capsules/123"
        );
    }

    #[test]
    fn test_api_url_with_trailing_slash_server() {
        let config = OuterboundsConfig {
            api_key: "key".to_string(),
            api_server: "https://api.example.com/".to_string(),
            perimeter: "prod".to_string(),
        };

        assert_eq!(
            config.api_url("capsules"),
            "https://api.example.com/v1/perimeters/prod/capsules"
        );
    }

    #[test]
    fn test_api_url_nested_path() {
        let config = OuterboundsConfig {
            api_key: "key".to_string(),
            api_server: "https://api.example.com".to_string(),
            perimeter: "default".to_string(),
        };

        assert_eq!(
            config.api_url("capsules/abc123/workers/w1/logs"),
            "https://api.example.com/v1/perimeters/default/capsules/abc123/workers/w1/logs"
        );
    }

    #[test]
    fn test_api_base_url() {
        let config = OuterboundsConfig {
            api_key: "key".to_string(),
            api_server: "https://api.example.com".to_string(),
            perimeter: "prod".to_string(),
        };

        assert_eq!(config.api_base_url(), "https://api.example.com");
    }

    // =========================================================================
    // Additional edge case tests
    // =========================================================================

    #[test]
    fn test_config_with_extra_unknown_fields() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().join(".metaflowconfig");
        fs::create_dir_all(&config_dir).unwrap();

        let config = serde_json::json!({
            "METAFLOW_SERVICE_AUTH_KEY": "key",
            "OBP_API_SERVER": "https://api.example.com",
            "UNKNOWN_FUTURE_FIELD": "should be ignored",
            "ANOTHER_RANDOM_FIELD": 12345
        });
        fs::write(config_dir.join("config.json"), config.to_string()).unwrap();

        with_clean_env(tmp.path(), || {
            let config = MetaflowConfig::load().unwrap();
            assert_eq!(config.api_key, Some("key".to_string()));
        });
    }

    #[test]
    fn test_config_with_unicode_values() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().join(".metaflowconfig");
        fs::create_dir_all(&config_dir).unwrap();

        let config = serde_json::json!({
            "METAFLOW_SERVICE_AUTH_KEY": "key-with-émoji-🚀",
            "OBP_API_SERVER": "https://api.example.com"
        });
        fs::write(config_dir.join("config.json"), config.to_string()).unwrap();

        with_clean_env(tmp.path(), || {
            let config = MetaflowConfig::load().unwrap();
            assert_eq!(config.api_key, Some("key-with-émoji-🚀".to_string()));
        });
    }

    #[test]
    fn test_config_with_special_characters_in_key() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().join(".metaflowconfig");
        fs::create_dir_all(&config_dir).unwrap();

        let config = serde_json::json!({
            "METAFLOW_SERVICE_AUTH_KEY": "a!b_c-d.e+f=g/h",
            "OBP_API_SERVER": "https://api.example.com"
        });
        fs::write(config_dir.join("config.json"), config.to_string()).unwrap();

        with_clean_env(tmp.path(), || {
            let config = OuterboundsConfig::load().unwrap();
            assert_eq!(config.api_key, "a!b_c-d.e+f=g/h");
        });
    }

    #[test]
    fn test_ob_config_missing_perimeter_key() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().join(".metaflowconfig");
        fs::create_dir_all(&config_dir).unwrap();

        let config = serde_json::json!({
            "OB_CURRENT_PERIMETER_MF_CONFIG_URL": "https://api.example.com/config"
        });
        fs::write(config_dir.join("ob_config.json"), config.to_string()).unwrap();

        with_clean_env(tmp.path(), || {
            let config = ObPerimeterConfig::load().unwrap();
            assert!(config.current_perimeter.is_none());
            assert!(config.get_config_url().is_some());
        });
    }

    #[test]
    fn test_ob_config_missing_url_key() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().join(".metaflowconfig");
        fs::create_dir_all(&config_dir).unwrap();

        let config = serde_json::json!({
            "OB_CURRENT_PERIMETER": "production"
        });
        fs::write(config_dir.join("ob_config.json"), config.to_string()).unwrap();

        with_clean_env(tmp.path(), || {
            let config = ObPerimeterConfig::load().unwrap();
            assert_eq!(config.current_perimeter, Some("production".to_string()));
            assert!(config.get_config_url().is_none());
        });
    }

    #[test]
    fn test_combined_config_uses_ob_perimeter_over_default() {
        let tmp = TempDir::new().unwrap();
        create_config(tmp.path(), "key", "https://api.example.com");
        create_ob_config(tmp.path(), "custom-perimeter", "https://url");

        with_clean_env(tmp.path(), || {
            let config = OuterboundsConfig::load().unwrap();
            assert_eq!(config.perimeter, "custom-perimeter");
        });
    }

    #[test]
    fn test_profile_specific_ob_config_not_affecting_default() {
        let tmp = TempDir::new().unwrap();
        create_config(tmp.path(), "key", "https://api.example.com");
        create_ob_config_with_profile(
            tmp.path(),
            Some("staging"),
            "staging-only-perimeter",
            "https://staging.url",
            false,
        );

        with_clean_env(tmp.path(), || {
            let config = OuterboundsConfig::load().unwrap();
            assert_eq!(config.perimeter, "default", "Default profile should not see staging ob_config");
        });
    }

    #[test]
    fn test_metaflow_home_with_tilde_expansion_not_needed() {
        let tmp = TempDir::new().unwrap();
        let abs_path = tmp.path().join("abs_config");
        fs::create_dir_all(&abs_path).unwrap();

        temp_env::with_vars(
            [
                ("HOME", Some(tmp.path().to_str().unwrap())),
                ("METAFLOW_HOME", Some(abs_path.to_str().unwrap())),
            ],
            || {
                let home = metaflow_home().unwrap();
                assert_eq!(home, abs_path);
            },
        );
    }

    #[test]
    fn test_api_url_with_empty_path() {
        let config = OuterboundsConfig {
            api_key: "key".to_string(),
            api_server: "https://api.example.com".to_string(),
            perimeter: "prod".to_string(),
        };

        assert_eq!(
            config.api_url(""),
            "https://api.example.com/v1/perimeters/prod/"
        );
    }

    #[test]
    fn test_config_dir_created_but_empty() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().join(".metaflowconfig");
        fs::create_dir_all(&config_dir).unwrap();

        with_clean_env(tmp.path(), || {
            assert!(MetaflowConfig::load().is_none());
            assert!(ObPerimeterConfig::load().is_none());
        });
    }

    #[test]
    fn test_multiple_profiles_independent() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().join(".metaflowconfig");
        create_config_in_dir(&config_dir, None, "default-key", "https://default.com");
        create_config_in_dir(&config_dir, Some("dev"), "dev-key", "https://dev.com");
        create_config_in_dir(&config_dir, Some("prod"), "prod-key", "https://prod.com");

        with_clean_env(tmp.path(), || {
            let default = MetaflowConfig::load_with_profile(None).unwrap();
            let dev = MetaflowConfig::load_with_profile(Some("dev")).unwrap();
            let prod = MetaflowConfig::load_with_profile(Some("prod")).unwrap();

            assert_eq!(default.api_key, Some("default-key".to_string()));
            assert_eq!(dev.api_key, Some("dev-key".to_string()));
            assert_eq!(prod.api_key, Some("prod-key".to_string()));
        });
    }

    #[test]
    fn test_empty_profile_treated_as_none() {
        let tmp = TempDir::new().unwrap();
        create_config(tmp.path(), "default-key", "https://default.com");

        with_clean_env(tmp.path(), || {
            let config = MetaflowConfig::load_with_profile(Some("")).unwrap();
            assert_eq!(config.api_key, Some("default-key".to_string()));
        });
    }

    #[test]
    fn test_platform_domain_various_formats() {
        let cases = [
            ("https://api.foo.obp.outerbounds.com/v1/perimeters/default/metaflowconfigs/default", Some("foo.obp.outerbounds.com")),
            ("https://api.bar.outerbounds.com/", Some("bar.outerbounds.com")),
            ("https://api.single/", Some("single")),
            ("http://api.notsecure.com/v1", None), // http not https
            ("https://noapi.example.com/v1", None), // missing api. prefix
            ("", None),
        ];

        for (url, expected) in cases {
            let config = ObPerimeterConfig {
                current_perimeter: None,
                config_url: if url.is_empty() { None } else { Some(url.to_string()) },
                config_url_legacy: None,
            };
            assert_eq!(
                config.platform_domain().as_deref(),
                expected,
                "URL: {}",
                url
            );
        }
    }
}
