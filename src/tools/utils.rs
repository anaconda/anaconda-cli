use std::process::Command;

/// Check if a command is available in PATH.
pub fn command_exists(cmd: &str) -> bool {
    Command::new(cmd)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Find the full path to a command using `which`.
/// Returns None if the command is not found.
pub fn which(cmd: &str) -> Option<String> {
    Command::new("which")
        .arg(cmd)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
}

/// Find the pip command (pip or pip3).
/// Returns the command name if found, None otherwise.
pub fn find_pip() -> Option<&'static str> {
    if command_exists("pip") {
        Some("pip")
    } else if command_exists("pip3") {
        Some("pip3")
    } else {
        None
    }
}
