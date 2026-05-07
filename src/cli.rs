use std::collections::HashMap;
use std::time::Instant;

use anaconda_otel_rs::signals::{increment_counter, record_histogram, shutdown_telemetry};
use clap::{CommandFactory, Parser, Subcommand};
use miette::{IntoDiagnostic, miette};

use crate::VERSION;
use crate::anaconda_cli;
use crate::auth;
use crate::config::{self, Config};
use crate::context::CommandContext;
use crate::feature;
#[cfg(feature = "feedback")]
use crate::feedback::{self, FeedbackType};
use crate::fetch::api_fetch;
use crate::help;
use crate::outerbounds::{self, ObAppCommands, ObCommands, ObFlowprojectCommands};
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

    config::setup_telemetry();

    let result = action.execute().await;

    shutdown_telemetry();

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
    },
    CheckForUpdate,
    ShowAvailableVersions,
    Bootstrap,
    OrgProxy {
        args: Vec<String>,
    },
    ObProxy {
        args: Vec<String>,
    },
    UserAgent {
        prefix: Option<String>,
    },
    #[cfg(feature = "feedback")]
    OpenFeedback {
        feedback_type: Option<FeedbackType>,
        description: Option<String>,
    },
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
    },
    FeatureDisable {
        feature: String,
        force: bool,
        pip: bool,
        uv: bool,
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
            Action::ObProxy { .. } => "ob",
            Action::UserAgent { .. } => "user-agent",
            #[cfg(feature = "feedback")]
            Action::OpenFeedback { .. } => "feedback",
            Action::ToolInstall { .. } => "tool.install",
            Action::ToolUninstall { .. } => "tool.uninstall",
            Action::ToolList => "tool.list",
            Action::ApiFetch { .. } => "api.fetch",
            Action::FeatureEnable { feature, .. } => match feature.as_str() {
                "main-x" => "feature.enable.main-x",
                "wheels" => "feature.enable.wheels",
                _ => "feature.enable.unknown",
            },
            Action::FeatureDisable { feature, .. } => match feature.as_str() {
                "main-x" => "feature.disable.main-x",
                "wheels" => "feature.disable.wheels",
                _ => "feature.disable.unknown",
            },
        }
    }

    /// Execute the action with telemetry middleware
    pub async fn execute(self) -> miette::Result<()> {
        let name = self.match_action_name();
        let mut ctx = CommandContext::new();
        ctx.telemetry.add("command", name);
        increment_counter("cli_command_invoked", 1, ctx.telemetry.attrs());

        let start = Instant::now();
        let result = self.run(&mut ctx).await;
        let duration_ms = start.elapsed().as_millis() as f64;

        match &result {
            Ok(_) => {
                increment_counter("cli_command_success", 1, ctx.telemetry.attrs());
                record_histogram(
                    "cli_command_success_duration_ms",
                    duration_ms,
                    ctx.telemetry.into_attrs(),
                );
            }
            Err(_) => {
                increment_counter("cli_command_failure", 1, ctx.telemetry.attrs());
                record_histogram(
                    "cli_command_failure_duration_ms",
                    duration_ms,
                    ctx.telemetry.into_attrs(),
                );
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
            Action::ObProxy { args } => outerbounds::run(ctx, &args).await,
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
            Action::Update { version } => {
                update::run_update(ctx, VERSION, version).await;
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
            #[cfg(feature = "feedback")]
            Action::OpenFeedback {
                feedback_type,
                description,
            } => {
                feedback::open_feedback(ctx, feedback_type, description);
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
            Action::FeatureEnable {
                feature,
                force,
                pip,
                uv,
            } => {
                match feature.as_str() {
                    "main-x" => feature::enable_main_x(ctx, force).await?,
                    "wheels" => feature::enable_wheels(ctx, force, pip, uv).await?,
                    _ => return Err(miette!("Unknown feature: {}", feature)),
                }
                Ok(())
            }
            Action::FeatureDisable {
                feature,
                force,
                pip,
                uv,
            } => {
                match feature.as_str() {
                    "main-x" => feature::disable_main_x(ctx, force).await?,
                    "wheels" => feature::disable_wheels(ctx, force, pip, uv).await?,
                    _ => return Err(miette!("Unknown feature: {}", feature)),
                }
                Ok(())
            }
        }
    }
}

/// Parse CLI arguments and return the action to perform along with log level.
/// Exits the process on unrecoverable errors (unknown commands, etc.)
pub fn parse() -> (Action, LogLevel) {
    match Cli::try_parse() {
        Ok(cli) => {
            let level: LogLevel = cli.verbose.into();
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
                    #[cfg(feature = "feedback")]
                    Some(SelfCommands::Feedback {
                        bug,
                        feature,
                        description,
                    }) => Action::OpenFeedback {
                        feedback_type: feedback::parse_feedback_type(bug, feature),
                        description,
                    },
                    Some(SelfCommands::Update {
                        version,
                        check,
                        list,
                    }) => {
                        if check {
                            Action::CheckForUpdate
                        } else if list {
                            Action::ShowAvailableVersions
                        } else {
                            Action::Update { version }
                        }
                    }
                    Some(SelfCommands::UserAgent { prefix }) => Action::UserAgent { prefix },
                },
                Some(Commands::Org { args }) => Action::OrgProxy { args },
                Some(Commands::Ob { command }) => match command {
                    None => Action::ShowSubcommandHelp("ob".to_string()),
                    Some(ObCommands::Init {
                        path,
                        name,
                        title,
                        no_git_init,
                    }) => {
                        let mut args = vec!["init".to_string()];
                        if let Some(p) = path {
                            args.push(p);
                        }
                        if let Some(n) = name {
                            args.push("--name".to_string());
                            args.push(n);
                        }
                        if let Some(t) = title {
                            args.push("--title".to_string());
                            args.push(t);
                        }
                        if no_git_init {
                            args.push("--no-git-init".to_string());
                        }
                        Action::ObProxy { args }
                    }
                    Some(ObCommands::Deploy { args: deploy_args }) => {
                        let mut args = vec!["deploy".to_string()];
                        args.extend(deploy_args);
                        Action::ObProxy { args }
                    }
                    Some(ObCommands::App { command: app_cmd }) => match app_cmd {
                        None => Action::ShowSubcommandHelp("ob app".to_string()),
                        Some(ObAppCommands::Open { name }) => Action::ObProxy {
                            args: vec!["app".to_string(), "open".to_string(), name],
                        },
                        Some(ObAppCommands::View { web }) => {
                            let mut args = vec!["app".to_string(), "view".to_string()];
                            if web {
                                args.push("--web".to_string());
                            }
                            Action::ObProxy { args }
                        }
                        Some(ObAppCommands::Delete { args: cmd_args }) => {
                            let mut args = vec!["app".to_string(), "delete".to_string()];
                            args.extend(cmd_args);
                            Action::ObProxy { args }
                        }
                        Some(ObAppCommands::Deploy { args: cmd_args }) => {
                            let mut args = vec!["app".to_string(), "deploy".to_string()];
                            args.extend(cmd_args);
                            Action::ObProxy { args }
                        }
                        Some(ObAppCommands::Info { args: cmd_args }) => {
                            let mut args = vec!["app".to_string(), "info".to_string()];
                            args.extend(cmd_args);
                            Action::ObProxy { args }
                        }
                        Some(ObAppCommands::List { args: cmd_args }) => {
                            let mut args = vec!["app".to_string(), "list".to_string()];
                            args.extend(cmd_args);
                            Action::ObProxy { args }
                        }
                        Some(ObAppCommands::Logs { args: cmd_args }) => {
                            let mut args = vec!["app".to_string(), "logs".to_string()];
                            args.extend(cmd_args);
                            Action::ObProxy { args }
                        }
                    },
                    Some(ObCommands::Check { args: check_args }) => {
                        let mut args = vec!["check".to_string()];
                        args.extend(check_args);
                        Action::ObProxy { args }
                    }
                    Some(ObCommands::Configure { args: cfg_args }) => {
                        let mut args = vec!["configure".to_string()];
                        args.extend(cfg_args);
                        Action::ObProxy { args }
                    }
                    Some(ObCommands::FastBakery { args: fb_args }) => {
                        let mut args = vec!["fast-bakery".to_string()];
                        args.extend(fb_args);
                        Action::ObProxy { args }
                    }
                    Some(ObCommands::Integrations { args: int_args }) => {
                        let mut args = vec!["integrations".to_string()];
                        args.extend(int_args);
                        Action::ObProxy { args }
                    }
                    Some(ObCommands::Kubernetes { args: k8s_args }) => {
                        let mut args = vec!["kubernetes".to_string()];
                        args.extend(k8s_args);
                        Action::ObProxy { args }
                    }
                    Some(ObCommands::Perimeter { args: perm_args }) => {
                        let mut args = vec!["perimeter".to_string()];
                        args.extend(perm_args);
                        Action::ObProxy { args }
                    }
                    Some(ObCommands::ServicePrincipalConfigure { args: spc_args }) => {
                        let mut args = vec!["service-principal-configure".to_string()];
                        args.extend(spc_args);
                        Action::ObProxy { args }
                    }
                    Some(ObCommands::Flowproject { command: fp_cmd }) => match fp_cmd {
                        None => Action::ShowSubcommandHelp("ob flowproject".to_string()),
                        Some(ObFlowprojectCommands::DeleteMetadata { args: dm_args }) => {
                            let mut args =
                                vec!["flowproject".to_string(), "delete-metadata".to_string()];
                            args.extend(dm_args);
                            Action::ObProxy { args }
                        }
                        Some(ObFlowprojectCommands::GetMetadata { args: gm_args }) => {
                            let mut args =
                                vec!["flowproject".to_string(), "get-metadata".to_string()];
                            args.extend(gm_args);
                            Action::ObProxy { args }
                        }
                        Some(ObFlowprojectCommands::ListTemplates { args: lt_args }) => {
                            let mut args =
                                vec!["flowproject".to_string(), "list-templates".to_string()];
                            args.extend(lt_args);
                            Action::ObProxy { args }
                        }
                        Some(ObFlowprojectCommands::SetMetadata { args: sm_args }) => {
                            let mut args =
                                vec!["flowproject".to_string(), "set-metadata".to_string()];
                            args.extend(sm_args);
                            Action::ObProxy { args }
                        }
                        Some(ObFlowprojectCommands::TeardownBranch { args: tb_args }) => {
                            let mut args =
                                vec!["flowproject".to_string(), "teardown-branch".to_string()];
                            args.extend(tb_args);
                            Action::ObProxy { args }
                        }
                    },
                },
                Some(Commands::Tool { command }) => match command {
                    None => Action::ShowSubcommandHelp("tool".to_string()),
                    Some(ToolCommands::Install { name }) => Action::ToolInstall { name },
                    Some(ToolCommands::List) => Action::ToolList,
                    Some(ToolCommands::Uninstall { name, force }) => {
                        Action::ToolUninstall { name, force }
                    }
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
                    }) => Action::FeatureEnable {
                        feature: name,
                        force,
                        pip,
                        uv,
                    },
                    Some(FeatureCommands::Disable {
                        name,
                        force,
                        pip,
                        uv,
                    }) => Action::FeatureDisable {
                        feature: name,
                        force,
                        pip,
                        uv,
                    },
                },
            };
            (action, level)
        }
        Err(e) => handle_parse_error(e),
    }
}

/// Check if a string is a valid subcommand name
fn is_valid_subcommand(name: &str) -> bool {
    Cli::command()
        .get_subcommands()
        .any(|s| s.get_name() == name)
}

fn handle_parse_error(e: clap::Error) -> (Action, LogLevel) {
    if e.kind() == clap::error::ErrorKind::DisplayHelp {
        // Check if help was requested for a subcommand (including nested ones)
        let args: Vec<String> = std::env::args().collect();
        // Collect all non-flag args after the binary name to build the subcommand path
        let subcommand_parts: Vec<&str> = args
            .iter()
            .skip(1)
            .filter(|a| !a.starts_with('-'))
            .map(|s| s.as_str())
            .collect();

        if !subcommand_parts.is_empty() && is_valid_subcommand(subcommand_parts[0]) {
            let subcommand_path = subcommand_parts.join(" ");
            return (Action::ShowSubcommandHelp(subcommand_path), LogLevel::Off);
        }
        return (Action::ShowHelp, LogLevel::Off);
    }
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

/// Get subcommand names and descriptions from clap for help introspection
fn get_subcommand_descriptions() -> HashMap<String, String> {
    Cli::command()
        .get_subcommands()
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
    override_usage = "ana [command] [options]",
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Increase verbosity (-v=error, -vv=warn, -vvv=info, -vvvv=debug, -vvvvv=trace)
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count, global = true)]
    verbose: u8,
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

    /// Outerbounds platform CLI (experimental)
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
    /// Open the feedback form
    #[cfg(feature = "feedback")]
    Feedback {
        /// Report a bug
        #[arg(long, conflicts_with = "feature")]
        bug: bool,

        /// Request a feature
        #[arg(long, conflicts_with = "bug")]
        feature: bool,

        /// Pre-fill the description
        description: Option<String>,
    },

    /// Manage your ana version
    Update {
        /// Version to install (e.g., v0.0.8)
        version: Option<String>,

        /// Check if an update is available
        #[arg(long, conflicts_with_all = ["list", "version"])]
        check: bool,

        /// List available versions
        #[arg(long, conflicts_with_all = ["check", "version"])]
        list: bool,
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
        name: String,
    },

    /// List available tools
    List,

    /// Uninstall a tool
    Uninstall {
        /// Name of the tool to uninstall
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
    /// Enable a feature
    Enable {
        /// Name of the feature to enable (e.g., main-x, wheels)
        name: String,

        /// Skip confirmation prompt
        #[arg(short = 'f', long)]
        force: bool,

        /// Configure pip (for wheels feature)
        #[arg(long)]
        pip: bool,

        /// Configure uv (for wheels feature)
        #[arg(long)]
        uv: bool,
    },

    /// Disable a feature
    Disable {
        /// Name of the feature to disable (e.g., main-x, wheels)
        name: String,

        /// Skip confirmation prompt
        #[arg(short = 'f', long)]
        force: bool,

        /// Deconfigure pip (for wheels feature)
        #[arg(long)]
        pip: bool,

        /// Deconfigure uv (for wheels feature)
        #[arg(long)]
        uv: bool,
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
        let hidden_from_help: std::collections::HashSet<_> =
            ["org", "config"].into_iter().collect();

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
}
