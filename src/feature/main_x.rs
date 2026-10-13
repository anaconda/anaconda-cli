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
    /// The main channel to add when no main channel is configured yet.
    main: String,
    main_x: String,
    /// The host to use for pixi auth login
    host: String,
}

impl ChannelUrls {
    fn new(is_premium: bool) -> Self {
        Self {
            // main-x is always from .cloud/repo
            main_x: main_x_url(),
            main: if is_premium {
                main_url_premium()
            } else {
                main_url_free()
            },
            // Auth is always against .cloud since main-x is always from .cloud
            host: REPO_HOST_CLOUD.to_string(),
        }
    }
}

fn main_x_url() -> String {
    format!("https://{}/repo/main-x", REPO_HOST_CLOUD)
}

fn main_url_premium() -> String {
    format!("https://{}/repo/main", REPO_HOST_CLOUD)
}

fn main_url_free() -> String {
    format!("https://{}/pkgs/main", REPO_HOST_COM)
}

/// Whether a channel is an Anaconda main channel (from either tier).
fn is_main_channel(channel: &str) -> bool {
    let channel = channel.trim_end_matches('/');
    channel == main_url_premium() || channel == main_url_free()
}

/// Whether a channel is the Anaconda main-x channel.
fn is_main_x_channel(channel: &str) -> bool {
    channel.trim_end_matches('/').ends_with("/repo/main-x")
}

/// Whether an ordered channel list contains both main and main-x, with main
/// first (so main-x packages take priority over main).
fn has_main_then_main_x<'a>(channels: impl IntoIterator<Item = &'a str> + Clone) -> bool {
    let main_pos = channels.clone().into_iter().position(is_main_channel);
    let main_x_pos = channels.into_iter().position(is_main_x_channel);
    matches!((main_pos, main_x_pos), (Some(m), Some(x)) if m < x)
}

/// Detect the repo host from configured channel URLs.
/// Since main-x always uses .cloud, we just return .cloud for auth purposes.
fn detect_repo_host(_channels: &[String]) -> String {
    // Auth is always against .cloud since main-x is always from .cloud
    REPO_HOST_CLOUD.to_string()
}

/// Represents a channel configuration action for enabling/disabling main-x via conda.
enum MainXCondaAction {
    /// Add (prepend) a channel to default_channels
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
    /// Prepend a channel to default-channels.
    AddChannel(String),
    /// Remove main-x while preserving other channels.
    /// Contains the list of channels to keep after removal.
    RemoveMainX(Vec<String>),
    /// Rewrite default-channels (e.g. to place main-x right after main).
    /// Contains the new list of channels.
    SetChannels(Vec<String>),
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
            MainXPixiAction::SetChannels(new_channels) => {
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
            MainXPixiAction::SetChannels(new_channels) => {
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

/// The order in which conda searches channels: `channels`, with the
/// `defaults` entry expanded to `default_channels`.
///
/// If `defaults` is missing from `channels`, enabling main-x adds it with
/// `conda config --add`, which prepends, so `default_channels` is placed first.
fn conda_effective_channels<'a>(
    channels: &'a [String],
    default_channels: &'a [String],
) -> Vec<&'a str> {
    let mut effective: Vec<&str> = vec![];
    if !channels.iter().any(|c| c == "defaults") {
        effective.extend(default_channels.iter().map(String::as_str));
    }
    for channel in channels {
        if channel == "defaults" {
            effective.extend(default_channels.iter().map(String::as_str));
        } else {
            effective.push(channel);
        }
    }
    effective
}

/// Plan the actions needed to enable main-x channel for conda.
///
/// Looks for main and main-x in both `channels` and `default_channels`. If
/// both are already present, with main searched before main-x (so main-x
/// packages take priority over main), nothing changes. Otherwise main-x is
/// added to `default_channels` right after main, adding main first if it is
/// not configured anywhere.
///
/// When main has to go into `default_channels`, any existing main entry
/// there is removed and re-added rather than appended to: `conda config
/// --show` reports conda's built-in default_channels even when none are
/// explicitly configured, but the first `conda config --add` writes an
/// explicit list containing only the added channel, which would silently
/// drop an implicit main. Removing an entry that only exists implicitly fails
/// harmlessly and the error is ignored on execute.
fn plan_conda_enable_actions(
    channels: &[String],
    default_channels: &[String],
    urls: &ChannelUrls,
) -> Vec<MainXCondaAction> {
    let mut actions = vec![];
    let defaults_pos = channels.iter().position(|c| c == "defaults");
    let effective = conda_effective_channels(channels, default_channels);

    if has_main_then_main_x(effective.iter().copied()) {
        // Already configured. default_channels is only searched through the
        // "defaults" entry, so make sure it is present when relied upon.
        if defaults_pos.is_none() && !has_main_then_main_x(channels.iter().map(String::as_str)) {
            actions.push(MainXCondaAction::EnsureDefaultsInChannels);
        }
        return actions;
    }

    // Any main-x entry in channels is searched before main (or there is no
    // main at all), so drop it; main-x is re-added to default_channels below.
    for channel in channels.iter().filter(|c| is_main_x_channel(c)) {
        actions.push(MainXCondaAction::RemoveChannel(channel.clone()));
    }

    // A main entry in channels listed before "defaults" is searched before
    // everything in default_channels, so main-x can simply be added there.
    let main_precedes_defaults =
        defaults_pos.is_some_and(|pos| channels[..pos].iter().any(|c| is_main_channel(c)));

    if main_precedes_defaults {
        for channel in default_channels.iter().filter(|c| is_main_x_channel(c)) {
            actions.push(MainXCondaAction::RemoveDefaultChannel(channel.clone()));
        }
        actions.push(MainXCondaAction::AddDefaultChannel(urls.main_x.clone()));
    } else {
        // Keep the user's existing main channel if there is one.
        let main = default_channels
            .iter()
            .chain(channels)
            .find(|c| is_main_channel(c))
            .cloned()
            .unwrap_or_else(|| urls.main.clone());

        for channel in default_channels
            .iter()
            .filter(|c| is_main_channel(c) || is_main_x_channel(c))
        {
            actions.push(MainXCondaAction::RemoveDefaultChannel(channel.clone()));
        }
        // Add in reverse order since `conda config --add` prepends
        actions.push(MainXCondaAction::AddDefaultChannel(urls.main_x.clone()));
        actions.push(MainXCondaAction::AddDefaultChannel(main));
    }

    if defaults_pos.is_none() {
        actions.push(MainXCondaAction::EnsureDefaultsInChannels);
    }

    actions
}

/// Plan the actions needed to disable main-x channel for conda.
///
/// Removes main-x from both `channels` and `default_channels`.
fn plan_conda_disable_actions(
    channels: &[String],
    default_channels: &[String],
) -> Vec<MainXCondaAction> {
    let from_channels = channels
        .iter()
        .filter(|c| is_main_x_channel(c))
        .map(|c| MainXCondaAction::RemoveChannel(c.clone()));
    let from_default_channels = default_channels
        .iter()
        .filter(|c| is_main_x_channel(c))
        .map(|c| MainXCondaAction::RemoveDefaultChannel(c.clone()));
    from_channels.chain(from_default_channels).collect()
}

/// Plan the actions needed to enable main-x channel for pixi.
///
/// If main and main-x are both already present, with main first (so main-x
/// packages take priority over main), nothing changes. Otherwise main-x is
/// placed right after the existing main channel, or main and main-x are
/// prepended if no main channel is configured.
fn plan_pixi_enable_actions(
    current_channels: &[String],
    urls: &ChannelUrls,
) -> Vec<MainXPixiAction> {
    if has_main_then_main_x(current_channels.iter().map(String::as_str)) {
        return vec![];
    }

    let has_main = current_channels.iter().any(|c| is_main_channel(c));
    let has_main_x = current_channels.iter().any(|c| is_main_x_channel(c));

    if !has_main && !has_main_x {
        // Prepend main-x, then main, so main ends up first
        return vec![
            MainXPixiAction::AddChannel(urls.main_x.clone()),
            MainXPixiAction::AddChannel(urls.main.clone()),
        ];
    }

    // Drop any misplaced main-x and re-insert it right after main
    let mut new_channels: Vec<String> = current_channels
        .iter()
        .filter(|c| !is_main_x_channel(c))
        .cloned()
        .collect();
    match new_channels.iter().position(|c| is_main_channel(c)) {
        Some(pos) => new_channels.insert(pos + 1, urls.main_x.clone()),
        None => {
            new_channels.insert(0, urls.main_x.clone());
            new_channels.insert(0, urls.main.clone());
        }
    }
    vec![MainXPixiAction::SetChannels(new_channels)]
}

/// Plan the actions needed to disable main-x channel for pixi.
///
/// Removes main-x from the channel list while preserving all other channels.
/// If main-x is the only channel, the result will unset default-channels entirely.
fn plan_pixi_disable_actions(current_channels: &[String]) -> Vec<MainXPixiAction> {
    let has_main_x = current_channels.iter().any(|c| is_main_x_channel(c));
    if has_main_x {
        let channels_to_keep: Vec<String> = current_channels
            .iter()
            .filter(|c| !is_main_x_channel(c))
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
    status::tos_disclaimer();

    // Step 1: Check login status and prompt if needed
    auth::ensure_logged_in(ctx).await?;

    // Step 2: Check subscription status to determine the main channel URL
    // to add if none is configured yet
    let is_premium = auth::has_premium_subscription(ctx).await?;
    let urls = ChannelUrls::new(is_premium);

    // Step 3: Determine what changes need to be made
    let conda_bin = find_conda()?;
    let channels = get_channels_conda(&conda_bin)?;
    let default_channels = get_default_channels_conda(&conda_bin)?;
    let actions = plan_conda_enable_actions(&channels, &default_channels, &urls);

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
    status::tos_disclaimer();

    // Step 1: Check login status and prompt if needed
    auth::ensure_logged_in(ctx).await?;

    // Step 2: Check subscription status to determine repo URL
    let is_premium = auth::has_premium_subscription(ctx).await?;
    let urls = ChannelUrls::new(is_premium);

    // Step 3: Determine what changes need to be made
    let pixi_bin = find_pixi()?;
    let current_channels = get_configured_channels_pixi(&pixi_bin)?;
    let actions = plan_pixi_enable_actions(&current_channels, &urls);

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
        .and_then(|conda_bin| {
            let channels = get_channels_conda(&conda_bin)?;
            let default_channels = get_default_channels_conda(&conda_bin)?;
            Ok(channels
                .iter()
                .chain(&default_channels)
                .any(|c| is_main_x_channel(c)))
        })
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
        .map(|channels| channels.iter().any(|c| is_main_x_channel(c)))
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
    let channels = get_channels_conda(&conda_bin)?;
    let default_channels = get_default_channels_conda(&conda_bin)?;
    let actions = plan_conda_disable_actions(&channels, &default_channels);

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

    fn strings(channels: &[&str]) -> Vec<String> {
        channels.iter().map(|c| c.to_string()).collect()
    }

    #[test]
    fn test_channel_urls_premium() {
        let urls = premium_urls();
        assert_eq!(urls.main_x, "https://repo.anaconda.cloud/repo/main-x");
        assert_eq!(urls.main, "https://repo.anaconda.cloud/repo/main");
        assert_eq!(urls.host, "repo.anaconda.cloud");
    }

    #[test]
    fn test_channel_urls_free() {
        let urls = free_urls();
        // main-x is always from .cloud/repo
        assert_eq!(urls.main_x, "https://repo.anaconda.cloud/repo/main-x");
        // main is from .com/pkgs
        assert_eq!(urls.main, "https://repo.anaconda.com/pkgs/main");
        // host is still .cloud for auth (main-x)
        assert_eq!(urls.host, "repo.anaconda.cloud");
    }

    #[test]
    fn test_is_main_channel() {
        assert!(is_main_channel("https://repo.anaconda.cloud/repo/main"));
        assert!(is_main_channel("https://repo.anaconda.com/pkgs/main"));
        assert!(is_main_channel("https://repo.anaconda.com/pkgs/main/"));
        assert!(!is_main_channel("https://repo.anaconda.cloud/repo/main-x"));
        assert!(!is_main_channel("https://repo.anaconda.com/pkgs/r"));
        assert!(!is_main_channel("defaults"));
        assert!(!is_main_channel("conda-forge"));
    }

    #[test]
    fn test_is_main_x_channel() {
        assert!(is_main_x_channel("https://repo.anaconda.cloud/repo/main-x"));
        assert!(is_main_x_channel(
            "https://repo.anaconda.cloud/repo/main-x/"
        ));
        assert!(!is_main_x_channel("https://repo.anaconda.cloud/repo/main"));
        assert!(!is_main_x_channel("conda-forge"));
    }

    #[test]
    fn test_has_main_then_main_x() {
        let premium = premium_urls();
        let free = free_urls();
        let check = |c: &[String]| has_main_then_main_x(c.iter().map(String::as_str));

        assert!(check(&[premium.main.clone(), premium.main_x.clone()]));
        assert!(check(&[free.main.clone(), "x".into(), free.main_x.clone()]));
        assert!(!check(&[premium.main_x.clone(), premium.main.clone()]));
        assert!(!check(std::slice::from_ref(&premium.main)));
        assert!(!check(std::slice::from_ref(&premium.main_x)));
        assert!(!check(&[]));
    }

    #[test]
    fn test_conda_effective_channels_expands_defaults() {
        let channels = strings(&["conda-forge", "defaults", "bioconda"]);
        let default_channels = strings(&["a", "b"]);
        assert_eq!(
            conda_effective_channels(&channels, &default_channels),
            vec!["conda-forge", "a", "b", "bioconda"]
        );
    }

    #[test]
    fn test_conda_effective_channels_without_defaults() {
        // "defaults" will be prepended, so default_channels come first
        let channels = strings(&["conda-forge"]);
        let default_channels = strings(&["a", "b"]);
        assert_eq!(
            conda_effective_channels(&channels, &default_channels),
            vec!["a", "b", "conda-forge"]
        );
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

    fn conda_commands(actions: &[MainXCondaAction]) -> Vec<String> {
        actions.iter().map(|a| a.command_display()).collect()
    }

    fn add_default(channel: &str) -> String {
        format!("conda config --add default_channels {}", channel)
    }

    fn remove_default(channel: &str) -> String {
        format!("conda config --remove default_channels {}", channel)
    }

    fn remove_channel(channel: &str) -> String {
        format!("conda config --remove channels {}", channel)
    }

    const ENSURE_DEFAULTS: &str = "conda config --add channels defaults";

    #[test]
    fn test_plan_conda_enable_actions_already_configured() {
        // Regression test for the issue: main then main-x in default_channels
        let urls = premium_urls();
        let channels = strings(&["defaults"]);
        let default_channels = vec![urls.main.clone(), urls.main_x.clone()];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &urls);
        assert!(conda_commands(&actions).is_empty());
    }

    #[test]
    fn test_plan_conda_enable_actions_already_configured_other_tier_main() {
        // Existing main channels are respected regardless of subscription tier
        let premium = premium_urls();
        let free = free_urls();
        let channels = strings(&["defaults"]);

        let default_channels = vec![premium.main.clone(), premium.main_x.clone()];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &free);
        assert!(conda_commands(&actions).is_empty());

        let default_channels = vec![free.main.clone(), free.main_x.clone()];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &premium);
        assert!(conda_commands(&actions).is_empty());
    }

    #[test]
    fn test_plan_conda_enable_actions_already_configured_with_extra_channels() {
        let urls = premium_urls();
        let channels = strings(&["conda-forge", "defaults"]);
        let default_channels = vec![
            urls.main.clone(),
            "https://repo.anaconda.cloud/repo/msys2".to_string(),
            urls.main_x.clone(),
        ];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &urls);
        assert!(conda_commands(&actions).is_empty());
    }

    #[test]
    fn test_plan_conda_enable_actions_neither_present() {
        let urls = premium_urls();
        let channels = strings(&["defaults"]);
        let actions = plan_conda_enable_actions(&channels, &[], &urls);

        // Add in reverse order since `conda config --add` prepends,
        // giving main, main-x
        assert_eq!(
            conda_commands(&actions),
            vec![add_default(&urls.main_x), add_default(&urls.main)]
        );
    }

    #[test]
    fn test_plan_conda_enable_actions_neither_present_free() {
        let urls = free_urls();
        let channels = strings(&["defaults"]);
        let default_channels = strings(&["https://conda.anaconda.org/conda-forge"]);
        let actions = plan_conda_enable_actions(&channels, &default_channels, &urls);

        assert_eq!(
            conda_commands(&actions),
            vec![
                add_default(&urls.main_x),
                add_default("https://repo.anaconda.com/pkgs/main"),
            ]
        );
    }

    #[test]
    fn test_plan_conda_enable_actions_implicit_defaults() {
        // conda's built-in default_channels, reported even when not configured
        let urls = premium_urls();
        let channels = strings(&["defaults"]);
        let default_channels = strings(&[
            "https://repo.anaconda.com/pkgs/main",
            "https://repo.anaconda.com/pkgs/r",
        ]);
        let actions = plan_conda_enable_actions(&channels, &default_channels, &urls);

        // The existing main is kept (and re-added so it is explicit); r is
        // neither removed nor added
        assert_eq!(
            conda_commands(&actions),
            vec![
                remove_default("https://repo.anaconda.com/pkgs/main"),
                add_default(&urls.main_x),
                add_default("https://repo.anaconda.com/pkgs/main"),
            ]
        );
    }

    #[test]
    fn test_plan_conda_enable_actions_main_only() {
        let urls = premium_urls();
        let channels = strings(&["defaults"]);
        let default_channels = vec![urls.main.clone(), "custom".to_string()];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &urls);

        // Custom channels are left alone
        assert_eq!(
            conda_commands(&actions),
            vec![
                remove_default(&urls.main),
                add_default(&urls.main_x),
                add_default(&urls.main),
            ]
        );
    }

    #[test]
    fn test_plan_conda_enable_actions_main_x_only() {
        let urls = free_urls();
        let channels = strings(&["defaults"]);
        let default_channels = vec![urls.main_x.clone()];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &urls);

        assert_eq!(
            conda_commands(&actions),
            vec![
                remove_default(&urls.main_x),
                add_default(&urls.main_x),
                add_default(&urls.main),
            ]
        );
    }

    #[test]
    fn test_plan_conda_enable_actions_wrong_order() {
        let urls = premium_urls();
        let channels = strings(&["defaults"]);
        let default_channels = vec![urls.main_x.clone(), urls.main.clone()];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &urls);

        assert_eq!(
            conda_commands(&actions),
            vec![
                remove_default(&urls.main_x),
                remove_default(&urls.main),
                add_default(&urls.main_x),
                add_default(&urls.main),
            ]
        );
    }

    #[test]
    fn test_plan_conda_enable_actions_both_in_channels() {
        let urls = premium_urls();
        let channels = vec![urls.main.clone(), urls.main_x.clone(), "defaults".into()];
        let actions = plan_conda_enable_actions(&channels, &[], &urls);
        assert!(conda_commands(&actions).is_empty());
    }

    #[test]
    fn test_plan_conda_enable_actions_main_in_channels_before_defaults() {
        let urls = premium_urls();
        let channels = vec![urls.main.clone(), "defaults".into()];
        let default_channels = strings(&["https://repo.anaconda.com/pkgs/r"]);
        let actions = plan_conda_enable_actions(&channels, &default_channels, &urls);

        // main is already searched first, so only main-x is needed
        assert_eq!(conda_commands(&actions), vec![add_default(&urls.main_x)]);
    }

    #[test]
    fn test_plan_conda_enable_actions_main_in_channels_after_defaults() {
        let urls = free_urls();
        let channels = vec!["defaults".into(), urls.main.clone()];
        let actions = plan_conda_enable_actions(&channels, &[], &urls);

        // main-x in default_channels would be searched before main, so
        // main is added ahead of it
        assert_eq!(
            conda_commands(&actions),
            vec![add_default(&urls.main_x), add_default(&urls.main)]
        );
    }

    #[test]
    fn test_plan_conda_enable_actions_main_x_in_channels_before_main() {
        let urls = premium_urls();
        let channels = vec![urls.main_x.clone(), "defaults".into()];
        let default_channels = vec![urls.main.clone()];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &urls);

        assert_eq!(
            conda_commands(&actions),
            vec![
                remove_channel(&urls.main_x),
                remove_default(&urls.main),
                add_default(&urls.main_x),
                add_default(&urls.main),
            ]
        );
    }

    #[test]
    fn test_plan_conda_enable_actions_main_x_in_channels_after_main() {
        let urls = premium_urls();
        let channels = vec!["defaults".into(), urls.main_x.clone()];
        let default_channels = vec![urls.main.clone()];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &urls);
        assert!(conda_commands(&actions).is_empty());
    }

    #[test]
    fn test_plan_conda_enable_actions_configured_but_defaults_missing() {
        let urls = premium_urls();
        let channels = strings(&["conda-forge"]);
        let default_channels = vec![urls.main.clone(), urls.main_x.clone()];
        let actions = plan_conda_enable_actions(&channels, &default_channels, &urls);

        // default_channels is only used through "defaults"
        assert_eq!(conda_commands(&actions), vec![ENSURE_DEFAULTS.to_string()]);
    }

    #[test]
    fn test_plan_conda_enable_actions_neither_present_defaults_missing() {
        let urls = premium_urls();
        let channels = strings(&["conda-forge"]);
        let actions = plan_conda_enable_actions(&channels, &[], &urls);

        assert_eq!(
            conda_commands(&actions),
            vec![
                add_default(&urls.main_x),
                add_default(&urls.main),
                ENSURE_DEFAULTS.to_string(),
            ]
        );
    }

    #[test]
    fn test_plan_conda_enable_actions_never_touches_msys2_or_r() {
        let urls = premium_urls();
        let channels = strings(&["defaults"]);
        let default_channels = strings(&[
            "https://repo.anaconda.com/pkgs/main",
            "https://repo.anaconda.com/pkgs/r",
            "https://repo.anaconda.com/pkgs/msys2",
        ]);
        let actions = plan_conda_enable_actions(&channels, &default_channels, &urls);

        for cmd in conda_commands(&actions) {
            assert!(!cmd.ends_with("/r"), "unexpected r change: {cmd}");
            assert!(!cmd.ends_with("/msys2"), "unexpected msys2 change: {cmd}");
        }
    }

    // ========================================================================
    // plan_conda_disable_actions tests
    // ========================================================================

    #[test]
    fn test_plan_conda_disable_actions_default_channels() {
        let urls = premium_urls();
        let channels = strings(&["defaults"]);
        let default_channels = vec![urls.main.clone(), urls.main_x.clone()];
        let actions = plan_conda_disable_actions(&channels, &default_channels);
        assert_eq!(conda_commands(&actions), vec![remove_default(&urls.main_x)]);
    }

    #[test]
    fn test_plan_conda_disable_actions_channels() {
        let urls = premium_urls();
        let channels = vec![urls.main.clone(), urls.main_x.clone(), "defaults".into()];
        let actions = plan_conda_disable_actions(&channels, &[]);
        assert_eq!(conda_commands(&actions), vec![remove_channel(&urls.main_x)]);
    }

    #[test]
    fn test_plan_conda_disable_actions_not_enabled() {
        let urls = premium_urls();
        let channels = strings(&["defaults"]);
        let default_channels = vec![urls.main.clone()];
        assert!(plan_conda_disable_actions(&channels, &default_channels).is_empty());
    }

    // ========================================================================
    // plan_pixi_enable_actions tests
    // ========================================================================

    fn pixi_set(channels: &[String]) -> Vec<String> {
        vec![MainXPixiAction::SetChannels(channels.to_vec()).command_display()]
    }

    fn pixi_commands(actions: &[MainXPixiAction]) -> Vec<String> {
        actions.iter().map(|a| a.command_display()).collect()
    }

    #[test]
    fn test_plan_pixi_enable_actions_empty_channels() {
        let urls = premium_urls();
        let actions = plan_pixi_enable_actions(&[], &urls);

        // Prepend main-x, then main, so main ends up first
        assert_eq!(
            pixi_commands(&actions),
            vec![
                MainXPixiAction::AddChannel(urls.main_x.clone()).command_display(),
                MainXPixiAction::AddChannel(urls.main.clone()).command_display(),
            ]
        );
    }

    #[test]
    fn test_plan_pixi_enable_actions_neither_present_with_extra_channels() {
        let urls = free_urls();
        let current = strings(&["conda-forge"]);
        let actions = plan_pixi_enable_actions(&current, &urls);

        assert_eq!(
            pixi_commands(&actions),
            vec![
                MainXPixiAction::AddChannel(urls.main_x.clone()).command_display(),
                MainXPixiAction::AddChannel(urls.main.clone()).command_display(),
            ]
        );
    }

    #[test]
    fn test_plan_pixi_enable_actions_already_configured() {
        let urls = premium_urls();
        let current = vec![urls.main.clone(), urls.main_x.clone()];
        assert!(plan_pixi_enable_actions(&current, &urls).is_empty());

        // Other-tier main is respected
        let current = vec![free_urls().main, urls.main_x.clone(), "conda-forge".into()];
        assert!(plan_pixi_enable_actions(&current, &urls).is_empty());
    }

    #[test]
    fn test_plan_pixi_enable_actions_main_already_present() {
        let urls = premium_urls();
        let current = vec!["conda-forge".into(), urls.main.clone(), "bioconda".into()];
        let actions = plan_pixi_enable_actions(&current, &urls);

        // main-x is inserted right after main
        assert_eq!(
            pixi_commands(&actions),
            pixi_set(&[
                "conda-forge".into(),
                urls.main.clone(),
                urls.main_x.clone(),
                "bioconda".into(),
            ])
        );
    }

    #[test]
    fn test_plan_pixi_enable_actions_keeps_existing_main() {
        let premium = premium_urls();
        let free = free_urls();
        let current = vec![free.main.clone()];
        let actions = plan_pixi_enable_actions(&current, &premium);

        assert_eq!(
            pixi_commands(&actions),
            pixi_set(&[free.main.clone(), premium.main_x.clone()])
        );
    }

    #[test]
    fn test_plan_pixi_enable_actions_main_x_only() {
        let urls = premium_urls();
        let current = vec![urls.main_x.clone(), "conda-forge".into()];
        let actions = plan_pixi_enable_actions(&current, &urls);

        assert_eq!(
            pixi_commands(&actions),
            pixi_set(&[urls.main.clone(), urls.main_x.clone(), "conda-forge".into()])
        );
    }

    #[test]
    fn test_plan_pixi_enable_actions_wrong_order() {
        let urls = premium_urls();
        let current = vec![urls.main_x.clone(), urls.main.clone()];
        let actions = plan_pixi_enable_actions(&current, &urls);

        assert_eq!(
            pixi_commands(&actions),
            pixi_set(&[urls.main.clone(), urls.main_x.clone()])
        );
    }

    #[test]
    fn test_plan_pixi_enable_actions_never_adds_msys2_or_r() {
        let urls = premium_urls();
        for current in [vec![], vec![urls.main.clone()], vec![urls.main_x.clone()]] {
            for cmd in pixi_commands(&plan_pixi_enable_actions(&current, &urls)) {
                assert!(!cmd.contains("/msys2"), "unexpected msys2: {cmd}");
                assert!(!cmd.contains("/r\""), "unexpected r: {cmd}");
                assert!(!cmd.ends_with("/r"), "unexpected r: {cmd}");
            }
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
