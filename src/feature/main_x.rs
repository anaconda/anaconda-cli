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
    RemoveDefaultChannel(String),
    /// Remove a channel from channels
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
            MainXCondaAction::RemoveDefaultChannel(channel) => {
                format!("conda config --remove default_channels {}", channel)
            }
            MainXCondaAction::RemoveChannel(channel) => {
                format!("conda config --remove channels {}", channel)
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
            MainXCondaAction::RemoveDefaultChannel(channel) => {
                // Ignore "not present" errors
                let _ = run_conda_config(conda_bin, &["--remove", "default_channels", channel]);
            }
            MainXCondaAction::RemoveChannel(channel) => {
                // Ignore "not present" errors
                let _ = run_conda_config(conda_bin, &["--remove", "channels", channel]);
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
/// Ensures all required default_channels are present (main, main-x, msys2, r)
/// and that "defaults" is in the channels list.
/// Ensures main comes before main-x (so main-x packages override main).
fn plan_conda_enable_actions(
    channels: &[String],
    default_channels: &[String],
    urls: &ChannelUrls,
    is_premium: bool,
) -> Vec<MainXCondaAction> {
    let mut actions = vec![];

    // Track which required channels are already present (correct tier)
    let mut have_main = false;
    let mut have_main_x = false;
    let mut have_msys2 = false;
    let mut have_r = false;

    // Step 1: Remove wrong-tier channels from channels list, track correct ones
    for channel in channels {
        if is_wrong_tier_channel(channel, is_premium) {
            actions.push(MainXCondaAction::RemoveChannel(channel.clone()));
        } else if channel == &urls.main {
            have_main = true;
        } else if channel == &urls.main_x {
            have_main_x = true;
        } else if channel == &urls.msys2 {
            have_msys2 = true;
        } else if channel == &urls.r {
            have_r = true;
        }
    }

    // Step 2: Remove wrong-tier channels from default_channels, track correct ones
    for channel in default_channels {
        if is_wrong_tier_channel(channel, is_premium) {
            actions.push(MainXCondaAction::RemoveDefaultChannel(channel.clone()));
        } else if channel == &urls.main {
            have_main = true;
        } else if channel == &urls.main_x {
            have_main_x = true;
        } else if channel == &urls.msys2 {
            have_msys2 = true;
        } else if channel == &urls.r {
            have_r = true;
        }
    }

    // Step 3: Add missing required channels in reverse order (since --add prepends)
    // Final order: main, main-x, msys2, r
    if !have_r {
        actions.push(MainXCondaAction::AddDefaultChannel(urls.r.clone()));
    }
    if !have_msys2 {
        actions.push(MainXCondaAction::AddDefaultChannel(urls.msys2.clone()));
    }
    if !have_main_x {
        actions.push(MainXCondaAction::AddDefaultChannel(urls.main_x.clone()));
    }
    if !have_main {
        actions.push(MainXCondaAction::AddDefaultChannel(urls.main.clone()));
    }

    // Step 4: Ensure "defaults" is in channels list
    if !channels.iter().any(|c| c == "defaults") {
        actions.push(MainXCondaAction::EnsureDefaultsInChannels);
    }

    actions
}

/// Plan the actions needed to disable main-x channel for conda.
///
/// Checks for main-x URLs from both premium (.cloud) and free (.com) domains.
fn plan_conda_disable_actions(default_channels: &[String]) -> Vec<MainXCondaAction> {
    // Check for main-x from either domain in default_channels
    let main_x_url = default_channels
        .iter()
        .find(|c| c.ends_with("/repo/main-x"));
    if let Some(url) = main_x_url {
        vec![MainXCondaAction::RemoveDefaultChannel(url.clone())]
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

        // Add missing required channels for the correct tier
        // Insert in reverse order so final order is: main, main-x, msys2, r
        // (main must come before main-x so main-x packages override main)
        for required in [&urls.r, &urls.msys2, &urls.main_x, &urls.main] {
            if !new_channels.iter().any(|c| c == required) {
                new_channels.insert(0, required.clone());
            }
        }

        return vec![MainXPixiAction::UpgradeChannels(new_channels)];
    }

    // Check if we need main-x but main is already present
    // In this case, prepending main-x would put it before main (wrong order)
    // So we need to do a full rewrite to ensure main comes before main-x
    let needs_main_x = !current_channels.iter().any(|c| c == &urls.main_x);
    let has_main = current_channels.iter().any(|c| c == &urls.main);

    if needs_main_x && has_main {
        // Rewrite to ensure correct ordering: main before main-x
        let mut new_channels: Vec<String> = current_channels.to_vec();

        // Insert missing required channels at the right positions
        // We want: ..., main, main-x, ... (main-x right after main)
        let main_pos = new_channels.iter().position(|c| c == &urls.main).unwrap();

        // Insert main-x right after main
        new_channels.insert(main_pos + 1, urls.main_x.clone());

        // Add msys2 and r at the end if missing
        if !new_channels.iter().any(|c| c == &urls.msys2) {
            new_channels.push(urls.msys2.clone());
        }
        if !new_channels.iter().any(|c| c == &urls.r) {
            new_channels.push(urls.r.clone());
        }

        return vec![MainXPixiAction::UpgradeChannels(new_channels)];
    }

    // Simple case: just prepend missing channels (in reverse order since prepend)
    // Final order should be: main, main-x, msys2, r
    // (main must come before main-x so main-x packages override main)
    let required_channels = [
        (&urls.r, "r"),
        (&urls.msys2, "msys2"),
        (&urls.main_x, "main_x"),
        (&urls.main, "main"),
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

/// Best-effort check for whether main-x is currently enabled for conda.
///
/// Returns `false` if conda isn't installed or the check otherwise fails,
/// since this is only used to decide whether to print an informational
/// warning, not to drive any state-changing behavior.
fn is_main_x_enabled_conda() -> bool {
    find_conda()
        .and_then(|conda_bin| get_default_channels_conda(&conda_bin))
        .map(|channels| channels.iter().any(|c| c.ends_with("/repo/main-x")))
        .unwrap_or(false)
}

/// Best-effort check for whether main-x is currently enabled for pixi.
///
/// Returns `false` if pixi isn't installed or the check otherwise fails,
/// since this is only used to decide whether to print an informational
/// warning, not to drive any state-changing behavior.
fn is_main_x_enabled_pixi() -> bool {
    find_pixi()
        .and_then(|pixi_bin| get_configured_channels_pixi(&pixi_bin))
        .map(|channels| channels.iter().any(|c| c.ends_with("/repo/main-x")))
        .unwrap_or(false)
}

/// Warn if main-x is still enabled for a tool other than the one that was
/// just acted on, so `ana feature disable main-x` (with no flag, or with
/// the flag for the tool that was already disabled) doesn't read as "fully
/// disabled" when it only ever touches one tool's configuration.
fn warn_if_main_x_enabled_elsewhere(other_tool: &str, other_tool_flag: &str) {
    let still_enabled = match other_tool {
        "conda" => is_main_x_enabled_conda(),
        "pixi" => is_main_x_enabled_pixi(),
        _ => false,
    };

    if still_enabled {
        status::blank_line();
        status::warn(&format!(
            "{} is still enabled for {}.",
            status::highlight("main-x"),
            other_tool
        ));
        status::info(&format!(
            "To disable for {}, run: {}",
            other_tool,
            status::highlight(&format!("ana feature disable main-x {}", other_tool_flag))
        ));
    }
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
            "{} feature is not enabled for conda",
            status::highlight("main-x")
        ));
        warn_if_main_x_enabled_elsewhere("pixi", "--pixi");
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

    warn_if_main_x_enabled_elsewhere("pixi", "--pixi");

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
        warn_if_main_x_enabled_elsewhere("conda", "--conda");
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

    warn_if_main_x_enabled_elsewhere("conda", "--conda");

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

        // Verify add order: r, msys2, main-x, main (reverse since --add prepends)
        // Final order after prepends: main, main-x, msys2, r
        let add_order: Vec<String> = actions
            .iter()
            .filter_map(|a| match a {
                MainXCondaAction::AddDefaultChannel(c) => Some(c.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            add_order,
            vec![
                urls.r.clone(),
                urls.msys2.clone(),
                urls.main_x.clone(),
                urls.main.clone()
            ],
            "Add order should result in main before main-x after prepends"
        );
    }

    #[test]
    fn test_plan_conda_enable_actions_all_present() {
        let urls = premium_urls();
        let channels = vec!["defaults".to_string()];
        let default_channels = vec![
            urls.main.clone(),
            urls.main_x.clone(),
            urls.msys2.clone(),
            urls.r.clone(),
        ];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &urls, true);

        // All correct-tier channels already present, nothing to do
        assert!(
            actions.is_empty(),
            "No actions needed when all correct channels present"
        );
    }

    #[test]
    fn test_plan_conda_enable_actions_partial_default_channels() {
        let urls = premium_urls();
        let channels = vec!["defaults".to_string()];
        let default_channels = vec![urls.main_x.clone(), urls.main.clone()];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &urls, true);

        // main and main-x are correct tier, just need to add msys2 and r
        let remove_count = actions
            .iter()
            .filter(|a| matches!(a, MainXCondaAction::RemoveDefaultChannel(_)))
            .count();
        let add_count = actions
            .iter()
            .filter(|a| matches!(a, MainXCondaAction::AddDefaultChannel(_)))
            .count();

        assert_eq!(
            remove_count, 0,
            "No removes needed - channels are correct tier"
        );
        assert_eq!(add_count, 2, "Should add msys2 and r");
    }

    #[test]
    fn test_plan_conda_enable_actions_main_x_only() {
        let urls = premium_urls();
        let channels = vec!["defaults".to_string()];
        let default_channels = vec![urls.main_x.clone()];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &urls, true);

        // main-x is correct tier, just need to add main, msys2, r
        let remove_count = actions
            .iter()
            .filter(|a| matches!(a, MainXCondaAction::RemoveDefaultChannel(_)))
            .count();
        let add_count = actions
            .iter()
            .filter(|a| matches!(a, MainXCondaAction::AddDefaultChannel(_)))
            .count();

        assert_eq!(
            remove_count, 0,
            "No removes needed - main-x is correct tier"
        );
        assert_eq!(add_count, 3, "Should add main, msys2, r");
    }

    #[test]
    fn test_plan_conda_enable_actions_main_only() {
        let urls = premium_urls();
        let channels = vec!["defaults".to_string()];
        let default_channels = vec![urls.main.clone()];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &urls, true);

        // main is correct tier, just need to add main-x, msys2, r
        let remove_count = actions
            .iter()
            .filter(|a| matches!(a, MainXCondaAction::RemoveDefaultChannel(_)))
            .count();
        let add_count = actions
            .iter()
            .filter(|a| matches!(a, MainXCondaAction::AddDefaultChannel(_)))
            .count();

        assert_eq!(remove_count, 0, "No removes needed - main is correct tier");
        assert_eq!(add_count, 3, "Should add main-x, msys2, r");

        // Verify the add order: r, msys2, main-x (reverse of final order since --add prepends)
        let add_order: Vec<String> = actions
            .iter()
            .filter_map(|a| match a {
                MainXCondaAction::AddDefaultChannel(c) => Some(c.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            add_order,
            vec![urls.r.clone(), urls.msys2.clone(), urls.main_x.clone()]
        );
    }

    #[test]
    fn test_plan_conda_enable_actions_upgrade_free_to_premium() {
        let free = free_urls();
        let premium = premium_urls();
        let channels = vec!["defaults".to_string()];
        // Free tier has .com/pkgs/main (wrong for premium)
        // main-x is always .cloud/repo so it's correct for both tiers
        let default_channels = vec![free.main.clone(), free.main_x.clone()];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &premium, true);

        // Should remove free.main (wrong tier) but keep main-x (correct for both)
        // Then add premium main, msys2, r
        let remove_count = actions
            .iter()
            .filter(|a| matches!(a, MainXCondaAction::RemoveDefaultChannel(_)))
            .count();
        let add_count = actions
            .iter()
            .filter(|a| matches!(a, MainXCondaAction::AddDefaultChannel(_)))
            .count();

        assert_eq!(remove_count, 1, "Should remove free.main only");
        assert_eq!(add_count, 3, "Should add premium main, msys2, r");
    }

    #[test]
    fn test_plan_conda_enable_actions_no_remove_when_no_channels() {
        let free = free_urls();
        let channels = vec!["defaults".to_string()];
        let default_channels: Vec<String> = vec![];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &free, false);

        // Should not have any remove actions when no existing managed channels
        let remove_count = actions
            .iter()
            .filter(|a| matches!(a, MainXCondaAction::RemoveDefaultChannel(_)))
            .count();
        assert_eq!(remove_count, 0);

        // Should add all 4 channels
        let add_count = actions
            .iter()
            .filter(|a| matches!(a, MainXCondaAction::AddDefaultChannel(_)))
            .count();
        assert_eq!(add_count, 4);
    }

    #[test]
    fn test_plan_conda_enable_actions_downgrade_premium_to_free() {
        let free = free_urls();
        let premium = premium_urls();
        let channels = vec!["defaults".to_string()];
        // Premium has .cloud/repo/main (wrong for free tier)
        // main-x is always .cloud/repo so it's correct for both tiers
        let default_channels = vec![premium.main.clone(), premium.main_x.clone()];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &free, false);

        // Should remove premium.main (wrong tier) but keep main-x (correct for both)
        // Then add free main, msys2, r
        let remove_count = actions
            .iter()
            .filter(|a| matches!(a, MainXCondaAction::RemoveDefaultChannel(_)))
            .count();
        let add_count = actions
            .iter()
            .filter(|a| matches!(a, MainXCondaAction::AddDefaultChannel(_)))
            .count();

        assert_eq!(remove_count, 1, "Should remove premium.main only");
        assert_eq!(add_count, 3, "Should add free main, msys2, r");
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

        // When main is present but main-x is not, we need to rewrite to ensure
        // main comes before main-x (prepending main-x would put it first)
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            MainXPixiAction::UpgradeChannels(channels) => {
                // Verify main comes before main-x
                let main_pos = channels.iter().position(|c| c == &urls.main).unwrap();
                let main_x_pos = channels.iter().position(|c| c == &urls.main_x).unwrap();
                assert!(
                    main_pos < main_x_pos,
                    "main should come before main-x, got main at {} and main-x at {}",
                    main_pos,
                    main_x_pos
                );
                // All required channels should be present
                assert!(channels.contains(&urls.main));
                assert!(channels.contains(&urls.main_x));
                assert!(channels.contains(&urls.msys2));
                assert!(channels.contains(&urls.r));
            }
            _ => panic!("Expected UpgradeChannels action"),
        }
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
    fn test_conda_channel_action_remove_default_channel() {
        let urls = premium_urls();
        let action = MainXCondaAction::RemoveDefaultChannel(urls.main_x.clone());
        let cmd = action.command_display();

        assert!(cmd.contains("conda config --remove default_channels"));
        assert!(cmd.contains(&urls.main_x));
    }

    #[test]
    fn test_conda_channel_action_remove_channel() {
        let urls = premium_urls();
        let action = MainXCondaAction::RemoveChannel(urls.main_x.clone());
        let cmd = action.command_display();

        assert!(cmd.contains("conda config --remove channels"));
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
