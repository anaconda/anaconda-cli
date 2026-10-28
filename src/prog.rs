//! The name users type to invoke ana.
//!
//! ana is normally run as `ana`, but the kilo wrapper (`src/wrappers/kilo.rs`)
//! runs it as `kilo ana` and sets `ANA_PROG_NAME` accordingly, so usage lines
//! and command hints match what the user typed.

use std::sync::LazyLock;

const DEFAULT_NAME: &str = "ana";

static NAME: LazyLock<String> = LazyLock::new(|| from_env(std::env::var("ANA_PROG_NAME").ok()));

fn from_env(value: Option<String>) -> String {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_NAME.to_string())
}

/// The program name, e.g. `ana` or `kilo ana`.
pub fn name() -> &'static str {
    &NAME
}

/// A full command line for `args`, e.g. `cmd("login")` → `ana login`.
pub fn cmd(args: &str) -> String {
    format!("{} {}", name(), args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_env_defaults_to_ana() {
        assert_eq!(from_env(None), "ana");
        assert_eq!(from_env(Some("  ".to_string())), "ana");
    }

    #[test]
    fn test_from_env_uses_override() {
        assert_eq!(from_env(Some("kilo ana".to_string())), "kilo ana");
    }
}
