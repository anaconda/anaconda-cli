use std::process::Command;

use crate::paths;
use crate::tools;

pub async fn run_bootstrap() -> Result<(), String> {
    let anaconda_bin = paths::bin_dir().join("anaconda");

    if anaconda_bin.exists() {
        eprintln!("anaconda-cli is already installed");
        return Ok(());
    }

    eprintln!("Installing anaconda-cli...");
    tools::install::install_tool("anaconda-cli")
        .await
        .map_err(|e| format!("{:?}", e))?;

    eprintln!("anaconda-cli installed successfully");
    Ok(())
}

pub fn run_subcommand(subcommand: &str, args: &[String]) -> Result<(), String> {
    let anaconda_bin = paths::bin_dir().join("anaconda");

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
