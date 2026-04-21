//! main-x channel initialization.
//!
//! Configures conda to use the Anaconda main-x channel for early access packages.

use std::path::Path;
use std::process::Command;

use miette::{Context, IntoDiagnostic};

use crate::auth;
use crate::paths;
use crate::ui::status;

const MAIN_CHANNEL: &str = "https://repo.anaconda.com/pkgs/main";
const MAIN_X_CHANNEL: &str = "https://repo.anaconda.cloud/repo/main-x";

/// Initialize main-x channel access.
///
/// This command:
/// 1. Ensures the user is logged in to Anaconda
/// 2. Adds the main-x channel to conda configuration (with main channel for fallback)
/// 3. Provides instructions for reverting the changes
pub async fn init_main_x() -> miette::Result<()> {
    status::info(&format!(
        "Initializing {} channel access...",
        status::highlight("main-x")
    ));
    status::blank_line();

    // Step 1: Check login status and prompt if needed
    ensure_logged_in().await?;

    // Step 2: Configure conda channels
    configure_conda_channels()?;

    // Step 3: Show success message and undo instructions
    status::blank_line();
    status::celebrate(&format!(
        "You can now install packages from the {} channel!",
        status::highlight("main-x")
    ));
    status::blank_line();
    status::info("To undo this configuration, run:");
    eprintln!(
        "  {}",
        status::highlight(&format!(
            "conda config --remove channels {}",
            MAIN_X_CHANNEL
        ))
    );

    Ok(())
}

/// Ensure the user is logged in, prompting them to login if not.
async fn ensure_logged_in() -> miette::Result<()> {
    status::waiting("Checking authentication status...");

    // Try to get API key to check if logged in
    let config = crate::config::Config::load();
    match auth::get_api_key(&config) {
        Ok(Some(_)) => {
            status::success("Already logged in");
            Ok(())
        }
        Ok(None) | Err(_) => {
            status::info("Not logged in. Starting login flow...");
            status::blank_line();
            auth::login()
                .await
                .map_err(|e| miette::miette!("Login failed: {}", e))?;
            Ok(())
        }
    }
}

/// Configure conda to use the main-x channel with main as fallback.
///
/// Ensures that:
/// - main-x is present in channels
/// - main is present in channels (for fallback)
/// - main has higher precedence than main-x (appears earlier in the list)
fn configure_conda_channels() -> miette::Result<()> {
    status::blank_line();
    status::waiting("Configuring conda channels...");

    let conda_bin = find_conda()?;
    let current_channels = get_configured_channels(&conda_bin)?;

    let has_main = current_channels.iter().any(|c| c == MAIN_CHANNEL);
    let has_main_x = current_channels.iter().any(|c| c == MAIN_X_CHANNEL);

    // Check if main comes before main-x (correct precedence)
    let main_before_main_x = if has_main && has_main_x {
        let main_pos = current_channels.iter().position(|c| c == MAIN_CHANNEL);
        let main_x_pos = current_channels.iter().position(|c| c == MAIN_X_CHANNEL);
        main_pos < main_x_pos
    } else {
        true // Will be configured correctly below
    };

    if has_main && has_main_x && main_before_main_x {
        status::success("Channels already configured correctly");
        return Ok(());
    }

    // Add channels as needed, using --add (prepend) to set precedence
    // We add main-x first, then main, so main ends up with higher precedence

    if !has_main_x {
        status::success(&format!("Adding {} channel", status::highlight("main-x")));
        run_conda_config(&conda_bin, &["--add", "channels", MAIN_X_CHANNEL])?;
    }

    if !has_main {
        status::success(&format!("Adding {} channel", status::highlight("main")));
        run_conda_config(&conda_bin, &["--add", "channels", MAIN_CHANNEL])?;
    } else if has_main_x && !main_before_main_x {
        // main exists but has lower precedence than main-x, need to fix
        status::waiting(&format!(
            "Adjusting channel precedence (main should be higher than {})...",
            status::highlight("main-x")
        ));
        // Remove and re-add main to move it to the top
        run_conda_config(&conda_bin, &["--remove", "channels", MAIN_CHANNEL])?;
        run_conda_config(&conda_bin, &["--add", "channels", MAIN_CHANNEL])?;
    }

    Ok(())
}

/// Run a conda config command.
fn run_conda_config(conda_bin: &Path, args: &[&str]) -> miette::Result<()> {
    let status = Command::new(conda_bin)
        .arg("config")
        .args(args)
        .status()
        .into_diagnostic()
        .context("failed to run conda config")?;

    if !status.success() {
        return Err(miette::miette!(
            "conda config {} failed with exit code: {}",
            args.join(" "),
            status
        ));
    }

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

/// Get the list of currently configured channels from conda config --show.
///
/// The output format is:
/// ```
/// channels:
///   - conda-forge
///   - defaults
/// ```
fn get_configured_channels(conda_bin: &Path) -> miette::Result<Vec<String>> {
    let output = Command::new(conda_bin)
        .args(["config", "--show", "channels"])
        .output()
        .into_diagnostic()
        .context("failed to run conda config --show channels")?;

    if !output.status.success() {
        // If command fails, assume no channels configured
        return Ok(vec![]);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse YAML-like output: skip "channels:" line, then extract "  - <channel>" lines
    let channels: Vec<String> = stdout
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("- ") {
                Some(trimmed.strip_prefix("- ").unwrap().to_string())
            } else {
                None
            }
        })
        .collect();

    Ok(channels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_constants() {
        assert_eq!(MAIN_CHANNEL, "https://repo.anaconda.com/pkgs/main");
        assert_eq!(MAIN_X_CHANNEL, "https://repo.anaconda.cloud/repo/main-x");
    }

    #[test]
    fn test_parse_channels_output() {
        let output = "channels:\n  - conda-forge\n  - defaults\n";
        let channels: Vec<String> = output
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.starts_with("- ") {
                    Some(trimmed.strip_prefix("- ").unwrap().to_string())
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(channels, vec!["conda-forge", "defaults"]);
    }
}
