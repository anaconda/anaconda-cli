//! Tool uninstallation.

use miette::{Context, IntoDiagnostic};

use super::tools;
use crate::input::prompt_yes_no;
use crate::paths;

/// Uninstall a tool.
///
/// Removes the tool's environment and any symlinks in the bin directory.
/// Cleans up empty directories afterward.
pub fn uninstall_tool(name: &str, force: bool) -> miette::Result<()> {
    // Verify the tool is known
    if tools::binaries(name).is_none() {
        return Err(miette::miette!("unknown tool: {}", name));
    }

    let prefix = paths::tool_prefix(name);
    let bin_dir = paths::bin_dir();

    // Check if the tool is installed
    if !prefix.exists() {
        eprintln!("{} is not installed", name);
        return Ok(());
    }

    // Collect what will be deleted
    let mut to_delete: Vec<String> = Vec::new();

    // Check for symlinks that will be removed
    if let Some(binaries) = tools::binaries(name) {
        for binary in binaries {
            let symlink_path = bin_dir.join(binary);
            if symlink_path.exists() || symlink_path.is_symlink() {
                to_delete.push(format!("  {}", symlink_path.display()));
            }
        }
    }

    // The tool directory itself
    to_delete.push(format!("  {}", prefix.display()));

    // Show what will be deleted
    eprintln!("The following will be removed:");
    for item in &to_delete {
        eprintln!("{}", item);
    }
    eprintln!();

    // Prompt for confirmation unless --force was passed
    if !force && !prompt_yes_no("Proceed with uninstall?") {
        eprintln!("Aborted.");
        return Ok(());
    }

    eprintln!();
    eprintln!("Uninstalling {}...", name);

    // Remove symlinks from bin directory
    if let Some(binaries) = tools::binaries(name) {
        for binary in binaries {
            let symlink_path = bin_dir.join(binary);
            if symlink_path.exists() || symlink_path.is_symlink() {
                std::fs::remove_file(&symlink_path)
                    .into_diagnostic()
                    .with_context(|| {
                        format!("failed to remove symlink: {}", symlink_path.display())
                    })?;
                eprintln!("   Removed {}", symlink_path.display());
            }
        }
    }

    // Remove the tool's environment directory
    std::fs::remove_dir_all(&prefix)
        .into_diagnostic()
        .with_context(|| format!("failed to remove tool directory: {}", prefix.display()))?;
    eprintln!("   Removed {}", prefix.display());

    // Clean up empty directories
    cleanup_empty_dir(&bin_dir)?;
    cleanup_empty_dir(&paths::ana_home().join("tools"))?;

    eprintln!("Successfully uninstalled {}", name);

    Ok(())
}

/// Remove a directory if it's empty.
fn cleanup_empty_dir(path: &std::path::Path) -> miette::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    // Check if directory is empty
    let is_empty = path
        .read_dir()
        .into_diagnostic()
        .with_context(|| format!("failed to read directory: {}", path.display()))?
        .next()
        .is_none();

    if is_empty {
        std::fs::remove_dir(path)
            .into_diagnostic()
            .with_context(|| format!("failed to remove empty directory: {}", path.display()))?;
        eprintln!("   Cleaned up empty directory: {}", path.display());
    }

    Ok(())
}
