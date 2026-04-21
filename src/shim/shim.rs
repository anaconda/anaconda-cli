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

    // Parse the path components and build a proper PathBuf
    // This handles both forward and back slashes in the config
    let target_path: PathBuf = shim_dir
        .join("..\\tools")
        .join(target_rel.replace('/', "\\"));

    if !target_path.exists() {
        return Err(format!("target not found: {}", target_path.display()));
    }

    // Execute with all arguments
    let args: Vec<String> = env::args().skip(1).collect();
    let status = Command::new(&target_path)
        .args(&args)
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
