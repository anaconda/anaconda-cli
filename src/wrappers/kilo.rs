//! Standalone kilo wrapper binary for ana.
//!
//! This binary is compiled separately and embedded into ana, then written to
//! ~/.ana/bin/kilo when `ana tool install kilo-cli` is run.
//!
//! The wrapper makes `kilo` the entrypoint for both CLIs:
//! - `kilo ana <args>` runs ana, which prints its commands as `kilo ana ...`
//! - `kilo upgrade` / `kilo uninstall` are redirected to ana's tool commands,
//!   since kilo-cli is managed by ana rather than kilo's own installer
//! - `kilo --help` adds an `ana` entry to kilo's command list
//! - everything else is handed to the real kilo in ~/.ana/tools/kilo-cli,
//!   after offering to update kilo when a newer version is published

use std::env;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio, exit};
use std::time::{Duration, SystemTime};

/// How long to wait for `ana tool check` before running kilo anyway.
const CHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// Program name ana prints in usage lines and command hints.
const ANA_PROG_NAME: &str = "kilo ana";

/// The `ana` row inserted into kilo's help command list.
const ANA_HELP_ROW: &str = "kilo ana                 Anaconda toolchain and account commands";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    exit(run(&args));
}

/// What to do with the wrapper's arguments.
#[derive(Debug, PartialEq)]
enum Route<'a> {
    /// Run ana with these arguments.
    Ana(&'a [String]),
    /// Refuse a kilo self-management command and point at ana's equivalent.
    Managed { command: &'a str, replacement: &'static str },
    /// Run kilo's top-level help and add the `ana` entry.
    Help,
    /// Hand the arguments to kilo unchanged.
    Kilo,
}

fn route(args: &[String]) -> Route<'_> {
    match args.first().map(String::as_str) {
        Some("ana") => Route::Ana(&args[1..]),
        Some(command @ "upgrade") => Route::Managed {
            command,
            replacement: "kilo ana tool update",
        },
        Some(command @ "uninstall") => Route::Managed {
            command,
            replacement: "kilo ana tool uninstall kilo-cli",
        },
        Some("-h" | "--help") if args.len() == 1 => Route::Help,
        Some("help") if args.len() == 1 => Route::Help,
        _ => Route::Kilo,
    }
}

fn run(args: &[String]) -> i32 {
    match route(args) {
        Route::Ana(rest) => run_ana(rest),
        Route::Managed {
            command,
            replacement,
        } => {
            eprintln!("`kilo {command}` is not available: kilo is managed by ana.");
            eprintln!("Use `{replacement}` instead.");
            1
        }
        Route::Help => run_kilo_help(args),
        Route::Kilo => {
            offer_update();
            run_kilo(args)
        }
    }
}

// === Path utilities (self-contained, no external dependencies) ===

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

fn kilo_prefix() -> PathBuf {
    ana_home().join("tools").join("kilo-cli")
}

/// The kilo launcher shipped in the kilo-cli package. It sets kilo's runtime
/// environment before running the native binary in `lib/kilo/`.
fn get_kilo_bin() -> PathBuf {
    #[cfg(unix)]
    let bin = kilo_prefix().join("bin").join("kilo");
    #[cfg(windows)]
    let bin = kilo_prefix().join("Scripts").join("kilo.bat");
    bin
}

/// The ana binary that installed this wrapper, recorded at install time.
/// Falls back to `ana` on PATH.
fn get_ana_bin() -> PathBuf {
    std::fs::read_to_string(kilo_prefix().join(".ana-exe"))
        .ok()
        .map(|s| PathBuf::from(s.trim()))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("ana"))
}

/// PATH with ana's bin directory and the kilo prefix's binary directories
/// first, so kilo finds the `rg` installed alongside it.
fn tool_path() -> std::ffi::OsString {
    let prefix = kilo_prefix();
    let mut dirs = vec![bin_dir(), prefix.join("bin")];
    if cfg!(windows) {
        dirs.push(prefix.join("Library").join("bin"));
    }
    if let Some(path) = env::var_os("PATH") {
        dirs.extend(env::split_paths(&path));
    }
    env::join_paths(dirs).unwrap_or_default()
}

// === Command handling ===

fn kilo_command(args: &[String]) -> Result<Command, i32> {
    let kilo_bin = get_kilo_bin();
    if !kilo_bin.exists() {
        eprintln!("kilo is not installed. Run `ana tool install kilo-cli` first.");
        return Err(1);
    }
    let mut cmd = Command::new(&kilo_bin);
    cmd.args(args).env("PATH", tool_path());
    Ok(cmd)
}

fn run_kilo(args: &[String]) -> i32 {
    match kilo_command(args) {
        Ok(cmd) => hand_off(cmd, "kilo"),
        Err(code) => code,
    }
}

fn run_ana(args: &[String]) -> i32 {
    let mut cmd = Command::new(get_ana_bin());
    cmd.args(args).env("ANA_PROG_NAME", ANA_PROG_NAME);
    hand_off(cmd, "ana")
}

// === Update check ===

/// Whether update checks are enabled (`ANA_UPDATE_CHECK`, default on).
fn update_check_enabled() -> bool {
    !matches!(
        env::var("ANA_UPDATE_CHECK")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "0" | "false" | "no" | "off"
    )
}

/// Minimum time between checks (`ANA_UPDATE_CHECK_INTERVAL_HOURS`, default 24).
fn update_check_interval() -> Duration {
    let hours = env::var("ANA_UPDATE_CHECK_INTERVAL_HOURS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(24);
    Duration::from_secs(hours * 3600)
}

fn last_check_file() -> PathBuf {
    kilo_prefix().join(".last-update-check")
}

/// True when the last check is older than the interval (or never happened).
fn check_due() -> bool {
    let Ok(modified) = std::fs::metadata(last_check_file()).and_then(|m| m.modified()) else {
        return true;
    };
    SystemTime::now()
        .duration_since(modified)
        .map_or(true, |age| age >= update_check_interval())
}

/// Ask ana for a newer kilo version. Returns it, or `None` when up to date
/// or when the check fails or takes longer than `CHECK_TIMEOUT`.
fn newer_version() -> Option<String> {
    let mut child = Command::new(get_ana_bin())
        .args(["tool", "check", "kilo-cli"])
        .env("ANA_PROG_NAME", ANA_PROG_NAME)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = SystemTime::now() + CHECK_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) | Err(_) => return None,
            Ok(None) if SystemTime::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    let output = child.wait_with_output().ok()?;
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!version.is_empty()).then_some(version)
}

/// Before running kilo, offer to update it when a newer version is published.
///
/// Only runs interactively, at most once per check interval, and never blocks
/// kilo: any failure, timeout, or a "no" answer just continues.
fn offer_update() {
    if !update_check_enabled()
        || !std::io::stdin().is_terminal()
        || !std::io::stderr().is_terminal()
        || !get_kilo_bin().exists()
        || !check_due()
    {
        return;
    }
    let version = newer_version();
    // Record the check even when it failed, so an outage doesn't add a
    // delay to every kilo start.
    let _ = std::fs::write(last_check_file(), b"");
    let Some(version) = version else {
        return;
    };

    eprint!("A new version of kilo is available: {version}. Update now? [Y/n] ");
    let _ = std::io::stderr().flush();
    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() || !is_yes(&answer) {
        return;
    }

    let updated = Command::new(get_ana_bin())
        .args(["tool", "update", "kilo-cli"])
        .env("ANA_PROG_NAME", ANA_PROG_NAME)
        // Keep the output about kilo: skip ana's own "new version" notice.
        .env("ANA_UPDATE_CHECK", "0")
        .status()
        .is_ok_and(|s| s.success());
    if !updated {
        eprintln!("kilo update failed; continuing with the installed version.");
    }
}

/// Default-yes answer to a [Y/n] prompt.
fn is_yes(answer: &str) -> bool {
    matches!(answer.trim().to_ascii_lowercase().as_str(), "" | "y" | "yes")
}

/// kilo prints its top-level help to stderr. Capture it and add the `ana`
/// command so `kilo --help` lists both CLIs.
fn run_kilo_help(args: &[String]) -> i32 {
    let mut cmd = match kilo_command(args) {
        Ok(cmd) => cmd,
        Err(code) => return code,
    };
    let output = match cmd.stdin(Stdio::null()).output() {
        Ok(output) => output,
        Err(e) => {
            eprintln!("Failed to run kilo: {e}");
            return 1;
        }
    };
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!(
        "{}",
        insert_ana_help_row(&String::from_utf8_lossy(&output.stderr))
    );
    output.status.code().unwrap_or(1)
}

/// Insert the `ana` row after the last `kilo <command>` row of the first
/// `Commands:` block, matching its indentation. Returns the text unchanged if
/// the block is not found.
fn insert_ana_help_row(help: &str) -> String {
    let lines: Vec<&str> = help.lines().collect();
    let Some(start) = lines.iter().position(|l| l.trim() == "Commands:") else {
        return help.to_string();
    };
    let block_end = lines[start + 1..]
        .iter()
        .position(|l| !l.trim_start().starts_with("kilo "))
        .map_or(lines.len(), |i| start + 1 + i);
    if block_end == start + 1 {
        return help.to_string();
    }
    let indent: String = lines[block_end - 1]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect();

    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    out.insert(block_end, format!("{indent}{ANA_HELP_ROW}"));
    let mut text = out.join("\n");
    if help.ends_with('\n') {
        text.push('\n');
    }
    text
}

/// Replace the wrapper with `cmd` on Unix so interactive programs own the
/// terminal and signals; wait for it on Windows.
fn hand_off(mut cmd: Command, name: &str) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let e = cmd.exec();
        eprintln!("Failed to run {name}: {e}");
        1
    }
    #[cfg(windows)]
    {
        match cmd.status() {
            Ok(status) => status.code().unwrap_or(1),
            Err(e) => {
                eprintln!("Failed to run {name}: {e}");
                1
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_route_ana_strips_prefix() {
        let a = args(&["ana", "tool", "list"]);
        assert_eq!(route(&a), Route::Ana(&a[1..]));
    }

    #[test]
    fn test_route_managed_commands() {
        assert!(matches!(
            route(&args(&["upgrade", "1.2.3"])),
            Route::Managed { command: "upgrade", .. }
        ));
        assert!(matches!(
            route(&args(&["uninstall"])),
            Route::Managed { command: "uninstall", .. }
        ));
    }

    #[test]
    fn test_route_help_only_at_top_level() {
        assert_eq!(route(&args(&["--help"])), Route::Help);
        assert_eq!(route(&args(&["-h"])), Route::Help);
        assert_eq!(route(&args(&["help"])), Route::Help);
        assert_eq!(route(&args(&["run", "--help"])), Route::Kilo);
        assert_eq!(route(&args(&["help", "run"])), Route::Kilo);
    }

    #[test]
    fn test_route_everything_else_goes_to_kilo() {
        assert_eq!(route(&args(&[])), Route::Kilo);
        assert_eq!(route(&args(&["run", "hello"])), Route::Kilo);
        assert_eq!(route(&args(&["./ana"])), Route::Kilo);
    }

    #[test]
    fn test_insert_ana_help_row_after_command_block() {
        let help = "Commands:\n  kilo run      run kilo\n  kilo help     show help\n\nOptions:\n";
        let out = insert_ana_help_row(help);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[2], "  kilo help     show help");
        assert_eq!(lines[3], format!("  {ANA_HELP_ROW}"));
        assert_eq!(lines[4], "");
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn test_insert_ana_help_row_without_commands_block() {
        let help = "something unexpected\n";
        assert_eq!(insert_ana_help_row(help), help);
    }
}
