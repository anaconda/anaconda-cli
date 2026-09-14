//! Common utilities shared between install backends (rattler and fleet).

use crate::paths;
use miette::{Context, IntoDiagnostic};
use std::path::{Path, PathBuf};

/// Embedded conda wrapper binary (compiled from src/wrappers/conda.rs)
#[cfg(unix)]
const CONDA_WRAPPER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/conda-wrapper"));
#[cfg(windows)]
const CONDA_WRAPPER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/conda-wrapper.exe"));

#[cfg(windows)]
const SHIM_BINARY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/shim.exe"));

/// Create symlinks (Unix) or shims (Windows) for the tool's binaries in ~/.ana/bin/
pub fn create_bin_symlinks(
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
            install_wrapper_binary(&bin_dir, binary)?;
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
fn install_wrapper_binary(bin_dir: &Path, binary: &Path) -> miette::Result<()> {
    let binary_name = binary.file_name().unwrap().to_string_lossy();

    #[cfg(windows)]
    let wrapper_path = bin_dir.join(format!("{}.exe", binary_name));
    #[cfg(not(windows))]
    let wrapper_path = bin_dir.join(binary_name.as_ref());

    if wrapper_path.exists() {
        std::fs::remove_file(&wrapper_path)
            .into_diagnostic()
            .context("failed to remove existing wrapper")?;
    }

    std::fs::write(&wrapper_path, CONDA_WRAPPER)
        .into_diagnostic()
        .with_context(|| format!("failed to write wrapper: {}", wrapper_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&wrapper_path, std::fs::Permissions::from_mode(0o755))
            .into_diagnostic()
            .context("failed to set wrapper permissions")?;
    }

    eprintln!("   Installed wrapper {}", wrapper_path.display());

    Ok(())
}

#[cfg(unix)]
/// Remove broken symlinks left behind in ~/.ana/bin/, e.g. when a tool's
/// install directory was deleted manually instead of via `ana tool uninstall`.
pub fn cleanup_broken_symlinks(bin_dir: &Path) -> miette::Result<()> {
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
fn create_bin_symlink(bin_dir: &Path, prefix: &Path, binary: &Path) -> miette::Result<()> {
    let tool_bin = prefix.join(binary);
    let symlink_path = bin_dir.join(binary.file_name().unwrap());

    if !tool_bin.exists() {
        eprintln!(
            "   Warning: binary '{}' not found in {}",
            binary.display(),
            prefix.display()
        );
        return Ok(());
    }

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
fn create_bin_shim(bin_dir: &Path, prefix: &Path, binary: &Path) -> miette::Result<()> {
    let tool_bin = prefix.join(binary).with_extension("exe");
    let shim_name = binary.file_stem().unwrap().to_string_lossy();
    let shim_path = bin_dir.join(format!("{}.exe", shim_name));

    if !tool_bin.exists() {
        eprintln!(
            "   Warning: binary '{}' not found in {}",
            binary.display(),
            prefix.display()
        );
        return Ok(());
    }

    std::fs::write(&shim_path, SHIM_BINARY)
        .into_diagnostic()
        .with_context(|| format!("failed to write shim: {}", shim_path.display()))?;

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
fn update_shims_cfg(shim_name: &str, target_path: &str) -> miette::Result<()> {
    let config_path = paths::ana_home().join("tools").join("shims.cfg");

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
pub fn write_conda_config(prefix: &Path) -> miette::Result<()> {
    let condarc_path = prefix.join(".condarc");
    let contents = include_str!("../../tool-specs/conda/.condarc");

    std::fs::write(&condarc_path, contents)
        .into_diagnostic()
        .with_context(|| format!("failed to write .condarc: {}", condarc_path.display()))?;

    eprintln!("   Configured conda channels and settings");

    Ok(())
}

/// Write a frozen marker file to protect the conda environment (CEP 22).
pub fn write_frozen_marker(prefix: &Path) -> miette::Result<()> {
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

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    mod unix_tests {
        use super::super::*;
        use tempfile::TempDir;

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
        use super::super::*;
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

            let tool_bin = prefix.join("bin").join("mytool.exe");
            std::fs::write(&tool_bin, "fake binary").unwrap();

            temp_env::with_var("ANA_HOME", Some(temp.path().to_str().unwrap()), || {
                let binary: PathBuf = ["bin", "mytool"].iter().collect();
                let result = create_bin_shim(&bin_dir, &prefix, &binary);
                assert!(result.is_ok(), "create_bin_shim failed: {:?}", result);

                let shim_path = bin_dir.join("mytool.exe");
                assert!(shim_path.exists(), "shim exe should exist");

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
