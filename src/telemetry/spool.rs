//! Spool telemetry batches to disk for deferred submission.

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::event::{TelemetryBatch, TelemetryEvent};

/// Maximum number of pending spool files to keep.
const MAX_PENDING_FILES: usize = 100;

/// Write a batch of events to a spool file.
///
/// Uses atomic write (temp file + rename) for crash safety.
/// Enforces MAX_PENDING_FILES limit by deleting oldest files.
/// Returns the path to the written file.
pub fn write_batch(events: Vec<TelemetryEvent>, version: &str) -> io::Result<PathBuf> {
    let pending_dir = super::pending_dir();
    fs::create_dir_all(&pending_dir)?;

    // Enforce max file count before writing
    enforce_max_files(&pending_dir, MAX_PENDING_FILES)?;

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

/// Enforce maximum file count by deleting oldest files.
fn enforce_max_files(dir: &PathBuf, max_files: usize) -> io::Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
        .collect();

    if entries.len() < max_files {
        return Ok(());
    }

    // Sort by filename (which is nanosecond timestamp) - oldest first
    entries.sort_by_key(|e| e.file_name());

    // Delete oldest files to make room
    let to_delete = entries.len() - max_files + 1; // +1 for the new file we're about to write
    for entry in entries.into_iter().take(to_delete) {
        let _ = fs::remove_file(entry.path());
    }

    Ok(())
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

    #[test]
    fn test_enforce_max_files_deletes_oldest() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pending_dir = temp_dir.path().join("telemetry").join("pending");
        fs::create_dir_all(&pending_dir).unwrap();

        // Create 5 files with sequential names
        for i in 0..5 {
            let path = pending_dir.join(format!("{}.json", i));
            fs::write(&path, "{}").unwrap();
        }

        // Enforce max of 3 files (need room for 1 new file, so keep 2)
        enforce_max_files(&pending_dir, 3).unwrap();

        let remaining: Vec<_> = fs::read_dir(&pending_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();

        // Should have deleted 3 oldest (0, 1, 2), keeping 2 (3, 4)
        assert_eq!(remaining.len(), 2);
        assert!(pending_dir.join("3.json").exists());
        assert!(pending_dir.join("4.json").exists());
    }

    #[test]
    fn test_enforce_max_files_no_op_when_under_limit() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pending_dir = temp_dir.path().join("telemetry").join("pending");
        fs::create_dir_all(&pending_dir).unwrap();

        // Create 2 files
        for i in 0..2 {
            let path = pending_dir.join(format!("{}.json", i));
            fs::write(&path, "{}").unwrap();
        }

        // Enforce max of 5 files - should do nothing
        enforce_max_files(&pending_dir, 5).unwrap();

        let remaining: Vec<_> = fs::read_dir(&pending_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();

        assert_eq!(remaining.len(), 2);
    }

    #[test]
    fn test_enforce_max_files_ignores_non_json() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pending_dir = temp_dir.path().join("telemetry").join("pending");
        fs::create_dir_all(&pending_dir).unwrap();

        // Create json files and non-json files
        for i in 0..3 {
            fs::write(pending_dir.join(format!("{}.json", i)), "{}").unwrap();
        }
        fs::write(pending_dir.join("test.txt"), "not json").unwrap();
        fs::write(pending_dir.join("123.tmp"), "temp").unwrap();

        // Enforce max of 2 json files
        enforce_max_files(&pending_dir, 2).unwrap();

        // Non-json files should still exist
        assert!(pending_dir.join("test.txt").exists());
        assert!(pending_dir.join("123.tmp").exists());

        // Only newest json file should remain (plus one slot for new file)
        let json_files: Vec<_> = fs::read_dir(&pending_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
            .collect();
        assert_eq!(json_files.len(), 1);
    }

    #[test]
    fn test_enforce_max_files_empty_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pending_dir = temp_dir.path().join("telemetry").join("pending");
        fs::create_dir_all(&pending_dir).unwrap();

        // Should handle empty directory without error
        let result = enforce_max_files(&pending_dir, 10);
        assert!(result.is_ok());
    }

    #[test]
    fn test_write_batch_multiple_events() {
        let temp_dir = tempfile::tempdir().unwrap();
        temp_env::with_var("ANA_HOME", Some(temp_dir.path()), || {
            let events = vec![
                TelemetryEvent::Counter {
                    name: "counter1".to_string(),
                    value: 1,
                    attributes: HashMap::new(),
                },
                TelemetryEvent::Counter {
                    name: "counter2".to_string(),
                    value: 2,
                    attributes: HashMap::new(),
                },
                TelemetryEvent::Histogram {
                    name: "histogram1".to_string(),
                    value: 3.14,
                    attributes: HashMap::new(),
                },
            ];

            let path = write_batch(events, "0.1.0").unwrap();
            let content = fs::read_to_string(&path).unwrap();
            let batch: TelemetryBatch = serde_json::from_str(&content).unwrap();

            assert_eq!(batch.events.len(), 3);
        });
    }

    #[test]
    fn test_write_batch_empty_events() {
        let temp_dir = tempfile::tempdir().unwrap();
        temp_env::with_var("ANA_HOME", Some(temp_dir.path()), || {
            let events: Vec<TelemetryEvent> = vec![];

            let path = write_batch(events, "0.1.0").unwrap();
            let content = fs::read_to_string(&path).unwrap();
            let batch: TelemetryBatch = serde_json::from_str(&content).unwrap();

            assert_eq!(batch.events.len(), 0);
        });
    }

    #[test]
    fn test_write_batch_contains_timestamp() {
        let temp_dir = tempfile::tempdir().unwrap();
        temp_env::with_var("ANA_HOME", Some(temp_dir.path()), || {
            let events = vec![TelemetryEvent::Counter {
                name: "test".to_string(),
                value: 1,
                attributes: HashMap::new(),
            }];

            let path = write_batch(events, "0.1.0").unwrap();
            let content = fs::read_to_string(&path).unwrap();
            let batch: TelemetryBatch = serde_json::from_str(&content).unwrap();

            // Timestamp should be RFC3339 format
            assert!(!batch.timestamp.is_empty());
            assert!(batch.timestamp.contains("T")); // ISO format contains T separator
        });
    }
}
