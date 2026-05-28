//! Tool uninstallation.

use miette::{Context, IntoDiagnostic};

use super::specs;
use crate::context::CommandContext;
use crate::input::prompt_yes_no;
use crate::paths;

/// Uninstall a tool.
///
/// Removes the tool's environment and any symlinks in the bin directory.
/// Cleans up empty directories afterward.
#[cfg_attr(feature = "conda-package", allow(dead_code))]
pub fn uninstall_tool(ctx: &mut CommandContext, name: &str, force: bool) -> miette::Result<()> {
    ctx.telemetry.add("tool_name", name.to_string());

    // Verify the tool is known
    if specs::binaries(name).is_none() {
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

    // Check for symlinks/shims that will be removed
    if let Some(binaries) = specs::binary_names(name) {
        for binary in binaries {
            let link_path = paths::bin_path(binary);
            if link_path.exists() || link_path.is_symlink() {
                to_delete.push(format!("  {}", link_path.display()));
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
    if !force && !prompt_yes_no("Proceed with uninstall?", false) {
        eprintln!("Aborted.");
        return Ok(());
    }

    eprintln!();
    eprintln!("Uninstalling {}...", name);

    // Remove symlinks/shims from bin directory
    if let Some(binaries) = specs::binary_names(name) {
        for binary in &binaries {
            let link_path = paths::bin_path(binary);
            if link_path.exists() || link_path.is_symlink() {
                std::fs::remove_file(&link_path)
                    .into_diagnostic()
                    .with_context(|| format!("failed to remove: {}", link_path.display()))?;
                eprintln!("   Removed {}", link_path.display());
            }
        }

        // On Windows, also remove entries from shims.cfg
        #[cfg(windows)]
        remove_shims_cfg_entries(&binaries)?;
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
#[cfg_attr(feature = "conda-package", allow(dead_code))]
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

#[cfg(windows)]
/// Remove entries from shims.cfg for the given binary names.
fn remove_shims_cfg_entries(binaries: &[&str]) -> miette::Result<()> {
    let config_path = paths::ana_home().join("tools").join("shims.cfg");

    if !config_path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&config_path)
        .into_diagnostic()
        .context("failed to read shims.cfg")?;

    // Filter out entries for the binaries being removed
    let new_content: String = content
        .lines()
        .filter(|line| {
            if let Some((name, _)) = line.split_once('=') {
                !binaries.contains(&name)
            } else {
                true
            }
        })
        .map(|line| format!("{}\r\n", line))
        .collect();

    std::fs::write(&config_path, new_content)
        .into_diagnostic()
        .context("failed to write shims.cfg")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    mod windows_tests {
        use tempfile::TempDir;

        use super::super::remove_shims_cfg_entries;

        #[test]
        fn test_remove_shims_cfg_entries_removes_single() {
            let temp = TempDir::new().unwrap();
            let tools_dir = temp.path().join("tools");
            std::fs::create_dir_all(&tools_dir).unwrap();

            let config_path = tools_dir.join("shims.cfg");
            std::fs::write(
                &config_path,
                "pixi=pixi\\bin\\pixi.exe\r\nanaconda=anaconda-cli\\Scripts\\anaconda.exe\r\n",
            )
            .unwrap();

            temp_env::with_var("ANA_HOME", Some(temp.path().to_str().unwrap()), || {
                let result = remove_shims_cfg_entries(&["pixi"]);
                assert!(result.is_ok());

                let content = std::fs::read_to_string(&config_path).unwrap();
                assert!(!content.contains("pixi="));
                assert!(content.contains("anaconda=anaconda-cli\\Scripts\\anaconda.exe"));
            });
        }

        #[test]
        fn test_remove_shims_cfg_entries_removes_multiple() {
            let temp = TempDir::new().unwrap();
            let tools_dir = temp.path().join("tools");
            std::fs::create_dir_all(&tools_dir).unwrap();

            let config_path = tools_dir.join("shims.cfg");
            std::fs::write(
                &config_path,
                "pixi=pixi\\bin\\pixi.exe\r\nanaconda=anaconda-cli\\Scripts\\anaconda.exe\r\nother=other\\bin\\other.exe\r\n",
            )
            .unwrap();

            temp_env::with_var("ANA_HOME", Some(temp.path().to_str().unwrap()), || {
                let result = remove_shims_cfg_entries(&["pixi", "anaconda"]);
                assert!(result.is_ok());

                let content = std::fs::read_to_string(&config_path).unwrap();
                assert!(!content.contains("pixi="));
                assert!(!content.contains("anaconda="));
                assert!(content.contains("other=other\\bin\\other.exe"));
            });
        }

        #[test]
        fn test_remove_shims_cfg_entries_handles_missing_file() {
            let temp = TempDir::new().unwrap();
            let tools_dir = temp.path().join("tools");
            std::fs::create_dir_all(&tools_dir).unwrap();

            temp_env::with_var("ANA_HOME", Some(temp.path().to_str().unwrap()), || {
                let result = remove_shims_cfg_entries(&["pixi"]);
                assert!(result.is_ok(), "should succeed when file doesn't exist");
            });
        }

        #[test]
        fn test_remove_shims_cfg_entries_preserves_trailing_newlines() {
            let temp = TempDir::new().unwrap();
            let tools_dir = temp.path().join("tools");
            std::fs::create_dir_all(&tools_dir).unwrap();

            let config_path = tools_dir.join("shims.cfg");
            std::fs::write(
                &config_path,
                "pixi=pixi\\bin\\pixi.exe\r\nanaconda=anaconda-cli\\Scripts\\anaconda.exe\r\n",
            )
            .unwrap();

            temp_env::with_var("ANA_HOME", Some(temp.path().to_str().unwrap()), || {
                let result = remove_shims_cfg_entries(&["pixi"]);
                assert!(result.is_ok());

                let content = std::fs::read_to_string(&config_path).unwrap();
                assert!(
                    content.ends_with("\r\n"),
                    "should preserve trailing newline"
                );
            });
        }
    }
}
