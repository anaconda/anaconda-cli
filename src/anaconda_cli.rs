use crate::context::CommandContext;
use crate::paths;
use crate::tools;

#[cfg(not(feature = "fleet"))]
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

#[cfg(feature = "fleet")]
pub async fn run_bootstrap(ctx: &mut CommandContext) -> Result<(), String> {
    if paths::tool_prefix("anaconda-cli").exists() {
        eprintln!("anaconda-cli is already installed");
        return Ok(());
    }

    eprintln!("Installing anaconda-cli...");
    tools::fleet::install_tool(ctx, "anaconda-cli")
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
    let mut full_args = vec![subcommand.to_string()];
    full_args.extend(args.iter().cloned());
    tools::run_tool_binary("anaconda-cli", "anaconda", &full_args).map_err(|e| format!("{:?}", e))
}
