use std::process::Command;

use crate::context::CommandContext;
use crate::paths;
use crate::tools;

pub async fn run_bootstrap(ctx: &mut CommandContext) -> Result<(), String> {
    let anaconda_bin = paths::bin_path("anaconda");

    if anaconda_bin.exists() {
        eprintln!("anaconda-cli is already installed");
        return Ok(());
    }

    eprintln!("Installing anaconda-cli...");
    tools::install::install_tool(ctx, "anaconda-cli")
        .await
        .map_err(|e| format!("{:?}", e))?;

    eprintln!("anaconda-cli installed successfully");
    Ok(())
}

pub fn run_subcommand(
    _ctx: &mut CommandContext,
    subcommand: &str,
    args: &[String],
) -> Result<(), String> {
    let anaconda_bin = paths::bin_path("anaconda");

    if !anaconda_bin.exists() {
        let msg = format!(
            "anaconda not found at {}. Run `ana bootstrap` first.",
            anaconda_bin.display()
        );
        tracing::error!("{}", msg);
        return Err(msg);
    }

    let status = Command::new(&anaconda_bin)
        .arg(subcommand)
        .args(args)
        .status()
        .map_err(|e| format!("Failed to run anaconda: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        let msg = format!("anaconda exited with code {}", status.code().unwrap_or(1));
        tracing::error!("{}", msg);
        Err(msg)
    }
}

pub fn run_ob(_ctx: &mut CommandContext, args: &[String]) -> Result<(), String> {
    let ob_bin = paths::bin_path("outerbounds");

    if !ob_bin.exists() {
        let msg = format!(
            "outerbounds not found at {}. Run `ana tool install outerbounds` first.",
            ob_bin.display()
        );
        tracing::error!("{}", msg);
        return Err(msg);
    }

    let status = Command::new(&ob_bin)
        .args(args)
        .status()
        .map_err(|e| format!("Failed to run outerbounds: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        let msg = format!(
            "outerbounds exited with code {}",
            status.code().unwrap_or(1)
        );
        tracing::error!("{}", msg);
        Err(msg)
    }
}

/// Run a binary from within a tool's installation directory.
/// This allows running binaries that aren't exposed as symlinks in ~/.ana/bin.
pub fn run_tool_binary(tool_name: &str, binary_name: &str, args: &[String]) -> Result<(), String> {
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
        return Err(msg);
    }

    let status = Command::new(&tool_bin)
        .args(args)
        .status()
        .map_err(|e| format!("Failed to run {}: {}", binary_name, e))?;

    if status.success() {
        Ok(())
    } else {
        let msg = format!(
            "{} exited with code {}",
            binary_name,
            status.code().unwrap_or(1)
        );
        tracing::error!("{}", msg);
        Err(msg)
    }
}
