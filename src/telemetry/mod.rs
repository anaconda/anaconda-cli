//! Non-blocking telemetry via spool files and detached subprocess.
//!
//! This module provides fire-and-forget telemetry that doesn't block CLI exit:
//!
//! 1. Commands buffer metrics locally in `TelemetryContext`
//! 2. At exit, events are serialized to `~/.ana/telemetry/pending/*.json`
//! 3. A detached subprocess (`ana telemetry-submit`) processes the files
//! 4. The parent CLI exits immediately
//!
//! The types in `event.rs` are portable and can be moved to anaconda-otel-rs.

pub mod event;
pub mod otel;
mod spawn;
mod spool;
mod submit;

pub use event::TelemetryEvent;
pub use otel::SerializableValue;
pub use spawn::{kill_submitters, spawn_telemetry_submitter};
pub use spool::write_batch;
pub use submit::submit_pending;

use std::path::PathBuf;

use crate::paths;

/// Directory for pending telemetry files: ~/.ana/telemetry/pending/
pub fn pending_dir() -> PathBuf {
    paths::ana_home().join("telemetry").join("pending")
}
