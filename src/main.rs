const APPLICATION: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("PKG_VERSION");

fn greeting() -> &'static str {
    "Hello, world!"
}

fn print_usage() {
    println!("{} {}", APPLICATION, VERSION);
    println!();
    println!("Usage: {} [command] [options]", APPLICATION);
    println!();
    println!("Commands:");
    println!("  self           Manage the ana installation");
    println!();
    println!("Options:");
    println!("  -V, --version  Print version");
    println!("  -h, --help     Print help");
}

fn print_self_usage() {
    println!("Manage the ana installation");
    println!();
    println!("Usage: {} self <command> [options]", APPLICATION);
    println!();
    println!("Commands:");
    println!("  update    Update ana to the latest version");
}

fn run_self_update() {
    print!("Running the update!")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{}", VERSION);
        return;
    }

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage();
        return;
    }

    if args.len() > 1 {
        match args[1].as_str() {
            "self" => {
                if args.len() < 3 {
                    print_self_usage();
                    return;
                }
                match args[2].as_str() {
                    "update" => {
                        run_self_update();
                        return;
                    }
                    cmd => {
                        eprintln!("Unknown self command: {}", cmd);
                        std::process::exit(1);
                    }
                }
            }
            cmd => {
                eprintln!("Unknown command: {}", cmd);
                std::process::exit(1);
            }
        }
    }

    println!("{}", greeting());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_greeting() {
        assert_eq!(greeting(), "Hello, world!");
    }

    #[test]
    fn test_version_is_set() {
        assert!(!VERSION.is_empty());
    }
}
