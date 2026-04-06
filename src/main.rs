mod anaconda_cli;
mod auth;
mod cli;
mod config;
mod diagnostics;
mod http;
mod input;
mod paths;
mod qr;
mod tools;
mod update;

pub const VERSION: &str = env!("PKG_VERSION");

#[tokio::main]
async fn main() {
    let _diagnostics_guard = diagnostics::init();
    cli::execute().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_is_set() {
        assert!(!VERSION.is_empty());
    }
}
