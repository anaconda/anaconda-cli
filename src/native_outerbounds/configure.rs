use std::path::Path;

use miette::{miette, Result};
use outerbounds::{get_ci_jwt, Outerbounds, ServicePrincipalParams};

use crate::ui::status;

fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/")
        && let Some(home) = dirs::home_dir() {
            return path.replacen("~", &home.to_string_lossy(), 1);
        }
    path.to_string()
}

pub async fn configure(
    encoded_config: &str,
    config_dir: &str,
    profile: Option<&str>,
    echo: bool,
    force: bool,
) -> Result<()> {
    let config_path = expand_tilde(config_dir);
    let ob = Outerbounds::without_config(Some(Path::new(&config_path)), profile);

    let result = ob
        .configure()
        .configure(encoded_config, echo, force)
        .await
        .map_err(|e| miette!("{}", e))?;

    if result.written {
        status::success(&format!("Configuration saved to {}", result.config_path));
    }

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
    name: Option<&str>,
    deployment_domain: Option<&str>,
    perimeter: Option<&str>,
    jwt_token: Option<&str>,
    github_actions: bool,
    config_dir: &str,
    profile: Option<&str>,
    echo: bool,
    force: bool,
) -> Result<()> {
    let config_path = expand_tilde(config_dir);
    let ob = Outerbounds::without_config(Some(Path::new(&config_path)), profile);

    // Get JWT token - either from argument or GitHub Actions OIDC
    let token = match jwt_token {
        Some(t) => t.to_string(),
        None if github_actions => {
            // Need to determine audience from deployment domain
            let domain = deployment_domain.ok_or_else(|| {
                miette!("--deployment-domain is required when using --github-actions")
            })?;
            let audience = format!("https://auth.{}/origin", domain);
            get_ci_jwt(&audience).await.map_err(|e| miette!("{}", e))?
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
        .await
        .map_err(|e| miette!("{}", e))?;

    status::success(&format!(
        "Service principal configuration saved to {}",
        result.config_path
    ));

    Ok(())
}
