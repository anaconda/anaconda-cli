//! Managed tool definitions: embedded lockfiles or install-time solves.

use std::path::PathBuf;

/// Where a tool's package set comes from.
#[cfg_attr(not(tool_install), allow(dead_code))]
pub enum Source {
    /// Installed from a lockfile embedded in the `ana` binary. New tool
    /// versions ship with new `ana` releases.
    Locked {
        lockfile: &'static str,
        /// Whether this tool should be auto-updated when `ana` is updated.
        auto_update: bool,
    },
    /// Resolved against `channels` whenever the tool is installed or
    /// updated, so new tool versions ship independently of `ana`.
    Solved {
        /// Channels in strict priority order.
        channels: &'static [&'static str],
        /// Package names to install. These always float to the newest
        /// version; other installed packages are kept when still compatible.
        packages: &'static [&'static str],
    },
}

/// Tool configuration.
struct Tool {
    name: &'static str,
    #[cfg_attr(not(tool_install), allow(dead_code))]
    source: Source,
    binaries: &'static [&'static [&'static str]],
    #[cfg_attr(not(tool_install), allow(dead_code))]
    experimental: Option<&'static str>,
}

/// Managed tool configurations.
const TOOLS: &[Tool] = &[
    Tool {
        name: "anaconda-cli",
        source: Source::Locked {
            lockfile: include_str!("../../tool-specs/anaconda-cli/pixi.lock"),
            auto_update: true,
        },
        // No symlink - anaconda-cli is only accessed via ana subcommands (e.g., ana mcp)
        // to avoid shadowing users' existing anaconda command from anaconda-auth
        binaries: &[],
        experimental: None,
    },
    Tool {
        name: "kilo-cli",
        source: Source::Solved {
            channels: &[
                "https://conda.anaconda.org/anaconda-cloud/label/rc",
                "https://repo.anaconda.com/pkgs/main",
            ],
            packages: &["kilo-cli"],
        },
        // Link the native binary, not the package's `bin/kilo` wrapper: the
        // wrapper locates `../lib/kilo` relative to its own unresolved path,
        // so it breaks when invoked through a symlink in ~/.ana/bin.
        binaries: &[&["lib", "kilo", "kilo"]],
        experimental: Some(
            "kilo-cli is a release candidate from the anaconda-cloud channel's rc label.",
        ),
    },
    #[cfg(unix)]
    Tool {
        name: "outerbounds",
        source: Source::Locked {
            lockfile: include_str!("../../tool-specs/outerbounds/pixi.lock"),
            auto_update: true,
        },
        binaries: &[&["bin", "outerbounds"]],
        experimental: None,
    },
    Tool {
        name: "pixi",
        source: Source::Locked {
            lockfile: include_str!("../../tool-specs/pixi/pixi.lock"),
            auto_update: false,
        },
        binaries: &[&["bin", "pixi"]],
        experimental: None,
    },
];

fn find_tool(name: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|t| t.name == name)
}

/// Returns where a tool's packages come from.
#[cfg_attr(not(tool_install), allow(dead_code))]
pub fn source(name: &str) -> Option<&'static Source> {
    find_tool(name).map(|t| &t.source)
}

/// Returns whether a tool is resolved at install time rather than locked.
#[cfg_attr(not(tool_install), allow(dead_code))]
pub fn is_solved(name: &str) -> bool {
    matches!(source(name), Some(Source::Solved { .. }))
}

/// Returns the lockfile content for a locked tool.
///
/// If `ANA_LOCKFILES_DIR` is set, reads from that directory.
/// Otherwise, returns the embedded lockfile compiled into the binary.
#[cfg_attr(not(tool_install), allow(dead_code))]
pub fn content(name: &str) -> Option<String> {
    if let Ok(dir) = std::env::var("ANA_LOCKFILES_DIR") {
        let path = PathBuf::from(dir).join(name).join("pixi.lock");
        std::fs::read_to_string(&path).ok()
    } else {
        match source(name)? {
            Source::Locked { lockfile, .. } => Some(lockfile.to_string()),
            Source::Solved { .. } => None,
        }
    }
}

/// Returns the channels to solve a solved tool against.
///
/// If `ANA_TOOL_CHANNELS` is set (comma-separated), it replaces the tool's
/// channels, e.g. to point at a local `file://` channel for testing.
#[cfg_attr(not(tool_install), allow(dead_code))]
pub fn solve_channels(defaults: &[&str]) -> Vec<String> {
    let overridden: Vec<String> = std::env::var("ANA_TOOL_CHANNELS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(String::from)
        .collect();
    if overridden.is_empty() {
        defaults.iter().map(|c| c.to_string()).collect()
    } else {
        overridden
    }
}

/// Returns the binaries to link for a tool.
pub fn binaries(name: &str) -> Option<Vec<PathBuf>> {
    find_tool(name).map(|t| t.binaries.iter().map(|b| b.iter().collect()).collect())
}

/// Returns the binary names to link for a tool.
pub fn binary_names(name: &str) -> Option<Vec<&'static str>> {
    find_tool(name).map(|t| {
        t.binaries
            .iter()
            .filter_map(|b| b.last().copied())
            .collect()
    })
}

/// Returns all available tool names.
pub fn all_tools() -> Vec<&'static str> {
    TOOLS.iter().map(|t| t.name).collect()
}

/// Returns the experimental warning message for a tool, if any.
#[cfg_attr(not(tool_install), allow(dead_code))]
pub fn experimental_message(name: &str) -> Option<&'static str> {
    find_tool(name).and_then(|t| t.experimental)
}

/// Returns whether auto-update is enabled for a locked tool by default.
/// Solved tools are never refreshed by `ana self update`.
#[cfg_attr(not(tool_install), allow(dead_code))]
pub fn auto_update_default(name: &str) -> bool {
    matches!(
        source(name),
        Some(Source::Locked {
            auto_update: true,
            ..
        })
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial(env)]
    fn test_content_embedded() {
        temp_env::with_var_unset("ANA_LOCKFILES_DIR", || {
            let lockfile = content("anaconda-cli");
            assert!(lockfile.is_some());
            assert!(lockfile.unwrap().starts_with("version: "));
        });
    }

    #[test]
    #[serial(env)]
    fn test_content_unknown_tool() {
        temp_env::with_var_unset("ANA_LOCKFILES_DIR", || {
            assert!(content("unknown-tool").is_none());
        });
    }

    #[test]
    fn test_auto_update_default_anaconda_cli() {
        assert!(auto_update_default("anaconda-cli"));
    }

    #[test]
    fn test_auto_update_default_pixi() {
        assert!(!auto_update_default("pixi"));
    }

    #[test]
    fn test_auto_update_default_unknown_tool() {
        assert!(!auto_update_default("unknown-tool"));
    }
}
