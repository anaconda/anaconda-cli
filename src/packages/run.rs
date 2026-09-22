use crate::context::CommandContext;

#[cfg(tool_install)]
use crate::tools;

#[cfg(not(tool_install))]
use crate::ui::status;

/// Run the `anaconda channel` command with the given arguments.
/// Auto-installs anaconda-cli if not present (which includes anaconda-client).
#[cfg(tool_install)]
pub async fn run(ctx: &mut CommandContext, args: &[String]) -> miette::Result<()> {
    tools::install::ensure_tool(ctx, "anaconda-cli").await?;

    let mut channel_args = vec!["channel".to_string()];
    channel_args.extend(args.iter().cloned());
    tools::run_tool_binary("anaconda-cli", "anaconda", &channel_args)
}

/// Run the `anaconda channel` command with the given arguments.
#[cfg(not(tool_install))]
pub async fn run(_ctx: &mut CommandContext, _args: &[String]) -> miette::Result<()> {
    status::blank_line();
    Err(crate::errors::ToolManagementUnavailableError.into())
}
