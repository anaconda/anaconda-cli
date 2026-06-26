use crate::context::CommandContext;
use crate::tools;

/// Run the `anaconda mcp` command with the given arguments.
/// Auto-installs or updates anaconda-cli as needed.
pub async fn run(ctx: &mut CommandContext, args: &[String]) -> miette::Result<()> {
    tools::install::ensure_tool(ctx, "anaconda-cli").await?;

    let mut mcp_args = vec!["mcp".to_string()];
    mcp_args.extend(args.iter().cloned());
    tools::run_tool_binary("anaconda-cli", "anaconda", &mcp_args)
}
