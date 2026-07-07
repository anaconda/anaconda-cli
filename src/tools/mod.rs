pub mod install;
pub mod list;
#[cfg(feature = "unstable")]
pub mod pip;
mod pixi_config;
mod run;
pub mod specs;
pub mod uninstall;
#[cfg(feature = "unstable")]
pub mod utils;
#[cfg(feature = "unstable")]
pub mod uv;

pub use run::run_tool_binary;

use crate::context::CommandContext;

/// Ensure a tool is installed, installing it if necessary.
pub async fn ensure_tool(ctx: &mut CommandContext, name: &str) -> miette::Result<()> {
    install::ensure_tool(ctx, name).await?;
    Ok(())
}
