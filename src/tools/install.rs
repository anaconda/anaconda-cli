//! Package installation from lockfiles via rattler.

use std::{path::Path, time::Instant};

use indicatif::{MultiProgress, ProgressDrawTarget};
use miette::{Context, IntoDiagnostic};
use rattler::{
    default_cache_dir,
    install::{IndicatifReporter, Installer},
    package_cache::PackageCache,
};
use rattler_conda_types::{Platform, PrefixRecord};
use rattler_lock::LockFile;

use super::{common, pixi_config, specs};
use crate::context::CommandContext;
use crate::paths;
use crate::ui::status;

/// Global progress bar for installation feedback.
static MULTI_PROGRESS: std::sync::LazyLock<MultiProgress> = std::sync::LazyLock::new(|| {
    let mp = MultiProgress::new();
    mp.set_draw_target(ProgressDrawTarget::stderr_with_hz(20));
    mp
});

/// Returns the names of all currently installed tools.
pub fn installed_tools() -> Vec<&'static str> {
    specs::all_tools()
        .into_iter()
        .filter(|name| paths::tool_prefix(name).exists())
        .collect()
}

/// Update all installed tools that have outdated lockfiles.
///
/// Only updates tools where auto-update is enabled. The global config setting
/// `auto_update_tools` overrides individual tool defaults when set.
///
/// Returns the names of tools that were updated.
pub async fn update_installed_tools(ctx: &mut CommandContext) -> miette::Result<Vec<String>> {
    let mut updated = Vec::new();
    for name in installed_tools() {
        if should_auto_update(ctx, name) && ensure_tool(ctx, name).await? {
            updated.push(name.to_string());
        }
    }
    Ok(updated)
}

/// Check if a tool should be auto-updated.
///
/// If `auto_update_tools` is set in config, use that value for all tools.
/// Otherwise, defer to each tool's default setting.
fn should_auto_update(ctx: &CommandContext, name: &str) -> bool {
    ctx.config
        .auto_update_tools
        .unwrap_or_else(|| specs::auto_update_default(name))
}

/// Ensure a managed tool is installed and up-to-date.
///
/// Returns `true` if an install/update was performed, `false` if already current.
pub async fn ensure_tool(ctx: &mut CommandContext, name: &str) -> miette::Result<bool> {
    let prefix = paths::tool_prefix(name);
    let hash_file = prefix.join(".lockfile-hash");

    let lock_content =
        specs::content(name).ok_or_else(|| miette::miette!("unknown tool: {}", name))?;
    let current_hash = hash_lockfile(&lock_content);

    // Check if tool is installed and lockfile hash matches
    if prefix.exists()
        && let Ok(stored_hash) = std::fs::read_to_string(&hash_file)
        && stored_hash.trim() == current_hash
    {
        return Ok(false);
    }

    install_tool(ctx, name).await?;
    Ok(true)
}

fn hash_lockfile(content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// Install a tool from its lockfile.
pub async fn install_tool(ctx: &mut CommandContext, name: &str) -> miette::Result<()> {
    ctx.telemetry.add("tool_name", name.to_string());

    // Show experimental warning if applicable
    if let Some(msg) = specs::experimental_message(name) {
        if name == "conda" {
            print_conda_experimental_warning();
        } else {
            crate::ui::status::warn(msg);
        }
        eprintln!();
    }

    let prefix = paths::tool_prefix(name);

    let lock_content =
        specs::content(name).ok_or_else(|| miette::miette!("unknown tool: {}", name))?;

    let binaries = specs::binaries(name).unwrap_or_default();
    let uses_wrapper = specs::uses_wrapper(name);

    eprintln!("Installing {} into {}", name, prefix.display());

    install_from_lockfile(ctx, &prefix, &lock_content).await?;

    // Store the lockfile hash for future update checks
    let hash_file = prefix.join(".lockfile-hash");
    std::fs::write(&hash_file, hash_lockfile(&lock_content))
        .into_diagnostic()
        .context("failed to write lockfile hash")?;

    // Create symlinks in bin directory
    common::create_bin_symlinks(&prefix, &binaries, uses_wrapper)?;

    // Tool-specific post-install configuration
    if name == "pixi" {
        pixi_config::configure_default_channels(&paths::bin_path("pixi"))?;
    }

    // For conda, write config and frozen marker
    if name == "conda" {
        common::write_conda_config(&prefix)?;
        common::write_frozen_marker(&prefix)?;
    }

    Ok(())
}

/// Print the experimental warning for the conda tool with styled highlights.
fn print_conda_experimental_warning() {
    status::warn("Conda as a managed tool is experimental.");
    eprintln!(
        "  Uses conda-spawn for activation ({}) instead of conda activate.",
        status::highlight("conda shell <env>")
    );
    eprintln!(
        "  Please report issues with {}, not to conda directly.",
        status::highlight("ana self feedback")
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_lockfile_deterministic() {
        let content = "version: 6\npackages:\n  - name: foo";
        let hash1 = hash_lockfile(content);
        let hash2 = hash_lockfile(content);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_lockfile_different_content() {
        let content1 = "version: 6\npackages:\n  - name: foo";
        let content2 = "version: 6\npackages:\n  - name: bar";
        assert_ne!(hash_lockfile(content1), hash_lockfile(content2));
    }

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
}
