use miette::Result;

use crate::context::CommandContext;
use crate::ui::status;

use super::output::{create_table, print_table};

pub async fn list(ctx: &CommandContext, perimeter_override: Option<&str>) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let result = ob
        .integrations()
        .list_in_perimeter(perimeter_override)
        .await?;

    if result.integrations.is_empty() {
        println!("No integrations found");
        return Ok(());
    }

    let mut table = create_table(&["Name", "Type", "Status", "Description"]);

    for integration in &result.integrations {
        let desc = if integration.integration_description.len() > 40 {
            format!("{}...", &integration.integration_description[..37])
        } else {
            integration.integration_description.clone()
        };
        table.add_row(vec![
            &integration.integration_name,
            &integration.integration_type,
            &integration.integration_status,
            &desc,
        ]);
    }

    print_table(table);
    Ok(())
}

pub async fn get(ctx: &CommandContext, name: &str, perimeter_override: Option<&str>) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let integration = ob
        .integrations()
        .get_in_perimeter(name, perimeter_override)
        .await?;

    println!("Name: {}", integration.integration_name);
    println!("Type: {}", integration.integration_type);
    println!("Status: {}", integration.integration_status);

    if !integration.integration_description.is_empty() {
        println!("Description: {}", integration.integration_description);
    }

    if integration.integration_has_secrets {
        println!("Has secrets: Yes");
        if let Some(ref keys) = integration.integration_secret_keys {
            println!("Secret keys: {}", keys.join(", "));
        }
    }

    if !integration.integration_spec.is_null() {
        println!(
            "\nSpec:\n{}",
            serde_json::to_string_pretty(&integration.integration_spec).unwrap_or_default()
        );
    }

    Ok(())
}

pub async fn delete(
    ctx: &CommandContext,
    name: &str,
    perimeter_override: Option<&str>,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let result = ob
        .integrations()
        .delete_in_perimeter(name, perimeter_override)
        .await?;

    if result.success {
        status::success(&format!("Deleted integration: {}", result.name));
    } else {
        status::warn(&format!("Failed to delete integration: {}", result.name));
    }

    Ok(())
}

pub async fn list_private_pypi(
    ctx: &CommandContext,
    perimeter_override: Option<&str>,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let repos = ob
        .integrations()
        .list_private_pypi_repositories_in_perimeter(perimeter_override)
        .await?;

    if repos.is_empty() {
        println!("No private PyPI repositories found");
        return Ok(());
    }

    let mut table = create_table(&["Repository Name", "Host Integration", "Default"]);

    for repo in &repos {
        let is_default = if repo.is_default { "Yes" } else { "" };
        table.add_row(vec![
            &repo.repository_name,
            &repo.host_integration_name,
            is_default,
        ]);
    }

    print_table(table);
    Ok(())
}

pub async fn list_private_conda(
    ctx: &CommandContext,
    perimeter_override: Option<&str>,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let channels = ob
        .integrations()
        .list_private_conda_channels_in_perimeter(perimeter_override)
        .await?;

    if channels.is_empty() {
        println!("No private Conda channels found");
        return Ok(());
    }

    let mut table = create_table(&["Channel Name", "Host Integration", "Default"]);

    for channel in &channels {
        let is_default = if channel.is_default { "Yes" } else { "" };
        table.add_row(vec![
            &channel.channel_name,
            &channel.host_integration_name,
            is_default,
        ]);
    }

    print_table(table);
    Ok(())
}
