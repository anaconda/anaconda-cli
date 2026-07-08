#[cfg(feature = "fleet")]
pub mod fleet;
#[cfg(not(feature = "fleet"))]
pub mod install;
#[cfg(feature = "fleet")]
pub use fleet::install_tool;
pub mod list;
#[cfg(feature = "unstable")]
pub mod pip;
mod pixi_config;
mod run;
pub mod specs;
#[cfg(not(feature = "fleet"))]
pub mod uninstall;
#[cfg(feature = "unstable")]
pub mod utils;
#[cfg(feature = "unstable")]
pub mod uv;

pub use run::run_tool_binary;

use crate::context::CommandContext;

/// Ensure a tool is installed, installing it if necessary.
#[cfg(not(feature = "fleet"))]
pub async fn ensure_tool(ctx: &mut CommandContext, name: &str) -> miette::Result<()> {
    install::ensure_tool(ctx, name).await?;
    Ok(())
}

#[cfg(feature = "fleet")]
pub async fn ensure_tool(ctx: &mut CommandContext, name: &str) -> miette::Result<()> {
    if !crate::paths::tool_prefix(name).exists() {
        crate::ui::status::info(&format!("Installing {}...", name));
        fleet::install_tool(ctx, name).await?;
        crate::ui::status::blank_line();
    }
    Ok(())
}
