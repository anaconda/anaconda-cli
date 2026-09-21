//! Embedded lockfiles for tool installation.

use std::path::PathBuf;

/// Tool configuration.
#[cfg_attr(not(tool_install), allow(dead_code))]
struct Tool {
    name: &'static str,
    #[cfg_attr(not(tool_install), allow(dead_code))]
    lockfile: &'static str,
    binaries: &'static [&'static [&'static str]],
    #[cfg_attr(not(tool_install), allow(dead_code))]
    experimental: Option<&'static str>,
    /// If true, a standalone wrapper binary is installed to ~/.ana/bin/
    /// instead of a symlink to the tool binary.
    uses_wrapper: bool,
    /// Whether this tool should be auto-updated when `ana` is updated.
    #[cfg_attr(feature = "fleet", allow(dead_code))]
    auto_update: bool,
    /// The executable inside the prefix that Fleet validates after install.
    /// None means it matches the tool name. Distinct from `binaries`, which
    /// lists what ana exposes on PATH.
    #[cfg_attr(not(feature = "fleet"), allow(dead_code))]
    delegate: Option<&'static str>,
}

/// Embedded tool configurations.
const TOOLS: &[Tool] = &[
    Tool {
        name: "anaconda-cli",
        lockfile: include_str!("../../tool-specs/anaconda-cli/pixi.lock"),
        // No symlink - anaconda-cli is only accessed via ana subcommands (e.g., ana mcp)
        // to avoid shadowing users' existing anaconda command from anaconda-auth
        binaries: &[],
        experimental: None,
        uses_wrapper: false,
        auto_update: true,
        // The anaconda-cli package provides `bin/anaconda`
        delegate: Some("anaconda"),
    },
    #[cfg(unix)]
    Tool {
        name: "outerbounds",
        lockfile: include_str!("../../tool-specs/outerbounds/pixi.lock"),
        binaries: &[&["bin", "outerbounds"]],
        experimental: Some("Outerbounds integration is an experimental alpha feature."),
        uses_wrapper: false,
        auto_update: true,
        delegate: None,
    },
    Tool {
        name: "conda",
        lockfile: include_str!("../../tool-specs/conda/pixi.lock"),
        // binaries is still needed with uses_wrapper to determine wrapper filename
        binaries: if cfg![unix] {
            &[&["bin", "conda"]]
        } else {
            &[&["Scripts", "conda"]]
        },
        experimental: Some("conda"),
        uses_wrapper: true,
        auto_update: true,
        delegate: None,
    },
    Tool {
        name: "pixi",
        lockfile: include_str!("../../tool-specs/pixi/pixi.lock"),
        binaries: &[&["bin", "pixi"]],
        experimental: None,
        uses_wrapper: false,
        auto_update: false,
        delegate: None,
    },
];

fn find_tool(name: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|t| t.name == name)
}

/// Returns the lockfile content for a tool.
///
/// If `ANA_LOCKFILES_DIR` is set, reads from that directory.
/// Otherwise, returns the embedded lockfile compiled into the binary.
#[cfg_attr(not(tool_install), allow(dead_code))]
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
#[cfg_attr(not(tool_install), allow(dead_code))]
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

/// Returns whether a tool uses a custom wrapper binary.
#[cfg_attr(all(not(tool_install), not(feature = "fleet")), allow(dead_code))]
pub fn uses_wrapper(name: &str) -> bool {
    find_tool(name).map(|t| t.uses_wrapper).unwrap_or(false)
}

/// Extract the tool package version from lockfile content for this platform.
#[cfg(tool_install)]
pub fn locked_version_from_str(lock_content: &str, tool_name: &str) -> miette::Result<String> {
    let package_name = match tool_name {
        "anaconda-cli" => "anaconda-cli-base",
        name => name,
    };
    let lock_file = rattler_lock::LockFile::from_str_with_base_directory(lock_content, None)
        .map_err(|e| miette::miette!("failed to parse lockfile: {e}"))?;
    let environment = lock_file
        .default_environment()
        .ok_or_else(|| miette::miette!("lockfile has no default environment"))?;
    let platform = rattler_conda_types::Platform::current();
    let records = environment
        .conda_repodata_records_by_platform()
        .map_err(|e| miette::miette!("failed to extract records from lockfile: {e}"))?
        .into_iter()
        .find(|(p, _)| p.subdir() == platform)
        .map(|(_, records)| records)
        .ok_or_else(|| miette::miette!("lockfile has no records for platform {platform}"))?;

    records
        .into_iter()
        .find(|record| record.package_record.name.as_normalized() == package_name)
        .map(|record| record.package_record.version.to_string())
        .ok_or_else(|| {
            miette::miette!("lockfile has no {package_name} package for platform {platform}")
        })
}

/// Version of the tool recorded in its embedded lockfile, if resolvable.
#[cfg(tool_install)]
pub fn locked_version(name: &str) -> Option<String> {
    content(name).and_then(|lock| locked_version_from_str(&lock, name).ok())
}

/// Returns whether auto-update is enabled for a tool by default.
#[cfg_attr(any(not(tool_install), feature = "fleet"), allow(dead_code))]
pub fn auto_update_default(name: &str) -> bool {
    find_tool(name).is_some_and(|t| t.auto_update)
}

/// Returns the delegate executable for a tool (defaults to the tool name).
#[cfg_attr(not(feature = "fleet"), allow(dead_code))]
pub fn delegate_executable(name: &str) -> &str {
    find_tool(name).and_then(|t| t.delegate).unwrap_or(name)
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
            assert!(lockfile.unwrap().contains("version: 6"));
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

    #[test]
    fn test_delegate_executable_anaconda_cli() {
        assert_eq!(delegate_executable("anaconda-cli"), "anaconda");
    }

    #[test]
    fn test_delegate_executable_defaults_to_name() {
        assert_eq!(delegate_executable("pixi"), "pixi");
        assert_eq!(delegate_executable("conda"), "conda");
        assert_eq!(delegate_executable("unknown-tool"), "unknown-tool");
    }

    #[cfg(tool_install)]
    #[test]
    fn test_locked_version_from_str() {
        assert_eq!(
            locked_version_from_str(include_str!("../../tool-specs/pixi/pixi.lock"), "pixi")
                .unwrap(),
            "0.70.2"
        );
        assert_eq!(
            locked_version_from_str(
                include_str!("../../tool-specs/anaconda-cli/pixi.lock"),
                "anaconda-cli"
            )
            .unwrap(),
            "0.9.1"
        );
    }

    #[cfg(tool_install)]
    #[test]
    fn test_locked_version_from_str_missing_package() {
        let lock = include_str!("../../tool-specs/pixi/pixi.lock");
        assert!(locked_version_from_str(lock, "unknown").is_err());
    }

    #[cfg(tool_install)]
    #[test]
    fn test_locked_version_from_str_invalid() {
        assert!(locked_version_from_str("version: 6\n", "pixi").is_err());
    }
}
