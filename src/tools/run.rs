use std::path::PathBuf;
use std::process::Command;

use miette::miette;

use crate::paths;

/// Resolve the path to a tool binary, preferring the tool's installation
/// directory under `~/.ana/tools/` and falling back to an external copy on
/// `PATH` (e.g. a user's existing conda install).
pub fn resolve_tool_binary(tool_name: &str, binary_name: &str) -> miette::Result<PathBuf> {
    if let Some(bin) = super::external::resolve_binary(tool_name, binary_name) {
        return Ok(bin);
    }

    let bin_subdir = if cfg!(windows) { "Scripts" } else { "bin" };
    let binary = paths::binary_name(binary_name);
    let tool_bin = paths::tool_prefix(tool_name).join(bin_subdir).join(&binary);

    Err(miette!(
        "{} not found at {}. Run `ana tool install {}` first.",
        binary_name,
        tool_bin.display(),
        tool_name
    ))
}

/// Run a binary from within a tool's installation directory.
pub fn run_tool_binary(tool_name: &str, binary_name: &str, args: &[String]) -> miette::Result<()> {
    let tool_bin = resolve_tool_binary(tool_name, binary_name)?;
    run_binary(&tool_bin, binary_name, args)
}

fn run_binary(tool_bin: &PathBuf, binary_name: &str, args: &[String]) -> miette::Result<()> {
    let status = Command::new(tool_bin)
        .args(args)
        .status()
        .map_err(|e| miette!("Failed to run {}: {}", binary_name, e))?;

    if status.success() {
        Ok(())
    } else {
        let msg = format!(
            "{} exited with code {}",
            binary_name,
            status.code().unwrap_or(1)
        );
        tracing::error!("{}", msg);
        Err(miette!(msg))
    }
}
