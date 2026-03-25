use std::process::Command;

pub fn run_bootstrap() -> Result<(), String> {
    let status = Command::new("echo")
        .arg("Hello from the bootstrapper!")
        .status()
        .map_err(|e| format!("Failed to run bootstrap: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "Bootstrap failed with exit code {}",
            status.code().unwrap_or(1)
        ))
    }
}
