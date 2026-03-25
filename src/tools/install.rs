//! Package installation from lockfiles via rattler.

use std::{path::Path, str::FromStr, time::Instant};

use indicatif::{MultiProgress, ProgressDrawTarget};
use miette::{Context, IntoDiagnostic};
use rattler::{
    default_cache_dir,
    install::{IndicatifReporter, Installer},
    package_cache::PackageCache,
};
use rattler_conda_types::{Platform, PrefixRecord};
use rattler_lock::LockFile;

use crate::paths;
use super::lockfiles;

/// Global progress bar for installation feedback.
static MULTI_PROGRESS: std::sync::LazyLock<MultiProgress> = std::sync::LazyLock::new(|| {
    let mp = MultiProgress::new();
    mp.set_draw_target(ProgressDrawTarget::stderr_with_hz(20));
    mp
});

/// Install a tool from its lockfile.
pub async fn install_tool(name: &str) -> miette::Result<()> {
    let prefix = paths::tool_prefix(name);

    let lock_content = lockfiles::content(name)
        .ok_or_else(|| miette::miette!("unknown tool: {}", name))?;

    eprintln!("Installing {} into {}", name, prefix.display());

    install_from_lockfile(&prefix, &lock_content).await?;

    // Create symlink in bin directory
    create_bin_symlink(name, &prefix)?;

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

/// Create a symlink for the tool's binary in ~/.ana/bin/
fn create_bin_symlink(name: &str, prefix: &Path) -> miette::Result<()> {
    let bin_dir = paths::bin_dir();
    std::fs::create_dir_all(&bin_dir)
        .into_diagnostic()
        .context("failed to create bin directory")?;

    // TODO(mattkram): The binary or binaries is not always the name. We need to either
    // inspect the top level package, or expose via our own custom configuration.
    let tool_bin = prefix.join("bin").join(name);
    let symlink_path = bin_dir.join(name);

    // Check if the tool binary exists
    if !tool_bin.exists() {
        eprintln!(
            "   Warning: binary '{}' not found in {}/bin/",
            name,
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

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&tool_bin, &symlink_path)
            .into_diagnostic()
            .with_context(|| format!("failed to create symlink: {}", symlink_path.display()))?;
    }

    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(&tool_bin, &symlink_path)
            .into_diagnostic()
            .with_context(|| format!("failed to create symlink: {}", symlink_path.display()))?;
    }

    eprintln!(
        "   Linked {} -> {}",
        symlink_path.display(),
        tool_bin.display()
    );

    Ok(())
}

/// Create an HTTP client for downloading packages.
fn make_download_client() -> reqwest_middleware::ClientWithMiddleware {
    let client = reqwest::Client::builder()
        .no_gzip()
        .build()
        .expect("failed to create HTTP client");

    // TODO: Add AuthenticationMiddleware for private channel support
    reqwest_middleware::ClientBuilder::new(client).build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lockfile_parse_error() {
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(install_from_lockfile(
                Path::new("/tmp/test"),
                "invalid lockfile content",
            ));

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("parse"),
            "error should mention parsing: {}",
            err
        );
    }
}
