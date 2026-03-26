mod update;

use clap::{Parser, Subcommand};
use indoc::formatdoc;
use std::io::{self, Write};

const APPLICATION: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("PKG_VERSION");

#[derive(Parser)]
#[command(
    name = APPLICATION,
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

fn print_main_help() {
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

fn print_self_help() {
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
fn prompt_yes_no(message: &str) -> bool {
    print!("{} [y/N] ", message);
    io::stdout().flush().unwrap();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return false;
    }

    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}

fn run_self_update(force: bool) {
    let check = match update::check_update(VERSION) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to check for updates: {}", e);
            return;
        }
    };

    match check {
        update::UpdateCheck::Available(release) => {
            if !force {
                let message = format!("Update {} -> {}?", VERSION, release.tag_name);
                if !prompt_yes_no(&message) {
                    println!("Update cancelled.");
                    return;
                }
            }
            match update::apply_update(&release) {
                Ok(()) => println!("Updated successfully: {} -> {}", VERSION, release.tag_name),
                Err(e) => eprintln!("Failed to update: {}", e),
            }
        }
        update::UpdateCheck::AlreadyUpToDate => {
            println!("Already up to date ({})", VERSION);
        }
        update::UpdateCheck::NoReleases => {
            println!("No releases available.");
        }
    }
}

fn show_available_versions() {
    let releases = match update::fetch_available_releases() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to fetch releases: {}", e);
            return;
        }
    };

    if releases.is_empty() {
        println!("No releases available.");
        return;
    }

    let current_tag = format!("v{}", VERSION);
    for release in releases {
        let marker = if release.tag_name == current_tag {
            " *"
        } else {
            ""
        };
        println!("{}{}", release.tag_name, marker);
    }
}

fn main() {
    // Handle custom error messages for unknown commands
    let result = Cli::try_parse();

    match result {
        Ok(cli) => {
            match cli.command {
                None => {
                    // No command provided - show help
                    print_main_help();
                }
                Some(Commands::Self_ { command }) => match command {
                    None => {
                        // `ana self` with no subcommand - show self help
                        print_self_help();
                    }
                    Some(SelfCommands::Update { yes, check, list }) => {
                        if check {
                            update::check_for_update(VERSION);
                        } else if list {
                            show_available_versions();
                        } else {
                            run_self_update(yes);
                        }
                    }
                },
            }
        }
        Err(e) => {
            // Check if it's a help or version request
            if e.kind() == clap::error::ErrorKind::DisplayHelp {
                print_main_help();
                return;
            }
            if e.kind() == clap::error::ErrorKind::DisplayVersion {
                println!("{}", VERSION);
                return;
            }

            // Handle unknown subcommand errors with custom format
            let err_str = e.to_string();
            if err_str.contains("unrecognized subcommand") {
                // Extract the unknown command name
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_version_is_set() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_cli_parses() {
        // Verify clap setup is valid
        Cli::command().debug_assert();
    }
}
