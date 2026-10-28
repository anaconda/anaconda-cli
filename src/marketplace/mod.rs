//! `ana marketplace` — conda packages as kilo MCP servers.
//!
//! `ana marketplace install <package>` solves a conda package that provides an
//! MCP server with rattler into its own prefix under
//! `~/.ana/marketplace/<name>`, finds the server executable the package
//! installs, and registers it as a local MCP server in kilo's config — all in
//! one command. `ana marketplace remove` undoes both steps.

mod commands;

pub use commands::MarketplaceCommands;

use std::path::{Path, PathBuf};

use miette::{Context, IntoDiagnostic, miette};
use rattler_conda_types::prefix_record::PathType;
use rattler_conda_types::{MatchSpec, PackageName, ParseStrictness, PrefixRecord};
use serde_json::{Value, json};

use crate::context::CommandContext;
use crate::mcp::clients;
use crate::paths;
use crate::tools::install;
use crate::ui::status;

/// MCP client servers are registered with.
const CLIENT: &str = "kilo";

/// Channels packages are solved against, in strict priority order.
const DEFAULT_CHANNELS: &[&str] = &[
    "https://repo.anaconda.com/pkgs/main",
    "https://conda.anaconda.org/conda-forge",
];

/// Marker key in the kilo entry identifying it as managed by `ana marketplace`.
/// Kilo ignores unknown keys and preserves them when it rewrites its config.
const MANAGED_MARKER: &str = "x-ana-marketplace";

/// A curated MCP server available as a conda package.
struct CatalogEntry {
    /// Conda package name; also the default server name in kilo.
    package: &'static str,
    /// Channel the package is published on.
    channel: &'static str,
    /// Executable the package installs that runs the server.
    command: &'static str,
    /// Arguments that start the server on stdio.
    args: &'static [&'static str],
    /// Extra match specs solved alongside the package, for upstream
    /// recipes whose dependency bounds admit versions the server can't run on.
    constraints: &'static [&'static str],
    description: &'static str,
}

const CONDA_FORGE: &str = "https://conda.anaconda.org/conda-forge";

/// Keeps Python servers off the 3.15 pre-release, which many conda-forge
/// packages don't support yet.
const STABLE_PYTHON: &str = "python<3.15";

/// MCP servers offered by `ana marketplace list`, sorted by package name.
const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        package: "conda-meta-mcp",
        channel: CONDA_FORGE,
        command: "cmm",
        args: &["run"],
        constraints: &[STABLE_PYTHON],
        description: "Conda ecosystem context: package metadata, files, and dependencies",
    },
    CatalogEntry {
        package: "excel-mcp-server",
        channel: CONDA_FORGE,
        command: "excel-mcp-server",
        args: &["stdio"],
        // Uses the mcp 1.x FastMCP API, but its recipe allows mcp 2.x.
        constraints: &[STABLE_PYTHON, "mcp<2"],
        description: "Read and edit Excel workbooks",
    },
    CatalogEntry {
        package: "markitdown-mcp-server",
        channel: CONDA_FORGE,
        command: "markitdown-mcp-server",
        args: &[],
        // Uses the mcp 1.x FastMCP API, but its recipe allows mcp 2.x.
        constraints: &[STABLE_PYTHON, "mcp<2"],
        description: "Convert documents to Markdown with MarkItDown",
    },
    CatalogEntry {
        package: "mcp-atlassian",
        channel: CONDA_FORGE,
        command: "mcp-atlassian",
        args: &[],
        constraints: &[STABLE_PYTHON],
        description: "Jira and Confluence (set credentials with --env)",
    },
    CatalogEntry {
        package: "mcp-grafana",
        channel: CONDA_FORGE,
        command: "mcp-grafana",
        args: &[],
        constraints: &[],
        description: "Grafana dashboards and data sources (set GRAFANA_URL with --env)",
    },
    CatalogEntry {
        package: "mcp-server-fetch",
        channel: CONDA_FORGE,
        command: "mcp-server-fetch",
        args: &[],
        constraints: &[STABLE_PYTHON],
        description: "Fetch web pages and convert them to Markdown",
    },
];

fn catalog_entry(package: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|e| e.package == package)
}

/// Run an `ana marketplace` subcommand.
pub async fn run(ctx: &mut CommandContext, command: MarketplaceCommands) -> miette::Result<()> {
    match command {
        MarketplaceCommands::Install {
            package,
            channel,
            command,
            name,
            env,
        } => install(ctx, &package, &channel, command, name, &env).await,
        MarketplaceCommands::List { json } => list(json),
        MarketplaceCommands::Remove { name } => remove(&name),
    }
}

fn marketplace_dir() -> PathBuf {
    paths::ana_home().join("marketplace")
}

fn server_prefix(name: &str) -> PathBuf {
    marketplace_dir().join(name)
}

/// Channels to solve against: `--channel` values when given, else
/// `ANA_MARKETPLACE_CHANNELS` (comma-separated), else the defaults.
fn channels(requested: &[String]) -> Vec<String> {
    if !requested.is_empty() {
        return requested.to_vec();
    }
    let from_env: Vec<String> = std::env::var("ANA_MARKETPLACE_CHANNELS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(String::from)
        .collect();
    if from_env.is_empty() {
        DEFAULT_CHANNELS.iter().map(|c| c.to_string()).collect()
    } else {
        from_env
    }
}

/// The package name of a match spec such as `mcp-server-fetch>=2026`.
fn package_name(spec: &str) -> miette::Result<PackageName> {
    let parsed = MatchSpec::from_str(spec, ParseStrictness::Lenient)
        .into_diagnostic()
        .with_context(|| format!("invalid package spec: {spec}"))?;
    match parsed.name {
        rattler_conda_types::PackageNameMatcher::Exact(name) => Ok(name),
        _ => Err(miette!("package spec must name a single package: {spec}")),
    }
}

/// Executables a package installed into its prefix's `bin/` (`Scripts\` on
/// Windows), including Python entry points.
fn package_executables(record: &PrefixRecord) -> Vec<PathBuf> {
    let bin_subdir = if cfg!(windows) { "Scripts" } else { "bin" };
    record
        .paths_data
        .paths
        .iter()
        .filter(|entry| entry.relative_path.starts_with(bin_subdir))
        .filter(|entry| {
            !matches!(
                entry.path_type,
                PathType::Directory | PathType::WindowsPythonEntryPointScript
            )
        })
        .filter(|entry| {
            !cfg!(windows)
                || entry
                    .relative_path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        })
        .map(|entry| entry.relative_path.clone())
        .collect()
}

/// Choose the MCP server executable among a package's executables.
///
/// An explicit `--command` must match one of them by file stem. Otherwise a
/// single executable is used, or the one named after the package when there
/// are several.
fn choose_server(
    package: &str,
    executables: &[PathBuf],
    requested: Option<&str>,
) -> miette::Result<PathBuf> {
    let stem = |p: &PathBuf| {
        p.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    };
    let names = || {
        executables
            .iter()
            .map(stem)
            .collect::<Vec<_>>()
            .join(", ")
    };

    if let Some(requested) = requested {
        return executables
            .iter()
            .find(|p| stem(p) == requested)
            .cloned()
            .ok_or_else(|| {
                miette!(
                    "{package} does not install an executable named '{requested}'. \
                     Available: {}",
                    names()
                )
            });
    }

    match executables {
        [] => Err(miette!(
            "{package} does not install any executables, so it cannot be run as an MCP server"
        )),
        [only] => Ok(only.clone()),
        _ => executables
            .iter()
            .find(|p| stem(p) == package)
            .cloned()
            .ok_or_else(|| {
                miette!(
                    "{package} installs several executables ({}); choose one with --command",
                    names()
                )
            }),
    }
}

/// Build kilo's local MCP server entry.
fn kilo_entry(package: &str, server: &Path, args: &[&str], env: &[(String, String)]) -> Value {
    let command: Vec<Value> = std::iter::once(server.to_string_lossy().to_string())
        .chain(args.iter().map(|a| a.to_string()))
        .map(Value::String)
        .collect();
    let mut entry = json!({
        "type": "local",
        "command": command,
        "enabled": true,
        MANAGED_MARKER: package,
    });
    if !env.is_empty() {
        entry["environment"] = env
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect::<serde_json::Map<_, _>>()
            .into();
    }
    entry
}

fn parse_env(pairs: &[String]) -> miette::Result<Vec<(String, String)>> {
    pairs
        .iter()
        .map(|pair| {
            pair.split_once('=')
                .filter(|(k, _)| !k.is_empty())
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .ok_or_else(|| miette!("invalid --env value '{pair}': expected KEY=VALUE"))
        })
        .collect()
}

/// `ana marketplace install` — solve, install, and register with kilo.
///
/// Catalog packages use their curated channel, executable, and arguments
/// unless overridden; any other conda package falls back to executable
/// discovery against the default channels.
async fn install(
    ctx: &mut CommandContext,
    spec: &str,
    requested_channels: &[String],
    command: Option<String>,
    name: Option<String>,
    env: &[String],
) -> miette::Result<()> {
    ctx.telemetry.add("marketplace_package", spec.to_string());

    let package = package_name(spec)?;
    let package = package.as_normalized();
    let entry = catalog_entry(package);
    let name = name.unwrap_or_else(|| package.to_string());
    let env = parse_env(env)?;
    let prefix = server_prefix(&name);
    let channels = match entry {
        Some(e) if requested_channels.is_empty() => vec![e.channel.to_string()],
        _ => channels(requested_channels),
    };
    let command = command.or_else(|| entry.map(|e| e.command.to_string()));
    let args = match entry {
        Some(e) if command.as_deref() == Some(e.command) => e.args,
        _ => &[],
    };

    let specs: Vec<&str> = std::iter::once(spec)
        .chain(entry.into_iter().flat_map(|e| e.constraints.iter().copied()))
        .collect();

    eprintln!("Installing {} into {}", name, prefix.display());
    install::solve_into_prefix(ctx, &prefix, &channels, &specs).await?;

    let records = install::installed_records(&prefix)?;
    let record = records
        .iter()
        .find(|r| r.repodata_record.package_record.name.as_normalized() == package)
        .ok_or_else(|| miette!("{package} was not installed into {}", prefix.display()))?;
    let version = record.repodata_record.package_record.version.to_string();
    let server = prefix.join(choose_server(
        package,
        &package_executables(record),
        command.as_deref(),
    )?);

    let (config_path, backup_path) = clients::set_entry(
        CLIENT,
        &name,
        &kilo_entry(package, &server, args, &env),
        true,
    )?;

    status::success(&format!(
        "Installed {} {} and registered MCP server {} in kilo: {}",
        status::highlight(package),
        version,
        status::highlight(&name),
        status::dim(&config_path.display().to_string())
    ));
    let shown = std::iter::once(server.display().to_string())
        .chain(args.iter().map(|a| a.to_string()))
        .collect::<Vec<_>>()
        .join(" ");
    eprintln!("  {}", status::dim(&format!("command: {shown}")));
    if let Some(backup) = backup_path {
        eprintln!(
            "  {}",
            status::dim(&format!("backup saved to {}", backup.display()))
        );
    }
    Ok(())
}

/// Installed MCP servers: (name, package, version, registered in kilo).
fn installed_servers() -> Vec<(String, String, String, bool)> {
    let Ok(entries) = std::fs::read_dir(marketplace_dir()) else {
        return Vec::new();
    };
    let mut servers: Vec<_> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().join("conda-meta").is_dir())
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let entry = clients::get_entry(CLIENT, &name);
            let package = entry
                .as_ref()
                .and_then(|v| v.get(MANAGED_MARKER))
                .and_then(Value::as_str)
                .unwrap_or(&name)
                .to_string();
            let version = install::installed_records(&e.path())
                .unwrap_or_default()
                .into_iter()
                .find(|r| r.repodata_record.package_record.name.as_normalized() == package)
                .map(|r| r.repodata_record.package_record.version.to_string())
                .unwrap_or_else(|| "?".to_string());
            (name, package, version, entry.is_some())
        })
        .collect();
    servers.sort();
    servers
}

/// `ana marketplace list` — the catalog with install status, plus any other
/// packages installed from the marketplace.
fn list(json: bool) -> miette::Result<()> {
    let installed = installed_servers();
    let installed_by_package = |package: &str| {
        installed
            .iter()
            .find(|(_, p, _, _)| p == package)
    };
    let others: Vec<_> = installed
        .iter()
        .filter(|(_, p, _, _)| catalog_entry(p).is_none())
        .collect();

    if json {
        let mut data: Vec<Value> = CATALOG
            .iter()
            .map(|e| {
                let found = installed_by_package(e.package);
                json!({
                    "package": e.package,
                    "description": e.description,
                    "channel": e.channel,
                    "installed": found.is_some(),
                    "name": found.map(|(n, _, _, _)| n),
                    "version": found.map(|(_, _, v, _)| v),
                    "registered": found.is_some_and(|(_, _, _, r)| *r),
                })
            })
            .collect();
        data.extend(others.iter().map(|(name, package, version, registered)| {
            json!({
                "package": package,
                "installed": true,
                "name": name,
                "version": version,
                "registered": registered,
            })
        }));
        println!("{}", serde_json::to_string_pretty(&data).into_diagnostic()?);
        return Ok(());
    }

    let mut table = crate::table::new(["Package", "Description", "Installed"]);
    let installed_cell = |found: Option<&(String, String, String, bool)>| match found {
        Some((_, _, version, true)) => crate::table::cell(format!("✓ {version}"))
            .fg(crate::table::Color::Green),
        Some((_, _, version, false)) => crate::table::cell(format!("{version} (not in kilo)")),
        None => crate::table::cell("—"),
    };
    for e in CATALOG {
        table.add_row([
            crate::table::cell(e.package),
            crate::table::cell(e.description),
            installed_cell(installed_by_package(e.package)),
        ]);
    }
    for found in &others {
        table.add_row([
            crate::table::cell(&found.1),
            crate::table::cell(""),
            installed_cell(Some(found)),
        ]);
    }
    println!("{table}");
    status::tip(&format!(
        "Install one with {}",
        status::highlight(&crate::prog::cmd("marketplace install <package>"))
    ));
    Ok(())
}

/// `ana marketplace remove` — unregister from kilo and delete the prefix.
fn remove(name: &str) -> miette::Result<()> {
    let prefix = server_prefix(name);
    let entry = clients::get_entry(CLIENT, name);
    let managed = entry.as_ref().is_some_and(|v| v.get(MANAGED_MARKER).is_some());

    if !prefix.exists() && !managed {
        return Err(miette!("'{name}' is not installed from the marketplace"));
    }

    if managed {
        for config in clients::remove_entry(CLIENT, name, true)? {
            status::success(&format!(
                "Removed MCP server {} from kilo: {}",
                status::highlight(name),
                status::dim(&config.config_path.display().to_string())
            ));
        }
    } else if entry.is_some() {
        status::warn(&format!(
            "kilo has an MCP server named '{name}' that ana did not create; leaving it in place"
        ));
    }

    if prefix.exists() {
        std::fs::remove_dir_all(&prefix)
            .into_diagnostic()
            .with_context(|| format!("failed to remove {}", prefix.display()))?;
        status::success(&format!(
            "Uninstalled {}: {}",
            status::highlight(name),
            status::dim(&prefix.display().to_string())
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn test_choose_server_single_executable() {
        let exes = paths(&["bin/fetch-server"]);
        assert_eq!(
            choose_server("mcp-server-fetch", &exes, None).unwrap(),
            PathBuf::from("bin/fetch-server")
        );
    }

    #[test]
    fn test_choose_server_prefers_package_name_among_several() {
        let exes = paths(&["bin/helper", "bin/mcp-server-fetch"]);
        assert_eq!(
            choose_server("mcp-server-fetch", &exes, None).unwrap(),
            PathBuf::from("bin/mcp-server-fetch")
        );
    }

    #[test]
    fn test_choose_server_ambiguous_requires_command() {
        let exes = paths(&["bin/a", "bin/b"]);
        let err = choose_server("pkg", &exes, None).unwrap_err().to_string();
        assert!(err.contains("--command"), "{err}");
    }

    #[test]
    fn test_choose_server_requested_must_exist() {
        let exes = paths(&["bin/a", "bin/b"]);
        assert_eq!(
            choose_server("pkg", &exes, Some("b")).unwrap(),
            PathBuf::from("bin/b")
        );
        assert!(choose_server("pkg", &exes, Some("c")).is_err());
    }

    #[test]
    fn test_choose_server_no_executables() {
        assert!(choose_server("mcp", &[], None).is_err());
    }

    #[test]
    fn test_package_name_strips_version_constraint() {
        assert_eq!(
            package_name("mcp-server-fetch>=2026").unwrap().as_normalized(),
            "mcp-server-fetch"
        );
        assert!(package_name("mcp-*").is_err());
    }

    #[test]
    fn test_parse_env_rejects_missing_equals() {
        assert_eq!(
            parse_env(&["A=1".to_string(), "B=x=y".to_string()]).unwrap(),
            vec![("A".into(), "1".into()), ("B".into(), "x=y".into())]
        );
        assert!(parse_env(&["NOPE".to_string()]).is_err());
        assert!(parse_env(&["=v".to_string()]).is_err());
    }
}
