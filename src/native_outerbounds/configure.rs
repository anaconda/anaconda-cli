use std::path::Path;

use miette::{Result, miette};
use outerbounds::commands::ObProjectConfig;
use outerbounds::{Outerbounds, ServicePrincipalParams, get_ci_jwt};

use crate::ui::status;

fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/")
        && let Some(home) = dirs::home_dir()
    {
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
    from_obproject_toml: bool,
    toml_path: &str,
    config_dir: &str,
    profile: Option<&str>,
    echo: bool,
    force: bool,
) -> Result<()> {
    let config_path = expand_tilde(config_dir);
    let ob = Outerbounds::without_config(Some(Path::new(&config_path)), profile);

    let mut params = ServicePrincipalParams::new(
        name.unwrap_or_default(),
        deployment_domain.unwrap_or_default(),
        perimeter,
        jwt_token.unwrap_or_default(),
    );

    // Fill in unset values from obproject.toml if requested
    if from_obproject_toml {
        let defaults = ObProjectConfig::from_toml_file(Path::new(toml_path))
            .map_err(|e| miette!("{}", e))?;
        params = params.with_toml_defaults(&defaults);
    }

    // Get JWT token - either from argument or GitHub Actions OIDC
    if params.jwt_token.is_empty() {
        if github_actions {
            params.jwt_token = get_ci_jwt(&params.audience())
                .await
                .map_err(|e| miette!("{}", e))?;
        } else {
            return Err(miette!(
                "No JWT token provided. Please provide either a valid jwt token or set --github-actions"
            ));
        }
    }

    params.validate().map_err(|e| miette!("{}", e))?;

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
