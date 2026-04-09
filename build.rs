use std::process::Command;

// Include the version parsing logic shared with the main crate's test suite.
// build.rs cannot depend on the crate itself, so we use include!() to share code.
include!("src/version.rs");

fn main() {
    // Re-run build.rs if these env vars change
    println!("cargo:rerun-if-env-changed=PKG_VERSION");
    println!("cargo:rerun-if-env-changed=SENTRY_DSN");

    // Re-run when git state changes (new commits, branch switches, tags)
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/packed-refs");
    if let Ok(head) = std::fs::read_to_string(".git/HEAD") {
        if let Some(ref_path) = head.trim().strip_prefix("ref: ") {
            println!("cargo:rerun-if-changed=.git/{}", ref_path);
        }
    }

    // Version resolution: PKG_VERSION env > git describe > CARGO_PKG_VERSION
    let version = resolve_version();
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

/// Resolve the package version from the best available source.
///
/// Priority: PKG_VERSION env var > git describe > CARGO_PKG_VERSION (0.0.0)
fn resolve_version() -> String {
    // 1. Explicit override via environment variable
    if let Ok(v) = std::env::var("PKG_VERSION") {
        if !v.is_empty() {
            return v;
        }
    }

    // 2. Derive from git tags using the same describe command as setuptools-scm
    if let Some(v) = version_from_git() {
        return v;
    }

    // 3. Fallback to Cargo.toml version (0.0.0)
    env!("CARGO_PKG_VERSION").to_string()
}

/// Run `git describe` and parse the output into a PEP 440 version string.
fn version_from_git() -> Option<String> {
    let output = Command::new("git")
        .args([
            "describe",
            "--dirty",
            "--tags",
            "--long",
            "--match",
            "v[0-9]*.[0-9]*.[0-9]*",
            "--exclude",
            "v*.dev*",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let desc = String::from_utf8(output.stdout).ok()?.trim().to_string();
    parse_git_describe(&desc)
}

/// Extract a dependency's resolved version from Cargo.lock.
fn extract_dep_version(lock_path: &str, dep_name: &str) -> Option<String> {
    let lockfile = cargo_lock::Lockfile::load(lock_path).ok()?;
    lockfile
        .packages
        .iter()
        .find(|p| p.name.as_str() == dep_name)
        .map(|p| p.version.to_string())
}
