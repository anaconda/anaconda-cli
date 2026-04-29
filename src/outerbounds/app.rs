use std::process::Command;

use serde::Deserialize;

#[derive(Deserialize)]
struct AppInfo {
    url: Option<String>,
}

#[derive(Deserialize)]
struct AppListResponse {
    apps: Vec<AppInfo>,
}

pub fn open_app(name: &str) -> Result<(), String> {
    // Get app info using outerbounds CLI
    let output = Command::new("outerbounds")
        .args(["app", "list", "--output", "json"])
        .output()
        .map_err(|e| format!("Failed to run outerbounds app list: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "outerbounds app list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let response: AppListResponse = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse app list response: {}", e))?;

    // Find app by name
    let app = response
        .apps
        .iter()
        .find(|a| {
            // Match by checking if URL contains the app name
            a.url
                .as_ref()
                .map(|u| u.contains(name))
                .unwrap_or(false)
        })
        .ok_or_else(|| format!("App '{}' not found", name))?;

    let url = app
        .url
        .as_ref()
        .ok_or_else(|| format!("App '{}' has no URL", name))?;

    // Open in browser
    webbrowser::open(url).map_err(|e| format!("Failed to open browser: {}", e))?;

    println!("Opened {} in browser", url);
    Ok(())
}
