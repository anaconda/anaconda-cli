//! Diagnostics and error reporting via Sentry.
//!
//! This module is conditionally compiled based on the `diagnostics` feature flag.
//! When enabled, it initializes Sentry for error tracking. When disabled, all
//! functions become no-ops.

#[cfg(feature = "diagnostics")]
mod inner {
    use std::sync::Arc;

    use sentry::TransportOptions;
    use sentry::transports::ReqwestHttpTransportOptions;

    use crate::VERSION;
    use crate::config::Config;

    const SENTRY_DSN: &str = env!("SENTRY_DSN");
    const BUILD_TARGET: &str = env!("BUILD_TARGET");

    /// Guard that must be held for the lifetime of the program.
    pub type Guard = Option<sentry::ClientInitGuard>;

    /// Initialize the diagnostics system.
    ///
    /// Returns a guard that must be held for the lifetime of the program.
    /// The DSN is injected at build time; an empty string disables Sentry.
    /// Set ANA_SENTRY_DISABLED=1 at runtime to disable even when DSN is present.
    pub fn init(config: &Config) -> Guard {
        if config.sentry_disabled {
            return None;
        }

        let client = crate::http::configure_tls(reqwest::Client::builder())
            .build()
            .ok()?;
        let guard = sentry::init((
            SENTRY_DSN,
            sentry::ClientOptions::new()
                .release(VERSION)
                .environment(config.sentry_environment.clone())
                .send_default_pii(false)
                .attach_stacktrace(true)
                .transport(move |options: &sentry::ClientOptions| {
                    let options = TransportOptions::try_from_client_options(options)
                        .expect("Sentry transport requires a DSN");
                    Arc::new(
                        ReqwestHttpTransportOptions::from(options)
                            .with_client(client.clone())
                            .build(),
                    ) as Arc<dyn sentry::Transport>
                }),
        ));

        sentry::configure_scope(|scope| {
            scope.set_tag("os", std::env::consts::OS);
            scope.set_tag("arch", std::env::consts::ARCH);
            scope.set_tag("target", BUILD_TARGET);
        });

        Some(guard)
    }
}

#[cfg(not(feature = "diagnostics"))]
mod inner {
    use crate::config::Config;

    /// Guard is a no-op when diagnostics is disabled.
    pub type Guard = ();

    pub fn init(_config: &Config) -> Guard {}
}

pub use inner::*;
