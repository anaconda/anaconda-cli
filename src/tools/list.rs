//! List available tools and their installation status.

use crate::paths;
use crate::table::{self, Color};

use super::tools;

/// Information about a tool for display.
pub struct ToolInfo {
    pub name: &'static str,
    pub installed: bool,
    pub binaries: &'static [&'static str],
}

/// List all available tools with their installation status.
pub fn list_tools() -> Vec<ToolInfo> {
    tools::all_tools()
        .iter()
        .map(|name| {
            let prefix = paths::tool_prefix(name);
            let installed = prefix.exists();
            let binaries = tools::binaries(name).unwrap_or(&[]);
            ToolInfo {
                name,
                installed,
                binaries,
            }
        })
        .collect()
}

/// Print the tool list as a formatted table.
pub fn print_tool_list() {
    let tools = list_tools();

    let mut table = table::new(["Name", "Installed", "Binaries"]);

    for tool in tools {
        let status_cell = if tool.installed {
            table::cell("✓").fg(Color::Green)
        } else {
            table::cell("✗").fg(Color::Red)
        };
        let binaries = tool.binaries.join(", ");
        table.add_row([table::cell(tool.name), status_cell, table::cell(&binaries)]);
    }

    println!("{table}");
}
