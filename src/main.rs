mod cli;
mod config;
mod console;
mod update;

pub const VERSION: &str = env!("PKG_VERSION");

fn main() {
    match cli::parse() {
        cli::Action::ShowHelp => cli::print_main_help(),
        cli::Action::ShowSelfHelp => cli::print_self_help(),
        cli::Action::ShowVersion => println!("{}", VERSION),
        cli::Action::ShowConfig => println!("{}", config::Config::load()),
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
