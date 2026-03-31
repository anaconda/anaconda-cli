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

#[cfg(feature = "diagnostics")]
const SENTRY_DSN: &str = "https://c3c5f0c3bc1590e4a439af529d0bec39@o4506633492365312.ingest.us.sentry.io/4511137151385600";

#[tokio::main]
async fn main() {
    // Initialize Sentry - guard must be held for the lifetime of the program
    #[cfg(feature = "diagnostics")]
    let _sentry_guard = sentry::init((
        SENTRY_DSN,
        sentry::ClientOptions {
            release: Some(VERSION.into()),
            environment: Some(
                std::env::var("ANA_ENV")
                    .unwrap_or_else(|_| "production".to_string())
                    .into(),
            ),
            ..Default::default()
        },
    ));

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
