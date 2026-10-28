//! Tool installation via rattler, from embedded lockfiles or install-time solves.

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

use super::specs::{self, Source};
use super::{pixi_config, solve};
use crate::context::CommandContext;
use crate::paths;

/// Global progress bar for installation feedback.
pub(super) static MULTI_PROGRESS: std::sync::LazyLock<MultiProgress> =
    std::sync::LazyLock::new(|| {
        let mp = MultiProgress::new();
        mp.set_draw_target(ProgressDrawTarget::stderr_with_hz(20));
        mp
    });

/// Returns the names of all currently installed tools.
pub fn installed_tools() -> Vec<&'static str> {
    specs::all_tools()
        .into_iter()
        .filter(|name| {
            let prefix = paths::tool_prefix(name);
            prefix.exists() && binaries_exist(&prefix)
        })
        .collect()
}

/// Update installed tools.
///
/// Locked tools are reinstalled when their embedded lockfile changed, and only
/// if auto-update is enabled (the global config setting `auto_update_tools`
/// overrides individual tool defaults when set). Solved tools are re-solved
/// against their channels unless `locked_only` is set, which `ana self update`
/// uses because a new `ana` binary does not change what a solve produces.
///
/// Returns the names of tools that were updated. A solved tool that fails to
/// re-solve (e.g. offline) keeps its current installation; the remaining tools
/// are still processed and the failure is returned afterwards.
pub async fn update_installed_tools(
    ctx: &mut CommandContext,
    locked_only: bool,
) -> miette::Result<Vec<String>> {
    let mut updated = Vec::new();
    let mut failed = Vec::new();
    for name in installed_tools() {
        let changed = match specs::source(name) {
            Some(Source::Solved { .. }) if !locked_only => {
                match install_solved(ctx, name).await {
                    Ok(changed) => changed,
                    Err(e) => {
                        let cause = e.chain().map(ToString::to_string).collect::<Vec<_>>();
                        crate::ui::status::warn(&format!(
                            "could not update {name}; keeping the installed version: {}",
                            cause.join(": ")
                        ));
                        failed.push(name);
                        false
                    }
                }
            }
            Some(Source::Locked { .. }) if should_auto_update(ctx, name) => {
                ensure_tool(ctx, name).await?
            }
            _ => false,
        };
        if changed {
            updated.push(name.to_string());
        }
    }
    if failed.is_empty() {
        Ok(updated)
    } else {
        Err(miette::miette!("failed to update: {}", failed.join(", ")))
    }
}

/// Check if a locked tool should be auto-updated.
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
/// Locked tools are reinstalled when the embedded lockfile changed. Solved
/// tools are only installed when missing; they change via `ana tool update`.
///
/// Returns `true` if an install/update was performed, `false` if already current.
pub async fn ensure_tool(ctx: &mut CommandContext, name: &str) -> miette::Result<bool> {
    let prefix = paths::tool_prefix(name);

    if specs::is_solved(name) {
        if prefix.exists() && binaries_exist(&prefix) {
            return Ok(false);
        }
        install_tool(ctx, name).await?;
        return Ok(true);
    }

    let hash_file = prefix.join(".lockfile-hash");
    let lock_content =
        specs::content(name).ok_or_else(|| miette::miette!("unknown tool: {}", name))?;
    let current_hash = hash_lockfile(&lock_content);

    // Check if tool is installed and lockfile hash matches
    if prefix.exists()
        && let Ok(stored_hash) = std::fs::read_to_string(&hash_file)
        && stored_hash.trim() == current_hash
    {
        if binaries_exist(&prefix) {
            return Ok(false);
        }

        // Prefix matches the current lockfile but expected binaries are missing
        // — remove the corrupted installation so that install_tool performs a
        // clean reinstall.
        std::fs::remove_dir_all(&prefix)
            .into_diagnostic()
            .with_context(|| {
                format!(
                    "failed to remove corrupted tool directory: {}",
                    prefix.display()
                )
            })?;
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

/// Check that all files rattler linked into the tool's `bin`/`Scripts`
/// directory are still present on disk.
///
/// Uses the `PrefixRecord`s rattler wrote to `conda-meta` as the source of
/// truth for what was actually installed, rather than a hardcoded per-tool
/// binary list, so this works for any tool without needing to know which
/// binaries matter.
pub(super) fn binaries_exist(prefix: &Path) -> bool {
    let Ok(records) = PrefixRecord::collect_from_prefix::<PrefixRecord>(prefix) else {
        return false;
    };
    if records.is_empty() {
        return false;
    }

    let bin_subdir = if cfg!(windows) { "Scripts" } else { "bin" };
    records
        .iter()
        .flat_map(|record| record.paths_data.paths.iter())
        .filter(|entry| entry.relative_path.starts_with(bin_subdir))
        .all(|entry| prefix.join(&entry.relative_path).exists())
}

/// Install a tool from its lockfile or by solving against its channels.
pub async fn install_tool(ctx: &mut CommandContext, name: &str) -> miette::Result<()> {
    ctx.telemetry.add("tool_name", name.to_string());

    // Show experimental warning if applicable
    if let Some(msg) = specs::experimental_message(name) {
        crate::ui::status::warn(msg);
        eprintln!();
    }

    match specs::source(name) {
        None => return Err(miette::miette!("unknown tool: {}", name)),
        Some(Source::Solved { .. }) => {
            install_solved(ctx, name).await?;
        }
        Some(Source::Locked { .. }) => {
            install_locked(ctx, name).await?;
        }
    }

    // Tool-specific post-install configuration
    if name == "pixi" {
        pixi_config::configure_default_channels(&paths::bin_path("pixi"))?;
    }

    Ok(())
}

/// Install a locked tool from its embedded (or overridden) lockfile.
async fn install_locked(ctx: &CommandContext, name: &str) -> miette::Result<()> {
    let prefix = paths::tool_prefix(name);
    let lock_content =
        specs::content(name).ok_or_else(|| miette::miette!("unknown tool: {}", name))?;

    eprintln!("Installing {} into {}", name, prefix.display());

    install_from_lockfile(ctx, &prefix, &lock_content).await?;

    // Store the lockfile hash for future update checks
    let hash_file = prefix.join(".lockfile-hash");
    std::fs::write(&hash_file, hash_lockfile(&lock_content))
        .into_diagnostic()
        .context("failed to write lockfile hash")?;

    create_bin_symlinks(
        &prefix,
        &specs::binaries(name).unwrap_or_default(),
        specs::uses_wrapper(name),
    )
}

/// Solve a solved tool against its channels and install the result.
///
/// Returns `true` if any package was installed, updated, or removed.
async fn install_solved(ctx: &CommandContext, name: &str) -> miette::Result<bool> {
    let Some(Source::Solved { channels, packages }) = specs::source(name) else {
        return Err(miette::miette!("{} is not a solved tool", name));
    };
    let prefix = paths::tool_prefix(name);
    let channels = specs::solve_channels(channels);

    eprintln!("Installing {} into {}", name, prefix.display());
    let changed = solve_into_prefix(ctx, &prefix, &channels, packages).await?;

    create_bin_symlinks(
        &prefix,
        &specs::binaries(name).unwrap_or_default(),
        specs::uses_wrapper(name),
    )?;
    Ok(changed)
}

/// Solve `packages` against `channels` and install the result into `prefix`,
/// preferring packages already installed there.
///
/// Returns `true` if any package was installed, updated, or removed.
pub(crate) async fn solve_into_prefix(
    ctx: &CommandContext,
    prefix: &Path,
    channels: &[String],
    packages: &[&str],
) -> miette::Result<bool> {
    eprintln!("   Solving against {}", channels.join(", "));
    let installed = installed_records(prefix)?;
    let records = solve::solve(channels, packages, &installed).await?;
    install_records(ctx, prefix, records).await
}

/// Records of the packages currently installed in `prefix`.
pub(crate) fn installed_records(prefix: &Path) -> miette::Result<Vec<PrefixRecord>> {
    if !prefix.join("conda-meta").exists() {
        return Ok(Vec::new());
    }
    PrefixRecord::collect_from_prefix::<PrefixRecord>(prefix)
        .into_diagnostic()
        .context("failed to read installed packages")
}

/// HTTP client for rattler (plain reqwest client, not middleware-wrapped).
pub(super) fn rattler_client() -> miette::Result<reqwest::Client> {
    crate::http::configure_client(reqwest::Client::builder())
        .no_gzip()
        .build()
        .into_diagnostic()
        .context("failed to create download client")
}

/// rattler's shared cache directory, created if missing.
pub(super) fn rattler_cache_dir() -> miette::Result<PathBuf> {
    // TODO(mattkram): Consider a custom cache dir
    let cache_dir = default_cache_dir()
        .map_err(|e| miette::miette!("could not determine cache directory: {}", e))?;
    rattler_cache::ensure_cache_dir(&cache_dir)
        .map_err(|e| miette::miette!("could not create cache directory: {}", e))?;
    Ok(cache_dir)
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

    install_records(_ctx, prefix, records).await?;
    Ok(())
}

/// Install exactly `records` into `prefix`, removing anything else.
///
/// Returns `true` if any package was installed, updated, or removed.
async fn install_records(
    _ctx: &CommandContext,
    prefix: &Path,
    records: Vec<rattler_conda_types::RepoDataRecord>,
) -> miette::Result<bool> {
    // Ensure prefix directory exists
    std::fs::create_dir_all(prefix)
        .into_diagnostic()
        .context("failed to create prefix directory")?;

    // Check what's already installed
    let installed = installed_records(prefix)?;

    let cache_dir = rattler_cache_dir()?;
    let package_cache = PackageCache::new(cache_dir.join(rattler_cache::PACKAGE_CACHE_DIR));

    // Run installation
    let start = Instant::now();
    let result = Installer::new()
        .with_download_client(rattler_client()?)
        .with_package_cache(package_cache)
        .with_target_platform(Platform::current())
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

    let operations = result.transaction.operations.len();
    if operations == 0 {
        eprintln!("   ✓ Already up to date");
    } else {
        eprintln!(
            "   Installed {} packages in {:.1}s",
            operations,
            start.elapsed().as_secs_f64()
        );
    }

    Ok(operations > 0)
}

/// Embedded kilo wrapper binary (compiled from src/wrappers/kilo.rs)
#[cfg(unix)]
const KILO_WRAPPER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/kilo-wrapper"));
#[cfg(windows)]
const KILO_WRAPPER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/kilo-wrapper.exe"));

/// Create symlinks (Unix) or shims (Windows) for the tool's binaries in
/// ~/.ana/bin/, or install the standalone wrapper for tools that use one.
fn create_bin_symlinks(
    prefix: &Path,
    binaries: &[PathBuf],
    uses_wrapper: bool,
) -> miette::Result<()> {
    let bin_dir = paths::bin_dir();
    std::fs::create_dir_all(&bin_dir)
        .into_diagnostic()
        .context("failed to create bin directory")?;

    #[cfg(unix)]
    cleanup_broken_symlinks(&bin_dir)?;

    for binary in binaries {
        if uses_wrapper {
            install_wrapper_binary(&bin_dir, prefix, binary)?;
        } else {
            #[cfg(unix)]
            create_bin_symlink(&bin_dir, prefix, binary)?;
            #[cfg(windows)]
            create_bin_shim(&bin_dir, prefix, binary)?;
        }
    }

    Ok(())
}

/// Install the embedded wrapper binary for a tool.
///
/// Also records the path of the running `ana` executable in the tool prefix
/// so the wrapper can dispatch `kilo ana ...` to the same binary.
fn install_wrapper_binary(bin_dir: &Path, prefix: &Path, binary: &Path) -> miette::Result<()> {
    let binary_name = binary.file_name().unwrap().to_string_lossy();
    let wrapper_path = bin_dir.join(paths::binary_name(&binary_name));

    if wrapper_path.exists() || wrapper_path.is_symlink() {
        std::fs::remove_file(&wrapper_path)
            .into_diagnostic()
            .context("failed to remove existing wrapper")?;
    }

    std::fs::write(&wrapper_path, KILO_WRAPPER)
        .into_diagnostic()
        .with_context(|| format!("failed to write wrapper: {}", wrapper_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&wrapper_path, std::fs::Permissions::from_mode(0o755))
            .into_diagnostic()
            .context("failed to set wrapper permissions")?;
    }

    let ana_exe = std::env::current_exe()
        .into_diagnostic()
        .context("failed to determine the ana executable path")?;
    std::fs::write(prefix.join(".ana-exe"), ana_exe.to_string_lossy().as_bytes())
        .into_diagnostic()
        .context("failed to record the ana executable path")?;

    eprintln!("   Installed wrapper {}", wrapper_path.display());

    Ok(())
}

#[cfg(unix)]
/// Remove broken symlinks left behind in ~/.ana/bin/, e.g. when a tool's
/// install directory was deleted manually instead of via `ana tool uninstall`.
pub(super) fn cleanup_broken_symlinks(bin_dir: &Path) -> miette::Result<()> {
    if !bin_dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(bin_dir)
        .into_diagnostic()
        .context("failed to read bin directory")?
    {
        let path = entry.into_diagnostic()?.path();
        // Only treat a missing target as broken; leave symlinks alone if the
        // target merely couldn't be stat'd (e.g. a permission error), since
        // `Path::exists()` can't distinguish the two cases.
        let target_missing = matches!(
            std::fs::metadata(&path),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound
        );
        if path.is_symlink() && target_missing {
            std::fs::remove_file(&path)
                .into_diagnostic()
                .with_context(|| format!("failed to remove broken symlink: {}", path.display()))?;
            eprintln!("   Removed broken symlink: {}", path.display());
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
    use tempfile::TempDir;

    #[test]
    fn test_hash_lockfile_deterministic() {
        let content = "version: 7\npackages:\n  - name: foo";
        let hash1 = hash_lockfile(content);
        let hash2 = hash_lockfile(content);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_lockfile_different_content() {
        let content1 = "version: 7\npackages:\n  - name: foo";
        let content2 = "version: 7\npackages:\n  - name: bar";
        assert_ne!(hash_lockfile(content1), hash_lockfile(content2));
    }

    /// Writes a fake `PrefixRecord` into `<prefix>/conda-meta`, as if rattler
    /// had linked a package with the given relative file paths.
    fn write_fake_prefix_record(prefix: &Path, package_name: &str, relative_paths: &[&str]) {
        use rattler_conda_types::package::DistArchiveIdentifier;
        use rattler_conda_types::prefix_record::{PathType, PathsEntry};
        use rattler_conda_types::{PackageName, PackageRecord, RepoDataRecord, Version};

        let package_record = PackageRecord::new(
            PackageName::new_unchecked(package_name),
            Version::major(1),
            "0".to_string(),
        );
        let identifier =
            DistArchiveIdentifier::try_from_filename(&format!("{package_name}-1-0.conda")).unwrap();
        let repodata_record = RepoDataRecord {
            package_record,
            identifier,
            url: url::Url::parse("file:///dev/null").unwrap(),
            channel: None,
        };
        let paths = relative_paths
            .iter()
            .map(|p| PathsEntry {
                relative_path: PathBuf::from(p),
                original_path: None,
                path_type: PathType::HardLink,
                no_link: false,
                sha256: None,
                sha256_in_prefix: None,
                size_in_bytes: None,
                file_mode: None,
                prefix_placeholder: None,
            })
            .collect();
        let record = PrefixRecord::from_repodata_record(repodata_record, paths);

        let conda_meta = prefix.join("conda-meta");
        std::fs::create_dir_all(&conda_meta).unwrap();
        record
            .write_to_path(conda_meta.join(record.file_name()), false)
            .unwrap();
    }

    #[test]
    fn test_binaries_exist_checks_linked_binary() {
        let temp = TempDir::new().unwrap();
        let bin_subdir = if cfg!(windows) { "Scripts" } else { "bin" };
        let relative = format!("{bin_subdir}/anaconda");
        write_fake_prefix_record(temp.path(), "anaconda-cli-base", &[&relative]);

        let binary = temp.path().join(&relative);
        std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
        std::fs::write(&binary, "binary").unwrap();

        assert!(binaries_exist(temp.path()));
    }

    #[test]
    fn test_binaries_exist_detects_missing_linked_binary() {
        let temp = TempDir::new().unwrap();
        let bin_subdir = if cfg!(windows) { "Scripts" } else { "bin" };
        write_fake_prefix_record(
            temp.path(),
            "anaconda-cli-base",
            &[&format!("{bin_subdir}/anaconda")],
        );

        // Binary is recorded in conda-meta but was never created on disk.
        assert!(!binaries_exist(temp.path()));
    }

    #[test]
    fn test_binaries_exist_false_when_no_conda_meta() {
        let temp = TempDir::new().unwrap();
        assert!(!binaries_exist(temp.path()));
    }

    #[test]
    fn test_binaries_exist_ignores_non_bin_files() {
        let temp = TempDir::new().unwrap();
        // A package that only installs files outside bin/Scripts (e.g. a
        // library-only dependency) shouldn't affect the binary check.
        write_fake_prefix_record(temp.path(), "some-lib", &["lib/some-lib.so"]);

        assert!(binaries_exist(temp.path()));
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

    #[cfg(unix)]
    mod unix_tests {
        use super::*;

        #[test]
        fn test_cleanup_broken_symlinks_removes_broken_only() {
            let temp = TempDir::new().unwrap();
            let bin_dir = temp.path().join("bin");
            std::fs::create_dir_all(&bin_dir).unwrap();

            // A valid symlink pointing to a file that exists
            let target = temp.path().join("target");
            std::fs::write(&target, "binary").unwrap();
            let valid_link = bin_dir.join("valid");
            std::os::unix::fs::symlink(&target, &valid_link).unwrap();

            // A broken symlink pointing to a file that doesn't exist
            let broken_link = bin_dir.join("broken");
            std::os::unix::fs::symlink(temp.path().join("missing"), &broken_link).unwrap();

            cleanup_broken_symlinks(&bin_dir).unwrap();

            assert!(valid_link.exists(), "valid symlink should remain");
            assert!(
                !broken_link.is_symlink(),
                "broken symlink should be removed"
            );
        }

        #[test]
        fn test_cleanup_broken_symlinks_missing_bin_dir_is_ok() {
            let temp = TempDir::new().unwrap();
            let bin_dir = temp.path().join("does-not-exist");
            assert!(cleanup_broken_symlinks(&bin_dir).is_ok());
        }

        #[test]
        fn test_cleanup_broken_symlinks_keeps_unreadable_target() {
            use std::os::unix::fs::PermissionsExt;

            let temp = TempDir::new().unwrap();
            let bin_dir = temp.path().join("bin");
            std::fs::create_dir_all(&bin_dir).unwrap();

            // A symlink whose target exists but sits behind a directory with
            // no execute permission, so stat()-ing it fails with
            // PermissionDenied rather than NotFound. This must NOT be
            // treated as broken.
            let locked_dir = temp.path().join("locked");
            std::fs::create_dir_all(&locked_dir).unwrap();
            let target = locked_dir.join("target");
            std::fs::write(&target, "binary").unwrap();
            let link = bin_dir.join("tool");
            std::os::unix::fs::symlink(&target, &link).unwrap();

            std::fs::set_permissions(&locked_dir, std::fs::Permissions::from_mode(0o000)).unwrap();
            let result = cleanup_broken_symlinks(&bin_dir);
            std::fs::set_permissions(&locked_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

            result.unwrap();
            assert!(
                link.is_symlink(),
                "symlink with unreadable (not missing) target should be kept"
            );
        }
    }

    #[cfg(windows)]
    mod windows_tests {
        use super::*;
        use serial_test::serial;
        use tempfile::TempDir;

        #[test]
        #[serial(env)]
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
        #[serial(env)]
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
        #[serial(env)]
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
        #[serial(env)]
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
        #[serial(env)]
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
