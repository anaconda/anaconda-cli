use miette::Result;
use outerbounds::SecretFormat;

use crate::context::CommandContext;

pub async fn get_metadata(ctx: &CommandContext, integration_name: &str) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let metadata = ob.secrets().get_metadata(integration_name).await?;

    println!("Integration: {}", integration_name);
    println!("Backend type: {}", metadata.secret_backend_type);
    println!("Resource ID: {}", metadata.secret_resource_id);

    Ok(())
}

pub async fn get(ctx: &CommandContext, integration_name: &str, json: bool) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let secret = ob.secrets().get(integration_name).await?;

    let format = if json {
        SecretFormat::Json
    } else {
        SecretFormat::Text
    };

    let output = outerbounds::format_secrets(&[secret], format);
    println!("{}", output);

    Ok(())
}

pub async fn get_many(ctx: &CommandContext, integration_names: &[String], json: bool) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let names: Vec<&str> = integration_names.iter().map(|s| s.as_str()).collect();
    let secrets = ob.secrets().get_many(&names).await?;

    let format = if json {
        SecretFormat::Json
    } else {
        SecretFormat::Text
    };

    let output = outerbounds::format_secrets(&secrets, format);
    println!("{}", output);

    Ok(())
}
