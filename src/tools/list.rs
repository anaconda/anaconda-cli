//! List available tools and their installation status.

use comfy_table::{
    Attribute, Cell, Color, Table, modifiers::UTF8_SOLID_INNER_BORDERS, presets::UTF8_FULL,
};

use crate::paths;

use super::lockfiles;

/// Information about a tool for display.
pub struct ToolInfo {
    pub name: &'static str,
    pub installed: bool,
    pub binaries: &'static [&'static str],
}

/// List all available tools with their installation status.
pub fn list_tools() -> Vec<ToolInfo> {
    lockfiles::all_tools()
        .iter()
        .map(|name| {
            let prefix = paths::tool_prefix(name);
            let installed = prefix.exists();
            let binaries = lockfiles::binaries(name).unwrap_or(&[]);
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

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.apply_modifier(UTF8_SOLID_INNER_BORDERS);
    table.set_header([
        Cell::new("Name").add_attribute(Attribute::Bold),
        Cell::new("Installed").add_attribute(Attribute::Bold),
        Cell::new("Binaries").add_attribute(Attribute::Bold),
    ]);

    for tool in tools {
        let status_cell = if tool.installed {
            Cell::new("✓").fg(Color::Green)
        } else {
            Cell::new("✗").fg(Color::Red)
        };
        let binaries = tool.binaries.join(", ");
        table.add_row([Cell::new(tool.name), status_cell, Cell::new(&binaries)]);
    }

    println!("{table}");
}
