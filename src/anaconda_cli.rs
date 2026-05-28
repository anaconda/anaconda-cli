use std::path::PathBuf;
use std::process::Command;

use crate::context::CommandContext;
#[cfg(not(feature = "conda-package"))]
use crate::paths;
#[cfg(not(feature = "conda-package"))]
use crate::tools;

/// Bootstrap anaconda-cli installation.
///
/// When built with `conda-package` feature, this is a no-op since anaconda-cli
/// is provided as a conda dependency.
#[cfg(feature = "conda-package")]
pub async fn run_bootstrap(_ctx: &mut CommandContext) -> Result<(), String> {
    eprintln!("anaconda-cli is provided by conda. No bootstrap needed.");
    Ok(())
}

#[cfg(not(feature = "conda-package"))]
pub async fn run_bootstrap(ctx: &mut CommandContext) -> Result<(), String> {
    if paths::tool_prefix("anaconda-cli").exists() {
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

/// Resolve the path to the anaconda binary.
#[cfg(feature = "conda-package")]
fn resolve_anaconda_bin() -> Result<PathBuf, String> {
    let conda_prefix = std::env::var("CONDA_PREFIX")
        .map_err(|_| "CONDA_PREFIX not set. Are you in an active conda environment?".to_string())?;

    let bin_subdir = if cfg!(windows) { "Scripts" } else { "bin" };
    let binary = if cfg!(windows) {
        "anaconda.exe"
    } else {
        "anaconda"
    };

    let anaconda_bin = PathBuf::from(&conda_prefix).join(bin_subdir).join(binary);

    if !anaconda_bin.exists() {
        return Err(format!(
            "anaconda not found at {}. Is the conda environment configured correctly?",
            anaconda_bin.display()
        ));
    }

    Ok(anaconda_bin)
}

#[cfg(not(feature = "conda-package"))]
fn resolve_anaconda_bin() -> Result<PathBuf, String> {
    let anaconda_bin = paths::bin_path("anaconda");

    if !anaconda_bin.exists() {
        return Err(format!(
            "anaconda not found at {}. Run `ana bootstrap` first.",
            anaconda_bin.display()
        ));
    }

    Ok(anaconda_bin)
}

pub fn run_subcommand(
    _ctx: &mut CommandContext,
    subcommand: &str,
    args: &[String],
) -> Result<(), String> {
    let anaconda_bin = resolve_anaconda_bin()?;
    run_anaconda_command(&anaconda_bin, subcommand, args)
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
