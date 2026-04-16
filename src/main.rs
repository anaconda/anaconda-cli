mod anaconda_cli;
mod auth;
mod cli;
mod config;
mod diagnostics;
#[cfg(feature = "feedback")]
mod feedback;
mod help;
mod http;
mod input;
mod paths;
mod qr;
mod table;
mod tools;
mod ua;
mod update;

pub const VERSION: &str = env!("PKG_VERSION");
#[cfg(feature = "feedback")]
pub const FEEDBACK_BASE_URL: &str = "https://docs.google.com/forms/d/e/1FAIpQLSeGd9p7pQSHvjIc6RNShjTQCGmM-5_3xkPNpNfYk102-HZB8Q/viewform";

#[tokio::main]
async fn main() {
    // Check if we're being invoked as "conda" (via symlink or hardlink)
    if tools::conda_wrapper::is_conda_invocation() {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let code = tools::conda_wrapper::run(&args);
        std::process::exit(code);
    }

    let config = config::Config::load();
    let _diagnostics_guard = diagnostics::init(&config);
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
