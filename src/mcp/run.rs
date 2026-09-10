use crate::context::CommandContext;
#[cfg(not(tool_install))]
use crate::paths;
use crate::tools;

/// Check if anaconda-mcp is installed by looking for its conda-meta entry.
#[cfg(not(tool_install))]
fn is_anaconda_mcp_installed() -> bool {
    let Some(conda_prefix) = paths::conda_prefix() else {
        return false;
    };

    let conda_meta = conda_prefix.join("conda-meta");
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
/// When built without `tool-install` feature, anaconda-cli is expected to be provided by conda,
/// and anaconda-mcp must be installed for the mcp subcommand to work.
#[cfg(not(tool_install))]
pub async fn run(_ctx: &mut CommandContext, args: &[String]) -> miette::Result<()> {
    if !is_anaconda_mcp_installed() {
        return Err(crate::errors::AnacondaMcpNotInstalledError.into());
    }

    let mut mcp_args = vec!["mcp".to_string()];
    mcp_args.extend(args.iter().cloned());
    tools::run_tool_binary("anaconda-cli", "anaconda", &mcp_args)
}

/// Run the `anaconda mcp` command with the given arguments.
/// Auto-installs or updates anaconda-cli as needed.
#[cfg(tool_install)]
pub async fn run(ctx: &mut CommandContext, args: &[String]) -> miette::Result<()> {
    tools::install::ensure_tool(ctx, "anaconda-cli").await?;

    let mut mcp_args = vec!["mcp".to_string()];
    mcp_args.extend(args.iter().cloned());
    tools::run_tool_binary("anaconda-cli", "anaconda", &mcp_args)
}
