//! Standalone conda wrapper binary for ana.
//!
//! This binary is compiled separately and embedded into ana, then written to
//! ~/.ana/bin/conda when `ana tool install conda` is run.
//!
//! The wrapper:
//! - Intercepts `activate`, `deactivate`, and `init` commands with helpful messaging
//! - Aliases `shell` -> `spawn`
//! - Filters `create` output to show conda-spawn instructions
//! - Shows feedback hint on errors
//! - Passes through all other commands to the real conda binary

use std::env;
use std::io::{BufRead, BufReader, IsTerminal, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio, exit};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let code = run(&args);
    exit(code);
}

fn run(args: &[String]) -> i32 {
    let first_arg = args.first().map(|s| s.as_str());

    // Handle disabled commands
    match first_arg {
        Some("activate") | Some("deactivate") => {
            print_disabled_shell_command(first_arg.unwrap());
            return 1;
        }
        Some("init") => {
            print_disabled_init();
            return 1;
        }
        _ => {}
    }

    // Handle "shell" as an alias for "spawn"
    if first_arg == Some("shell") {
        let mut new_args = args.to_vec();
        new_args[0] = "spawn".to_string();
        return hand_off_to_conda(&new_args);
    }

    // Check if this is a create command that needs output filtering
    if should_filter_create_output(args) {
        return run_conda_filtered(args);
    }

    // Pass through to conda
    hand_off_to_conda(args)
}

// === Path utilities (self-contained, no external dependencies) ===
// TODO: Consider extracting to a shared crate to avoid duplication with ana's paths module

fn home_dir() -> PathBuf {
    #[cfg(unix)]
    {
        env::var("HOME")
            .map(PathBuf::from)
            .expect("HOME environment variable not set")
    }
    #[cfg(windows)]
    {
        env::var("USERPROFILE")
            .map(PathBuf::from)
            .expect("USERPROFILE environment variable not set")
    }
}

fn ana_home() -> PathBuf {
    env::var("ANA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir().join(".ana"))
}

fn bin_dir() -> PathBuf {
    ana_home().join("bin")
}

fn conda_prefix() -> PathBuf {
    ana_home().join("tools").join("conda")
}

fn get_conda_bin() -> PathBuf {
    #[cfg(unix)]
    let bin = conda_prefix().join("bin").join("conda");
    #[cfg(windows)]
    let bin = conda_prefix().join("Scripts").join("conda.exe");
    bin
}

// === Styled output (minimal implementation without console crate) ===

fn yellow_bold(s: &str) -> String {
    format!("\x1b[1;33m{}\x1b[0m", s)
}

fn green(s: &str) -> String {
    format!("\x1b[32m{}\x1b[0m", s)
}

fn cyan(s: &str) -> String {
    format!("\x1b[36m{}\x1b[0m", s)
}

fn blue(s: &str) -> String {
    format!("\x1b[34m{}\x1b[0m", s)
}

// === Message functions ===
// TODO: Update "Learn more" URLs to point to ana documentation once available

fn print_disabled_shell_command(command: &str) {
    eprintln!(
        "{} `conda {command}` is not available via ana.",
        yellow_bold("!")
    );
    eprintln!();
    eprintln!("  ana uses conda-spawn for environment activation.");
    eprintln!("  Instead of `conda activate myenv`, run:");
    eprintln!();
    eprintln!("    {}", green("conda shell myenv"));
    eprintln!();
    eprintln!("  To leave the environment, exit the subshell (Ctrl+D or `exit`).");
    eprintln!();
    eprintln!("  Learn more: https://github.com/conda-incubator/conda-spawn");
}

fn print_disabled_init() {
    eprintln!("{} `conda init` is not needed with ana.", yellow_bold("!"));
    eprintln!();
    eprintln!("  ana provides conda without requiring shell initialization.");
    eprintln!(
        "  Just add {} to your PATH and you're ready to go.",
        cyan("~/.ana/bin")
    );
    eprintln!();
    eprintln!("  To activate environments, use:");
    eprintln!();
    eprintln!("    {}", green("conda shell myenv"));
    eprintln!();
    eprintln!("  Learn more: https://github.com/conda-incubator/conda-spawn");
}

fn print_error_feedback_hint() {
    eprintln!();
    eprintln!(
        "If this error is related to ana's conda integration, please report it with {}.",
        blue("ana self feedback")
    );
}

fn print_activation_hint(env_name: &Option<String>) {
    let name = env_name.as_deref().unwrap_or("<env-name>");
    println!("#");
    println!("# To activate this environment, use");
    println!("#     $ conda shell {}", name);
    println!("#");
    println!("# To leave the environment, exit the subshell (Ctrl+D or `exit`).");
    println!("#");
}

// === Command handling ===

fn should_filter_create_output(args: &[String]) -> bool {
    let is_create = args.first().map(|s| s == "create").unwrap_or(false);
    let is_env_create = args.len() >= 2 && args[0] == "env" && args[1] == "create";

    if !is_create && !is_env_create {
        return false;
    }

    let has_yes_flag = args.iter().any(|a| a == "-y" || a == "--yes");
    let stdin_not_tty = !std::io::stdin().is_terminal();

    has_yes_flag || stdin_not_tty
}

fn extract_env_name(args: &[String]) -> Option<String> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "-n" || arg == "--name" {
            return iter.next().cloned();
        }
        if let Some(name) = arg.strip_prefix("-n=") {
            return Some(name.to_string());
        }
        if let Some(name) = arg.strip_prefix("--name=") {
            return Some(name.to_string());
        }
    }
    None
}

fn run_conda_filtered(args: &[String]) -> i32 {
    let conda_bin = get_conda_bin();
    let prefix = conda_prefix();

    let mut cmd = Command::new(&conda_bin);
    cmd.args(args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::inherit());
    cmd.env("CONDA_ROOT_PREFIX", &prefix);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to run conda: {}", e);
            return 1;
        }
    };

    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        let mut stdout_handle = std::io::stdout().lock();
        let mut in_activation_hint = false;
        let env_name = extract_env_name(args);

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };

            if line.contains("To activate this environment") {
                in_activation_hint = true;
                continue;
            }

            if in_activation_hint {
                if line.contains("conda activate") || line.contains("conda deactivate") {
                    continue;
                }
                if line.trim().is_empty() || !line.starts_with('#') {
                    in_activation_hint = false;
                    print_activation_hint(&env_name);
                }
            }

            if !in_activation_hint {
                let _ = writeln!(stdout_handle, "{}", line);
            }
        }
    }

    match child.wait() {
        Ok(status) => {
            let code = status.code().unwrap_or(1);
            if code != 0 {
                print_error_feedback_hint();
            }
            code
        }
        Err(e) => {
            eprintln!("Failed to wait for conda: {}", e);
            print_error_feedback_hint();
            1
        }
    }
}

fn hand_off_to_conda(args: &[String]) -> i32 {
    let conda_bin = get_conda_bin();

    if !conda_bin.exists() {
        eprintln!("Conda is not installed. Run `ana tool install conda` first.");
        return 1;
    }

    let prefix = conda_prefix();
    let bin_dir = bin_dir();

    let mut cmd = Command::new(&conda_bin);
    cmd.args(args);
    cmd.env("CONDA_ROOT_PREFIX", &prefix);

    // Add ana's bin directory to PATH so our wrapper takes precedence for subcommands
    let path = env::var("PATH").unwrap_or_default();
    #[cfg(unix)]
    let new_path = format!("{}:{}", bin_dir.display(), path);
    #[cfg(windows)]
    let new_path = format!("{};{}", bin_dir.display(), path);
    cmd.env("PATH", new_path);

    match cmd.status() {
        Ok(status) => {
            let code = status.code().unwrap_or(1);
            if code != 0 {
                print_error_feedback_hint();
            }
            code
        }
        Err(e) => {
            eprintln!("Failed to run conda: {}", e);
            print_error_feedback_hint();
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_filter_create_output_with_yes_flag() {
        let args: Vec<String> = vec!["create", "-n", "myenv", "-y"]
            .into_iter()
            .map(String::from)
            .collect();
        assert!(should_filter_create_output(&args));
    }

    #[test]
    fn test_should_not_filter_other_commands() {
        let args: Vec<String> = vec!["install", "numpy", "-y"]
            .into_iter()
            .map(String::from)
            .collect();
        assert!(!should_filter_create_output(&args));
    }

    #[test]
    fn test_extract_env_name_short_flag() {
        let args: Vec<String> = vec!["create", "-n", "myenv", "python"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(extract_env_name(&args), Some("myenv".to_string()));
    }

    #[test]
    fn test_extract_env_name_long_flag() {
        let args: Vec<String> = vec!["create", "--name", "myenv", "python"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(extract_env_name(&args), Some("myenv".to_string()));
    }

    #[test]
    fn test_extract_env_name_not_present() {
        let args: Vec<String> = vec!["create", "-p", "/path/to/env", "python"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(extract_env_name(&args), None);
    }
}
