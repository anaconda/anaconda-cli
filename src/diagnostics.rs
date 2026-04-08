//! Diagnostics and error reporting via Sentry.
//!
//! This module is conditionally compiled based on the `diagnostics` feature flag.
//! When enabled, it initializes Sentry for error tracking. When disabled, all
//! functions become no-ops.

#[cfg(feature = "diagnostics")]
mod inner {
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
    pub fn init() -> Guard {
        let config = Config::load();

        if config.sentry_disabled {
            return None;
        }

        let guard = sentry::init((
            SENTRY_DSN,
            sentry::ClientOptions {
                release: Some(VERSION.into()),
                environment: Some(config.sentry_environment.into()),
                send_default_pii: false,
                attach_stacktrace: true,
                ..Default::default()
            },
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
    /// Guard is a no-op when diagnostics is disabled.
    pub type Guard = ();

    pub fn init() -> Guard {}
}

pub use inner::*;
