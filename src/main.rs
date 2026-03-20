mod update;

use indoc::formatdoc;
use std::io::{self, Write};

const APPLICATION: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("PKG_VERSION");

fn usage() -> String {
    formatdoc! {"
        {APPLICATION} {VERSION}

        Usage: {APPLICATION} [command] [options]

        Commands:
          self           Manage the {APPLICATION} installation

        Options:
          -V, --version  Print version
          -h, --help     Print help"
    }
}

fn self_usage() -> String {
    formatdoc! {"
        Manage the installation

        Usage: {APPLICATION} self <command> [options]

        Commands:
          update    Update {APPLICATION} to the latest version"
    }
}

fn print_usage() {
    println!("{}", usage());
}

fn print_self_usage() {
    println!("{}", self_usage());
}

fn print_version() {
    println!("{}", VERSION);
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
        update::UpdateCheck::Available { from, release } => {
            if !force {
                let message = format!("Update {} -> {}?", from, release.tag_name);
                if !prompt_yes_no(&message) {
                    println!("Update cancelled.");
                    return;
                }
            }
            match update::apply_update(&release) {
                Ok(()) => println!("Updated successfully: {} -> {}", from, release.tag_name),
                Err(e) => eprintln!("Failed to update: {}", e),
            }
        }
        update::UpdateCheck::AlreadyUpToDate(v) => {
            println!("Already up to date ({})", v);
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

#[derive(Debug)]
enum Command {
    Help,
    SelfHelp,
    Version,
    SelfUpdate { force: bool },
    SelfUpdateCheck,
    SelfShowAvailable,
}

fn parse_args(args: &[String]) -> Result<Command, String> {
    // Display help
    if args.len() <= 1 || args.iter().any(|a| a == "--help" || a == "-h") {
        return Ok(Command::Help);
    }

    // Display version
    if args.iter().any(|a| a == "--version" || a == "-V") {
        return Ok(Command::Version);
    }

    // Handle subcommands
    match args[1].as_str() {
        "self" => {
            if args.len() < 3 {
                return Ok(Command::SelfHelp);
            }
            match args[2].as_str() {
                "update" => {
                    if args.iter().any(|a| a == "--show-available") {
                        Ok(Command::SelfShowAvailable)
                    } else if args.iter().any(|a| a == "--check") {
                        Ok(Command::SelfUpdateCheck)
                    } else {
                        let force = args.iter().any(|a| a == "--yes" || a == "-y");
                        Ok(Command::SelfUpdate { force })
                    }
                }
                cmd => Err(format!("Unknown self command: {}", cmd)),
            }
        }
        cmd => Err(format!("Unknown command: {}", cmd)),
    }
}

fn run(args: &[String]) -> Result<(), String> {
    match parse_args(args)? {
        Command::Help => print_usage(),
        Command::SelfHelp => print_self_usage(),
        Command::Version => print_version(),
        Command::SelfUpdate { force } => run_self_update(force),
        Command::SelfUpdateCheck => update::check_for_update(VERSION),
        Command::SelfShowAvailable => show_available_versions(),
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if let Err(msg) = run(&args) {
        eprintln!("{}", msg);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(input: &[&str]) -> Vec<String> {
        input.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_version_is_set() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_no_args_parses_to_help() {
        assert!(matches!(parse_args(&args(&["ana"])), Ok(Command::Help)));
    }

    #[test]
    fn test_help_flag() {
        assert!(matches!(
            parse_args(&args(&["ana", "--help"])),
            Ok(Command::Help)
        ));
    }

    #[test]
    fn test_help_flag_short() {
        assert!(matches!(
            parse_args(&args(&["ana", "-h"])),
            Ok(Command::Help)
        ));
    }

    #[test]
    fn test_version_flag() {
        assert!(matches!(
            parse_args(&args(&["ana", "--version"])),
            Ok(Command::Version)
        ));
    }

    #[test]
    fn test_version_flag_short() {
        assert!(matches!(
            parse_args(&args(&["ana", "-V"])),
            Ok(Command::Version)
        ));
    }

    #[test]
    fn test_self_no_subcommand() {
        assert!(matches!(
            parse_args(&args(&["ana", "self"])),
            Ok(Command::SelfHelp)
        ));
    }

    #[test]
    fn test_self_update() {
        assert!(matches!(
            parse_args(&args(&["ana", "self", "update"])),
            Ok(Command::SelfUpdate { force: false })
        ));
    }

    #[test]
    fn test_self_update_yes() {
        assert!(matches!(
            parse_args(&args(&["ana", "self", "update", "--yes"])),
            Ok(Command::SelfUpdate { force: true })
        ));
    }

    #[test]
    fn test_self_update_yes_short() {
        assert!(matches!(
            parse_args(&args(&["ana", "self", "update", "-y"])),
            Ok(Command::SelfUpdate { force: true })
        ));
    }

    #[test]
    fn test_unknown_command() {
        let result = parse_args(&args(&["ana", "foo"]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown command: foo"));
    }

    #[test]
    fn test_self_update_show_available() {
        assert!(matches!(
            parse_args(&args(&["ana", "self", "update", "--show-available"])),
            Ok(Command::SelfShowAvailable)
        ));
    }

    #[test]
    fn test_self_update_check() {
        assert!(matches!(
            parse_args(&args(&["ana", "self", "update", "--check"])),
            Ok(Command::SelfUpdateCheck)
        ));
    }

    #[test]
    fn test_unknown_self_command() {
        let result = parse_args(&args(&["ana", "self", "unknown"]));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Unknown self command: unknown")
        );
    }
}
