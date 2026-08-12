use miette::{miette, Result};
use outerbounds::{get_ci_jwt, ServicePrincipalParams};

use crate::context::CommandContext;
use crate::ui::status;

pub async fn configure(
    ctx: &CommandContext,
    encoded_config: &str,
    echo: bool,
    force: bool,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;
    let result = ob.configure().configure(encoded_config, echo, force).await?;

    status::success(&format!("Configuration saved to {}", result.config_path));

    if echo {
        println!("\nDecoded configuration:");
        println!(
            "{}",
            serde_json::to_string_pretty(&result.config).unwrap_or_default()
        );
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn service_principal_configure(
    ctx: &CommandContext,
    name: Option<&str>,
    deployment_domain: Option<&str>,
    perimeter: Option<&str>,
    jwt_token: Option<&str>,
    github_actions: bool,
    echo: bool,
    force: bool,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    // Get JWT token - either from argument or GitHub Actions OIDC
    let token = match jwt_token {
        Some(t) => t.to_string(),
        None if github_actions => {
            // Need to determine audience from deployment domain
            let domain = deployment_domain.ok_or_else(|| {
                miette!("--deployment-domain is required when using --github-actions")
            })?;
            let audience = format!("https://auth.{}/origin", domain);
            get_ci_jwt(&audience).await?
        }
        None => {
            return Err(miette!(
                "Either --jwt-token or --github-actions must be specified"
            ));
        }
    };

    let name = name
        .ok_or_else(|| miette!("--name is required"))?
        .to_string();
    let deployment_domain = deployment_domain
        .ok_or_else(|| miette!("--deployment-domain is required"))?
        .to_string();

    let params = ServicePrincipalParams::new(name, deployment_domain, perimeter, token);

    let result = ob
        .configure()
        .service_principal_configure(&params, echo, force)
        .await?;

    status::success(&format!(
        "Service principal configuration saved to {}",
        result.config_path
    ));

    Ok(())
}
