//! Portable telemetry event types.
//!
//! These types have no ana-cli dependencies and can be moved to anaconda-otel-rs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single telemetry event (counter or histogram).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum TelemetryEvent {
    Counter {
        name: String,
        value: u64,
        attributes: HashMap<String, SerializableValue>,
    },
    Histogram {
        name: String,
        value: f64,
        attributes: HashMap<String, SerializableValue>,
    },
}

/// A value type that can be serialized and converted to/from OpenTelemetry Value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum SerializableValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

impl From<opentelemetry::Value> for SerializableValue {
    fn from(v: opentelemetry::Value) -> Self {
        match v {
            opentelemetry::Value::String(s) => SerializableValue::String(s.to_string()),
            opentelemetry::Value::I64(i) => SerializableValue::Int(i),
            opentelemetry::Value::F64(f) => SerializableValue::Float(f),
            opentelemetry::Value::Bool(b) => SerializableValue::Bool(b),
            _ => SerializableValue::String(format!("{:?}", v)),
        }
    }
}

impl From<SerializableValue> for opentelemetry::Value {
    fn from(v: SerializableValue) -> Self {
        match v {
            SerializableValue::String(s) => opentelemetry::Value::String(s.into()),
            SerializableValue::Int(i) => opentelemetry::Value::I64(i),
            SerializableValue::Float(f) => opentelemetry::Value::F64(f),
            SerializableValue::Bool(b) => opentelemetry::Value::Bool(b),
        }
    }
}

/// A batch of telemetry events with metadata.
#[derive(Debug, Serialize, Deserialize)]
pub struct TelemetryBatch {
    pub timestamp: String,
    pub version: String,
    pub events: Vec<TelemetryEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_serialization_roundtrip() {
        let mut attrs = HashMap::new();
        attrs.insert(
            "command".to_string(),
            SerializableValue::String("test".to_string()),
        );
        attrs.insert("count".to_string(), SerializableValue::Int(42));

        let event = TelemetryEvent::Counter {
            name: "cli_command_invoked".to_string(),
            value: 1,
            attributes: attrs,
        };

        let json = serde_json::to_string(&event).unwrap();
        let parsed: TelemetryEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }

    #[test]
    fn test_histogram_serialization_roundtrip() {
        let mut attrs = HashMap::new();
        attrs.insert(
            "os".to_string(),
            SerializableValue::String("macos".to_string()),
        );

        let event = TelemetryEvent::Histogram {
            name: "cli_command_duration_ms".to_string(),
            value: 123.45,
            attributes: attrs,
        };

        let json = serde_json::to_string(&event).unwrap();
        let parsed: TelemetryEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }

    #[test]
    fn test_batch_serialization() {
        let batch = TelemetryBatch {
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            version: "0.1.0".to_string(),
            events: vec![TelemetryEvent::Counter {
                name: "test".to_string(),
                value: 1,
                attributes: HashMap::new(),
            }],
        };

        let json = serde_json::to_string_pretty(&batch).unwrap();
        let parsed: TelemetryBatch = serde_json::from_str(&json).unwrap();
        assert_eq!(batch.timestamp, parsed.timestamp);
        assert_eq!(batch.version, parsed.version);
        assert_eq!(batch.events.len(), 1);
    }

    #[test]
    fn test_serializable_value_from_otel() {
        let string_val = opentelemetry::Value::String("test".into());
        let int_val = opentelemetry::Value::I64(42);
        let float_val = opentelemetry::Value::F64(3.14);
        let bool_val = opentelemetry::Value::Bool(true);

        assert_eq!(
            SerializableValue::from(string_val),
            SerializableValue::String("test".to_string())
        );
        assert_eq!(SerializableValue::from(int_val), SerializableValue::Int(42));
        assert_eq!(
            SerializableValue::from(float_val),
            SerializableValue::Float(3.14)
        );
        assert_eq!(
            SerializableValue::from(bool_val),
            SerializableValue::Bool(true)
        );
    }
}
