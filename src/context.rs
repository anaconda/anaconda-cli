//! Command execution context passed through the call stack.
//!
//! Similar to Go's context pattern, this carries cross-cutting concerns
//! (telemetry, config, etc.) without polluting function signatures.

use std::collections::HashMap;
use std::env::consts::{ARCH, OS};

use opentelemetry::Value;

use crate::VERSION;
use crate::http::Client;

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
    /// HTTP client for API requests.
    pub client: Client,
}

impl CommandContext {
    /// Create a new command context.
    pub fn new() -> Self {
        let client = Client::from_config().expect("failed to create HTTP client");
        Self {
            telemetry: TelemetryContext::new(),
            client,
        }
    }
}

impl Default for CommandContext {
    fn default() -> Self {
        Self::new()
    }
}
