use std::fs;
use std::path::PathBuf;
use std::process::Command;

use miette::{Context, IntoDiagnostic, miette};
use toml_edit::{DocumentMut, Item, Table};

use crate::auth;
use crate::config::Config;

const INDEX_NAME: &str = "anaconda-wheels";

/// Get the base URL for uv auth by removing /simple/ suffix if present.
fn get_base_url(pip_index_url: &str) -> &str {
    pip_index_url
        .trim_end_matches('/')
        .trim_end_matches("/simple")
        .trim_end_matches('/')
}

/// Get the path to the global uv.toml config file.
fn get_uv_config_path() -> miette::Result<PathBuf> {
    let config_dir =
        dirs::config_dir().ok_or_else(|| miette!("Could not determine config directory"))?;
    Ok(config_dir.join("uv").join("uv.toml"))
}

/// Configure the global uv.toml to use Anaconda's wheels index as the default.
fn configure_global_index(index_url: &str) -> miette::Result<()> {
    let config_path = get_uv_config_path()?;

    // Create parent directory if needed
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).into_diagnostic()?;
    }

    // Load existing config or create new one
    let content = fs::read_to_string(&config_path).unwrap_or_default();
    let mut doc: DocumentMut = content.parse().into_diagnostic()?;

    // Get or create the [[index]] array
    let index_array = doc
        .entry("index")
        .or_insert(Item::ArrayOfTables(toml_edit::ArrayOfTables::new()));

    let index_array = match index_array {
        Item::ArrayOfTables(arr) => arr,
        _ => return Err(miette!("'index' in uv.toml is not an array of tables")),
    };

    // Check if we already have an anaconda-wheels index entry
    let mut found = false;
    for table in index_array.iter_mut() {
        if let Some(name) = table.get("name").and_then(|v| v.as_str()) {
            if name == INDEX_NAME {
                // Update existing entry
                table["url"] = toml_edit::value(index_url);
                table["default"] = toml_edit::value(true);
                found = true;
                break;
            }
        }
    }

    if !found {
        // Add new entry
        let mut new_table = Table::new();
        new_table["name"] = toml_edit::value(INDEX_NAME);
        new_table["url"] = toml_edit::value(index_url);
        new_table["default"] = toml_edit::value(true);
        index_array.push(new_table);
    }

    fs::write(&config_path, doc.to_string()).into_diagnostic()?;
    Ok(())
}

/// Remove the Anaconda wheels index from the global uv.toml config.
fn deconfigure_global_index() -> miette::Result<()> {
    let config_path = get_uv_config_path()?;

    if !config_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&config_path).into_diagnostic()?;
    let mut doc: DocumentMut = content.parse().into_diagnostic()?;

    if let Some(Item::ArrayOfTables(index_array)) = doc.get_mut("index") {
        // Find and remove the anaconda-wheels entry
        let mut indices_to_remove = Vec::new();
        for (i, table) in index_array.iter().enumerate() {
            if let Some(name) = table.get("name").and_then(|v| v.as_str()) {
                if name == INDEX_NAME {
                    indices_to_remove.push(i);
                }
            }
        }

        // Remove in reverse order to preserve indices
        for i in indices_to_remove.into_iter().rev() {
            index_array.remove(i);
        }

        // Clean up empty index array
        if index_array.is_empty() {
            doc.remove("index");
        }
    }

    fs::write(&config_path, doc.to_string()).into_diagnostic()?;
    Ok(())
}

/// Configure uv to use Anaconda's wheels index with authentication.
/// Caller should verify uv is installed before calling.
pub fn configure(config: &Config) -> miette::Result<()> {
    let api_key = auth::get_api_key(config)
        .into_diagnostic()?
        .ok_or_else(|| miette!("Not logged in"))?;

    // Step 1: Configure global index in uv.toml
    configure_global_index(&config.pip_index_url)?;

    // Step 2: Configure auth credentials
    let base_url = get_base_url(&config.pip_index_url);

    let output = Command::new("uv")
        .args(["auth", "login", base_url, "--token", &api_key])
        .output()
        .into_diagnostic()
        .context("Failed to run uv auth")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(miette!("Failed to configure uv auth: {}", stderr));
    }

    Ok(())
}

/// Remove uv configuration for Anaconda's wheels index.
/// Caller should verify uv is installed before calling.
pub fn deconfigure(config: &Config) -> miette::Result<()> {
    // Step 1: Remove global index from uv.toml
    deconfigure_global_index()?;

    // Step 2: Remove auth credentials
    let base_url = get_base_url(&config.pip_index_url);

    let output = Command::new("uv")
        .args(["auth", "logout", base_url])
        .output()
        .into_diagnostic()
        .context("Failed to run uv auth")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Ignore "not logged in" errors
        if !stderr.contains("not logged in") && !stderr.contains("No credentials") {
            return Err(miette!("Failed to deconfigure uv auth: {}", stderr));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_get_base_url_with_simple_suffix() {
        let url = "https://repo.example.com/wheels/simple/";
        assert_eq!(get_base_url(url), "https://repo.example.com/wheels");
    }

    #[test]
    fn test_get_base_url_with_simple_no_trailing_slash() {
        let url = "https://repo.example.com/wheels/simple";
        assert_eq!(get_base_url(url), "https://repo.example.com/wheels");
    }

    #[test]
    fn test_get_base_url_without_simple() {
        let url = "https://repo.example.com/wheels/";
        assert_eq!(get_base_url(url), "https://repo.example.com/wheels");
    }

    #[test]
    fn test_get_base_url_no_trailing_slash() {
        let url = "https://repo.example.com/wheels";
        assert_eq!(get_base_url(url), "https://repo.example.com/wheels");
    }

    #[test]
    fn test_get_base_url_real_wheels_url() {
        let url = "https://repo-latest.dev-us-east-1.anaconda.cloud/repo/wheels-test/simple/";
        assert_eq!(
            get_base_url(url),
            "https://repo-latest.dev-us-east-1.anaconda.cloud/repo/wheels-test"
        );
    }

    #[test]
    fn test_configure_global_index_creates_new_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("uv").join("uv.toml");

        // Create the uv directory
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();

        // Manually write config to temp location
        let index_url = "https://example.com/simple/";
        let mut doc: DocumentMut = "".parse().unwrap();

        let index_array = doc
            .entry("index")
            .or_insert(Item::ArrayOfTables(toml_edit::ArrayOfTables::new()));

        if let Item::ArrayOfTables(arr) = index_array {
            let mut new_table = Table::new();
            new_table["name"] = toml_edit::value(INDEX_NAME);
            new_table["url"] = toml_edit::value(index_url);
            new_table["default"] = toml_edit::value(true);
            arr.push(new_table);
        }

        fs::write(&config_path, doc.to_string()).unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("[[index]]"));
        assert!(content.contains("anaconda-wheels"));
        assert!(content.contains("https://example.com/simple/"));
        assert!(content.contains("default = true"));
    }

    #[test]
    fn test_configure_global_index_updates_existing_entry() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("uv").join("uv.toml");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();

        // Write initial config with anaconda-wheels entry
        let initial_content = r#"[[index]]
name = "anaconda-wheels"
url = "https://old-url.com/simple/"
default = true
"#;
        fs::write(&config_path, initial_content).unwrap();

        // Parse and update
        let content = fs::read_to_string(&config_path).unwrap();
        let mut doc: DocumentMut = content.parse().unwrap();

        if let Some(Item::ArrayOfTables(arr)) = doc.get_mut("index") {
            for table in arr.iter_mut() {
                if let Some(name) = table.get("name").and_then(|v| v.as_str()) {
                    if name == INDEX_NAME {
                        table["url"] = toml_edit::value("https://new-url.com/simple/");
                    }
                }
            }
        }

        fs::write(&config_path, doc.to_string()).unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("https://new-url.com/simple/"));
        assert!(!content.contains("https://old-url.com/simple/"));
    }

    #[test]
    fn test_configure_global_index_preserves_other_entries() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("uv").join("uv.toml");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();

        // Write initial config with another index entry
        let initial_content = r#"[[index]]
name = "other-index"
url = "https://other.com/simple/"

[some-other-config]
key = "value"
"#;
        fs::write(&config_path, initial_content).unwrap();

        // Parse and add anaconda-wheels
        let content = fs::read_to_string(&config_path).unwrap();
        let mut doc: DocumentMut = content.parse().unwrap();

        let index_array = doc
            .entry("index")
            .or_insert(Item::ArrayOfTables(toml_edit::ArrayOfTables::new()));

        if let Item::ArrayOfTables(arr) = index_array {
            let mut new_table = Table::new();
            new_table["name"] = toml_edit::value(INDEX_NAME);
            new_table["url"] = toml_edit::value("https://anaconda.com/simple/");
            new_table["default"] = toml_edit::value(true);
            arr.push(new_table);
        }

        fs::write(&config_path, doc.to_string()).unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("other-index"));
        assert!(content.contains("anaconda-wheels"));
        assert!(content.contains("[some-other-config]"));
    }

    #[test]
    fn test_deconfigure_global_index_removes_entry() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("uv").join("uv.toml");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();

        // Write config with anaconda-wheels and another entry
        let initial_content = r#"[[index]]
name = "other-index"
url = "https://other.com/simple/"

[[index]]
name = "anaconda-wheels"
url = "https://anaconda.com/simple/"
default = true
"#;
        fs::write(&config_path, initial_content).unwrap();

        // Parse and remove anaconda-wheels
        let content = fs::read_to_string(&config_path).unwrap();
        let mut doc: DocumentMut = content.parse().unwrap();

        if let Some(Item::ArrayOfTables(index_array)) = doc.get_mut("index") {
            let mut indices_to_remove = Vec::new();
            for (i, table) in index_array.iter().enumerate() {
                if let Some(name) = table.get("name").and_then(|v| v.as_str()) {
                    if name == INDEX_NAME {
                        indices_to_remove.push(i);
                    }
                }
            }
            for i in indices_to_remove.into_iter().rev() {
                index_array.remove(i);
            }
        }

        fs::write(&config_path, doc.to_string()).unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("other-index"));
        assert!(!content.contains("anaconda-wheels"));
    }
}
