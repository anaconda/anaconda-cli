mod auth;
mod cli;
mod config;
mod update;

pub const VERSION: &str = env!("PKG_VERSION");

fn main() {
    match cli::parse() {
        cli::Action::ShowHelp => cli::print_main_help(),
        cli::Action::ShowSelfHelp => cli::print_self_help(),
        cli::Action::ShowAuthHelp => cli::print_auth_help(),
        cli::Action::ShowVersion => println!("{}", VERSION),
        cli::Action::ShowConfig => config::Config::load().print_table(),
        cli::Action::Login => {
            if let Err(e) = auth::login() {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        cli::Action::Logout => {
            if let Err(e) = auth::logout() {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        cli::Action::Update { force } => update::run_update(VERSION, force),
        cli::Action::CheckForUpdate => update::check_for_update(VERSION),
        cli::Action::ShowAvailableVersions => update::show_available_versions(VERSION),
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
