use std::collections::HashMap;

use std::time::Instant;

use anaconda_otel_rs::signals::{increment_counter, record_histogram, shutdown_telemetry};
use clap::{Parser, Subcommand};
use indoc::formatdoc;

use crate::VERSION;
use crate::anaconda_cli;
use crate::auth;
use crate::config::{self, Config};
use crate::update;

pub fn execute() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    config::setup_telemetry();

    let result = parse().execute();

    shutdown_telemetry();

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

/// Action to be performed, returned by parse()
pub enum Action {
    ShowHelp,
    ShowSelfHelp,
    ShowAuthHelp,
    ShowVersion,
    ShowConfig,
    Login,
    Logout,
    ShowApiKey,
    Whoami,
    Update { force: bool },
    CheckForUpdate,
    ShowAvailableVersions,
    Bootstrap,
}

impl Action {
    fn match_action_name(&self) -> &'static str {
        match self {
            Action::ShowHelp => "help",
            Action::ShowSelfHelp => "self.help",
            Action::ShowAuthHelp => "auth.help",
            Action::ShowVersion => "version",
            Action::ShowConfig => "config",
            Action::Login => "login",
            Action::Logout => "logout",
            Action::ShowApiKey => "auth.api-key",
            Action::Whoami => "whoami",
            Action::Update { .. } => "self.update",
            Action::CheckForUpdate => "self.update.check",
            Action::ShowAvailableVersions => "self.update.list",
            Action::Bootstrap => "bootstrap",
        }
    }

    /// Execute the action with telemetry middleware
    pub fn execute(self) -> Result<(), Box<dyn std::error::Error>> {
        let name = self.match_action_name();
        let mut attrs = HashMap::new();
        attrs.insert("command".to_string(), name.into());
        increment_counter("cli_command_invoked", 1, attrs.clone());

        let start = Instant::now();
        let result = self.run();
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

    fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            Action::ShowHelp => {
                print_main_help();
                Ok(())
            }
            Action::ShowSelfHelp => {
                print_self_help();
                Ok(())
            }
            Action::ShowAuthHelp => {
                print_auth_help();
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
            Action::Bootstrap => Ok(anaconda_cli::run_bootstrap()?),
            Action::Login => Ok(auth::login()?),
            Action::Logout => Ok(auth::logout()?),
            Action::ShowApiKey => Ok(auth::show_api_key()?),
            Action::Whoami => Ok(auth::whoami()?),
            Action::Update { force } => {
                update::run_update(VERSION, force);
                Ok(())
            }
            Action::CheckForUpdate => {
                update::check_for_update(VERSION);
                Ok(())
            }
            Action::ShowAvailableVersions => {
                update::show_available_versions(VERSION);
                Ok(())
            }
        }
    }
}

/// Parse CLI arguments and return the action to perform.
/// Exits the process on unrecoverable errors (unknown commands, etc.)
pub fn parse() -> Action {
    match Cli::try_parse() {
        Ok(cli) => match cli.command {
            None => Action::ShowHelp,
            Some(Commands::Bootstrap) => Action::Bootstrap,
            Some(Commands::Config) => Action::ShowConfig,
            Some(Commands::Login) => Action::Login,
            Some(Commands::Logout) => Action::Logout,
            Some(Commands::Whoami) => Action::Whoami,
            Some(Commands::Auth { command }) => match command {
                None => Action::ShowAuthHelp,
                Some(AuthCommands::ApiKey) => Action::ShowApiKey,
                Some(AuthCommands::Login) => Action::Login,
                Some(AuthCommands::Logout) => Action::Logout,
                Some(AuthCommands::Whoami) => Action::Whoami,
            },
            Some(Commands::Self_ { command }) => match command {
                None => Action::ShowSelfHelp,
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
        },
        Err(e) => handle_parse_error(e),
    }
}

fn handle_parse_error(e: clap::Error) -> Action {
    if e.kind() == clap::error::ErrorKind::DisplayHelp {
        return Action::ShowHelp;
    }
    if e.kind() == clap::error::ErrorKind::DisplayVersion {
        return Action::ShowVersion;
    }

    // Handle unknown subcommand errors with custom format
    let err_str = e.to_string();
    if err_str.contains("unrecognized subcommand") {
        let args: Vec<String> = std::env::args().collect();
        if args.len() > 1 && args[1] == "self" {
            if args.len() > 2 {
                eprintln!("Unknown self command: {}", args[2]);
            }
        } else if args.len() > 1 {
            eprintln!("Unknown command: {}", args[1]);
        }
        std::process::exit(1);
    }

    // For other errors, use clap's error handling
    e.exit();
}

pub fn print_main_help() {
    println!(
        "{}",
        formatdoc! {"
        ana {VERSION}

        Usage: ana [command] [options]

        Commands:
          auth           Authentication commands
          bootstrap      Install the Anaconda CLI
          config         Show current configuration
          login          Log in to Anaconda
          logout         Log out from Anaconda
          whoami         Display information about the logged-in user
          self           Manage the ana installation

        Options:
          -V, --version  Print version
          -h, --help     Print help
        "}
    );
}

pub fn print_self_help() {
    println!(
        "{}",
        formatdoc! {"
        Manage the installation

        Usage: ana self <command> [options]

        Commands:
          update    Update ana to the latest version
        "}
    );
}

pub fn print_auth_help() {
    println!(
        "{}",
        formatdoc! {"
        Authentication commands

        Usage: ana auth <command> [options]

        Commands:
          api-key   Display the API key for the logged-in user
          login     Log in to Anaconda
          logout    Log out from Anaconda
          whoami    Display information about the logged-in user
        "}
    );
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
    Login,

    /// Log out from Anaconda
    Logout,

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
}
