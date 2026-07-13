//! Standalone conda wrapper binary for ana.
//!
//! This binary is compiled separately and embedded into ana, then written to
//! ~/.ana/bin/conda when `ana tool install conda` is run.
//!
//! The wrapper passes through all commands to the real conda binary and shows
//! a feedback hint on errors directing users to report issues via ana.

use std::env;
use std::path::PathBuf;
use std::process::{Command, exit};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let code = run(&args);
    exit(code);
}

fn run(args: &[String]) -> i32 {
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

// === Styled output ===

fn blue(s: &str) -> String {
    format!("\x1b[34m{}\x1b[0m", s)
}

fn print_error_feedback_hint() {
    eprintln!();
    eprintln!(
        "If this error is related to ana's conda integration, please report it with {}.",
        blue("ana self feedback")
    );
}

// === Command handling ===

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

    // Ensure ana's bin directory is in PATH
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
