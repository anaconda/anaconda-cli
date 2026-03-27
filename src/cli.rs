use clap::{Parser, Subcommand};
use indoc::formatdoc;

use crate::VERSION;

/// Action to be performed, returned by parse()
pub enum Action {
    ShowHelp,
    ShowSelfHelp,
    ShowAuthHelp,
    ShowVersion,
    ShowConfig,
    Login,
    Logout,
    Whoami,
    ShowApiKey,
    Update { force: bool },
    CheckForUpdate,
    ShowAvailableVersions,
}

/// Parse CLI arguments and return the action to perform.
/// Exits the process on unrecoverable errors (unknown commands, etc.)
pub fn parse() -> Action {
    match Cli::try_parse() {
        Ok(cli) => match cli.command {
            None => Action::ShowHelp,
            Some(Commands::Config) => Action::ShowConfig,
            Some(Commands::Login) => Action::Login,
            Some(Commands::Logout) => Action::Logout,
            Some(Commands::Whoami) => Action::Whoami,
            Some(Commands::Auth { command }) => match command {
                None => Action::ShowAuthHelp,
                Some(AuthCommands::Login) => Action::Login,
                Some(AuthCommands::Logout) => Action::Logout,
                Some(AuthCommands::Whoami) => Action::Whoami,
                Some(AuthCommands::ApiKey) => Action::ShowApiKey,
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
