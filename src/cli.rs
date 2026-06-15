use std::collections::HashMap;
use std::time::Instant;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use miette::{IntoDiagnostic, miette};

use crate::VERSION;
use crate::anaconda_cli;
use crate::auth;
use crate::config::Config;
use crate::context::CommandContext;
use crate::feature;
use crate::feedback;
use crate::fetch::api_fetch;
use crate::help;
use crate::mcp::{self, McpAction, McpCommands};
#[cfg(unix)]
use crate::outerbounds::{self, ObAction, ObCommands};
use crate::repo::{self, RepoAction, RepoCommands};
use crate::tools;
use crate::update;

/// Log level for tracing output.
#[derive(Debug, Clone, Copy, Default)]
pub enum LogLevel {
    #[default]
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<u8> for LogLevel {
    fn from(count: u8) -> Self {
        match count {
            0 => Self::Off,
            1 => Self::Error,
            2 => Self::Warn,
            3 => Self::Info,
            4 => Self::Debug,
            _ => Self::Trace,
        }
    }
}

impl LogLevel {
    fn as_filter_str(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Error => "ana=error,anaconda_otel_rs=off,opentelemetry=off,reqwest=off",
            Self::Warn => "ana=warn,anaconda_otel_rs=off,opentelemetry=off,reqwest=off",
            Self::Info => "ana=info,anaconda_otel_rs=off,opentelemetry=off,reqwest=off",
            Self::Debug => "ana=debug,anaconda_otel_rs=off,opentelemetry=off,reqwest=off",
            Self::Trace => "ana=trace,anaconda_otel_rs=off,opentelemetry=off,reqwest=off",
        }
    }
}

pub async fn execute() {
    let (action, level) = parse();

    let filter = build_tracing_filter(level);
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let skip_telemetry_spawn = matches!(
        &action,
        Action::TelemetrySubmit | Action::TelemetryKill | Action::TelemetryStatus
    );

    let result = action.execute().await;

    if !skip_telemetry_spawn {
        if let Err(e) = crate::telemetry::spawn_telemetry_submitter() {
            tracing::debug!("Failed to spawn telemetry submitter: {}", e);
        }
    }

    if let Err(e) = result {
        tracing::error!("Command failed: {}", e);
        eprintln!("Error: {:?}", e);
        std::process::exit(1);
    }
}

/// Build tracing filter based on log level.
/// Respects RUST_LOG env var if set, otherwise uses verbosity flags.
fn build_tracing_filter(level: LogLevel) -> tracing_subscriber::EnvFilter {
    if let Ok(filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        return filter;
    }

    tracing_subscriber::EnvFilter::new(level.as_filter_str())
}

/// Action to be performed, returned by parse()
pub enum Action {
    ShowHelp,
    ShowSubcommandHelp(String),
    ShowVersion,
    ShowConfig,
    Login {
        api_key: Option<String>,
        prompt_api_key: bool,
        force: bool,
    },
    Logout,
    ShowApiKey,
    Whoami {
        json: bool,
    },
    Update {
        version: Option<String>,
        force: bool,
    },
    CheckForUpdate,
    ShowAvailableVersions,
    Bootstrap,
    OrgProxy {
        args: Vec<String>,
    },
    #[cfg(unix)]
    ObProxy {
        args: Vec<String>,
    },
    #[cfg(unix)]
    ObAutoConfigure {
        instance: String,
    },
    McpRun {
        args: Vec<String>,
    },
    RepoRun {
        args: Vec<String>,
    },
    UserAgent {
        prefix: Option<String>,
    },
    OpenFeedback,
    ToolInstall {
        name: String,
    },
    ToolUninstall {
        name: String,
        force: bool,
    },
    ToolList,
    ApiFetch {
        method: String,
        url: String,
        query_args: Option<String>,
        data: Option<String>,
        json: Option<String>,
    },
    FeatureEnable {
        feature: String,
        force: bool,
        pip: bool,
        uv: bool,
        conda: bool,
        pixi: bool,
    },
    FeatureDisable {
        feature: String,
        force: bool,
        pip: bool,
        uv: bool,
        conda: bool,
        pixi: bool,
    },
    FeatureList,
    TelemetrySubmit,
    TelemetryKill,
    TelemetryStatus,
    Upload {
        args: Vec<String>,
    },
    Channels {
        args: Vec<String>,
    },
}

impl Action {
    fn match_action_name(&self) -> &'static str {
        match self {
            Action::ShowHelp => "help",
            Action::ShowSubcommandHelp(_) => "subcommand.help",
            Action::ShowVersion => "version",
            Action::ShowConfig => "config",
            Action::Login { .. } => "login",
            Action::Logout => "logout",
            Action::ShowApiKey => "auth.api-key",
            Action::Whoami { .. } => "whoami",
            Action::Update { .. } => "self.update",
            Action::CheckForUpdate => "self.update.check",
            Action::ShowAvailableVersions => "self.update.list",
            Action::Bootstrap => "bootstrap",
            Action::OrgProxy { .. } => "org",
            #[cfg(unix)]
            Action::ObProxy { .. } => "ob",
            #[cfg(unix)]
            Action::ObAutoConfigure { .. } => "ob.configure.auto",
            Action::McpRun { .. } => "mcp",
            Action::RepoRun { .. } => "repo",
            Action::UserAgent { .. } => "user-agent",
            Action::OpenFeedback => "feedback",
            Action::ToolInstall { .. } => "tool.install",
            Action::ToolUninstall { .. } => "tool.uninstall",
            Action::ToolList => "tool.list",
            Action::ApiFetch { .. } => "api.fetch",
            Action::FeatureEnable { feature, .. } => match feature.as_str() {
                "main-x" => "feature.enable.main-x",
                #[cfg(feature = "unstable")]
                "wheels" => "feature.enable.wheels",
                _ => "feature.enable.unknown",
            },
            Action::FeatureDisable { feature, .. } => match feature.as_str() {
                "main-x" => "feature.disable.main-x",
                #[cfg(feature = "unstable")]
                "wheels" => "feature.disable.wheels",
                _ => "feature.disable.unknown",
            },
            Action::FeatureList => "feature.list",
            Action::TelemetrySubmit => "telemetry-submit",
            Action::TelemetryKill => "telemetry-kill",
            Action::TelemetryStatus => "telemetry-status",
            Action::Upload { .. } => "upload",
            Action::Channels { .. } => "channels",
        }
    }

    /// Execute the action with telemetry middleware
    pub async fn execute(self) -> miette::Result<()> {
        let name = self.match_action_name();
        let is_telemetry_submit = matches!(&self, Action::TelemetrySubmit);

        let mut ctx = CommandContext::new();
        ctx.telemetry.add("command", name);
        ctx.telemetry.record_counter("cli_command_invoked", 1);

        let start = Instant::now();
        let result = self.run(&mut ctx).await;
        let duration_ms = start.elapsed().as_millis() as f64;

        match &result {
            Ok(_) => {
                ctx.telemetry.record_counter("cli_command_success", 1);
                ctx.telemetry
                    .record_histogram("cli_command_success_duration_ms", duration_ms);
            }
            Err(_) => {
                ctx.telemetry.record_counter("cli_command_failure", 1);
                ctx.telemetry
                    .record_histogram("cli_command_failure_duration_ms", duration_ms);
            }
        }

        if !is_telemetry_submit {
            if let Err(e) = ctx.telemetry.flush_to_spool() {
                tracing::debug!("Failed to spool telemetry: {}", e);
            }
        }

        result
    }

    async fn run(self, ctx: &mut CommandContext) -> miette::Result<()> {
        match self {
            Action::ShowHelp => {
                let subcommands = get_subcommand_descriptions();
                help::print_help(subcommands);
                Ok(())
            }
            Action::ShowSubcommandHelp(path) => {
                help::print_subcommand_help(&get_subcommand(&path), &path);
                Ok(())
            }
            Action::ShowVersion => {
                println!("{}", VERSION);
                Ok(())
            }
            Action::ShowConfig => {
                Config::load().print_table();
                Ok(())
            }
            Action::Bootstrap => Ok(anaconda_cli::run_bootstrap(ctx)
                .await
                .map_err(|e| miette!("{}", e))?),
            Action::OrgProxy { args } => Ok(
                anaconda_cli::run_subcommand(ctx, "org", &args).map_err(|e| miette!("{}", e))?
            ),
            Action::McpRun { args } => mcp::run(ctx, &args).await,
            Action::RepoRun { args } => repo::run(ctx, &args).await,
            #[cfg(unix)]
            Action::ObProxy { args } => outerbounds::run(ctx, &args).await,
            #[cfg(unix)]
            Action::ObAutoConfigure { instance } => {
                outerbounds::auto_configure(ctx, &instance).await
            }
            Action::ToolInstall { name } => {
                tools::install::install_tool(ctx, &name).await?;
                Ok(())
            }
            Action::ToolUninstall { name, force } => {
                tools::uninstall::uninstall_tool(ctx, &name, force)?;
                Ok(())
            }
            Action::ToolList => {
                tools::list::print_tool_list(ctx);
                Ok(())
            }
            Action::Login {
                api_key,
                prompt_api_key,
                force,
            } => Ok(auth::login(ctx, api_key, prompt_api_key, force)
                .await
                .into_diagnostic()?),
            Action::Logout => Ok(auth::logout(ctx).into_diagnostic()?),
            Action::ShowApiKey => Ok(auth::show_api_key(ctx).into_diagnostic()?),
            Action::Whoami { json } => Ok(auth::whoami(ctx, json).await.into_diagnostic()?),
            Action::Update { version, force } => {
                update::run_update(ctx, VERSION, version, force).await;
                Ok(())
            }
            Action::CheckForUpdate => {
                update::check_for_update(ctx, VERSION).await;
                Ok(())
            }
            Action::ShowAvailableVersions => {
                update::show_available_versions(ctx, VERSION).await;
                Ok(())
            }
            Action::UserAgent { prefix } => {
                if let Some(p) = prefix {
                    crate::ua::set_env_prefix(p);
                }
                println!("{}", crate::ua::user_agent());
                Ok(())
            }
            Action::OpenFeedback => {
                feedback::open_feedback();
                Ok(())
            }
            Action::ApiFetch {
                method,
                url,
                query_args,
                data,
                json,
            } => {
                api_fetch(
                    ctx,
                    &method,
                    &url,
                    query_args.as_deref(),
                    data.as_deref(),
                    json.as_deref(),
                )
                .await
            }
            #[allow(unused_variables)]
            Action::FeatureEnable {
                feature,
                force,
                pip,
                uv,
                conda,
                pixi,
            } => {
                match feature.as_str() {
                    "main-x" => {
                        if pixi {
                            feature::enable_main_x_pixi(ctx, force).await?
                        } else {
                            // Default to conda (--conda flag or no flag)
                            feature::enable_main_x_conda(ctx, force).await?
                        }
                    }
                    #[cfg(feature = "unstable")]
                    "wheels" => {
                        // wheels is an experimental feature that requires the feature flag
                        // to be enabled first before configuring pip/uv
                        if pip || uv {
                            // User wants to configure tools - check if experimental flag is enabled
                            if !feature::is_feature_enabled("wheels") {
                                use crate::ui::status::{blank_line, highlight, tip, warn};
                                warn(&format!(
                                    "The {} feature is experimental and hidden from public use.",
                                    highlight("wheels")
                                ));
                                tip(&format!(
                                    "Enable the experimental flag first with {}",
                                    highlight("ana feature enable wheels")
                                ));
                                blank_line();
                                return Err(miette!(
                                    "Experimental feature 'wheels' is not enabled"
                                ));
                            }
                            feature::enable_wheels(ctx, force, pip, uv).await?
                        } else {
                            // No --pip/--uv flags - treat as enabling the experimental feature
                            crate::ui::status::warn(&format!(
                                "The '{}' feature is experimental and may change or be removed.",
                                "wheels"
                            ));
                            if !force
                                && !crate::input::prompt_yes_no(
                                    "Enable this experimental feature?",
                                    false,
                                )
                            {
                                return Ok(());
                            }
                            feature::enable_feature("wheels")?;
                            crate::ui::status::success("Experimental feature 'wheels' enabled.");
                            crate::ui::status::blank_line();
                            crate::ui::status::tip(&format!(
                                "Now configure your tools with {} or {}",
                                crate::ui::status::highlight("ana feature enable wheels --pip"),
                                crate::ui::status::highlight("--uv")
                            ));
                        }
                    }
                    name if feature::is_valid_feature(name) => {
                        crate::ui::status::warn(&format!(
                            "The '{}' feature is experimental and may change or be removed.",
                            name
                        ));
                        if !force
                            && !crate::input::prompt_yes_no(
                                "Enable this experimental feature?",
                                false,
                            )
                        {
                            return Ok(());
                        }
                        feature::enable_feature(name)?;
                        crate::ui::status::success(&format!(
                            "Experimental feature '{}' enabled.",
                            name
                        ));
                    }
                    _ => return Err(miette!("Unknown feature: {}", feature)),
                }
                // Silence unused variable warning for conda - it's the default when pixi is false
                let _ = conda;
                Ok(())
            }
            #[allow(unused_variables)]
            Action::FeatureDisable {
                feature,
                force,
                pip,
                uv,
                conda,
                pixi,
            } => {
                match feature.as_str() {
                    "main-x" => {
                        if pixi {
                            feature::disable_main_x_pixi(ctx, force).await?
                        } else {
                            // Default to conda (--conda flag or no flag)
                            feature::disable_main_x_conda(ctx, force).await?
                        }
                    }
                    #[cfg(feature = "unstable")]
                    "wheels" => {
                        // wheels is an experimental feature
                        if pip || uv {
                            // User wants to deconfigure tools
                            feature::disable_wheels(ctx, force, pip, uv).await?
                        } else {
                            // No --pip/--uv flags - disable the experimental feature flag
                            feature::disable_feature("wheels")?;
                            crate::ui::status::success("Experimental feature 'wheels' disabled.");
                        }
                    }
                    name if feature::is_valid_feature(name) => {
                        feature::disable_feature(name)?;
                        crate::ui::status::success(&format!(
                            "Experimental feature '{}' disabled.",
                            name
                        ));
                    }
                    _ => return Err(miette!("Unknown feature: {}", feature)),
                }
                // Silence unused variable warning for conda - it's the default when pixi is false
                let _ = conda;
                Ok(())
            }
            Action::FeatureList => {
                feature::list::print_feature_list(ctx);
                Ok(())
            }
            Action::TelemetrySubmit => {
                crate::telemetry::submit_pending().map_err(|e| miette!("{}", e))?;
                Ok(())
            }
            Action::TelemetryKill => {
                match crate::telemetry::kill_submitters() {
                    Ok(0) => println!("No telemetry processes found"),
                    Ok(n) => println!("Killed {} telemetry process(es)", n),
                    Err(e) => return Err(miette!("Failed to kill processes: {}", e)),
                }
                Ok(())
            }
            Action::TelemetryStatus => {
                match crate::telemetry::list_submitters() {
                    Ok(pids) if pids.is_empty() => println!("No telemetry processes running"),
                    Ok(pids) => {
                        println!("{} telemetry process(es) running:", pids.len());
                        for pid in pids {
                            println!("  PID {}", pid);
                        }
                    }
                    Err(e) => return Err(miette!("Failed to list processes: {}", e)),
                }
                Ok(())
            }
            Action::Upload { args } => route_package_command(ctx, "upload", &args).await,
            Action::Channels { args } => route_package_command(ctx, "channels", &args).await,
        }
    }
}

async fn route_package_command(
    ctx: &mut CommandContext,
    command: &str,
    args: &[String],
) -> miette::Result<()> {
    let channel_arg = extract_channel_arg(command, args);

    match channel_arg {
        Some(channel) if channel.matches('/').count() == 1 => {
            let filtered_args = filter_args_for_repo(command, args);
            repo::run(ctx, &[vec![command.to_string()], filtered_args].concat()).await
        }
        Some(channel) if channel.contains('/') => {
            Err(miette!("Invalid channel format '{}': only one '/' separator allowed", channel))
        }
        _ => {
            if command == "channels" && is_create_command(args) {
                return Err(miette!(
                    "Must specify an organization and channel to create a channel:\n  ana channels create <org_name>/<channel_name>"
                ));
            }
            let transformed_args = transform_args_for_org(command, args);
            Ok(anaconda_cli::run_subcommand(ctx, command, &transformed_args).map_err(|e| miette!("{}", e))?)
        }
    }
}

fn filter_args_for_repo(command: &str, args: &[String]) -> Vec<String> {
    let mut filtered = Vec::new();
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];

        match command {
            "upload" => {
                if arg == "--summary" || arg == "-s" {
                    i += 1;
                    if i < args.len() {
                        i += 1;
                    }
                } else {
                    filtered.push(arg.clone());
                    i += 1;
                }
            }
            _ => {
                filtered.push(arg.clone());
                i += 1;
            }
        }
    }

    filtered
}

fn transform_args_for_org(command: &str, args: &[String]) -> Vec<String> {
    let mut transformed = Vec::new();
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];

        match command {
            "upload" => {
                if arg == "-c" {
                    transformed.push("-u".to_string());
                    i += 1;
                } else if arg == "--channel" {
                    transformed.push("--user".to_string());
                    i += 1;
                } else {
                    transformed.push(arg.clone());
                    i += 1;
                }
            }
            "remove" => {
                transformed.push(arg.clone());
                i += 1;
            }
            _ => {
                transformed.push(arg.clone());
                i += 1;
            }
        }
    }

    transformed
}

fn is_create_command(args: &[String]) -> bool {
    !args.is_empty() && args[0] == "create"
}

fn extract_channel_arg(command: &str, args: &[String]) -> Option<String> {
    match command {
        "channels" => {
            if args.is_empty() {
                return None;
            }
            match args[0].as_str() {
                "create" | "remove" => {
                    args.iter()
                        .find(|arg| arg.contains('/') && !arg.starts_with('-'))
                        .cloned()
                }
                _ => None,
            }
        }
        "upload" => {
            for (i, arg) in args.iter().enumerate() {
                if (arg == "--channel" || arg == "-c") && i + 1 < args.len() {
                    return Some(args[i + 1].clone());
                }
            }
            None
        }
        _ => None,
    }
}

/// Parse CLI arguments and return the action to perform along with log level.
/// Exits the process on unrecoverable errors (unknown commands, etc.)
pub fn parse() -> (Action, LogLevel) {
    // Two-step parsing: first get ArgMatches, then convert to typed struct.
    // This gives us access to both the raw matches (for subcommand path extraction)
    // and the typed Cli struct.
    let matches = match Cli::command().try_get_matches() {
        Ok(m) => m,
        Err(e) => return handle_parse_error(e),
    };

    let cli = match Cli::from_arg_matches(&matches) {
        Ok(c) => c,
        Err(e) => return handle_parse_error(e.into()),
    };

    let level: LogLevel = cli.verbose.into();

    // Handle --help flag (global, so it works at any level)
    if cli.help {
        let action = match get_subcommand_path_from_matches(&matches) {
            None => Action::ShowHelp,
            Some(path) => Action::ShowSubcommandHelp(path),
        };
        return (action, level);
    }

    let action = match cli.command {
        None => Action::ShowHelp,
        Some(Commands::Bootstrap) => Action::Bootstrap,
        Some(Commands::Config) => Action::ShowConfig,
        Some(Commands::Login {
            api_key,
            prompt_api_key,
            force,
        }) => Action::Login {
            api_key,
            prompt_api_key,
            force,
        },
        Some(Commands::Logout) => Action::Logout,
        Some(Commands::Whoami { json }) => Action::Whoami { json },
        Some(Commands::Auth { command }) => match command {
            None => Action::ShowSubcommandHelp("auth".to_string()),
            Some(AuthCommands::ApiKey) => Action::ShowApiKey,
            Some(AuthCommands::Login {
                api_key,
                prompt_api_key,
                force,
            }) => Action::Login {
                api_key,
                prompt_api_key,
                force,
            },
            Some(AuthCommands::Logout) => Action::Logout,
            Some(AuthCommands::Whoami { json }) => Action::Whoami { json },
        },
        Some(Commands::Self_ { command }) => match command {
            None => Action::ShowSubcommandHelp("self".to_string()),
            Some(SelfCommands::Feedback) => Action::OpenFeedback,
            Some(SelfCommands::Update {
                version,
                check,
                list,
                force,
            }) => {
                if check {
                    Action::CheckForUpdate
                } else if list {
                    Action::ShowAvailableVersions
                } else {
                    Action::Update { version, force }
                }
            }
            Some(SelfCommands::UserAgent { prefix }) => Action::UserAgent { prefix },
        },
        Some(Commands::Org { args }) => Action::OrgProxy { args },
        Some(Commands::Mcp { command }) => match command {
            None => Action::ShowSubcommandHelp("mcp".to_string()),
            Some(cmd) => match cmd.into_action() {
                McpAction::ShowHelp(path) => Action::ShowSubcommandHelp(path),
                McpAction::Run(args) => Action::McpRun { args },
            },
        },
        Some(Commands::Repo { command }) => match command {
            None => Action::ShowSubcommandHelp("repo".to_string()),
            Some(cmd) => match cmd.into_action() {
                RepoAction::ShowHelp(path) => Action::ShowSubcommandHelp(path),
                RepoAction::Run(args) => Action::RepoRun { args },
            },
        },
        #[cfg(unix)]
        Some(Commands::Ob { command }) => {
            if !feature::is_feature_enabled("outerbounds") {
                use crate::ui::status::{blank_line, highlight, tip, warn};
                warn(&format!(
                    "The {} command requires the experimental {} feature.",
                    highlight("ob"),
                    highlight("outerbounds")
                ));
                tip(&format!(
                    "Enable it with {}",
                    highlight("ana feature enable outerbounds")
                ));
                blank_line();
                std::process::exit(1);
            }
            match command {
                None => Action::ShowSubcommandHelp("ob".to_string()),
                Some(cmd) => match cmd.into_action() {
                    ObAction::ShowHelp(path) => Action::ShowSubcommandHelp(path),
                    ObAction::Proxy(args) => Action::ObProxy { args },
                    ObAction::AutoConfigure { instance } => Action::ObAutoConfigure { instance },
                },
            }
        }
        Some(Commands::Tool { command }) => match command {
            None => Action::ShowSubcommandHelp("tool".to_string()),
            Some(ToolCommands::Install { name }) => Action::ToolInstall { name },
            Some(ToolCommands::List) => Action::ToolList,
            Some(ToolCommands::Uninstall { name, force }) => Action::ToolUninstall { name, force },
        },
        Some(Commands::Api { command }) => match command {
            None => Action::ShowSubcommandHelp("api".to_string()),
            Some(ApiCommands::Fetch {
                method,
                url,
                query_args,
                data,
                json,
            }) => Action::ApiFetch {
                method,
                url,
                query_args,
                data,
                json,
            },
        },
        Some(Commands::Feature { command }) => match command {
            None => Action::ShowSubcommandHelp("feature".to_string()),
            Some(FeatureCommands::Enable {
                name,
                force,
                pip,
                uv,
                conda,
                pixi,
            }) => Action::FeatureEnable {
                feature: name,
                force,
                pip,
                uv,
                conda,
                pixi,
            },
            Some(FeatureCommands::Disable {
                name,
                force,
                pip,
                uv,
                conda,
                pixi,
            }) => Action::FeatureDisable {
                feature: name,
                force,
                pip,
                uv,
                conda,
                pixi,
            },
            Some(FeatureCommands::List) => Action::FeatureList,
        },
        Some(Commands::TelemetrySubmit) => Action::TelemetrySubmit,
        Some(Commands::TelemetryKill) => Action::TelemetryKill,
        Some(Commands::TelemetryStatus) => Action::TelemetryStatus,
        Some(Commands::Upload { channel, no_progress, summary, files }) => {
            let mut args = Vec::new();
            if let Some(c) = channel {
                args.push("-c".to_string());
                args.push(c);
            }
            if no_progress {
                args.push("--no-progress".to_string());
            }
            args.extend(files);
            if let Some(s) = summary {
                args.push("--summary".to_string());
                args.push(s);
            }
            Action::Upload { args }
        }
        Some(Commands::Channels { command }) => match command {
            None => Action::ShowSubcommandHelp("channels".to_string()),
            Some(ChannelsCommands::Create { channel, private, public, authenticated }) => {
                let mut args = vec!["create".to_string()];
                if private {
                    args.push("--private".to_string());
                }
                if public {
                    args.push("--public".to_string());
                }
                if authenticated {
                    args.push("--authenticated".to_string());
                }
                args.push(channel.clone());
                Action::Channels { args }
            }
            Some(ChannelsCommands::Remove { channel }) => {
                let args = vec!["remove".to_string(), channel.clone()];
                Action::Channels { args }
            }
        },
    };

    (action, level)
}

/// Extract the subcommand path from ArgMatches by walking the subcommand chain.
/// This derives the path from clap's own metadata rather than hardcoding strings.
fn get_subcommand_path_from_matches(matches: &clap::ArgMatches) -> Option<String> {
    let mut path_parts = Vec::new();
    let mut current = matches;

    while let Some((name, sub_matches)) = current.subcommand() {
        path_parts.push(name.to_string());
        current = sub_matches;
    }

    if path_parts.is_empty() {
        None
    } else {
        Some(path_parts.join(" "))
    }
}

fn handle_parse_error(e: clap::Error) -> (Action, LogLevel) {
    if e.kind() == clap::error::ErrorKind::DisplayVersion {
        return (Action::ShowVersion, LogLevel::Off);
    }

    // Handle unknown subcommand errors with custom format
    let err_str = e.to_string();
    if err_str.contains("unrecognized subcommand") {
        let args: Vec<String> = std::env::args().collect();
        if args.len() > 1 && args[1] == "self" {
            if args.len() > 2 {
                tracing::error!("Unknown self command: {}", args[2]);
                eprintln!("Unknown self command: {}", args[2]);
            }
        } else if args.len() > 1 {
            tracing::error!("Unknown command: {}", args[1]);
            eprintln!("Unknown command: {}", args[1]);
        }
        std::process::exit(1);
    }

    // For other errors, use clap's error handling
    e.exit();
}

/// Get subcommand names and descriptions from clap for help introspection.
/// Filters out experimental commands when their features are not enabled.
fn get_subcommand_descriptions() -> HashMap<String, String> {
    #[cfg(unix)]
    let show_ob = feature::is_feature_enabled("outerbounds");
    #[cfg(not(unix))]
    let show_ob = false;

    Cli::command()
        .get_subcommands()
        .filter(|s| show_ob || s.get_name() != "ob")
        .map(|s| {
            (
                s.get_name().to_string(),
                s.get_about().map(|a| a.to_string()).unwrap_or_default(),
            )
        })
        .collect()
}

/// Get a subcommand's clap Command by name (supports nested paths like "self update")
fn get_subcommand(path: &str) -> clap::Command {
    let parts: Vec<&str> = path.split_whitespace().collect();
    let mut cmd = Cli::command();

    for part in parts {
        let subcmd = cmd
            .get_subcommands()
            .find(|s| s.get_name() == part)
            .cloned()
            .expect("subcommand should exist");
        cmd = subcmd;
    }

    cmd
}

#[derive(Parser)]
#[command(
    name = "ana",
    version = VERSION,
    about = "",
    long_about = None,
    subcommand_required = false,
    arg_required_else_help = false,
    disable_help_subcommand = true,
    disable_help_flag = true,
    override_usage = "ana [command] [options]",
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Increase verbosity (-v=error, -vv=warn, -vvv=info, -vvvv=debug, -vvvvv=trace)
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Show help information
    #[arg(short = 'h', long = "help", global = true, action = clap::ArgAction::SetTrue)]
    help: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Authentication commands
    #[command(
        subcommand_required = false,
        arg_required_else_help = false,
        override_usage = "ana auth <command> [options]"
    )]
    Auth {
        #[command(subcommand)]
        command: Option<AuthCommands>,
    },

    /// Install the Anaconda CLI
    Bootstrap,

    /// Show current configuration
    Config,

    /// Log in to Anaconda
    Login {
        /// API key to use directly (bypasses device flow). Use '-' to read from stdin.
        api_key: Option<String>,

        /// Prompt for API key (hidden input) instead of using device flow
        #[arg(long = "api-key")]
        prompt_api_key: bool,

        /// Overwrite existing credentials without confirmation
        #[arg(long, short = 'f')]
        force: bool,
    },

    /// Log out from Anaconda
    Logout,

    /// Display information about the logged-in user
    Whoami {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Manage the ana installation
    #[command(
        subcommand_required = false,
        arg_required_else_help = false,
        override_usage = "ana self <command> [options]"
    )]
    Self_ {
        #[command(subcommand)]
        command: Option<SelfCommands>,
    },

    /// Interact with anaconda.org
    #[command(
        trailing_var_arg = true,
        override_usage = "ana org <command> [options]"
    )]
    Org {
        /// Arguments to pass to anaconda org
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Anaconda MCP — Model Context Protocol tools for AI assistants
    #[command(
        subcommand_required = false,
        arg_required_else_help = false,
        override_usage = "ana mcp <command> [options]"
    )]
    Mcp {
        #[command(subcommand)]
        command: Option<McpCommands>,
    },

    /// Anaconda Repository CLI — manage Package Security Manager
    #[command(
        subcommand_required = false,
        arg_required_else_help = false,
        override_usage = "ana repo <command> [options]"
    )]
    Repo {
        #[command(subcommand)]
        command: Option<RepoCommands>,
    },

    /// Outerbounds platform CLI (experimental)
    #[cfg(unix)]
    #[command(
        subcommand_required = false,
        arg_required_else_help = false,
        override_usage = "ana ob <command> [options]",
        after_help = "Note: Outerbounds integration is an experimental alpha feature."
    )]
    Ob {
        #[command(subcommand)]
        command: Option<ObCommands>,
    },

    /// Manage tools
    #[command(
        subcommand_required = false,
        arg_required_else_help = false,
        override_usage = "ana tool <command> [options]"
    )]
    Tool {
        #[command(subcommand)]
        command: Option<ToolCommands>,
    },

    /// API commands
    #[command(
        subcommand_required = false,
        arg_required_else_help = false,
        override_usage = "ana api <command> [options]"
    )]
    Api {
        #[command(subcommand)]
        command: Option<ApiCommands>,
    },

    /// Enable or disable Anaconda features
    #[command(
        subcommand_required = false,
        arg_required_else_help = false,
        override_usage = "ana feature <command> [options]"
    )]
    Feature {
        #[command(subcommand)]
        command: Option<FeatureCommands>,
    },

    /// Submit pending telemetry batches (internal use only)
    #[command(hide = true)]
    TelemetrySubmit,

    /// Kill background telemetry processes (internal use only)
    #[command(hide = true)]
    TelemetryKill,

    /// Check status of background telemetry processes (internal use only)
    #[command(hide = true)]
    TelemetryStatus,

    /// Upload a package
    #[command(override_usage = "ana upload [options] <file>")]
    Upload {
        /// Target channel
        #[arg(short = 'c', long = "channel")]
        channel: Option<String>,

        /// Don't show upload progress
        #[arg(long)]
        no_progress: bool,

        /// Package summary
        #[arg(short = 's', long)]
        summary: Option<String>,

        /// Files to upload
        files: Vec<String>,
    },

    /// Manage channels
    #[command(
        subcommand_required = false,
        arg_required_else_help = false,
        override_usage = "ana channels <command> [options]"
    )]
    Channels {
        #[command(subcommand)]
        command: Option<ChannelsCommands>,
    },
}

#[derive(Subcommand)]
enum ChannelsCommands {
    /// Create a new channel
    #[command(group = clap::ArgGroup::new("privacy").required(false).multiple(false))]
    Create {
        /// Channel name
        #[arg(required_unless_present = "help", default_value = "")]
        channel: String,

        /// Create a private channel
        #[arg(long, group = "privacy")]
        private: bool,

        /// Create a public channel
        #[arg(long, group = "privacy")]
        public: bool,

        /// Create an authenticated channel
        #[arg(long, group = "privacy")]
        authenticated: bool,
    },

    /// Remove a channel
    #[command(override_usage = "ana channels remove <org>/<channel>")]
    Remove {
        /// Channel in format org/channel
        #[arg(required_unless_present = "help", default_value = "")]
        channel: String,
    },
}

#[derive(Subcommand)]
enum AuthCommands {
    /// Display the API key for the logged-in user
    ApiKey,

    /// Log in to Anaconda
    Login {
        /// API key to use directly (bypasses device flow). Use '-' to read from stdin.
        api_key: Option<String>,

        /// Prompt for API key (hidden input) instead of using device flow
        #[arg(long = "api-key")]
        prompt_api_key: bool,

        /// Overwrite existing credentials without confirmation
        #[arg(long, short = 'f')]
        force: bool,
    },

    /// Log out from Anaconda
    Logout,

    /// Display information about the logged-in user
    Whoami {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum SelfCommands {
    /// Open GitHub issues page to report bugs or request features
    Feedback,

    /// Manage your ana version
    Update {
        /// Version to install (e.g., v0.0.8)
        version: Option<String>,

        /// Check if an update is available
        #[arg(long, conflicts_with_all = ["list", "version", "force"])]
        check: bool,

        /// List available versions
        #[arg(long, conflicts_with_all = ["check", "version", "force"])]
        list: bool,

        /// Force reinstall even if already on the target version
        #[arg(long, conflicts_with_all = ["check", "list"])]
        force: bool,
    },

    /// Display the user-agent string
    #[command(name = "user-agent")]
    UserAgent {
        /// Optional conda prefix to use for AAU tokens
        #[arg(long)]
        prefix: Option<String>,
    },
}

#[derive(Subcommand)]
enum ToolCommands {
    /// Install a tool
    Install {
        /// Name of the tool to install
        #[arg(required_unless_present = "help", default_value = "")]
        name: String,
    },

    /// List available tools
    List,

    /// Uninstall a tool
    Uninstall {
        /// Name of the tool to uninstall
        #[arg(required_unless_present = "help", default_value = "")]
        name: String,

        /// Skip confirmation prompt
        #[arg(short = 'y', long = "yes")]
        force: bool,
    },
}

#[derive(Subcommand)]
enum ApiCommands {
    /// Fetch data from the API
    Fetch {
        /// API path (e.g., /api/auth/passport)
        #[arg(required_unless_present = "help", default_value = "")]
        url: String,

        /// HTTP method to use
        #[arg(long, default_value = "GET")]
        method: String,

        /// Comma-separated query arguments (e.g., key=value,key2=value2)
        #[arg(short = 'q', long = "query-args")]
        query_args: Option<String>,

        /// Request body data
        #[arg(short = 'd', long, conflicts_with = "json")]
        data: Option<String>,

        /// JSON request body
        #[arg(short = 'j', long, conflicts_with = "data")]
        json: Option<String>,
    },
}

#[derive(Subcommand)]
enum FeatureCommands {
    /// List available features
    List,

    /// Enable a feature
    Enable {
        /// Name of the feature to enable (e.g., main-x)
        #[arg(required_unless_present = "help", default_value = "")]
        name: String,

        /// Skip confirmation prompt
        #[arg(short = 'f', long)]
        force: bool,

        /// Configure pip (for wheels feature)
        #[arg(long, hide = true)]
        pip: bool,

        /// Configure uv (for wheels feature)
        #[arg(long, hide = true)]
        uv: bool,

        /// Configure conda (for main-x feature, default if neither --conda nor --pixi specified)
        #[arg(long)]
        conda: bool,

        /// Configure pixi (for main-x feature)
        #[arg(long)]
        pixi: bool,
    },

    /// Disable a feature
    Disable {
        /// Name of the feature to disable (e.g., main-x)
        #[arg(required_unless_present = "help", default_value = "")]
        name: String,

        /// Skip confirmation prompt
        #[arg(short = 'f', long)]
        force: bool,

        /// Deconfigure pip (for wheels feature)
        #[arg(long, hide = true)]
        pip: bool,

        /// Deconfigure uv (for wheels feature)
        #[arg(long, hide = true)]
        uv: bool,

        /// Deconfigure conda (for main-x feature, default if neither --conda nor --pixi specified)
        #[arg(long)]
        conda: bool,

        /// Deconfigure pixi (for main-x feature)
        #[arg(long)]
        pixi: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_parses() {
        // Verify clap setup is valid
        Cli::command().debug_assert();
    }

    #[test]
    fn test_all_subcommands_in_help_sections() {
        // Commands intentionally hidden from help output
        // "ob" is conditionally hidden based on experimental feature state
        // "bootstrap" is hidden as it's synonymous to `ana tool install anaconda-cli`
        let hidden_from_help: std::collections::HashSet<_> = [
            "org",
            "repo",
            "config",
            "ob",
            "bootstrap",
            "telemetry-submit",
            "telemetry-kill",
            "telemetry-status",
        ]
        .into_iter()
        .collect();

        let cmd = Cli::command();
        let clap_subcommands: std::collections::HashSet<_> = cmd
            .get_subcommands()
            .map(|s| s.get_name())
            .filter(|name| !hidden_from_help.contains(name))
            .collect();

        let help_section_commands: std::collections::HashSet<_> =
            help::get_all_section_commands().into_iter().collect();

        let missing: Vec<_> = clap_subcommands
            .difference(&help_section_commands)
            .collect();

        assert!(
            missing.is_empty(),
            "Subcommands missing from help sections: {:?}. \
             Add them to HELP_SECTIONS in src/help/data.rs",
            missing
        );
    }

    #[test]
    #[cfg(feature = "unstable")]
    fn test_feature_enable_wheels() {
        let cli = Cli::try_parse_from(["ana", "feature", "enable", "wheels"]).unwrap();
        match cli.command {
            Some(Commands::Feature {
                command:
                    Some(FeatureCommands::Enable {
                        name,
                        force,
                        pip,
                        uv,
                        ..
                    }),
            }) => {
                assert_eq!(name, "wheels");
                assert!(!force);
                assert!(!pip);
                assert!(!uv);
            }
            _ => panic!("Expected Feature Enable command"),
        }
    }

    #[test]
    #[cfg(feature = "unstable")]
    fn test_feature_disable_wheels() {
        let cli = Cli::try_parse_from(["ana", "feature", "disable", "wheels"]).unwrap();
        match cli.command {
            Some(Commands::Feature {
                command:
                    Some(FeatureCommands::Disable {
                        name,
                        force,
                        pip,
                        uv,
                        ..
                    }),
            }) => {
                assert_eq!(name, "wheels");
                assert!(!force);
                assert!(!pip);
                assert!(!uv);
            }
            _ => panic!("Expected Feature Disable command"),
        }
    }

    #[test]
    #[cfg(feature = "unstable")]
    fn test_feature_enable_wheels_pip_flag() {
        let cli = Cli::try_parse_from(["ana", "feature", "enable", "wheels", "--pip"]).unwrap();
        match cli.command {
            Some(Commands::Feature {
                command:
                    Some(FeatureCommands::Enable {
                        name,
                        force,
                        pip,
                        uv,
                        ..
                    }),
            }) => {
                assert_eq!(name, "wheels");
                assert!(!force);
                assert!(pip);
                assert!(!uv);
            }
            _ => panic!("Expected Feature Enable command"),
        }
    }

    #[test]
    #[cfg(feature = "unstable")]
    fn test_feature_enable_wheels_uv_flag() {
        let cli = Cli::try_parse_from(["ana", "feature", "enable", "wheels", "--uv"]).unwrap();
        match cli.command {
            Some(Commands::Feature {
                command:
                    Some(FeatureCommands::Enable {
                        name,
                        force,
                        pip,
                        uv,
                        ..
                    }),
            }) => {
                assert_eq!(name, "wheels");
                assert!(!force);
                assert!(!pip);
                assert!(uv);
            }
            _ => panic!("Expected Feature Enable command"),
        }
    }

    #[test]
    #[cfg(feature = "unstable")]
    fn test_feature_enable_wheels_both_flags() {
        let cli =
            Cli::try_parse_from(["ana", "feature", "enable", "wheels", "--pip", "--uv"]).unwrap();
        match cli.command {
            Some(Commands::Feature {
                command:
                    Some(FeatureCommands::Enable {
                        name,
                        force,
                        pip,
                        uv,
                        ..
                    }),
            }) => {
                assert_eq!(name, "wheels");
                assert!(!force);
                assert!(pip);
                assert!(uv);
            }
            _ => panic!("Expected Feature Enable command"),
        }
    }

    #[test]
    #[cfg(feature = "unstable")]
    fn test_feature_disable_wheels_pip_flag() {
        let cli = Cli::try_parse_from(["ana", "feature", "disable", "wheels", "--pip"]).unwrap();
        match cli.command {
            Some(Commands::Feature {
                command:
                    Some(FeatureCommands::Disable {
                        name,
                        force,
                        pip,
                        uv,
                        ..
                    }),
            }) => {
                assert_eq!(name, "wheels");
                assert!(!force);
                assert!(pip);
                assert!(!uv);
            }
            _ => panic!("Expected Feature Disable command"),
        }
    }

    #[test]
    #[cfg(feature = "unstable")]
    fn test_feature_disable_wheels_uv_flag() {
        let cli = Cli::try_parse_from(["ana", "feature", "disable", "wheels", "--uv"]).unwrap();
        match cli.command {
            Some(Commands::Feature {
                command:
                    Some(FeatureCommands::Disable {
                        name,
                        force,
                        pip,
                        uv,
                        ..
                    }),
            }) => {
                assert_eq!(name, "wheels");
                assert!(!force);
                assert!(!pip);
                assert!(uv);
            }
            _ => panic!("Expected Feature Disable command"),
        }
    }

    #[test]
    #[cfg(feature = "unstable")]
    fn test_feature_disable_wheels_both_flags() {
        let cli =
            Cli::try_parse_from(["ana", "feature", "disable", "wheels", "--pip", "--uv"]).unwrap();
        match cli.command {
            Some(Commands::Feature {
                command:
                    Some(FeatureCommands::Disable {
                        name,
                        force,
                        pip,
                        uv,
                        ..
                    }),
            }) => {
                assert_eq!(name, "wheels");
                assert!(!force);
                assert!(pip);
                assert!(uv);
            }
            _ => panic!("Expected Feature Disable command"),
        }
    }

    #[test]
    fn test_subcommand_path_from_matches_feature_enable() {
        let matches = Cli::command()
            .try_get_matches_from(["ana", "feature", "enable", "main-x"])
            .unwrap();
        let path = get_subcommand_path_from_matches(&matches);
        assert_eq!(path, Some("feature enable".to_string()));
    }

    #[test]
    fn test_subcommand_path_from_matches_self_update() {
        let matches = Cli::command()
            .try_get_matches_from(["ana", "self", "update"])
            .unwrap();
        let path = get_subcommand_path_from_matches(&matches);
        assert_eq!(path, Some("self update".to_string()));
    }

    #[test]
    fn test_subcommand_path_from_matches_no_subcommand() {
        let matches = Cli::command().try_get_matches_from(["ana"]).unwrap();
        let path = get_subcommand_path_from_matches(&matches);
        assert_eq!(path, None);
    }

    #[test]
    fn test_help_flag_with_feature_enable_and_argument() {
        // "ana feature enable main-x --help" should parse successfully with help=true
        let cli = Cli::try_parse_from(["ana", "feature", "enable", "main-x", "--help"]).unwrap();
        assert!(cli.help);
        match cli.command {
            Some(Commands::Feature {
                command: Some(FeatureCommands::Enable { name, .. }),
            }) => {
                assert_eq!(name, "main-x");
            }
            _ => panic!("Expected Feature Enable command"),
        }
    }

    #[test]
    fn test_help_flag_position_before_argument() {
        // "ana feature enable --help main-x" should also work
        let cli = Cli::try_parse_from(["ana", "feature", "enable", "--help", "main-x"]).unwrap();
        assert!(cli.help);
        match cli.command {
            Some(Commands::Feature {
                command: Some(FeatureCommands::Enable { name, .. }),
            }) => {
                assert_eq!(name, "main-x");
            }
            _ => panic!("Expected Feature Enable command"),
        }
    }

    #[test]
    fn test_help_flag_global_at_root() {
        // "ana --help" should work
        let cli = Cli::try_parse_from(["ana", "--help"]).unwrap();
        assert!(cli.help);
        assert!(cli.command.is_none());
    }

    #[test]
    fn test_extract_channel_arg_channels_create() {
        let args = vec!["create".to_string(), "org/channel".to_string()];
        let result = extract_channel_arg("channels", &args);
        assert_eq!(result, Some("org/channel".to_string()));
    }

    #[test]
    fn test_extract_channel_arg_channels_create_with_flags() {
        let args = vec!["create".to_string(), "--private".to_string(), "org/channel".to_string()];
        let result = extract_channel_arg("channels", &args);
        assert_eq!(result, Some("org/channel".to_string()));
    }

    #[test]
    fn test_extract_channel_arg_channels_remove() {
        let args = vec!["remove".to_string(), "org/channel".to_string()];
        let result = extract_channel_arg("channels", &args);
        assert_eq!(result, Some("org/channel".to_string()));
    }

    #[test]
    fn test_extract_channel_arg_upload_with_channel_flag() {
        let args = vec!["-c".to_string(), "org/channel".to_string(), "file.tar.gz".to_string()];
        let result = extract_channel_arg("upload", &args);
        assert_eq!(result, Some("org/channel".to_string()));
    }

    #[test]
    fn test_extract_channel_arg_upload_with_channel_long_flag() {
        let args = vec!["--channel".to_string(), "org/channel".to_string(), "file.tar.gz".to_string()];
        let result = extract_channel_arg("upload", &args);
        assert_eq!(result, Some("org/channel".to_string()));
    }

    #[test]
    fn test_extract_channel_arg_no_slash_returns_none() {
        let args = vec!["create".to_string(), "channel".to_string()];
        let result = extract_channel_arg("channels", &args);
        assert_eq!(result, None);
    }

    #[test]
    fn test_transform_args_for_org_upload_channel_to_user() {
        let args = vec!["-c".to_string(), "username".to_string(), "file.tar.gz".to_string()];
        let result = transform_args_for_org("upload", &args);
        assert_eq!(result, vec!["-u", "username", "file.tar.gz"]);
    }

    #[test]
    fn test_transform_args_for_org_upload_channel_long_to_user() {
        let args = vec!["--channel".to_string(), "username".to_string(), "file.tar.gz".to_string()];
        let result = transform_args_for_org("upload", &args);
        assert_eq!(result, vec!["--user", "username", "file.tar.gz"]);
    }

    #[test]
    fn test_filter_args_for_repo_strips_summary() {
        let args = vec![
            "upload".to_string(),
            "--summary".to_string(),
            "test summary".to_string(),
            "-c".to_string(),
            "org/channel".to_string(),
            "file.tar.gz".to_string(),
        ];
        let result = filter_args_for_repo("upload", &args);
        assert_eq!(result, vec!["upload", "-c", "org/channel", "file.tar.gz"]);
    }

    #[test]
    fn test_filter_args_for_repo_strips_summary_short_flag() {
        let args = vec![
            "upload".to_string(),
            "-s".to_string(),
            "test summary".to_string(),
            "-c".to_string(),
            "org/channel".to_string(),
            "file.tar.gz".to_string(),
        ];
        let result = filter_args_for_repo("upload", &args);
        assert_eq!(result, vec!["upload", "-c", "org/channel", "file.tar.gz"]);
    }

    #[test]
    fn test_filter_args_for_repo_preserves_other_args() {
        let args = vec![
            "upload".to_string(),
            "--no-progress".to_string(),
            "-c".to_string(),
            "org/channel".to_string(),
            "file.tar.gz".to_string(),
        ];
        let result = filter_args_for_repo("upload", &args);
        assert_eq!(result, vec!["upload", "--no-progress", "-c", "org/channel", "file.tar.gz"]);
    }
}
