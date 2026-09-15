//! Persistent MCP install state, shared with other Anaconda tools via
//! `~/.anaconda/mcp_state.json`.

use std::path::PathBuf;

use serde_json::{Value, json};

fn state_path() -> PathBuf {
    crate::paths::home_dir().join(".anaconda").join("mcp_state.json")
}

fn read() -> Value {
    std::fs::read_to_string(state_path())
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .filter(|v: &Value| v.is_object())
        .unwrap_or_else(|| json!({}))
}

fn write(state: &Value) {
    let path = state_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(out) = serde_json::to_string(state) {
        let _ = std::fs::write(path, out);
    }
}

/// True when this is the first MCP setup on this machine.
pub fn is_new_install() -> bool {
    read().get("first_install_at").is_none()
}

/// Record the first install timestamp. No-op if already set.
pub fn mark_installed() {
    let mut state = read();
    if state.get("first_install_at").is_none() {
        state["first_install_at"] = json!(chrono::Utc::now().to_rfc3339());
        write(&state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial(env)]
    fn test_new_install_flow() {
        let dir = tempfile::tempdir().unwrap();
        temp_env::with_var("HOME", Some(dir.path().to_string_lossy().as_ref()), || {
            assert!(is_new_install());
            mark_installed();
            assert!(!is_new_install());
            // Marking twice keeps the original timestamp
            let first = read()["first_install_at"].clone();
            mark_installed();
            assert_eq!(read()["first_install_at"], first);
        });
    }
}
