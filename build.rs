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

    // Extract rattler version from Cargo.lock for the user-agent string
    println!("cargo:rerun-if-changed=Cargo.lock");
    let rattler_version =
        extract_dep_version("Cargo.lock", "rattler").unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=RATTLER_VERSION={}", rattler_version);
}

/// Extract a dependency's resolved version from Cargo.lock.
fn extract_dep_version(lock_path: &str, dep_name: &str) -> Option<String> {
    let content = std::fs::read_to_string(lock_path).ok()?;
    let mut found_name = false;
    for line in content.lines() {
        if line.starts_with("name = ") && line.contains(&format!("\"{}\"", dep_name)) {
            found_name = true;
        } else if found_name && line.starts_with("version = ") {
            return line.split('"').nth(1).map(|s| s.to_string());
        }
    }
    None
}
