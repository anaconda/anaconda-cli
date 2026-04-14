use std::collections::HashMap;
use std::env::consts::{ARCH, OS};
use std::time::Instant;

use anaconda_otel_rs::signals::{increment_counter, record_histogram, shutdown_telemetry};
use clap::{CommandFactory, Parser, Subcommand};
use opentelemetry::Value;

use crate::VERSION;
use crate::anaconda_cli;
use crate::auth;
use crate::config::{self, Config};
#[cfg(feature = "feedback")]
use crate::feedback::{self, FeedbackType};
use crate::help;
use crate::project;
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

/// Build base telemetry attributes with system information.
fn system_attrs() -> HashMap<String, Value> {
    let mut attrs = HashMap::new();
    attrs.insert("os".to_string(), OS.into());
    attrs.insert("arch".to_string(), ARCH.into());
    attrs.insert("version".to_string(), VERSION.into());
    attrs
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
        eprintln!("Error: {}", e);
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
    Login,
    Logout,
    ShowApiKey,
    Whoami,
    Install,
    Run {
        task: String,
    },
    RunCommand {
        command: Vec<String>,
    },
    Shell,
    Update {
        force: bool,
    },
    CheckForUpdate,
    ShowAvailableVersions,
    Bootstrap,
    OrgProxy {
        args: Vec<String>,
    },
    #[cfg(feature = "feedback")]
    OpenFeedback {
        feedback_type: Option<FeedbackType>,
        description: Option<String>,
    },
}

impl Action {
    fn match_action_name(&self) -> &'static str {
        match self {
            Action::ShowHelp => "help",
            Action::ShowSubcommandHelp(_) => "subcommand.help",
            Action::ShowVersion => "version",
            Action::ShowConfig => "config",
            Action::Login => "login",
            Action::Logout => "logout",
            Action::ShowApiKey => "auth.api-key",
            Action::Whoami => "whoami",
            Action::Install => "install",
            Action::Run { .. } => "run",
            Action::RunCommand { .. } => "run",
            Action::Shell => "shell",
            Action::Update { .. } => "self.update",
            Action::CheckForUpdate => "self.update.check",
            Action::ShowAvailableVersions => "self.update.list",
            Action::Bootstrap => "bootstrap",
            Action::OrgProxy { .. } => "org",
            #[cfg(feature = "feedback")]
            Action::OpenFeedback { .. } => "feedback",
        }
    }

    /// Execute the action with telemetry middleware
    pub async fn execute(self) -> Result<(), Box<dyn std::error::Error>> {
        let name = self.match_action_name();
        let mut attrs = system_attrs();
        attrs.insert("command".to_string(), name.into());
        increment_counter("cli_command_invoked", 1, attrs.clone());

        let start = Instant::now();
        let result = self.run().await;
        let duration_ms = start.elapsed().as_millis() as f64;

        match &result {
            Ok(_) => {
                increment_counter("cli_command_success", 1, attrs.clone());
                record_histogram("cli_command_success_duration_ms", duration_ms, attrs);
            }
            Err(_) => {
                increment_counter("cli_command_failure", 1, attrs.clone());
                record_histogram("cli_command_failure_duration_ms", duration_ms, attrs);
            }
        }

        result
    }

    async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            Action::ShowHelp => {
                let subcommands = get_subcommand_descriptions();
                help::print_help(subcommands);
                Ok(())
            }
            Action::ShowSubcommandHelp(name) => {
                help::print_subcommand_help(&get_subcommand(&name));
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
            Action::Bootstrap => Ok(anaconda_cli::run_bootstrap().await?),
            Action::OrgProxy { args } => Ok(anaconda_cli::run_subcommand("org", &args)?),
            Action::Install => {
                let manifest_path = find_manifest()?;
                let manifest = project::manifest::parse(&manifest_path)?;
                let lockfile_path = manifest
                    .lockfile_path
                    .as_ref()
                    .ok_or("No lockfile (ana.lock or pixi.lock) found")?;
                // No stale lockfile check here — prepare/install is an explicit user
                // action. Staleness guards only apply to run and shell, which would
                // otherwise silently use an outdated environment.
                project::env::install(&manifest_path, lockfile_path).await?;
                Ok(())
            }
            Action::Run { task } => {
                let manifest_path = find_manifest()?;
                let manifest = project::manifest::parse(&manifest_path)?;

                if !manifest.tasks.contains_key(&task) {
                    eprintln!("Unknown task: '{}'\n\nAvailable tasks:", task);
                    project::tasks::list(&manifest.tasks);
                    return Err(format!("Unknown task: '{}'", task).into());
                }

                let env_prefix = ensure_env_installed(&manifest_path, &manifest).await?;
                let status = project::tasks::run(&task, &manifest.tasks, &env_prefix)?;
                if !status.success() {
                    std::process::exit(status.code().unwrap_or(1));
                }
                Ok(())
            }
            Action::RunCommand { command } => {
                let manifest_path = find_manifest()?;
                let manifest = project::manifest::parse(&manifest_path)?;

                let env_prefix = ensure_env_installed(&manifest_path, &manifest).await?;
                let cmd = command.join(" ");
                let status = project::tasks::run_command(&cmd, &env_prefix)?;
                if !status.success() {
                    std::process::exit(status.code().unwrap_or(1));
                }
                Ok(())
            }
            Action::Shell => {
                let manifest_path = find_manifest()?;
                let manifest = project::manifest::parse(&manifest_path)?;
                let _env_prefix = ensure_env_installed(&manifest_path, &manifest).await?;
                let status = project::env::shell(&manifest_path)?;
                if !status.success() {
                    std::process::exit(status.code().unwrap_or(1));
                }
                Ok(())
            }
            Action::Login => Ok(auth::login().await?),
            Action::Logout => Ok(auth::logout()?),
            Action::ShowApiKey => Ok(auth::show_api_key()?),
            Action::Whoami => Ok(auth::whoami().await?),
            Action::Update { force } => {
                update::run_update(VERSION, force).await;
                Ok(())
            }
            Action::CheckForUpdate => {
                update::check_for_update(VERSION).await;
                Ok(())
            }
            Action::ShowAvailableVersions => {
                update::show_available_versions(VERSION).await;
                Ok(())
            }
            #[cfg(feature = "feedback")]
            Action::OpenFeedback {
                feedback_type,
                description,
            } => {
                feedback::open_feedback(feedback_type, description);
                Ok(())
            }
        }
    }
}

/// Find the project manifest in the current directory.
fn find_manifest() -> Result<std::path::PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| format!("Failed to get cwd: {}", e))?;
    project::manifest::find_manifest(&cwd)
        .ok_or_else(|| "No ana.toml or pixi.toml found in current directory".to_string())
}

/// Check that a lockfile exists and is not stale relative to the manifest.
///
/// At this layer (install-only), there is no solver to regenerate the lockfile,
/// so we error if the lockfile is out of date.
fn require_lockfile_current(
    manifest_path: &std::path::Path,
    lockfile_path: &std::path::Path,
) -> Result<(), String> {
    if project::env::lockfile_is_stale(manifest_path, lockfile_path) {
        Err("Lockfile is out of date (manifest has been modified). \
             Please regenerate the lockfile with your lockfile manager (e.g. pixi)."
            .to_string())
    } else {
        Ok(())
    }
}

/// Ensure the project environment is installed, auto-installing if needed.
/// Returns the environment prefix path.
async fn ensure_env_installed(
    manifest_path: &std::path::Path,
    manifest: &project::manifest::Manifest,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let lockfile_path = manifest
        .lockfile_path
        .as_ref()
        .ok_or("No lockfile (ana.lock or pixi.lock) found")?;

    require_lockfile_current(manifest_path, lockfile_path)?;

    if !project::env::is_installed(manifest_path) {
        eprintln!("Environment not installed, running install first...");
        project::env::install(manifest_path, lockfile_path).await?;
    }
    Ok(project::env::env_prefix(manifest_path))
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
                Some(Commands::Install) => Action::Install,
                Some(Commands::Login) => Action::Login,
                Some(Commands::Logout) => Action::Logout,
                Some(Commands::Run { task, args }) => {
                    if !args.is_empty() {
                        Action::RunCommand { command: args }
                    } else if let Some(task) = task {
                        Action::Run { task }
                    } else {
                        Action::ShowHelp
                    }
                }
                Some(Commands::Shell) => Action::Shell,
                Some(Commands::Whoami) => Action::Whoami,
                Some(Commands::Auth { command }) => match command {
                    None => Action::ShowSubcommandHelp("auth".to_string()),
                    Some(AuthCommands::ApiKey) => Action::ShowApiKey,
                    Some(AuthCommands::Login) => Action::Login,
                    Some(AuthCommands::Logout) => Action::Logout,
                    Some(AuthCommands::Whoami) => Action::Whoami,
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
                    Some(SelfCommands::Update { yes, check, list }) => {
                        if check {
                            Action::CheckForUpdate
                        } else if list {
                            Action::ShowAvailableVersions
                        } else {
                            Action::Update { force: yes }
                        }
                    }
                },
                Some(Commands::Org { args }) => Action::OrgProxy { args },
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
        // Check if help was requested for a subcommand
        let args: Vec<String> = std::env::args().collect();
        if args.len() > 1 {
            let subcommand = &args[1];
            // Check if it's a valid subcommand (not a flag)
            if !subcommand.starts_with('-') && is_valid_subcommand(subcommand) {
                return (
                    Action::ShowSubcommandHelp(subcommand.clone()),
                    LogLevel::Off,
                );
            }
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

/// Get a subcommand's clap Command by name
fn get_subcommand(name: &str) -> clap::Command {
    Cli::command()
        .get_subcommands()
        .find(|s| s.get_name() == name)
        .cloned()
        .expect("subcommand should exist")
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

    /// Install the project environment from lockfile
    #[command(name = "prepare", alias = "install")]
    Install,

    /// Log in to Anaconda
    Login,

    /// Log out from Anaconda
    Logout,

    /// Run a project task or arbitrary command in the project environment
    Run {
        /// Task name to run, or use -- to run an arbitrary command
        task: Option<String>,

        /// Arbitrary command and arguments (after --)
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// Launch a subshell with the project environment active
    Shell,

    /// Display information about the logged-in user
    Whoami,

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
}

#[derive(Subcommand)]
enum AuthCommands {
    /// Display the API key for the logged-in user
    ApiKey,

    /// Log in to Anaconda
    Login,

    /// Log out from Anaconda
    Logout,

    /// Display information about the logged-in user
    Whoami,
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

    /// Update ana to the latest version
    Update {
        /// Skip confirmation prompt
        #[arg(short = 'y', long = "yes")]
        yes: bool,

        /// Check if an update is available
        #[arg(long, conflicts_with_all = ["yes", "list"])]
        check: bool,

        /// List available versions
        #[arg(long, conflicts_with_all = ["yes", "check"])]
        list: bool,
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
        let cmd = Cli::command();
        let clap_subcommands: std::collections::HashSet<_> =
            cmd.get_subcommands().map(|s| s.get_name()).collect();

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
}
