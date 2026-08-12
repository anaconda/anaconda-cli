use miette::{Result, miette};
use outerbounds::{CapsuleFilters, Tag};

use crate::context::CommandContext;
use crate::ui::status;

use super::output::{create_table, print_table};

/// Helper to get api_url and perimeter from config
async fn get_api_context(ctx: &CommandContext) -> Result<(String, String)> {
    let ob = ctx.outerbounds_client().await?;

    let config = ob
        .config()
        .ok_or_else(|| miette!("Config not loaded. Run 'ana obn configure' first."))?;

    let api_url = config
        .obp_api_server
        .as_ref()
        .ok_or_else(|| miette!("OBP_API_SERVER not found in config"))?
        .clone();

    let perimeter =
        ob.perimeter().show_current().await?.ok_or_else(|| {
            miette!("No perimeter set. Run 'ana obn perimeter switch <id>' first.")
        })?;

    Ok((api_url, perimeter))
}

fn parse_tags(tags: &[String]) -> Vec<Tag> {
    tags.iter()
        .filter_map(|t| {
            let parts: Vec<&str> = t.splitn(2, '=').collect();
            if parts.len() == 2 {
                Some(Tag {
                    key: parts[0].to_string(),
                    value: parts[1].to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub async fn list(
    ctx: &CommandContext,
    project: Option<&str>,
    branch: Option<&str>,
    name: Option<&str>,
    tags: &[String],
    format: Option<&str>,
    auth_type: Option<&str>,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;
    let (api_url, perimeter) = get_api_context(ctx).await?;

    let filters = CapsuleFilters {
        project: project.map(|s| s.to_string()),
        branch: branch.map(|s| s.to_string()),
        name: name.map(|s| s.to_string()),
        id: None,
        auth_type: auth_type.map(|s| s.to_string()),
        tags: parse_tags(tags),
    };

    let capsules = ob.app().list(&api_url, &perimeter, filters).await?;

    if format == Some("json") {
        let json = serde_json::to_string_pretty(&capsules)
            .map_err(|e| miette!("Failed to serialize: {}", e))?;
        println!("{}", json);
        return Ok(());
    }

    if capsules.is_empty() {
        println!("No apps found");
        return Ok(());
    }

    let mut table = create_table(&["ID", "Name", "Ready", "Created"]);

    for capsule in &capsules {
        let name = capsule.spec.display_name.as_deref().unwrap_or("-");
        let ready = capsule
            .status
            .as_ref()
            .map(|s| {
                if s.ready_to_serve_traffic {
                    "Yes"
                } else {
                    "No"
                }
            })
            .unwrap_or("unknown");
        let created = capsule
            .metadata
            .as_ref()
            .and_then(|m| m.created_at.as_deref())
            .unwrap_or("-");

        table.add_row(vec![&capsule.id, name, ready, created]);
    }

    print_table(table);
    Ok(())
}

pub async fn info(ctx: &CommandContext, id: &str, format: Option<&str>) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;
    let (api_url, perimeter) = get_api_context(ctx).await?;

    let app_info = ob.app().info(&api_url, &perimeter, id).await?;

    if format == Some("json") {
        let json = serde_json::to_string_pretty(&app_info)
            .map_err(|e| miette!("Failed to serialize: {}", e))?;
        println!("{}", json);
        return Ok(());
    }

    let capsule = &app_info.capsule;

    println!("App ID: {}", capsule.id);

    if let Some(ref name) = capsule.spec.display_name {
        println!("Name: {}", name);
    }

    if let Some(ref status_info) = capsule.status {
        let ready_str = if status_info.ready_to_serve_traffic {
            "Ready"
        } else {
            "Not Ready"
        };
        println!("Status: {}", ready_str);

        if status_info.update_in_progress {
            println!("Update in progress: Yes");
        }

        if let Some(ref version) = status_info.currently_served_version {
            println!("Current version: {}", version);
        }

        if let Some(replicas) = status_info.available_replicas {
            println!("Available replicas: {}", replicas);
        }

        if let Some(ref access) = status_info.access_info
            && let Some(ref url) = access.out_of_cluster_url
        {
            println!("URL: {}", url);
        }
    }

    if let Some(ref metadata) = capsule.metadata {
        if let Some(ref created) = metadata.created_at {
            println!("Created: {}", created);
        }
        if let Some(ref modified) = metadata.last_modified_at {
            println!("Last modified: {}", modified);
        }
    }

    // Print workers
    if !app_info.workers.is_empty() {
        println!("\nWorkers:");
        let mut table = create_table(&["ID", "Phase", "Version"]);
        for worker in &app_info.workers {
            let worker_id = worker.worker_id.as_deref().unwrap_or("-");
            let phase = worker.phase.as_deref().unwrap_or("-");
            let version = worker.version.as_deref().unwrap_or("-");
            table.add_row(vec![worker_id, phase, version]);
        }
        print_table(table);
    }

    Ok(())
}

pub async fn delete(ctx: &CommandContext, ids: &[String]) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;
    let (api_url, perimeter) = get_api_context(ctx).await?;

    let results = ob.app().delete(&api_url, &perimeter, ids).await?;

    for result in &results {
        if result.success {
            status::success(&format!("Deleted app: {}", result.id));
        } else {
            let err = result.error.as_deref().unwrap_or("Unknown error");
            status::warn(&format!("Failed to delete {}: {}", result.id, err));
        }
    }

    Ok(())
}

pub async fn logs(
    ctx: &CommandContext,
    id: &str,
    worker_id: Option<&str>,
    previous: bool,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;
    let (api_url, perimeter) = get_api_context(ctx).await?;

    // If no worker_id provided, get the first worker
    let worker = match worker_id {
        Some(w) => w.to_string(),
        None => {
            let app_info = ob.app().info(&api_url, &perimeter, id).await?;
            app_info
                .workers
                .first()
                .and_then(|w| w.worker_id.clone())
                .ok_or_else(|| miette!("No workers found for app {}", id))?
        }
    };

    let logs = ob
        .app()
        .logs(&api_url, &perimeter, id, &worker, previous)
        .await?;

    for log_line in &logs {
        if let Some(ref msg) = log_line.message {
            println!("{}", msg);
        }
    }

    Ok(())
}
