use std::collections::HashMap;

use anaconda_otel_rs::signals::increment_counter;
use clap::{Parser, Subcommand};
use indoc::formatdoc;

use crate::VERSION;
use crate::config::Config;
use crate::update;

/// Action to be performed, returned by parse()
pub enum Action {
    ShowHelp,
    ShowSelfHelp,
    ShowVersion,
    ShowConfig,
    Update { force: bool },
    CheckForUpdate,
    ShowAvailableVersions,
}

impl Action {
    pub fn name(&self) -> &'static str {
        match self {
            Action::ShowHelp => "help",
            Action::ShowSelfHelp => "self.help",
            Action::ShowVersion => "version",
            Action::ShowConfig => "config",
            Action::Update { .. } => "self.update",
            Action::CheckForUpdate => "self.update.check",
            Action::ShowAvailableVersions => "self.update.list",
        }
    }

    /// Execute the action with telemetry middleware
    pub fn execute(self) -> Result<(), Box<dyn std::error::Error>> {
        let name = self.name();
        let mut attrs = HashMap::new();
        attrs.insert("command".to_string(), name.into());
        increment_counter("cli.command.invoked", 1, attrs.clone());

        let result = self.run();

        match &result {
            Ok(_) => {
                increment_counter("cli.command.success", 1, attrs);
            }
            Err(_) => {
                increment_counter("cli.command.failure", 1, attrs);
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
            Action::ShowVersion => {
                println!("{}", VERSION);
                Ok(())
            }
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
            Action::ShowConfig => {
                Config::load().print_table();
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
            Some(Commands::Config) => Action::ShowConfig,
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
          config         Show current configuration
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
    /// Show current configuration
    Config,

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
