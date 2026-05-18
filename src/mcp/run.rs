use crate::context::CommandContext;
use crate::paths;
use crate::tools;
use crate::ui::status;

/// Run the `anaconda-cli mcp` command with the given arguments.
/// Auto-installs anaconda-cli if not present.
pub async fn run(ctx: &mut CommandContext, args: &[String]) -> miette::Result<()> {
    if !paths::bin_path("anaconda-cli").exists() {
        status::info("Installing anaconda-cli...");
        tools::install::install_tool(ctx, "anaconda-cli").await?;
        status::blank_line();
    }

    let mut mcp_args = vec!["mcp".to_string()];
    mcp_args.extend(args.iter().cloned());
    tools::run_tool_binary("anaconda-cli", "anaconda", &mcp_args)
}
