use crate::context::CommandContext;
use crate::paths;
use crate::tools;
use crate::ui::status;

/// Run the `anaconda mcp` command with the given arguments.
/// Auto-installs anaconda-cli if not present.
pub async fn run(ctx: &mut CommandContext, args: &[String]) -> miette::Result<()> {
    if !paths::bin_path("anaconda").exists() {
        status::info("Installing anaconda-cli...");
        tools::install::install_tool(ctx, "anaconda-cli").await?;
        status::blank_line();
    } else if let Some((installed_version, min_required)) =
        tools::install::check_tool_incompatible("anaconda-cli")
    {
        status::warn(&format!(
            "anaconda-cli {} is incompatible with this version of ana (requires >= {})",
            installed_version, min_required
        ));
        eprintln!("  Run: ana tool install anaconda-cli");
        status::blank_line();
    }

    let mut mcp_args = vec!["mcp".to_string()];
    mcp_args.extend(args.iter().cloned());
    tools::run_tool_binary("anaconda-cli", "anaconda", &mcp_args)
}
