//! Embedded lockfiles for tool installation.

use std::path::PathBuf;

/// Tool configuration for lockfile-based tools.
struct LockfileTool {
    name: &'static str,
    lockfile: &'static str,
    binaries: &'static [&'static [&'static str]],
    /// If set, the tool is experimental and this message will be shown as a warning.
    experimental: Option<&'static str>,
}

/// Embedded tool configurations (lockfile-based).
const LOCKFILE_TOOLS: &[LockfileTool] = &[
    LockfileTool {
        name: "anaconda-cli",
        lockfile: include_str!("../../tool-specs/anaconda-cli/pixi.lock"),
        // No symlink - anaconda-cli is only accessed via ana subcommands (e.g., ana mcp)
        // to avoid shadowing users' existing anaconda command from anaconda-auth
        binaries: &[],
        experimental: None,
    },
    #[cfg(unix)]
    LockfileTool {
        name: "outerbounds",
        lockfile: include_str!("../../tool-specs/outerbounds/pixi.lock"),
        binaries: &[&["bin", "outerbounds"]],
        experimental: Some("Outerbounds integration is an experimental alpha feature."),
    },
    LockfileTool {
        name: "pixi",
        lockfile: include_str!("../../tool-specs/pixi/pixi.lock"),
        binaries: &[&["bin", "pixi"]],
        experimental: None,
    },
];

/// Tools that use shell installers instead of lockfiles.
const INSTALLER_TOOLS: &[&str] = &["miniconda"];

fn find_lockfile_tool(name: &str) -> Option<&'static LockfileTool> {
    LOCKFILE_TOOLS.iter().find(|t| t.name == name)
}

/// Returns true if this tool uses a shell installer instead of a lockfile.
pub fn is_installer_tool(name: &str) -> bool {
    INSTALLER_TOOLS.contains(&name)
}

/// Returns the lockfile content for a tool.
///
/// If `ANA_LOCKFILES_DIR` is set, reads from that directory.
/// Otherwise, returns the embedded lockfile compiled into the binary.
/// Returns None for installer-based tools (like miniconda).
pub fn content(name: &str) -> Option<String> {
    if is_installer_tool(name) {
        return None;
    }
    if let Ok(dir) = std::env::var("ANA_LOCKFILES_DIR") {
        let path = PathBuf::from(dir).join(name).join("pixi.lock");
        std::fs::read_to_string(&path).ok()
    } else {
        find_lockfile_tool(name).map(|t| t.lockfile.to_string())
    }
}

/// Returns the binaries to link for a tool.
pub fn binaries(name: &str) -> Option<Vec<PathBuf>> {
    if name == "miniconda" {
        return Some(super::miniconda::binaries());
    }
    find_lockfile_tool(name).map(|t| t.binaries.iter().map(|b| b.iter().collect()).collect())
}

/// Returns the binary names to link for a tool.
pub fn binary_names(name: &str) -> Option<Vec<&'static str>> {
    if name == "miniconda" {
        return Some(vec!["conda"]);
    }
    find_lockfile_tool(name).map(|t| {
        t.binaries
            .iter()
            .filter_map(|b| b.last().copied())
            .collect()
    })
}

/// Returns all available tool names.
pub fn all_tools() -> Vec<&'static str> {
    let mut tools: Vec<&'static str> = LOCKFILE_TOOLS.iter().map(|t| t.name).collect();
    tools.extend(INSTALLER_TOOLS.iter().copied());
    tools.sort();
    tools
}

/// Returns the experimental warning message for a tool, if any.
pub fn experimental_message(name: &str) -> Option<&'static str> {
    find_lockfile_tool(name).and_then(|t| t.experimental)
}

/// Returns true if this is a known tool (either lockfile-based or installer-based).
pub fn is_known_tool(name: &str) -> bool {
    find_lockfile_tool(name).is_some() || is_installer_tool(name)
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

    #[test]
    fn test_miniconda_is_installer_tool() {
        assert!(is_installer_tool("miniconda"));
        assert!(!is_installer_tool("pixi"));
        assert!(!is_installer_tool("unknown"));
    }

    #[test]
    fn test_miniconda_has_no_lockfile_content() {
        assert!(content("miniconda").is_none());
    }

    #[test]
    fn test_miniconda_binaries() {
        let binaries = binaries("miniconda");
        assert!(binaries.is_some());
        let binaries = binaries.unwrap();
        assert!(!binaries.is_empty());
    }

    #[test]
    fn test_miniconda_binary_names() {
        let names = binary_names("miniconda");
        assert!(names.is_some());
        assert!(names.unwrap().contains(&"conda"));
    }

    #[test]
    fn test_miniconda_in_all_tools() {
        let tools = all_tools();
        assert!(tools.contains(&"miniconda"));
    }

    #[test]
    fn test_miniconda_is_known_tool() {
        assert!(is_known_tool("miniconda"));
        assert!(is_known_tool("pixi"));
        assert!(!is_known_tool("unknown-tool"));
    }
}
