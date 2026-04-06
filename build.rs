fn main() {
    // Re-run build.rs if these env vars change
    println!("cargo:rerun-if-env-changed=PKG_VERSION");
    println!("cargo:rerun-if-env-changed=SENTRY_DSN");

    // Version is passed via PKG_VERSION environment variable.
    // This should be set by CI or locally via: PKG_VERSION=$(python scripts/get_version.py)
    // Falls back to CARGO_PKG_VERSION if not set.
    let version =
        std::env::var("PKG_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());
    println!("cargo:rustc-env=PKG_VERSION={}", version);

    // Sentry DSN is injected at build time. If not set, defaults to empty string
    // which will disable Sentry at runtime.
    let sentry_dsn = std::env::var("SENTRY_DSN").unwrap_or_default();
    println!("cargo:rustc-env=SENTRY_DSN={}", sentry_dsn);

    // Expose build target info for Sentry tags
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=BUILD_TARGET={}", target);
}
