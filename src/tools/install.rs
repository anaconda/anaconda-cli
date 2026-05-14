//! Package installation from lockfiles via rattler.

use std::{path::Path, path::PathBuf, time::Instant};

use indicatif::{MultiProgress, ProgressDrawTarget};
use miette::{Context, IntoDiagnostic};
use rattler::{
    default_cache_dir,
    install::{IndicatifReporter, Installer},
    package_cache::PackageCache,
};
use rattler_conda_types::{Platform, PrefixRecord};
use rattler_lock::LockFile;

use super::{pixi_config, tools};
use crate::context::CommandContext;
use crate::paths;

/// Check all installed tools and return the names of those that need updating.
///
/// Returns a list of tool names that are installed but at a different version
/// than what's in the embedded lockfile.
pub fn check_all_tools_need_update() -> Vec<&'static str> {
    tools::all_tools()
        .into_iter()
        .filter(|tool_name| {
            tools::package_name(tool_name)
                .and_then(|pkg| check_tool_needs_update(tool_name, pkg))
                .is_some()
        })
        .collect()
}

/// Check if a tool needs updating by comparing installed vs embedded lockfile versions.
///
/// Returns `Some(installed_version)` if the tool is installed but at a different version
/// than what's in the embedded lockfile. Returns `None` if the tool is not installed
/// or is already at the correct version.
pub fn check_tool_needs_update(tool_name: &str, package_name: &str) -> Option<String> {
    let prefix = paths::tool_prefix(tool_name);
    if !prefix.exists() {
        return None;
    }

    // Get installed version from prefix
    let installed = PrefixRecord::collect_from_prefix::<PrefixRecord>(&prefix).ok()?;
    let installed_version = installed
        .iter()
        .find(|r| r.repodata_record.package_record.name.as_normalized() == package_name)
        .map(|r| r.repodata_record.package_record.version.to_string())?;

    // Get expected version from embedded lockfile
    let lock_content = tools::content(tool_name)?;
    let expected_version = get_package_version_from_lockfile(&lock_content, package_name)?;

    if installed_version < expected_version {
        Some(installed_version)
    } else {
        None
    }
}

/// Extract a package version from a lockfile string for the current platform.
fn get_package_version_from_lockfile(lock_content: &str, package_name: &str) -> Option<String> {
    let lock_file = LockFile::from_str_with_base_directory(lock_content, None).ok()?;
    let env = lock_file.default_environment()?;
    let current_platform = Platform::current();
    let records_by_platform = env.conda_repodata_records_by_platform().ok()?;

    let records = records_by_platform
        .into_iter()
        .find(|(p, _)| p.subdir() == current_platform)
        .map(|(_, records)| records)?;

    records
        .iter()
        .find(|r| r.package_record.name.as_normalized() == package_name)
        .map(|r| r.package_record.version.to_string())
}

/// Global progress bar for installation feedback.
static MULTI_PROGRESS: std::sync::LazyLock<MultiProgress> = std::sync::LazyLock::new(|| {
    let mp = MultiProgress::new();
    mp.set_draw_target(ProgressDrawTarget::stderr_with_hz(20));
    mp
});

/// Install a tool from its lockfile.
pub async fn install_tool(ctx: &mut CommandContext, name: &str) -> miette::Result<()> {
    ctx.telemetry.add("tool_name", name.to_string());

    // Show experimental warning if applicable
    if let Some(msg) = tools::experimental_message(name) {
        crate::ui::status::warn(msg);
        eprintln!();
    }

    let prefix = paths::tool_prefix(name);

    let lock_content =
        tools::content(name).ok_or_else(|| miette::miette!("unknown tool: {}", name))?;

    let binaries = tools::binaries(name).unwrap_or(Vec::new());

    eprintln!("Installing {} into {}", name, prefix.display());

    install_from_lockfile(ctx, &prefix, &lock_content).await?;

    // Create symlinks in bin directory
    create_bin_symlinks(&prefix, &binaries)?;

    // Tool-specific post-install configuration
    if name == "pixi" {
        pixi_config::configure_default_channels(&paths::bin_path("pixi"))?;
    }

    Ok(())
}

/// Install packages from a lockfile string to a prefix.
pub async fn install_from_lockfile(
    _ctx: &CommandContext,
    prefix: &Path,
    lock_content: &str,
) -> miette::Result<()> {
    let lock_file = LockFile::from_str_with_base_directory(lock_content, None)
        .into_diagnostic()
        .context("failed to parse lockfile")?;

    let env = lock_file
        .default_environment()
        .ok_or_else(|| miette::miette!("lockfile has no default environment"))?;

    let current_platform = Platform::current();
    let records_by_platform = env
        .conda_repodata_records_by_platform()
        .into_diagnostic()
        .context("failed to extract records from lockfile")?;

    let records = records_by_platform
        .into_iter()
        .find(|(p, _)| p.subdir() == current_platform)
        .map(|(_, records)| records)
        .ok_or_else(|| {
            miette::miette!("lockfile has no records for platform {}", current_platform)
        })?;

    eprintln!(
        "   Lockfile contains {} packages for {}",
        records.len(),
        current_platform
    );

    // Ensure prefix directory exists
    std::fs::create_dir_all(prefix)
        .into_diagnostic()
        .context("failed to create prefix directory")?;

    // Check what's already installed
    let installed = PrefixRecord::collect_from_prefix::<PrefixRecord>(prefix).into_diagnostic()?;

    // Build HTTP client for rattler (plain reqwest client, not middleware-wrapped)
    let client = reqwest::Client::builder()
        .no_gzip()
        .user_agent(crate::ua::user_agent())
        .build()
        .into_diagnostic()
        .context("failed to create download client")?;

    // Ensure cache directory exists
    // TODO(mattkram): Consider a custom cache dir
    let cache_dir = default_cache_dir()
        .map_err(|e| miette::miette!("could not determine cache directory: {}", e))?;
    rattler_cache::ensure_cache_dir(&cache_dir)
        .map_err(|e| miette::miette!("could not create cache directory: {}", e))?;

    let package_cache = PackageCache::new(cache_dir.join(rattler_cache::PACKAGE_CACHE_DIR));

    // Run installation
    let start = Instant::now();
    let result = Installer::new()
        .with_download_client(client)
        .with_package_cache(package_cache)
        .with_target_platform(current_platform)
        .with_installed_packages(installed)
        // TODO(mattkram): Review whether we should execute link scripts by default or not
        .with_execute_link_scripts(true)
        .with_reporter(
            IndicatifReporter::builder()
                .with_multi_progress(MULTI_PROGRESS.clone())
                .finish(),
        )
        .install(prefix, records)
        .await
        .into_diagnostic()
        .context("failed to install packages")?;

    if result.transaction.operations.is_empty() {
        eprintln!("   ✓ Already up to date");
    } else {
        eprintln!(
            "   Installed {} packages in {:.1}s",
            result.transaction.operations.len(),
            start.elapsed().as_secs_f64()
        );
    }

    Ok(())
}

/// Create symlinks (Unix) or shims (Windows) for the tool's binaries in ~/.ana/bin/
fn create_bin_symlinks(prefix: &Path, binaries: &[PathBuf]) -> miette::Result<()> {
    let bin_dir = paths::bin_dir();
    std::fs::create_dir_all(&bin_dir)
        .into_diagnostic()
        .context("failed to create bin directory")?;

    for binary in binaries {
        #[cfg(unix)]
        create_bin_symlink(&bin_dir, prefix, binary)?;
        #[cfg(windows)]
        create_bin_shim(&bin_dir, prefix, binary)?;
    }

    Ok(())
}

#[cfg(unix)]
/// Create a single symlink for a binary.
fn create_bin_symlink(bin_dir: &Path, prefix: &Path, binary: &Path) -> miette::Result<()> {
    let tool_bin = prefix.join(binary);
    let symlink_path = bin_dir.join(binary.file_name().unwrap());

    // Check if the tool binary exists
    if !tool_bin.exists() {
        eprintln!(
            "   Warning: binary '{}' not found in {}",
            binary.display(),
            prefix.display()
        );
        return Ok(());
    }

    // Remove existing symlink if present
    if symlink_path.exists() || symlink_path.is_symlink() {
        std::fs::remove_file(&symlink_path)
            .into_diagnostic()
            .context("failed to remove existing symlink")?;
    }

    std::os::unix::fs::symlink(&tool_bin, &symlink_path)
        .into_diagnostic()
        .with_context(|| format!("failed to create symlink: {}", symlink_path.display()))?;

    eprintln!(
        "   Linked {} -> {}",
        symlink_path.display(),
        tool_bin.display()
    );

    Ok(())
}

#[cfg(windows)]
/// Embedded shim binary (compiled from src/bin/shim.rs)
const SHIM_BINARY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/shim.exe"));

#[cfg(windows)]
/// Create a binary shim for a tool.
///
/// Copies the shim executable to bin_dir/<name>.exe and updates shims.cfg
/// with the mapping from shim name to target binary.
fn create_bin_shim(bin_dir: &Path, prefix: &Path, binary: &Path) -> miette::Result<()> {
    let tool_bin = prefix.join(binary).with_extension("exe");
    let shim_name = binary.file_stem().unwrap().to_string_lossy();
    let shim_path = bin_dir.join(format!("{}.exe", shim_name));

    // Check if the tool binary exists
    if !tool_bin.exists() {
        eprintln!(
            "   Warning: binary '{}' not found in {}",
            binary.display(),
            prefix.display()
        );
        return Ok(());
    }

    // Copy shim binary to bin_dir
    std::fs::write(&shim_path, SHIM_BINARY)
        .into_diagnostic()
        .with_context(|| format!("failed to write shim: {}", shim_path.display()))?;

    // Update shims.cfg with the mapping
    // Format: <tool_name>\<binary_path_with_exe>
    let tool_name = prefix.file_name().unwrap().to_string_lossy();
    let rel_target = format!("{}\\{}", tool_name, binary.with_extension("exe").display());
    update_shims_cfg(&shim_name, &rel_target)?;

    eprintln!(
        "   Created shim {} -> {}",
        shim_path.display(),
        tool_bin.display()
    );

    Ok(())
}

#[cfg(windows)]
/// Update or add an entry in shims.cfg.
///
/// The config file is at ~/.ana/tools/shims.cfg with format:
/// ```
/// pixi=pixi\bin\pixi.exe
/// anaconda=anaconda-cli\Scripts\anaconda.exe
/// ```
fn update_shims_cfg(shim_name: &str, target_path: &str) -> miette::Result<()> {
    let config_path = paths::ana_home().join("tools").join("shims.cfg");

    // Read existing config or start empty
    let mut entries: Vec<(String, String)> = if config_path.exists() {
        std::fs::read_to_string(&config_path)
            .into_diagnostic()
            .context("failed to read shims.cfg")?
            .lines()
            .filter_map(|line| {
                line.split_once('=')
                    .map(|(k, v)| (k.to_string(), v.to_string()))
            })
            .collect()
    } else {
        Vec::new()
    };

    // Update or add the entry
    let mut found = false;
    for (name, path) in &mut entries {
        if name == shim_name {
            *path = target_path.to_string();
            found = true;
            break;
        }
    }
    if !found {
        entries.push((shim_name.to_string(), target_path.to_string()));
    }

    // Write back
    let content: String = entries
        .iter()
        .map(|(k, v)| format!("{}={}\r\n", k, v))
        .collect::<Vec<_>>()
        .join("");

    std::fs::write(&config_path, content)
        .into_diagnostic()
        .context("failed to write shims.cfg")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_lockfile_parse_error() {
        let ctx = CommandContext::new();
        let result =
            install_from_lockfile(&ctx, Path::new("/tmp/test"), "invalid lockfile content").await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("parse"),
            "error should mention parsing: {}",
            err
        );
    }

    #[test]
    fn test_get_package_version_from_lockfile_finds_package() {
        let lock_content = tools::content("anaconda-cli").unwrap();
        let version = get_package_version_from_lockfile(&lock_content, "anaconda-cli-base");
        assert!(version.is_some(), "should find anaconda-cli-base version");
        assert!(
            !version.as_ref().unwrap().is_empty(),
            "version should not be empty"
        );
    }

    #[test]
    fn test_get_package_version_from_lockfile_unknown_package() {
        let lock_content = tools::content("anaconda-cli").unwrap();
        let version = get_package_version_from_lockfile(&lock_content, "nonexistent-package");
        assert!(version.is_none(), "should not find nonexistent package");
    }

    #[test]
    fn test_check_tool_needs_update_nonexistent_tool() {
        temp_env::with_var("ANA_HOME", Some("/nonexistent/path"), || {
            let result = check_tool_needs_update("anaconda-cli", "anaconda-cli-base");
            assert!(result.is_none(), "nonexistent tool should return None");
        });
    }

    #[cfg(windows)]
    mod windows_tests {
        use super::*;
        use tempfile::TempDir;

        #[test]
        fn test_create_bin_shim_creates_exe_and_config() {
            let temp = TempDir::new().unwrap();
            let bin_dir = temp.path().join("bin");
            let tools_dir = temp.path().join("tools");
            let prefix = tools_dir.join("mytool");
            std::fs::create_dir_all(&bin_dir).unwrap();
            std::fs::create_dir_all(prefix.join("bin")).unwrap();

            // Create fake tool binary
            let tool_bin = prefix.join("bin").join("mytool.exe");
            std::fs::write(&tool_bin, "fake binary").unwrap();

            // Set ANA_HOME so shims.cfg goes to our temp dir
            temp_env::with_var("ANA_HOME", Some(temp.path().to_str().unwrap()), || {
                let binary: PathBuf = ["bin", "mytool"].iter().collect();
                let result = create_bin_shim(&bin_dir, &prefix, &binary);
                assert!(result.is_ok(), "create_bin_shim failed: {:?}", result);

                // Check shim exe was created
                let shim_path = bin_dir.join("mytool.exe");
                assert!(shim_path.exists(), "shim exe should exist");

                // Check shims.cfg was created with correct content
                let config_path = tools_dir.join("shims.cfg");
                assert!(config_path.exists(), "shims.cfg should exist");
                let config_content = std::fs::read_to_string(&config_path).unwrap();
                assert!(
                    config_content.contains("mytool=mytool\\bin\\mytool.exe"),
                    "shims.cfg should contain mapping, got: {}",
                    config_content
                );
            });
        }

        #[test]
        fn test_create_bin_shim_skips_missing_binary() {
            let temp = TempDir::new().unwrap();
            let bin_dir = temp.path().join("bin");
            let tools_dir = temp.path().join("tools");
            let prefix = tools_dir.join("mytool");
            std::fs::create_dir_all(&bin_dir).unwrap();
            std::fs::create_dir_all(&prefix).unwrap();

            temp_env::with_var("ANA_HOME", Some(temp.path().to_str().unwrap()), || {
                let binary: PathBuf = ["bin", "nonexistent"].iter().collect();
                let result = create_bin_shim(&bin_dir, &prefix, &binary);
                assert!(result.is_ok(), "should succeed with warning");

                // No shim should be created
                assert!(!bin_dir.join("nonexistent.exe").exists());
            });
        }

        #[test]
        fn test_update_shims_cfg_creates_new_file() {
            let temp = TempDir::new().unwrap();
            let tools_dir = temp.path().join("tools");
            std::fs::create_dir_all(&tools_dir).unwrap();

            temp_env::with_var("ANA_HOME", Some(temp.path().to_str().unwrap()), || {
                let result = update_shims_cfg("pixi", "pixi\\bin\\pixi.exe");
                assert!(result.is_ok());

                let config_path = tools_dir.join("shims.cfg");
                let content = std::fs::read_to_string(&config_path).unwrap();
                assert_eq!(content, "pixi=pixi\\bin\\pixi.exe\r\n");
            });
        }

        #[test]
        fn test_update_shims_cfg_adds_entry() {
            let temp = TempDir::new().unwrap();
            let tools_dir = temp.path().join("tools");
            std::fs::create_dir_all(&tools_dir).unwrap();

            let config_path = tools_dir.join("shims.cfg");
            std::fs::write(&config_path, "existing=path\\to\\existing.exe\r\n").unwrap();

            temp_env::with_var("ANA_HOME", Some(temp.path().to_str().unwrap()), || {
                let result = update_shims_cfg("pixi", "pixi\\bin\\pixi.exe");
                assert!(result.is_ok());

                let content = std::fs::read_to_string(&config_path).unwrap();
                assert!(content.contains("existing=path\\to\\existing.exe\r\n"));
                assert!(content.contains("pixi=pixi\\bin\\pixi.exe\r\n"));
            });
        }

        #[test]
        fn test_update_shims_cfg_updates_existing_entry() {
            let temp = TempDir::new().unwrap();
            let tools_dir = temp.path().join("tools");
            std::fs::create_dir_all(&tools_dir).unwrap();

            let config_path = tools_dir.join("shims.cfg");
            std::fs::write(&config_path, "pixi=old\\path\\pixi.exe\r\n").unwrap();

            temp_env::with_var("ANA_HOME", Some(temp.path().to_str().unwrap()), || {
                let result = update_shims_cfg("pixi", "pixi\\bin\\pixi.exe");
                assert!(result.is_ok());

                let content = std::fs::read_to_string(&config_path).unwrap();
                assert_eq!(content, "pixi=pixi\\bin\\pixi.exe\r\n");
                assert!(!content.contains("old\\path"));
            });
        }
    }
}
