use std::process::Command;

use miette::miette;

use crate::context::CommandContext;
use crate::paths;
use crate::tools;

/// Run the `anaconda mcp` command with the given arguments.
/// Auto-installs anaconda-cli if not present.
pub async fn run(ctx: &mut CommandContext, args: &[String]) -> miette::Result<()> {
    let anaconda_bin = paths::bin_path("anaconda");

    if !anaconda_bin.exists() {
        eprintln!("anaconda-cli not installed, installing...");
        tools::install::install_tool(ctx, "anaconda-cli")
            .await
            .map_err(|e| miette!("{:?}", e))?;
    }

    let status = Command::new(&anaconda_bin)
        .arg("mcp")
        .args(args)
        .status()
        .map_err(|e| miette!("Failed to run anaconda mcp: {}", e))?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}
