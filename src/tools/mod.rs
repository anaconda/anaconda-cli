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
#[cfg(tool_install)]
mod run;
pub mod specs;
#[cfg(all(tool_install, not(feature = "fleet")))]
pub mod uninstall;
#[cfg(feature = "unstable")]
pub mod utils;
#[cfg(feature = "unstable")]
pub mod uv;

#[cfg(tool_install)]
pub use run::run_tool_binary;

#[cfg(tool_install)]
use crate::context::CommandContext;

/// Print a tool's experimental warning and require explicit acknowledgment.
///
/// Non-experimental tools return immediately. `yes` skips the prompt for
/// non-interactive callers (e.g. `--yes` in scripts/CI).
#[cfg_attr(not(tool_install), allow(dead_code))]
pub fn confirm_experimental_install(name: &str, yes: bool) -> miette::Result<()> {
    if specs::experimental_message(name).is_none() {
        return Ok(());
    }

    if name == "conda" {
        crate::ui::status::warn("Conda as a managed tool is experimental.");
        eprintln!(
            "  Installed under ana's tool prefix and exposed as {} via a wrapper.",
            crate::ui::status::highlight("conda")
        );
        eprintln!(
            "  Activation uses conda's standard shell integration ({}).",
            crate::ui::status::highlight("conda init")
        );
        eprintln!(
            "  Please report issues with {}, not to conda directly.",
            crate::ui::status::highlight("ana self feedback")
        );
    } else if let Some(msg) = specs::experimental_message(name) {
        crate::ui::status::warn(msg);
    }
    eprintln!();

    if yes {
        return Ok(());
    }

    use std::io::IsTerminal;

    if !std::io::stdin().is_terminal() {
        return Err(miette::miette!(
            "{} is experimental and requires confirmation; re-run with --yes to accept.",
            name
        ));
    }

    if !crate::input::prompt_yes_no("Do you acknowledge and want to continue?", false) {
        return Err(miette::miette!("Aborted: {} is experimental.", name));
    }

    Ok(())
}

/// Returns the names of all currently installed tools.
#[cfg(all(tool_install, not(feature = "fleet")))]
pub fn installed_tools() -> Vec<&'static str> {
    install::installed_tools()
}

/// Returns the names of all currently installed tools (fleet version).
#[cfg(all(tool_install, feature = "fleet"))]
pub fn installed_tools() -> Vec<String> {
    fleet::list_installed()
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.id)
        .collect()
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
pub async fn update_installed_tools(ctx: &mut CommandContext) -> miette::Result<Vec<String>> {
    fleet::update_installed_tools(ctx).await
}

/// Ensure a tool is installed, installing it if necessary.
#[cfg(all(tool_install, not(feature = "fleet")))]
pub async fn ensure_tool(ctx: &mut CommandContext, name: &str) -> miette::Result<()> {
    install::ensure_tool(ctx, name).await?;
    Ok(())
}

/// Ensure a tool is installed, installing it if necessary (fleet version).
///
/// A healthy installation is reused only when its recorded lockfile hash
/// matches the embedded lockfile. Missing or interrupted installations (no
/// Fleet metadata) and stale ones (hash mismatch) are (re)installed.
#[cfg(all(tool_install, feature = "fleet"))]
pub async fn ensure_tool(ctx: &mut CommandContext, name: &str) -> miette::Result<()> {
    let lock_content =
        specs::content(name).ok_or_else(|| miette::miette!("unknown tool: {}", name))?;
    let desired_hash = fleet::lock_hash(&lock_content);
    let needs_install = fleet::tool_status(name)?
        .is_none_or(|runtime| runtime.lock_sha256.as_deref() != Some(desired_hash.as_str()));
    if needs_install {
        crate::ui::status::info(&format!("Installing {}...", name));
        fleet::install_tool(ctx, name).await?;
        crate::ui::status::blank_line();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confirm_experimental_skips_regular_tools() {
        assert!(confirm_experimental_install("pixi", false).is_ok());
        assert!(confirm_experimental_install("unknown-tool", false).is_ok());
    }

    #[test]
    fn test_confirm_experimental_accepts_with_yes() {
        assert!(confirm_experimental_install("conda", true).is_ok());
    }
}
