//! User-agent string construction for ana HTTP requests.
//!
//! Combines platform identification with AAU (Anaconda Anonymous Usage)
//! telemetry tokens from the `anaconda-anon-usage` crate.
//!
//! Format: `ana/{version} {platform} rattler/{version} {aau_tokens}`
//!
//! Example (macOS): `ana/0.1.0 Darwin/25.2.0 OSX/26.2 rattler/0.40.3 aau/0.7.6 c/... s/...`

mod platform;

use std::sync::{LazyLock, OnceLock};

use crate::VERSION;

const RATTLER_VERSION: &str = env!("RATTLER_VERSION");

/// Global environment prefix for AAU token generation.
///
/// Set once per process via `set_env_prefix()` when the target project
/// environment is known. Falls back to `CONDA_PREFIX` if unset.
static ENV_PREFIX: OnceLock<String> = OnceLock::new();

/// Set the environment prefix for AAU token generation.
///
/// Call this early in command dispatch when the target project manifest is
/// known. Subsequent calls are ignored (first write wins).
pub fn set_env_prefix(prefix: impl Into<String>) {
    let _ = ENV_PREFIX.set(prefix.into());
}

/// Get the environment prefix, if one was set.
pub fn env_prefix() -> Option<&'static str> {
    ENV_PREFIX.get().map(|s| s.as_str())
}

/// Build the AAU config, resolving the JWT from the auth module.
fn aau_config() -> anaconda_anon_usage::Config {
    let config = crate::config::Config::load();
    let jwt = match crate::auth::get_api_key(&config) {
        Ok(Some(api_key)) => Some(api_key),
        Ok(None) => {
            tracing::debug!("No API key available");
            None
        }
        Err(e) => {
            tracing::debug!("Error reading API key: {}", e);
            None
        }
    };
    anaconda_anon_usage::Config {
        env_prefix: env_prefix().map(|s| s.to_string()),
        anaconda_jwt: jwt,
        prefix: Some(base_user_agent().to_string()),
        platform: true,
        rattler_version: Some(RATTLER_VERSION.to_string()),
        reqwest_version: None,
    }
}

/// Return the base user-agent string (without AAU tokens).
fn base_user_agent() -> &'static str {
    static USER_AGENT: LazyLock<String> = LazyLock::new(|| {
        format!(
            "ana/{} {} rattler/{}",
            VERSION,
            platform::platform_string(),
            RATTLER_VERSION
        )
    });
    &USER_AGENT
}

/// Build the full user-agent string for ana HTTP requests.
///
/// Includes platform info and AAU identity tokens. Uses the global env
/// prefix (set via `set_env_prefix()`) for the environment token, falling
/// back to `CONDA_PREFIX` if unset.
///
/// The result is cached after first call — the user-agent is immutable
/// for the lifetime of the process.
///
/// Format: `ana/{version} {platform} rattler/{version} {aau_tokens}`
pub fn user_agent() -> &'static str {
    static UA: OnceLock<String> = OnceLock::new();
    UA.get_or_init(|| anaconda_anon_usage::token_string(&aau_config()))
}

/// Flush any deferred token writes to disk.
pub fn finalize_deferred_writes() -> std::result::Result<(), anaconda_anon_usage::Error> {
    anaconda_anon_usage::finalize_deferred_writes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_user_agent_starts_with_ana() {
        let ua = base_user_agent();
        assert!(ua.starts_with("ana/"), "expected ana/ prefix, got: {}", ua);
    }

    #[test]
    fn test_base_user_agent_contains_rattler() {
        let ua = base_user_agent();
        assert!(
            ua.contains("rattler/"),
            "expected rattler/ in UA, got: {}",
            ua
        );
    }

    #[test]
    fn test_base_user_agent_print() {
        let ua = base_user_agent();
        eprintln!("User-Agent: {}", ua);
        assert!(!ua.is_empty());
    }

    #[test]
    fn user_agent_contains_ana_version() {
        let ua = user_agent();
        assert!(ua.starts_with("ana/"), "expected ana/ prefix, got: {}", ua);
        assert!(ua.contains(crate::VERSION));
    }

    #[test]
    fn user_agent_contains_aau_version() {
        let ua = user_agent();
        assert!(
            ua.contains(&format!("aau/{}", anaconda_anon_usage::VERSION)),
            "expected aau version in: {}",
            ua
        );
    }

    #[test]
    fn user_agent_has_identity_tokens() {
        let ua = user_agent();
        assert!(ua.contains(" c/"), "UA should have client token: {}", ua);
        assert!(ua.contains(" s/"), "UA should have session token: {}", ua);
    }

    #[test]
    fn user_agent_contains_platform_info() {
        let ua = user_agent();
        assert!(ua.contains("rattler/"), "expected rattler in UA: {}", ua);
    }

    #[test]
    fn user_agent_tokens_are_valid_base64url() {
        let ua = user_agent();
        // Find the aau/ segment and validate all subsequent tokens
        let parts: Vec<&str> = ua.split_whitespace().collect();
        let aau_idx = parts.iter().position(|p| p.starts_with("aau/"));
        assert!(aau_idx.is_some(), "no aau/ in UA: {}", ua);
        for part in &parts[aau_idx.unwrap() + 1..] {
            let (prefix, value) = part.split_once('/').unwrap();
            assert_eq!(prefix.len(), 1, "unexpected prefix: {}", prefix);
            assert!(
                !value.is_empty() && value.len() <= 36,
                "token {}/{} has invalid length {}",
                prefix,
                value,
                value.len()
            );
            assert!(
                value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "token {}/{} contains invalid characters",
                prefix,
                value
            );
        }
    }
}
