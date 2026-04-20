//! Embedded lockfiles for tool installation.

use std::path::PathBuf;

/// Tool configuration.
struct Tool {
    name: &'static str,
    lockfile: &'static str,
    binaries: &'static [&'static [&'static str]],
}

/// Embedded tool configurations.
const TOOLS: &[Tool] = &[
    Tool {
        name: "anaconda-cli",
        lockfile: include_str!("../../tool-specs/anaconda-cli/pixi.lock"),
        binaries: if cfg![unix] {
            &[&["bin", "anaconda"]]
        } else {
            &[&["Scripts", "anaconda"]]
        },
    },
    Tool {
        name: "pixi",
        lockfile: include_str!("../../tool-specs/pixi/pixi.lock"),
        binaries: &[&["bin", "pixi"]],
    },
];

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

/// Returns the binaries to symlink for a tool.
pub fn binaries(name: &str) -> Option<Vec<PathBuf>> {
    find_tool(name).map(|t| t.binaries.iter().map(|b| b.iter().collect()).collect())
}

/// Returns all available tool names.
pub fn all_tools() -> Vec<&'static str> {
    TOOLS.iter().map(|t| t.name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_embedded() {
        temp_env::with_var_unset("ANA_LOCKFILES_DIR", || {
            let lockfile = content("anaconda-cli");
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
