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

// Integration creation functions

fn print_integration_created(integration: &outerbounds::Integration) {
    status::success(&format!(
        "Created integration: {}",
        integration.integration_name
    ));
    println!("Type: {}", integration.integration_type);
    println!("Status: {}", integration.integration_status);
}

pub async fn anaconda_create(
    ctx: &CommandContext,
    name: &str,
    description: Option<&str>,
    perimeter: Option<&str>,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;
    let integrations = ob.integrations();

    let mut builder = integrations.anaconda();
    if let Some(desc) = description {
        builder = builder.description(desc);
    }
    if let Some(p) = perimeter {
        builder = builder.perimeter(p);
    }

    let integration = builder.create(name).await?;
    print_integration_created(&integration);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn artifactory_create(
    ctx: &CommandContext,
    name: &str,
    description: Option<&str>,
    url: &str,
    username: &str,
    password: &str,
    perimeter: Option<&str>,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;
    let integrations = ob.integrations();

    let mut builder = integrations
        .artifactory()
        .domain(url)
        .username(username)
        .password(password);

    if let Some(desc) = description {
        builder = builder.description(desc);
    }
    if let Some(p) = perimeter {
        builder = builder.perimeter(p);
    }

    let integration = builder.create(name).await?;
    print_integration_created(&integration);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn azure_artifacts_create(
    ctx: &CommandContext,
    name: &str,
    description: Option<&str>,
    organization: &str,
    project: &str,
    _feed: &str,
    username: &str,
    pat: &str,
    perimeter: Option<&str>,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;
    let integrations = ob.integrations();

    let mut builder = integrations
        .azure_artifacts()
        .organization(organization)
        .project_name(project)
        .username(username)
        .password(pat);

    if let Some(desc) = description {
        builder = builder.description(desc);
    }
    if let Some(p) = perimeter {
        builder = builder.perimeter(p);
    }

    let integration = builder.create(name).await?;
    print_integration_created(&integration);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn code_artifacts_create(
    ctx: &CommandContext,
    name: &str,
    description: Option<&str>,
    domain_name: &str,
    domain_owner: &str,
    aws_region: &str,
    target_role: Option<&str>,
    perimeter: Option<&str>,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;
    let integrations = ob.integrations();

    let mut builder = integrations
        .code_artifacts()
        .domain_name(domain_name)
        .domain_owner(domain_owner)
        .aws_region(aws_region);

    if let Some(role) = target_role {
        builder = builder.target_role(role);
    }
    if let Some(desc) = description {
        builder = builder.description(desc);
    }
    if let Some(p) = perimeter {
        builder = builder.perimeter(p);
    }

    let integration = builder.create(name).await?;
    print_integration_created(&integration);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn container_registry_create(
    ctx: &CommandContext,
    name: &str,
    description: Option<&str>,
    registry_domain: &str,
    target_role_arn: Option<&str>,
    use_task_role: bool,
    username: Option<&str>,
    password: Option<&str>,
    perimeter: Option<&str>,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;
    let integrations = ob.integrations();

    let mut builder = integrations
        .container_registry()
        .registry_domain(registry_domain);

    if let Some(role) = target_role_arn {
        builder = builder.target_role_arn(role);
    }
    if use_task_role {
        builder = builder.use_task_role(true);
    }
    if let Some(u) = username {
        builder = builder.username(u);
    }
    if let Some(p) = password {
        builder = builder.password(p);
    }
    if let Some(desc) = description {
        builder = builder.description(desc);
    }
    if let Some(p) = perimeter {
        builder = builder.perimeter(p);
    }

    let integration = builder.create(name).await?;
    print_integration_created(&integration);
    Ok(())
}

fn parse_secrets(secrets: &[String]) -> std::collections::HashMap<String, String> {
    secrets
        .iter()
        .filter_map(|s| {
            let parts: Vec<&str> = s.splitn(2, '=').collect();
            if parts.len() == 2 {
                Some((parts[0].to_string(), parts[1].to_string()))
            } else {
                None
            }
        })
        .collect()
}

pub async fn custom_secret_create(
    ctx: &CommandContext,
    name: &str,
    description: Option<&str>,
    secrets: &[String],
    perimeter: Option<&str>,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;
    let integrations = ob.integrations();

    let secret_map = parse_secrets(secrets);
    let mut builder = integrations.custom_secret().secrets(secret_map);

    if let Some(desc) = description {
        builder = builder.description(desc);
    }
    if let Some(p) = perimeter {
        builder = builder.perimeter(p);
    }

    let integration = builder.create(name).await?;
    print_integration_created(&integration);
    Ok(())
}

pub async fn custom_secret_update(
    ctx: &CommandContext,
    name: &str,
    description: Option<&str>,
    secrets: &[String],
    perimeter: Option<&str>,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;
    let integrations = ob.integrations();

    let secret_map = parse_secrets(secrets);
    let mut builder = integrations.custom_secret().secrets(secret_map);

    if let Some(desc) = description {
        builder = builder.description(desc);
    }
    if let Some(p) = perimeter {
        builder = builder.perimeter(p);
    }

    let integration = builder.update(name).await?;
    status::success(&format!(
        "Updated integration: {}",
        integration.integration_name
    ));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn git_pypi_repository_create(
    ctx: &CommandContext,
    name: &str,
    description: Option<&str>,
    repository_url: &str,
    username: Option<&str>,
    password: Option<&str>,
    perimeter: Option<&str>,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;
    let integrations = ob.integrations();

    let mut builder = integrations
        .git_pypi_repository()
        .repository_url(repository_url);

    if let Some(u) = username {
        builder = builder.username(u);
    }
    if let Some(p) = password {
        builder = builder.password(p);
    }
    if let Some(desc) = description {
        builder = builder.description(desc);
    }
    if let Some(p) = perimeter {
        builder = builder.perimeter(p);
    }

    let integration = builder.create(name).await?;
    print_integration_created(&integration);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn gitlab_artifacts_create(
    ctx: &CommandContext,
    name: &str,
    description: Option<&str>,
    gitlab_url: &str,
    project_id: &str,
    username: Option<&str>,
    password: Option<&str>,
    perimeter: Option<&str>,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;
    let integrations = ob.integrations();

    let mut builder = integrations
        .gitlab_artifacts()
        .gitlab_url(gitlab_url)
        .project_id(project_id);

    if let Some(u) = username {
        builder = builder.username(u);
    }
    if let Some(p) = password {
        builder = builder.password(p);
    }
    if let Some(desc) = description {
        builder = builder.description(desc);
    }
    if let Some(p) = perimeter {
        builder = builder.perimeter(p);
    }

    let integration = builder.create(name).await?;
    print_integration_created(&integration);
    Ok(())
}

pub async fn private_conda_channels_add(
    ctx: &CommandContext,
    channel_name: &str,
    host_integration_name: &str,
    is_default: bool,
    perimeter: Option<&str>,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;
    let integrations = ob.integrations();

    let mut builder = integrations.private_conda_channels();
    if let Some(p) = perimeter {
        builder = builder.perimeter(p);
    }

    let integration = builder
        .add(channel_name, host_integration_name, is_default)
        .await?;
    print_integration_created(&integration);
    Ok(())
}

pub async fn private_pypi_repositories_add(
    ctx: &CommandContext,
    repository_name: &str,
    host_integration_name: &str,
    is_default: bool,
    perimeter: Option<&str>,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;
    let integrations = ob.integrations();

    let mut builder = integrations.private_pypi_repositories();
    if let Some(p) = perimeter {
        builder = builder.perimeter(p);
    }

    let integration = builder
        .add(repository_name, host_integration_name, is_default)
        .await?;
    print_integration_created(&integration);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn s3_proxy_create(
    ctx: &CommandContext,
    name: &str,
    description: Option<&str>,
    bucket_name: &str,
    endpoint_url: &str,
    region: &str,
    access_key_id: &str,
    secret_access_key: &str,
    perimeter: Option<&str>,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;
    let integrations = ob.integrations();

    let mut builder = integrations
        .s3_proxy()
        .bucket_name(bucket_name)
        .endpoint_url(endpoint_url)
        .region(region)
        .access_key_id(access_key_id)
        .secret_access_key(secret_access_key);

    if let Some(desc) = description {
        builder = builder.description(desc);
    }
    if let Some(p) = perimeter {
        builder = builder.perimeter(p);
    }

    let integration = builder.create(name).await?;
    print_integration_created(&integration);
    Ok(())
}
