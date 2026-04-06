//! User-agent string construction.
//!
//! Produces a static user-agent string compiled from build-time and runtime
//! platform information. The string is computed once and cached for the
//! lifetime of the process.
//!
//! Format: `ana/{version} {platform} rattler/{version}`
//!
//! Example (macOS): `ana/0.1.0 Darwin/25.2.0 OSX/26.2 rattler/0.40.3`
//! Example (Linux):  `ana/0.1.0 Linux/6.5.0 ubuntu/22.04 glibc/2.35 rattler/0.40.3`

mod platform;

use std::sync::LazyLock;

use crate::VERSION;

const RATTLER_VERSION: &str = env!("RATTLER_VERSION");

/// Cached user-agent string (computed once per process).
static USER_AGENT: LazyLock<String> = LazyLock::new(build_user_agent);

/// Return the user-agent string.
pub fn user_agent() -> &'static str {
    &USER_AGENT
}

fn build_user_agent() -> String {
    format!(
        "ana/{} {} rattler/{}",
        VERSION,
        platform::platform_string(),
        RATTLER_VERSION
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_agent_starts_with_ana() {
        let ua = user_agent();
        assert!(ua.starts_with("ana/"), "expected ana/ prefix, got: {}", ua);
    }

    #[test]
    fn test_user_agent_contains_rattler() {
        let ua = user_agent();
        assert!(
            ua.contains("rattler/"),
            "expected rattler/ in UA, got: {}",
            ua
        );
    }

    #[test]
    fn test_user_agent_print() {
        let ua = user_agent();
        eprintln!("User-Agent: {}", ua);
        assert!(!ua.is_empty());
    }
}
