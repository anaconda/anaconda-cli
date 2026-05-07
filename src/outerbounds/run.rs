use miette::miette;

use crate::context::CommandContext;
use crate::tools;
use crate::ui::status;

use super::{InitOptions, init_project, open_app, view_app};

/// Run the outerbounds CLI wrapper with the given arguments.
pub async fn run(ctx: &mut CommandContext, args: &[String]) -> miette::Result<()> {
    // Auto-install outerbounds tool if not present
    if !crate::paths::bin_path("outerbounds").exists() {
        status::info("Installing outerbounds tool...");
        tools::install::install_tool(ctx, "outerbounds").await?;
        status::blank_line();
    }

    // Handle `ob app open <name>`
    if args.len() >= 3 && args[0] == "app" && args[1] == "open" {
        return open_app(&args[2]).map_err(|e| miette!("{}", e));
    }

    // Handle `ob app view [--web]`
    if args.len() >= 2 && args[0] == "app" && args[1] == "view" {
        let web = args.get(2).map(|a| a == "--web").unwrap_or(false);
        return view_app(web).map_err(|e| miette!("{}", e));
    }

    // Handle `ob init [path] [options]`
    if !args.is_empty() && args[0] == "init" {
        let init_args: Vec<String> = args[1..].to_vec();
        let opts = InitOptions::from_args(&init_args);
        return init_project(opts).map_err(|e| miette!("{}", e));
    }

    // Handle `ob deploy` by running obproject-deploy from the outerbounds tool
    if !args.is_empty() && args[0] == "deploy" {
        let deploy_args: Vec<String> = args[1..].to_vec();
        tools::run_tool_binary("outerbounds", "obproject-deploy", &deploy_args)
            .map_err(|e| miette!("{}", e))?;
        status::blank_line();
        status::celebrate("Deployment complete!");
        status::blank_line();
        eprintln!("Open your app in the browser with:");
        eprintln!("  {}", status::highlight("ana ob app view --web"));
        return Ok(());
    }

    // Pass through to the outerbounds CLI
    tools::run_tool_binary("outerbounds", "outerbounds", args).map_err(|e| miette!("{}", e))
}
