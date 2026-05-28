//! Command execution context passed through the call stack.
//!
//! Similar to Go's context pattern, this carries cross-cutting concerns
//! (telemetry, config, etc.) without polluting function signatures.

use std::collections::HashMap;
use std::env;
use std::env::consts::{ARCH, OS};
use std::sync::OnceLock;
use std::time::Duration;

use opentelemetry::Value;

use crate::VERSION;
use crate::config::Config;
use crate::http::{self, Client};
use crate::telemetry::{SerializableValue, TelemetryEvent};

/// Telemetry context for collecting command-specific attributes.
#[derive(Debug, Default)]
pub struct TelemetryContext {
    attrs: HashMap<String, Value>,
    events: Vec<TelemetryEvent>,
}

impl TelemetryContext {
    /// Create a new telemetry context with system attributes pre-populated.
    fn new() -> Self {
        let mut attrs = HashMap::new();
        attrs.insert("os".to_string(), OS.into());
        attrs.insert("arch".to_string(), ARCH.into());
        attrs.insert("version".to_string(), VERSION.into());
        Self {
            attrs,
            events: Vec::new(),
        }
    }

    /// Add an attribute.
    pub fn add(&mut self, key: impl Into<String>, value: impl Into<Value>) {
        self.attrs.insert(key.into(), value.into());
    }

    /// Record a counter metric (buffered locally).
    pub fn record_counter(&mut self, name: &str, value: u64) {
        let attrs = self
            .attrs
            .iter()
            .map(|(k, v)| (k.clone(), SerializableValue::from(v.clone())))
            .collect();

        self.events.push(TelemetryEvent::Counter {
            name: name.to_string(),
            value,
            attributes: attrs,
        });
    }

    /// Record a histogram metric (buffered locally).
    pub fn record_histogram(&mut self, name: &str, value: f64) {
        let attrs = self
            .attrs
            .iter()
            .map(|(k, v)| (k.clone(), SerializableValue::from(v.clone())))
            .collect();

        self.events.push(TelemetryEvent::Histogram {
            name: name.to_string(),
            value,
            attributes: attrs,
        });
    }

    /// Flush buffered events to spool file.
    ///
    /// Returns Ok(None) if telemetry is disabled or no events.
    pub fn flush_to_spool(self) -> std::io::Result<Option<std::path::PathBuf>> {
        if self.events.is_empty() {
            return Ok(None);
        }

        if !crate::config::telemetry_enabled() {
            return Ok(None);
        }

        crate::telemetry::write_batch(self.events, VERSION).map(Some)
    }
}

/// Command execution context passed through the call stack.
pub struct CommandContext {
    /// Telemetry attributes collector.
    pub telemetry: TelemetryContext,
    /// Configuration.
    pub config: Config,
    /// HTTP client for API requests (lazy initialized).
    client: OnceLock<Client>,
    /// GitHub API client (lazy initialized).
    #[cfg_attr(not(feature = "self-update"), allow(dead_code))]
    github_client: OnceLock<reqwest_middleware::ClientWithMiddleware>,
    /// Download client (lazy initialized).
    #[cfg_attr(not(feature = "self-update"), allow(dead_code))]
    download_client: OnceLock<reqwest_middleware::ClientWithMiddleware>,
    /// Unauthenticated client (lazy initialized).
    unauthenticated_client: OnceLock<Client>,
}

impl CommandContext {
    /// Create a new command context.
    pub fn new() -> Self {
        let config = Config::load();
        let mut telemetry = TelemetryContext::new();

        // Add cached user_id to telemetry if available
        if let Ok(Some(user_id)) = crate::auth::get_user_id(&config) {
            telemetry.add("user_id", user_id);
        }

        // Add AAU tokens for correlation with anonymous usage tracking
        if let Some(token) = crate::ua::client_token() {
            telemetry.add("client_token", token);
        }
        if let Some(token) = crate::ua::session_token() {
            telemetry.add("session_token", token);
        }

        Self {
            telemetry,
            config,
            client: OnceLock::new(),
            github_client: OnceLock::new(),
            download_client: OnceLock::new(),
            unauthenticated_client: OnceLock::new(),
        }
    }

    /// Get the main HTTP client (authenticated with API key if available).
    pub fn client(&self) -> &Client {
        self.client.get_or_init(|| {
            Client::from_config(&self.config).expect("failed to create HTTP client")
        })
    }

    /// Get or create a GitHub API client.
    /// Uses GITHUB_TOKEN for authentication if available (higher rate limits).
    #[cfg_attr(not(feature = "self-update"), allow(dead_code))]
    pub fn github_client(&self) -> &reqwest_middleware::ClientWithMiddleware {
        self.github_client.get_or_init(|| {
            let builder = reqwest::Client::builder();
            let builder = match env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty()) {
                Some(token) => builder.default_headers(http::bearer_header(&token)),
                None => builder,
            };
            http::build_client(builder).expect("failed to create GitHub client")
        })
    }

    /// Get or create a download client optimized for binary downloads (no gzip).
    #[cfg_attr(not(feature = "self-update"), allow(dead_code))]
    pub fn download_client(&self) -> &reqwest_middleware::ClientWithMiddleware {
        self.download_client.get_or_init(|| {
            http::build_client(reqwest::Client::builder().no_gzip())
                .expect("failed to create download client")
        })
    }

    /// Get or create an unauthenticated client with a timeout.
    /// Used for login flows where the user isn't authenticated yet.
    pub fn unauthenticated_client(&self, timeout: Duration) -> &Client {
        self.unauthenticated_client.get_or_init(|| {
            Client::new(
                reqwest::Client::builder().timeout(timeout),
                self.config.base_url(),
            )
            .expect("failed to create unauthenticated client")
        })
    }
}

impl Default for CommandContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl CommandContext {
    /// Create a test context with a custom config and pre-built client.
    pub fn with_client(config: Config, client: Client) -> Self {
        let telemetry = TelemetryContext::new();
        let client_lock = OnceLock::new();
        client_lock.set(client).ok();
        Self {
            telemetry,
            config,
            client: client_lock,
            github_client: OnceLock::new(),
            download_client: OnceLock::new(),
            unauthenticated_client: OnceLock::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_context_includes_aau_tokens() {
        let ctx = CommandContext::new();

        // Record a counter to capture attributes
        let mut telemetry = ctx.telemetry;
        telemetry.record_counter("test_metric", 1);

        // Check that the event has client_token and session_token
        assert_eq!(telemetry.events.len(), 1);
        if let TelemetryEvent::Counter { attributes, .. } = &telemetry.events[0] {
            assert!(
                attributes.contains_key("client_token"),
                "missing client_token in attributes: {:?}",
                attributes.keys().collect::<Vec<_>>()
            );
            assert!(
                attributes.contains_key("session_token"),
                "missing session_token in attributes: {:?}",
                attributes.keys().collect::<Vec<_>>()
            );
        } else {
            panic!("expected Counter event");
        }
    }
}
