use std::fs;
use std::io::Write;
use std::process::Command;

use crate::auth;
use crate::config::Config;

/// Configure uv to use Anaconda's wheels index with authentication.
pub fn configure(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let api_key = auth::get_api_key(config)?
        .ok_or("Login required to configure uv. Run `ana login` first.")?;

    configure_uv(config, &api_key)?;
    Ok(())
}

/// Remove uv configuration for Anaconda's wheels index.
pub fn deconfigure(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let uv_config_dir = dirs::home_dir()
        .ok_or("Could not determine home directory")?
        .join(".config")
        .join("uv");
    let uv_config_path = uv_config_dir.join("uv.toml");

    if !uv_config_path.exists() {
        println!("No uv config found at {}", uv_config_path.display());
        return Ok(());
    }

    let existing_config = fs::read_to_string(&uv_config_path)?;

    // Remove the index block for our URL
    let filtered: Vec<&str> = existing_config
        .split("[[index]]")
        .filter(|block| !block.contains(&config.pip_index_url))
        .collect();

    let new_config = if filtered.len() > 1 {
        filtered.join("[[index]]")
    } else {
        filtered.join("")
    };

    if new_config.trim().is_empty() {
        fs::remove_file(&uv_config_path)?;
        println!("Removed uv config at {}", uv_config_path.display());
    } else {
        fs::write(&uv_config_path, new_config.trim())?;
        println!("Updated uv config at {}", uv_config_path.display());
    }

    Ok(())
}

/// Configure uv to use Anaconda's package index with authentication.
fn configure_uv(config: &Config, api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Get the UV config directory
    let uv_config_dir = dirs::home_dir()
        .ok_or("Could not determine home directory")?
        .join(".config")
        .join("uv");
    let uv_config_path = uv_config_dir.join("uv.toml");

    // Create config directory if needed
    fs::create_dir_all(&uv_config_dir)?;

    // Read existing config or start fresh
    let existing_config = fs::read_to_string(&uv_config_path).unwrap_or_default();

    // Check if we already have the index configured
    let has_index = existing_config.contains(&config.pip_index_url);

    // Build new config content
    if !has_index {
        // Write index configuration (matching team's research format)
        let index_config = format!(
            r#"[[index]]
url = "{}"
default = true
"#,
            config.pip_index_url
        );

        let new_config = if existing_config.is_empty() {
            index_config
        } else {
            format!("{}\n{}", existing_config.trim_end(), index_config)
        };

        let mut file = fs::File::create(&uv_config_path)?;
        file.write_all(new_config.as_bytes())?;
        println!("Config written to {}", uv_config_path.display());
    } else {
        println!("Index already configured in {}", uv_config_path.display());
    }

    // Configure authentication using `uv auth login`
    // Note: `uv auth` subcommand was added in uv 0.7.x
    let output = Command::new("uv")
        .args(["auth", "login", &config.pip_index_url, "--token", api_key])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to configure uv auth: {}", stderr).into());
    }

    println!("Configured uv to use {}", config.pip_index_url);
    Ok(())
}
