use miette::miette;

use crate::anaconda_cli;
use crate::context::CommandContext;
use crate::help;
use crate::tools;
use crate::ui::status;

use super::{init_project, open_app, print_init_help, view_app, InitOptions};

/// Run the outerbounds CLI wrapper with the given arguments.
pub async fn run(ctx: &mut CommandContext, args: &[String]) -> miette::Result<()> {
    if args.is_empty() || args.first().map(|a| a == "--help" || a == "-h").unwrap_or(false) {
        help::outerbounds::print_outerbounds_help();
        return Ok(());
    }

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
        match InitOptions::parse(&init_args) {
            Ok(opts) => {
                return init_project(opts).map_err(|e| miette!("{}", e));
            }
            Err(e) if e == "help" => {
                print_init_help();
                return Ok(());
            }
            Err(e) => {
                return Err(miette!("{}", e));
            }
        }
    }

    // Handle `ob deploy` by running obproject-deploy from the outerbounds tool
    if !args.is_empty() && args[0] == "deploy" {
        let deploy_args: Vec<String> = args[1..].to_vec();
        anaconda_cli::run_tool_binary("outerbounds", "obproject-deploy", &deploy_args)
            .map_err(|e| miette!("{}", e))?;
        status::blank_line();
        status::celebrate("Deployment complete!");
        status::blank_line();
        eprintln!("Open your app in the browser with:");
        eprintln!("  {}", status::highlight("ana ob app view --web"));
        return Ok(());
    }

    // Pass through to the outerbounds CLI
    anaconda_cli::run_ob(ctx, args).map_err(|e| miette!("{}", e))
}
