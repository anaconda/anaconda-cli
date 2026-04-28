//! Portable telemetry event types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::otel::SerializableValue;

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

}
