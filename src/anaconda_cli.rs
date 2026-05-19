use std::process::Command;

use crate::context::CommandContext;
use crate::paths;
use crate::tools;

pub async fn run_bootstrap(ctx: &mut CommandContext) -> Result<(), String> {
    let anaconda_cli_bin = paths::bin_path("anaconda-cli");

    if anaconda_cli_bin.exists() {
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
    let anaconda_cli_bin = paths::bin_path("anaconda-cli");

    if !anaconda_cli_bin.exists() {
        let msg = format!(
            "anaconda-cli not found at {}. Run `ana bootstrap` first.",
            anaconda_cli_bin.display()
        );
        tracing::error!("{}", msg);
        return Err(msg);
    }

    run_anaconda_command(&anaconda_cli_bin, subcommand, args)
}

fn run_anaconda_command(
    anaconda_bin: &std::path::Path,
    subcommand: &str,
    args: &[String],
) -> Result<(), String> {
    let status = Command::new(anaconda_bin)
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
