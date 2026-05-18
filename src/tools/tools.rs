//! Embedded lockfiles for tool installation.

use std::path::PathBuf;

/// A binary to expose from a tool installation.
struct Binary {
    /// Path components to the binary within the tool prefix (e.g., ["bin", "anaconda"]).
    path: &'static [&'static str],
    /// Optional custom name for the symlink. If None, uses the binary filename.
    link_name: Option<&'static str>,
}

/// Tool configuration.
struct Tool {
    name: &'static str,
    lockfile: &'static str,
    binaries: &'static [Binary],
    /// If set, the tool is experimental and this message will be shown as a warning.
    experimental: Option<&'static str>,
}

/// Embedded tool configurations.
const TOOLS: &[Tool] = &[
    Tool {
        name: "anaconda-cli",
        lockfile: include_str!("../../tool-specs/anaconda-cli/pixi.lock"),
        binaries: if cfg![unix] {
            &[Binary {
                path: &["bin", "anaconda"],
                link_name: Some("anaconda-cli"),
            }]
        } else {
            &[Binary {
                path: &["Scripts", "anaconda"],
                link_name: Some("anaconda-cli"),
            }]
        },
        experimental: None,
    },
    #[cfg(unix)]
    Tool {
        name: "outerbounds",
        lockfile: include_str!("../../tool-specs/outerbounds/pixi.lock"),
        binaries: &[Binary {
            path: &["bin", "outerbounds"],
            link_name: None,
        }],
        experimental: Some("Outerbounds integration is an experimental alpha feature."),
    },
    Tool {
        name: "pixi",
        lockfile: include_str!("../../tool-specs/pixi/pixi.lock"),
        binaries: &[Binary {
            path: &["bin", "pixi"],
            link_name: None,
        }],
        experimental: None,
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

/// Information about a binary to link.
pub struct BinaryInfo {
    /// Path to the binary within the tool prefix.
    pub path: PathBuf,
    /// Name to use for the symlink (may differ from the binary filename).
    pub link_name: String,
}

/// Returns the binaries to link for a tool.
pub fn binaries(name: &str) -> Option<Vec<BinaryInfo>> {
    find_tool(name).map(|t| {
        t.binaries
            .iter()
            .map(|b| {
                let path: PathBuf = b.path.iter().collect();
                let link_name = b
                    .link_name
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| path.file_name().unwrap().to_string_lossy().to_string());
                BinaryInfo { path, link_name }
            })
            .collect()
    })
}

/// Returns the binary names (symlink names) to link for a tool.
pub fn binary_names(name: &str) -> Option<Vec<String>> {
    find_tool(name).map(|t| {
        t.binaries
            .iter()
            .map(|b| {
                b.link_name.map(|s| s.to_string()).unwrap_or_else(|| {
                    b.path.last().unwrap().to_string()
                })
            })
            .collect()
    })
}

/// Returns all available tool names.
pub fn all_tools() -> Vec<&'static str> {
    TOOLS.iter().map(|t| t.name).collect()
}

/// Returns the experimental warning message for a tool, if any.
pub fn experimental_message(name: &str) -> Option<&'static str> {
    find_tool(name).and_then(|t| t.experimental)
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
