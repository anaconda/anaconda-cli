use std::path::PathBuf;
use std::process::Command;

use miette::miette;

#[cfg(tool_install)]
use crate::paths;

/// Resolve the path to a tool binary.
///
/// When built with `conda-package` feature, looks in `$CONDA_PREFIX/bin/`.
/// Otherwise, looks in the tool's installation directory under `~/.ana/tools/`.
#[cfg(not(tool_install))]
fn resolve_tool_binary(binary_name: &str) -> miette::Result<PathBuf> {
    let conda_prefix = std::env::var("CONDA_PREFIX")
        .map_err(|_| miette!("CONDA_PREFIX not set. Are you in an active conda environment?"))?;

    let bin_subdir = if cfg!(windows) { "Scripts" } else { "bin" };
    let binary = if cfg!(windows) {
        format!("{}.exe", binary_name)
    } else {
        binary_name.to_string()
    };

    let tool_bin = PathBuf::from(&conda_prefix).join(bin_subdir).join(&binary);

    if !tool_bin.exists() {
        return Err(miette!(
            "{} not found at {}. Is the conda environment configured correctly?",
            binary_name,
            tool_bin.display()
        ));
    }

    Ok(tool_bin)
}

#[cfg(tool_install)]
fn resolve_tool_binary_with_tool_name(
    tool_name: &str,
    binary_name: &str,
) -> miette::Result<PathBuf> {
    let bin_subdir = if cfg!(windows) { "Scripts" } else { "bin" };
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

/// Run a binary from within a tool's installation directory.
///
/// When built with `conda-package` feature, the `tool_name` parameter is ignored
/// and the binary is resolved from `$CONDA_PREFIX/bin/`.
#[cfg(not(tool_install))]
pub fn run_tool_binary(_tool_name: &str, binary_name: &str, args: &[String]) -> miette::Result<()> {
    let tool_bin = resolve_tool_binary(binary_name)?;
    run_binary(&tool_bin, binary_name, args)
}

#[cfg(tool_install)]
pub fn run_tool_binary(tool_name: &str, binary_name: &str, args: &[String]) -> miette::Result<()> {
    let tool_bin = resolve_tool_binary_with_tool_name(tool_name, binary_name)?;
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
