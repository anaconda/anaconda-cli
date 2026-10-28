//! List available tools and their installation status.

use crate::context::CommandContext;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::paths;
use crate::table::{self, Color};

use super::specs;

/// Installation status of a tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallStatus {
    /// Installed and managed by `ana` (`~/.ana/tools/`), or present in the
    /// conda environment for the `conda-package` build.
    Managed,
    /// Not managed by `ana`, but one of the tool's binaries was found on
    /// `PATH` (e.g. pixi installed via its own install script).
    External(PathBuf),
    /// Not installed.
    NotInstalled,
}

/// Information about a tool for display.
pub struct ToolInfo {
    pub name: &'static str,
    pub status: InstallStatus,
    pub binaries: Vec<PathBuf>,
}

/// An externally-managed installer. Unlike the rattler-managed tools above,
/// these are downloaded (and SHA256-verified) into the current directory by
/// `ana tool download`, then installed by the user — `ana` does not manage the
/// resulting installation.
struct Installer {
    name: &'static str,
    /// Version offered by the download, or `None` if not yet available. Only
    /// "latest" is supported today.
    version: Option<&'static str>,
    /// Download command arguments after the program name, or `None` if not
    /// yet available.
    command: Option<&'static str>,
    status: &'static str,
}

/// Externally-managed installers shown in the second table.
const INSTALLERS: &[Installer] = &[
    Installer {
        name: "miniconda",
        version: Some("latest"),
        command: Some("tool download miniconda"),
        status: "available",
    },
    Installer {
        name: "anaconda",
        version: None,
        command: None,
        status: "coming soon",
    },
];

/// Check if a tool is installed as a conda package in the current environment.
#[cfg(not(tool_install))]
fn is_conda_package_installed(name: &str) -> bool {
    let Some(prefix) = paths::conda_prefix() else {
        return false;
    };

    let conda_meta = prefix.join("conda-meta");
    std::fs::read_dir(&conda_meta)
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{name}-"))
            })
        })
        .unwrap_or(false)
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// Search `path_var` (a `PATH`-style list) for the first executable matching
/// any of `binary_names`, ignoring anything that lives in or resolves into
/// `ana_home` (i.e. `ana`'s own symlinks/shims and managed tool prefixes).
///
/// Empty and relative `PATH` entries are skipped so the result never depends
/// on the current working directory.
fn find_external_binary(
    binary_names: &[&str],
    path_var: Option<&OsStr>,
    ana_home: &Path,
) -> Option<PathBuf> {
    if binary_names.is_empty() {
        return None;
    }
    let path_var = path_var?;
    let ana_home_canonical = ana_home.canonicalize().ok();
    let is_ana_owned = |p: &Path| {
        p.starts_with(ana_home)
            || ana_home_canonical
                .as_deref()
                .is_some_and(|home| p.starts_with(home))
    };

    std::env::split_paths(path_var)
        .filter(|dir| dir.is_absolute())
        .flat_map(|dir| {
            binary_names
                .iter()
                .map(move |name| dir.join(paths::binary_name(name)))
        })
        .find(|candidate| {
            if !is_executable(candidate) || is_ana_owned(candidate) {
                return false;
            }
            // Follow symlinks so e.g. a link into ~/.ana/tools/ isn't
            // reported as an external installation.
            candidate
                .canonicalize()
                .map_or(true, |resolved| !is_ana_owned(&resolved))
        })
}

/// List all available tools with their installation status.
///
/// When built with `conda-package` feature, managed status reflects the
/// conda environment's conda-meta entries rather than `~/.ana/tools/`.
/// Tools that are not managed but whose binaries are found on `PATH` are
/// reported as [`InstallStatus::External`].
pub fn list_tools() -> Vec<ToolInfo> {
    #[cfg(all(unix, tool_install))]
    if let Err(err) = super::install::cleanup_broken_symlinks(&paths::bin_dir()) {
        crate::ui::status::warn(&format!("failed to clean up broken symlinks: {err}"));
    }

    let path_var = std::env::var_os("PATH");
    let ana_home = paths::ana_home();

    specs::all_tools()
        .iter()
        .map(|name| {
            #[cfg(tool_install)]
            let managed = {
                let prefix = paths::tool_prefix(name);
                prefix.exists() && super::install::binaries_exist(&prefix)
            };
            #[cfg(not(tool_install))]
            let managed = is_conda_package_installed(name);

            let status = if managed {
                InstallStatus::Managed
            } else if let Some(path) = find_external_binary(
                &specs::binary_names(name).unwrap_or_default(),
                path_var.as_deref(),
                &ana_home,
            ) {
                InstallStatus::External(path)
            } else {
                InstallStatus::NotInstalled
            };

            let binaries = specs::binaries(name).unwrap_or_default();
            ToolInfo {
                name,
                status,
                binaries,
            }
        })
        .collect()
}

/// Print the tool list as a formatted table.
pub fn print_tool_list(_ctx: &mut CommandContext) {
    let tools = list_tools();

    let mut table = table::new(["Name", "Installed", "Binaries"]);
    let mut external = Vec::new();

    for tool in tools {
        let status_cell = match &tool.status {
            InstallStatus::Managed => table::cell("✓").fg(Color::Green),
            InstallStatus::External(path) => {
                external.push((tool.name, path.clone()));
                table::cell("external").fg(Color::Yellow)
            }
            InstallStatus::NotInstalled => table::cell("✗").fg(Color::Red),
        };
        let binaries = tool
            .binaries
            .iter()
            .filter_map(|b| b.file_stem().and_then(|s| s.to_str()))
            .collect::<Vec<_>>()
            .join(", ");
        table.add_row([table::cell(tool.name), status_cell, table::cell(&binaries)]);
    }

    println!("{table}");

    if !external.is_empty() {
        println!();
        println!("External tools were found on PATH but are not managed by ana:");
        for (name, path) in external {
            println!("  {name}: {}", path.display());
        }
    }

    print_installer_list();
}

/// Print the externally-managed installer table.
fn print_installer_list() {
    let mut table = table::new(["Name", "Version", "Download command", "Status"]);

    for installer in INSTALLERS {
        table.add_row([
            table::cell(installer.name),
            table::cell(installer.version.unwrap_or("n/a")),
            table::cell(installer.command.map_or("n/a".to_string(), crate::prog::cmd)),
            table::cell(installer.status),
        ]);
    }

    println!();
    println!("Externally Managed Installers");
    println!("{table}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use tempfile::TempDir;

    /// Create a fake executable named `name` in `dir`.
    fn make_executable(dir: &Path, name: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(paths::binary_name(name));
        std::fs::write(&path, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    fn path_var(dirs: &[&Path]) -> OsString {
        std::env::join_paths(dirs).unwrap()
    }

    #[test]
    fn test_find_external_binary_found_on_path() {
        let temp = TempDir::new().unwrap();
        let ana_home = temp.path().join(".ana");
        let ext_bin = temp.path().join(".pixi").join("bin");
        let pixi = make_executable(&ext_bin, "pixi");

        let found = find_external_binary(&["pixi"], Some(&path_var(&[&ext_bin])), &ana_home);
        assert_eq!(found, Some(pixi));
    }

    #[test]
    fn test_find_external_binary_respects_path_order() {
        let temp = TempDir::new().unwrap();
        let ana_home = temp.path().join(".ana");
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        let expected = make_executable(&first, "pixi");
        make_executable(&second, "pixi");

        let found = find_external_binary(&["pixi"], Some(&path_var(&[&first, &second])), &ana_home);
        assert_eq!(found, Some(expected));
    }

    #[test]
    fn test_find_external_binary_not_found() {
        let temp = TempDir::new().unwrap();
        let ana_home = temp.path().join(".ana");
        let empty = temp.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();

        assert_eq!(
            find_external_binary(&["pixi"], Some(&path_var(&[&empty])), &ana_home),
            None
        );
    }

    #[test]
    fn test_find_external_binary_no_path_var() {
        let temp = TempDir::new().unwrap();
        assert_eq!(find_external_binary(&["pixi"], None, temp.path()), None);
    }

    #[test]
    fn test_find_external_binary_no_binaries() {
        let temp = TempDir::new().unwrap();
        let ext_bin = temp.path().join("bin");
        make_executable(&ext_bin, "anaconda");

        assert_eq!(
            find_external_binary(&[], Some(&path_var(&[&ext_bin])), temp.path()),
            None
        );
    }

    #[test]
    fn test_find_external_binary_ignores_ana_bin_dir() {
        let temp = TempDir::new().unwrap();
        let ana_home = temp.path().join(".ana");
        let ana_bin = ana_home.join("bin");
        make_executable(&ana_bin, "pixi");

        assert_eq!(
            find_external_binary(&["pixi"], Some(&path_var(&[&ana_bin])), &ana_home),
            None
        );
    }

    /// Restores the process working directory on drop, even if the test panics.
    struct CwdGuard(PathBuf);

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }

    #[test]
    #[serial_test::serial(env)]
    fn test_find_external_binary_skips_relative_entries() {
        let temp = TempDir::new().unwrap();
        let ana_home = temp.path().join(".ana");
        let abs_dir = temp.path().join("relative-bin");
        make_executable(&abs_dir, "pixi");

        // Make `relative-bin/pixi` resolve to a real executable from the
        // current working directory, so only the absolute-path filter can
        // reject it.
        let _guard = CwdGuard(std::env::current_dir().unwrap());
        std::env::set_current_dir(temp.path()).unwrap();
        let rel = PathBuf::from("relative-bin");
        assert!(is_executable(&rel.join(paths::binary_name("pixi"))));

        assert_eq!(
            find_external_binary(&["pixi"], Some(&path_var(&[&rel])), &ana_home),
            None
        );
        // The same directory given as an absolute path is found.
        assert!(find_external_binary(&["pixi"], Some(&path_var(&[&abs_dir])), &ana_home).is_some());
    }

    #[cfg(unix)]
    #[test]
    fn test_find_external_binary_ignores_non_executable() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let ana_home = temp.path().join(".ana");
        let ext_bin = temp.path().join("bin");
        let pixi = make_executable(&ext_bin, "pixi");
        std::fs::set_permissions(&pixi, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert_eq!(
            find_external_binary(&["pixi"], Some(&path_var(&[&ext_bin])), &ana_home),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_find_external_binary_ignores_symlink_into_ana_home() {
        let temp = TempDir::new().unwrap();
        let ana_home = temp.path().join(".ana");
        let managed = make_executable(&ana_home.join("tools/pixi/bin"), "pixi");
        let other_bin = temp.path().join("other-bin");
        std::fs::create_dir_all(&other_bin).unwrap();
        std::os::unix::fs::symlink(&managed, other_bin.join("pixi")).unwrap();

        assert_eq!(
            find_external_binary(&["pixi"], Some(&path_var(&[&other_bin])), &ana_home),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_find_external_binary_skips_ana_bin_before_external() {
        let temp = TempDir::new().unwrap();
        let ana_home = temp.path().join(".ana");
        let ana_bin = ana_home.join("bin");
        make_executable(&ana_bin, "pixi");
        let ext_bin = temp.path().join(".pixi").join("bin");
        let external = make_executable(&ext_bin, "pixi");

        let found =
            find_external_binary(&["pixi"], Some(&path_var(&[&ana_bin, &ext_bin])), &ana_home);
        assert_eq!(found, Some(external));
    }
}
