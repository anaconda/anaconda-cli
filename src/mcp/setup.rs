//! Handlers for `ana mcp clients`, `ana mcp setup`, and `ana mcp remove`.

use std::io::IsTerminal;

use miette::{IntoDiagnostic, miette};
use serde_json::{Value, json};

use super::{clients, state};
use crate::auth;
use crate::context::CommandContext;
use crate::errors::McpError;
use crate::mcp::commands::McpClient;
use crate::ui::status;

const TRANSPORT: &str = "http";

fn remote_url(ctx: &CommandContext) -> String {
    format!("{}/api/mcp", ctx.config.base_url())
}

/// Success line for a configured/updated client.
fn print_configured(action: &str, client: &str, result: &clients::ConfigureResult) {
    status::success(&format!(
        "{action} {} config ({TRANSPORT}): {}",
        status::highlight(client),
        status::dim(&result.config_path.display().to_string())
    ));
    print_backup(&result.backup_path);
}

/// Success lines for a removed client entry.
fn print_removed(client: &str, result: &clients::RemoveResult) {
    for config in &result.configs {
        status::success(&format!(
            "Removed '{}' from {} config: {}",
            result.server_name,
            status::highlight(client),
            status::dim(&config.config_path.display().to_string())
        ));
        print_backup(&config.backup_path);
    }
}

fn print_backup(backup_path: &Option<std::path::PathBuf>) {
    if let Some(backup) = backup_path {
        eprintln!(
            "  {}",
            status::dim(&format!("backup saved to {}", backup.display()))
        );
    }
}

fn require_token(ctx: &CommandContext) -> miette::Result<String> {
    match auth::get_api_key(&ctx.config) {
        Ok(Some(token)) => Ok(token),
        _ => Err(McpError::AuthRequired.into()),
    }
}

/// `ana mcp clients` — list supported clients and their install status.
pub fn list_clients(json: bool) -> miette::Result<()> {
    if json {
        let mut data = serde_json::Map::new();
        for spec in clients::SPECS {
            data.insert(
                spec.name.to_string(),
                json!({
                    "transport": TRANSPORT,
                    "config_key": spec.config_key,
                    "installed": clients::is_installed(spec.name, "anaconda-mcp"),
                }),
            );
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&Value::Object(data)).into_diagnostic()?
        );
        return Ok(());
    }

    let mut table = crate::table::new(["Client", "Transport", "Installed"]);
    for spec in clients::SPECS {
        let installed_cell = if clients::is_installed(spec.name, "anaconda-mcp") {
            crate::table::cell("✓").fg(crate::table::Color::Green)
        } else {
            crate::table::cell("—")
        };
        table.add_row([
            crate::table::cell(spec.name),
            crate::table::cell(TRANSPORT),
            installed_cell,
        ]);
    }
    println!("{table}");
    Ok(())
}

/// `ana mcp setup` — configure clients to use Anaconda MCP.
pub fn setup(
    ctx: &mut CommandContext,
    selected: &[McpClient],
    name: &str,
    no_backup: bool,
    json: bool,
) -> miette::Result<()> {
    let token = require_token(ctx)?;
    let url = remote_url(ctx);

    if selected.is_empty() {
        if !std::io::stdin().is_terminal() {
            return Err(miette!(
                "Missing option '--client'. Run 'ana mcp clients' to see available clients."
            ));
        }
        return setup_wizard(ctx, &url, &token, name, no_backup, json);
    }

    let mut results = serde_json::Map::new();
    let mut failures = 0;

    for client in selected {
        match clients::configure(client.name(), name, &url, &token, !no_backup) {
            Ok(result) => {
                results.insert(client.name().to_string(), configure_json(&result));
                if !json {
                    let action = if result.created {
                        "Created"
                    } else if result.updated {
                        "Updated"
                    } else {
                        "Added"
                    };
                    print_configured(action, client.name(), &result);
                }
            }
            Err(e) => {
                status::error(&format!("{}: {e}", client.name()));
                failures += 1;
            }
        }
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&Value::Object(results)).into_diagnostic()?
        );
    }

    if failures > 0 {
        return Err(miette!("Failed to configure {failures} client(s)"));
    }

    record_install(ctx);
    Ok(())
}

/// `ana mcp remove` — remove Anaconda MCP from client configurations.
pub fn remove(
    selected: &[McpClient],
    name: &str,
    no_backup: bool,
    json: bool,
) -> miette::Result<()> {
    if selected.is_empty() {
        if !std::io::stdin().is_terminal() {
            return Err(miette!(
                "Missing option '--client'. Run 'ana mcp clients' to see available clients."
            ));
        }
        return remove_wizard(name, no_backup, json);
    }

    let names: Vec<&str> = selected.iter().map(|c| c.name()).collect();
    run_removals(&names, name, no_backup, json)
}

/// Interactive remove: multiselect over clients where the entry is installed.
fn remove_wizard(name: &str, no_backup: bool, json: bool) -> miette::Result<()> {
    let installed: Vec<&clients::ClientSpec> = clients::SPECS
        .iter()
        .filter(|s| clients::is_installed(s.name, name))
        .collect();

    if installed.is_empty() {
        status::info("Anaconda MCP is not configured in any supported client.");
        return Ok(());
    }

    let items: Vec<&str> = installed.iter().map(|s| s.name).collect();
    let selections = crate::input::multiselect(
        "Select agents to remove the Anaconda MCP service from",
        &items,
        &[],
    )
    .map_err(|e| miette!("Remove aborted: {e}"))?;

    if selections.is_empty() {
        status::info("No changes.");
        return Ok(());
    }

    let names: Vec<&str> = selections.into_iter().map(|i| items[i]).collect();
    run_removals(&names, name, no_backup, json)
}

/// Remove the server entry from each named client, reporting per-client results.
fn run_removals(names: &[&str], name: &str, no_backup: bool, json: bool) -> miette::Result<()> {
    let mut results = serde_json::Map::new();
    let mut failures = 0;

    for client in names {
        match clients::remove(client, name, !no_backup) {
            Ok(result) => {
                results.insert(client.to_string(), remove_json(&result));
                if !json {
                    print_removed(client, &result);
                }
            }
            Err(e) => {
                status::error(&format!("{client}: {e}"));
                failures += 1;
            }
        }
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&Value::Object(results)).into_diagnostic()?
        );
    }

    if failures > 0 {
        return Err(miette!("Failed to update {failures} client(s)"));
    }
    Ok(())
}

/// Interactive setup: multiselect over all clients, pre-checked where installed.
fn setup_wizard(
    ctx: &mut CommandContext,
    url: &str,
    token: &str,
    name: &str,
    no_backup: bool,
    json: bool,
) -> miette::Result<()> {
    let installed: Vec<bool> = clients::SPECS
        .iter()
        .map(|s| clients::is_installed(s.name, name))
        .collect();
    let items: Vec<&str> = clients::SPECS.iter().map(|s| s.name).collect();

    let selections = crate::input::multiselect(
        "Select agents to configure with the Anaconda MCP service",
        &items,
        &installed,
    )
    .map_err(|e| miette!("Setup aborted: {e}"))?;

    let is_selected = |idx: usize| selections.contains(&idx);
    let adds: Vec<&clients::ClientSpec> = clients::SPECS
        .iter()
        .enumerate()
        .filter(|(i, s)| is_selected(*i) && (!installed[*i] || clients::needs_update(s.name, name)))
        .map(|(_, s)| s)
        .collect();
    let removes: Vec<&clients::ClientSpec> = clients::SPECS
        .iter()
        .enumerate()
        .filter(|(i, _)| !is_selected(*i) && installed[*i])
        .map(|(_, s)| s)
        .collect();

    if adds.is_empty() && removes.is_empty() {
        status::info("No changes.");
        return Ok(());
    }

    let mut results = serde_json::Map::new();
    let mut failures = 0;

    for spec in &adds {
        match clients::configure(spec.name, name, url, token, !no_backup) {
            Ok(result) => {
                results.insert(spec.name.to_string(), configure_json(&result));
                if !json {
                    print_configured("Configured", spec.name, &result);
                }
            }
            Err(e) => {
                status::error(&format!("{}: {e}", spec.name));
                failures += 1;
            }
        }
    }

    for spec in &removes {
        match clients::remove(spec.name, name, !no_backup) {
            Ok(result) => {
                let mut value = remove_json(&result);
                value["action"] = json!("removed");
                results.insert(spec.name.to_string(), value);
                if !json {
                    print_removed(spec.name, &result);
                }
            }
            Err(e) => {
                status::error(&format!("{}: {e}", spec.name));
                failures += 1;
            }
        }
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&Value::Object(results)).into_diagnostic()?
        );
    }

    if failures > 0 {
        return Err(miette!("Failed to update {failures} client(s)"));
    }

    if !adds.is_empty() {
        record_install(ctx);
    }
    Ok(())
}

/// Stamp the new-install telemetry attribute and persist install state.
fn record_install(ctx: &mut CommandContext) {
    ctx.telemetry
        .add("mcp_new_install", state::is_new_install());
    state::mark_installed();
}

fn configure_json(result: &clients::ConfigureResult) -> Value {
    json!({
        "config_path": result.config_path.display().to_string(),
        "backup_path": result.backup_path.as_ref().map(|p| p.display().to_string()),
        "server_name": result.server_name,
        "transport": TRANSPORT,
        "created": result.created,
        "updated": result.updated,
    })
}

fn remove_json(result: &clients::RemoveResult) -> Value {
    let configs: Vec<Value> = result
        .configs
        .iter()
        .map(|config| {
            json!({
                "config_path": config.config_path.display().to_string(),
                "backup_path": config.backup_path.as_ref().map(|p| p.display().to_string()),
            })
        })
        .collect();
    json!({
        "server_name": result.server_name,
        "removed": result.removed,
        "configs": configs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_configure_json_shape() {
        let result = clients::ConfigureResult {
            config_path: "/tmp/mcp.json".into(),
            backup_path: Some("/tmp/mcp.20260915.backup.json".into()),
            server_name: "anaconda-mcp".into(),
            created: true,
            updated: false,
        };
        let value = configure_json(&result);
        assert_eq!(value["transport"], "http");
        assert_eq!(value["created"], true);
        assert_eq!(value["backup_path"], "/tmp/mcp.20260915.backup.json");
    }

    #[test]
    fn test_remove_json_shape() {
        let result = clients::RemoveResult {
            server_name: "anaconda-mcp".into(),
            removed: true,
            configs: vec![
                clients::RemovedConfig {
                    config_path: "/tmp/kilo.jsonc".into(),
                    backup_path: Some("/tmp/kilo.20260915.backup.jsonc".into()),
                },
                clients::RemovedConfig {
                    config_path: "/tmp/kilo.json".into(),
                    backup_path: None,
                },
            ],
        };
        let value = remove_json(&result);
        assert_eq!(value["removed"], true);
        assert_eq!(value["configs"][0]["config_path"], "/tmp/kilo.jsonc");
        assert_eq!(
            value["configs"][0]["backup_path"],
            "/tmp/kilo.20260915.backup.jsonc"
        );
        assert!(value["configs"][1]["backup_path"].is_null());
    }
}
