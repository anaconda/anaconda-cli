#[cfg(feature = "conda-package")]
use std::path::PathBuf;

use crate::context::CommandContext;
#[cfg(not(feature = "conda-package"))]
use crate::paths;
use crate::tools;
#[cfg(not(feature = "conda-package"))]
use crate::ui::status;

/// Check if anaconda-mcp is installed by looking for its conda-meta entry.
#[cfg(feature = "conda-package")]
fn is_anaconda_mcp_installed() -> bool {
    let Some(conda_prefix) = std::env::var("CONDA_PREFIX").ok() else {
        return false;
    };

    let conda_meta = PathBuf::from(&conda_prefix).join("conda-meta");
    if !conda_meta.is_dir() {
        return false;
    }

    // Look for anaconda-mcp-*.json in conda-meta
    std::fs::read_dir(&conda_meta)
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("anaconda-mcp-")
            })
        })
        .unwrap_or(false)
}

/// Run the `anaconda mcp` command with the given arguments.
///
/// When built without `conda-package` feature, auto-installs anaconda-cli if not present.
/// When built with `conda-package` feature, anaconda-cli is expected to be provided by conda,
/// and anaconda-mcp must be installed for the mcp subcommand to work.
#[cfg(feature = "conda-package")]
pub async fn run(_ctx: &mut CommandContext, args: &[String]) -> miette::Result<()> {
    if !is_anaconda_mcp_installed() {
        return Err(crate::errors::AnacondaMcpNotInstalledError.into());
    }

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
