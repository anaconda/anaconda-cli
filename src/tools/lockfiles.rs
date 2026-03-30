//! Embedded lockfiles for tool installation.

use std::path::PathBuf;

/// Tool configuration.
struct Tool {
    name: &'static str,
    lockfile: &'static str,
}

/// Embedded tool configurations.
const TOOLS: &[Tool] = &[Tool {
    name: "anaconda-cli",
    lockfile: include_str!("../../lockfiles/anaconda-cli/pixi.lock"),
}];

fn find_tool(name: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|t| t.name == name)
}

/// Returns the lockfile content for a tool.
///
/// If `ANA_LOCKFILES_DIR` is set, reads from that directory.
/// Otherwise, returns the embedded lockfile compiled into the binary.
pub fn content(name: &str) -> Option<String> {
    if let Ok(dir) = std::env::var("ANA_LOCKFILES_DIR") {
        let path = PathBuf::from(dir).join(name).join("pixi.lock");
        std::fs::read_to_string(&path).ok()
    } else {
        find_tool(name).map(|t| t.lockfile.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_embedded() {
        temp_env::with_var_unset("ANA_LOCKFILES_DIR", || {
            let lockfile = content("uv");
            assert!(lockfile.is_some());
            assert!(lockfile.unwrap().contains("version: 6"));
        });
    }

    #[test]
    fn test_content_unknown_tool() {
        temp_env::with_var_unset("ANA_LOCKFILES_DIR", || {
            assert!(content("unknown-tool").is_none());
        });
    }
}
