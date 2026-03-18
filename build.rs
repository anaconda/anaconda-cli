fn main() {
    // Version is passed via PKG_VERSION environment variable.
    // This should be set by CI or locally via: PKG_VERSION=$(python scripts/get_version.py)
    // Falls back to CARGO_PKG_VERSION if not set.
    let version = std::env::var("PKG_VERSION")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());

    println!("cargo:rustc-env=PKG_VERSION={}", version);
}
