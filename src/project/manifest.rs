//! Parse pixi.toml/ana.toml for project metadata, dependencies, and tasks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Manifest filenames in preference order.
const MANIFEST_FILENAMES: &[&str] = &["ana.toml", "pixi.toml"];

/// Lockfile filenames in preference order.
const LOCKFILE_FILENAMES: &[&str] = &["ana.lock", "pixi.lock"];

/// A parsed task definition from the manifest.
#[derive(Debug, Clone)]
pub struct Task {
    pub name: String,
    pub cmd: String,
    pub depends_on: Vec<String>,
    pub description: Option<String>,
}

/// A dependency specification from the manifest.
#[derive(Debug, Clone)]
pub struct Dependency {
    /// The package name.
    pub name: String,
    /// The version constraint as a raw string (MatchSpec format).
    pub version_spec: Option<String>,
}

/// Parsed project manifest.
#[derive(Debug)]
pub struct Manifest {
    pub path: PathBuf,
    pub lockfile_path: Option<PathBuf>,
    /// Project name from [workspace] or [project].
    pub name: Option<String>,
    /// Channel URLs/names from [workspace] channels.
    pub channels: Vec<String>,
    /// Target platforms from [workspace] platforms.
    pub platforms: Vec<String>,
    /// Dependencies from [dependencies].
    pub dependencies: Vec<Dependency>,
    /// Whether the manifest contains a [pypi-dependencies] section.
    pub has_pypi_dependencies: bool,
    /// Task definitions from [tasks.*].
    pub tasks: HashMap<String, Task>,
}

/// Find the manifest file in the given directory.
pub fn find_manifest(dir: &Path) -> Option<PathBuf> {
    for filename in MANIFEST_FILENAMES {
        let path = dir.join(filename);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// Find the lockfile in the given directory.
pub fn find_lockfile(dir: &Path) -> Option<PathBuf> {
    for filename in LOCKFILE_FILENAMES {
        let path = dir.join(filename);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// Parse a manifest file.
pub fn parse(manifest_path: &Path) -> Result<Manifest, String> {
    let content = std::fs::read_to_string(manifest_path)
        .map_err(|e| format!("Failed to read {}: {}", manifest_path.display(), e))?;

    let doc: toml::Value =
        toml::from_str(&content).map_err(|e| format!("Failed to parse TOML: {}", e))?;

    // Parse [workspace] or [project] section
    let workspace = doc.get("workspace").or_else(|| doc.get("project"));

    let name = workspace
        .and_then(|w| w.get("name"))
        .and_then(|v| v.as_str())
        .map(String::from);

    let channels = workspace
        .and_then(|w| w.get("channels"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let platforms = workspace
        .and_then(|w| w.get("platforms"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    // Parse [dependencies]
    let dependencies = parse_dependencies(&doc)?;

    // Check for [pypi-dependencies]
    let has_pypi_dependencies = doc
        .get("pypi-dependencies")
        .and_then(|v| v.as_table())
        .is_some_and(|t| !t.is_empty());

    // Parse [tasks.*]
    let mut tasks = HashMap::new();
    if let Some(tasks_table) = doc.get("tasks").and_then(|v| v.as_table()) {
        for (name, value) in tasks_table {
            let task = parse_task(name, value)?;
            tasks.insert(name.clone(), task);
        }
    }

    let dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let lockfile_path = find_lockfile(dir);

    Ok(Manifest {
        path: manifest_path.to_path_buf(),
        lockfile_path,
        name,
        channels,
        platforms,
        dependencies,
        has_pypi_dependencies,
        tasks,
    })
}

fn parse_dependencies(doc: &toml::Value) -> Result<Vec<Dependency>, String> {
    let mut deps = Vec::new();

    let Some(deps_table) = doc.get("dependencies").and_then(|v| v.as_table()) else {
        return Ok(deps);
    };

    for (name, value) in deps_table {
        let version_spec = match value {
            // Simple form: package = ">=1.0"
            toml::Value::String(s) => {
                if s == "*" {
                    None
                } else {
                    Some(s.clone())
                }
            }
            // Table form: package = { version = ">=1.0", ... }
            toml::Value::Table(t) => t.get("version").and_then(|v| v.as_str()).map(String::from),
            _ => None,
        };

        deps.push(Dependency {
            name: name.clone(),
            version_spec,
        });
    }

    Ok(deps)
}

fn parse_task(name: &str, value: &toml::Value) -> Result<Task, String> {
    match value {
        // Shorthand: task-name = "command string"
        toml::Value::String(cmd) => Ok(Task {
            name: name.to_string(),
            cmd: cmd.clone(),
            depends_on: Vec::new(),
            description: None,
        }),
        // Full form: [tasks.name] with cmd, depends-on, description
        toml::Value::Table(table) => {
            let cmd = table
                .get("cmd")
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("Task '{}' missing 'cmd' field", name))?
                .to_string();

            let depends_on = table
                .get("depends-on")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            let description = table
                .get("description")
                .and_then(|v| v.as_str())
                .map(String::from);

            Ok(Task {
                name: name.to_string(),
                cmd,
                depends_on,
                description,
            })
        }
        _ => Err(format!(
            "Task '{}': expected string or table, got {:?}",
            name, value
        )),
    }
}

/// Build a MatchSpec string from a dependency name and optional version constraint.
pub fn dependency_to_match_spec(dep: &Dependency) -> String {
    match &dep.version_spec {
        Some(spec) => format!("{} {}", dep.name, spec),
        None => dep.name.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_find_manifest_prefers_ana_toml() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("ana.toml"), "[workspace]").unwrap();
        fs::write(tmp.path().join("pixi.toml"), "[workspace]").unwrap();

        let found = find_manifest(tmp.path()).unwrap();
        assert_eq!(found.file_name().unwrap(), "ana.toml");
    }

    #[test]
    fn test_find_manifest_falls_back_to_pixi() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("pixi.toml"), "[workspace]").unwrap();

        let found = find_manifest(tmp.path()).unwrap();
        assert_eq!(found.file_name().unwrap(), "pixi.toml");
    }

    #[test]
    fn test_find_manifest_none() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(find_manifest(tmp.path()).is_none());
    }

    #[test]
    fn test_find_lockfile_prefers_ana_lock() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("ana.lock"), "").unwrap();
        fs::write(tmp.path().join("pixi.lock"), "").unwrap();

        let found = find_lockfile(tmp.path()).unwrap();
        assert_eq!(found.file_name().unwrap(), "ana.lock");
    }

    #[test]
    fn test_find_lockfile_falls_back_to_pixi() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("pixi.lock"), "").unwrap();

        let found = find_lockfile(tmp.path()).unwrap();
        assert_eq!(found.file_name().unwrap(), "pixi.lock");
    }

    #[test]
    fn test_parse_shorthand_task() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("ana.toml");
        fs::write(
            &manifest,
            r#"
[workspace]
name = "test"

[tasks]
hello = "echo hello"
"#,
        )
        .unwrap();

        let m = parse(&manifest).unwrap();
        assert_eq!(m.tasks.len(), 1);
        let task = &m.tasks["hello"];
        assert_eq!(task.cmd, "echo hello");
        assert!(task.depends_on.is_empty());
    }

    #[test]
    fn test_parse_full_task() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("ana.toml");
        fs::write(
            &manifest,
            r#"
[workspace]
name = "test"

[tasks.build]
cmd = "cargo build"
description = "Build the project"

[tasks.test]
cmd = "cargo test"
depends-on = ["build"]
description = "Run tests"
"#,
        )
        .unwrap();

        let m = parse(&manifest).unwrap();
        assert_eq!(m.tasks.len(), 2);

        let build = &m.tasks["build"];
        assert_eq!(build.cmd, "cargo build");
        assert!(build.depends_on.is_empty());

        let test = &m.tasks["test"];
        assert_eq!(test.cmd, "cargo test");
        assert_eq!(test.depends_on, vec!["build"]);
    }

    #[test]
    fn test_parse_task_missing_cmd() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("ana.toml");
        fs::write(
            &manifest,
            r#"
[tasks.bad]
description = "no cmd"
"#,
        )
        .unwrap();

        let result = parse(&manifest);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("missing 'cmd'"));
    }

    #[test]
    fn test_find_lockfile_none() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(find_lockfile(tmp.path()).is_none());
    }

    #[test]
    fn test_parse_populates_lockfile_path() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("ana.toml");
        fs::write(&manifest, "[tasks]").unwrap();
        fs::write(tmp.path().join("ana.lock"), "").unwrap();

        let m = parse(&manifest).unwrap();
        assert!(m.lockfile_path.is_some());
        assert_eq!(m.lockfile_path.unwrap().file_name().unwrap(), "ana.lock");
    }

    #[test]
    fn test_parse_no_lockfile() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("ana.toml");
        fs::write(&manifest, "[tasks]").unwrap();

        let m = parse(&manifest).unwrap();
        assert!(m.lockfile_path.is_none());
    }

    #[test]
    fn test_parse_invalid_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("ana.toml");
        fs::write(&manifest, "this is not valid toml {{{{").unwrap();

        let result = parse(&manifest);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to parse TOML"));
    }

    #[test]
    fn test_parse_nonexistent_file() {
        let result = parse(Path::new("/nonexistent/path/ana.toml"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to read"));
    }

    #[test]
    fn test_parse_empty_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("ana.toml");
        fs::write(&manifest, "# empty manifest\n").unwrap();

        let m = parse(&manifest).unwrap();
        assert!(m.tasks.is_empty());
    }

    #[test]
    fn test_parse_task_invalid_type() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("ana.toml");
        fs::write(
            &manifest,
            r#"
[tasks]
bad = 42
"#,
        )
        .unwrap();

        let result = parse(&manifest);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expected string or table"));
    }

    #[test]
    fn test_parse_workspace_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("ana.toml");
        fs::write(
            &manifest,
            r#"
[workspace]
name = "my-project"
channels = ["conda-forge"]
platforms = ["osx-arm64", "linux-64"]
"#,
        )
        .unwrap();

        let m = parse(&manifest).unwrap();
        assert_eq!(m.name, Some("my-project".to_string()));
        assert_eq!(m.channels, vec!["conda-forge"]);
        assert_eq!(m.platforms, vec!["osx-arm64", "linux-64"]);
    }

    #[test]
    fn test_parse_dependencies_string() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("ana.toml");
        fs::write(
            &manifest,
            r#"
[dependencies]
python = ">=3.10"
rust = "*"
numpy = ">=1.24,<2"
"#,
        )
        .unwrap();

        let m = parse(&manifest).unwrap();
        assert_eq!(m.dependencies.len(), 3);

        let python = m.dependencies.iter().find(|d| d.name == "python").unwrap();
        assert_eq!(python.version_spec, Some(">=3.10".to_string()));

        let rust = m.dependencies.iter().find(|d| d.name == "rust").unwrap();
        assert_eq!(rust.version_spec, None); // "*" means no constraint

        let numpy = m.dependencies.iter().find(|d| d.name == "numpy").unwrap();
        assert_eq!(numpy.version_spec, Some(">=1.24,<2".to_string()));
    }

    #[test]
    fn test_parse_dependencies_table() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("ana.toml");
        fs::write(
            &manifest,
            r#"
[dependencies]
python = { version = ">=3.10" }
"#,
        )
        .unwrap();

        let m = parse(&manifest).unwrap();
        assert_eq!(m.dependencies.len(), 1);
        let python = &m.dependencies[0];
        assert_eq!(python.name, "python");
        assert_eq!(python.version_spec, Some(">=3.10".to_string()));
    }

    #[test]
    fn test_parse_project_section_fallback() {
        // pixi.toml uses [project] instead of [workspace]
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("pixi.toml");
        fs::write(
            &manifest,
            r#"
[project]
name = "pixi-project"
channels = ["conda-forge"]
platforms = ["linux-64"]
"#,
        )
        .unwrap();

        let m = parse(&manifest).unwrap();
        assert_eq!(m.name, Some("pixi-project".to_string()));
        assert_eq!(m.channels, vec!["conda-forge"]);
        assert_eq!(m.platforms, vec!["linux-64"]);
    }

    #[test]
    fn test_parse_workspace_preferred_over_project() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("ana.toml");
        fs::write(
            &manifest,
            r#"
[workspace]
name = "from-workspace"

[project]
name = "from-project"
"#,
        )
        .unwrap();

        let m = parse(&manifest).unwrap();
        assert_eq!(m.name, Some("from-workspace".to_string()));
    }

    #[test]
    fn test_parse_no_workspace_or_project() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("ana.toml");
        fs::write(&manifest, "[dependencies]\npython = \"*\"\n").unwrap();

        let m = parse(&manifest).unwrap();
        assert_eq!(m.name, None);
        assert!(m.channels.is_empty());
        assert!(m.platforms.is_empty());
    }

    #[test]
    fn test_parse_empty_dependencies_table() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("ana.toml");
        fs::write(&manifest, "[dependencies]\n").unwrap();

        let m = parse(&manifest).unwrap();
        assert!(m.dependencies.is_empty());
    }

    #[test]
    fn test_parse_no_dependencies_section() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("ana.toml");
        fs::write(&manifest, "[workspace]\nname = \"test\"\n").unwrap();

        let m = parse(&manifest).unwrap();
        assert!(m.dependencies.is_empty());
    }

    #[test]
    fn test_dependency_to_match_spec() {
        let dep = Dependency {
            name: "python".to_string(),
            version_spec: Some(">=3.10".to_string()),
        };
        assert_eq!(dependency_to_match_spec(&dep), "python >=3.10");

        let dep_no_ver = Dependency {
            name: "rust".to_string(),
            version_spec: None,
        };
        assert_eq!(dependency_to_match_spec(&dep_no_ver), "rust");
    }

    #[test]
    fn test_parse_detects_pypi_dependencies() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("pixi.toml");
        fs::write(
            &manifest,
            r#"
[workspace]
name = "test"

[dependencies]
python = ">=3.10"

[pypi-dependencies]
requests = "*"
"#,
        )
        .unwrap();

        let m = parse(&manifest).unwrap();
        assert!(m.has_pypi_dependencies);
    }

    #[test]
    fn test_parse_no_pypi_dependencies() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("ana.toml");
        fs::write(
            &manifest,
            r#"
[workspace]
name = "test"

[dependencies]
python = ">=3.10"
"#,
        )
        .unwrap();

        let m = parse(&manifest).unwrap();
        assert!(!m.has_pypi_dependencies);
    }

    #[test]
    fn test_parse_empty_pypi_dependencies() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("ana.toml");
        fs::write(
            &manifest,
            r#"
[workspace]
name = "test"

[pypi-dependencies]
"#,
        )
        .unwrap();

        let m = parse(&manifest).unwrap();
        assert!(!m.has_pypi_dependencies);
    }
}
