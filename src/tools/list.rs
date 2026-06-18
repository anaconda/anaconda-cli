//! List available tools and their installation status.

use crate::context::CommandContext;
use std::path::PathBuf;

use crate::paths;
use crate::table::{self, Color};

use super::specs;

/// Information about a tool for display.
pub struct ToolInfo {
    pub name: &'static str,
    pub installed: bool,
    pub binaries: Vec<PathBuf>,
}

/// An externally-managed installer. Unlike the rattler-managed tools above,
/// these are downloaded (and SHA256-verified) into the current directory by
/// `ana tool download`, then installed by the user — `ana` does not manage the
/// resulting installation.
struct Installer {
    name: &'static str,
    /// Version offered by the download, or `None` if not yet available. Only
    /// "latest" is supported today.
    version: Option<&'static str>,
    /// Download command, or `None` if not yet available.
    command: Option<&'static str>,
    status: &'static str,
}

/// Externally-managed installers shown in the second table.
const INSTALLERS: &[Installer] = &[
    Installer {
        name: "miniconda",
        version: Some("latest"),
        command: Some("ana tool download miniconda"),
        status: "available",
    },
    Installer {
        name: "anaconda",
        version: None,
        command: None,
        status: "coming soon",
    },
];

/// List all available tools with their installation status.
pub fn list_tools() -> Vec<ToolInfo> {
    specs::all_tools()
        .iter()
        .map(|name| {
            let prefix = paths::tool_prefix(name);
            let installed = prefix.exists();
            let binaries = specs::binaries(name).unwrap_or_default();
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

    print_installer_list();
}

/// Print the externally-managed installer table.
fn print_installer_list() {
    let mut table = table::new(["Name", "Version", "Download command", "Status"]);

    for installer in INSTALLERS {
        table.add_row([
            table::cell(installer.name),
            table::cell(installer.version.unwrap_or("n/a")),
            table::cell(installer.command.unwrap_or("n/a")),
            table::cell(installer.status),
        ]);
    }

    println!();
    println!("Externally Managed Installers");
    println!("{table}");
}
