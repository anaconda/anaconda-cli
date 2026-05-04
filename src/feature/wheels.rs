//! wheels feature.
//!
//! Configures pip and/or uv to use Anaconda's wheels index.

use crate::auth;
use crate::config::Config;
use crate::context::CommandContext;
use crate::input::prompt_yes_no;
use crate::tools::utils::{command_exists, find_pip};
use crate::ui::status;

/// Represents a tool configuration action to be executed.
enum ConfigAction {
    ConfigurePip,
    ConfigureUv,
    DeconfigurePip,
    DeconfigureUv,
}

impl ConfigAction {
    fn tool_name(&self) -> &'static str {
        match self {
            ConfigAction::ConfigurePip | ConfigAction::DeconfigurePip => "pip",
            ConfigAction::ConfigureUv | ConfigAction::DeconfigureUv => "uv",
        }
    }

    fn command_description(&self, config: &Config) -> String {
        match self {
            ConfigAction::ConfigurePip => {
                let pip_cmd = find_pip().unwrap_or("pip");
                format!("{} config set global.index-url {}", pip_cmd, config.pip_index_url)
            }
            ConfigAction::DeconfigurePip => {
                let pip_cmd = find_pip().unwrap_or("pip");
                format!("{} config unset global.index-url", pip_cmd)
            }
            ConfigAction::ConfigureUv => {
                let base_url = get_uv_base_url(&config.pip_index_url);
                format!("uv auth login {}", base_url)
            }
            ConfigAction::DeconfigureUv => {
                let base_url = get_uv_base_url(&config.pip_index_url);
                format!("uv auth logout {}", base_url)
            }
        }
    }
}

/// Get the base URL for uv auth by removing /simple/ suffix if present.
fn get_uv_base_url(pip_index_url: &str) -> &str {
    pip_index_url
        .trim_end_matches('/')
        .trim_end_matches("/simple")
        .trim_end_matches('/')
}

/// Discover available tools and prompt the user for each one.
/// Returns the list of actions to perform.
fn discover_and_prompt_tools(enable: bool, force: bool) -> miette::Result<Vec<ConfigAction>> {
    let pip_cmd = find_pip();
    let uv_available = command_exists("uv");

    if pip_cmd.is_none() && !uv_available {
        return Err(miette::miette!(
            "Neither pip nor uv found in PATH. Please install at least one first."
        ));
    }

    status::info("Detected package managers:");
    if let Some(cmd) = pip_cmd {
        eprintln!("  {} {} (pip)", status::highlight("✓"), cmd);
    }
    if uv_available {
        eprintln!("  {} uv", status::highlight("✓"));
    }
    status::blank_line();

    let mut actions = Vec::new();

    if pip_cmd.is_some() {
        let prompt = if enable {
            "Configure pip to use Anaconda's wheels index?"
        } else {
            "Remove pip configuration for Anaconda's wheels index?"
        };
        if force || prompt_yes_no(prompt) {
            if enable {
                actions.push(ConfigAction::ConfigurePip);
            } else {
                actions.push(ConfigAction::DeconfigurePip);
            }
        }
    }

    if uv_available {
        let prompt = if enable {
            "Configure uv to use Anaconda's wheels index?"
        } else {
            "Remove uv configuration for Anaconda's wheels index?"
        };
        if force || prompt_yes_no(prompt) {
            if enable {
                actions.push(ConfigAction::ConfigureUv);
            } else {
                actions.push(ConfigAction::DeconfigureUv);
            }
        }
    }

    Ok(actions)
}

/// Enable wheels feature for pip and/or uv.
pub async fn enable_wheels(
    ctx: &mut CommandContext,
    force: bool,
    pip: bool,
    uv: bool,
) -> miette::Result<()> {
    status::info(&format!(
        "Enabling {} feature...",
        status::highlight("wheels")
    ));
    status::blank_line();

    // Step 1: Determine which tools to configure
    let actions = if pip || uv {
        // Explicit flags provided - use those (error if tool not found)
        let mut actions = Vec::new();
        if pip {
            if find_pip().is_none() {
                return Err(miette::miette!(
                    "'pip' is not installed or not found in PATH. Please install pip first."
                ));
            }
            actions.push(ConfigAction::ConfigurePip);
        }
        if uv {
            if !command_exists("uv") {
                return Err(miette::miette!(
                    "'uv' is not installed or not found in PATH. Please install uv first."
                ));
            }
            actions.push(ConfigAction::ConfigureUv);
        }
        actions
    } else {
        // No flags - auto-detect and prompt for each available tool
        discover_and_prompt_tools(true, force)?
    };

    if actions.is_empty() {
        status::info("No tools selected for configuration.");
        return Ok(());
    }

    // Step 2: Check login status and prompt if needed
    ensure_logged_in(ctx).await?;

    // Step 3: Show planned changes
    status::blank_line();
    status::info("The following commands will be run:");
    for action in &actions {
        let cmd = action.command_description(&ctx.config);
        eprintln!("  {}", status::highlight(&cmd));
    }
    status::blank_line();

    // Step 4: Prompt for confirmation unless --force
    if !force && !prompt_yes_no("Proceed?") {
        eprintln!("Aborted.");
        return Ok(());
    }

    // Step 5: Execute the changes
    status::blank_line();
    for action in &actions {
        let cmd = action.command_description(&ctx.config);
        status::running(&format!("Running {}", status::highlight(&cmd)));

        match action {
            ConfigAction::ConfigurePip => {
                crate::tools::pip::configure(&ctx.config)
                    .map_err(|e| miette::miette!("Failed to configure pip: {}", e))?;
            }
            ConfigAction::ConfigureUv => {
                crate::tools::uv::configure(&ctx.config)
                    .map_err(|e| miette::miette!("Failed to configure uv: {}", e))?;
            }
            _ => unreachable!(),
        }

        status::finish_running(&format!("Ran {}", status::highlight(&cmd)));
    }

    // Step 6: Show success message and undo instructions
    status::blank_line();
    let tools: Vec<_> = actions.iter().map(|a| a.tool_name()).collect();
    status::celebrate(&format!(
        "You can now install packages from Anaconda's wheels index using {}!",
        tools.join(" and ")
    ));
    status::blank_line();
    status::info("To disable this feature, run:");
    let flags: Vec<_> = actions
        .iter()
        .map(|a| format!("--{}", a.tool_name()))
        .collect();
    eprintln!(
        "  {}",
        status::highlight(&format!("ana feature disable wheels {}", flags.join(" ")))
    );

    Ok(())
}

/// Disable wheels feature for pip and/or uv.
pub async fn disable_wheels(
    ctx: &mut CommandContext,
    force: bool,
    pip: bool,
    uv: bool,
) -> miette::Result<()> {
    status::info(&format!(
        "Disabling {} feature...",
        status::highlight("wheels")
    ));
    status::blank_line();

    // Step 1: Determine which tools to deconfigure
    let actions = if pip || uv {
        // Explicit flags provided - use those (error if tool not found)
        let mut actions = Vec::new();
        if pip {
            if find_pip().is_none() {
                return Err(miette::miette!(
                    "'pip' is not installed or not found in PATH. Please install pip first."
                ));
            }
            actions.push(ConfigAction::DeconfigurePip);
        }
        if uv {
            if !command_exists("uv") {
                return Err(miette::miette!(
                    "'uv' is not installed or not found in PATH. Please install uv first."
                ));
            }
            actions.push(ConfigAction::DeconfigureUv);
        }
        actions
    } else {
        // No flags - auto-detect and prompt for each available tool
        discover_and_prompt_tools(false, force)?
    };

    if actions.is_empty() {
        status::info("No tools selected for deconfiguration.");
        return Ok(());
    }

    // Step 2: Show planned changes
    status::info("The following commands will be run:");
    for action in &actions {
        let cmd = action.command_description(&ctx.config);
        eprintln!("  {}", status::highlight(&cmd));
    }
    status::blank_line();

    // Step 3: Prompt for confirmation unless --force
    if !force && !prompt_yes_no("Proceed?") {
        eprintln!("Aborted.");
        return Ok(());
    }

    // Step 4: Execute the changes
    status::blank_line();
    for action in &actions {
        let cmd = action.command_description(&ctx.config);
        status::running(&format!("Running {}", status::highlight(&cmd)));

        match action {
            ConfigAction::DeconfigurePip => {
                crate::tools::pip::deconfigure()
                    .map_err(|e| miette::miette!("Failed to deconfigure pip: {}", e))?;
            }
            ConfigAction::DeconfigureUv => {
                crate::tools::uv::deconfigure(&ctx.config)
                    .map_err(|e| miette::miette!("Failed to deconfigure uv: {}", e))?;
            }
            _ => unreachable!(),
        }

        status::finish_running(&format!("Ran {}", status::highlight(&cmd)));
    }

    // Step 5: Show success and re-enable instructions
    status::blank_line();
    status::info("To re-enable, run:");
    let flags: Vec<_> = actions
        .iter()
        .map(|a| format!("--{}", a.tool_name()))
        .collect();
    eprintln!(
        "  {}",
        status::highlight(&format!("ana feature enable wheels {}", flags.join(" ")))
    );

    Ok(())
}

/// Ensure the user is logged in, prompting them to login if not.
async fn ensure_logged_in(ctx: &mut CommandContext) -> miette::Result<()> {
    status::waiting("Checking authentication status...");

    let config = Config::load();
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
