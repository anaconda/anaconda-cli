use std::path::PathBuf;
use std::process::Command;

use miette::miette;

use crate::context::CommandContext;
#[cfg(tool_install)]
use crate::tools;
use crate::ui::status;

use super::{InitOptions, ensure_configured, init_project, open_app, view_app};

/// Resolve the path to an outerbounds binary.
///
/// In the tool-install build, binaries live under `~/.ana/tools/outerbounds/bin/`.
/// In the conda-package build, they are expected in `$CONDA_PREFIX/bin/`.
#[cfg(tool_install)]
pub(super) fn resolve_ob_binary(binary_name: &str) -> miette::Result<PathBuf> {
    let bin_subdir = if cfg!(windows) { "Scripts" } else { "bin" };
    let binary = crate::paths::binary_name(binary_name);
    let tool_bin = crate::paths::tool_prefix("outerbounds")
        .join(bin_subdir)
        .join(&binary);

    if !tool_bin.exists() {
        return Err(miette!(
            "{} not found at {}. Run `ana tool install outerbounds` first.",
            binary_name,
            tool_bin.display(),
        ));
    }

    Ok(tool_bin)
}

#[cfg(not(tool_install))]
pub(super) fn resolve_ob_binary(binary_name: &str) -> miette::Result<PathBuf> {
    let conda_prefix = crate::paths::conda_prefix().ok_or_else(|| {
        miette!(
            "Could not determine conda environment prefix. \
             Are you in an active conda environment?"
        )
    })?;

    let bin_subdir = if cfg!(windows) { "Scripts" } else { "bin" };
    let binary = crate::paths::binary_name(binary_name);
    let ob_bin = conda_prefix.join(bin_subdir).join(&binary);

    if !ob_bin.exists() {
        return Err(miette!(
            "{} not found at {}. Is the outerbounds package installed in this environment?",
            binary_name,
            ob_bin.display(),
        ));
    }

    Ok(ob_bin)
}

/// Run a resolved outerbounds binary with the given arguments.
fn run_ob_binary(binary_name: &str, args: &[String]) -> miette::Result<()> {
    let bin_path = resolve_ob_binary(binary_name)?;

    let status = Command::new(&bin_path)
        .args(args)
        .status()
        .map_err(|e| miette!("Failed to run {}: {}", binary_name, e))?;

    if status.success() {
        Ok(())
    } else {
        let msg = format!(
            "{} exited with code {}",
            binary_name,
            status.code().unwrap_or(1)
        );
        tracing::error!("{}", msg);
        Err(miette!(msg))
    }
}

/// Run the outerbounds CLI wrapper with the given arguments.
pub async fn run(ctx: &mut CommandContext, args: &[String]) -> miette::Result<()> {
    #[cfg(tool_install)]
    tools::install::ensure_tool(ctx, "outerbounds").await?;

    // Suppress unused variable warning in conda-package builds where ctx is
    // not needed after the cfg-gated ensure_tool call above.
    let _ = ctx;

    // Handle `platform app open <name>`
    if args.len() >= 3 && args[0] == "app" && args[1] == "open" {
        return open_app(&args[2]);
    }

    // Handle `platform app view [--web]`
    if args.len() >= 2 && args[0] == "app" && args[1] == "view" {
        let web = args.get(2).map(|a| a == "--web").unwrap_or(false);
        return view_app(web);
    }

    // Handle `platform init [path] [options]`
    if !args.is_empty() && args[0] == "init" {
        let init_args: Vec<String> = args[1..].to_vec();
        let opts = InitOptions::from_args(&init_args);
        return init_project(opts);
    }

    // Handle `platform check` - verify configuration first to give a nicer error
    if !args.is_empty() && args[0] == "check" {
        ensure_configured()?;
        return run_ob_binary("outerbounds", args);
    }

    // Handle `platform deploy` by running obproject-deploy from the outerbounds tool
    if !args.is_empty() && args[0] == "deploy" {
        let deploy_args: Vec<String> = args[1..].to_vec();
        run_ob_binary("obproject-deploy", &deploy_args)?;
        status::blank_line();
        status::celebrate("Deployment complete!");
        status::blank_line();
        eprintln!("Open your app in the browser with:");
        eprintln!("  {}", status::highlight("ana platform app view --web"));
        return Ok(());
    }

    // Pass through to the outerbounds CLI
    run_ob_binary("outerbounds", args)
}
