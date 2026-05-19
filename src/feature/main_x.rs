//! main-x feature.
//!
//! Configures conda or pixi to use the Anaconda main-x channel for early access packages.

use std::path::Path;
use std::process::Command;

use miette::{Context, IntoDiagnostic};
use serde_json::Value;

use crate::auth;
use crate::context::CommandContext;
use crate::input::prompt_yes_no;
use crate::paths;
use crate::ui::status;

const MAIN_CHANNEL: &str = "https://repo.anaconda.cloud/repo/main";
const MAIN_X_CHANNEL: &str = "https://repo.anaconda.cloud/repo/main-x";
const REPO_HOST: &str = "repo.anaconda.cloud";

/// Represents a channel configuration action to be executed for conda.
enum CondaChannelAction {
    AddMain,
    AddMainX,
    RemoveMainX,
}

impl CondaChannelAction {
    fn commands(&self) -> Vec<(&'static str, &'static str)> {
        match self {
            CondaChannelAction::AddMain => {
                vec![("--add", MAIN_CHANNEL)]
            }
            CondaChannelAction::AddMainX => {
                vec![("--add", MAIN_X_CHANNEL)]
            }
            CondaChannelAction::RemoveMainX => {
                vec![("--remove", MAIN_X_CHANNEL)]
            }
        }
    }

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

/// Represents a channel configuration action to be executed for pixi.
enum PixiChannelAction {
    AddMain,
    AddMainX,
    RemoveMainX,
}

impl PixiChannelAction {
    fn command_display(&self) -> String {
        match self {
            PixiChannelAction::AddMain => {
                format!(
                    "pixi config prepend --global default-channels {}",
                    MAIN_CHANNEL
                )
            }
            PixiChannelAction::AddMainX => {
                format!(
                    "pixi config prepend --global default-channels {}",
                    MAIN_X_CHANNEL
                )
            }
            PixiChannelAction::RemoveMainX => {
                "pixi config unset --global default-channels".to_string()
            }
        }
    }

    fn execute_with_status(&self, pixi_bin: &Path) -> miette::Result<()> {
        let cmd = self.command_display();
        status::running(&format!("Running {}", status::highlight(&cmd)));
        match self {
            PixiChannelAction::AddMain => {
                run_pixi_config(
                    pixi_bin,
                    &["prepend", "--global", "default-channels", MAIN_CHANNEL],
                )?;
            }
            PixiChannelAction::AddMainX => {
                run_pixi_config(
                    pixi_bin,
                    &["prepend", "--global", "default-channels", MAIN_X_CHANNEL],
                )?;
            }
            PixiChannelAction::RemoveMainX => {
                run_pixi_config(pixi_bin, &["unset", "--global", "default-channels"])?;
            }
        }
        status::finish_running(&format!("Ran {}", status::highlight(&cmd)));
        Ok(())
    }
}

/// Configure pixi auth for repo.anaconda.cloud.
fn run_pixi_auth_login(pixi_bin: &Path, api_key: &str) -> miette::Result<()> {
    let cmd = format!("pixi auth login {} --token <token>", REPO_HOST);
    status::running(&format!("Running {}", status::highlight(&cmd)));

    let output = Command::new(pixi_bin)
        .args(["auth", "login", REPO_HOST, "--token", api_key])
        .output()
        .into_diagnostic()
        .context("failed to run pixi auth login")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(miette::miette!("pixi auth login failed: {}", stderr));
    }

    status::finish_running(&format!("Ran {}", status::highlight(&cmd)));
    Ok(())
}

/// Remove pixi auth for repo.anaconda.cloud.
fn run_pixi_auth_logout(pixi_bin: &Path) -> miette::Result<()> {
    let cmd = format!("pixi auth logout {}", REPO_HOST);
    status::running(&format!("Running {}", status::highlight(&cmd)));

    let output = Command::new(pixi_bin)
        .args(["auth", "logout", REPO_HOST])
        .output()
        .into_diagnostic()
        .context("failed to run pixi auth logout")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Ignore "not logged in" type errors
        if !stderr.contains("No credentials") && !stderr.contains("not found") {
            return Err(miette::miette!("pixi auth logout failed: {}", stderr));
        }
    }

    status::finish_running(&format!("Ran {}", status::highlight(&cmd)));
    Ok(())
}

/// Plan the actions needed to enable main-x channel for conda.
///
/// Ensures both main and main-x channels are added with priority main -> main-x.
/// Since `conda config --add` prepends, we add main-x first, then main.
/// Note: "defaults" is treated as equivalent to main channel.
fn plan_conda_enable_actions(current_channels: &[String]) -> Vec<CondaChannelAction> {
    let has_main = current_channels
        .iter()
        .any(|c| c == MAIN_CHANNEL || c == "defaults");
    let has_main_x = current_channels.iter().any(|c| c == MAIN_X_CHANNEL);

    let mut actions = vec![];

    // Add main-x first (will be second after main is prepended)
    if !has_main_x {
        actions.push(CondaChannelAction::AddMainX);
    }

    // Add main second (prepends, so it ends up first)
    // Skip if "defaults" is present since it's equivalent to main
    if !has_main {
        actions.push(CondaChannelAction::AddMain);
    }

    actions
}

/// Plan the actions needed to disable main-x channel for conda.
fn plan_conda_disable_actions(current_channels: &[String]) -> Vec<CondaChannelAction> {
    let has_main_x = current_channels.iter().any(|c| c == MAIN_X_CHANNEL);
    if has_main_x {
        vec![CondaChannelAction::RemoveMainX]
    } else {
        vec![]
    }
}

/// Plan the actions needed to enable main-x channel for pixi.
///
/// Ensures both main and main-x channels are added with priority main -> main-x.
/// Since `pixi config prepend` prepends, we add main-x first, then main.
fn plan_pixi_enable_actions(current_channels: &[String]) -> Vec<PixiChannelAction> {
    let has_main = current_channels.iter().any(|c| c == MAIN_CHANNEL);
    let has_main_x = current_channels.iter().any(|c| c == MAIN_X_CHANNEL);

    let mut actions = vec![];

    // Add main-x first (will be second after main is prepended)
    if !has_main_x {
        actions.push(PixiChannelAction::AddMainX);
    }

    // Add main second (prepends, so it ends up first)
    if !has_main {
        actions.push(PixiChannelAction::AddMain);
    }

    actions
}

/// Plan the actions needed to disable main-x channel for pixi.
fn plan_pixi_disable_actions(current_channels: &[String]) -> Vec<PixiChannelAction> {
    let has_main_x = current_channels.iter().any(|c| c == MAIN_X_CHANNEL);
    if has_main_x {
        vec![PixiChannelAction::RemoveMainX]
    } else {
        vec![]
    }
}

/// Enable main-x channel access via conda.
///
/// This command:
/// 1. Ensures the user is logged in to Anaconda
/// 2. Shows planned changes and prompts for confirmation
/// 3. Adds the main-x channel to conda configuration
/// 4. Provides instructions for reverting the changes
pub async fn enable_main_x_conda(ctx: &CommandContext, force: bool) -> miette::Result<()> {
    status::info(&format!(
        "Enabling {} feature via {}...",
        status::highlight("main-x"),
        status::highlight("conda")
    ));
    status::blank_line();

    // Step 1: Check login status and prompt if needed
    auth::ensure_logged_in(ctx).await.into_diagnostic()?;

    // Step 2: Determine what changes need to be made
    let conda_bin = find_conda()?;
    let current_channels = get_configured_channels_conda(&conda_bin)?;
    let actions = plan_conda_enable_actions(&current_channels);

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

/// Enable main-x channel access via pixi.
///
/// This command:
/// 1. Ensures the user is logged in to Anaconda
/// 2. Shows planned changes and prompts for confirmation
/// 3. Configures pixi auth for repo.anaconda.cloud
/// 4. Adds the main-x channel to pixi global configuration
/// 5. Provides instructions for reverting the changes
pub async fn enable_main_x_pixi(ctx: &CommandContext, force: bool) -> miette::Result<()> {
    status::info(&format!(
        "Enabling {} feature via {}...",
        status::highlight("main-x"),
        status::highlight("pixi")
    ));
    status::blank_line();

    // Step 1: Check login status and prompt if needed
    auth::ensure_logged_in(ctx).await.into_diagnostic()?;

    // Step 2: Determine what changes need to be made
    let pixi_bin = find_pixi()?;
    let current_channels = get_configured_channels_pixi(&pixi_bin)?;
    let actions = plan_pixi_enable_actions(&current_channels);

    if actions.is_empty() {
        status::success("Feature already enabled");
        return Ok(());
    }

    // Step 3: Show planned changes
    status::blank_line();
    status::info("The following commands will be run:");
    eprintln!(
        "  {}",
        status::highlight(&format!("pixi auth login {} --token <token>", REPO_HOST))
    );
    for action in &actions {
        eprintln!("  {}", status::highlight(&action.command_display()));
    }
    status::blank_line();

    // Step 4: Prompt for confirmation unless --force
    if !force && !prompt_yes_no("Proceed?", true) {
        eprintln!("Aborted.");
        return Ok(());
    }

    // Step 5: Execute the changes
    status::blank_line();

    // Get the API key for auth
    let api_key = auth::get_api_key(&ctx.config)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("Not logged in"))?;

    // Configure pixi auth first
    run_pixi_auth_login(&pixi_bin, &api_key)?;

    // Then configure channels
    for action in &actions {
        action.execute_with_status(&pixi_bin)?;
    }

    // Step 6: Show success message and undo instructions
    status::blank_line();
    status::celebrate(&format!(
        "You can now install packages from the {} channel with pixi!",
        status::highlight("main-x")
    ));
    status::blank_line();
    status::info("To disable this feature, run:");
    eprintln!(
        "  {}",
        status::highlight("ana feature disable main-x --pixi")
    );

    Ok(())
}

/// Disable main-x channel configuration for conda.
///
/// This command removes the main-x channel from conda configuration.
pub async fn disable_main_x_conda(_ctx: &CommandContext, force: bool) -> miette::Result<()> {
    status::info(&format!(
        "Disabling {} feature via {}...",
        status::highlight("main-x"),
        status::highlight("conda")
    ));
    status::blank_line();

    let conda_bin = find_conda()?;
    let current_channels = get_configured_channels_conda(&conda_bin)?;
    let actions = plan_conda_disable_actions(&current_channels);

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

/// Disable main-x channel configuration for pixi.
///
/// This command removes the main-x channel and auth from pixi global configuration.
pub async fn disable_main_x_pixi(_ctx: &CommandContext, force: bool) -> miette::Result<()> {
    status::info(&format!(
        "Disabling {} feature via {}...",
        status::highlight("main-x"),
        status::highlight("pixi")
    ));
    status::blank_line();

    let pixi_bin = find_pixi()?;
    let current_channels = get_configured_channels_pixi(&pixi_bin)?;
    let actions = plan_pixi_disable_actions(&current_channels);

    if actions.is_empty() {
        status::success(&format!(
            "{} feature is not enabled for pixi",
            status::highlight("main-x")
        ));
        return Ok(());
    }

    // Show planned changes
    status::info("The following commands will be run:");
    for action in &actions {
        eprintln!("  {}", status::highlight(&action.command_display()));
    }
    eprintln!(
        "  {}",
        status::highlight(&format!("pixi auth logout {}", REPO_HOST))
    );
    status::blank_line();

    // Prompt for confirmation unless --force
    if !force && !prompt_yes_no("Proceed?", true) {
        eprintln!("Aborted.");
        return Ok(());
    }

    status::blank_line();

    // Remove channels first
    for action in actions {
        action.execute_with_status(&pixi_bin)?;
    }

    // Then remove auth
    run_pixi_auth_logout(&pixi_bin)?;

    status::blank_line();
    status::info("To re-enable, run:");
    eprintln!(
        "  {}",
        status::highlight("ana feature enable main-x --pixi")
    );

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

/// Run a pixi config command.
fn run_pixi_config(pixi_bin: &Path, args: &[&str]) -> miette::Result<()> {
    let status = Command::new(pixi_bin)
        .arg("config")
        .args(args)
        .status()
        .into_diagnostic()
        .context("failed to run pixi config")?;

    if !status.success() {
        return Err(miette::miette!(
            "pixi config {} failed with exit code: {}",
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
            "This feature currently requires conda to be installed separately. Install it from: https://www.anaconda.com/download"
        )),
    }
}

/// Find the pixi binary.
///
/// First checks if pixi is installed via ana (in ~/.ana/tools/pixi),
/// then falls back to looking in PATH.
fn find_pixi() -> miette::Result<std::path::PathBuf> {
    // Check ana-managed pixi first
    let pixi = paths::tool_prefix("pixi")
        .join("bin")
        .join(paths::binary_name("pixi"));
    if pixi.exists() {
        return Ok(pixi);
    }

    // Check if pixi is in PATH by trying to run it
    let pixi_path = std::path::PathBuf::from("pixi");
    let check = Command::new(&pixi_path).arg("--version").output();

    match check {
        Ok(output) if output.status.success() => Ok(pixi_path),
        _ => Err(miette::miette!(
            "pixi not found. Install it with: ana tool install pixi"
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
fn get_configured_channels_conda(conda_bin: &Path) -> miette::Result<Vec<String>> {
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
/// Get the list of currently configured global default channels from pixi config.
fn get_configured_channels_pixi(pixi_bin: &Path) -> miette::Result<Vec<String>> {
    let output = Command::new(pixi_bin)
        .args(["config", "list", "--json", "--global"])
        .output()
        .into_diagnostic()
        .context("failed to run pixi config list --json --global")?;

    if !output.status.success() {
        // If command fails, assume no channels configured
        return Ok(vec![]);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    let obj: Value = serde_json::from_str(&stdout).unwrap_or(Value::Null);

    let channels: Vec<String> = obj["default-channels"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

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
    // plan_conda_enable_actions tests
    // ========================================================================

    #[test]
    fn test_plan_conda_enable_actions_empty_channels() {
        let current_channels: Vec<String> = vec![];
        let actions = plan_conda_enable_actions(&current_channels);

        // Adds main-x first, then main (so main ends up first after prepending)
        assert_eq!(actions.len(), 2);
        assert!(matches!(actions[0], CondaChannelAction::AddMainX));
        assert!(matches!(actions[1], CondaChannelAction::AddMain));
    }

    #[test]
    fn test_plan_conda_enable_actions_defaults_only() {
        let current_channels = vec!["defaults".to_string()];
        let actions = plan_conda_enable_actions(&current_channels);

        // "defaults" is equivalent to main, so only need to add main-x
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], CondaChannelAction::AddMainX));
    }

    #[test]
    fn test_plan_conda_enable_actions_conda_forge_and_defaults() {
        let current_channels = vec!["conda-forge".to_string(), "defaults".to_string()];
        let actions = plan_conda_enable_actions(&current_channels);

        // "defaults" is equivalent to main, so only need to add main-x
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], CondaChannelAction::AddMainX));
    }

    #[test]
    fn test_plan_conda_enable_actions_main_and_main_x_already_present() {
        let current_channels = vec![
            MAIN_CHANNEL.to_string(),
            MAIN_X_CHANNEL.to_string(),
            "conda-forge".to_string(),
            "defaults".to_string(),
        ];
        let actions = plan_conda_enable_actions(&current_channels);

        assert!(
            actions.is_empty(),
            "No actions needed when main and main-x already configured"
        );
    }

    #[test]
    fn test_plan_conda_enable_actions_main_x_only() {
        let current_channels = vec![MAIN_X_CHANNEL.to_string()];
        let actions = plan_conda_enable_actions(&current_channels);

        // Still need to add main
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], CondaChannelAction::AddMain));
    }

    #[test]
    fn test_plan_conda_enable_actions_main_only() {
        let current_channels = vec![MAIN_CHANNEL.to_string()];
        let actions = plan_conda_enable_actions(&current_channels);

        // Still need to add main-x
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], CondaChannelAction::AddMainX));
    }

    // ========================================================================
    // plan_pixi_enable_actions tests
    // ========================================================================

    #[test]
    fn test_plan_pixi_enable_actions_empty_channels() {
        let current_channels: Vec<String> = vec![];
        let actions = plan_pixi_enable_actions(&current_channels);

        // Adds main-x first, then main (so main ends up first after prepending)
        assert_eq!(actions.len(), 2);
        assert!(matches!(actions[0], PixiChannelAction::AddMainX));
        assert!(matches!(actions[1], PixiChannelAction::AddMain));
    }

    #[test]
    fn test_plan_pixi_enable_actions_main_x_already_present() {
        let current_channels = vec![MAIN_X_CHANNEL.to_string()];
        let actions = plan_pixi_enable_actions(&current_channels);

        // Still need to add main
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], PixiChannelAction::AddMain));
    }

    #[test]
    fn test_plan_pixi_enable_actions_main_already_present() {
        let current_channels = vec![MAIN_CHANNEL.to_string()];
        let actions = plan_pixi_enable_actions(&current_channels);

        // Still need to add main-x
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], PixiChannelAction::AddMainX));
    }

    #[test]
    fn test_plan_pixi_enable_actions_both_already_present() {
        let current_channels = vec![MAIN_CHANNEL.to_string(), MAIN_X_CHANNEL.to_string()];
        let actions = plan_pixi_enable_actions(&current_channels);

        assert!(actions.is_empty());
    }

    // ========================================================================
    // CondaChannelAction::commands tests
    // ========================================================================

    #[test]
    fn test_conda_channel_action_add_main_x_commands() {
        let action = CondaChannelAction::AddMainX;
        let commands = action.commands();

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0], ("--add", MAIN_X_CHANNEL));
    }

    #[test]
    fn test_conda_channel_action_commands_format() {
        let action = CondaChannelAction::AddMainX;
        let commands = action.commands();

        for (flag, channel) in commands {
            assert!(flag.starts_with("--"), "Flag should start with --");
            assert!(!channel.is_empty(), "Channel should not be empty");
        }
    }

    // ========================================================================
    // PixiChannelAction::command_display tests
    // ========================================================================

    #[test]
    fn test_pixi_channel_action_add_main_x_display() {
        let action = PixiChannelAction::AddMainX;
        let cmd = action.command_display();

        assert!(cmd.contains("pixi config prepend"));
        assert!(cmd.contains(MAIN_X_CHANNEL));
    }

    #[test]
    fn test_pixi_channel_action_remove_main_x_display() {
        let action = PixiChannelAction::RemoveMainX;
        let cmd = action.command_display();

        assert!(cmd.contains("pixi config unset"));
    }
}
