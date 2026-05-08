//! Binary shim that forwards execution to a target specified in shims.cfg.
//!
//! The shim:
//! 1. Gets its own filename (e.g., `pixi.exe` -> `pixi`)
//! 2. Reads `../tools/shims.cfg` relative to itself
//! 3. Looks up the target path for its name
//! 4. Executes the target with all arguments passed through
//!
//! shims.cfg format (one entry per line):
//! ```
//! pixi=pixi\bin\pixi.exe
//! anaconda=anaconda-cli\Scripts\anaconda.exe
//! ```

use std::env;
use std::path::PathBuf;
use std::process::{Command, exit};

/// Environment variable set to indicate wrapper invocation to ana.exe.
/// Must match the constant in src/tools/conda_wrapper.rs.
const WRAPPER_INVOCATION_ENV_VAR: &str = "_ANA_INTERNAL_WRAPPER_INVOCATION";

fn main() {
    if let Err(e) = run() {
        eprintln!("shim error: {}", e);
        exit(1);
    }
}

fn run() -> Result<(), String> {
    let shim_path = env::current_exe().map_err(|e| format!("failed to get exe path: {}", e))?;
    let shim_dir = shim_path.parent().ok_or("shim has no parent directory")?;
    let shim_name = shim_path
        .file_stem()
        .ok_or("shim has no file stem")?
        .to_string_lossy();

    // Read shims.cfg from ..\tools\shims.cfg
    let config_path = shim_dir.join("..\\tools\\shims.cfg");
    let config_content = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("failed to read {}: {}", config_path.display(), e))?;

    // Find matching line: name=path
    let target_rel = config_content
        .lines()
        .filter_map(|line| line.split_once('='))
        .find(|(name, _)| *name == shim_name)
        .map(|(_, path)| path)
        .ok_or_else(|| format!("no config found for '{}'", shim_name))?;

    // Parse the path and check if it's a wrapper invocation (pointing to ana.exe)
    let target_rel_path = PathBuf::from(target_rel.replace('/', "\\"));
    let is_wrapper_invocation = target_rel_path
        .file_name()
        .map(|name| name == "ana.exe")
        .unwrap_or(false);

    let target_path = if is_wrapper_invocation {
        target_rel_path
    } else {
        let path = shim_dir.join("..\\tools").join(&target_rel_path);
        if !path.exists() {
            return Err(format!("target not found: {}", path.display()));
        }
        path
    };

    // Execute with all arguments
    let args: Vec<String> = env::args().skip(1).collect();
    let mut cmd = Command::new(&target_path);
    cmd.args(&args);
    if is_wrapper_invocation {
        cmd.env(WRAPPER_INVOCATION_ENV_VAR, &shim_name.as_ref());
    }
    let status = cmd
        .status()
        .map_err(|e| format!("failed to execute {}: {}", target_path.display(), e))?;

    exit(status.code().unwrap_or(1));
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_parse_config_line() {
        let content = "pixi=pixi\\bin\\pixi.exe\nanaconda=anaconda-cli\\Scripts\\anaconda.exe\n";
        let result: Option<&str> = content
            .lines()
            .filter_map(|line| line.split_once('='))
            .find(|(name, _)| *name == "pixi")
            .map(|(_, path)| path);
        assert_eq!(result, Some("pixi\\bin\\pixi.exe"));
    }

    #[test]
    fn test_parse_config_not_found() {
        let content = "pixi=pixi\\bin\\pixi.exe\n";
        let result: Option<&str> = content
            .lines()
            .filter_map(|line| line.split_once('='))
            .find(|(name, _)| *name == "unknown")
            .map(|(_, path)| path);
        assert_eq!(result, None);
    }

    #[test]
    fn test_path_normalization() {
        // Config might have forward slashes, we normalize to backslashes
        let path_from_config = "pixi/bin/pixi.exe";
        let normalized = path_from_config.replace('/', "\\");
        assert_eq!(normalized, "pixi\\bin\\pixi.exe");
    }
}
