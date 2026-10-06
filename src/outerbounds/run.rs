use crate::context::CommandContext;
use crate::tools;

/// Forward the given arguments to the outerbounds CLI unchanged.
pub async fn run(ctx: &mut CommandContext, args: &[String]) -> miette::Result<()> {
    tools::install::ensure_tool(ctx, "outerbounds").await?;
    tools::run_tool_binary(
        "outerbounds",
        "outerbounds",
        args,
        &[("OB_CLI_CALLED_FROM_ANA", "true")],
    )
}
