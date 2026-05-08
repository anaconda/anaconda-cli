use std::process::Command;

use miette::miette;

use crate::paths;

/// Run a binary from within a tool's installation directory.
pub fn run_tool_binary(tool_name: &str, binary_name: &str, args: &[String]) -> miette::Result<()> {
    let bin_subdir = if cfg!(windows) { "Scripts" } else { "bin" };
    let binary = paths::binary_name(binary_name);
    let tool_bin = paths::tool_prefix(tool_name).join(bin_subdir).join(&binary);

    if !tool_bin.exists() {
        let msg = format!(
            "{} not found at {}. Run `ana tool install {}` first.",
            binary_name,
            tool_bin.display(),
            tool_name
        );
        tracing::error!("{}", msg);
        return Err(miette!(msg));
    }

    let status = Command::new(&tool_bin)
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
