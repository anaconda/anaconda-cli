//! Task execution for detected projects.

use std::process::ExitStatus;

use crate::paths;
use crate::projects;

/// Run a task using a tool.
///
/// If `tool` is specified, use that tool. Otherwise, auto-detect from project.
/// The tool's task prefix (e.g., "run" for pixi) is automatically applied.
pub async fn run(tool: Option<&str>, args: &[String]) -> Result<ExitStatus, String> {
    let tool_name = match tool {
        Some(name) => {
            eprintln!("Using tool: {}", name);
            name.to_string()
        }
        None => {
            let project = projects::detect_current()
                .ok_or_else(|| "No supported project found (looking for: pixi.toml)".to_string())?;
            let name = project.tool_name();
            eprintln!("Detected project type: {:?} (using {})", project, name);
            name.to_string()
        }
    };

    if args.is_empty() {
        return Err("No task specified".to_string());
    }

    // Ensure the tool is installed
    ensure_tool_installed(&tool_name).await?;

    // Run the task
    run_task(&tool_name, args).await
}

async fn ensure_tool_installed(tool_name: &str) -> Result<(), String> {
    let tool_bin = paths::bin_dir().join(tool_name);

    if !tool_bin.exists() {
        eprintln!("{} not found, installing...", tool_name);
        super::install::install_tool(tool_name)
            .await
            .map_err(|e| format!("{:?}", e))?;
    }

    Ok(())
}

async fn run_task(tool_name: &str, args: &[String]) -> Result<ExitStatus, String> {
    let tool_bin = paths::bin_dir().join(tool_name);

    let mut cmd = std::process::Command::new(&tool_bin);

    // Prepend task prefix (e.g., "run" for pixi)
    cmd.args(super::lockfiles::task_prefix(tool_name));
    cmd.args(args);

    // Prepend ana's bin directory to PATH so ana tools take precedence
    let bin_dir = paths::bin_dir();
    let path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), path);
    cmd.env("PATH", new_path);

    cmd.status()
        .map_err(|e| format!("Failed to run {}: {}", tool_name, e))
}
