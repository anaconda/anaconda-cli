use miette::{miette, Result};
use outerbounds::FlowProjectCommands;

use crate::context::CommandContext;
use crate::ui::status;

use super::output::{create_table, print_table};

fn parse_id(id: &str) -> Result<(String, String)> {
    FlowProjectCommands::parse_id(id).map_err(|e| miette!("{}", e))
}

pub async fn get_metadata(ctx: &CommandContext, id: Option<&str>) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let (project, branch) = match id {
        Some(id_str) => parse_id(id_str)?,
        None => return Err(miette!("--id is required")),
    };

    let metadata = ob.flowproject().get_metadata(&project, &branch).await?;

    match metadata {
        Some(m) => {
            let proj = m.project.as_deref().unwrap_or("-");
            let br = m.branch.as_deref().unwrap_or("-");
            println!("Project: {}", proj);
            println!("Branch: {}", br);

            if !m.workflows.is_empty() {
                println!("\nWorkflows:");
                let mut table = create_table(&["Flow Name", "Template ID"]);
                for wf in &m.workflows {
                    let name = wf.flow_name.as_deref().unwrap_or("-");
                    let template_id = wf.flow_template_id.as_deref().unwrap_or("-");
                    table.add_row(vec![name, template_id]);
                }
                print_table(table);
            }

            if !m.data.is_empty() {
                println!("\nData Assets:");
                let mut table = create_table(&["ID", "Name"]);
                for asset in &m.data {
                    let id = asset.id.as_deref().unwrap_or("-");
                    let name = asset.name.as_deref().unwrap_or("-");
                    table.add_row(vec![id, name]);
                }
                print_table(table);
            }

            if !m.models.is_empty() {
                println!("\nModel Assets:");
                let mut table = create_table(&["ID", "Name"]);
                for asset in &m.models {
                    let id = asset.id.as_deref().unwrap_or("-");
                    let name = asset.name.as_deref().unwrap_or("-");
                    table.add_row(vec![id, name]);
                }
                print_table(table);
            }
        }
        None => {
            println!("No metadata found for {}/{}", project, branch);
        }
    }

    Ok(())
}

pub async fn set_metadata(ctx: &CommandContext, json: &str) -> Result<()> {
    let metadata: serde_json::Value =
        serde_json::from_str(json).map_err(|e| miette!("Invalid JSON metadata: {}", e))?;

    let ob = ctx.outerbounds_client().await?;
    let result = ob.flowproject().set_metadata(metadata).await?;

    println!(
        "{}",
        serde_json::to_string_pretty(&result).map_err(|e| miette!("Failed to serialize: {}", e))?
    );

    Ok(())
}

pub async fn delete_metadata(
    ctx: &CommandContext,
    id: &str,
    yes: bool,
    _output: Option<&str>,
) -> Result<()> {
    let (project, branch) = parse_id(id)?;

    if !yes
        && !crate::input::prompt_yes_no(
            &format!("Delete flowproject metadata for {}/{}?", project, branch),
            false,
        )
    {
        status::info("Aborted.");
        return Ok(());
    }

    let ob = ctx.outerbounds_client().await?;

    let result = ob.flowproject().delete_metadata(&project, &branch).await?;

    if result.success {
        status::success(&format!(
            "Deleted metadata for {}/{}",
            result.project, result.branch
        ));
    } else {
        println!("No metadata found for {}/{}", result.project, result.branch);
    }

    Ok(())
}

pub async fn list_templates(ctx: &CommandContext, id: &str, _output: Option<&str>) -> Result<()> {
    let (project, branch) = parse_id(id)?;
    let ob = ctx.outerbounds_client().await?;

    let templates = ob.flowproject().list_templates(&project, &branch).await?;

    if templates.is_empty() {
        println!("No workflow templates found for {}/{}", project, branch);
        return Ok(());
    }

    println!("Workflow Templates:");
    for template in &templates {
        println!("  - {}", template);
    }

    Ok(())
}

pub async fn teardown_branch(
    ctx: &CommandContext,
    id: &str,
    dry_run: bool,
    yes: bool,
    _output: Option<&str>,
) -> Result<()> {
    let (project, branch) = parse_id(id)?;

    if !dry_run
        && !yes
        && !crate::input::prompt_yes_no(
            &format!(
                "Tear down all deployed resources for {}/{}?",
                project, branch
            ),
            false,
        )
    {
        status::info("Aborted.");
        return Ok(());
    }

    let ob = ctx.outerbounds_client().await?;

    let result = ob
        .flowproject()
        .teardown_branch(&project, &branch, dry_run)
        .await?;

    if dry_run {
        println!("Dry run for {}/{}:", result.project, result.branch);
        println!("\nWould delete:");
    } else {
        println!("Teardown for {}/{}:", result.project, result.branch);
        println!("\nDeleted:");
    }

    if !result.templates_found.is_empty() || !result.templates_deleted.is_empty() {
        let templates = if dry_run {
            &result.templates_found
        } else {
            &result.templates_deleted
        };
        if !templates.is_empty() {
            println!("\nWorkflow Templates:");
            for name in templates {
                println!("  - {}", name);
            }
        }
    }

    if !result.data_assets_found.is_empty() || !result.data_assets_deleted.is_empty() {
        let assets = if dry_run {
            &result.data_assets_found
        } else {
            &result.data_assets_deleted
        };
        if !assets.is_empty() {
            println!("\nData Assets:");
            for name in assets {
                println!("  - {}", name);
            }
        }
    }

    if !result.model_assets_found.is_empty() || !result.model_assets_deleted.is_empty() {
        let assets = if dry_run {
            &result.model_assets_found
        } else {
            &result.model_assets_deleted
        };
        if !assets.is_empty() {
            println!("\nModel Assets:");
            for name in assets {
                println!("  - {}", name);
            }
        }
    }

    if !result.apps_found.is_empty() || !result.apps_deleted.is_empty() {
        let apps = if dry_run {
            &result.apps_found
        } else {
            &result.apps_deleted
        };
        if !apps.is_empty() {
            println!("\nApps:");
            for name in apps {
                println!("  - {}", name);
            }
        }
    }

    if result.has_metadata {
        if dry_run {
            println!("\nFlowproject metadata: would be deleted");
        } else if result.metadata_deleted {
            println!("\nFlowproject metadata: deleted");
        }
    }

    if !result.errors.is_empty() {
        println!("\nErrors:");
        for err in &result.errors {
            println!("  - {}", err);
        }
    }

    Ok(())
}
