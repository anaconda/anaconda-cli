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
    let uses_wrapper = tools::uses_wrapper(name);

    eprintln!("Installing {} into {}", name, prefix.display());

    install_from_lockfile(ctx, &prefix, &lock_content).await?;

    // Create symlinks in bin directory
    create_bin_symlinks(&prefix, &binaries, uses_wrapper)?;

    // Tool-specific post-install configuration
    if name == "pixi" {
        pixi_config::configure_default_channels(&paths::bin_path("pixi"))?;
    }

    // For conda, write config and frozen marker
    if name == "conda" {
        write_conda_config(&prefix)?;
        write_frozen_marker(&prefix)?;
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
fn create_bin_symlinks(
    prefix: &Path,
    binaries: &[PathBuf],
    uses_wrapper: bool,
) -> miette::Result<()> {
    let bin_dir = paths::bin_dir();
    std::fs::create_dir_all(&bin_dir)
        .into_diagnostic()
        .context("failed to create bin directory")?;

    for binary in binaries {
        if uses_wrapper {
            create_wrapper_symlink(&bin_dir, binary)?;
        } else {
            #[cfg(unix)]
            create_bin_symlink(&bin_dir, prefix, binary)?;
            #[cfg(windows)]
            create_bin_shim(&bin_dir, prefix, binary)?;
        }
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

/// Create a symlink that points to the ana binary itself.
///
/// This is used for tools that ana wraps (like conda), where ana detects
/// the binary name and acts as a wrapper for the underlying tool.
fn create_wrapper_symlink(bin_dir: &Path, binary: &str) -> miette::Result<()> {
    let symlink_path = bin_dir.join(binary);

    // Get the path to the current ana executable
    let ana_bin = std::env::current_exe()
        .into_diagnostic()
        .context("failed to get current executable path")?;

    // Remove existing symlink if present
    if symlink_path.exists() || symlink_path.is_symlink() {
        std::fs::remove_file(&symlink_path)
            .into_diagnostic()
            .context("failed to remove existing symlink")?;
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&ana_bin, &symlink_path)
            .into_diagnostic()
            .with_context(|| format!("failed to create symlink: {}", symlink_path.display()))?;
    }

    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(&ana_bin, &symlink_path)
            .into_diagnostic()
            .with_context(|| format!("failed to create symlink: {}", symlink_path.display()))?;
    }

    eprintln!(
        "   Linked {} -> {} (wrapper)",
        symlink_path.display(),
        ana_bin.display()
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

/// Write .condarc configuration for the conda environment.
///
/// This sets up the default channels (similar to miniconda) and other
/// ana-specific configuration. The config is stored in lockfiles/conda/.condarc
/// and compiled into the binary.
fn write_conda_config(prefix: &Path) -> miette::Result<()> {
    let condarc_path = prefix.join(".condarc");
    let contents = include_str!("../../lockfiles/conda/.condarc");

    std::fs::write(&condarc_path, contents)
        .into_diagnostic()
        .with_context(|| format!("failed to write .condarc: {}", condarc_path.display()))?;

    eprintln!("   Configured conda channels and settings");

    Ok(())
}

/// Write a frozen marker file to protect the conda environment (CEP 22).
///
/// This prevents users from accidentally modifying the tool's environment
/// with `conda install`. They should use `conda self install` instead.
fn write_frozen_marker(prefix: &Path) -> miette::Result<()> {
    let conda_meta = prefix.join("conda-meta");
    std::fs::create_dir_all(&conda_meta)
        .into_diagnostic()
        .context("failed to create conda-meta directory")?;

    let frozen_path = conda_meta.join("frozen");
    let contents = serde_json::json!({
        "message": concat!(
            "This environment is managed by ana.\n",
            "To install packages, use: conda self install <package>\n",
            "To update conda, use: conda self update\n",
            "To override, pass --override-frozen to conda commands."
        )
    });

    std::fs::write(
        &frozen_path,
        serde_json::to_string_pretty(&contents).unwrap(),
    )
    .into_diagnostic()
    .with_context(|| format!("failed to write frozen marker: {}", frozen_path.display()))?;

    eprintln!("   Froze environment to prevent accidental modifications");

    Ok(())
}

/// Create an HTTP client for downloading packages.
fn make_download_client() -> reqwest_middleware::ClientWithMiddleware {
    // TODO: Add AuthenticationMiddleware for private channel support
    crate::http::build_client(reqwest::Client::builder().no_gzip())
        .expect("failed to create HTTP client")
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
