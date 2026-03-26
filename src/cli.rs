use clap::{Parser, Subcommand};
use indoc::formatdoc;

use crate::VERSION;

pub fn parse() -> Result<Cli, clap::Error> {
    Cli::try_parse()
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
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
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
pub enum SelfCommands {
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

pub fn print_main_help() {
    println!(
        "{}",
        formatdoc! {"
        ana {VERSION}

        Usage: ana [command] [options]

        Commands:
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
