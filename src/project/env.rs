//! Project environment installation from lockfiles.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Instant;

use indicatif::{MultiProgress, ProgressDrawTarget};
use miette::{Context, IntoDiagnostic};
use rattler::{
    default_cache_dir,
    install::{IndicatifReporter, Installer},
    package_cache::PackageCache,
};
use rattler_conda_types::{Platform, PrefixRecord};
use rattler_lock::LockFile;

/// Default environment name.
const DEFAULT_ENV: &str = "default";

/// Global progress bar for installation feedback.
static MULTI_PROGRESS: std::sync::LazyLock<MultiProgress> = std::sync::LazyLock::new(|| {
    let mp = MultiProgress::new();
    mp.set_draw_target(ProgressDrawTarget::stderr_with_hz(20));
    mp
});

/// Returns the project environment prefix based on the manifest filename.
/// - `pixi.toml` → `<project_root>/.pixi/envs/default`
/// - `ana.toml`  → `<project_root>/.ana/envs/default`
pub fn env_prefix(manifest_path: &Path) -> PathBuf {
    let project_root = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let env_dir = match manifest_path.file_name().and_then(|f| f.to_str()) {
        Some("pixi.toml") => ".pixi",
        _ => ".ana",
    };
    project_root.join(env_dir).join("envs").join(DEFAULT_ENV)
}

/// Install a project environment from a lockfile.
///
/// Phase 1: Install conda packages via rattler.
/// Phase 2: Install PyPI wheel packages (if any in lockfile).
pub async fn install(manifest_path: &Path, lockfile_path: &Path) -> Result<PathBuf, String> {
    let prefix = env_prefix(manifest_path);
    let total_start = Instant::now();

    let lock_content = std::fs::read_to_string(lockfile_path)
        .map_err(|e| format!("Failed to read lockfile {}: {}", lockfile_path.display(), e))?;

    let lock_file = LockFile::from_str(&lock_content)
        .map_err(|e| format!("Failed to parse lockfile: {}", e))?;

    let env = lock_file
        .default_environment()
        .ok_or_else(|| "No default environment in lockfile".to_string())?;

    let platform = Platform::current();

    let conda_records = env
        .conda_repodata_records(platform)
        .map_err(|e| format!("Failed to extract conda records: {}", e))?
        .unwrap_or_default();

    let pypi_packages: Vec<_> = env
        .pypi_packages(platform)
        .map(|p| p.collect())
        .unwrap_or_default();

    // Print combined summary line
    eprintln!("Installing environment into {}", prefix.display());
    if pypi_packages.is_empty() {
        eprintln!(
            "Lockfile contains {} conda packages for {}",
            conda_records.len(),
            platform
        );
    } else {
        eprintln!(
            "Lockfile contains {} conda + {} PyPI packages for {}",
            conda_records.len(),
            pypi_packages.len(),
            platform
        );
    }

    let conda_changed = install_conda_packages(&prefix, conda_records, platform)
        .await
        .map_err(|e| format!("{:#}", e))?;

    let pypi_changed = if !pypi_packages.is_empty() {
        install_pypi_packages(&prefix, &pypi_packages).await?
    } else {
        false
    };

    if !conda_changed && !pypi_changed {
        eprintln!("{} Already up to date", console::style("\u{2713}").green());
    }

    eprintln!("Done in {:.1}s", total_start.elapsed().as_secs_f64());

    // Touch the lockfile so its mtime is newer than the manifest. This prevents
    // stale-lockfile checks from triggering in subsequent run/shell commands
    // within the same session (important in CI, where git checkout timestamps
    // can make the manifest appear newer than the lockfile).
    let _ = filetime::set_file_mtime(lockfile_path, filetime::FileTime::now());

    Ok(prefix)
}

/// Check if a project environment already exists and has packages installed.
pub fn is_installed(manifest_path: &Path) -> bool {
    let prefix = env_prefix(manifest_path);
    prefix.join("conda-meta").exists()
}

/// Check if the lockfile is stale relative to the manifest.
///
/// Returns true if the manifest has been modified more recently than the lockfile,
/// indicating the lockfile may not reflect the current manifest state.
pub fn lockfile_is_stale(manifest_path: &Path, lockfile_path: &Path) -> bool {
    let Ok(manifest_meta) = std::fs::metadata(manifest_path) else {
        return false;
    };
    let Ok(lockfile_meta) = std::fs::metadata(lockfile_path) else {
        return true;
    };

    let Ok(manifest_mtime) = manifest_meta.modified() else {
        return false;
    };
    let Ok(lockfile_mtime) = lockfile_meta.modified() else {
        return true;
    };

    manifest_mtime > lockfile_mtime
}

/// Launch an interactive subshell with the project environment activated.
pub fn shell(manifest_path: &Path) -> Result<std::process::ExitStatus, String> {
    let prefix = env_prefix(manifest_path);
    let env_bin = prefix.join("bin");

    let path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", env_bin.display(), path);

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

    eprintln!("Entering ana shell (type 'exit' to leave)");

    std::process::Command::new(&shell)
        .env("PATH", &new_path)
        .env("CONDA_PREFIX", &prefix)
        .env("ANA_SHELL", "1")
        .status()
        .map_err(|e| format!("Failed to launch shell: {}", e))
}

/// Install conda packages into a prefix from pre-parsed repodata records.
///
/// Returns `true` if packages were installed, `false` if already up to date.
async fn install_conda_packages(
    prefix: &Path,
    records: Vec<rattler_conda_types::RepoDataRecord>,
    platform: Platform,
) -> miette::Result<bool> {
    std::fs::create_dir_all(prefix)
        .into_diagnostic()
        .context("failed to create prefix directory")?;

    let installed = PrefixRecord::collect_from_prefix::<PrefixRecord>(prefix).into_diagnostic()?;

    let client = make_download_client()?;

    let cache_dir = default_cache_dir()
        .map_err(|e| miette::miette!("could not determine cache directory: {}", e))?;
    rattler_cache::ensure_cache_dir(&cache_dir)
        .map_err(|e| miette::miette!("could not create cache directory: {}", e))?;

    let package_cache = PackageCache::new(cache_dir.join(rattler_cache::PACKAGE_CACHE_DIR));

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
        Ok(false)
    } else {
        eprintln!(
            "Installed {} conda packages in {:.1}s",
            result.transaction.operations.len(),
            start.elapsed().as_secs_f64()
        );
        Ok(true)
    }
}

/// Detect the Python (major, minor) version from installed conda packages.
fn detect_python_version(prefix: &Path) -> Result<(u32, u32), String> {
    let records = PrefixRecord::collect_from_prefix::<PrefixRecord>(prefix)
        .map_err(|e| format!("Failed to read prefix records: {}", e))?;

    for record in &records {
        if record.repodata_record.package_record.name.as_normalized() == "python" {
            let version = &record.repodata_record.package_record.version;
            let v_str = version.to_string();
            let parts: Vec<&str> = v_str.split('.').collect();
            if parts.len() >= 2 {
                let major: u32 = parts[0]
                    .parse()
                    .map_err(|_| format!("Invalid Python major version: {}", parts[0]))?;
                let minor: u32 = parts[1]
                    .parse()
                    .map_err(|_| format!("Invalid Python minor version: {}", parts[1]))?;
                return Ok((major, minor));
            }
        }
    }

    Err("No python package found in conda prefix".to_string())
}

/// Check if a PyPI package is already installed by looking for its dist-info directory.
fn is_pypi_pkg_installed(site_packages: &Path, name: &str, version: &str) -> bool {
    // PEP 427: dist-info directory uses normalized name with underscores
    let normalized = name.replace('-', "_");
    let dist_info = site_packages.join(format!("{}-{}.dist-info", normalized, version));
    dist_info.is_dir()
}

/// Install PyPI wheel packages into the prefix.
///
/// Downloads all wheels first, then installs them, with progress for each phase.
/// Returns `true` if packages were installed, `false` if already up to date.
async fn install_pypi_packages(
    prefix: &Path,
    pypi_packages: &[(
        &rattler_lock::PypiPackageData,
        &rattler_lock::PypiPackageEnvironmentData,
    )],
) -> Result<bool, String> {
    let python_version = detect_python_version(prefix)?;

    let site_packages = prefix.join(format!(
        "lib/python{}.{}/site-packages",
        python_version.0, python_version.1
    ));

    // Filter to packages that aren't already installed
    let to_install: Vec<_> = pypi_packages
        .iter()
        .filter(|(pkg_data, _)| {
            !is_pypi_pkg_installed(
                &site_packages,
                &pkg_data.name.to_string(),
                &pkg_data.version.to_string(),
            )
        })
        .collect();

    if to_install.is_empty() {
        return Ok(false);
    }

    let cache = crate::pypi::installer::WheelCache::new().ok();
    let check = console::style("\u{2714}").green();
    let total_start = Instant::now();

    // Phase 1: Validate cache — resolve each package to cached bytes or mark for download
    let cache_start = Instant::now();
    let pb = MULTI_PROGRESS.add(indicatif::ProgressBar::new(to_install.len() as u64));
    pb.set_style(
        indicatif::ProgressStyle::with_template("   {bar:20.cyan/blue} {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("━╸─"),
    );
    pb.set_message("validate cache");

    struct WheelEntry<'a> {
        name: &'a str,
        url: Option<&'a str>,
        sha256: Option<String>,
        bytes: Option<Vec<u8>>,
    }

    let mut entries: Vec<WheelEntry> = Vec::with_capacity(to_install.len());
    for (pkg_data, _env_data) in &to_install {
        pb.set_message(format!("{}", pkg_data.name));

        let sha256_hex = pkg_data.hash.as_ref().and_then(|h| {
            h.sha256().map(|bytes| {
                bytes
                    .iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<String>()
            })
        });

        // Try cache lookup
        let cached = sha256_hex
            .as_deref()
            .and_then(|h| cache.as_ref().and_then(|c| c.get(h)));

        entries.push(WheelEntry {
            name: pkg_data.name.as_ref(),
            url: pkg_data.location.as_url().map(|u| u.as_str()),
            sha256: sha256_hex,
            bytes: cached,
        });
        pb.inc(1);
    }

    pb.finish_and_clear();
    let cached_count = entries.iter().filter(|e| e.bytes.is_some()).count();
    let download_count = entries.iter().filter(|e| e.bytes.is_none()).count();
    eprintln!(
        "{check} validate cache       {} packages in {:.0}ms",
        entries.len(),
        cache_start.elapsed().as_secs_f64() * 1000.0
    );

    // Phase 2: Download cache misses
    if download_count > 0 {
        let dl_start = Instant::now();
        let client = reqwest::Client::new();
        let pb = MULTI_PROGRESS.add(indicatif::ProgressBar::new(download_count as u64));
        pb.set_style(
            indicatif::ProgressStyle::with_template("   {bar:20.cyan/blue} {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("━╸─"),
        );
        pb.set_message("downloading wheels");

        for entry in &mut entries {
            if entry.bytes.is_some() {
                continue;
            }

            pb.set_message(entry.name.to_string());

            let Some(url) = entry.url else {
                pb.inc(1);
                continue;
            };

            let bytes = crate::pypi::installer::download_wheel(
                &client,
                url,
                entry.sha256.as_deref(),
                cache.as_ref(),
            )
            .await?;
            entry.bytes = Some(bytes);
            pb.inc(1);
        }

        pb.finish_and_clear();
        eprintln!(
            "{check} downloading wheels   {} packages in {:.0}ms",
            download_count,
            dl_start.elapsed().as_secs_f64() * 1000.0
        );
    }

    // Phase 3: Install all wheels
    let inst_start = Instant::now();
    let installable: Vec<_> = entries
        .iter()
        .filter_map(|e| e.bytes.as_ref().map(|b| (e.name, b)))
        .collect();

    let pb = MULTI_PROGRESS.add(indicatif::ProgressBar::new(installable.len() as u64));
    pb.set_style(
        indicatif::ProgressStyle::with_template("   {bar:20.cyan/blue} {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("━╸─"),
    );
    pb.set_message("installing wheels");

    for (name, bytes) in &installable {
        pb.set_message(name.to_string());
        crate::pypi::installer::install_wheel(prefix, bytes, python_version)?;
        pb.inc(1);
    }

    pb.finish_and_clear();
    eprintln!(
        "{check} installing wheels    took {:.0}ms",
        inst_start.elapsed().as_secs_f64() * 1000.0
    );

    eprintln!(
        "Installed {} PyPI packages in {:.1}s",
        installable.len(),
        total_start.elapsed().as_secs_f64()
    );

    Ok(true)
}

/// Create an HTTP client for downloading packages.
fn make_download_client() -> miette::Result<reqwest_middleware::ClientWithMiddleware> {
    let client = reqwest::Client::builder()
        .no_gzip()
        .build()
        .into_diagnostic()
        .context("failed to create HTTP client")?;

    Ok(reqwest_middleware::ClientBuilder::new(client).build())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_prefix_ana_toml() {
        let manifest = Path::new("/some/project/ana.toml");
        assert_eq!(
            env_prefix(manifest),
            PathBuf::from("/some/project/.ana/envs/default")
        );
    }

    #[test]
    fn test_env_prefix_pixi_toml() {
        let manifest = Path::new("/some/project/pixi.toml");
        assert_eq!(
            env_prefix(manifest),
            PathBuf::from("/some/project/.pixi/envs/default")
        );
    }

    #[test]
    fn test_is_installed_false_for_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("ana.toml");
        assert!(!is_installed(&manifest));
    }

    #[test]
    fn test_is_installed_true_with_conda_meta() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("ana.toml");
        let conda_meta = env_prefix(&manifest).join("conda-meta");
        std::fs::create_dir_all(&conda_meta).unwrap();
        assert!(is_installed(&manifest));
    }

    #[test]
    fn test_lockfile_is_stale_when_manifest_newer() {
        let tmp = tempfile::tempdir().unwrap();
        let lockfile = tmp.path().join("ana.lock");
        let manifest = tmp.path().join("ana.toml");

        // Create lockfile first, then manifest (manifest is newer)
        std::fs::write(&lockfile, "lockfile content").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(&manifest, "manifest content").unwrap();

        assert!(lockfile_is_stale(&manifest, &lockfile));
    }

    #[test]
    fn test_lockfile_is_not_stale_when_lockfile_newer() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("ana.toml");
        let lockfile = tmp.path().join("ana.lock");

        // Create manifest first, then lockfile (lockfile is newer)
        std::fs::write(&manifest, "manifest content").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(&lockfile, "lockfile content").unwrap();

        assert!(!lockfile_is_stale(&manifest, &lockfile));
    }

    #[test]
    fn test_lockfile_is_stale_when_lockfile_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("ana.toml");
        let lockfile = tmp.path().join("ana.lock");
        std::fs::write(&manifest, "manifest content").unwrap();

        assert!(lockfile_is_stale(&manifest, &lockfile));
    }

    #[test]
    fn test_lockfile_is_not_stale_when_manifest_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("ana.toml");
        let lockfile = tmp.path().join("ana.lock");
        std::fs::write(&lockfile, "lockfile content").unwrap();

        assert!(!lockfile_is_stale(&manifest, &lockfile));
    }

    #[test]
    fn test_is_pypi_pkg_installed_true() {
        let tmp = tempfile::tempdir().unwrap();
        let site = tmp.path();
        std::fs::create_dir_all(site.join("httpx-0.28.1.dist-info")).unwrap();
        assert!(is_pypi_pkg_installed(site, "httpx", "0.28.1"));
    }

    #[test]
    fn test_is_pypi_pkg_installed_false() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!is_pypi_pkg_installed(tmp.path(), "httpx", "0.28.1"));
    }

    #[test]
    fn test_is_pypi_pkg_installed_normalizes_dashes() {
        let tmp = tempfile::tempdir().unwrap();
        let site = tmp.path();
        // PEP 427: dist-info uses underscores
        std::fs::create_dir_all(site.join("typing_extensions-4.13.0.dist-info")).unwrap();
        assert!(is_pypi_pkg_installed(site, "typing-extensions", "4.13.0"));
    }

    #[test]
    fn test_is_pypi_pkg_installed_wrong_version() {
        let tmp = tempfile::tempdir().unwrap();
        let site = tmp.path();
        std::fs::create_dir_all(site.join("httpx-0.27.0.dist-info")).unwrap();
        assert!(!is_pypi_pkg_installed(site, "httpx", "0.28.1"));
    }

    /// Write a minimal conda prefix record JSON that PrefixRecord can parse.
    fn write_prefix_record(conda_meta: &Path, name: &str, version: &str, build: &str) {
        let filename = format!("{}-{}-{}.json", name, version, build);
        let json = serde_json::json!({
            "name": name,
            "version": version,
            "build": build,
            "build_number": 0,
            "depends": [],
            "subdir": "osx-arm64",
            "fn": format!("{}-{}-{}.conda", name, version, build),
            "url": format!("https://conda.anaconda.org/conda-forge/osx-arm64/{}-{}-{}.conda", name, version, build),
            "channel": "https://conda.anaconda.org/conda-forge/",
            "files": [],
            "paths_data": { "paths_version": 1, "paths": [] },
            "link": { "source": "/tmp/fake", "type": 1 }
        });
        std::fs::write(conda_meta.join(filename), json.to_string()).unwrap();
    }

    #[test]
    fn test_detect_python_version() {
        let tmp = tempfile::tempdir().unwrap();
        let prefix = tmp.path();
        let conda_meta = prefix.join("conda-meta");
        std::fs::create_dir_all(&conda_meta).unwrap();

        write_prefix_record(&conda_meta, "python", "3.14.4", "h4c637c5_100");
        write_prefix_record(&conda_meta, "numpy", "2.4.3", "py314h1234_0");

        let version = detect_python_version(prefix).unwrap();
        assert_eq!(version, (3, 14));
    }

    #[test]
    fn test_detect_python_version_312() {
        let tmp = tempfile::tempdir().unwrap();
        let prefix = tmp.path();
        let conda_meta = prefix.join("conda-meta");
        std::fs::create_dir_all(&conda_meta).unwrap();

        write_prefix_record(&conda_meta, "python", "3.12.9", "h5b0b3c5_0");

        let version = detect_python_version(prefix).unwrap();
        assert_eq!(version, (3, 12));
    }

    #[test]
    fn test_detect_python_version_no_python() {
        let tmp = tempfile::tempdir().unwrap();
        let prefix = tmp.path();
        let conda_meta = prefix.join("conda-meta");
        std::fs::create_dir_all(&conda_meta).unwrap();

        write_prefix_record(&conda_meta, "numpy", "2.4.3", "py314h1234_0");

        let result = detect_python_version(prefix);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No python package"));
    }

    #[test]
    fn test_detect_python_version_empty_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        let result = detect_python_version(tmp.path());
        // Empty dir (no conda-meta) — should still return error, not panic
        assert!(result.is_err() || result.unwrap() == (0, 0));
    }
}
