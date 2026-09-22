//! Registry of supported AI clients and their MCP configuration files.

use std::path::{Path, PathBuf};

use jsonc_parser::ParseOptions;
use jsonc_parser::cst::{CstInputValue, CstRootNode};
use miette::{IntoDiagnostic, miette};
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
        name: "devin",
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
];

pub fn spec(client: &str) -> Option<&'static ClientSpec> {
    SPECS.iter().find(|s| s.name == client)
}

/// Candidate config file paths for a client, in preference order.
///
/// Most clients have a single file. Kilo has two: `kilo.jsonc` takes
/// precedence over `kilo.json`, matching Kilo's own merge order.
fn config_candidates(client: &str) -> Result<Vec<PathBuf>, McpError> {
    let home = paths::home_dir();
    match client {
        "claude-code" => Ok(vec![home.join(".claude.json")]),
        "cursor" => Ok(vec![home.join(".cursor").join("mcp.json")]),
        // Devin Desktop (rebranded Windsurf) still stores MCP config here.
        "devin" => Ok(vec![
            home.join(".codeium")
                .join("windsurf")
                .join("mcp_config.json"),
        ]),
        "vscode" => Ok(vec![
            dirs::config_dir()
                .unwrap_or(home)
                .join("Code")
                .join("User")
                .join("mcp.json"),
        ]),
        "opencode" => Ok(vec![
            home.join(".config").join("opencode").join("opencode.json"),
        ]),
        "kilo" => {
            let dir = home.join(".config").join("kilo");
            Ok(vec![dir.join("kilo.jsonc"), dir.join("kilo.json")])
        }
        "codex" => {
            let base = std::env::var("CODEX_HOME")
                .ok()
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".codex"));
            Ok(vec![base.join("config.toml")])
        }
        _ => Err(McpError::UnsupportedClient(client.to_string())),
    }
}

/// Config file a client's entry is written to.
///
/// This is the first existing candidate, or the preferred candidate when none
/// exist (created on write). For kilo that means `kilo.jsonc` if present, else
/// an existing `kilo.json`, else a new `kilo.jsonc`.
pub fn config_path(client: &str) -> Result<PathBuf, McpError> {
    let candidates = config_candidates(client)?;
    Ok(candidates
        .iter()
        .find(|path| path.exists())
        .cloned()
        .unwrap_or_else(|| candidates[0].clone()))
}

/// Config files a client's entry may be read from or removed from.
///
/// Returns every existing candidate, or the preferred candidate when none
/// exist.
fn config_paths(client: &str) -> Result<Vec<PathBuf>, McpError> {
    let candidates = config_candidates(client)?;
    let existing: Vec<PathBuf> = candidates
        .iter()
        .filter(|path| path.exists())
        .cloned()
        .collect();
    if existing.is_empty() {
        Ok(vec![candidates[0].clone()])
    } else {
        Ok(existing)
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
        "devin" => json!({ "serverUrl": url, "headers": headers }),
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

/// True when the config file is JSON with comments (`.jsonc`).
fn is_jsonc(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonc"))
}

/// Parse JSONC (comments and trailing commas allowed) into a CST, treating an
/// empty file as an empty object.
fn parse_jsonc(path: &Path, text: &str) -> miette::Result<CstRootNode> {
    let text = if text.trim().is_empty() { "{}" } else { text };
    CstRootNode::parse(text, &ParseOptions::default())
        .map_err(|e| miette!("Failed to parse {}: {e}", path.display()))
}

/// Convert a JSON value into a CST input value for insertion.
fn to_cst_input(value: &Value) -> CstInputValue {
    match value {
        Value::Null => CstInputValue::Null,
        Value::Bool(b) => CstInputValue::Bool(*b),
        Value::Number(n) => CstInputValue::Number(n.to_string()),
        Value::String(s) => CstInputValue::String(s.clone()),
        Value::Array(items) => CstInputValue::Array(items.iter().map(to_cst_input).collect()),
        Value::Object(map) => CstInputValue::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), to_cst_input(value)))
                .collect(),
        ),
    }
}

/// Add or update a server entry in a JSONC document, preserving comments and
/// formatting elsewhere in the file.
fn set_jsonc_entry(
    path: &Path,
    text: &str,
    config_key: &str,
    server_name: &str,
    value: &Value,
) -> miette::Result<String> {
    let root = parse_jsonc(path, text)?;
    let servers = root.object_value_or_set().object_value_or_set(config_key);
    match servers.get(server_name) {
        Some(existing) => existing.set_value(to_cst_input(value)),
        None => {
            servers.append(server_name, to_cst_input(value));
        }
    }
    Ok(root.to_string())
}

/// Remove a server entry from a JSONC document, preserving comments and
/// formatting elsewhere. Returns `None` when the entry is absent.
fn remove_jsonc_entry(
    path: &Path,
    text: &str,
    config_key: &str,
    server_name: &str,
) -> miette::Result<Option<String>> {
    let root = parse_jsonc(path, text)?;
    let Some(servers) = root
        .object_value()
        .and_then(|obj| obj.object_value(config_key))
    else {
        return Ok(None);
    };
    match servers.get(server_name) {
        Some(entry) => {
            entry.remove();
            Ok(Some(root.to_string()))
        }
        None => Ok(None),
    }
}

fn load_json(path: &Path) -> Value {
    let Some(content) = std::fs::read_to_string(path).ok() else {
        return json!({});
    };
    let parsed = if is_jsonc(path) {
        jsonc_parser::parse_to_serde_value(&content, &ParseOptions::default()).ok()
    } else {
        serde_json::from_str(&content).ok()
    };
    parsed
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

/// Write a JSONC document, ensuring a trailing newline.
fn save_jsonc(path: &Path, content: &str) -> miette::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).into_diagnostic()?;
    }
    let mut out = content.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
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
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "config".to_string());
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_else(|| "json".to_string());
    let backup_path = path.with_file_name(format!("{stem}.{timestamp}.backup.{extension}"));
    std::fs::copy(path, &backup_path).into_diagnostic()?;
    Ok(Some(backup_path))
}

/// True when a server entry exists in any of the client's config files.
pub fn is_installed(client: &str, server_name: &str) -> bool {
    let Some(spec) = spec(client) else {
        return false;
    };
    let Ok(paths) = config_paths(client) else {
        return false;
    };
    paths.iter().any(|path| {
        if !path.exists() {
            return false;
        }
        match spec.format {
            ConfigFormat::Json => load_json(path)
                .get(spec.config_key)
                .and_then(|servers| servers.get(server_name))
                .is_some(),
            ConfigFormat::Toml => load_toml(path)
                .get(spec.config_key)
                .and_then(|servers| servers.get(server_name))
                .is_some(),
        }
    })
}

/// True when an entry exists in any config file but is not a remote
/// (URL-based) configuration, e.g. a legacy stdio entry from a previous install.
pub fn needs_update(client: &str, server_name: &str) -> bool {
    let Some(spec) = spec(client) else {
        return false;
    };
    let Ok(paths) = config_paths(client) else {
        return false;
    };
    paths.iter().any(|path| {
        if !path.exists() {
            return false;
        }
        match spec.format {
            ConfigFormat::Json => load_json(path)
                .get(spec.config_key)
                .and_then(|servers| servers.get(server_name))
                .map(|entry| entry.get("url").is_none() && entry.get("serverUrl").is_none())
                .unwrap_or(false),
            ConfigFormat::Toml => load_toml(path)
                .get(spec.config_key)
                .and_then(|servers| servers.get(server_name))
                .map(|entry| entry.get("url").is_none() && entry.get("serverUrl").is_none())
                .unwrap_or(false),
        }
    })
}

/// Result of configuring a client.
#[derive(Debug)]
pub struct ConfigureResult {
    pub config_path: PathBuf,
    pub backup_path: Option<PathBuf>,
    pub server_name: String,
    pub created: bool,
    pub updated: bool,
    /// Path of the Anaconda Package Intelligence skill written for this client.
    pub skill_path: Option<PathBuf>,
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

    let mut result = ConfigureResult {
        created: !path.exists(),
        updated: is_installed(client, server_name),
        config_path: path.clone(),
        backup_path: if backup {
            backup_config_file(&path)?
        } else {
            None
        },
        server_name: server_name.to_string(),
        skill_path: None,
    };

    match spec.format {
        ConfigFormat::Json if is_jsonc(&path) => {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let updated = set_jsonc_entry(
                &path,
                &text,
                spec.config_key,
                server_name,
                &build_json_entry(client, url, token),
            )?;
            save_jsonc(&path, &updated)?;
        }
        ConfigFormat::Json => {
            let mut config = load_json(&path);
            let root = config
                .as_object_mut()
                .expect("load_json always returns an object");
            let servers = root.entry(spec.config_key).or_insert_with(|| json!({}));
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

    // A configured agent also gets the Anaconda Package Intelligence skill.
    result.skill_path = Some(super::skill::install(client)?);

    Ok(result)
}

/// A config file the entry was removed from.
#[derive(Debug)]
pub struct RemovedConfig {
    pub config_path: PathBuf,
    pub backup_path: Option<PathBuf>,
}

/// Result of removing a server entry from a client.
#[derive(Debug)]
pub struct RemoveResult {
    pub server_name: String,
    pub removed: bool,
    /// One entry per config file the server was removed from.
    pub configs: Vec<RemovedConfig>,
}

/// Remove the Anaconda MCP server entry from every config file that has it.
pub fn remove(client: &str, server_name: &str, backup: bool) -> miette::Result<RemoveResult> {
    let spec = spec(client).ok_or_else(|| McpError::UnsupportedClient(client.to_string()))?;
    let paths = config_paths(client)?;

    let mut configs = Vec::new();
    for path in &paths {
        if let Some(removed) = remove_from_path(spec, path, server_name, backup)? {
            configs.push(removed);
        }
    }

    if configs.is_empty() {
        let any_exists = paths.iter().any(|path| path.exists());
        if !any_exists {
            return Err(McpError::ConfigNotFound(paths[0].clone()).into());
        }
        return Err(McpError::ServerNotFound {
            server: server_name.to_string(),
            client: client.to_string(),
        }
        .into());
    }

    Ok(RemoveResult {
        server_name: server_name.to_string(),
        removed: true,
        configs,
    })
}

/// Remove the entry from a single config file, returning `None` when the file
/// does not exist or does not contain the entry.
fn remove_from_path(
    spec: &ClientSpec,
    path: &Path,
    server_name: &str,
    backup: bool,
) -> miette::Result<Option<RemovedConfig>> {
    if !path.exists() {
        return Ok(None);
    }

    match spec.format {
        ConfigFormat::Json if is_jsonc(path) => {
            let text = std::fs::read_to_string(path).into_diagnostic()?;
            let Some(updated) = remove_jsonc_entry(path, &text, spec.config_key, server_name)?
            else {
                return Ok(None);
            };
            let backup_path = if backup {
                backup_config_file(path)?
            } else {
                None
            };
            save_jsonc(path, &updated)?;
            Ok(Some(RemovedConfig {
                config_path: path.to_path_buf(),
                backup_path,
            }))
        }
        ConfigFormat::Json => {
            let mut config = load_json(path);
            let exists = config
                .get(spec.config_key)
                .and_then(|servers| servers.as_object())
                .is_some_and(|servers| servers.contains_key(server_name));
            if !exists {
                return Ok(None);
            }
            let backup_path = if backup {
                backup_config_file(path)?
            } else {
                None
            };
            config
                .get_mut(spec.config_key)
                .and_then(|servers| servers.as_object_mut())
                .expect("checked above")
                .remove(server_name);
            save_json(path, &config)?;
            Ok(Some(RemovedConfig {
                config_path: path.to_path_buf(),
                backup_path,
            }))
        }
        ConfigFormat::Toml => {
            let mut doc = load_toml(path);
            let exists = doc
                .get(spec.config_key)
                .and_then(|servers| servers.as_table_like())
                .is_some_and(|servers| servers.contains_key(server_name));
            if !exists {
                return Ok(None);
            }
            let backup_path = if backup {
                backup_config_file(path)?
            } else {
                None
            };
            doc.get_mut(spec.config_key)
                .and_then(|servers| servers.as_table_like_mut())
                .expect("checked above")
                .remove(server_name);
            save_toml(path, &doc)?;
            Ok(Some(RemovedConfig {
                config_path: path.to_path_buf(),
                backup_path,
            }))
        }
    }
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

        let devin = build_json_entry("devin", URL, TOKEN);
        assert_eq!(devin["serverUrl"], URL);
        assert!(devin.get("url").is_none());

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
        std::fs::write(
            &path,
            r#"{"other": true, "mcpServers": {"existing": {"url": "x"}}}"#,
        )
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
        configure_json_client(
            dir.path(),
            r#"{"other": 1, "mcpServers": {"keep": {"url": "y"}}}"#,
        )
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

    /// Set HOME/USERPROFILE so `config_path` resolves under a temp directory.
    fn with_home<T>(dir: &Path, f: impl FnOnce() -> T) -> T {
        let home = dir.to_string_lossy().to_string();
        temp_env::with_vars(
            [
                ("HOME", Some(home.as_str())),
                ("USERPROFILE", Some(home.as_str())),
            ],
            f,
        )
    }

    #[test]
    #[serial(env)]
    fn test_kilo_config_path_prefers_existing_jsonc() {
        let dir = tempfile::tempdir().unwrap();
        let kilo_dir = dir.path().join(".config").join("kilo");
        std::fs::create_dir_all(&kilo_dir).unwrap();

        with_home(dir.path(), || {
            // Neither file exists: create kilo.jsonc, matching Kilo's default.
            assert_eq!(config_path("kilo").unwrap(), kilo_dir.join("kilo.jsonc"));

            // Only kilo.json exists: use it, don't create kilo.jsonc.
            std::fs::write(kilo_dir.join("kilo.json"), "{}").unwrap();
            assert_eq!(config_path("kilo").unwrap(), kilo_dir.join("kilo.json"));

            // Both exist: kilo.jsonc wins.
            std::fs::write(kilo_dir.join("kilo.jsonc"), "{}").unwrap();
            assert_eq!(config_path("kilo").unwrap(), kilo_dir.join("kilo.jsonc"));
        });
    }

    #[test]
    #[serial(env)]
    fn test_kilo_configure_and_remove_preserve_jsonc() {
        let dir = tempfile::tempdir().unwrap();
        let kilo_dir = dir.path().join(".config").join("kilo");
        std::fs::create_dir_all(&kilo_dir).unwrap();
        let jsonc = kilo_dir.join("kilo.jsonc");
        std::fs::write(&jsonc, "{\n  // keep me\n  \"permission\": {}\n}\n").unwrap();

        with_home(dir.path(), || {
            let result = configure("kilo", "anaconda-mcp", URL, TOKEN, false).unwrap();
            assert_eq!(
                result.skill_path,
                Some(kilo_dir.join("skills").join("anaconda-intelligence").join("SKILL.md"))
            );
            assert!(kilo_dir.join("skills/anaconda-intelligence/SKILL.md").exists());
            assert!(!kilo_dir.join("kilo.json").exists());
            assert!(is_installed("kilo", "anaconda-mcp"));
            assert!(
                std::fs::read_to_string(&jsonc)
                    .unwrap()
                    .contains("// keep me")
            );

            remove("kilo", "anaconda-mcp", false).unwrap();
            assert!(!is_installed("kilo", "anaconda-mcp"));
            assert!(
                std::fs::read_to_string(&jsonc)
                    .unwrap()
                    .contains("// keep me")
            );
        });
    }

    #[test]
    #[serial(env)]
    fn test_kilo_installed_when_entry_only_in_json() {
        let dir = tempfile::tempdir().unwrap();
        let kilo_dir = dir.path().join(".config").join("kilo");
        std::fs::create_dir_all(&kilo_dir).unwrap();
        let jsonc = kilo_dir.join("kilo.jsonc");
        let json = kilo_dir.join("kilo.json");
        // `kilo.jsonc` is higher precedence but has no entry; the entry lives
        // in `kilo.json`.
        std::fs::write(&jsonc, "{\n  // keep me\n  \"permission\": {}\n}\n").unwrap();
        std::fs::write(
            &json,
            r#"{"mcp": {"anaconda-mcp": {"type": "remote", "url": "https://anaconda.com/api/mcp"}}}"#,
        )
        .unwrap();

        with_home(dir.path(), || {
            assert!(is_installed("kilo", "anaconda-mcp"));
            assert!(!needs_update("kilo", "anaconda-mcp"));

            let result = remove("kilo", "anaconda-mcp", false).unwrap();
            assert_eq!(result.configs.len(), 1);
            assert_eq!(result.configs[0].config_path, json);
            assert!(!is_installed("kilo", "anaconda-mcp"));
            assert!(
                std::fs::read_to_string(&jsonc)
                    .unwrap()
                    .contains("// keep me")
            );
            assert!(
                !std::fs::read_to_string(&json)
                    .unwrap()
                    .contains("anaconda-mcp")
            );
        });
    }

    #[test]
    #[serial(env)]
    fn test_kilo_remove_from_both_files() {
        let dir = tempfile::tempdir().unwrap();
        let kilo_dir = dir.path().join(".config").join("kilo");
        std::fs::create_dir_all(&kilo_dir).unwrap();
        let jsonc = kilo_dir.join("kilo.jsonc");
        let json = kilo_dir.join("kilo.json");
        std::fs::write(
            &jsonc,
            "{\n  // keep me\n  \"mcp\": {\"anaconda-mcp\": {\"type\": \"remote\", \"url\": \"https://anaconda.com/api/mcp\"}}\n}\n",
        )
        .unwrap();
        std::fs::write(
            &json,
            r#"{"mcp": {"anaconda-mcp": {"type": "remote", "url": "https://anaconda.com/api/mcp"}}}"#,
        )
        .unwrap();

        with_home(dir.path(), || {
            assert!(is_installed("kilo", "anaconda-mcp"));

            let result = remove("kilo", "anaconda-mcp", false).unwrap();
            assert_eq!(result.configs.len(), 2);
            assert_eq!(result.configs[0].config_path, jsonc);
            assert_eq!(result.configs[1].config_path, json);
            assert!(!is_installed("kilo", "anaconda-mcp"));
            assert!(
                std::fs::read_to_string(&jsonc)
                    .unwrap()
                    .contains("// keep me")
            );
            assert!(
                !std::fs::read_to_string(&jsonc)
                    .unwrap()
                    .contains("anaconda-mcp")
            );
            assert!(
                !std::fs::read_to_string(&json)
                    .unwrap()
                    .contains("anaconda-mcp")
            );

            // Nothing left to remove.
            assert!(remove("kilo", "anaconda-mcp", false).is_err());
        });
    }

    #[test]
    fn test_jsonc_helpers_preserve_comments_and_unrelated_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kilo.jsonc");
        let original = "{\n  // top comment\n  \"permission\": { \"bash\": \"allow\" }\n}\n";

        let added = set_jsonc_entry(
            &path,
            original,
            "mcp",
            "anaconda-mcp",
            &build_json_entry("kilo", URL, TOKEN),
        )
        .unwrap();
        assert!(added.contains("// top comment"));

        std::fs::write(&path, &added).unwrap();
        let parsed = load_json(&path);
        assert_eq!(parsed["permission"]["bash"], "allow");
        assert_eq!(parsed["mcp"]["anaconda-mcp"]["url"], URL);

        let updated = set_jsonc_entry(
            &path,
            &added,
            "mcp",
            "anaconda-mcp",
            &build_json_entry("kilo", "https://new.example/api/mcp", TOKEN),
        )
        .unwrap();
        assert!(updated.contains("// top comment"));
        assert_eq!(updated.matches("anaconda-mcp").count(), 1);

        std::fs::write(&path, &updated).unwrap();
        assert_eq!(
            load_json(&path)["mcp"]["anaconda-mcp"]["url"],
            "https://new.example/api/mcp"
        );

        let removed = remove_jsonc_entry(&path, &updated, "mcp", "anaconda-mcp")
            .unwrap()
            .unwrap();
        assert!(removed.contains("// top comment"));
        assert!(!removed.contains("anaconda-mcp"));
        std::fs::write(&path, &removed).unwrap();
        assert_eq!(load_json(&path)["permission"]["bash"], "allow");
        assert!(
            remove_jsonc_entry(&path, &removed, "mcp", "anaconda-mcp")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn test_jsonc_entry_from_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kilo.jsonc");
        let content = set_jsonc_entry(
            &path,
            "",
            "mcp",
            "anaconda-mcp",
            &build_json_entry("kilo", URL, TOKEN),
        )
        .unwrap();
        std::fs::write(&path, &content).unwrap();
        assert_eq!(load_json(&path)["mcp"]["anaconda-mcp"]["url"], URL);
        assert_eq!(load_json(&path)["mcp"]["anaconda-mcp"]["type"], "remote");
    }

    #[test]
    fn test_backup_config_file_preserves_jsonc_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kilo.jsonc");
        std::fs::write(&path, "{}").unwrap();
        let backup = backup_config_file(&path).unwrap().unwrap();
        let name = backup.file_name().unwrap().to_string_lossy();
        assert!(
            name.starts_with("kilo.") && name.ends_with(".backup.jsonc"),
            "unexpected backup name: {name}"
        );
    }
}
