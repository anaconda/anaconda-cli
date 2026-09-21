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
/// This removes the old prefix so Fleet can do a fresh install, but refuses
/// when the prefix contains user-created named environments (envs/), which a
/// recursive removal would destroy. (Fleet's force option cannot adopt a
/// legacy prefix, so it is not a substitute for this migration.)
fn migrate_legacy_install(prefix: &Path, name: &str) -> miette::Result<()> {
    match std::fs::read_dir(prefix.join("envs")) {
        Ok(mut entries) => {
            if entries.next().transpose().into_diagnostic()?.is_some() {
                return Err(miette::miette!(
                    "Cannot migrate {name}: its envs directory is not empty. \
                     Keep using the existing installation until these environments \
                     have been migrated."
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).into_diagnostic(),
    }

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

    let lock_content =
        specs::content(name).ok_or_else(|| miette::miette!("unknown tool: {}", name))?;

    let binaries = specs::binaries(name).unwrap_or_default();

    // The delegate and requested packages are independent of exposed binaries.
    let delegate = specs::delegate_executable(name);
    let manifest = match name {
        "anaconda-cli" => include_str!("../../tool-specs/anaconda-cli/pixi.toml"),
        "conda" => include_str!("../../tool-specs/conda/pixi.toml"),
        "pixi" => include_str!("../../tool-specs/pixi/pixi.toml"),
        "outerbounds" => include_str!("../../tool-specs/outerbounds/pixi.toml"),
        _ => return Err(miette::miette!("unknown tool: {name}")),
    };
    let manifest: toml::Value = toml::from_str(manifest)
        .into_diagnostic()
        .context("failed to parse tool manifest")?;
    let requested_specs = manifest
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| miette::miette!("tool manifest has no dependencies"))?
        .iter()
        .map(|(package, version)| {
            let version = version.as_str().ok_or_else(|| {
                miette::miette!("expected a version string for dependency {package}")
            })?;
            Ok(format!("{package} {version}"))
        })
        .collect::<miette::Result<Vec<_>>>()?;

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

    // Reuse current prefixes while still refreshing ana's launchers below.
    let desired_hash = spec.lock_sha256();
    let existing = fleet.get(name)?;
    let force = existing.is_some();
    let installed = match existing {
        Some(runtime) if runtime.lock_sha256.as_deref() == Some(desired_hash.as_str()) => {
            eprintln!("{} is already up to date.", name);
            runtime
        }
        _ => {
            eprintln!("Installing {} into {}", name, prefix.display());

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
            installed
        }
    };

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

/// Extract the tool package version from the lockfile for this platform.
fn tool_version_from_lock(lock_content: &str, tool_name: &str) -> miette::Result<String> {
    let package_name = match tool_name {
        "anaconda-cli" => "anaconda-cli-base",
        name => name,
    };
    let lock_file = rattler_lock::LockFile::from_str_with_base_directory(lock_content, None)
        .into_diagnostic()
        .context("failed to parse lockfile")?;
    let environment = lock_file
        .default_environment()
        .ok_or_else(|| miette::miette!("lockfile has no default environment"))?;
    let platform = rattler_conda_types::Platform::current();
    let records = environment
        .conda_repodata_records_by_platform()
        .into_diagnostic()
        .context("failed to extract records from lockfile")?
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
        for (name, lock_content, expected) in [
            (
                "pixi",
                include_str!("../../tool-specs/pixi/pixi.lock"),
                "0.70.2",
            ),
            (
                "anaconda-cli",
                include_str!("../../tool-specs/anaconda-cli/pixi.lock"),
                "0.9.1",
            ),
        ] {
            assert_eq!(
                tool_version_from_lock(lock_content, name).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn test_tool_version_from_lock_missing_package() {
        let lock_content = include_str!("../../tool-specs/pixi/pixi.lock");
        assert!(tool_version_from_lock(lock_content, "unknown").is_err());
    }

    #[test]
    fn test_tool_version_from_lock_invalid() {
        assert!(tool_version_from_lock("version: 6\n", "pixi").is_err());
    }

    #[test]
    fn test_migrate_legacy_install_refuses_nonempty_envs() {
        let temp = tempfile::TempDir::new().unwrap();
        let prefix = temp.path().join("conda");
        std::fs::create_dir_all(prefix.join("envs").join("myenv")).unwrap();

        let result = migrate_legacy_install(&prefix, "conda");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("envs directory is not empty"),
            "unexpected error: {err}"
        );
        assert!(prefix.exists(), "prefix should be preserved");
    }

    #[test]
    fn test_migrate_legacy_install_removes_prefix_with_empty_envs() {
        let temp = tempfile::TempDir::new().unwrap();
        let prefix = temp.path().join("conda");
        std::fs::create_dir_all(prefix.join("envs")).unwrap();
        std::fs::write(prefix.join(".lockfile-hash"), "abc").unwrap();

        migrate_legacy_install(&prefix, "conda").unwrap();
        assert!(!prefix.exists(), "prefix should be removed");
    }

    #[test]
    fn test_migrate_legacy_install_removes_prefix_without_envs() {
        let temp = tempfile::TempDir::new().unwrap();
        let prefix = temp.path().join("pixi");
        std::fs::create_dir_all(&prefix).unwrap();
        std::fs::write(prefix.join(".lockfile-hash"), "abc").unwrap();

        migrate_legacy_install(&prefix, "pixi").unwrap();
        assert!(!prefix.exists(), "prefix should be removed");
    }
}
