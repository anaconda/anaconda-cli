use std::path::PathBuf;

/// Returns the user's home directory using OS-specific environment variables.
///
/// - Unix (Linux, macOS): reads `HOME`
/// - Windows: reads `USERPROFILE`
///
/// Panics if the environment variable is not set.
#[cfg(unix)]
pub fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .expect("HOME environment variable not set")
}

#[cfg(windows)]
pub fn home_dir() -> PathBuf {
    std::env::var("USERPROFILE")
        .map(PathBuf::from)
        .expect("USERPROFILE environment variable not set")
}

/// Returns the ana home directory (~/.ana or ANA_HOME).
pub fn ana_home() -> PathBuf {
    std::env::var("ANA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir().join(".ana"))
}

/// Returns the tools directory (~/.ana/tools).
fn tools_dir() -> PathBuf {
    ana_home().join("tools")
}

/// Returns the bin directory for shims (~/.ana/bin).
pub fn bin_dir() -> PathBuf {
    ana_home().join("bin")
}

/// Returns the prefix for a specific tool (~/.ana/tools/<name>).
pub fn tool_prefix(name: &str) -> PathBuf {
    tools_dir().join(name)
}

/// Returns the binary name with platform-specific extension (.exe on Windows).
pub fn binary_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{}.exe", name)
    } else {
        name.to_string()
    }
}

/// Returns the path to a binary in the bin directory, adding .exe on Windows.
pub fn bin_path(name: &str) -> PathBuf {
    bin_dir().join(binary_name(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_home_dir_returns_path() {
        let home = home_dir();
        assert!(
            home.is_absolute(),
            "home_dir should return an absolute path"
        );
        assert!(!home.as_os_str().is_empty(), "home_dir should not be empty");
    }

    #[test]
    fn test_ana_home_default() {
        temp_env::with_var_unset("ANA_HOME", || {
            let ana = ana_home();
            assert!(
                ana.ends_with(".ana"),
                "default ana_home should end with .ana"
            );
            assert!(ana.is_absolute(), "ana_home should return an absolute path");
        });
    }

    #[test]
    fn test_ana_home_from_env() {
        temp_env::with_var("ANA_HOME", Some("/custom/ana/path"), || {
            assert_eq!(ana_home(), PathBuf::from("/custom/ana/path"));
        });
    }

    #[test]
    fn test_tools_dir() {
        temp_env::with_var("ANA_HOME", Some("/test/ana"), || {
            assert_eq!(tools_dir(), PathBuf::from("/test/ana/tools"));
        });
    }

    #[test]
    fn test_bin_dir() {
        temp_env::with_var("ANA_HOME", Some("/test/ana"), || {
            assert_eq!(bin_dir(), PathBuf::from("/test/ana/bin"));
        });
    }

    #[test]
    fn test_tool_prefix() {
        temp_env::with_var("ANA_HOME", Some("/test/ana"), || {
            assert_eq!(
                tool_prefix("some-tool"),
                PathBuf::from("/test/ana/tools/some-tool")
            );
        });
    }

    #[test]
    fn test_binary_name() {
        let name = binary_name("pixi");
        if cfg!(windows) {
            assert_eq!(name, "pixi.exe");
        } else {
            assert_eq!(name, "pixi");
        }
    }

    #[test]
    fn test_bin_path() {
        temp_env::with_var("ANA_HOME", Some("/test/ana"), || {
            let path = bin_path("pixi");
            if cfg!(windows) {
                assert_eq!(path, PathBuf::from("/test/ana/bin/pixi.exe"));
            } else {
                assert_eq!(path, PathBuf::from("/test/ana/bin/pixi"));
            }
        });
    }
}
