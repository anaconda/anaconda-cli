use std::process::Command;

/// Check if a command is available in PATH.
pub fn command_exists(cmd: &str) -> bool {
    Command::new(cmd)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Return an error if the command is not available.
pub fn require_command(cmd: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !command_exists(cmd) {
        return Err(format!(
            "'{}' is not installed or not found in PATH. Please install {} first.",
            cmd, cmd
        )
        .into());
    }
    Ok(())
}
