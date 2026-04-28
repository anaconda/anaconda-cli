use std::process::Command;

use crate::auth;
use crate::config::Config;

/// Configure uv to use Anaconda's wheels index with authentication.
pub fn configure(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let api_key = auth::get_api_key(config)?
        .ok_or("Login required to configure uv. Run `ana login` first.")?;

    // Get base URL (remove /simple/ suffix if present)
    let base_url = config
        .pip_index_url
        .trim_end_matches('/')
        .trim_end_matches("/simple")
        .trim_end_matches('/');

    let output = Command::new("uv")
        .args(["auth", "login", base_url, "--token", &api_key])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to configure uv auth: {}", stderr).into());
    }

    println!("Configured uv authentication for {}", base_url);
    Ok(())
}

/// Remove uv configuration for Anaconda's wheels index.
pub fn deconfigure(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    // Get base URL (remove /simple/ suffix if present)
    let base_url = config
        .pip_index_url
        .trim_end_matches('/')
        .trim_end_matches("/simple")
        .trim_end_matches('/');

    let output = Command::new("uv")
        .args(["auth", "logout", base_url])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Ignore "not logged in" errors
        if !stderr.contains("not logged in") && !stderr.contains("No credentials") {
            return Err(format!("Failed to deconfigure uv auth: {}", stderr).into());
        }
    }

    println!("Removed uv authentication for {}", base_url);
    Ok(())
}
