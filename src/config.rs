//! Global configuration for ana.
//!
//! Configuration is loaded from environment variables. In the future, this will
//! also support reading from `~/.ana/config.toml`.
//!
//! # Environment Variables
//!
//! | Variable             | Default         | Description                    |
//! |----------------------|-----------------|--------------------------------|
//! | `ANA_AUTH_DOMAIN`    | `anaconda.com`  | Authentication domain          |
//! | `ANA_AUTH_CLIENT_ID` | (Anaconda's ID) | OAuth client ID                |
//! | `ANA_SSL_VERIFY`     | `true`          | SSL certificate verification   |
//! | `ANA_OPEN_BROWSER`   | `true`          | Auto-open browser during login |
//!
//! Boolean values are parsed as `false` for empty, "0", or "false" (case-insensitive),
//! and `true` for any other value.

use std::env;
use std::fmt;

const DEFAULT_DOMAIN: &str = "anaconda.com";
const DEFAULT_CLIENT_ID: &str = "b4ad7f1d-c784-46b5-a9fe-106e50441f5a";
const DEFAULT_SSL_VERIFY: bool = true;
const DEFAULT_OPEN_BROWSER: bool = true;

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
}

impl Default for Config {
    fn default() -> Self {
        Self::load()
    }
}

impl Config {
    /// Load configuration from environment variables.
    pub fn load() -> Self {
        let domain = env::var("ANA_AUTH_DOMAIN").unwrap_or_else(|_| DEFAULT_DOMAIN.to_string());
        let client_id =
            env::var("ANA_AUTH_CLIENT_ID").unwrap_or_else(|_| DEFAULT_CLIENT_ID.to_string());
        let ssl_verify = parse_bool_env("ANA_SSL_VERIFY", DEFAULT_SSL_VERIFY);
        let open_browser = parse_bool_env("ANA_OPEN_BROWSER", DEFAULT_OPEN_BROWSER);

        Self {
            domain,
            client_id,
            ssl_verify,
            open_browser,
        }
    }
}

impl fmt::Display for Config {
    /// Format the configuration as a table.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rows = [
            ("domain", self.domain.as_str()),
            ("client_id", self.client_id.as_str()),
            ("ssl_verify", bool_to_str(self.ssl_verify)),
            ("open_browser", bool_to_str(self.open_browser)),
        ];
        write!(
            f,
            "{}",
            crate::console::format_table(("Setting", "Value"), &rows)
        )
    }
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
    fn test_config_display() {
        let config = test_config("anaconda.com", true, false);
        let display_str = format!("{}", config);

        assert!(display_str.contains("│ domain"));
        assert!(display_str.contains("│ anaconda.com"));
        assert!(display_str.contains("│ true"));
        assert!(display_str.contains("│ false"));
    }

    #[test]
    fn test_config_display_format() {
        let config = test_config("test.com", false, true);
        let display_str = format!("{}", config);

        // Should be a unicode table
        assert!(display_str.starts_with('┌'));
        assert!(display_str.ends_with('┘'));
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
        temp_env::with_var("ANA_AUTH_DOMAIN", Some("custom.example.com"), || {
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
}
