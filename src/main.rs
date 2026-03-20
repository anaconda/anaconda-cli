mod update;

use indoc::formatdoc;

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

fn run_self_update() {
    let releases = match update::fetch_available_releases() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to fetch releases: {}", e);
            return;
        }
    };

    let latest = match releases.first() {
        Some(r) => r,
        None => {
            println!("No releases available.");
            return;
        }
    };

    let current = match update::parse_version(VERSION) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed to parse current version: {}", e);
            return;
        }
    };

    let latest_version = match update::parse_version(&latest.tag_name) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed to parse latest version: {}", e);
            return;
        }
    };

    if latest_version <= current {
        println!("Already up to date ({})", VERSION);
        return;
    }

    let asset = match update::get_asset_for_platform(latest) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Failed to find asset: {}", e);
            return;
        }
    };

    println!("Update available: {} -> {}", VERSION, latest.tag_name);
    println!("Downloading {}...", asset.name);

    match update::download_asset(asset) {
        Ok(path) => println!("Downloaded to: {}", path.display()),
        Err(e) => eprintln!("Failed to download: {}", e),
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
    SelfUpdate,
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
                        Ok(Command::SelfUpdate)
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
        Command::SelfUpdate => run_self_update(),
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
            Ok(Command::SelfUpdate)
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
}
