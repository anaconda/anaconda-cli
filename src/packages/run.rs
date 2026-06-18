use crate::context::CommandContext;
use crate::paths;
use crate::tools;
use crate::ui::status;

/// Run the `anaconda channel` command with the given arguments.
/// Auto-installs anaconda-cli if not present.
pub async fn run(ctx: &mut CommandContext, args: &[String]) -> miette::Result<()> {
    if !paths::bin_path("anaconda").exists() {
        status::info("Installing anaconda-cli...");
        tools::install::install_tool(ctx, "anaconda-cli").await?;
        status::blank_line();
    }

    let mut channel_args = vec!["channel".to_string()];
    channel_args.extend(args.iter().cloned());
    tools::run_tool_binary("anaconda-cli", "anaconda", &channel_args)
}
