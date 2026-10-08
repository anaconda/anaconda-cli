fn main() {
    // Set up cfg aliases for feature combinations
    cfg_aliases::cfg_aliases! {
        // self_update: enabled by default, disabled by conda-package
        self_update: { all(feature = "self-update", not(feature = "conda-package")) },
        // tool_install: enabled by default, disabled by conda-package
        tool_install: { all(feature = "tool-install", not(feature = "conda-package")) },
    }

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

    // On Windows, compile the shim binary and place it in OUT_DIR
    #[cfg(windows)]
    build_shim();
}

#[cfg(windows)]
fn build_shim() {
    use std::path::PathBuf;

    println!("cargo:rerun-if-env-changed=WINDOWS_SHIM_PATH");
    println!("cargo:rerun-if-changed=src/shim/shim.rs");

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let shim_out = out_dir.join("shim.exe");

    // If a pre-built shim is provided (e.g., signed), copy it instead of compiling
    if let Ok(shim_path) = std::env::var("WINDOWS_SHIM_PATH") {
        std::fs::copy(&shim_path, &shim_out).expect("failed to copy shim from WINDOWS_SHIM_PATH");
        return;
    }

    // Otherwise compile it directly
    use std::process::Command;

    let shim_src = PathBuf::from("src/shim/shim.rs");

    let status = Command::new("rustc")
        .args([
            "--edition=2024",
            "-O", // optimize for size
            "-o",
            shim_out.to_str().unwrap(),
            shim_src.to_str().unwrap(),
        ])
        .status()
        .expect("failed to run rustc for shim");

    if !status.success() {
        panic!("failed to compile shim binary");
    }
}
