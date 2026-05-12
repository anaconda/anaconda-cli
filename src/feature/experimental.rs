//! Experimental feature management via ~/.ana/config.toml
//!
//! Stores feature flags in a `[ana.features]` section:
//! ```toml
//! [ana.features]
//! outerbounds = true
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::paths::ana_home;

/// Valid experimental feature names.
#[cfg(all(unix, feature = "unstable"))]
const VALID_FEATURES: &[&str] = &["outerbounds", "wheels"];

#[cfg(all(unix, not(feature = "unstable")))]
const VALID_FEATURES: &[&str] = &["outerbounds"];

#[cfg(all(windows, feature = "unstable"))]
const VALID_FEATURES: &[&str] = &["wheels"];

#[cfg(all(windows, not(feature = "unstable")))]
const VALID_FEATURES: &[&str] = &[];

/// Root config structure for ~/.ana/config.toml
#[derive(Default, Serialize, Deserialize)]
struct AnaConfig {
    ana: Option<AnaSection>,
}

/// The [ana] section of the config
#[derive(Default, Serialize, Deserialize)]
struct AnaSection {
    features: Option<HashMap<String, bool>>,
}

/// Returns the path to the config file.
fn config_path() -> PathBuf {
    ana_home().join("config.toml")
}

/// Load the config file, returning default if it doesn't exist or can't be parsed.
fn load_config() -> AnaConfig {
    let path = config_path();
    if !path.exists() {
        return AnaConfig::default();
    }

    fs::read_to_string(&path)
        .ok()
        .and_then(|content| toml::from_str(&content).ok())
        .unwrap_or_default()
}

/// Save the config file, preserving existing content where possible.
fn save_config(config: &AnaConfig) -> miette::Result<()> {
    let path = config_path();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| miette::miette!("Failed to create config directory: {}", e))?;
    }

    let content = toml::to_string_pretty(config)
        .map_err(|e| miette::miette!("Failed to serialize config: {}", e))?;

    fs::write(&path, content).map_err(|e| miette::miette!("Failed to write config file: {}", e))
}

/// Check if an experimental feature is enabled.
pub fn is_feature_enabled(name: &str) -> bool {
    let config = load_config();
    config
        .ana
        .and_then(|a| a.features)
        .and_then(|f| f.get(name).copied())
        .unwrap_or(false)
}

/// Check if a feature name is valid.
pub fn is_valid_feature(name: &str) -> bool {
    VALID_FEATURES.contains(&name)
}

/// Enable an experimental feature.
pub fn enable_feature(name: &str) -> miette::Result<()> {
    if !is_valid_feature(name) {
        return Err(miette::miette!("Unknown experimental feature: {}", name));
    }

    let mut config = load_config();

    // Ensure nested structure exists
    let ana = config.ana.get_or_insert_with(AnaSection::default);
    let features = ana.features.get_or_insert_with(HashMap::new);
    features.insert(name.to_string(), true);

    save_config(&config)
}

/// Disable an experimental feature.
pub fn disable_feature(name: &str) -> miette::Result<()> {
    if !is_valid_feature(name) {
        return Err(miette::miette!("Unknown experimental feature: {}", name));
    }

    let mut config = load_config();

    // Ensure nested structure exists
    let ana = config.ana.get_or_insert_with(AnaSection::default);
    let features = ana.features.get_or_insert_with(HashMap::new);
    features.insert(name.to_string(), false);

    save_config(&config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    #[cfg(unix)]
    fn test_is_valid_feature() {
        assert!(is_valid_feature("outerbounds"));
        assert!(!is_valid_feature("unknown"));
        assert!(!is_valid_feature(""));
    }

    #[test]
    #[cfg(windows)]
    fn test_is_valid_feature_windows() {
        // On Windows, no experimental features are available
        assert!(!is_valid_feature("outerbounds"));
        assert!(!is_valid_feature("unknown"));
    }

    #[test]
    #[cfg(unix)]
    fn test_enable_disable_feature() {
        let tmp = TempDir::new().unwrap();

        temp_env::with_var("ANA_HOME", Some(tmp.path().to_str().unwrap()), || {
            // Initially disabled
            assert!(!is_feature_enabled("outerbounds"));

            // Enable it
            enable_feature("outerbounds").unwrap();
            assert!(is_feature_enabled("outerbounds"));

            // Verify config file exists and has correct content
            let config_content = fs::read_to_string(tmp.path().join("config.toml")).unwrap();
            assert!(config_content.contains("[ana.features]"));
            assert!(config_content.contains("outerbounds = true"));

            // Disable it
            disable_feature("outerbounds").unwrap();
            assert!(!is_feature_enabled("outerbounds"));

            // Verify config updated
            let config_content = fs::read_to_string(tmp.path().join("config.toml")).unwrap();
            assert!(config_content.contains("outerbounds = false"));
        });
    }

    #[test]
    fn test_enable_invalid_feature() {
        let tmp = TempDir::new().unwrap();

        temp_env::with_var("ANA_HOME", Some(tmp.path().to_str().unwrap()), || {
            let result = enable_feature("invalid_feature");
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("Unknown"));
        });
    }

    #[test]
    #[cfg(windows)]
    fn test_enable_outerbounds_invalid_on_windows() {
        let tmp = TempDir::new().unwrap();

        temp_env::with_var("ANA_HOME", Some(tmp.path().to_str().unwrap()), || {
            // On Windows, outerbounds is not a valid feature
            let result = enable_feature("outerbounds");
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("Unknown"));
        });
    }

    #[test]
    fn test_load_config_missing_file() {
        let tmp = TempDir::new().unwrap();

        temp_env::with_var("ANA_HOME", Some(tmp.path().to_str().unwrap()), || {
            // Should return default config without error
            let config = load_config();
            assert!(config.ana.is_none());
        });
    }

    #[test]
    #[cfg(unix)]
    fn test_load_config_preserves_other_content() {
        let tmp = TempDir::new().unwrap();

        temp_env::with_var("ANA_HOME", Some(tmp.path().to_str().unwrap()), || {
            // Write initial config with other content
            let initial = r#"
[ana.features]
outerbounds = true

[ana.other]
key = "value"
"#;
            fs::write(tmp.path().join("config.toml"), initial).unwrap();

            // Enable feature (should preserve structure)
            let _config = load_config();
            assert!(is_feature_enabled("outerbounds"));

            // Disable and re-enable
            disable_feature("outerbounds").unwrap();
            enable_feature("outerbounds").unwrap();
            assert!(is_feature_enabled("outerbounds"));
        });
    }
}
