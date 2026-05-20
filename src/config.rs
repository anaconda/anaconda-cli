//! Global configuration for ana.
//!
//! Configuration is loaded from environment variables. In the future, this will
//! also support reading from `~/.ana/config.toml`.
//!
//! # Environment Variables
//!
//! | Variable                         | Default                    | Description                     |
//! |--------------------------------- |----------------------------|---------------------------------|
//! | `ANA_DOMAIN`                     | `anaconda.com`             | Authentication domain           |
//! | `ANA_AUTH_DOMAIN_OVERRIDE`       | (none)                     | Override auth domain for OIDC   |
//! | `ANA_AUTH_CLIENT_ID`             | (Anaconda's ID)            | OAuth client ID                 |
//! | `ANA_SSL_VERIFY`                 | `true`                     | SSL certificate verification    |
//! | `ANA_OPEN_BROWSER`               | `true`                     | Auto-open browser during login  |
//! | `ANA_METRICS_ENDPOINT`           | (Anaconda metrics URL)     | OpenTelemetry metrics endpoint (authenticated) |
//! | `ANA_METRICS_PUBLIC_ENDPOINT`    | (Anaconda public URL)      | OpenTelemetry metrics endpoint (unauthenticated) |
//! | `ANA_METRICS_EXPORT_INTERVAL_MS` | `1000`                     | Metrics export interval in ms   |
//! | `ANA_METRICS_CONSOLE_EXPORTER`   | `false`                    | Enable console metrics exporter |
//! | `ANA_METRICS_SKIP_INTERNET_CHECK`| `true`                     | Skip internet connectivity check|
//! | `ANA_USE_HTTPS`                  | `true`                     | Use HTTPS (set false for HTTP)  |
//! | `ANA_ENABLE_TELEMETRY`           | `true`                     | Enable/disable telemetry        |
//! | `ANA_PRERELEASES`                | `false`                    | Include prereleases in updates  |
//! | `ANA_PIP_INDEX_URL`              | `https://repo.anaconda.cloud/repo/anaconda-wheels/simple` | Package index URL for Anaconda wheels |
//! | `ANA_SELF_UPDATE_URL`            | (Anaconda static URL)      | Update URL; set to `github` for GitHub Releases |
//! | `ANACONDA_AUTH_AUTH_DOMAIN_OVERRIDE` | (none)                 | Override auth domain for OIDC   |
//! | `ANACONDA_AUTH_PREFERRED_TOKEN_STORAGE` | `anaconda-keyring`  | Token storage: "system" or "anaconda-keyring" |
//! | `ANACONDA_AUTH_API_KEY`          | (none)                     | Static API key (bypasses OIDC)  |
//! | `ANACONDA_AUTH_KEYRING`          | (none)                     | Keyring backend config (JSON)   |
//! | `ANACONDA_AUTH_REDIRECT_URI`     | `http://127.0.0.1:8000/auth/oidc` | OIDC redirect URI        |
//! | `ANACONDA_AUTH_OPENID_CONFIG_PATH` | `/.well-known/openid-configuration` | OIDC discovery path |
//! | `ANACONDA_AUTH_OIDC_REQUEST_HEADERS` | (User-Agent header)    | OIDC request headers (JSON)     |
//! | `ANACONDA_AUTH_LOGIN_SUCCESS_PATH` | `/app/local-login-success` | Success redirect path         |
//! | `ANACONDA_AUTH_LOGIN_ERROR_PATH` | `/app/local-login-error`   | Error redirect path             |
//! | `ANACONDA_AUTH_USE_UNIFIED_REPO_API_KEY` | `false`            | Single key for repo and API     |
//! | `ANACONDA_AUTH_HASH_HOSTNAME`    | `true`                     | Hash hostnames in keyring keys  |
//! | `ANACONDA_AUTH_PROXY_SERVERS`    | (none)                     | Proxy config (JSON map)         |
//! | `ANACONDA_AUTH_CLIENT_CERT`      | (none)                     | Client cert path for mTLS       |
//! | `ANACONDA_AUTH_CLIENT_CERT_KEY`  | (none)                     | Client cert key path for mTLS   |
//! | `ANACONDA_AUTH_USE_DEVICE_FLOW`  | `false`                    | Use OIDC device flow            |
//! | `ANACONDA_AUTH_EXTRA_HEADERS`    | (none)                     | Extra HTTP headers (JSON map)   |
//! | `ANACONDA_AUTH_ENV_MANAGER_CHANNEL` | `anaconda-cloud`        | Env manager conda channel       |
//! | `ANACONDA_AUTH_ENV_MANAGER_PACKAGE` | `anaconda-env-manager`  | Env manager package name        |
//! | `ANACONDA_AUTH_ENV_MANAGER_VERSION` | (none)                  | Env manager version constraint  |
//!
//! When the `diagnostics` feature is enabled:
//!
//! | Variable                         | Default                    | Description                     |
//! |--------------------------------- |----------------------------|---------------------------------|
//! | `ANA_SENTRY_DISABLED`            | `false`                    | Disable Sentry error reporting  |
//! | `ANA_SENTRY_ENVIRONMENT`         | `production`               | Sentry environment tag          |
//!
//! Boolean values are parsed as `false` for empty, "0", or "false" (case-insensitive),
//! and `true` for any other value.

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use crate::table;

/// Check if telemetry is enabled.
pub fn telemetry_enabled() -> bool {
    parse_bool_env("ANA_ENABLE_TELEMETRY", true)
}

const DEFAULT_DOMAIN: &str = "anaconda.com";
const DEFAULT_CLIENT_ID: &str = "b4ad7f1d-c784-46b5-a9fe-106e50441f5a";
const DEFAULT_SSL_VERIFY: bool = true;
const DEFAULT_OPEN_BROWSER: bool = true;
const DEFAULT_METRICS_ENDPOINT: &str = "https://metrics.aa.anaconda.com/v1/metrics";
const DEFAULT_METRICS_PUBLIC_ENDPOINT: &str = "https://public.telemetry.anaconda.com/v1/metrics";
const DEFAULT_METRICS_EXPORT_INTERVAL_MS: i64 = 1000;
const DEFAULT_METRICS_CONSOLE_EXPORTER: bool = false;
const DEFAULT_METRICS_SKIP_INTERNET_CHECK: bool = true;
const DEFAULT_USE_HTTPS: bool = true;
const DEFAULT_INCLUDE_PRERELEASES: bool = false;
const DEFAULT_PIP_INDEX_URL: &str = "https://repo.anaconda.cloud/repo/anaconda-wheels/simple";
const DEFAULT_SELF_UPDATE_URL: &str = "https://anaconda.sh";
const DEFAULT_PREFERRED_TOKEN_STORAGE: &str = "anaconda-keyring";
const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:8000/auth/oidc";
const DEFAULT_OPENID_CONFIG_PATH: &str = "/.well-known/openid-configuration";
const DEFAULT_LOGIN_SUCCESS_PATH: &str = "/app/local-login-success";
const DEFAULT_LOGIN_ERROR_PATH: &str = "/app/local-login-error";
const DEFAULT_USE_UNIFIED_REPO_API_KEY: bool = false;
const DEFAULT_HASH_HOSTNAME: bool = true;
const DEFAULT_USE_DEVICE_FLOW: bool = false;
const DEFAULT_ENV_MANAGER_CHANNEL: &str = "anaconda-cloud";
const DEFAULT_ENV_MANAGER_PACKAGE: &str = "anaconda-env-manager";
#[cfg(feature = "diagnostics")]
const DEFAULT_SENTRY_DISABLED: bool = false;
#[cfg(feature = "diagnostics")]
const DEFAULT_SENTRY_ENVIRONMENT: &str = "production";

/// Global configuration for ana.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Config {
    /// The domain for authentication (e.g., "anaconda.com")
    pub domain: String,

    /// Override the authentication domain for OIDC discovery and token endpoints
    pub auth_domain_override: Option<String>,

    /// OAuth client ID
    pub client_id: String,

    /// Whether to verify SSL certificates
    pub ssl_verify: bool,

    /// Whether to automatically open browser during login
    pub open_browser: bool,

    /// OpenTelemetry metrics endpoint URL (authenticated)
    pub metrics_endpoint: String,

    /// OpenTelemetry metrics endpoint URL (unauthenticated/public)
    pub metrics_public_endpoint: String,

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

    /// Whether to include prereleases when checking for updates
    pub include_prereleases: bool,

    /// Pip index URL for package installation
    pub pip_index_url: String,

    /// Base URL for static self-update; if None, uses GitHub Releases
    pub self_update_url: Option<String>,

    /// Where to store authentication tokens: "system" or "anaconda-keyring"
    pub preferred_token_storage: String,

    /// Static API key for authentication (bypasses OIDC flow when set)
    pub api_key: Option<String>,

    /// Keyring backend configuration (backend identifier -> settings)
    pub keyring: Option<HashMap<String, HashMap<String, String>>>,

    /// Local redirect URI for OIDC authorization code flow
    pub redirect_uri: String,

    /// URL path for OpenID Connect discovery document
    pub openid_config_path: String,

    /// HTTP headers for OIDC requests
    pub oidc_request_headers: HashMap<String, String>,

    /// URL path for successful login redirect
    pub login_success_path: String,

    /// URL path for failed login redirect
    pub login_error_path: String,

    /// Use a single API key for both repository and API access
    pub use_unified_repo_api_key: bool,

    /// Hash hostnames before using as keyring storage keys
    pub hash_hostname: bool,

    /// Proxy server configuration (protocol -> proxy URL)
    pub proxy_servers: Option<HashMap<String, String>>,

    /// Path to client certificate for mutual TLS
    pub client_cert: Option<String>,

    /// Path to private key for client certificate
    pub client_cert_key: Option<String>,

    /// Use OIDC Device Authorization Grant flow instead of Authorization Code flow
    pub use_device_flow: bool,

    /// Additional HTTP headers for all requests
    pub extra_headers: Option<HashMap<String, String>>,

    /// Conda channel for environment manager package
    pub env_manager_channel: String,

    /// Name of the environment manager conda package
    pub env_manager_package: String,

    /// Version constraint for environment manager package
    pub env_manager_version: Option<String>,

    /// Whether Sentry error reporting is disabled
    #[cfg(feature = "diagnostics")]
    pub sentry_disabled: bool,

    /// Sentry environment tag (e.g., "production", "integration-test")
    #[cfg(feature = "diagnostics")]
    pub sentry_environment: String,
}

impl Default for Config {
    fn default() -> Self {
        Self::load()
    }
}

impl Config {
    /// Load configuration from environment variables.
    pub fn load() -> Self {
        let domain = normalize_domain(
            &env::var("ANA_DOMAIN").unwrap_or_else(|_| DEFAULT_DOMAIN.to_string()),
        );
        let auth_domain_override = env::var("ANACONDA_AUTH_AUTH_DOMAIN_OVERRIDE")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(|s| normalize_domain(&s));
        let client_id =
            env::var("ANA_AUTH_CLIENT_ID").unwrap_or_else(|_| DEFAULT_CLIENT_ID.to_string());
        let ssl_verify = parse_bool_env("ANA_SSL_VERIFY", DEFAULT_SSL_VERIFY);
        let open_browser = parse_bool_env("ANA_OPEN_BROWSER", DEFAULT_OPEN_BROWSER);
        let metrics_endpoint = env::var("ANA_METRICS_ENDPOINT")
            .unwrap_or_else(|_| DEFAULT_METRICS_ENDPOINT.to_string());
        let metrics_public_endpoint = env::var("ANA_METRICS_PUBLIC_ENDPOINT")
            .unwrap_or_else(|_| DEFAULT_METRICS_PUBLIC_ENDPOINT.to_string());
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
        let include_prereleases = parse_bool_env("ANA_PRERELEASES", DEFAULT_INCLUDE_PRERELEASES);
        let pip_index_url =
            env::var("ANA_PIP_INDEX_URL").unwrap_or_else(|_| DEFAULT_PIP_INDEX_URL.to_string());
        let self_update_url = match env::var("ANA_SELF_UPDATE_URL") {
            Ok(s) if s.trim().eq_ignore_ascii_case("github") => None,
            Ok(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
            _ => Some(DEFAULT_SELF_UPDATE_URL.to_string()),
        };
        let preferred_token_storage = env::var("ANACONDA_AUTH_PREFERRED_TOKEN_STORAGE")
            .unwrap_or_else(|_| DEFAULT_PREFERRED_TOKEN_STORAGE.to_string());
        let api_key = env::var("ANACONDA_AUTH_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty());
        let keyring = env::var("ANACONDA_AUTH_KEYRING")
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok());
        let redirect_uri = env::var("ANACONDA_AUTH_REDIRECT_URI")
            .unwrap_or_else(|_| DEFAULT_REDIRECT_URI.to_string());
        let openid_config_path = env::var("ANACONDA_AUTH_OPENID_CONFIG_PATH")
            .unwrap_or_else(|_| DEFAULT_OPENID_CONFIG_PATH.to_string());
        let oidc_request_headers = env::var("ANACONDA_AUTH_OIDC_REQUEST_HEADERS")
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(default_oidc_request_headers);
        let login_success_path = env::var("ANACONDA_AUTH_LOGIN_SUCCESS_PATH")
            .unwrap_or_else(|_| DEFAULT_LOGIN_SUCCESS_PATH.to_string());
        let login_error_path = env::var("ANACONDA_AUTH_LOGIN_ERROR_PATH")
            .unwrap_or_else(|_| DEFAULT_LOGIN_ERROR_PATH.to_string());
        let use_unified_repo_api_key = parse_bool_env(
            "ANACONDA_AUTH_USE_UNIFIED_REPO_API_KEY",
            DEFAULT_USE_UNIFIED_REPO_API_KEY,
        );
        let hash_hostname =
            parse_bool_env("ANACONDA_AUTH_HASH_HOSTNAME", DEFAULT_HASH_HOSTNAME);
        let proxy_servers = env::var("ANACONDA_AUTH_PROXY_SERVERS")
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok());
        let client_cert = env::var("ANACONDA_AUTH_CLIENT_CERT")
            .ok()
            .filter(|s| !s.trim().is_empty());
        let client_cert_key = env::var("ANACONDA_AUTH_CLIENT_CERT_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty());
        let use_device_flow =
            parse_bool_env("ANACONDA_AUTH_USE_DEVICE_FLOW", DEFAULT_USE_DEVICE_FLOW);
        let extra_headers = env::var("ANACONDA_AUTH_EXTRA_HEADERS")
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok());
        let env_manager_channel = env::var("ANACONDA_AUTH_ENV_MANAGER_CHANNEL")
            .unwrap_or_else(|_| DEFAULT_ENV_MANAGER_CHANNEL.to_string());
        let env_manager_package = env::var("ANACONDA_AUTH_ENV_MANAGER_PACKAGE")
            .unwrap_or_else(|_| DEFAULT_ENV_MANAGER_PACKAGE.to_string());
        let env_manager_version = env::var("ANACONDA_AUTH_ENV_MANAGER_VERSION")
            .ok()
            .filter(|s| !s.trim().is_empty());
        #[cfg(feature = "diagnostics")]
        let sentry_disabled = parse_bool_env("ANA_SENTRY_DISABLED", DEFAULT_SENTRY_DISABLED);
        #[cfg(feature = "diagnostics")]
        let sentry_environment = env::var("ANA_SENTRY_ENVIRONMENT")
            .unwrap_or_else(|_| DEFAULT_SENTRY_ENVIRONMENT.to_string());

        Self {
            domain,
            auth_domain_override,
            client_id,
            ssl_verify,
            open_browser,
            metrics_endpoint,
            metrics_public_endpoint,
            metrics_export_interval_ms,
            metrics_console_exporter,
            metrics_skip_internet_check,
            keyring_path,
            use_https,
            include_prereleases,
            pip_index_url,
            self_update_url,
            preferred_token_storage,
            api_key,
            keyring,
            redirect_uri,
            openid_config_path,
            oidc_request_headers,
            login_success_path,
            login_error_path,
            use_unified_repo_api_key,
            hash_hostname,
            proxy_servers,
            client_cert,
            client_cert_key,
            use_device_flow,
            extra_headers,
            env_manager_channel,
            env_manager_package,
            env_manager_version,
            #[cfg(feature = "diagnostics")]
            sentry_disabled,
            #[cfg(feature = "diagnostics")]
            sentry_environment,
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

    /// Get the authentication domain (uses auth_domain_override if set, otherwise domain).
    pub fn auth_domain(&self) -> &str {
        self.auth_domain_override.as_deref().unwrap_or(&self.domain)
    }

    /// Get the base URL for authentication requests.
    pub fn auth_base_url(&self) -> String {
        format!("{}://{}", self.protocol(), self.auth_domain())
    }

    /// Get the OpenID Connect well-known configuration URL.
    pub fn well_known_url(&self) -> String {
        format!("{}{}", self.auth_base_url(), self.openid_config_path)
    }
}

impl Config {
    /// Print the configuration as a table.
    pub fn print_table(&self) {
        let mut table = table::new(["Setting", "Value"]);

        if let Ok(serde_json::Value::Object(map)) = serde_json::to_value(self) {
            for (key, value) in map {
                let value_str = match value {
                    serde_json::Value::String(s) => s,
                    serde_json::Value::Bool(b) => b.to_string(),
                    serde_json::Value::Number(n) => n.to_string(),
                    _ => value.to_string(),
                };
                table.add_row([key, value_str]);
            }
        }

        println!("{table}");
    }
}

/// Get the default keyring path
fn default_keyring_path() -> PathBuf {
    crate::paths::home_dir().join(".anaconda").join("keyring")
}

/// Get default OIDC request headers
fn default_oidc_request_headers() -> HashMap<String, String> {
    let mut headers = HashMap::new();
    headers.insert(
        "User-Agent".to_string(),
        format!("ana/{}", env!("CARGO_PKG_VERSION")),
    );
    headers
}

/// Normalize a domain by stripping scheme (http://, https://) and path components.
/// e.g., "https://stage.anaconda.com/app" -> "stage.anaconda.com"
fn normalize_domain(domain: &str) -> String {
    let domain = domain.trim();

    // If it looks like a URL (has scheme), parse it properly
    if domain.starts_with("http://") || domain.starts_with("https://") {
        if let Ok(url) = url::Url::parse(domain) {
            if let Some(host) = url.host_str() {
                return host.to_string();
            }
        }
    }

    // Otherwise treat as bare domain - strip any path component
    domain.split('/').next().unwrap_or(domain).to_string()
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
            auth_domain_override: None,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            ssl_verify,
            open_browser,
            metrics_endpoint: DEFAULT_METRICS_ENDPOINT.to_string(),
            metrics_public_endpoint: DEFAULT_METRICS_PUBLIC_ENDPOINT.to_string(),
            metrics_export_interval_ms: DEFAULT_METRICS_EXPORT_INTERVAL_MS,
            metrics_console_exporter: DEFAULT_METRICS_CONSOLE_EXPORTER,
            metrics_skip_internet_check: DEFAULT_METRICS_SKIP_INTERNET_CHECK,
            keyring_path: default_keyring_path(),
            use_https: true,
            include_prereleases: false,
            pip_index_url: DEFAULT_PIP_INDEX_URL.to_string(),
            self_update_url: Some(DEFAULT_SELF_UPDATE_URL.to_string()),
            preferred_token_storage: DEFAULT_PREFERRED_TOKEN_STORAGE.to_string(),
            api_key: None,
            keyring: None,
            redirect_uri: DEFAULT_REDIRECT_URI.to_string(),
            openid_config_path: DEFAULT_OPENID_CONFIG_PATH.to_string(),
            oidc_request_headers: default_oidc_request_headers(),
            login_success_path: DEFAULT_LOGIN_SUCCESS_PATH.to_string(),
            login_error_path: DEFAULT_LOGIN_ERROR_PATH.to_string(),
            use_unified_repo_api_key: DEFAULT_USE_UNIFIED_REPO_API_KEY,
            hash_hostname: DEFAULT_HASH_HOSTNAME,
            proxy_servers: None,
            client_cert: None,
            client_cert_key: None,
            use_device_flow: DEFAULT_USE_DEVICE_FLOW,
            extra_headers: None,
            env_manager_channel: DEFAULT_ENV_MANAGER_CHANNEL.to_string(),
            env_manager_package: DEFAULT_ENV_MANAGER_PACKAGE.to_string(),
            env_manager_version: None,
            #[cfg(feature = "diagnostics")]
            sentry_disabled: false,
            #[cfg(feature = "diagnostics")]
            sentry_environment: DEFAULT_SENTRY_ENVIRONMENT.to_string(),
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
        // Should end with .anaconda/keyring
        assert!(config.keyring_path.ends_with(".anaconda/keyring"));
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

    #[test]
    fn test_config_load_include_prereleases_false_from_env() {
        temp_env::with_var("ANA_PRERELEASES", Some("false"), || {
            let config = Config::load();
            assert!(!config.include_prereleases);
        });
    }

    #[test]
    fn test_config_default_include_prereleases_is_false() {
        temp_env::with_var("ANA_PRERELEASES", None::<&str>, || {
            let config = Config::load();
            assert!(!config.include_prereleases);
        });
    }

    #[test]
    fn test_config_load_include_prereleases_true_from_env() {
        temp_env::with_var("ANA_PRERELEASES", Some("true"), || {
            let config = Config::load();
            assert!(config.include_prereleases);
        });
    }

    #[test]
    fn test_config_default_pip_index_url() {
        temp_env::with_var("ANA_PIP_INDEX_URL", None::<&str>, || {
            let config = Config::load();
            assert_eq!(
                config.pip_index_url,
                "https://repo.anaconda.cloud/repo/anaconda-wheels/simple"
            );
        });
    }

    #[test]
    fn test_config_default_self_update_url() {
        temp_env::with_var("ANA_SELF_UPDATE_URL", None::<&str>, || {
            let config = Config::load();
            assert_eq!(
                config.self_update_url,
                Some(DEFAULT_SELF_UPDATE_URL.to_string())
            );
        });
    }

    #[test]
    fn test_config_load_pip_index_url_from_env() {
        temp_env::with_var(
            "ANA_PIP_INDEX_URL",
            Some("https://custom.example.com/simple/"),
            || {
                let config = Config::load();
                assert_eq!(config.pip_index_url, "https://custom.example.com/simple/");
            },
        );
    }

    #[test]
    fn test_config_self_update_url_custom() {
        temp_env::with_var("ANA_SELF_UPDATE_URL", Some("https://example.com"), || {
            let config = Config::load();
            assert_eq!(
                config.self_update_url,
                Some("https://example.com".to_string())
            );
        });
    }

    #[test]
    fn test_config_self_update_url_github_magic_value() {
        temp_env::with_var("ANA_SELF_UPDATE_URL", Some("github"), || {
            let config = Config::load();
            assert_eq!(config.self_update_url, None);
        });
    }

    #[test]
    fn test_config_self_update_url_github_case_insensitive() {
        temp_env::with_var("ANA_SELF_UPDATE_URL", Some("GitHub"), || {
            let config = Config::load();
            assert_eq!(config.self_update_url, None);
        });
    }

    #[test]
    fn test_normalize_domain_plain() {
        assert_eq!(normalize_domain("anaconda.com"), "anaconda.com");
    }

    #[test]
    fn test_normalize_domain_strips_https() {
        assert_eq!(normalize_domain("https://anaconda.com"), "anaconda.com");
    }

    #[test]
    fn test_normalize_domain_strips_http() {
        assert_eq!(normalize_domain("http://anaconda.com"), "anaconda.com");
    }

    #[test]
    fn test_normalize_domain_strips_path() {
        assert_eq!(normalize_domain("anaconda.com/app"), "anaconda.com");
    }

    #[test]
    fn test_normalize_domain_strips_scheme_and_path() {
        assert_eq!(
            normalize_domain("https://stage.anaconda.com/app"),
            "stage.anaconda.com"
        );
    }

    #[test]
    fn test_normalize_domain_strips_deep_path() {
        assert_eq!(
            normalize_domain("https://example.com/foo/bar/baz"),
            "example.com"
        );
    }

    #[test]
    fn test_normalize_domain_trims_whitespace() {
        assert_eq!(normalize_domain("  anaconda.com  "), "anaconda.com");
    }

    #[test]
    fn test_config_load_domain_strips_path() {
        temp_env::with_var("ANA_DOMAIN", Some("stage.anaconda.com/app"), || {
            let config = Config::load();
            assert_eq!(config.domain, "stage.anaconda.com");
        });
    }

    #[test]
    fn test_config_load_domain_strips_scheme() {
        temp_env::with_var("ANA_DOMAIN", Some("https://stage.anaconda.com"), || {
            let config = Config::load();
            assert_eq!(config.domain, "stage.anaconda.com");
        });
    }

    #[test]
    fn test_config_auth_domain_override_from_env() {
        temp_env::with_var(
            "ANACONDA_AUTH_AUTH_DOMAIN_OVERRIDE",
            Some("sso.example.com"),
            || {
                let config = Config::load();
                assert_eq!(
                    config.auth_domain_override,
                    Some("sso.example.com".to_string())
                );
            },
        );
    }

    #[test]
    fn test_config_auth_domain_override_normalizes_domain() {
        temp_env::with_var(
            "ANACONDA_AUTH_AUTH_DOMAIN_OVERRIDE",
            Some("https://sso.example.com/path"),
            || {
                let config = Config::load();
                assert_eq!(
                    config.auth_domain_override,
                    Some("sso.example.com".to_string())
                );
            },
        );
    }

    #[test]
    fn test_config_auth_domain_returns_override_when_set() {
        let mut config = test_config("anaconda.com", true, true);
        config.auth_domain_override = Some("sso.example.com".to_string());
        assert_eq!(config.auth_domain(), "sso.example.com");
    }

    #[test]
    fn test_config_auth_domain_returns_domain_when_no_override() {
        let config = test_config("anaconda.com", true, true);
        assert_eq!(config.auth_domain(), "anaconda.com");
    }

    #[test]
    fn test_config_well_known_url_uses_auth_domain() {
        let mut config = test_config("anaconda.com", true, true);
        config.auth_domain_override = Some("sso.example.com".to_string());
        assert_eq!(
            config.well_known_url(),
            "https://sso.example.com/.well-known/openid-configuration"
        );
    }

    #[test]
    fn test_config_preferred_token_storage_from_env() {
        temp_env::with_var("ANACONDA_AUTH_PREFERRED_TOKEN_STORAGE", Some("system"), || {
            let config = Config::load();
            assert_eq!(config.preferred_token_storage, "system");
        });
    }

    #[test]
    fn test_config_default_preferred_token_storage() {
        temp_env::with_var("ANACONDA_AUTH_PREFERRED_TOKEN_STORAGE", None::<&str>, || {
            let config = Config::load();
            assert_eq!(config.preferred_token_storage, "anaconda-keyring");
        });
    }

    #[test]
    fn test_config_api_key_from_env() {
        temp_env::with_var("ANACONDA_AUTH_API_KEY", Some("test-api-key"), || {
            let config = Config::load();
            assert_eq!(config.api_key, Some("test-api-key".to_string()));
        });
    }

    #[test]
    fn test_config_api_key_empty_is_none() {
        temp_env::with_var("ANACONDA_AUTH_API_KEY", Some("  "), || {
            let config = Config::load();
            assert_eq!(config.api_key, None);
        });
    }

    #[test]
    fn test_config_use_device_flow_from_env() {
        temp_env::with_var("ANACONDA_AUTH_USE_DEVICE_FLOW", Some("true"), || {
            let config = Config::load();
            assert!(config.use_device_flow);
        });
    }

    #[test]
    fn test_config_default_use_device_flow_is_false() {
        temp_env::with_var("ANACONDA_AUTH_USE_DEVICE_FLOW", None::<&str>, || {
            let config = Config::load();
            assert!(!config.use_device_flow);
        });
    }

    #[test]
    fn test_config_proxy_servers_from_env() {
        temp_env::with_var(
            "ANACONDA_AUTH_PROXY_SERVERS",
            Some(r#"{"https":"http://proxy:8080"}"#),
            || {
                let config = Config::load();
                let proxies = config.proxy_servers.unwrap();
                assert_eq!(proxies.get("https"), Some(&"http://proxy:8080".to_string()));
            },
        );
    }

    #[test]
    fn test_config_client_cert_from_env() {
        temp_env::with_var("ANACONDA_AUTH_CLIENT_CERT", Some("/path/to/cert.pem"), || {
            let config = Config::load();
            assert_eq!(config.client_cert, Some("/path/to/cert.pem".to_string()));
        });
    }

    #[test]
    fn test_config_env_manager_channel_from_env() {
        temp_env::with_var(
            "ANACONDA_AUTH_ENV_MANAGER_CHANNEL",
            Some("custom-channel"),
            || {
                let config = Config::load();
                assert_eq!(config.env_manager_channel, "custom-channel");
            },
        );
    }

    #[test]
    fn test_config_default_env_manager_values() {
        temp_env::with_vars(
            [
                ("ANACONDA_AUTH_ENV_MANAGER_CHANNEL", None::<&str>),
                ("ANACONDA_AUTH_ENV_MANAGER_PACKAGE", None),
                ("ANACONDA_AUTH_ENV_MANAGER_VERSION", None),
            ],
            || {
                let config = Config::load();
                assert_eq!(config.env_manager_channel, "anaconda-cloud");
                assert_eq!(config.env_manager_package, "anaconda-env-manager");
                assert_eq!(config.env_manager_version, None);
            },
        );
    }

    #[test]
    fn test_config_hash_hostname_from_env() {
        temp_env::with_var("ANACONDA_AUTH_HASH_HOSTNAME", Some("false"), || {
            let config = Config::load();
            assert!(!config.hash_hostname);
        });
    }

    #[test]
    fn test_config_default_hash_hostname_is_true() {
        temp_env::with_var("ANACONDA_AUTH_HASH_HOSTNAME", None::<&str>, || {
            let config = Config::load();
            assert!(config.hash_hostname);
        });
    }

    #[test]
    fn test_config_extra_headers_from_env() {
        temp_env::with_var(
            "ANACONDA_AUTH_EXTRA_HEADERS",
            Some(r#"{"X-Custom-Header":"custom-value"}"#),
            || {
                let config = Config::load();
                let headers = config.extra_headers.unwrap();
                assert_eq!(
                    headers.get("X-Custom-Header"),
                    Some(&"custom-value".to_string())
                );
            },
        );
    }

    #[test]
    fn test_config_default_oidc_request_headers() {
        let config = Config::load();
        assert!(config.oidc_request_headers.contains_key("User-Agent"));
        assert!(config
            .oidc_request_headers
            .get("User-Agent")
            .unwrap()
            .starts_with("ana/"));
    }
}
