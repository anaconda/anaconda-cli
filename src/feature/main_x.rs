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

const REPO_HOST_CLOUD: &str = "repo.anaconda.cloud";
const REPO_HOST_COM: &str = "repo.anaconda.com";
const ANACONDA_DOWNLOAD_URL: &str = "https://www.anaconda.com/download";

/// Channel URLs for conda/pixi configuration.
#[derive(Clone)]
struct ChannelUrls {
    main: String,
    main_x: String,
    msys2: String,
    r: String,
    /// The host to use for pixi auth login
    host: String,
}

impl ChannelUrls {
    fn new(is_premium: bool) -> Self {
        // main-x is always from .cloud/repo
        let main_x = format!("https://{}/repo/main-x", REPO_HOST_CLOUD);

        if is_premium {
            // Premium: all channels from .cloud/repo
            Self {
                main_x,
                main: format!("https://{}/repo/main", REPO_HOST_CLOUD),
                msys2: format!("https://{}/repo/msys2", REPO_HOST_CLOUD),
                r: format!("https://{}/repo/r", REPO_HOST_CLOUD),
                host: REPO_HOST_CLOUD.to_string(),
            }
        } else {
            // Free: main-x from .cloud/repo, others from .com/pkgs
            Self {
                main_x,
                main: format!("https://{}/pkgs/main", REPO_HOST_COM),
                msys2: format!("https://{}/pkgs/msys2", REPO_HOST_COM),
                r: format!("https://{}/pkgs/r", REPO_HOST_COM),
                host: REPO_HOST_CLOUD.to_string(), // Auth is still against .cloud for main-x
            }
        }
    }

    fn required_default_channels(&self) -> Vec<&str> {
        vec![&self.main_x, &self.main, &self.msys2, &self.r]
    }
}

/// Detect the repo host from configured channel URLs.
/// Since main-x always uses .cloud, we just return .cloud for auth purposes.
fn detect_repo_host(_channels: &[String]) -> String {
    // Auth is always against .cloud since main-x is always from .cloud
    REPO_HOST_CLOUD.to_string()
}

/// Check if a channel is from the wrong tier and should be removed.
/// Premium tier uses .cloud/repo for all channels.
/// Free tier uses .cloud/repo for main-x, .com/pkgs for others.
fn is_wrong_tier_channel(channel: &str, is_premium: bool) -> bool {
    if is_premium {
        // Premium: remove any .com/pkgs channels (should use .cloud/repo instead)
        channel.contains(REPO_HOST_COM) && channel.contains("/pkgs/")
    } else {
        // Free: remove any .cloud/repo channels EXCEPT main-x
        channel.contains(REPO_HOST_CLOUD)
            && channel.contains("/repo/")
            && !channel.ends_with("/repo/main-x")
    }
}

/// Represents a channel configuration action for enabling/disabling main-x via conda.
enum MainXCondaAction {
    /// Add a channel to default_channels (used for main-x, main, msys2, r)
    AddDefaultChannel(String),
    /// Add "defaults" to channels list
    EnsureDefaultsInChannels,
    /// Remove a channel from default_channels
    RemoveChannel(String),
}

impl MainXCondaAction {
    fn command_display(&self) -> String {
        match self {
            MainXCondaAction::AddDefaultChannel(channel) => {
                format!("conda config --add default_channels {}", channel)
            }
            MainXCondaAction::EnsureDefaultsInChannels => {
                "conda config --add channels defaults".to_string()
            }
            MainXCondaAction::RemoveChannel(channel) => {
                format!("conda config --remove default_channels {}", channel)
            }
        }
    }

    fn execute_with_status(&self, conda_bin: &Path) -> miette::Result<()> {
        let cmd = self.command_display();
        status::running(&format!("Running {}", status::highlight(&cmd)));

        match self {
            MainXCondaAction::AddDefaultChannel(channel) => {
                run_conda_config(conda_bin, &["--add", "default_channels", channel])?;
            }
            MainXCondaAction::EnsureDefaultsInChannels => {
                run_conda_config(conda_bin, &["--add", "channels", "defaults"])?;
            }
            MainXCondaAction::RemoveChannel(channel) => {
                // Ignore "not present" errors
                let _ = run_conda_config(conda_bin, &["--remove", "default_channels", channel]);
            }
        }

        status::finish_running(&format!("Ran {}", status::highlight(&cmd)));
        Ok(())
    }
}

/// Represents a channel configuration action to be executed for pixi.
enum MainXPixiAction {
    /// Add a channel to default-channels.
    AddChannel(String),
    /// Remove main-x while preserving other channels.
    /// Contains the list of channels to keep after removal.
    RemoveMainX(Vec<String>),
    /// Rewrite channels (for tier upgrade/downgrade).
    /// Contains the new list of channels.
    UpgradeChannels(Vec<String>),
}

impl MainXPixiAction {
    fn command_display(&self) -> String {
        match self {
            MainXPixiAction::AddChannel(url) => {
                format!("pixi config prepend --global default-channels {}", url)
            }
            MainXPixiAction::RemoveMainX(channels_to_keep) => {
                format_pixi_remove_main_x_command(channels_to_keep)
            }
            MainXPixiAction::UpgradeChannels(new_channels) => {
                format!(
                    "pixi config set --global default-channels [{}]",
                    format_pixi_channels_json(new_channels)
                )
            }
        }
    }

    fn execute_with_status(&self, pixi_bin: &Path) -> miette::Result<()> {
        let cmd = self.command_display();
        status::running(&format!("Running {}", status::highlight(&cmd)));
        match self {
            MainXPixiAction::AddChannel(url) => {
                run_pixi_config(pixi_bin, &["prepend", "--global", "default-channels", url])?;
            }
            MainXPixiAction::RemoveMainX(channels_to_keep) => {
                execute_pixi_remove_main_x(pixi_bin, channels_to_keep)?;
            }
            MainXPixiAction::UpgradeChannels(new_channels) => {
                let channels_json = format!("[{}]", format_pixi_channels_json(new_channels));
                run_pixi_config(
                    pixi_bin,
                    &["set", "--global", "default-channels", &channels_json],
                )?;
            }
        }
        status::finish_running(&format!("Ran {}", status::highlight(&cmd)));
        Ok(())
    }
}

/// Format the display command for removing main-x from pixi while preserving other channels.
fn format_pixi_remove_main_x_command(channels_to_keep: &[String]) -> String {
    if channels_to_keep.is_empty() {
        "pixi config unset --global default-channels".to_string()
    } else {
        format!(
            "pixi config set --global default-channels [{}]",
            format_pixi_channels_json(channels_to_keep)
        )
    }
}

/// Execute the pixi config command to remove main-x while preserving other channels.
fn execute_pixi_remove_main_x(pixi_bin: &Path, channels_to_keep: &[String]) -> miette::Result<()> {
    if channels_to_keep.is_empty() {
        run_pixi_config(pixi_bin, &["unset", "--global", "default-channels"])
    } else {
        let channels_json = format!("[{}]", format_pixi_channels_json(channels_to_keep));
        run_pixi_config(
            pixi_bin,
            &["set", "--global", "default-channels", &channels_json],
        )
    }
}

/// Format channels as a JSON-style string for pixi config (e.g., `"chan1", "chan2"`).
fn format_pixi_channels_json(channels: &[String]) -> String {
    channels
        .iter()
        .map(|c| format!("\"{}\"", c))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Configure pixi auth for the repo host.
fn run_pixi_auth_login(pixi_bin: &Path, api_key: &str, host: &str) -> miette::Result<()> {
    let cmd = format!("pixi auth login {} --token <token>", host);
    status::running(&format!("Running {}", status::highlight(&cmd)));

    let output = Command::new(pixi_bin)
        .args(["auth", "login", host, "--token", api_key])
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

/// Remove pixi auth for the repo host.
fn run_pixi_auth_logout(pixi_bin: &Path, host: &str) -> miette::Result<()> {
    let cmd = format!("pixi auth logout {}", host);
    status::running(&format!("Running {}", status::highlight(&cmd)));

    let output = Command::new(pixi_bin)
        .args(["auth", "logout", host])
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
/// Ensures all required default_channels are present (main-x, main, msys2, r)
/// and that "defaults" is in the channels list.
/// Removes channels from the wrong tier (free vs premium) based on subscription status.
fn plan_conda_enable_actions(
    channels: &[String],
    default_channels: &[String],
    urls: &ChannelUrls,
    is_premium: bool,
) -> Vec<MainXCondaAction> {
    let mut actions = vec![];

    // Remove channels from the wrong tier
    for channel in default_channels {
        if is_wrong_tier_channel(channel, is_premium) {
            actions.push(MainXCondaAction::RemoveChannel(channel.clone()));
        }
    }

    let required = urls.required_default_channels();

    // Add any missing required default_channels (in reverse order since --add prepends)
    for channel in required.iter().rev() {
        if !default_channels.iter().any(|c| c == *channel) {
            actions.push(MainXCondaAction::AddDefaultChannel((*channel).to_string()));
        }
    }

    // Ensure "defaults" is in channels list
    if !channels.iter().any(|c| c == "defaults") {
        actions.push(MainXCondaAction::EnsureDefaultsInChannels);
    }

    actions
}

/// Plan the actions needed to disable main-x channel for conda.
///
/// Checks for main-x URLs from both premium (.cloud) and free (.com) domains.
fn plan_conda_disable_actions(current_channels: &[String]) -> Vec<MainXCondaAction> {
    // Check for main-x from either domain
    let main_x_url = current_channels
        .iter()
        .find(|c| c.ends_with("/repo/main-x"));
    if let Some(url) = main_x_url {
        vec![MainXCondaAction::RemoveChannel(url.clone())]
    } else {
        vec![]
    }
}

/// Plan the actions needed to enable main-x channel for pixi.
///
/// Ensures all required channels are present (main-x, main, msys2, r).
/// Removes channels from the wrong tier (free vs premium) based on subscription status.
fn plan_pixi_enable_actions(
    current_channels: &[String],
    urls: &ChannelUrls,
    is_premium: bool,
) -> Vec<MainXPixiAction> {
    let mut actions = vec![];

    // Check if there are channels from the wrong tier that need to be removed
    let has_wrong_tier = current_channels
        .iter()
        .any(|c| is_wrong_tier_channel(c, is_premium));

    // If there are wrong-tier channels, do a full rewrite
    if has_wrong_tier {
        // Filter out wrong-tier channels and collect the rest
        let mut new_channels: Vec<String> = current_channels
            .iter()
            .filter(|c| !is_wrong_tier_channel(c, is_premium))
            .cloned()
            .collect();

        // Add missing required channels for the correct tier (in reverse priority order)
        for required in [&urls.r, &urls.msys2, &urls.main, &urls.main_x] {
            if !new_channels.iter().any(|c| c == required) {
                new_channels.insert(0, required.clone());
            }
        }

        return vec![MainXPixiAction::UpgradeChannels(new_channels)];
    }

    // No tier change needed, just add missing channels (in reverse order since prepend)
    let required_channels = [
        (&urls.r, "r"),
        (&urls.msys2, "msys2"),
        (&urls.main, "main"),
        (&urls.main_x, "main_x"),
    ];

    for (url, _name) in required_channels {
        if !current_channels.iter().any(|c| c == url) {
            actions.push(MainXPixiAction::AddChannel(url.clone()));
        }
    }

    actions
}

/// Plan the actions needed to disable main-x channel for pixi.
///
/// Removes main-x from the channel list while preserving all other channels.
/// If main-x is the only channel, the result will unset default-channels entirely.
/// Checks for main-x URLs from both premium (.cloud) and free (.com) domains.
fn plan_pixi_disable_actions(current_channels: &[String]) -> Vec<MainXPixiAction> {
    // Check for main-x from either domain
    let has_main_x = current_channels.iter().any(|c| c.ends_with("/repo/main-x"));
    if has_main_x {
        let channels_to_keep: Vec<String> = current_channels
            .iter()
            .filter(|c| !c.ends_with("/repo/main-x"))
            .cloned()
            .collect();
        vec![MainXPixiAction::RemoveMainX(channels_to_keep)]
    } else {
        vec![]
    }
}

/// Enable main-x channel access via conda.
///
/// This command:
/// 1. Ensures the user is logged in to Anaconda
/// 2. Checks subscription status to determine repo URL (.cloud for premium, .com for free)
/// 3. Shows planned changes and prompts for confirmation
/// 4. Adds the main-x channel to conda configuration
/// 5. Provides instructions for reverting the changes
pub async fn enable_main_x_conda(ctx: &CommandContext, force: bool) -> miette::Result<()> {
    status::info(&format!(
        "Enabling {} feature via {}...",
        status::highlight("main-x"),
        status::highlight("conda")
    ));
    status::blank_line();

    // Step 1: Check login status and prompt if needed
    auth::ensure_logged_in(ctx).await?;

    // Step 2: Check subscription status to determine repo URL
    let is_premium = auth::has_premium_subscription(ctx).await?;
    let urls = ChannelUrls::new(is_premium);

    // Step 3: Determine what changes need to be made
    let conda_bin = find_conda()?;
    let channels = get_channels_conda(&conda_bin)?;
    let default_channels = get_default_channels_conda(&conda_bin)?;
    let actions = plan_conda_enable_actions(&channels, &default_channels, &urls, is_premium);

    if actions.is_empty() {
        status::success("Feature already enabled");
        return Ok(());
    }

    // Step 4: Show planned changes
    status::blank_line();
    status::info("The following commands will be run:");
    for action in &actions {
        eprintln!("  {}", status::highlight(&action.command_display()));
    }
    status::blank_line();

    // Step 5: Prompt for confirmation unless --force
    if !force && !prompt_yes_no("Proceed?", true) {
        eprintln!("Aborted.");
        return Ok(());
    }

    // Step 6: Execute the changes
    status::blank_line();
    for action in &actions {
        action.execute_with_status(&conda_bin)?;
    }

    // Step 7: Show success message and undo instructions
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
/// 2. Checks subscription status to determine repo URL (.cloud for premium, .com for free)
/// 3. Shows planned changes and prompts for confirmation
/// 4. Configures pixi auth for the repo host
/// 5. Adds the main-x channel to pixi global configuration
/// 6. Provides instructions for reverting the changes
pub async fn enable_main_x_pixi(ctx: &CommandContext, force: bool) -> miette::Result<()> {
    status::info(&format!(
        "Enabling {} feature via {}...",
        status::highlight("main-x"),
        status::highlight("pixi")
    ));
    status::blank_line();

    // Step 1: Check login status and prompt if needed
    auth::ensure_logged_in(ctx).await?;

    // Step 2: Check subscription status to determine repo URL
    let is_premium = auth::has_premium_subscription(ctx).await?;
    let urls = ChannelUrls::new(is_premium);

    // Step 3: Determine what changes need to be made
    let pixi_bin = find_pixi()?;
    let current_channels = get_configured_channels_pixi(&pixi_bin)?;
    let actions = plan_pixi_enable_actions(&current_channels, &urls, is_premium);

    if actions.is_empty() {
        status::success("Feature already enabled");
        return Ok(());
    }

    // Step 4: Show planned changes
    status::blank_line();
    status::info("The following commands will be run:");
    eprintln!(
        "  {}",
        status::highlight(&format!("pixi auth login {} --token <token>", urls.host))
    );
    for action in &actions {
        eprintln!("  {}", status::highlight(&action.command_display()));
    }
    status::blank_line();

    // Step 5: Prompt for confirmation unless --force
    if !force && !prompt_yes_no("Proceed?", true) {
        eprintln!("Aborted.");
        return Ok(());
    }

    // Step 6: Execute the changes
    status::blank_line();

    // Get the API key for auth
    let api_key = auth::get_api_key(&ctx.config)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("Not logged in"))?;

    // Configure pixi auth first
    run_pixi_auth_login(&pixi_bin, &api_key, &urls.host)?;

    // Then configure channels
    for action in &actions {
        action.execute_with_status(&pixi_bin)?;
    }

    // Step 7: Show success message and undo instructions
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
    let current_channels = get_default_channels_conda(&conda_bin)?;
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
        eprintln!("  {}", status::highlight(&action.command_display()));
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

    // Detect which host to logout from based on current channel URLs
    let host = detect_repo_host(&current_channels);

    // Show planned changes
    status::info("The following commands will be run:");
    for action in &actions {
        eprintln!("  {}", status::highlight(&action.command_display()));
    }
    eprintln!(
        "  {}",
        status::highlight(&format!("pixi auth logout {}", host))
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
    run_pixi_auth_logout(&pixi_bin, &host)?;

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
    let output = Command::new(conda_bin)
        .arg("config")
        .args(args)
        .output()
        .into_diagnostic()
        .context("failed to run conda config")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(miette::miette!(
            "conda config {} failed: {}",
            args.join(" "),
            stderr.trim()
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
            "This feature currently requires conda to be installed separately. Install it from: {}",
            status::highlight(ANACONDA_DOWNLOAD_URL)
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
fn get_channels_conda(conda_bin: &Path) -> miette::Result<Vec<String>> {
    get_conda_config_list(conda_bin, "channels")
}

/// Get the list of currently configured default_channels from conda config --show.
fn get_default_channels_conda(conda_bin: &Path) -> miette::Result<Vec<String>> {
    get_conda_config_list(conda_bin, "default_channels")
}

/// Get a list config value from conda config --show.
///
/// The output format is:
/// ```
/// <key>:
///   - value1
///   - value2
/// ```
fn get_conda_config_list(conda_bin: &Path, key: &str) -> miette::Result<Vec<String>> {
    let output = Command::new(conda_bin)
        .args(["config", "--show", key])
        .output()
        .into_diagnostic()
        .context(format!("failed to run conda config --show {}", key))?;

    if !output.status.success() {
        return Ok(vec![]);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    let values: Vec<String> = stdout
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

    Ok(values)
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

    fn premium_urls() -> ChannelUrls {
        ChannelUrls::new(true)
    }

    fn free_urls() -> ChannelUrls {
        ChannelUrls::new(false)
    }

    #[test]
    fn test_channel_urls_premium() {
        let urls = premium_urls();
        assert_eq!(urls.main_x, "https://repo.anaconda.cloud/repo/main-x");
        assert_eq!(urls.main, "https://repo.anaconda.cloud/repo/main");
        assert_eq!(urls.msys2, "https://repo.anaconda.cloud/repo/msys2");
        assert_eq!(urls.r, "https://repo.anaconda.cloud/repo/r");
        assert_eq!(urls.host, "repo.anaconda.cloud");
    }

    #[test]
    fn test_channel_urls_free() {
        let urls = free_urls();
        // main-x is always from .cloud/repo
        assert_eq!(urls.main_x, "https://repo.anaconda.cloud/repo/main-x");
        // others are from .com/pkgs
        assert_eq!(urls.main, "https://repo.anaconda.com/pkgs/main");
        assert_eq!(urls.msys2, "https://repo.anaconda.com/pkgs/msys2");
        assert_eq!(urls.r, "https://repo.anaconda.com/pkgs/r");
        // host is still .cloud for auth (main-x)
        assert_eq!(urls.host, "repo.anaconda.cloud");
    }

    #[test]
    fn test_is_wrong_tier_channel_premium() {
        // Premium user should remove .com/pkgs channels
        assert!(is_wrong_tier_channel(
            "https://repo.anaconda.com/pkgs/main",
            true
        ));
        assert!(is_wrong_tier_channel(
            "https://repo.anaconda.com/pkgs/msys2",
            true
        ));
        // Premium user should NOT remove .cloud/repo channels
        assert!(!is_wrong_tier_channel(
            "https://repo.anaconda.cloud/repo/main",
            true
        ));
        assert!(!is_wrong_tier_channel(
            "https://repo.anaconda.cloud/repo/main-x",
            true
        ));
    }

    #[test]
    fn test_is_wrong_tier_channel_free() {
        // Free user should remove .cloud/repo channels EXCEPT main-x
        assert!(is_wrong_tier_channel(
            "https://repo.anaconda.cloud/repo/main",
            false
        ));
        assert!(is_wrong_tier_channel(
            "https://repo.anaconda.cloud/repo/msys2",
            false
        ));
        // Free user should NOT remove main-x (it's always from .cloud)
        assert!(!is_wrong_tier_channel(
            "https://repo.anaconda.cloud/repo/main-x",
            false
        ));
        // Free user should NOT remove .com/pkgs channels
        assert!(!is_wrong_tier_channel(
            "https://repo.anaconda.com/pkgs/main",
            false
        ));
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
        let urls = premium_urls();
        let channels: Vec<String> = vec![];
        let default_channels: Vec<String> = vec![];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &urls, true);

        // Should add all 4 required default_channels plus "defaults" to channels
        assert_eq!(actions.len(), 5);
        // Check that defaults is added to channels
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, MainXCondaAction::EnsureDefaultsInChannels))
        );
    }

    #[test]
    fn test_plan_conda_enable_actions_defaults_in_channels() {
        let urls = premium_urls();
        let channels = vec!["defaults".to_string()];
        let default_channels: Vec<String> = vec![];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &urls, true);

        // Should add all 4 required default_channels, but not "defaults" to channels
        assert_eq!(actions.len(), 4);
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, MainXCondaAction::EnsureDefaultsInChannels))
        );
    }

    #[test]
    fn test_plan_conda_enable_actions_all_present() {
        let urls = premium_urls();
        let channels = vec!["defaults".to_string()];
        let default_channels = vec![
            urls.main_x.clone(),
            urls.main.clone(),
            urls.msys2.clone(),
            urls.r.clone(),
        ];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &urls, true);

        assert!(
            actions.is_empty(),
            "No actions needed when all channels already configured"
        );
    }

    #[test]
    fn test_plan_conda_enable_actions_partial_default_channels() {
        let urls = premium_urls();
        let channels = vec!["defaults".to_string()];
        let default_channels = vec![urls.main_x.clone(), urls.main.clone()];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &urls, true);

        // Should only add msys2 and r (the missing ones)
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn test_plan_conda_enable_actions_main_x_only() {
        let urls = premium_urls();
        let channels = vec!["defaults".to_string()];
        let default_channels = vec![urls.main_x.clone()];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &urls, true);

        // Should add main, msys2, and r (the missing ones)
        assert_eq!(actions.len(), 3);
    }

    #[test]
    fn test_plan_conda_enable_actions_upgrade_free_to_premium() {
        let free = free_urls();
        let premium = premium_urls();
        let channels = vec!["defaults".to_string()];
        // Free tier has .com/pkgs channels (except main-x which is always .cloud/repo)
        let default_channels = vec![free.main.clone(), free.main_x.clone()];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &premium, true);

        // Should have 1 remove (.com/pkgs/main) + 3 adds (premium main, msys2, r)
        // main-x is already correct (.cloud/repo/main-x)
        let remove_count = actions
            .iter()
            .filter(|a| matches!(a, MainXCondaAction::RemoveChannel(_)))
            .count();
        assert_eq!(remove_count, 1); // Only free.main (.com/pkgs/main)

        let add_count = actions
            .iter()
            .filter(|a| matches!(a, MainXCondaAction::AddDefaultChannel(_)))
            .count();
        assert_eq!(add_count, 3); // premium main, msys2, r (main-x already present)
    }

    #[test]
    fn test_plan_conda_enable_actions_no_remove_when_correct_tier() {
        let free = free_urls();
        let channels = vec!["defaults".to_string()];
        let default_channels: Vec<String> = vec![];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &free, false);

        // Should not have any remove actions when no wrong-tier channels
        let remove_count = actions
            .iter()
            .filter(|a| matches!(a, MainXCondaAction::RemoveChannel(_)))
            .count();
        assert_eq!(remove_count, 0);
    }

    #[test]
    fn test_plan_conda_enable_actions_downgrade_premium_to_free() {
        let free = free_urls();
        let premium = premium_urls();
        let channels = vec!["defaults".to_string()];
        // Premium has all .cloud/repo channels
        let default_channels = vec![premium.main.clone(), premium.main_x.clone()];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &free, false);

        // Should have 1 remove (.cloud/repo/main, NOT main-x) + 3 adds (free main, msys2, r)
        // main-x stays (.cloud/repo/main-x is correct for both tiers)
        let remove_count = actions
            .iter()
            .filter(|a| matches!(a, MainXCondaAction::RemoveChannel(_)))
            .count();
        assert_eq!(remove_count, 1); // Only premium.main (.cloud/repo/main)

        let add_count = actions
            .iter()
            .filter(|a| matches!(a, MainXCondaAction::AddDefaultChannel(_)))
            .count();
        assert_eq!(add_count, 3); // free main, msys2, r (main-x already present)
    }

    // ========================================================================
    // plan_pixi_enable_actions tests
    // ========================================================================

    #[test]
    fn test_plan_pixi_enable_actions_empty_channels() {
        let urls = premium_urls();
        let current_channels: Vec<String> = vec![];
        let actions = plan_pixi_enable_actions(&current_channels, &urls, true);

        // Should add all 4 required channels
        assert_eq!(actions.len(), 4);
        assert!(
            actions
                .iter()
                .all(|a| matches!(a, MainXPixiAction::AddChannel(_)))
        );
    }

    #[test]
    fn test_plan_pixi_enable_actions_main_x_already_present() {
        let urls = premium_urls();
        let current_channels = vec![urls.main_x.clone()];
        let actions = plan_pixi_enable_actions(&current_channels, &urls, true);

        // Still need to add main, msys2, r
        assert_eq!(actions.len(), 3);
        assert!(
            actions
                .iter()
                .all(|a| matches!(a, MainXPixiAction::AddChannel(_)))
        );
    }

    #[test]
    fn test_plan_pixi_enable_actions_main_already_present() {
        let urls = premium_urls();
        let current_channels = vec![urls.main.clone()];
        let actions = plan_pixi_enable_actions(&current_channels, &urls, true);

        // Still need to add main-x, msys2, r
        assert_eq!(actions.len(), 3);
        assert!(
            actions
                .iter()
                .all(|a| matches!(a, MainXPixiAction::AddChannel(_)))
        );
    }

    #[test]
    fn test_plan_pixi_enable_actions_all_already_present() {
        let urls = premium_urls();
        let current_channels = vec![
            urls.main.clone(),
            urls.main_x.clone(),
            urls.msys2.clone(),
            urls.r.clone(),
        ];
        let actions = plan_pixi_enable_actions(&current_channels, &urls, true);

        assert!(actions.is_empty());
    }

    #[test]
    fn test_plan_pixi_enable_actions_upgrade_free_to_premium() {
        let free = free_urls();
        let premium = premium_urls();
        // Free tier has .com/pkgs/main
        let current_channels = vec![free.main.clone()];
        let actions = plan_pixi_enable_actions(&current_channels, &premium, true);

        // Should have a single UpgradeChannels action
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            MainXPixiAction::UpgradeChannels(channels) => {
                assert!(channels.contains(&premium.main));
                assert!(channels.contains(&premium.main_x));
                assert!(channels.contains(&premium.msys2));
                assert!(channels.contains(&premium.r));
                // Should not contain .com/pkgs channels
                assert!(!channels.iter().any(|c| c.contains("/pkgs/")));
            }
            _ => panic!("Expected UpgradeChannels action"),
        }
    }

    #[test]
    fn test_plan_pixi_enable_actions_no_rewrite_when_correct_tier() {
        let free = free_urls();
        let current_channels: Vec<String> = vec![];
        let actions = plan_pixi_enable_actions(&current_channels, &free, false);

        // Should not have rewrite action when no wrong-tier channels
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, MainXPixiAction::UpgradeChannels(_)))
        );
    }

    #[test]
    fn test_plan_pixi_enable_actions_downgrade_premium_to_free() {
        let free = free_urls();
        let premium = premium_urls();
        // Premium has .cloud/repo/main
        let current_channels = vec![premium.main.clone()];
        let actions = plan_pixi_enable_actions(&current_channels, &free, false);

        // Should have a single UpgradeChannels action (rewrite to free tier)
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            MainXPixiAction::UpgradeChannels(channels) => {
                assert!(channels.contains(&free.main));
                assert!(channels.contains(&free.main_x));
                assert!(channels.contains(&free.msys2));
                assert!(channels.contains(&free.r));
                // Should not contain .cloud/repo channels except main-x
                let cloud_repo_non_main_x: Vec<_> = channels
                    .iter()
                    .filter(|c| {
                        c.contains(REPO_HOST_CLOUD)
                            && c.contains("/repo/")
                            && !c.ends_with("/main-x")
                    })
                    .collect();
                assert!(cloud_repo_non_main_x.is_empty());
            }
            _ => panic!("Expected UpgradeChannels action"),
        }
    }

    // ========================================================================
    // MainXCondaAction::command_display tests
    // ========================================================================

    #[test]
    fn test_conda_channel_action_add_default_channel() {
        let urls = premium_urls();
        let action = MainXCondaAction::AddDefaultChannel(urls.main_x.clone());
        let cmd = action.command_display();

        assert!(cmd.contains("conda config --add default_channels"));
        assert!(cmd.contains(&urls.main_x));
    }

    #[test]
    fn test_conda_channel_action_add_defaults_to_channels() {
        let action = MainXCondaAction::EnsureDefaultsInChannels;
        let cmd = action.command_display();

        assert!(cmd.contains("conda config --add channels defaults"));
    }

    #[test]
    fn test_conda_channel_action_remove_channel() {
        let urls = premium_urls();
        let action = MainXCondaAction::RemoveChannel(urls.main_x.clone());
        let cmd = action.command_display();

        assert!(cmd.contains("conda config --remove default_channels"));
        assert!(cmd.contains(&urls.main_x));
    }

    // ========================================================================
    // MainXPixiAction::command_display tests
    // ========================================================================

    #[test]
    fn test_pixi_channel_action_add_channel_display() {
        let urls = premium_urls();
        let action = MainXPixiAction::AddChannel(urls.main_x.clone());
        let cmd = action.command_display();

        assert!(cmd.contains("pixi config prepend"));
        assert!(cmd.contains(&urls.main_x));
    }

    #[test]
    fn test_pixi_channel_action_remove_main_x_display_empty() {
        let action = MainXPixiAction::RemoveMainX(vec![]);
        let cmd = action.command_display();

        assert!(cmd.contains("pixi config unset"));
    }

    #[test]
    fn test_pixi_channel_action_remove_main_x_display_with_channels() {
        let urls = premium_urls();
        let action = MainXPixiAction::RemoveMainX(vec![urls.main.clone()]);
        let cmd = action.command_display();

        assert!(cmd.contains("pixi config set"));
        assert!(cmd.contains(&urls.main));
    }

    // ========================================================================
    // plan_pixi_disable_actions tests
    // ========================================================================

    #[test]
    fn test_plan_pixi_disable_actions_main_and_main_x() {
        let urls = premium_urls();
        let current_channels = vec![urls.main.clone(), urls.main_x.clone()];
        let actions = plan_pixi_disable_actions(&current_channels);

        assert_eq!(actions.len(), 1);
        match &actions[0] {
            MainXPixiAction::RemoveMainX(channels_to_keep) => {
                assert_eq!(channels_to_keep, &vec![urls.main.clone()]);
            }
            _ => panic!("Expected RemoveMainX action"),
        }
    }

    #[test]
    fn test_plan_pixi_disable_actions_main_x_only() {
        let urls = premium_urls();
        let current_channels = vec![urls.main_x.clone()];
        let actions = plan_pixi_disable_actions(&current_channels);

        assert_eq!(actions.len(), 1);
        match &actions[0] {
            MainXPixiAction::RemoveMainX(channels_to_keep) => {
                assert!(channels_to_keep.is_empty());
            }
            _ => panic!("Expected RemoveMainX action"),
        }
    }

    #[test]
    fn test_plan_pixi_disable_actions_no_main_x() {
        let urls = premium_urls();
        let current_channels = vec![urls.main.clone()];
        let actions = plan_pixi_disable_actions(&current_channels);

        assert!(actions.is_empty());
    }

    #[test]
    fn test_plan_pixi_disable_actions_preserves_other_channels() {
        let urls = premium_urls();
        let current_channels = vec![
            urls.main.clone(),
            urls.main_x.clone(),
            "conda-forge".to_string(),
        ];
        let actions = plan_pixi_disable_actions(&current_channels);

        assert_eq!(actions.len(), 1);
        match &actions[0] {
            MainXPixiAction::RemoveMainX(channels_to_keep) => {
                assert_eq!(
                    channels_to_keep,
                    &vec![urls.main.clone(), "conda-forge".to_string()]
                );
            }
            _ => panic!("Expected RemoveMainX action"),
        }
    }

    // ========================================================================
    // detect_repo_host tests
    // ========================================================================

    #[test]
    fn test_detect_repo_host_always_cloud() {
        // Auth is always against .cloud since main-x is always from .cloud
        let channels = vec![
            "https://repo.anaconda.cloud/repo/main".to_string(),
            "conda-forge".to_string(),
        ];
        assert_eq!(detect_repo_host(&channels), REPO_HOST_CLOUD);

        let channels = vec![
            "https://repo.anaconda.com/pkgs/main".to_string(),
            "conda-forge".to_string(),
        ];
        assert_eq!(detect_repo_host(&channels), REPO_HOST_CLOUD);

        let channels = vec!["conda-forge".to_string()];
        assert_eq!(detect_repo_host(&channels), REPO_HOST_CLOUD);
    }
}
