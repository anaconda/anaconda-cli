use std::fs;
use std::process::Command;

use serde::Deserialize;

use crate::paths;

#[derive(Deserialize)]
struct AppInfo {
    status: AppStatus,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppStatus {
    access_info: Option<AccessInfo>,
}

#[derive(Deserialize)]
struct AccessInfo {
    #[serde(rename = "outOfClusterURL")]
    out_of_cluster_url: Option<String>,
}

#[derive(Deserialize)]
struct AppConfig {
    name: String,
}

pub fn open_app(name: &str) -> Result<(), String> {
    let url = get_app_url(name)?;
    open_url_in_browser(&url)?;
    println!("Opened {} in browser", url);
    Ok(())
}

pub fn view_app(web: bool) -> Result<(), String> {
    let app_name = detect_app_name()?;
    let url = get_app_url(&app_name)?;

    if web {
        open_url_in_browser(&url)?;
        println!("Opened {} in browser", url);
    } else {
        println!("{}", url);
    }
    Ok(())
}

fn detect_app_name() -> Result<String, String> {
    let cwd = std::env::current_dir().map_err(|e| format!("Failed to get current directory: {}", e))?;

    // Look for deployments directory
    let deployments_dir = cwd.join("deployments");
    if !deployments_dir.exists() {
        return Err("No deployments directory found. Are you in an Outerbounds project?".to_string());
    }

    // Find first app config (config.yaml or app.yaml)
    let entries = fs::read_dir(&deployments_dir)
        .map_err(|e| format!("Failed to read deployments directory: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Try config.yaml first, then app.yaml
            for config_name in &["config.yaml", "app.yaml"] {
                let config_path = path.join(config_name);
                if config_path.exists() {
                    let content = fs::read_to_string(&config_path)
                        .map_err(|e| format!("Failed to read {}: {}", config_path.display(), e))?;
                    let config: AppConfig = serde_yaml::from_str(&content)
                        .map_err(|e| format!("Failed to parse {}: {}", config_path.display(), e))?;
                    return Ok(config.name);
                }
            }
        }
    }

    Err("No app config found in deployments directory".to_string())
}

fn get_app_url(name: &str) -> Result<String, String> {
    let ob_bin = paths::bin_path("outerbounds");

    let output = Command::new(&ob_bin)
        .args(["app", "list", "--format", "json", "--name", name])
        .output()
        .map_err(|e| format!("Failed to run outerbounds app list: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "outerbounds app list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let apps: Vec<AppInfo> = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse app list response: {}", e))?;

    // Find the app (should be filtered by name already)
    let app = apps
        .first()
        .ok_or_else(|| format!("App '{}' not found. Is it deployed?", name))?;

    let url = app
        .status
        .access_info
        .as_ref()
        .and_then(|a| a.out_of_cluster_url.clone())
        .ok_or_else(|| format!("App '{}' has no URL", name))?;

    // Add https:// prefix if not present
    if url.starts_with("http://") || url.starts_with("https://") {
        Ok(url)
    } else {
        Ok(format!("https://{}", url))
    }
}

fn open_url_in_browser(url: &str) -> Result<(), String> {
    webbrowser::open(url).map_err(|e| format!("Failed to open browser: {}", e))
}
