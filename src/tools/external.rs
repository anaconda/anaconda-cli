//! Detection of externally-installed copies of tools ana can manage.
//!
//! When a user already has a tool on `PATH` (e.g. conda from Miniconda), ana
//! can use that installation instead of requiring a managed one. These helpers
//! locate such an installation, extract its version for comparison against the
//! version ana expects, and point the user at `ana tool install`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::paths;
use crate::ui::status;

use super::specs;

/// An existing, non-ana installation of a tool found on `PATH`.
pub struct ExternalInstall {
    pub path: PathBuf,
    pub version: Option<String>,
}

/// Search `PATH` for `binary`, skipping ana's own shim directory.
pub fn find_binary(binary: &str) -> Option<PathBuf> {
    let ana_bin = paths::bin_dir();
    let names = candidate_names(binary);
    let path_var = std::env::var_os("PATH")?;

    for dir in std::env::split_paths(&path_var) {
        if dir == ana_bin {
            continue;
        }
        for name in &names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn candidate_names(binary: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        let mut names = vec![binary.to_string()];
        if Path::new(binary).extension().is_none() {
            let pathext =
                std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
            for ext in pathext.split(';').filter(|e| !e.is_empty()) {
                names.push(format!("{binary}{}", ext.to_ascii_lowercase()));
            }
        }
        names
    }
    #[cfg(not(windows))]
    {
        vec![binary.to_string()]
    }
}

/// Detect an external installation of a managed tool.
pub fn detect(name: &str) -> Option<ExternalInstall> {
    let binary = specs::delegate_executable(name);
    let path = find_binary(binary)?;
    let version = version_at(&path);
    Some(ExternalInstall { path, version })
}

fn version_at(path: &Path) -> Option<String> {
    let output = Command::new(path).arg("--version").output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = if stdout.trim().is_empty() {
        stderr
    } else {
        stdout
    };
    parse_version(&text)
}

/// Pull the first version-looking token (e.g. "conda 25.1.0" -> "25.1.0").
fn parse_version(text: &str) -> Option<String> {
    text.split_whitespace().find_map(|token| {
        let token = token.trim_start_matches('v');
        let looks_like_version =
            token.contains('.') && token.chars().next().is_some_and(|c| c.is_ascii_digit());
        looks_like_version.then(|| {
            token
                .trim_matches(|c: char| {
                    !c.is_ascii_alphanumeric() && c != '.' && c != '-' && c != '+'
                })
                .to_string()
        })
    })
}

/// Report an external install and how to let ana manage it instead.
pub fn report(name: &str, external: &ExternalInstall) {
    let version = external.version.as_deref().unwrap_or("unknown");
    eprintln!(
        "Using existing {} at {} (version {}).",
        status::highlight(name),
        external.path.display(),
        version
    );
    if let Some(expected) = specs::locked_version(name) {
        if external.version.as_deref() == Some(expected.as_str()) {
            eprintln!("  This matches the version ana expects ({expected}).");
        } else {
            eprintln!("  ana expects version {expected}.");
            eprintln!("  If you run into issues, try updating to that version.");
        }
    }
    eprintln!(
        "  Run {} to let ana manage it instead.",
        status::highlight(&format!("`ana tool install {name}`"))
    );
    status::blank_line();
}

/// Resolve a tool binary, preferring ana's managed install and falling back to
/// an external copy on `PATH`.
pub fn resolve_binary(tool_name: &str, binary_name: &str) -> Option<PathBuf> {
    let managed = paths::tool_prefix(tool_name)
        .join(if cfg!(windows) { "Scripts" } else { "bin" })
        .join(paths::binary_name(binary_name));
    if managed.exists() {
        return Some(managed);
    }
    find_binary(binary_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_parse_version() {
        assert_eq!(parse_version("conda 25.1.0").as_deref(), Some("25.1.0"));
        assert_eq!(parse_version("pixi 0.70.2\n").as_deref(), Some("0.70.2"));
        assert_eq!(parse_version("v1.2.3").as_deref(), Some("1.2.3"));
        assert_eq!(
            parse_version("outerbounds 2.0.0-beta.1").as_deref(),
            Some("2.0.0-beta.1")
        );
        assert!(parse_version("no version here").is_none());
    }

    #[cfg(unix)]
    #[test]
    #[serial(env)]
    fn test_detect_and_report_external() {
        use std::os::unix::fs::PermissionsExt;

        let home = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("outerbounds");
        std::fs::write(&bin, "#!/bin/sh\necho 'outerbounds 9.9.9'\n").unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();

        temp_env::with_vars(
            [
                ("ANA_HOME", Some(home.path().to_str().unwrap())),
                ("PATH", Some(dir.path().to_str().unwrap())),
            ],
            || {
                let external = detect("outerbounds").expect("should find external outerbounds");
                assert_eq!(external.version.as_deref(), Some("9.9.9"));
                assert_eq!(
                    resolve_binary("outerbounds", "outerbounds").as_deref(),
                    Some(bin.as_path())
                );
            },
        );
    }

    #[test]
    #[serial(env)]
    fn test_find_binary_skips_ana_bin_dir() {
        let home = tempfile::tempdir().unwrap();
        let ana_bin = home.path().join("bin");
        std::fs::create_dir_all(&ana_bin).unwrap();
        let shim = ana_bin.join(paths::binary_name("outerbounds"));
        std::fs::write(&shim, "").unwrap();

        temp_env::with_vars(
            [
                ("ANA_HOME", Some(home.path().to_str().unwrap())),
                ("PATH", Some(ana_bin.to_str().unwrap())),
            ],
            || {
                assert!(find_binary("outerbounds").is_none());
            },
        );
    }
}
