//! Registry of supported AI clients and their MCP configuration files.

use std::path::{Path, PathBuf};

use miette::IntoDiagnostic;
use serde_json::{Value, json};

use crate::errors::McpError;
use crate::paths;

/// Config file format used by a client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFormat {
    Json,
    Toml,
}

/// A supported AI client.
#[derive(Debug, Clone, Copy)]
pub struct ClientSpec {
    pub name: &'static str,
    /// Key under which MCP server entries live in the client's config file.
    pub config_key: &'static str,
    pub format: ConfigFormat,
}

/// All supported clients, sorted alphabetically by name.
pub const SPECS: &[ClientSpec] = &[
    ClientSpec {
        name: "claude-code",
        config_key: "mcpServers",
        format: ConfigFormat::Json,
    },
    ClientSpec {
        name: "codex",
        config_key: "mcp_servers",
        format: ConfigFormat::Toml,
    },
    ClientSpec {
        name: "cursor",
        config_key: "mcpServers",
        format: ConfigFormat::Json,
    },
    ClientSpec {
        name: "kilo",
        config_key: "mcp",
        format: ConfigFormat::Json,
    },
    ClientSpec {
        name: "opencode",
        config_key: "mcp",
        format: ConfigFormat::Json,
    },
    ClientSpec {
        name: "vscode",
        config_key: "servers",
        format: ConfigFormat::Json,
    },
    ClientSpec {
        name: "windsurf",
        config_key: "mcpServers",
        format: ConfigFormat::Json,
    },
];

pub fn spec(client: &str) -> Option<&'static ClientSpec> {
    SPECS.iter().find(|s| s.name == client)
}

/// Default config file path for a client.
pub fn config_path(client: &str) -> Result<PathBuf, McpError> {
    let home = paths::home_dir();
    match client {
        "claude-code" => Ok(home.join(".claude.json")),
        "cursor" => Ok(home.join(".cursor").join("mcp.json")),
        "windsurf" => Ok(home
            .join(".codeium")
            .join("windsurf")
            .join("mcp_config.json")),
        "vscode" => Ok(dirs::config_dir()
            .unwrap_or(home)
            .join("Code")
            .join("User")
            .join("mcp.json")),
        "opencode" => Ok(home.join(".config").join("opencode").join("opencode.json")),
        "kilo" => Ok(home.join(".config").join("kilo").join("kilo.json")),
        "codex" => {
            let base = std::env::var("CODEX_HOME")
                .ok()
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".codex"));
            Ok(base.join("config.toml"))
        }
        _ => Err(McpError::UnsupportedClient(client.to_string())),
    }
}

fn auth_headers_json(token: &str) -> Value {
    json!({ "Authorization": format!("Bearer {token}") })
}

/// Build the per-client JSON entry for the remote Anaconda MCP server.
fn build_json_entry(client: &str, url: &str, token: &str) -> Value {
    let headers = auth_headers_json(token);
    match client {
        "claude-code" | "vscode" => json!({ "type": "http", "url": url, "headers": headers }),
        "cursor" => json!({ "url": url, "headers": headers }),
        "windsurf" => json!({ "serverUrl": url, "headers": headers }),
        _ => json!({ "type": "remote", "url": url, "enabled": true, "headers": headers }),
    }
}

/// Build the codex TOML entry for the remote Anaconda MCP server.
fn build_toml_entry(url: &str, token: &str) -> toml_edit::Table {
    let mut entry = toml_edit::Table::new();
    entry["url"] = toml_edit::value(url);
    let mut headers = toml_edit::Table::new();
    headers["Authorization"] = toml_edit::value(format!("Bearer {token}"));
    entry["http_headers"] = toml_edit::Item::Table(headers);
    entry
}

fn load_json(path: &Path) -> Value {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .filter(|v: &Value| v.is_object())
        .unwrap_or_else(|| json!({}))
}

fn save_json(path: &Path, config: &Value) -> miette::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).into_diagnostic()?;
    }
    let mut out = serde_json::to_string_pretty(config).into_diagnostic()?;
    out.push('\n');
    std::fs::write(path, out).into_diagnostic()?;
    Ok(())
}

fn load_toml(path: &Path) -> toml_edit::DocumentMut {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| content.parse::<toml_edit::DocumentMut>().ok())
        .unwrap_or_default()
}

/// Get or create a standard (non-inline) TOML table under `table[key]`.
///
/// Index-assignment on a missing key creates an inline table; explicit
/// insertion renders as a `[table.key]` section instead.
pub(crate) fn ensure_toml_table<'a>(
    table: &'a mut toml_edit::Table,
    key: &str,
) -> &'a mut toml_edit::Item {
    if !table.contains_key(key) {
        let mut item = toml_edit::Table::new();
        // Don't render a bare `[parent]` header when it only holds sub-tables.
        item.set_implicit(true);
        table.insert(key, toml_edit::Item::Table(item));
    }
    &mut table[key]
}

fn save_toml(path: &Path, doc: &toml_edit::DocumentMut) -> miette::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).into_diagnostic()?;
    }
    std::fs::write(path, doc.to_string()).into_diagnostic()?;
    Ok(())
}

/// Create a timestamped backup of a config file.
///
/// Returns `None` when the file does not exist.
pub fn backup_config_file(path: &Path) -> miette::Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let backup_path = path.with_extension(format!("{timestamp}.backup.json"));
    std::fs::copy(path, &backup_path).into_diagnostic()?;
    Ok(Some(backup_path))
}

/// True when a server entry exists in the client's config file.
pub fn is_installed(client: &str, server_name: &str) -> bool {
    let Some(spec) = spec(client) else {
        return false;
    };
    let Ok(path) = config_path(client) else {
        return false;
    };
    if !path.exists() {
        return false;
    }
    match spec.format {
        ConfigFormat::Json => load_json(&path)
            .get(spec.config_key)
            .and_then(|servers| servers.get(server_name))
            .is_some(),
        ConfigFormat::Toml => load_toml(&path)
            .get(spec.config_key)
            .and_then(|servers| servers.get(server_name))
            .is_some(),
    }
}

/// True when an entry exists but is not a remote (URL-based) configuration,
/// e.g. a legacy stdio entry left over from a previous install.
pub fn needs_update(client: &str, server_name: &str) -> bool {
    let Some(spec) = spec(client) else {
        return false;
    };
    let Ok(path) = config_path(client) else {
        return false;
    };
    if !path.exists() {
        return false;
    }
    match spec.format {
        ConfigFormat::Json => load_json(&path)
            .get(spec.config_key)
            .and_then(|servers| servers.get(server_name))
            .map(|entry| entry.get("url").is_none() && entry.get("serverUrl").is_none())
            .unwrap_or(false),
        ConfigFormat::Toml => load_toml(&path)
            .get(spec.config_key)
            .and_then(|servers| servers.get(server_name))
            .map(|entry| entry.get("url").is_none() && entry.get("serverUrl").is_none())
            .unwrap_or(false),
    }
}

/// Result of configuring a client.
#[derive(Debug)]
pub struct ConfigureResult {
    pub config_path: PathBuf,
    pub backup_path: Option<PathBuf>,
    pub server_name: String,
    pub created: bool,
    pub updated: bool,
}

/// Add or update the Anaconda MCP server entry in a client's config file.
pub fn configure(
    client: &str,
    server_name: &str,
    url: &str,
    token: &str,
    backup: bool,
) -> miette::Result<ConfigureResult> {
    let spec = spec(client).ok_or_else(|| McpError::UnsupportedClient(client.to_string()))?;
    let path = config_path(client)?;

    let result = ConfigureResult {
        created: !path.exists(),
        updated: is_installed(client, server_name),
        config_path: path.clone(),
        backup_path: if backup {
            backup_config_file(&path)?
        } else {
            None
        },
        server_name: server_name.to_string(),
    };

    match spec.format {
        ConfigFormat::Json => {
            let mut config = load_json(&path);
            let root = config
                .as_object_mut()
                .expect("load_json always returns an object");
            let servers = root
                .entry(spec.config_key)
                .or_insert_with(|| json!({}));
            if !servers.is_object() {
                *servers = json!({});
            }
            servers
                .as_object_mut()
                .expect("servers is an object")
                .insert(
                    server_name.to_string(),
                    build_json_entry(client, url, token),
                );
            save_json(&path, &config)?;
        }
        ConfigFormat::Toml => {
            let mut doc = load_toml(&path);
            let servers = ensure_toml_table(doc.as_table_mut(), spec.config_key);
            servers[server_name] = toml_edit::Item::Table(build_toml_entry(url, token));
            save_toml(&path, &doc)?;
        }
    }

    Ok(result)
}

/// Result of removing a server entry from a client.
#[derive(Debug)]
pub struct RemoveResult {
    pub config_path: PathBuf,
    pub backup_path: Option<PathBuf>,
    pub server_name: String,
    pub removed: bool,
}

/// Remove the Anaconda MCP server entry from a client's config file.
pub fn remove(client: &str, server_name: &str, backup: bool) -> miette::Result<RemoveResult> {
    let spec = spec(client).ok_or_else(|| McpError::UnsupportedClient(client.to_string()))?;
    let path = config_path(client)?;

    if !path.exists() {
        return Err(McpError::ConfigNotFound(path).into());
    }

    let backup_path = if backup {
        backup_config_file(&path)?
    } else {
        None
    };

    match spec.format {
        ConfigFormat::Json => {
            let mut config = load_json(&path);
            let exists = config
                .get(spec.config_key)
                .and_then(|servers| servers.as_object())
                .is_some_and(|servers| servers.contains_key(server_name));
            if !exists {
                return Err(McpError::ServerNotFound {
                    server: server_name.to_string(),
                    client: client.to_string(),
                }
                .into());
            }
            config
                .get_mut(spec.config_key)
                .and_then(|servers| servers.as_object_mut())
                .expect("checked above")
                .remove(server_name);
            save_json(&path, &config)?;
        }
        ConfigFormat::Toml => {
            let mut doc = load_toml(&path);
            let exists = doc
                .get(spec.config_key)
                .and_then(|servers| servers.as_table_like())
                .is_some_and(|servers| servers.contains_key(server_name));
            if !exists {
                return Err(McpError::ServerNotFound {
                    server: server_name.to_string(),
                    client: client.to_string(),
                }
                .into());
            }
            doc.get_mut(spec.config_key)
                .and_then(|servers| servers.as_table_like_mut())
                .expect("checked above")
                .remove(server_name);
            save_toml(&path, &doc)?;
        }
    }

    Ok(RemoveResult {
        config_path: path,
        backup_path,
        server_name: server_name.to_string(),
        removed: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    const URL: &str = "https://anaconda.com/api/mcp";
    const TOKEN: &str = "test-token";

    #[test]
    fn test_specs_sorted_and_unique() {
        let names: Vec<_> = SPECS.iter().map(|s| s.name).collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(names.len(), sorted.len());
    }

    #[test]
    fn test_unsupported_client() {
        assert!(spec("emacs").is_none());
        assert!(matches!(
            config_path("emacs"),
            Err(McpError::UnsupportedClient(_))
        ));
    }

    #[test]
    fn test_json_entry_shapes() {
        let cc = build_json_entry("claude-code", URL, TOKEN);
        assert_eq!(cc["type"], "http");
        assert_eq!(cc["url"], URL);
        assert_eq!(cc["headers"]["Authorization"], "Bearer test-token");

        let cursor = build_json_entry("cursor", URL, TOKEN);
        assert!(cursor.get("type").is_none());
        assert_eq!(cursor["url"], URL);

        let windsurf = build_json_entry("windsurf", URL, TOKEN);
        assert_eq!(windsurf["serverUrl"], URL);
        assert!(windsurf.get("url").is_none());

        for client in ["kilo", "opencode"] {
            let entry = build_json_entry(client, URL, TOKEN);
            assert_eq!(entry["type"], "remote");
            assert_eq!(entry["enabled"], true);
            assert_eq!(entry["url"], URL);
        }
    }

    #[test]
    fn test_toml_entry_shape() {
        let entry = build_toml_entry(URL, TOKEN);
        assert_eq!(entry["url"].as_str(), Some(URL));
        assert_eq!(
            entry["http_headers"]["Authorization"].as_str(),
            Some("Bearer test-token")
        );
    }

    fn configure_json_client(dir: &Path, config: &str) -> miette::Result<()> {
        let path = dir.join("mcp.json");
        std::fs::write(&path, config).unwrap();
        let mut doc: Value = serde_json::from_str(config).unwrap();
        doc["mcpServers"]["anaconda-mcp"] = build_json_entry("cursor", URL, TOKEN);
        save_json(&path, &doc)
    }

    #[test]
    fn test_json_configure_preserves_unrelated_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        std::fs::write(&path, r#"{"other": true, "mcpServers": {"existing": {"url": "x"}}}"#)
            .unwrap();

        let mut config = load_json(&path);
        config["mcpServers"]["anaconda-mcp"] = build_json_entry("cursor", URL, TOKEN);
        save_json(&path, &config).unwrap();

        let written: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["other"], true);
        assert_eq!(written["mcpServers"]["existing"]["url"], "x");
        assert_eq!(written["mcpServers"]["anaconda-mcp"]["url"], URL);
    }

    #[test]
    fn test_json_remove_preserves_unrelated_keys() {
        let dir = tempfile::tempdir().unwrap();
        configure_json_client(dir.path(), r#"{"other": 1, "mcpServers": {"keep": {"url": "y"}}}"#)
            .unwrap();
        let path = dir.path().join("mcp.json");

        let mut config = load_json(&path);
        config["mcpServers"]
            .as_object_mut()
            .unwrap()
            .remove("anaconda-mcp");
        save_json(&path, &config).unwrap();

        let written: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(written["mcpServers"].get("anaconda-mcp").is_none());
        assert_eq!(written["mcpServers"]["keep"]["url"], "y");
        assert_eq!(written["other"], 1);
    }

    #[test]
    fn test_toml_configure_and_remove_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[settings]\nfoo = 1\n").unwrap();

        let mut doc = load_toml(&path);
        let servers = ensure_toml_table(doc.as_table_mut(), "mcp_servers");
        servers["anaconda-mcp"] = toml_edit::Item::Table(build_toml_entry(URL, TOKEN));
        save_toml(&path, &doc).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("[settings]"));
        assert!(content.contains("[mcp_servers.anaconda-mcp]"));

        let mut doc = load_toml(&path);
        assert!(doc["mcp_servers"].get("anaconda-mcp").is_some());
        doc["mcp_servers"]
            .as_table_like_mut()
            .unwrap()
            .remove("anaconda-mcp");
        save_toml(&path, &doc).unwrap();

        let doc = load_toml(&path);
        assert!(
            doc.get("mcp_servers")
                .and_then(|servers| servers.get("anaconda-mcp"))
                .is_none()
        );
        assert_eq!(doc["settings"]["foo"].as_integer(), Some(1));
    }

    #[test]
    fn test_backup_config_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        assert!(backup_config_file(&path).unwrap().is_none());

        std::fs::write(&path, "{}").unwrap();
        let backup = backup_config_file(&path).unwrap().unwrap();
        assert!(backup.exists());
        let name = backup.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("mcp.") && name.ends_with(".backup.json"));
    }

    #[test]
    #[serial(env)]
    fn test_codex_config_path_respects_codex_home() {
        temp_env::with_var("CODEX_HOME", Some("/tmp/codex-test"), || {
            assert_eq!(
                config_path("codex").unwrap(),
                PathBuf::from("/tmp/codex-test/config.toml")
            );
        });
    }
}
