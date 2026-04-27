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
    fn test_content_from_custom_dir() {
        let dir = tempfile::tempdir().unwrap();
        let tool_dir = dir.path().join("my-tool");
        std::fs::create_dir_all(&tool_dir).unwrap();
        std::fs::write(tool_dir.join("pixi.lock"), "custom lockfile content").unwrap();

        temp_env::with_var(
            "ANA_LOCKFILES_DIR",
            Some(dir.path().to_str().unwrap()),
            || {
                let lockfile = content("my-tool");
                assert!(lockfile.is_some());
                assert_eq!(lockfile.unwrap(), "custom lockfile content");
            },
        );
    }

    #[test]
    fn test_content_custom_dir_missing_file() {
        let dir = tempfile::tempdir().unwrap();

        temp_env::with_var(
            "ANA_LOCKFILES_DIR",
            Some(dir.path().to_str().unwrap()),
            || {
                let lockfile = content("nonexistent-tool");
                assert!(lockfile.is_none());
            },
        );
    }

    #[test]
    fn test_all_tools() {
        let tools = all_tools();
        assert!(tools.contains(&"anaconda-cli"));
        assert!(tools.contains(&"pixi"));
        assert_eq!(tools.len(), 2);
    }

    #[test]
    fn test_binaries_anaconda_cli() {
        let bins = binaries("anaconda-cli");
        assert!(bins.is_some());
        let bins = bins.unwrap();
        assert_eq!(bins.len(), 1);
        if cfg!(unix) {
            assert_eq!(bins[0], PathBuf::from("bin/anaconda"));
        } else {
            assert_eq!(bins[0], PathBuf::from("Scripts/anaconda"));
        }
    }

    #[test]
    fn test_binaries_pixi() {
        let bins = binaries("pixi");
        assert!(bins.is_some());
        let bins = bins.unwrap();
        assert_eq!(bins.len(), 1);
        assert_eq!(bins[0], PathBuf::from("bin/pixi"));
    }

    #[test]
    fn test_binaries_unknown_tool() {
        assert!(binaries("unknown-tool").is_none());
    }

    #[test]
    fn test_binary_names_anaconda_cli() {
        let names = binary_names("anaconda-cli");
        assert!(names.is_some());
        let names = names.unwrap();
        assert_eq!(names, vec!["anaconda"]);
    }

    #[test]
    fn test_binary_names_pixi() {
        let names = binary_names("pixi");
        assert!(names.is_some());
        let names = names.unwrap();
        assert_eq!(names, vec!["pixi"]);
    }

    #[test]
    fn test_binary_names_unknown_tool() {
        assert!(binary_names("unknown-tool").is_none());
    }

    #[test]
    fn test_find_tool_returns_correct_tool() {
        let tool = find_tool("pixi");
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().name, "pixi");
    }

    #[test]
    fn test_find_tool_unknown() {
        assert!(find_tool("not-a-real-tool").is_none());
    }
}
