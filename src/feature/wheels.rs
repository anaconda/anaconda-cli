//! wheels feature.
//!
//! Configures pip and/or uv to use Anaconda's wheels index.

use crate::auth;
use crate::config::Config;
use crate::context::CommandContext;
use crate::input::prompt_yes_no;
use crate::tools::utils::{command_exists, find_pip, which};
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

    fn planned_changes(&self, config: &Config) -> Vec<PlannedChange> {
        match self {
            ConfigAction::ConfigurePip => {
                let pip_cmd = find_pip().unwrap_or("pip");
                vec![PlannedChange::Command(format!(
                    "{} config set global.index-url {}",
                    pip_cmd, config.pip_index_url
                ))]
            }
            ConfigAction::DeconfigurePip => {
                let pip_cmd = find_pip().unwrap_or("pip");
                vec![PlannedChange::Command(format!(
                    "{} config unset global.index-url",
                    pip_cmd
                ))]
            }
            ConfigAction::ConfigureUv => {
                let base_url = get_uv_base_url(&config.pip_index_url);
                let config_path = get_uv_config_path();
                vec![
                    PlannedChange::FileChange("Set default index in".to_string(), config_path),
                    PlannedChange::Command(format!("uv auth login {}", base_url)),
                ]
            }
            ConfigAction::DeconfigureUv => {
                let base_url = get_uv_base_url(&config.pip_index_url);
                let config_path = get_uv_config_path();
                vec![
                    PlannedChange::FileChange("Remove default index from".to_string(), config_path),
                    PlannedChange::Command(format!("uv auth logout {}", base_url)),
                ]
            }
        }
    }
}

/// Represents a planned change to display to the user.
enum PlannedChange {
    Command(String),
    FileChange(String, String),
}

impl PlannedChange {
    fn display(&self) -> String {
        match self {
            PlannedChange::Command(cmd) => {
                format!("Run: {}", status::highlight(cmd))
            }
            PlannedChange::FileChange(description, path) => {
                format!("{}: {}", description, status::highlight(path))
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

/// Get the path to the global uv.toml config file for display purposes.
fn get_uv_config_path() -> String {
    dirs::config_dir()
        .map(|p| p.join("uv").join("uv.toml").display().to_string())
        .unwrap_or_else(|| "~/.config/uv/uv.toml".to_string())
}

/// Discover available tools and return actions for all of them.
/// Returns the list of actions to perform.
fn discover_tools(enable: bool) -> miette::Result<Vec<ConfigAction>> {
    let pip_cmd = find_pip();
    let uv_available = command_exists("uv");

    if pip_cmd.is_none() && !uv_available {
        return Err(miette::miette!(
            "Neither pip nor uv found in PATH. Please install at least one first."
        ));
    }

    status::info("Detected package managers:");
    if let Some(cmd) = pip_cmd {
        let path = which(cmd).unwrap_or_else(|| cmd.to_string());
        eprintln!("  {} pip ({})", status::checkmark(), status::dim(&path));
    }
    if uv_available {
        let path = which("uv").unwrap_or_else(|| "uv".to_string());
        eprintln!("  {} uv ({})", status::checkmark(), status::dim(&path));
    }
    status::blank_line();

    let mut actions = Vec::new();

    if pip_cmd.is_some() {
        if enable {
            actions.push(ConfigAction::ConfigurePip);
        } else {
            actions.push(ConfigAction::DeconfigurePip);
        }
    }

    if uv_available {
        if enable {
            actions.push(ConfigAction::ConfigureUv);
        } else {
            actions.push(ConfigAction::DeconfigureUv);
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
        // No flags - auto-detect all available tools
        discover_tools(true)?
    };

    if actions.is_empty() {
        status::info("No tools selected for configuration.");
        return Ok(());
    }

    // Show tip about flags if multiple tools detected and no explicit flags
    if !pip && !uv && actions.len() > 1 {
        status::tip("Use --pip or --uv to configure only one tool.");
        status::blank_line();
    }

    // Step 2: Check login status and prompt if needed
    ensure_logged_in(ctx).await?;

    // Step 3: Show planned changes
    status::blank_line();
    status::info("The following changes will be made:");
    for action in &actions {
        for change in action.planned_changes(&ctx.config) {
            eprintln!("  {}", change.display());
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
        status::running(&format!("Configuring {}...", action.tool_name()));

        let result = match action {
            ConfigAction::ConfigurePip => crate::tools::pip::configure(&ctx.config),
            ConfigAction::ConfigureUv => crate::tools::uv::configure(&ctx.config),
            _ => unreachable!(),
        };

        if let Err(e) = result {
            eprintln!();
            return Err(miette::miette!(
                "Failed to configure {}: {}",
                action.tool_name(),
                e
            ));
        }

        status::finish_running(&format!("Configured {}", action.tool_name()));
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
        // No flags - auto-detect all available tools
        discover_tools(false)?
    };

    if actions.is_empty() {
        status::info("No tools selected for deconfiguration.");
        return Ok(());
    }

    // Show tip about flags if multiple tools detected and no explicit flags
    if !pip && !uv && actions.len() > 1 {
        status::tip("Use --pip or --uv to deconfigure only one tool.");
        status::blank_line();
    }

    // Step 2: Show planned changes
    status::info("The following changes will be made:");
    for action in &actions {
        for change in action.planned_changes(&ctx.config) {
            eprintln!("  {}", change.display());
        }
    }
    status::blank_line();

    // Step 3: Prompt for confirmation unless --force
    if !force && !prompt_yes_no("Proceed?", true) {
        eprintln!("Aborted.");
        return Ok(());
    }

    // Step 4: Execute the changes
    status::blank_line();
    for action in &actions {
        status::running(&format!("Deconfiguring {}...", action.tool_name()));

        let result = match action {
            ConfigAction::DeconfigurePip => crate::tools::pip::deconfigure(),
            ConfigAction::DeconfigureUv => crate::tools::uv::deconfigure(&ctx.config),
            _ => unreachable!(),
        };

        if let Err(e) = result {
            eprintln!();
            return Err(miette::miette!(
                "Failed to deconfigure {}: {}",
                action.tool_name(),
                e
            ));
        }

        status::finish_running(&format!("Deconfigured {}", action.tool_name()));
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
