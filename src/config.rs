//! Global configuration for ana.
//!
//! Configuration is loaded from environment variables. In the future, this will
//! also support reading from `~/.ana/config.toml`.
//!
//! # Environment Variables
//!
//! | Variable                         | Default                    | Description                    |
//! |--------------------------------- |----------------------------|--------------------------------|
//! | `ANA_DOMAIN`                     | `anaconda.com`             | Authentication domain          |
//! | `ANA_AUTH_CLIENT_ID`             | (Anaconda's ID)            | OAuth client ID                |
//! | `ANA_SSL_VERIFY`                 | `true`                     | SSL certificate verification   |
//! | `ANA_OPEN_BROWSER`               | `true`                     | Auto-open browser during login |
//! | `ANA_METRICS_ENDPOINT`           | (Anaconda metrics URL)     | OpenTelemetry metrics endpoint |
//! | `ANA_METRICS_EXPORT_INTERVAL_MS` | `1000`                     | Metrics export interval in ms  |
//! | `ANA_METRICS_CONSOLE_EXPORTER`   | `false`                    | Enable console metrics exporter|
//! | `ANA_METRICS_SKIP_INTERNET_CHECK`| `true`                     | Skip internet connectivity check|
//! | `ANA_USE_HTTPS`                  | `true`                     | Use HTTPS (set false for HTTP) |
//! | `ANA_ENABLE_TELEMETRY`           | `true`                     | Enable/disable telemetry        |
//!
//! Boolean values are parsed as `false` for empty, "0", or "false" (case-insensitive),
//! and `true` for any other value.

use anaconda_otel_rs::{
    attributes::ResourceAttributes, config::Configuration, signals::initialize_telemetry,
};
use comfy_table::{
    Attribute, Cell, Table, modifiers::UTF8_SOLID_INNER_BORDERS, presets::UTF8_FULL,
};
use std::env;
use std::path::PathBuf;

use crate::VERSION;
use crate::auth;

pub fn setup_telemetry() {
    if !parse_bool_env("ANA_ENABLE_TELEMETRY", true) {
        return;
    }
    let _ = try_setup_telemetry();
}

fn try_setup_telemetry() -> Result<(), Box<dyn std::error::Error>> {
    let app_config = Config::load();

    let mut otel_config = Configuration::new(Some(&app_config.metrics_endpoint), None)?;

    let api_key = auth::get_api_key(&app_config).ok().flatten();
    otel_config.set_auth_token(api_key);
    otel_config.set_console_exporter(app_config.metrics_console_exporter);
    otel_config.set_metrics_export_interval_ms(app_config.metrics_export_interval_ms);
    otel_config.skip_internet_check = app_config.metrics_skip_internet_check;

    let attrs = ResourceAttributes::new("ana-cli", VERSION)?;

    initialize_telemetry(otel_config, attrs, vec!["metrics"])
        .map_err(|e| format!("Telemetry initialization failed: {}", e))?;

    Ok(())
}

// TODO(mattkram): Update default to anaconda.com before public release
const DEFAULT_DOMAIN: &str = "stage.anaconda.com";
const DEFAULT_CLIENT_ID: &str = "b4ad7f1d-c784-46b5-a9fe-106e50441f5a";
const DEFAULT_SSL_VERIFY: bool = true;
const DEFAULT_OPEN_BROWSER: bool = true;
const DEFAULT_METRICS_ENDPOINT: &str = "https://metrics.auth.anacondaconnect.com/v1/metrics";
const DEFAULT_METRICS_EXPORT_INTERVAL_MS: i64 = 1000;
const DEFAULT_METRICS_CONSOLE_EXPORTER: bool = false;
const DEFAULT_METRICS_SKIP_INTERNET_CHECK: bool = true;
const DEFAULT_USE_HTTPS: bool = true;

/// Global configuration for ana.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// The domain for authentication (e.g., "anaconda.com")
    pub domain: String,

    /// OAuth client ID
    pub client_id: String,

    /// Whether to verify SSL certificates
    pub ssl_verify: bool,

    /// Whether to automatically open browser during login
    pub open_browser: bool,

    /// OpenTelemetry metrics endpoint URL
    pub metrics_endpoint: String,

    /// Metrics export interval in milliseconds
    pub metrics_export_interval_ms: i64,

    /// Enable console metrics exporter
    pub metrics_console_exporter: bool,

    /// Skip internet connectivity check
    pub metrics_skip_internet_check: bool,

    /// Path to the keyring file for storing API keys
    pub keyring_path: PathBuf,

    /// Whether to use HTTPS (set false for HTTP, e.g. testing)
    pub use_https: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self::load()
    }
}

impl Config {
    /// Load configuration from environment variables.
    pub fn load() -> Self {
        let domain = env::var("ANA_DOMAIN").unwrap_or_else(|_| DEFAULT_DOMAIN.to_string());
        let client_id =
            env::var("ANA_AUTH_CLIENT_ID").unwrap_or_else(|_| DEFAULT_CLIENT_ID.to_string());
        let ssl_verify = parse_bool_env("ANA_SSL_VERIFY", DEFAULT_SSL_VERIFY);
        let open_browser = parse_bool_env("ANA_OPEN_BROWSER", DEFAULT_OPEN_BROWSER);
        let metrics_endpoint = env::var("ANA_METRICS_ENDPOINT")
            .unwrap_or_else(|_| DEFAULT_METRICS_ENDPOINT.to_string());
        let metrics_export_interval_ms = env::var("ANA_METRICS_EXPORT_INTERVAL_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_METRICS_EXPORT_INTERVAL_MS);
        let metrics_console_exporter = parse_bool_env(
            "ANA_METRICS_CONSOLE_EXPORTER",
            DEFAULT_METRICS_CONSOLE_EXPORTER,
        );
        let metrics_skip_internet_check = parse_bool_env(
            "ANA_METRICS_SKIP_INTERNET_CHECK",
            DEFAULT_METRICS_SKIP_INTERNET_CHECK,
        );
        let keyring_path = env::var("ANA_KEYRING_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| default_keyring_path());
        let use_https = parse_bool_env("ANA_USE_HTTPS", DEFAULT_USE_HTTPS);

        Self {
            domain,
            client_id,
            ssl_verify,
            open_browser,
            metrics_endpoint,
            metrics_export_interval_ms,
            metrics_console_exporter,
            metrics_skip_internet_check,
            keyring_path,
            use_https,
        }
    }

    /// Get the protocol (http or https) based on configuration.
    fn protocol(&self) -> &'static str {
        if self.use_https { "https" } else { "http" }
    }

    /// Get the base URL for API requests.
    pub fn base_url(&self) -> String {
        format!("{}://{}", self.protocol(), self.domain)
    }

    /// Get the OpenID Connect well-known configuration URL.
    pub fn well_known_url(&self) -> String {
        format!("{}/.well-known/openid-configuration", self.base_url())
    }
}

impl Config {
    /// Print the configuration as a table.
    pub fn print_table(&self) {
        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        table.apply_modifier(UTF8_SOLID_INNER_BORDERS);
        table.set_header([
            Cell::new("Setting").add_attribute(Attribute::Bold),
            Cell::new("Value").add_attribute(Attribute::Bold),
        ]);
        table.add_row(["domain", &self.domain]);
        table.add_row(["client_id", &self.client_id]);
        table.add_row(["ssl_verify", bool_to_str(self.ssl_verify)]);
        table.add_row(["open_browser", bool_to_str(self.open_browser)]);
        println!("{table}");
    }
}

/// Get the default keyring path (~/.ana/keyring).
fn default_keyring_path() -> PathBuf {
    dirs::home_dir()
        .expect("Could not determine home directory")
        .join(".ana")
        .join("keyring")
}

/// Convert a boolean to a string.
fn bool_to_str(val: bool) -> &'static str {
    if val { "true" } else { "false" }
}

/// Parse a boolean from a string value.
///
/// Whitespace is trimmed before parsing.
/// Returns `false` if the value is empty, "0", or "false" (case-insensitive).
/// Returns `true` for any other value.
fn parse_bool(val: &str) -> bool {
    let val = val.trim().to_lowercase();
    !(val.is_empty() || val == "0" || val == "false")
}

/// Parse a boolean from an environment variable.
///
/// Returns `default` if the variable is not set.
fn parse_bool_env(name: &str, default: bool) -> bool {
    match env::var(name) {
        Ok(val) => parse_bool(&val),
        Err(_) => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a config with specific values for testing
    fn test_config(domain: &str, ssl_verify: bool, open_browser: bool) -> Config {
        Config {
            domain: domain.to_string(),
            client_id: DEFAULT_CLIENT_ID.to_string(),
            ssl_verify,
            open_browser,
            metrics_endpoint: DEFAULT_METRICS_ENDPOINT.to_string(),
            metrics_export_interval_ms: DEFAULT_METRICS_EXPORT_INTERVAL_MS,
            metrics_console_exporter: DEFAULT_METRICS_CONSOLE_EXPORTER,
            metrics_skip_internet_check: DEFAULT_METRICS_SKIP_INTERNET_CHECK,
            keyring_path: default_keyring_path(),
            use_https: true,
        }
    }

    #[test]
    fn test_parse_bool_env_nonexistent_returns_default() {
        // When not set, returns default
        assert!(parse_bool_env("NONEXISTENT_VAR_12345", true));
        assert!(!parse_bool_env("NONEXISTENT_VAR_12345", false));
    }

    #[test]
    fn test_parse_bool_env_values() {
        // Test that "0" and "false" are all interpreted as false
        // We can't easily test this without setting env vars, so we test the logic directly
        let test_cases = vec![
            ("0", false),
            ("false", false),
            ("FALSE", false),
            ("False", false),
            ("1", true),
            ("true", true),
            ("TRUE", true),
            ("anything", true),
            ("", false),
            ("  ", false),
            ("  true  ", true),
            ("  false  ", false),
        ];

        for (input, expected) in test_cases {
            assert_eq!(
                parse_bool(input),
                expected,
                "Expected parse of '{}' to be {}",
                input,
                expected
            );
        }
    }

    #[test]
    fn test_default_values() {
        let config = test_config("anaconda.com", true, true);
        assert_eq!(config.domain, "anaconda.com");
        assert_eq!(config.client_id, DEFAULT_CLIENT_ID);
        assert!(config.ssl_verify);
        assert!(config.open_browser);
    }

    #[test]
    fn test_config_equality() {
        let base_config = test_config("anaconda.com", true, true);
        let same_config = test_config("anaconda.com", true, true);
        let different_config = test_config("other.com", true, true);

        assert_eq!(base_config, same_config);
        assert_ne!(base_config, different_config);
    }

    #[test]
    fn test_config_clone() {
        let base_config = test_config("anaconda.com", true, false);
        let cloned_config = base_config.clone();

        assert_eq!(base_config, cloned_config);
        assert_eq!(base_config.domain, cloned_config.domain);
        assert_eq!(base_config.ssl_verify, cloned_config.ssl_verify);
        assert_eq!(base_config.open_browser, cloned_config.open_browser);
    }

    #[test]
    fn test_config_load_returns_valid_config() {
        // This test verifies Config::load() doesn't panic and returns valid data
        let config = Config::load();

        // Domain should never be empty
        assert!(!config.domain.is_empty());
        // Client ID should never be empty
        assert!(!config.client_id.is_empty());
    }

    #[test]
    fn test_config_default_is_load() {
        // Default implementation should be equivalent to load() (for now)
        let default_config = Config::default();
        let loaded_config = Config::load();

        assert_eq!(default_config, loaded_config);
    }

    #[test]
    fn test_config_load_domain_from_env() {
        temp_env::with_var("ANA_DOMAIN", Some("custom.example.com"), || {
            let config = Config::load();
            assert_eq!(config.domain, "custom.example.com");
        });
    }

    #[test]
    fn test_config_load_client_id_from_env() {
        temp_env::with_var("ANA_AUTH_CLIENT_ID", Some("my-custom-client-id"), || {
            let config = Config::load();
            assert_eq!(config.client_id, "my-custom-client-id");
        });
    }

    #[test]
    fn test_config_load_ssl_verify_false_from_env() {
        temp_env::with_var("ANA_SSL_VERIFY", Some("false"), || {
            let config = Config::load();
            assert!(!config.ssl_verify);
        });
    }

    #[test]
    fn test_config_load_open_browser_false_from_env() {
        temp_env::with_var("ANA_OPEN_BROWSER", Some("0"), || {
            let config = Config::load();
            assert!(!config.open_browser);
        });
    }

    #[test]
    fn test_config_load_keyring_path_from_env() {
        temp_env::with_var("ANA_KEYRING_PATH", Some("/custom/path/keyring"), || {
            let config = Config::load();
            assert_eq!(config.keyring_path, PathBuf::from("/custom/path/keyring"));
        });
    }

    #[test]
    fn test_config_default_keyring_path() {
        let config = Config::load();
        // Should end with .ana/keyring
        assert!(config.keyring_path.ends_with(".ana/keyring"));
    }

    #[test]
    fn test_config_load_use_https_false_from_env() {
        temp_env::with_var("ANA_USE_HTTPS", Some("false"), || {
            let config = Config::load();
            assert!(!config.use_https);
        });
    }

    #[test]
    fn test_config_default_use_https_is_true() {
        temp_env::with_var("ANA_USE_HTTPS", None::<&str>, || {
            let config = Config::load();
            assert!(config.use_https);
        });
    }

    #[test]
    fn test_config_base_url_https() {
        let config = test_config("example.com", true, true);
        assert_eq!(config.base_url(), "https://example.com");
    }

    #[test]
    fn test_config_base_url_http() {
        let mut config = test_config("example.com", true, true);
        config.use_https = false;
        assert_eq!(config.base_url(), "http://example.com");
    }
}
