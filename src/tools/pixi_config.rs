//! Pixi configuration management.
//!
//! Handles configuring pixi's global settings, particularly default channels.

use std::path::Path;
use std::process::Command;

use miette::{Context, IntoDiagnostic};

use crate::input::prompt_yes_no;

const ANACONDA_MAIN_CHANNEL: &str = "https://repo.anaconda.com/pkgs/main";

/// Configure pixi's default channels to prefer repo.anaconda.com/pkgs/main.
///
/// This checks for existing configuration and either:
/// - Prompts the user if they have existing channel configuration
/// - Informs and configures automatically if no existing config
pub fn configure_default_channels(pixi_bin: &Path) -> miette::Result<()> {
    let existing_channels = get_default_channels(pixi_bin)?;

    if let Some(ref channels) = existing_channels {
        // Check if already configured correctly
        if channels.len() == 1 && channels[0] == ANACONDA_MAIN_CHANNEL {
            eprintln!();
            eprintln!("   Pixi default channels already configured correctly.");
            return Ok(());
        }

        // User has existing configuration - show it and prompt
        eprintln!();
        eprintln!("   Pixi already has default channels configured:");
        for channel in channels {
            eprintln!("     - {}", channel);
        }
        eprintln!();
        eprintln!(
            "   We recommend using {} for better Anaconda integration.",
            ANACONDA_MAIN_CHANNEL
        );
        eprintln!();

        if !prompt_yes_no("   Update default channels?") {
            eprintln!("   Keeping existing channel configuration.");
            return Ok(());
        }
    } else {
        // No existing config - inform user and proceed
        eprintln!();
        eprintln!("   Configuring pixi to use Anaconda's main channel by default.");
    }

    // Set default-channels using pixi config set --global
    // Value must be a JSON array string
    let channels_json =
        serde_json::to_string(&[ANACONDA_MAIN_CHANNEL]).expect("failed to serialize channels");
    let status = Command::new(pixi_bin)
        .args([
            "config",
            "set",
            "--global",
            "default-channels",
            &channels_json,
        ])
        .status()
        .into_diagnostic()
        .context("failed to run pixi config set")?;

    if !status.success() {
        return Err(miette::miette!("pixi config set failed with {}", status));
    }

    // Show how to revert
    if let Some(channels) = existing_channels {
        let old_channels_json =
            serde_json::to_string(&channels).expect("failed to serialize channels");
        eprintln!(
            "   To revert, run: pixi config set --global default-channels '{}'",
            old_channels_json
        );
    }

    eprintln!("   Updated pixi default channels.");

    Ok(())
}

/// Get the currently configured default channels, if any.
fn get_default_channels(pixi_bin: &Path) -> miette::Result<Option<Vec<String>>> {
    let output = Command::new(pixi_bin)
        .args(["config", "list", "--global", "--json"])
        .output()
        .into_diagnostic()
        .context("failed to run pixi config list")?;

    if !output.status.success() {
        // If config list fails, assume no config exists
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let config: serde_json::Value = serde_json::from_str(&stdout)
        .into_diagnostic()
        .context("failed to parse pixi config output")?;

    let Some(channels) = config.get("default-channels") else {
        return Ok(None);
    };

    let Some(channels_array) = channels.as_array() else {
        return Ok(None);
    };

    let channels: Vec<String> = channels_array
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();

    if channels.is_empty() {
        Ok(None)
    } else {
        Ok(Some(channels))
    }
}
