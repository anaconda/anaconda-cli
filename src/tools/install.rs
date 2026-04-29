//! Package installation from lockfiles via rattler.

use std::{path::Path, path::PathBuf, process::Command, str::FromStr, time::Instant};

use indicatif::{MultiProgress, ProgressDrawTarget};
use miette::{Context, IntoDiagnostic};
use rattler::{
    default_cache_dir,
    install::{IndicatifReporter, Installer},
    package_cache::PackageCache,
};
use rattler_conda_types::{Platform, PrefixRecord};
use rattler_lock::{LockFile, UrlOrPath};
use sha2::{Digest, Sha256};

use super::{pixi_config, tools};
use crate::context::CommandContext;
use crate::paths;

/// Filename for storing the lockfile hash in the tool prefix.
const LOCKFILE_HASH_FILENAME: &str = ".lockfile-hash";

/// Global progress bar for installation feedback.
static MULTI_PROGRESS: std::sync::LazyLock<MultiProgress> = std::sync::LazyLock::new(|| {
    let mp = MultiProgress::new();
    mp.set_draw_target(ProgressDrawTarget::stderr_with_hz(20));
    mp
});

/// Compute SHA-256 hash of content and return as hex string.
fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    hex_encode(&result)
}

/// Encode bytes as lowercase hex string.
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Read the stored lockfile hash from the tool prefix.
fn read_stored_hash(prefix: &Path) -> Option<String> {
    let hash_file = prefix.join(LOCKFILE_HASH_FILENAME);
    std::fs::read_to_string(hash_file)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Write the lockfile hash to the tool prefix.
fn write_hash(prefix: &Path, hash: &str) -> miette::Result<()> {
    let hash_file = prefix.join(LOCKFILE_HASH_FILENAME);
    std::fs::write(&hash_file, hash)
        .into_diagnostic()
        .context("failed to write lockfile hash")?;
    Ok(())
}

/// Install a tool from its lockfile.
pub async fn install_tool(ctx: &mut CommandContext, name: &str) -> miette::Result<()> {
    ctx.telemetry.add("tool_name", name.to_string());

    let prefix = paths::tool_prefix(name);

    let lock_content =
        tools::content(name).ok_or_else(|| miette::miette!("unknown tool: {}", name))?;

    let current_hash = compute_hash(&lock_content);
    let binaries = tools::binaries(name).unwrap_or(Vec::new());

    // Check if already up-to-date
    if let Some(stored_hash) = read_stored_hash(&prefix) {
        if stored_hash == current_hash {
            eprintln!("{} is already up to date", name);
            return Ok(());
        }
        eprintln!("Updating {} in {}", name, prefix.display());
    } else {
        eprintln!("Installing {} into {}", name, prefix.display());
    }

    install_from_lockfile(&prefix, &lock_content).await?;

    // Store the hash after successful installation
    write_hash(&prefix, &current_hash)?;

    // Create symlinks in bin directory
    create_bin_symlinks(&prefix, &binaries)?;

    // Tool-specific post-install configuration
    if name == "pixi" {
        pixi_config::configure_default_channels(&paths::bin_path("pixi"))?;
    }

    Ok(())
}

/// Install packages from a lockfile string to a prefix.
pub async fn install_from_lockfile(prefix: &Path, lock_content: &str) -> miette::Result<()> {
    let lock_file = LockFile::from_str(lock_content)
        .into_diagnostic()
        .context("failed to parse lockfile")?;

    let env = lock_file
        .default_environment()
        .ok_or_else(|| miette::miette!("lockfile has no default environment"))?;

    let platform = Platform::current();
    let records = env
        .conda_repodata_records(platform)
        .into_diagnostic()
        .context("failed to extract records from lockfile")?
        .ok_or_else(|| miette::miette!("lockfile has no records for platform {}", platform))?;

    eprintln!(
        "   Lockfile contains {} packages for {}",
        records.len(),
        platform
    );

    // Ensure prefix directory exists
    std::fs::create_dir_all(prefix)
        .into_diagnostic()
        .context("failed to create prefix directory")?;

    // Check what's already installed
    let installed = PrefixRecord::collect_from_prefix::<PrefixRecord>(prefix).into_diagnostic()?;

    // Build HTTP client
    let client = make_download_client();

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
        .with_target_platform(platform)
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
        eprintln!("   ✓ Conda packages already up to date");
    } else {
        eprintln!(
            "   Installed {} conda packages in {:.1}s",
            result.transaction.operations.len(),
            start.elapsed().as_secs_f64()
        );
    }

    // Install PyPI packages via pip
    install_pypi_packages(prefix, &env, platform)?;

    Ok(())
}

/// Install PyPI packages from the lockfile using pip.
fn install_pypi_packages(
    prefix: &Path,
    env: &rattler_lock::Environment,
    platform: Platform,
) -> miette::Result<()> {
    let Some(pypi_iter) = env.pypi_packages(platform) else {
        return Ok(());
    };
    let pypi_packages: Vec<_> = pypi_iter.collect();

    if pypi_packages.is_empty() {
        return Ok(());
    }

    eprintln!(
        "   Installing {} PyPI packages via pip",
        pypi_packages.len()
    );

    let python_bin = prefix.join("bin").join("python");
    if !python_bin.exists() {
        return Err(miette::miette!(
            "python not found at {}",
            python_bin.display()
        ));
    }

    // Collect URLs for all PyPI packages
    let urls: Vec<String> = pypi_packages
        .iter()
        .filter_map(|(data, _env_data)| match &data.location {
            UrlOrPath::Url(url) => Some(url.to_string()),
            UrlOrPath::Path(_) => None, // Skip local paths for now
        })
        .collect();

    if urls.is_empty() {
        return Ok(());
    }

    let start = Instant::now();
    let status = Command::new(&python_bin)
        .args(["-m", "pip", "install", "--quiet", "--no-deps"])
        .args(&urls)
        .status()
        .into_diagnostic()
        .context("failed to run pip")?;

    if !status.success() {
        return Err(miette::miette!(
            "pip install failed with exit code {}",
            status.code().unwrap_or(1)
        ));
    }

    eprintln!(
        "   Installed {} PyPI packages in {:.1}s",
        urls.len(),
        start.elapsed().as_secs_f64()
    );

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
        let result =
            install_from_lockfile(Path::new("/tmp/test"), "invalid lockfile content").await;

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
