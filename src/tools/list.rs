//! List available tools and their installation status.

use crate::context::CommandContext;
use std::path::PathBuf;

use crate::paths;
use crate::table::{self, Color};

use super::tools;

/// Information about a tool for display.
pub struct ToolInfo {
    pub name: &'static str,
    pub installed: bool,
    pub binaries: Vec<PathBuf>,
}

/// List all available tools with their installation status.
pub fn list_tools() -> Vec<ToolInfo> {
    tools::all_tools()
        .iter()
        .map(|name| {
            let prefix = paths::tool_prefix(name);
            let installed = prefix.exists();
            let binaries = tools::binaries(name).unwrap_or(Vec::new());
            ToolInfo {
                name,
                installed,
                binaries,
            }
        })
        .collect()
}

/// Print the tool list as a formatted table.
pub fn print_tool_list(_ctx: &mut CommandContext) {
    let tools = list_tools();

    let mut table = table::new(["Name", "Installed", "Binaries"]);

    for tool in tools {
        let status_cell = if tool.installed {
            table::cell("✓").fg(Color::Green)
        } else {
            table::cell("✗").fg(Color::Red)
        };
        let binaries = tool
            .binaries
            .iter()
            .filter_map(|b| b.file_stem().and_then(|s| s.to_str()))
            .collect::<Vec<_>>()
            .join(", ");
        table.add_row([table::cell(tool.name), status_cell, table::cell(&binaries)]);
    }

    println!("{table}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_tools_returns_all_tools() {
        temp_env::with_var("ANA_HOME", Some("/nonexistent/path"), || {
            let tools = list_tools();

            // Should have the known tools
            let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
            assert!(names.contains(&"anaconda-cli"));
            assert!(names.contains(&"pixi"));
        });
    }

    #[test]
    fn test_list_tools_detects_not_installed() {
        temp_env::with_var("ANA_HOME", Some("/nonexistent/path"), || {
            let tools = list_tools();

            // Tools should not be installed in nonexistent path
            for tool in tools {
                assert!(
                    !tool.installed,
                    "Tool {} should not be installed",
                    tool.name
                );
            }
        });
    }

    #[test]
    fn test_list_tools_detects_installed() {
        let temp = tempfile::tempdir().unwrap();
        let tools_dir = temp.path().join("tools");

        // Create fake installed tool
        std::fs::create_dir_all(tools_dir.join("pixi")).unwrap();

        temp_env::with_var("ANA_HOME", Some(temp.path().to_str().unwrap()), || {
            let tools = list_tools();

            let pixi = tools.iter().find(|t| t.name == "pixi").unwrap();
            assert!(pixi.installed, "pixi should be detected as installed");

            let anaconda_cli = tools.iter().find(|t| t.name == "anaconda-cli").unwrap();
            assert!(
                !anaconda_cli.installed,
                "anaconda-cli should not be installed"
            );
        });
    }

    #[test]
    fn test_tool_info_has_binaries() {
        temp_env::with_var("ANA_HOME", Some("/nonexistent"), || {
            let tools = list_tools();

            let pixi = tools.iter().find(|t| t.name == "pixi").unwrap();
            assert!(!pixi.binaries.is_empty(), "pixi should have binaries");

            let anaconda_cli = tools.iter().find(|t| t.name == "anaconda-cli").unwrap();
            assert!(
                !anaconda_cli.binaries.is_empty(),
                "anaconda-cli should have binaries"
            );
        });
    }
}
