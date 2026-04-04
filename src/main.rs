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
const SENTRY_DSN: &str = env!("SENTRY_DSN");
#[cfg(feature = "diagnostics")]
const BUILD_TARGET: &str = env!("BUILD_TARGET");

#[tokio::main]
async fn main() {
    // Initialize Sentry - guard must be held for the lifetime of the program
    // DSN is injected at build time; empty string disables Sentry
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
            send_default_pii: false,
            attach_stacktrace: true,
            ..Default::default()
        },
    ));

    #[cfg(feature = "diagnostics")]
    sentry::configure_scope(|scope| {
        scope.set_tag("os", std::env::consts::OS);
        scope.set_tag("arch", std::env::consts::ARCH);
        scope.set_tag("target", BUILD_TARGET);
    });

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
