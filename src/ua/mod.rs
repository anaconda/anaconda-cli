//! User-agent string construction.
//!
//! Produces a static user-agent string compiled from build-time and runtime
//! platform information. The string is computed once and cached for the
//! lifetime of the process.
//!
//! Format: `ana/{version} {kernel}/{release} {os}/{version} rattler/{version}`
//!
//! Example (macOS): `ana/0.1.0 Darwin/25.2.0 OSX/26.2 rattler/0.40.3`

mod platform;

use std::sync::LazyLock;

use crate::VERSION;

/// Cached user-agent string (computed once per process).
static USER_AGENT: LazyLock<String> = LazyLock::new(build_user_agent);

/// Return the user-agent string.
pub fn user_agent() -> &'static str {
    &USER_AGENT
}

fn build_user_agent() -> String {
    format!("ana/{} {}", VERSION, platform::platform_string())
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
}
