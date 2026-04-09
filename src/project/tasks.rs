//! Task execution with dependency resolution.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::ExitStatus;

use super::manifest::Task;

/// Topological sort of tasks by depends-on. Returns task names in execution order.
fn execution_order(target: &str, tasks: &HashMap<String, Task>) -> Result<Vec<String>, String> {
    let mut order = Vec::new();
    let mut visited = HashSet::new();
    let mut in_stack = HashSet::new();

    fn visit(
        name: &str,
        tasks: &HashMap<String, Task>,
        visited: &mut HashSet<String>,
        in_stack: &mut HashSet<String>,
        order: &mut Vec<String>,
    ) -> Result<(), String> {
        if in_stack.contains(name) {
            return Err(format!(
                "Circular dependency detected involving task '{}'",
                name
            ));
        }
        if visited.contains(name) {
            return Ok(());
        }

        let task = tasks
            .get(name)
            .ok_or_else(|| format!("Unknown task: '{}'", name))?;

        in_stack.insert(name.to_string());

        for dep in &task.depends_on {
            visit(dep, tasks, visited, in_stack, order)?;
        }

        in_stack.remove(name);
        visited.insert(name.to_string());
        order.push(name.to_string());

        Ok(())
    }

    visit(target, tasks, &mut visited, &mut in_stack, &mut order)?;
    Ok(order)
}

/// Run a task (and its dependencies) within a project environment.
pub fn run(
    task_name: &str,
    tasks: &HashMap<String, Task>,
    env_prefix: &Path,
) -> Result<ExitStatus, String> {
    let order = execution_order(task_name, tasks)?;

    let mut last_status = None;

    for name in &order {
        let task = &tasks[name];
        eprintln!("▶ {}", name);

        let status = run_single_task(task, env_prefix)?;
        if !status.success() {
            return Ok(status);
        }
        last_status = Some(status);
    }

    last_status.ok_or_else(|| "No tasks to run".to_string())
}

/// Build the PATH string with the environment's bin directories prepended.
fn env_path(env_prefix: &Path) -> String {
    let path = std::env::var("PATH").unwrap_or_default();
    let sep = if cfg!(windows) { ";" } else { ":" };

    let mut dirs = vec![env_prefix.join("bin")];
    if cfg!(windows) {
        dirs.push(env_prefix.join("Scripts"));
        dirs.push(env_prefix.join("Library").join("bin"));
        dirs.push(env_prefix.to_path_buf());
    }

    let prefix: Vec<_> = dirs.iter().map(|d| d.display().to_string()).collect();
    format!("{}{}{}", prefix.join(sep), sep, path)
}

/// Return the shell binary and the flag used to pass a command string.
///
/// On Unix, honours $SHELL and falls back to /bin/sh.
/// On Windows, uses cmd.exe with /C.
fn shell_and_flag() -> (String, &'static str) {
    if cfg!(windows) {
        let shell = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
        (shell, "/C")
    } else {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        (shell, "-c")
    }
}

/// Run a command string in a shell with the project environment activated.
fn run_in_env(cmd: &str, env_prefix: &Path) -> Result<ExitStatus, String> {
    let new_path = env_path(env_prefix);
    let (shell, flag) = shell_and_flag();

    std::process::Command::new(&shell)
        .arg(flag)
        .arg(cmd)
        .env("PATH", &new_path)
        .env("CONDA_PREFIX", env_prefix)
        .status()
        .map_err(|e| format!("Failed to run command: {}", e))
}

/// Run an arbitrary command in a shell with the project environment activated.
pub fn run_command(cmd: &str, env_prefix: &Path) -> Result<ExitStatus, String> {
    run_in_env(cmd, env_prefix)
}

/// Execute a single task command in a shell with the environment activated.
fn run_single_task(task: &Task, env_prefix: &Path) -> Result<ExitStatus, String> {
    run_in_env(&task.cmd, env_prefix)
}

/// List available tasks.
pub fn list(tasks: &HashMap<String, Task>) {
    let mut names: Vec<_> = tasks.keys().collect();
    names.sort();

    for name in names {
        let task = &tasks[name];
        match &task.description {
            Some(desc) => eprintln!("  {:<20} {}", name, desc),
            None => eprintln!("  {}", name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tasks(specs: &[(&str, &str, &[&str])]) -> HashMap<String, Task> {
        specs
            .iter()
            .map(|(name, cmd, deps)| {
                (
                    name.to_string(),
                    Task {
                        name: name.to_string(),
                        cmd: cmd.to_string(),
                        depends_on: deps.iter().map(|s| s.to_string()).collect(),
                        description: None,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn test_execution_order_no_deps() {
        let tasks = make_tasks(&[("test", "cargo test", &[])]);
        let order = execution_order("test", &tasks).unwrap();
        assert_eq!(order, vec!["test"]);
    }

    #[test]
    fn test_execution_order_with_deps() {
        let tasks = make_tasks(&[
            ("build", "cargo build", &[]),
            ("test", "cargo test", &["build"]),
        ]);
        let order = execution_order("test", &tasks).unwrap();
        assert_eq!(order, vec!["build", "test"]);
    }

    #[test]
    fn test_execution_order_diamond() {
        let tasks = make_tasks(&[
            ("a", "echo a", &[]),
            ("b", "echo b", &["a"]),
            ("c", "echo c", &["a"]),
            ("d", "echo d", &["b", "c"]),
        ]);
        let order = execution_order("d", &tasks).unwrap();
        // a must come before b and c, d must be last
        assert_eq!(order[0], "a");
        assert_eq!(*order.last().unwrap(), "d");
        assert_eq!(order.len(), 4);
    }

    #[test]
    fn test_execution_order_circular() {
        let tasks = make_tasks(&[("a", "echo a", &["b"]), ("b", "echo b", &["a"])]);
        let result = execution_order("a", &tasks);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Circular dependency"));
    }

    #[test]
    fn test_execution_order_unknown_task() {
        let tasks = make_tasks(&[]);
        let result = execution_order("missing", &tasks);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown task"));
    }

    #[test]
    fn test_execution_order_unknown_dep() {
        let tasks = make_tasks(&[("a", "echo a", &["missing"])]);
        let result = execution_order("a", &tasks);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown task: 'missing'"));
    }

    #[test]
    fn test_run_single_task_echo() {
        let task = Task {
            name: "hello".to_string(),
            cmd: "echo hello".to_string(),
            depends_on: Vec::new(),
            description: None,
        };
        let tmp = tempfile::tempdir().unwrap();
        let status = run_single_task(&task, tmp.path()).unwrap();
        assert!(status.success());
    }

    #[test]
    fn test_run_single_task_failure() {
        let task = Task {
            name: "fail".to_string(),
            cmd: "false".to_string(),
            depends_on: Vec::new(),
            description: None,
        };
        let tmp = tempfile::tempdir().unwrap();
        let status = run_single_task(&task, tmp.path()).unwrap();
        assert!(!status.success());
    }

    #[test]
    fn test_execution_order_self_referencing_cycle() {
        let tasks = make_tasks(&[("a", "echo a", &["a"])]);
        let result = execution_order("a", &tasks);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Circular dependency"));
    }

    #[test]
    fn test_execution_order_deep_chain() {
        let tasks = make_tasks(&[
            ("e", "echo e", &["d"]),
            ("d", "echo d", &["c"]),
            ("c", "echo c", &["b"]),
            ("b", "echo b", &["a"]),
            ("a", "echo a", &[]),
        ]);
        let order = execution_order("e", &tasks).unwrap();
        assert_eq!(order, vec!["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn test_run_stops_on_dependency_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let marker = tmp.path().join("marker");

        // Task "a" fails, task "b" depends on "a" and would create a marker file
        let tasks = make_tasks(&[
            ("a", "false", &[]),
            ("b", &format!("touch {}", marker.display()), &["a"]),
        ]);

        let status = run("b", &tasks, tmp.path()).unwrap();
        assert!(!status.success());
        // "b" should never have run, so the marker file should not exist
        assert!(
            !marker.exists(),
            "Task 'b' should not have executed after 'a' failed"
        );
    }

    #[test]
    fn test_shell_and_flag_returns_valid_shell() {
        let (shell, flag) = shell_and_flag();
        // On all platforms we should get a non-empty shell path and flag
        assert!(!shell.is_empty(), "shell should not be empty");
        assert!(!flag.is_empty(), "flag should not be empty");

        if cfg!(windows) {
            // Windows: flag should be /C
            assert_eq!(flag, "/C");
        } else {
            // Unix: flag should be -c
            assert_eq!(flag, "-c");
        }
    }

    #[test]
    fn test_env_path_contains_env_bin() {
        let tmp = tempfile::tempdir().unwrap();
        let path = env_path(tmp.path());
        let bin_dir = tmp.path().join("bin").display().to_string();
        assert!(
            path.contains(&bin_dir),
            "PATH should contain env bin directory"
        );
    }

    #[test]
    fn test_env_path_uses_correct_separator() {
        let tmp = tempfile::tempdir().unwrap();
        let path = env_path(tmp.path());
        let sep = if cfg!(windows) { ";" } else { ":" };
        assert!(
            path.contains(sep),
            "PATH should use platform-appropriate separator"
        );
    }

    #[cfg(windows)]
    #[test]
    fn test_env_path_includes_windows_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let path = env_path(tmp.path());
        let scripts = tmp.path().join("Scripts").display().to_string();
        let lib_bin = tmp.path().join("Library").join("bin").display().to_string();
        assert!(
            path.contains(&scripts),
            "PATH should contain Scripts dir on Windows"
        );
        assert!(
            path.contains(&lib_bin),
            "PATH should contain Library\\bin on Windows"
        );
    }

    #[test]
    fn test_run_single_task_prepends_env_bin_to_path() {
        let tmp = tempfile::tempdir().unwrap();
        let bin_dir = tmp.path().join("bin");
        std::fs::create_dir(&bin_dir).unwrap();

        // Create a platform-appropriate script in the env's bin/ directory
        #[cfg(windows)]
        {
            let script = bin_dir.join("my-test-tool.cmd");
            std::fs::write(&script, "@echo off\r\necho tool-ran\r\n").unwrap();
        }
        #[cfg(not(windows))]
        {
            let script = bin_dir.join("my-test-tool");
            std::fs::write(&script, "#!/bin/sh\necho tool-ran").unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        // Run a task that invokes the tool by name (not full path)
        let task = Task {
            name: "use-tool".to_string(),
            cmd: "my-test-tool".to_string(),
            depends_on: Vec::new(),
            description: None,
        };
        let status = run_single_task(&task, tmp.path()).unwrap();
        assert!(
            status.success(),
            "Tool in env bin/ should be found via PATH"
        );
    }
}
