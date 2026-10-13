use std::path::{Path, PathBuf};
use std::process::Command;

use miette::miette;

use crate::paths;

/// Resolve the path to a tool binary in the tool's installation directory
/// under `~/.ana/tools/`.
fn resolve_tool_binary(tool_name: &str, binary_name: &str) -> miette::Result<PathBuf> {
    let bin_subdir = if cfg!(windows) { "Scripts" } else { "bin" };
    resolve_tool_binary_in(tool_name, Path::new(bin_subdir), binary_name)
}

/// Resolve the path to a tool binary located in `bin_subdir` (relative to the
/// tool prefix under `~/.ana/tools/`).
fn resolve_tool_binary_in(
    tool_name: &str,
    bin_subdir: &Path,
    binary_name: &str,
) -> miette::Result<PathBuf> {
    let binary = paths::binary_name(binary_name);
    let tool_bin = paths::tool_prefix(tool_name).join(bin_subdir).join(&binary);

    if !tool_bin.exists() {
        return Err(miette!(
            "{} not found at {}. Run `ana tool install {}` first.",
            binary_name,
            tool_bin.display(),
            tool_name
        ));
    }

    Ok(tool_bin)
}

/// Run a binary from within a tool's installation directory, setting any
/// extra environment variables in `envs` for the child process.
pub fn run_tool_binary(
    tool_name: &str,
    binary_name: &str,
    args: &[String],
    envs: &[(&str, &str)],
) -> miette::Result<()> {
    let tool_bin = resolve_tool_binary(tool_name, binary_name)?;
    run_binary(&tool_bin, binary_name, args, envs)
}

/// Path to a binary located in `bin_subdir` of a tool's installation
/// directory, for callers that manage the process themselves.
///
/// Use this for tools whose binaries don't live in the default `bin`
/// (Unix) / `Scripts` (Windows) directory, e.g. native conda packages that
/// install to `Library/bin` on Windows.
pub fn tool_binary_path(
    tool_name: &str,
    bin_subdir: &Path,
    binary_name: &str,
) -> miette::Result<PathBuf> {
    resolve_tool_binary_in(tool_name, bin_subdir, binary_name)
}

fn run_binary(
    tool_bin: &PathBuf,
    binary_name: &str,
    args: &[String],
    envs: &[(&str, &str)],
) -> miette::Result<()> {
    let status = Command::new(tool_bin)
        .args(args)
        .envs(envs.iter().copied())
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
