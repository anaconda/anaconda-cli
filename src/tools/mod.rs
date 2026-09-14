#[cfg(any(tool_install, feature = "fleet"))]
mod common;
#[cfg(all(tool_install, feature = "fleet"))]
mod fleet;
#[cfg(all(tool_install, not(feature = "fleet")))]
pub mod install;
pub mod list;
#[cfg(feature = "unstable")]
pub mod pip;
#[cfg(any(tool_install, feature = "fleet"))]
mod pixi_config;
mod run;
pub mod specs;
#[cfg(all(tool_install, not(feature = "fleet")))]
pub mod uninstall;
#[cfg(feature = "unstable")]
pub mod utils;
#[cfg(feature = "unstable")]
pub mod uv;

pub use run::run_tool_binary;

#[cfg(tool_install)]
use crate::context::CommandContext;

/// Returns the names of all currently installed tools.
#[cfg(all(tool_install, not(feature = "fleet")))]
pub fn installed_tools() -> Vec<&'static str> {
    install::installed_tools()
}

/// Install a tool by name.
#[cfg(all(tool_install, not(feature = "fleet")))]
pub async fn install_tool(ctx: &mut CommandContext, name: &str) -> miette::Result<()> {
    install::install_tool(ctx, name).await
}

/// Install a tool by name (fleet version).
#[cfg(all(tool_install, feature = "fleet"))]
pub async fn install_tool(ctx: &mut CommandContext, name: &str) -> miette::Result<()> {
    fleet::install_tool(ctx, name).await
}

/// Uninstall a tool by name.
#[cfg(all(tool_install, not(feature = "fleet")))]
pub fn uninstall_tool(ctx: &mut CommandContext, name: &str, force: bool) -> miette::Result<()> {
    uninstall::uninstall_tool(ctx, name, force)
}

/// Uninstall a tool by name (fleet version).
#[cfg(all(tool_install, feature = "fleet"))]
pub fn uninstall_tool(ctx: &mut CommandContext, name: &str, force: bool) -> miette::Result<()> {
    fleet::uninstall_tool(ctx, name, force)
}

/// Update all installed tools.
///
/// Returns the names of tools that were updated.
#[cfg(all(tool_install, not(feature = "fleet")))]
pub async fn update_installed_tools(ctx: &mut CommandContext) -> miette::Result<Vec<String>> {
    install::update_installed_tools(ctx).await
}

/// Update all installed tools (fleet version).
#[cfg(all(tool_install, feature = "fleet"))]
pub async fn update_installed_tools(_ctx: &mut CommandContext) -> miette::Result<Vec<String>> {
    // TODO: Implement update for fleet
    Err(miette::miette!(
        "Tool update is not yet supported with the fleet feature"
    ))
}

/// Ensure a tool is installed, installing it if necessary.
#[cfg(all(tool_install, not(feature = "fleet")))]
pub async fn ensure_tool(ctx: &mut CommandContext, name: &str) -> miette::Result<()> {
    install::ensure_tool(ctx, name).await?;
    Ok(())
}

/// Ensure a tool is installed, installing it if necessary (fleet version).
#[cfg(all(tool_install, feature = "fleet"))]
pub async fn ensure_tool(ctx: &mut CommandContext, name: &str) -> miette::Result<()> {
    if !crate::paths::tool_prefix(name).exists() {
        crate::ui::status::info(&format!("Installing {}...", name));
        fleet::install_tool(ctx, name).await?;
        crate::ui::status::blank_line();
    }
    Ok(())
}
