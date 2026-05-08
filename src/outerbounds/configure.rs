//! Outerbounds auto-configuration using Anaconda SSO.
//!
//! Flow:
//! 1. Device auth flow to get OIDC JWT
//! 2. Fetch passport to discover available OB instances
//! 3. Call OB auth endpoint with JWT to get magic string
//! 4. Run `outerbounds configure <magic_string>`

use std::process::Command;
use std::time::Duration;

use miette::miette;
use serde::Deserialize;
use tokio::time::sleep;

use crate::auth::responses::{DeviceAuthResponse, OpenIdConfig, TokenErrorResponse, TokenResponse};
use crate::context::CommandContext;
use crate::tools;
use crate::ui::status;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize)]
struct MagicStringResponse {
    #[serde(rename = "magicString")]
    magic_string: String,
}

/// Perform device auth flow and return the JWT access token.
async fn get_jwt_via_device_flow(ctx: &CommandContext) -> miette::Result<String> {
    let client = ctx.unauthenticated_client(REQUEST_TIMEOUT);

    // Fetch OpenID configuration
    let openid_config: OpenIdConfig = client
        .get(ctx.config.well_known_url())
        .send()
        .await
        .map_err(|e| miette!("Failed to fetch OpenID config: {}", e))?
        .json()
        .await
        .map_err(|e| miette!("Failed to parse OpenID config: {}", e))?;

    let device_auth_endpoint = openid_config
        .device_authorization_endpoint
        .ok_or_else(|| miette!("No device_authorization_endpoint in OpenID config"))?;

    // Request device authorization
    let device_response: DeviceAuthResponse = client
        .post(&device_auth_endpoint)
        .form(&[
            ("client_id", ctx.config.client_id.as_str()),
            ("scope", "openid profile email"),
        ])
        .send()
        .await
        .map_err(|e| miette!("Failed to request device auth: {}", e))?
        .json()
        .await
        .map_err(|e| miette!("Failed to parse device auth response: {}", e))?;

    // Display instructions
    let display_uri = device_response
        .verification_uri_complete
        .as_deref()
        .unwrap_or(&device_response.verification_uri);

    // Try to open browser
    let browser_opened = if ctx.config.open_browser {
        let uri = device_response
            .verification_uri_complete
            .as_ref()
            .unwrap_or(&device_response.verification_uri);
        webbrowser::open(uri).is_ok()
    } else {
        false
    };

    if browser_opened {
        status::info(&format!(
            "Opening {} in your browser...",
            status::highlight(&ctx.config.domain)
        ));
    } else {
        status::info("To authenticate, visit:");
    }
    status::blank_line();
    eprintln!("  {}", status::highlight(display_uri));
    if device_response.verification_uri_complete.is_none() {
        status::blank_line();
        status::info(&format!(
            "And enter the code: {}",
            status::highlight(&device_response.user_code)
        ));
    }
    status::blank_line();
    status::waiting("Waiting for authentication...");

    // Poll for token
    let interval = Duration::from_secs(device_response.interval.unwrap_or(5));
    let timeout = Duration::from_secs(device_response.expires_in);
    let start = std::time::Instant::now();

    loop {
        if start.elapsed() > timeout {
            return Err(miette!("Authentication timed out"));
        }

        sleep(interval).await;

        let response = client
            .post(&openid_config.token_endpoint)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", &device_response.device_code),
                ("client_id", &ctx.config.client_id),
            ])
            .send()
            .await
            .map_err(|e| miette!("Token request failed: {}", e))?;

        if response.status().is_success() {
            let token: TokenResponse = response
                .json()
                .await
                .map_err(|e| miette!("Failed to parse token response: {}", e))?;
            status::blank_line();
            status::success("Authentication complete");
            return Ok(token.access_token);
        }

        let error: TokenErrorResponse = response
            .json()
            .await
            .map_err(|e| miette!("Failed to parse error response: {}", e))?;

        match error.error.as_str() {
            "authorization_pending" => continue,
            "slow_down" => {
                sleep(Duration::from_secs(5)).await;
                continue;
            }
            "expired_token" => return Err(miette!("Authentication timed out")),
            "access_denied" => return Err(miette!("Access denied by user")),
            _ => {
                let msg = error.error_description.unwrap_or(error.error);
                return Err(miette!("Authorization error: {}", msg));
            }
        }
    }
}

/// Get magic string from Outerbounds auth endpoint.
async fn get_magic_string(
    ctx: &CommandContext,
    jwt: &str,
    ob_domain: &str,
) -> miette::Result<String> {
    let client = ctx.unauthenticated_client(REQUEST_TIMEOUT);

    // Construct the auth URL: auth.<domain>/generate/obp-magic-string
    let auth_url = format!("https://auth.{}/generate/obp-magic-string", ob_domain);

    status::info(&format!(
        "Fetching configuration from {}...",
        status::highlight(ob_domain)
    ));

    let response = client
        .get(&auth_url)
        .header("Authorization", format!("Bearer {}", jwt))
        .send()
        .await
        .map_err(|e| miette!("Failed to fetch magic string: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(miette!("Failed to get magic string: {} - {}", status, body));
    }

    let magic: MagicStringResponse = response
        .json()
        .await
        .map_err(|e| miette!("Failed to parse magic string response: {}", e))?;

    Ok(magic.magic_string)
}

/// Run `outerbounds configure <magic_string>`.
fn run_ob_configure(magic_string: &str) -> miette::Result<()> {
    let ob_path = crate::paths::bin_path("outerbounds");

    status::info("Running outerbounds configure...");

    let output = Command::new(&ob_path)
        .args(["configure", magic_string])
        .output()
        .map_err(|e| miette!("Failed to run outerbounds configure: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(miette!("outerbounds configure failed: {}", stderr));
    }

    Ok(())
}

/// Auto-configure Outerbounds using Anaconda SSO.
pub async fn auto_configure(ctx: &mut CommandContext, ob_domain: &str) -> miette::Result<()> {
    // Ensure outerbounds tool is installed
    if !crate::paths::bin_path("outerbounds").exists() {
        status::info("Installing outerbounds tool...");
        tools::install::install_tool(ctx, "outerbounds").await?;
        status::blank_line();
    }

    status::info("Configuring Outerbounds via Anaconda SSO");
    status::blank_line();

    // Step 1: Get JWT via device flow
    let jwt = get_jwt_via_device_flow(ctx).await?;

    // Step 2: Get magic string from OB
    let magic_string = get_magic_string(ctx, &jwt, ob_domain).await?;

    // Step 3: Run outerbounds configure
    run_ob_configure(&magic_string)?;

    status::blank_line();
    status::success(&format!(
        "Outerbounds configured for {}",
        status::highlight(ob_domain)
    ));

    Ok(())
}
