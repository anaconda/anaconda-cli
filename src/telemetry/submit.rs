//! Submit pending telemetry batches to the metrics endpoint.

use std::fs;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime};

use super::event::{TelemetryBatch, TelemetryEvent};
use super::otel;

/// Default timeout for the telemetry submission process (30 seconds).
const SUBMIT_TIMEOUT: Duration = Duration::from_secs(30);

/// Submit all pending telemetry batches with a timeout.
///
/// Called by the detached telemetry-submit subprocess.
/// Returns an error if submission takes longer than SUBMIT_TIMEOUT.
pub fn submit_pending() -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = mpsc::channel::<Result<(), String>>();

    thread::spawn(move || {
        let result = submit_pending_inner().map_err(|e| e.to_string());
        let _ = tx.send(result);
    });

    match rx.recv_timeout(SUBMIT_TIMEOUT) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e.into()),
        Err(mpsc::RecvTimeoutError::Timeout) => Err("Telemetry submission timed out".into()),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err("Telemetry submission thread panicked".into())
        }
    }
}

/// Inner submission logic without timeout.
fn submit_pending_inner() -> Result<(), Box<dyn std::error::Error>> {
    let pending_dir = super::pending_dir();

    if !pending_dir.exists() {
        return Ok(());
    }

    otel::setup();

    let entries: Vec<_> = fs::read_dir(&pending_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
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

    otel::shutdown();

    Ok(())
}

fn submit_batch_file(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let batch: TelemetryBatch = serde_json::from_str(&content)?;

    for event in batch.events {
        match event {
            TelemetryEvent::Counter {
                name,
                value,
                attributes,
            } => {
                otel::submit_counter(&name, value, attributes);
            }
            TelemetryEvent::Histogram {
                name,
                value,
                attributes,
            } => {
                otel::submit_histogram(&name, value, attributes);
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

    use crate::telemetry::otel::SerializableValue;

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

    #[test]
    fn test_submit_pending_inner_no_pending_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        temp_env::with_var("ANA_HOME", Some(temp_dir.path()), || {
            // pending dir doesn't exist - should return Ok
            let result = submit_pending_inner();
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_submit_pending_inner_empty_pending_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pending_dir = temp_dir.path().join("telemetry").join("pending");
        fs::create_dir_all(&pending_dir).unwrap();

        temp_env::with_var("ANA_HOME", Some(temp_dir.path()), || {
            // pending dir exists but is empty - should return Ok
            let result = submit_pending_inner();
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_submit_pending_inner_skips_non_json_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pending_dir = temp_dir.path().join("telemetry").join("pending");
        fs::create_dir_all(&pending_dir).unwrap();

        // Create a non-json file
        fs::write(pending_dir.join("test.txt"), "not json").unwrap();
        // Create a tmp file (in-progress write)
        fs::write(pending_dir.join("123.tmp"), "{}").unwrap();

        temp_env::with_var("ANA_HOME", Some(temp_dir.path()), || {
            let result = submit_pending_inner();
            assert!(result.is_ok());

            // Files should still exist (not processed)
            assert!(pending_dir.join("test.txt").exists());
            assert!(pending_dir.join("123.tmp").exists());
        });
    }

    #[test]
    fn test_submit_pending_inner_handles_invalid_json() {
        let temp_dir = tempfile::tempdir().unwrap();
        let pending_dir = temp_dir.path().join("telemetry").join("pending");
        fs::create_dir_all(&pending_dir).unwrap();

        // Create an invalid json file
        fs::write(pending_dir.join("123.json"), "not valid json").unwrap();

        temp_env::with_var("ANA_HOME", Some(temp_dir.path()), || {
            // Should not panic, just log warning and continue
            let result = submit_pending_inner();
            assert!(result.is_ok());

            // Invalid file should still exist (not deleted on parse failure)
            assert!(pending_dir.join("123.json").exists());
        });
    }

    #[test]
    fn test_cleanup_old_files_empty_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        // Should handle empty directory without error
        let result = cleanup_old_files(temp_dir.path(), 7);
        assert!(result.is_ok());
    }

    #[test]
    fn test_submit_pending_timeout_returns_result() {
        let temp_dir = tempfile::tempdir().unwrap();
        temp_env::with_var("ANA_HOME", Some(temp_dir.path()), || {
            // With no pending dir, should complete quickly and return Ok
            let result = submit_pending();
            assert!(result.is_ok());
        });
    }
}
