//! Submit pending telemetry batches to the metrics endpoint.

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use anaconda_otel_rs::signals::{increment_counter, record_histogram, shutdown_telemetry};

use super::event::{TelemetryBatch, TelemetryEvent};

/// Submit all pending telemetry batches.
///
/// Called by the detached telemetry-submit subprocess.
pub fn submit_pending() -> Result<(), Box<dyn std::error::Error>> {
    let pending_dir = super::pending_dir();

    if !pending_dir.exists() {
        return Ok(());
    }

    crate::config::setup_telemetry();

    let entries: Vec<_> = fs::read_dir(&pending_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
        .collect();

    for entry in entries {
        let path = entry.path();

        match submit_batch_file(&path) {
            Ok(()) => {
                if let Err(e) = fs::remove_file(&path) {
                    tracing::warn!("Failed to delete spool file {:?}: {}", path, e);
                }
            }
            Err(e) => {
                tracing::warn!("Failed to submit telemetry batch {:?}: {}", path, e);
            }
        }
    }

    cleanup_old_files(&pending_dir, 7)?;

    shutdown_telemetry();

    Ok(())
}

fn submit_batch_file(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let batch: TelemetryBatch = serde_json::from_str(&content)?;

    for event in batch.events {
        let attrs = match &event {
            TelemetryEvent::Counter { attributes, .. } => attributes,
            TelemetryEvent::Histogram { attributes, .. } => attributes,
        };

        let otel_attrs: std::collections::HashMap<String, opentelemetry::Value> = attrs
            .iter()
            .map(|(k, v)| (k.clone(), v.clone().into()))
            .collect();

        match event {
            TelemetryEvent::Counter { name, value, .. } => {
                increment_counter(&name, value, otel_attrs);
            }
            TelemetryEvent::Histogram { name, value, .. } => {
                record_histogram(&name, value, otel_attrs);
            }
        }
    }

    Ok(())
}

fn cleanup_old_files(dir: &Path, max_age_days: i64) -> Result<(), std::io::Error> {
    let max_age = Duration::from_secs((max_age_days * 24 * 60 * 60) as u64);
    let now = SystemTime::now();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let metadata = entry.metadata()?;

        if let Ok(modified) = metadata.modified() {
            if let Ok(age) = now.duration_since(modified) {
                if age > max_age {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::Write;

    use crate::telemetry::event::SerializableValue;

    #[test]
    fn test_cleanup_old_files_keeps_recent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let new_file = temp_dir.path().join("new.json");

        fs::write(&new_file, "{}").unwrap();

        cleanup_old_files(temp_dir.path(), 7).unwrap();

        assert!(new_file.exists(), "Recent file should remain");
    }

    #[test]
    fn test_submit_batch_file_parses_correctly() {
        let temp_dir = tempfile::tempdir().unwrap();
        let batch_file = temp_dir.path().join("batch.json");

        let mut attrs = HashMap::new();
        attrs.insert(
            "command".to_string(),
            SerializableValue::String("test".to_string()),
        );

        let batch = TelemetryBatch {
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            version: "0.1.0".to_string(),
            events: vec![TelemetryEvent::Counter {
                name: "test_counter".to_string(),
                value: 1,
                attributes: attrs,
            }],
        };

        let mut file = fs::File::create(&batch_file).unwrap();
        file.write_all(serde_json::to_string(&batch).unwrap().as_bytes())
            .unwrap();
        drop(file);

        // Note: submit_batch_file calls increment_counter which requires telemetry setup
        // This test just verifies parsing works
        let content = fs::read_to_string(&batch_file).unwrap();
        let parsed: TelemetryBatch = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed.events.len(), 1);
    }
}
