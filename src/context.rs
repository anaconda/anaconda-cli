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

/// Telemetry context for collecting command-specific attributes.
#[derive(Debug, Default)]
pub struct TelemetryContext {
    attrs: HashMap<String, Value>,
}

impl TelemetryContext {
    /// Create a new telemetry context with system attributes pre-populated.
    fn new() -> Self {
        let mut attrs = HashMap::new();
        attrs.insert("os".to_string(), OS.into());
        attrs.insert("arch".to_string(), ARCH.into());
        attrs.insert("version".to_string(), VERSION.into());
        Self { attrs }
    }

    /// Add an attribute.
    pub fn add(&mut self, key: impl Into<String>, value: impl Into<Value>) {
        self.attrs.insert(key.into(), value.into());
    }

    /// Get the attributes for recording.
    pub(crate) fn into_attrs(self) -> HashMap<String, Value> {
        self.attrs
    }

    /// Clone attributes for intermediate recording.
    pub(crate) fn attrs(&self) -> HashMap<String, Value> {
        self.attrs.clone()
    }
}

/// Command execution context passed through the call stack.
pub struct CommandContext {
    /// Telemetry attributes collector.
    pub telemetry: TelemetryContext,
    /// Configuration.
    pub config: Config,
    /// HTTP client for API requests.
    pub client: Client,
    /// GitHub API client (lazy initialized).
    github_client: OnceLock<reqwest_middleware::ClientWithMiddleware>,
    /// Download client (lazy initialized).
    download_client: OnceLock<reqwest_middleware::ClientWithMiddleware>,
    /// Unauthenticated client (lazy initialized).
    unauthenticated_client: OnceLock<reqwest_middleware::ClientWithMiddleware>,
}

impl CommandContext {
    /// Create a new command context.
    pub fn new() -> Self {
        let config = Config::load();
        let client = Client::from_config(&config).expect("failed to create HTTP client");
        Self {
            telemetry: TelemetryContext::new(),
            config,
            client,
            github_client: OnceLock::new(),
            download_client: OnceLock::new(),
            unauthenticated_client: OnceLock::new(),
        }
    }

    /// Get or create a GitHub API client (uses GITHUB_TOKEN).
    /// Returns None if GITHUB_TOKEN is not set or client creation fails.
    pub fn github_client(&self) -> Option<&reqwest_middleware::ClientWithMiddleware> {
        if self.github_client.get().is_none() {
            if let Some(client) = env::var("GITHUB_TOKEN")
                .ok()
                .filter(|t| !t.is_empty())
                .and_then(|token| {
                    http::build_client(
                        reqwest::Client::builder().default_headers(http::bearer_header(&token)),
                    )
                    .ok()
                })
            {
                let _ = self.github_client.set(client);
            }
        }
        self.github_client.get()
    }

    /// Get or create a download client optimized for binary downloads (no gzip).
    pub fn download_client(&self) -> Option<&reqwest_middleware::ClientWithMiddleware> {
        if self.download_client.get().is_none() {
            if let Some(client) = http::build_client(reqwest::Client::builder().no_gzip()).ok() {
                let _ = self.download_client.set(client);
            }
        }
        self.download_client.get()
    }

    /// Get or create an unauthenticated client with a timeout.
    /// Used for login flows where the user isn't authenticated yet.
    pub fn unauthenticated_client(
        &self,
        timeout: Duration,
    ) -> Option<&reqwest_middleware::ClientWithMiddleware> {
        if self.unauthenticated_client.get().is_none() {
            if let Some(client) =
                http::build_client(reqwest::Client::builder().timeout(timeout)).ok()
            {
                let _ = self.unauthenticated_client.set(client);
            }
        }
        self.unauthenticated_client.get()
    }
}

impl Default for CommandContext {
    fn default() -> Self {
        Self::new()
    }
}
