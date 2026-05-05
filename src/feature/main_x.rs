//! main-x feature.
//!
//! Configures conda to use the Anaconda main-x channel for early access packages.

use std::path::Path;
use std::process::Command;

use miette::{Context, IntoDiagnostic};

use crate::auth;
use crate::context::CommandContext;
use crate::input::prompt_yes_no;
use crate::paths;
use crate::ui::status;

const MAIN_X_CHANNEL: &str = "https://repo.anaconda.cloud/repo/main-x";

/// Represents a channel configuration action to be executed.
enum ChannelAction {
    AddMainX,
    RemoveMainX,
}

impl ChannelAction {
    /// Returns the conda commands that will be executed for this action.
    fn commands(&self) -> Vec<(&'static str, &'static str)> {
        match self {
            ChannelAction::AddMainX => {
                vec![("--add", MAIN_X_CHANNEL)]
            }
            ChannelAction::RemoveMainX => {
                vec![("--remove", MAIN_X_CHANNEL)]
            }
        }
    }

    /// Execute the action, showing status for each command.
    fn execute_with_status(&self, conda_bin: &Path) -> miette::Result<()> {
        for (flag, channel) in self.commands() {
            let cmd = format!("conda config {} channels {}", flag, channel);
            status::running(&format!("Running {}", status::highlight(&cmd)));
            run_conda_config(conda_bin, &[flag, "channels", channel])?;
            status::finish_running(&format!("Ran {}", status::highlight(&cmd)));
        }
        Ok(())
    }
}

/// Plan the actions needed to enable main-x channel.
fn plan_enable_actions(current_channels: &[String]) -> Vec<ChannelAction> {
    let mut actions = Vec::new();

    let has_main_x = current_channels.iter().any(|c| c == MAIN_X_CHANNEL);

    // Only need to add main-x if not already present.
    // We don't need to add main explicitly - "defaults" includes main,
    // and most users will already have defaults configured.
    if !has_main_x {
        actions.push(ChannelAction::AddMainX);
    }

    actions
}

/// Plan the actions needed to disable main-x channel.
fn plan_disable_actions(current_channels: &[String]) -> Vec<ChannelAction> {
    let mut actions = Vec::new();

    let has_main_x = current_channels.iter().any(|c| c == MAIN_X_CHANNEL);

    if has_main_x {
        actions.push(ChannelAction::RemoveMainX);
    }

    actions
}

/// Enable main-x channel access.
///
/// This command:
/// 1. Ensures the user is logged in to Anaconda
/// 2. Shows planned changes and prompts for confirmation
/// 3. Adds the main-x channel to conda configuration (with main channel for fallback)
/// 4. Provides instructions for reverting the changes
pub async fn enable_main_x(ctx: &mut CommandContext, force: bool) -> miette::Result<()> {
    status::info(&format!(
        "Enabling {} feature...",
        status::highlight("main-x")
    ));
    status::blank_line();

    // Step 1: Check login status and prompt if needed
    ensure_logged_in(ctx).await?;

    // Step 2: Determine what changes need to be made
    let conda_bin = find_conda()?;
    let current_channels = get_configured_channels(&conda_bin)?;
    let actions = plan_enable_actions(&current_channels);

    if actions.is_empty() {
        status::success("Feature already enabled");
        return Ok(());
    }

    // Step 3: Show planned changes
    status::blank_line();
    status::info("The following commands will be run:");
    for action in &actions {
        for (flag, channel) in action.commands() {
            let cmd = format!("conda config {} channels {}", flag, channel);
            eprintln!("  {}", status::highlight(&cmd));
        }
    }
    status::blank_line();

    // Step 4: Prompt for confirmation unless --force
    if !force && !prompt_yes_no("Proceed?", true) {
        eprintln!("Aborted.");
        return Ok(());
    }

    // Step 5: Execute the changes
    status::blank_line();
    for action in &actions {
        action.execute_with_status(&conda_bin)?;
    }

    // Step 6: Show success message and undo instructions
    status::blank_line();
    status::celebrate(&format!(
        "You can now install packages from the {} channel!",
        status::highlight("main-x")
    ));
    status::blank_line();
    status::info("To disable this feature, run:");
    eprintln!("  {}", status::highlight("ana feature disable main-x"));

    Ok(())
}

/// Disable main-x channel configuration.
///
/// This command removes the main-x channel from conda configuration.
pub async fn disable_main_x(_ctx: &mut CommandContext, force: bool) -> miette::Result<()> {
    status::info(&format!(
        "Disabling {} feature...",
        status::highlight("main-x")
    ));
    status::blank_line();

    let conda_bin = find_conda()?;
    let current_channels = get_configured_channels(&conda_bin)?;
    let actions = plan_disable_actions(&current_channels);

    if actions.is_empty() {
        status::success(&format!(
            "{} feature is not enabled",
            status::highlight("main-x")
        ));
        return Ok(());
    }

    // Show planned changes
    status::info("The following commands will be run:");
    for action in &actions {
        for (flag, channel) in action.commands() {
            let cmd = format!("conda config {} channels {}", flag, channel);
            eprintln!("  {}", status::highlight(&cmd));
        }
    }
    status::blank_line();

    // Prompt for confirmation unless --force
    if !force && !prompt_yes_no("Proceed?", true) {
        eprintln!("Aborted.");
        return Ok(());
    }

    status::blank_line();
    for action in actions {
        action.execute_with_status(&conda_bin)?;
    }

    status::blank_line();
    status::info("To re-enable, run:");
    eprintln!("  {}", status::highlight("ana feature enable main-x"));

    Ok(())
}

/// Ensure the user is logged in, prompting them to login if not.
async fn ensure_logged_in(ctx: &mut CommandContext) -> miette::Result<()> {
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
            auth::login(ctx, None, false, false)
                .await
                .map_err(|e| miette::miette!("Login failed: {}", e))?;
            Ok(())
        }
    }
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
                Some(trimmed.strip_prefix("- ").unwrap().trim().to_string())
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
        assert_eq!(MAIN_X_CHANNEL, "https://repo.anaconda.cloud/repo/main-x");
    }

    // ========================================================================
    // Channel parsing tests
    // ========================================================================

    #[test]
    fn test_parse_channels_output_typical() {
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

    #[test]
    fn test_parse_channels_output_empty() {
        let output = "channels: []\n";
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

        assert!(channels.is_empty());
    }

    #[test]
    fn test_parse_channels_output_with_urls() {
        let output = "channels:\n  - https://repo.anaconda.cloud/repo/main-x\n  - conda-forge\n  - defaults\n";
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

        assert_eq!(
            channels,
            vec![
                "https://repo.anaconda.cloud/repo/main-x",
                "conda-forge",
                "defaults"
            ]
        );
    }

    #[test]
    fn test_parse_channels_output_single_channel() {
        let output = "channels:\n  - defaults\n";
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

        assert_eq!(channels, vec!["defaults"]);
    }

    // ========================================================================
    // plan_enable_actions tests
    // ========================================================================

    #[test]
    fn test_plan_enable_actions_empty_channels() {
        let current_channels: Vec<String> = vec![];
        let actions = plan_enable_actions(&current_channels);

        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], ChannelAction::AddMainX));
    }

    #[test]
    fn test_plan_enable_actions_defaults_only() {
        let current_channels = vec!["defaults".to_string()];
        let actions = plan_enable_actions(&current_channels);

        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], ChannelAction::AddMainX));
    }

    #[test]
    fn test_plan_enable_actions_conda_forge_and_defaults() {
        let current_channels = vec!["conda-forge".to_string(), "defaults".to_string()];
        let actions = plan_enable_actions(&current_channels);

        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], ChannelAction::AddMainX));
    }

    #[test]
    fn test_plan_enable_actions_main_x_already_present() {
        let current_channels = vec![
            MAIN_X_CHANNEL.to_string(),
            "conda-forge".to_string(),
            "defaults".to_string(),
        ];
        let actions = plan_enable_actions(&current_channels);

        assert!(
            actions.is_empty(),
            "No actions needed when main-x already configured"
        );
    }

    #[test]
    fn test_plan_enable_actions_main_x_only() {
        let current_channels = vec![MAIN_X_CHANNEL.to_string()];
        let actions = plan_enable_actions(&current_channels);

        assert!(actions.is_empty());
    }

    // ========================================================================
    // ChannelAction::commands tests
    // ========================================================================

    #[test]
    fn test_channel_action_add_main_x_commands() {
        let action = ChannelAction::AddMainX;
        let commands = action.commands();

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0], ("--add", MAIN_X_CHANNEL));
    }

    #[test]
    fn test_channel_action_commands_format() {
        let action = ChannelAction::AddMainX;
        let commands = action.commands();

        for (flag, channel) in commands {
            assert!(flag.starts_with("--"), "Flag should start with --");
            assert!(!channel.is_empty(), "Channel should not be empty");
        }
    }
}
