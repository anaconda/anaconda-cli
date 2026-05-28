use crate::context::CommandContext;
#[cfg(not(feature = "conda-package"))]
use crate::paths;
use crate::tools;
#[cfg(not(feature = "conda-package"))]
use crate::ui::status;

/// Run the `anaconda mcp` command with the given arguments.
///
/// When built without `conda-package` feature, auto-installs anaconda-cli if not present.
/// When built with `conda-package` feature, anaconda-cli is expected to be provided by conda.
#[cfg(feature = "conda-package")]
pub async fn run(_ctx: &mut CommandContext, args: &[String]) -> miette::Result<()> {
    let mut mcp_args = vec!["mcp".to_string()];
    mcp_args.extend(args.iter().cloned());
    tools::run_tool_binary("anaconda-cli", "anaconda", &mcp_args)
}

#[cfg(not(feature = "conda-package"))]
pub async fn run(ctx: &mut CommandContext, args: &[String]) -> miette::Result<()> {
    if !paths::tool_prefix("anaconda-cli").exists() {
        status::info("Installing anaconda-cli...");
        tools::install::install_tool(ctx, "anaconda-cli").await?;
        status::blank_line();
    }

    let mut mcp_args = vec!["mcp".to_string()];
    mcp_args.extend(args.iter().cloned());
    tools::run_tool_binary("anaconda-cli", "anaconda", &mcp_args)
}
