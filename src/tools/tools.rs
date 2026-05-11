//! Embedded lockfiles for tool installation.

use std::path::PathBuf;

/// Tool configuration.
struct Tool {
    name: &'static str,
    lockfile: &'static str,
    binaries: &'static [&'static [&'static str]],
    /// If set, the tool is experimental and this message will be shown as a warning.
    experimental: Option<&'static str>,
    /// If true, the binary in ~/.ana/bin/ should be a link to ana itself,
    /// which acts as a wrapper for the actual tool binary.
    uses_wrapper: bool,
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
        experimental: None,
        uses_wrapper: false,
    },
    #[cfg(unix)]
    Tool {
        name: "outerbounds",
        lockfile: include_str!("../../tool-specs/outerbounds/pixi.lock"),
        binaries: &[&["bin", "outerbounds"]],
        experimental: Some("Outerbounds integration is an experimental alpha feature."),
        uses_wrapper: false,
    },
    Tool {
        name: "conda",
        lockfile: include_str!("../../tool-specs/conda/pixi.lock"),
        binaries: if cfg![unix] {
            &[&["bin", "conda"]]
        } else {
            &[&["Scripts", "conda"]]
        },
        experimental: None,
        uses_wrapper: true,
    },
    Tool {
        name: "pixi",
        lockfile: include_str!("../../tool-specs/pixi/pixi.lock"),
        binaries: &[&["bin", "pixi"]],
        experimental: None,
        uses_wrapper: false,
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
pub fn experimental_message(name: &str) -> Option<&'static str> {
    find_tool(name).and_then(|t| t.experimental)
}

/// Returns whether a tool uses a wrapper binary (ana acts as the binary).
pub fn uses_wrapper(name: &str) -> bool {
    find_tool(name).map(|t| t.uses_wrapper).unwrap_or(false)
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
