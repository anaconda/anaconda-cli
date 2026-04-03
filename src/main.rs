mod anaconda_cli;
mod auth;
mod cli;
mod config;
mod input;
mod paths;
mod qr;
mod tools;
mod update;

pub const VERSION: &str = env!("PKG_VERSION");

fn main() {
    cli::execute();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_is_set() {
        assert!(!VERSION.is_empty());
    }
}
