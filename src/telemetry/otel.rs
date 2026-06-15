//! OpenTelemetry integration for telemetry submission.
//!
//! This module consolidates all anaconda_otel_rs interactions:
//! - Value type conversions
//! - Telemetry initialization
//! - Metric submission

use std::collections::HashMap;

use anaconda_otel_rs::{
    attributes::ResourceAttributes,
    config::Configuration,
    signals::{increment_counter, initialize_telemetry, record_histogram, shutdown_telemetry},
};
use opentelemetry::Value;
use serde::{Deserialize, Serialize};

use crate::VERSION;

/// A value type that can be serialized and converted to/from OpenTelemetry Value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum SerializableValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

impl From<Value> for SerializableValue {
    fn from(v: Value) -> Self {
        match v {
            Value::String(s) => SerializableValue::String(s.to_string()),
            Value::I64(i) => SerializableValue::Int(i),
            Value::F64(f) => SerializableValue::Float(f),
            Value::Bool(b) => SerializableValue::Bool(b),
            _ => SerializableValue::String(format!("{:?}", v)),
        }
    }
}

impl From<SerializableValue> for Value {
    fn from(v: SerializableValue) -> Self {
        match v {
            SerializableValue::String(s) => Value::String(s.into()),
            SerializableValue::Int(i) => Value::I64(i),
            SerializableValue::Float(f) => Value::F64(f),
            SerializableValue::Bool(b) => Value::Bool(b),
        }
    }
}

/// Initialize the OpenTelemetry telemetry system.
pub fn setup() {
    if !crate::config::telemetry_enabled() {
        return;
    }
    let _ = try_setup();
}

fn try_setup() -> Result<(), Box<dyn std::error::Error>> {
    let app_config = crate::config::Config::load();

    let api_key = crate::auth::get_api_key(&app_config).ok().flatten();

    let endpoint = if api_key.is_some() {
        &app_config.metrics_endpoint
    } else {
        &app_config.metrics_public_endpoint
    };

    let mut otel_config = Configuration::new(Some(endpoint), None)?;

    if let Some(key) = api_key {
        otel_config.set_auth_token(Some(key));
    }
    otel_config.set_console_exporter(app_config.metrics_console_exporter);
    otel_config.set_metrics_export_interval_ms(app_config.metrics_export_interval_ms);
    otel_config.skip_internet_check = app_config.metrics_skip_internet_check;

    let attrs = ResourceAttributes::new("ana-cli", VERSION)?;

    initialize_telemetry(otel_config, attrs, vec!["metrics"])
        .map_err(|e| format!("Telemetry initialization failed: {}", e))?;

    Ok(())
}

/// Shutdown the OpenTelemetry telemetry system.
pub fn shutdown() {
    shutdown_telemetry();
}

/// Submit a counter metric to OpenTelemetry.
pub fn submit_counter(name: &str, value: u64, attrs: HashMap<String, SerializableValue>) {
    let otel_attrs: HashMap<String, Value> =
        attrs.into_iter().map(|(k, v)| (k, v.into())).collect();
    increment_counter(name, value, otel_attrs);
}

/// Submit a histogram metric to OpenTelemetry.
pub fn submit_histogram(name: &str, value: f64, attrs: HashMap<String, SerializableValue>) {
    let otel_attrs: HashMap<String, Value> =
        attrs.into_iter().map(|(k, v)| (k, v.into())).collect();
    record_histogram(name, value, otel_attrs);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serializable_value_from_otel() {
        let string_val = Value::String("test".into());
        let int_val = Value::I64(42);
        let float_val = Value::F64(3.125);
        let bool_val = Value::Bool(true);

        assert_eq!(
            SerializableValue::from(string_val),
            SerializableValue::String("test".to_string())
        );
        assert_eq!(SerializableValue::from(int_val), SerializableValue::Int(42));
        assert_eq!(
            SerializableValue::from(float_val),
            SerializableValue::Float(3.125)
        );
        assert_eq!(
            SerializableValue::from(bool_val),
            SerializableValue::Bool(true)
        );
    }

    #[test]
    fn test_serializable_value_roundtrip() {
        let values = vec![
            SerializableValue::String("hello".to_string()),
            SerializableValue::Int(42),
            SerializableValue::Float(3.125),
            SerializableValue::Bool(true),
        ];

        for original in values {
            let otel: Value = original.clone().into();
            let back: SerializableValue = otel.into();
            assert_eq!(original, back);
        }
    }
}
