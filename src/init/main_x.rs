//! Main-X channel initialization.
//!
//! Configures conda to use the Anaconda Main-X channel for early access packages.

use std::path::Path;
use std::process::Command;

use miette::{Context, IntoDiagnostic};

use crate::auth;
use crate::paths;

const MAIN_X_CHANNEL: &str = "https://repo.anaconda.cloud/repo/main-x";

/// Initialize Main-X channel access.
///
/// This command:
/// 1. Ensures the user is logged in to Anaconda
/// 2. Adds the Main-X channel to conda configuration
/// 3. Provides instructions for reverting the changes
pub async fn init_main_x() -> miette::Result<()> {
    eprintln!("Initializing Main-X channel access...");
    eprintln!();

    // Step 1: Check login status and prompt if needed
    ensure_logged_in().await?;

    // Step 2: Configure conda channel
    configure_conda_channel()?;

    // Step 3: Show success message and undo instructions
    eprintln!();
    eprintln!("Main-X channel configured successfully!");
    eprintln!();
    eprintln!("You can now install packages from the Main-X channel.");
    eprintln!();
    eprintln!("To undo this configuration, run:");
    eprintln!("  conda config --remove channels {}", MAIN_X_CHANNEL);

    Ok(())
}

/// Ensure the user is logged in, prompting them to login if not.
async fn ensure_logged_in() -> miette::Result<()> {
    eprintln!("Checking authentication status...");

    // Try to get API key to check if logged in
    let config = crate::config::Config::load();
    match auth::get_api_key(&config) {
        Ok(Some(_)) => {
            eprintln!("  Already logged in.");
            Ok(())
        }
        Ok(None) | Err(_) => {
            eprintln!("  Not logged in. Starting login flow...");
            eprintln!();
            auth::login()
                .await
                .map_err(|e| miette::miette!("Login failed: {}", e))?;
            Ok(())
        }
    }
}

/// Configure conda to use the Main-X channel.
fn configure_conda_channel() -> miette::Result<()> {
    eprintln!("Configuring conda channels...");

    let conda_bin = find_conda()?;

    // Check if channel is already configured
    if is_channel_configured(&conda_bin)? {
        eprintln!("  Main-X channel is already configured.");
        return Ok(());
    }

    // Add the channel (prepend so it has higher priority)
    eprintln!("  Adding Main-X channel: {}", MAIN_X_CHANNEL);

    let status = Command::new(&conda_bin)
        .args(["config", "--add", "channels", MAIN_X_CHANNEL])
        .status()
        .into_diagnostic()
        .context("failed to run conda config")?;

    if !status.success() {
        return Err(miette::miette!(
            "conda config failed with exit code: {}",
            status
        ));
    }

    eprintln!("  Channel added successfully.");

    Ok(())
}

/// Find the conda binary.
///
/// First checks if conda is installed via ana (in ~/.ana/tools/conda),
/// then falls back to looking in PATH.
fn find_conda() -> miette::Result<std::path::PathBuf> {
    // Check ana-managed conda first
    let ana_conda = paths::tool_prefix("conda")
        .join("bin")
        .join(paths::binary_name("conda"));
    if ana_conda.exists() {
        return Ok(ana_conda);
    }

    // Check if conda is in PATH by trying to run it
    let conda_path = std::path::PathBuf::from("conda");
    let check = Command::new(&conda_path).arg("--version").output();

    match check {
        Ok(output) if output.status.success() => Ok(conda_path),
        _ => Err(miette::miette!(
            "conda not found. Install it with: ana tool install conda"
        )),
    }
}

/// Check if the Main-X channel is already in the conda configuration.
fn is_channel_configured(conda_bin: &Path) -> miette::Result<bool> {
    let output = Command::new(conda_bin)
        .args(["config", "--show", "channels"])
        .output()
        .into_diagnostic()
        .context("failed to run conda config --show")?;

    if !output.status.success() {
        // If command fails, assume channel is not configured
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.contains(MAIN_X_CHANNEL))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_main_x_channel_constant() {
        assert_eq!(MAIN_X_CHANNEL, "https://repo.anaconda.cloud/repo/main-x");
    }
}
