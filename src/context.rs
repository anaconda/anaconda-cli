//! Command execution context passed through the call stack.
//!
//! Similar to Go's context pattern, this carries cross-cutting concerns
//! (telemetry, config, etc.) without polluting function signatures.

use std::collections::HashMap;
use std::env::consts::{ARCH, OS};

use opentelemetry::Value;

use crate::config::Config;
use crate::VERSION;

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
#[derive(Debug)]
pub struct CommandContext {
    /// Telemetry attributes collector.
    pub telemetry: TelemetryContext,
    /// Application configuration.
    pub config: Config,
}

impl CommandContext {
    /// Create a new command context.
    pub fn new() -> Self {
        Self {
            telemetry: TelemetryContext::new(),
            config: Config::load(),
        }
    }
}

impl Default for CommandContext {
    fn default() -> Self {
        Self::new()
    }
}
