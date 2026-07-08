#[cfg(feature = "fleet")]
mod fleet;
#[cfg(not(feature = "fleet"))]
mod install;
pub mod list;
#[cfg(feature = "unstable")]
pub mod pip;
mod pixi_config;
mod run;
pub mod specs;
#[cfg(not(feature = "fleet"))]
mod uninstall;
#[cfg(feature = "unstable")]
pub mod utils;
#[cfg(feature = "unstable")]
pub mod uv;

pub use run::run_tool_binary;

use crate::context::CommandContext;

/// Returns the names of all currently installed tools.
#[cfg(not(feature = "fleet"))]
pub fn installed_tools() -> Vec<&'static str> {
    install::installed_tools()
}

/// Install a tool by name.
#[cfg(not(feature = "fleet"))]
pub async fn install_tool(ctx: &mut CommandContext, name: &str) -> miette::Result<()> {
    install::install_tool(ctx, name).await
}

#[cfg(feature = "fleet")]
pub async fn install_tool(ctx: &mut CommandContext, name: &str) -> miette::Result<()> {
    fleet::install_tool(ctx, name).await
}

/// Uninstall a tool by name.
#[cfg(not(feature = "fleet"))]
pub fn uninstall_tool(ctx: &mut CommandContext, name: &str, force: bool) -> miette::Result<()> {
    uninstall::uninstall_tool(ctx, name, force)
}

#[cfg(feature = "fleet")]
pub fn uninstall_tool(ctx: &mut CommandContext, name: &str, force: bool) -> miette::Result<()> {
    fleet::uninstall_tool(ctx, name, force)
}

/// Update all installed tools.
///
/// Returns the names of tools that were updated.
#[cfg(not(feature = "fleet"))]
pub async fn update_installed_tools(ctx: &mut CommandContext) -> miette::Result<Vec<String>> {
    install::update_installed_tools(ctx).await
}

#[cfg(feature = "fleet")]
pub async fn update_installed_tools(_ctx: &mut CommandContext) -> miette::Result<Vec<String>> {
    // TODO: Implement update for fleet
    Err(miette::miette!(
        "Tool update is not yet supported with the fleet feature"
    ))
}

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
