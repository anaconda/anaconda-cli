//! Plugin discovery and delegation for anaconda CLI plugins.
//!
//! Discovers plugins registered via the `anaconda_cli.subcommand` entry point group
//! and delegates subcommand execution to them.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::time::SystemTime;

use miette::{IntoDiagnostic, Result, miette};
use serde::{Deserialize, Serialize};

/// Entry point group name for anaconda CLI subcommand plugins.
const ENTRY_POINT_GROUP: &str = "anaconda_cli.subcommand";

/// Plugin metadata discovered from entry points.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    /// The subcommand name (e.g., "upload", "download")
    pub name: String,
    /// The module path (e.g., "anaconda_cli_base.upload")
    pub module: String,
    /// The attribute/function to call (e.g., "main")
    pub attr: Option<String>,
    /// The package that provides this plugin
    pub package: String,
    /// Short description for help text
    pub summary: Option<String>,
}

/// Cached plugin registry with invalidation support.
#[derive(Debug, Serialize, Deserialize)]
struct PluginCache {
    /// Cache format version
    version: u32,
    /// CONDA_PREFIX this cache was built for
    conda_prefix: String,
    /// Modification time of conda-meta when cache was built (as secs since epoch)
    conda_meta_mtime: u64,
    /// Discovered plugins
    plugins: Vec<Plugin>,
}

impl PluginCache {
    const VERSION: u32 = 1;
}

/// Get the cache file path for the current environment.
fn cache_path() -> Option<PathBuf> {
    let conda_prefix = std::env::var("CONDA_PREFIX").ok()?;
    Some(PathBuf::from(&conda_prefix).join(".ana-plugins.json"))
}

/// Get the modification time of the conda-meta directory.
fn conda_meta_mtime() -> Option<u64> {
    let conda_prefix = std::env::var("CONDA_PREFIX").ok()?;
    let conda_meta = PathBuf::from(&conda_prefix).join("conda-meta");
    let metadata = std::fs::metadata(&conda_meta).ok()?;
    let mtime = metadata.modified().ok()?;
    mtime
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Load plugins from cache if valid, otherwise discover and cache.
pub fn load_plugins() -> Vec<Plugin> {
    // No CONDA_PREFIX means no plugins
    let Some(conda_prefix) = std::env::var("CONDA_PREFIX").ok() else {
        return Vec::new();
    };

    // Try to load from cache
    if let Some(plugins) = load_from_cache(&conda_prefix) {
        return plugins;
    }

    // Discover plugins and cache
    let plugins = discover_plugins();
    if let Err(e) = save_to_cache(&conda_prefix, &plugins) {
        tracing::debug!("Failed to save plugin cache: {}", e);
    }
    plugins
}

/// Load plugins from cache if valid.
fn load_from_cache(conda_prefix: &str) -> Option<Vec<Plugin>> {
    let cache_file = cache_path()?;
    let content = std::fs::read_to_string(&cache_file).ok()?;
    let cache: PluginCache = serde_json::from_str(&content).ok()?;

    // Validate cache
    if cache.version != PluginCache::VERSION {
        tracing::debug!("Plugin cache version mismatch");
        return None;
    }
    if cache.conda_prefix != conda_prefix {
        tracing::debug!("Plugin cache conda_prefix mismatch");
        return None;
    }

    // Check if conda-meta has been modified
    let current_mtime = conda_meta_mtime()?;
    if cache.conda_meta_mtime != current_mtime {
        tracing::debug!("Plugin cache stale (conda-meta modified)");
        return None;
    }

    tracing::debug!("Loaded {} plugins from cache", cache.plugins.len());
    Some(cache.plugins)
}

/// Save plugins to cache.
fn save_to_cache(conda_prefix: &str, plugins: &[Plugin]) -> std::io::Result<()> {
    let Some(cache_file) = cache_path() else {
        return Ok(());
    };
    let Some(mtime) = conda_meta_mtime() else {
        return Ok(());
    };

    let cache = PluginCache {
        version: PluginCache::VERSION,
        conda_prefix: conda_prefix.to_string(),
        conda_meta_mtime: mtime,
        plugins: plugins.to_vec(),
    };

    let content = serde_json::to_string_pretty(&cache)?;
    std::fs::write(&cache_file, content)?;
    tracing::debug!("Saved {} plugins to cache", plugins.len());
    Ok(())
}

/// Discover plugins by querying Python's importlib.metadata.
fn discover_plugins() -> Vec<Plugin> {
    let python_code = format!(
        r#"
import json
import sys
try:
    from importlib.metadata import entry_points
except ImportError:
    print("[]")
    sys.exit(0)

try:
    # Python 3.10+ returns a SelectableGroups, 3.9 returns a dict
    eps = entry_points()
    if hasattr(eps, 'select'):
        group = eps.select(group="{group}")
    else:
        group = eps.get("{group}", [])

    plugins = []
    for ep in group:
        plugins.append({{
            "name": ep.name,
            "module": ep.value.split(":")[0],
            "attr": ep.value.split(":")[1] if ":" in ep.value else None,
            "package": ep.dist.name if ep.dist else "unknown",
            "summary": None,
        }})
    print(json.dumps(plugins))
except Exception as e:
    print("[]", file=sys.stderr)
    sys.exit(0)
"#,
        group = ENTRY_POINT_GROUP
    );

    let output = Command::new("python").args(["-c", &python_code]).output();

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<Vec<Plugin>>(stdout.trim()) {
                Ok(plugins) => {
                    tracing::debug!("Discovered {} plugins", plugins.len());
                    plugins
                }
                Err(e) => {
                    tracing::debug!("Failed to parse plugin discovery output: {}", e);
                    Vec::new()
                }
            }
        }
        Ok(output) => {
            tracing::debug!(
                "Plugin discovery failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            Vec::new()
        }
        Err(e) => {
            tracing::debug!("Failed to run Python for plugin discovery: {}", e);
            Vec::new()
        }
    }
}

/// Get plugin descriptions for help output.
pub fn get_plugin_descriptions() -> HashMap<String, String> {
    load_plugins()
        .into_iter()
        .map(|p| {
            let desc = p.summary.unwrap_or_else(|| format!("(from {})", p.package));
            (p.name, desc)
        })
        .collect()
}

/// Find a plugin by subcommand name.
pub fn find_plugin(name: &str) -> Option<Plugin> {
    load_plugins().into_iter().find(|p| p.name == name)
}

/// Run a plugin subcommand by delegating to Python.
pub fn run_plugin(plugin: &Plugin, args: &[String]) -> Result<()> {
    // Build the Python invocation using the entry point's callable
    // We set sys.argv[0] to "ana <subcommand>" for proper help output
    let status = if let Some(attr) = &plugin.attr {
        let code = format!(
            r#"
import sys
sys.argv[0] = "ana {name}"
from {module} import {attr}
{attr}()
"#,
            name = plugin.name,
            module = plugin.module,
            attr = attr
        );
        Command::new("python")
            .args(["-c", &code])
            .args(args)
            .status()
            .into_diagnostic()?
    } else {
        // For module-style entry points, use -m
        Command::new("python")
            .args(["-m", &plugin.module])
            .args(args)
            .status()
            .into_diagnostic()?
    };

    if status.success() {
        Ok(())
    } else {
        Err(miette!(
            "{} exited with code {}",
            plugin.name,
            status.code().unwrap_or(1)
        ))
    }
}

/// Invalidate the plugin cache (forces re-discovery on next load).
#[allow(dead_code)]
pub fn invalidate_cache() {
    if let Some(cache_file) = cache_path() {
        let _ = std::fs::remove_file(&cache_file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_serialization() {
        let plugin = Plugin {
            name: "upload".to_string(),
            module: "anaconda_cli_base.upload".to_string(),
            attr: Some("main".to_string()),
            package: "anaconda-client".to_string(),
            summary: Some("Upload packages".to_string()),
        };

        let json = serde_json::to_string(&plugin).unwrap();
        let parsed: Plugin = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "upload");
        assert_eq!(parsed.module, "anaconda_cli_base.upload");
    }

    #[test]
    fn test_cache_serialization() {
        let cache = PluginCache {
            version: PluginCache::VERSION,
            conda_prefix: "/home/user/miniconda3".to_string(),
            conda_meta_mtime: 1234567890,
            plugins: vec![Plugin {
                name: "test".to_string(),
                module: "test_module".to_string(),
                attr: None,
                package: "test-package".to_string(),
                summary: None,
            }],
        };

        let json = serde_json::to_string_pretty(&cache).unwrap();
        let parsed: PluginCache = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, PluginCache::VERSION);
        assert_eq!(parsed.plugins.len(), 1);
    }
}
