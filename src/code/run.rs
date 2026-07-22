use crate::context::CommandContext;
use crate::tools;

/// Run the `kilo` command with the given arguments.
/// Auto-installs or updates kilo as needed.
pub async fn run(ctx: &mut CommandContext, args: &[String]) -> miette::Result<()> {
    tools::install::ensure_tool(ctx, "kilo").await?;
    tools::run_tool_binary("kilo", "kilo", args)
}
