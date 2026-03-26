mod cli;
mod update;

use cli::{print_main_help, print_self_help, Commands, SelfCommands};

pub const VERSION: &str = env!("PKG_VERSION");

fn main() {
    // Handle custom error messages for unknown commands
    let result = cli::parse();

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
                            update::show_available_versions(VERSION);
                        } else {
                            update::run_update(VERSION, yes);
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

    #[test]
    fn test_version_is_set() {
        assert!(!VERSION.is_empty());
    }
}
