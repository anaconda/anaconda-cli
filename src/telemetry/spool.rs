//! Spool telemetry batches to disk for deferred submission.

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::event::{TelemetryBatch, TelemetryEvent};

/// Write a batch of events to a spool file.
///
/// Uses atomic write (temp file + rename) for crash safety.
/// Returns the path to the written file.
pub fn write_batch(events: Vec<TelemetryEvent>, version: &str) -> io::Result<PathBuf> {
    let pending_dir = super::pending_dir();
    fs::create_dir_all(&pending_dir)?;

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let timestamp = chrono::Utc::now().to_rfc3339();

    let batch = TelemetryBatch {
        timestamp,
        version: version.to_string(),
        events,
    };

    let filename = format!("{}.json", nanos);
    let final_path = pending_dir.join(&filename);
    let temp_path = pending_dir.join(format!("{}.tmp", nanos));

    let content = serde_json::to_string_pretty(&batch)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let mut file = fs::File::create(&temp_path)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    drop(file);

    fs::rename(&temp_path, &final_path)?;

    Ok(final_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::telemetry::otel::SerializableValue;

    #[test]
    fn test_write_batch_creates_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        temp_env::with_var("ANA_HOME", Some(temp_dir.path()), || {
            let mut attrs = HashMap::new();
            attrs.insert(
                "command".to_string(),
                SerializableValue::String("test".to_string()),
            );

            let events = vec![TelemetryEvent::Counter {
                name: "test_counter".to_string(),
                value: 1,
                attributes: attrs,
            }];

            let path = write_batch(events, "0.1.0").unwrap();
            assert!(path.exists());
            assert!(path.extension().unwrap() == "json");

            let content = fs::read_to_string(&path).unwrap();
            let batch: TelemetryBatch = serde_json::from_str(&content).unwrap();
            assert_eq!(batch.version, "0.1.0");
            assert_eq!(batch.events.len(), 1);
        });
    }

    #[test]
    fn test_write_batch_no_temp_files_left() {
        let temp_dir = tempfile::tempdir().unwrap();
        temp_env::with_var("ANA_HOME", Some(temp_dir.path()), || {
            let events = vec![TelemetryEvent::Counter {
                name: "test".to_string(),
                value: 1,
                attributes: HashMap::new(),
            }];

            write_batch(events, "0.1.0").unwrap();

            let pending_dir = super::super::pending_dir();
            let tmp_files: Vec<_> = fs::read_dir(&pending_dir)
                .unwrap()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map_or(false, |ext| ext == "tmp"))
                .collect();

            assert!(tmp_files.is_empty(), "No .tmp files should remain");
        });
    }
}
