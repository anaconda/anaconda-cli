//! Tool installation using conda-ship's Fleet API.
//!
//! This module replaces the direct rattler-based installation with conda-ship's
//! Fleet API, which provides a higher-level abstraction for managing multiple
//! locked conda prefixes.

use std::path::Path;

use conda_ship::fleet::{Fleet, InstallOptions, InstalledRuntime, RuntimeSpec};
use miette::{Context, IntoDiagnostic};

use super::{common, pixi_config, specs};
use crate::context::CommandContext;
use crate::paths;

/// Check if a prefix is an old rattler-based installation (pre-Fleet).
///
/// Old installations have:
/// - A `conda-meta/` directory (bootstrapped conda prefix)
/// - A `.lockfile-hash` file (ana's old staleness marker)
/// - No `.{name}.json` Fleet metadata file (Fleet writes it at the prefix root)
fn is_legacy_rattler_install(prefix: &Path, name: &str) -> bool {
    let conda_meta = prefix.join("conda-meta");
    let lockfile_hash = prefix.join(".lockfile-hash");
    let fleet_metadata = prefix.join(format!(".{name}.json"));

    conda_meta.is_dir() && lockfile_hash.exists() && !fleet_metadata.exists()
}

/// Migrate a legacy rattler-based installation to Fleet.
///
/// This removes the old prefix so Fleet can do a fresh install.
fn migrate_legacy_install(prefix: &Path, name: &str) -> miette::Result<()> {
    crate::ui::status::info(&format!(
        "Migrating {} from legacy installation to Fleet...",
        name
    ));

    std::fs::remove_dir_all(prefix)
        .into_diagnostic()
        .with_context(|| format!("failed to remove legacy installation: {}", prefix.display()))?;

    Ok(())
}

/// Install a tool using conda-ship's Fleet API.
pub async fn install_tool(ctx: &mut CommandContext, name: &str) -> miette::Result<()> {
    ctx.telemetry.add("tool_name", name.to_string());

    if let Some(msg) = specs::experimental_message(name) {
        crate::ui::status::warn(msg);
        eprintln!();
    }

    let lock_content =
        specs::content(name).ok_or_else(|| miette::miette!("unknown tool: {}", name))?;

    let binaries = specs::binaries(name).unwrap_or_default();

    // The delegate executable and requested package are independent of the
    // binaries ana exposes on PATH (e.g. anaconda-cli exposes nothing but
    // provides `bin/anaconda`).
    let delegate = specs::delegate_executable(name);
    let requested_specs: Vec<String> = vec![name.to_string()];

    let spec = RuntimeSpec {
        id: name.to_string(),
        version: tool_version_from_lock(&lock_content, name)?,
        delegate_executable: delegate.to_string(),
        lock_content,
        requested_specs,
        // Fleet writes .condarc and the frozen marker before marking the
        // prefix ready, so a failure cannot leave a ready but
        // unconfigured/unprotected installation.
        condarc: (name == "conda")
            .then(|| include_str!("../../tool-specs/conda/.condarc").to_owned()),
        freeze_base: name == "conda",
        installer: None,
    };

    let fleet = Fleet::new(paths::ana_home().join("tools"));
    let prefix = paths::tool_prefix(name);

    // Migrate legacy rattler-based installations
    if is_legacy_rattler_install(&prefix, name) {
        migrate_legacy_install(&prefix, name)?;
    }

    eprintln!("Installing {} into {}", name, prefix.display());

    // Force a reinstall when an existing healthy runtime was installed from a
    // different lockfile. Fleet::install reuses ready installations otherwise.
    // Interrupted installs have no usable metadata, so `get` returns None and
    // install recovers them without force.
    let desired_hash = spec.lock_sha256();
    let force = fleet
        .get(name)?
        .is_some_and(|runtime| runtime.lock_sha256.as_deref() != Some(desired_hash.as_str()));

    let installed = fleet
        .install(
            spec,
            InstallOptions {
                force,
                ..InstallOptions::default()
            },
        )
        .await
        .with_context(|| format!("failed to install tool: {}", name))?;

    eprintln!(
        "   Installed {} v{} to {}",
        installed.id,
        installed.version,
        installed.prefix.display()
    );

    // TODO: Consider passing uses_wrapper into Fleet APIs directly
    let uses_wrapper = specs::uses_wrapper(name);
    common::create_bin_symlinks(&installed.prefix, &binaries, uses_wrapper)?;

    if name == "pixi" {
        pixi_config::configure_default_channels(&paths::bin_path("pixi"))?;
    }

    Ok(())
}

/// Uninstall a tool using conda-ship's Fleet API.
pub fn uninstall_tool(ctx: &mut CommandContext, name: &str, force: bool) -> miette::Result<()> {
    ctx.telemetry.add("tool_name", name.to_string());

    if specs::binaries(name).is_none() {
        return Err(miette::miette!("unknown tool: {}", name));
    }

    let fleet = Fleet::new(paths::ana_home().join("tools"));
    let bin_dir = paths::bin_dir();

    let status = fleet.get(name)?;
    if status.is_none() {
        eprintln!("{} is not installed", name);
        return Ok(());
    }

    let mut to_delete: Vec<String> = Vec::new();

    if let Some(binaries) = specs::binary_names(name) {
        for binary in binaries {
            let link_path = paths::bin_path(binary);
            if link_path.exists() || link_path.is_symlink() {
                to_delete.push(format!("  {}", link_path.display()));
            }
        }
    }

    let prefix = paths::tool_prefix(name);
    to_delete.push(format!("  {}", prefix.display()));

    eprintln!("The following will be removed:");
    for item in &to_delete {
        eprintln!("{}", item);
    }
    eprintln!();

    if !force && !crate::input::prompt_yes_no("Proceed with uninstall?", false) {
        eprintln!("Aborted.");
        return Ok(());
    }

    eprintln!();
    eprintln!("Uninstalling {}...", name);

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

        #[cfg(windows)]
        remove_shims_cfg_entries(&binaries)?;
    }

    fleet
        .remove(name)
        .with_context(|| format!("failed to remove tool: {}", name))?;

    eprintln!("   Removed {}", prefix.display());

    cleanup_empty_dir(&bin_dir)?;

    eprintln!("Successfully uninstalled {}", name);

    Ok(())
}

/// List installed tools using Fleet API.
pub fn list_installed() -> miette::Result<Vec<InstalledRuntime>> {
    let fleet = Fleet::new(paths::ana_home().join("tools"));
    fleet.list()
}

/// Hex-encoded SHA-256 of lockfile content, matching Fleet's `lock_sha256`.
pub fn lock_hash(lock_content: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(lock_content.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Get status of a specific tool.
///
/// Returns `None` when the tool is not installed or its installation was
/// interrupted (no valid Fleet metadata).
pub fn tool_status(name: &str) -> miette::Result<Option<InstalledRuntime>> {
    let fleet = Fleet::new(paths::ana_home().join("tools"));
    fleet.get(name)
}

/// Update all installed Fleet-managed tools whose lockfiles have changed.
///
/// Only updates tools where auto-update is enabled. The global config setting
/// `auto_update_tools` overrides individual tool defaults when set.
///
/// Returns the names of tools that were updated.
pub async fn update_installed_tools(ctx: &mut CommandContext) -> miette::Result<Vec<String>> {
    let mut updated = Vec::new();
    for runtime in list_installed()? {
        let name = runtime.id.as_str();
        let auto_update = ctx
            .config
            .auto_update_tools
            .unwrap_or_else(|| specs::auto_update_default(name));
        if !auto_update {
            continue;
        }
        // Skip tools that are no longer in the catalog
        let Some(lock_content) = specs::content(name) else {
            continue;
        };
        let desired_hash = lock_hash(&lock_content);
        if runtime.lock_sha256.as_deref() == Some(desired_hash.as_str()) {
            continue;
        }
        crate::ui::status::info(&format!("Updating {}...", name));
        install_tool(ctx, name).await?;
        updated.push(name.to_string());
    }
    Ok(updated)
}

/// Extract version from lockfile for a tool.
fn tool_version_from_lock(lock_content: &str, tool_name: &str) -> miette::Result<String> {
    // Parse the lockfile to find the tool's version
    // Look for a package that matches the tool name
    for line in lock_content.lines() {
        let line = line.trim();
        if line.starts_with("- conda:") && line.contains(&format!("/{tool_name}-")) {
            // Extract version from URL like: .../pixi-0.70.2-hef7b95b_0.conda
            if let Some(filename) = line.rsplit('/').next() {
                let parts: Vec<&str> = filename.split('-').collect();
                if parts.len() >= 2 {
                    return Ok(parts[1].to_string());
                }
            }
        }
    }

    // Fallback to "latest" if we can't extract a version
    Ok("latest".to_string())
}

#[cfg(windows)]
fn remove_shims_cfg_entries(binaries: &[&str]) -> miette::Result<()> {
    let config_path = paths::ana_home().join("tools").join("shims.cfg");

    if !config_path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&config_path)
        .into_diagnostic()
        .context("failed to read shims.cfg")?;

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

fn cleanup_empty_dir(path: &Path) -> miette::Result<()> {
    if !path.exists() {
        return Ok(());
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_version_from_lock() {
        let lock_content = r#"
version: 6
environments:
  default:
    packages:
      osx-arm64:
      - conda: https://repo.anaconda.com/pkgs/main/osx-arm64/pixi-0.70.2-h46fb4a7_0.conda
"#;
        let version = tool_version_from_lock(lock_content, "pixi").unwrap();
        assert_eq!(version, "0.70.2");
    }

    #[test]
    fn test_tool_version_from_lock_fallback() {
        let lock_content = "version: 6\n";
        let version = tool_version_from_lock(lock_content, "unknown").unwrap();
        assert_eq!(version, "latest");
    }
}
