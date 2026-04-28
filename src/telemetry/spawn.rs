//! Cross-platform detached process spawning and management for telemetry submission.

use std::io;
use std::process::{Command, Stdio};

/// Spawn a detached telemetry submission process.
///
/// The parent can exit immediately - the child continues independently.
pub fn spawn_telemetry_submitter() -> io::Result<()> {
    let exe = std::env::current_exe()?;

    #[cfg(unix)]
    {
        spawn_detached_unix(&exe)
    }

    #[cfg(windows)]
    {
        spawn_detached_windows(&exe)
    }
}

#[cfg(unix)]
fn spawn_detached_unix(exe: &std::path::Path) -> io::Result<()> {
    use std::os::unix::process::CommandExt;

    let mut cmd = Command::new(exe);
    cmd.arg("telemetry-submit")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);

    let _child = cmd.spawn()?;

    Ok(())
}

#[cfg(windows)]
fn spawn_detached_windows(exe: &std::path::Path) -> io::Result<()> {
    use std::os::windows::process::CommandExt;

    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let mut cmd = Command::new(exe);
    cmd.arg("telemetry-submit")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW);

    let _child = cmd.spawn()?;

    Ok(())
}

/// Check for running telemetry-submit processes.
///
/// Returns a list of PIDs.
pub fn list_submitters() -> io::Result<Vec<u32>> {
    #[cfg(unix)]
    {
        list_submitters_unix()
    }

    #[cfg(windows)]
    {
        list_submitters_windows()
    }
}

#[cfg(unix)]
fn list_submitters_unix() -> io::Result<Vec<u32>> {
    let output = Command::new("pgrep")
        .args(["-f", "ana telemetry-submit"])
        .output()?;

    let pids: Vec<u32> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect();

    Ok(pids)
}

#[cfg(windows)]
fn list_submitters_windows() -> io::Result<Vec<u32>> {
    let output = Command::new("wmic")
        .args([
            "process",
            "where",
            "commandline like '%ana%telemetry-submit%'",
            "get",
            "processid",
        ])
        .output()?;

    let pids: Vec<u32> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect();

    Ok(pids)
}

/// Kill any running telemetry-submit processes.
///
/// Returns the number of processes killed.
pub fn kill_submitters() -> io::Result<u32> {
    #[cfg(unix)]
    {
        kill_submitters_unix()
    }

    #[cfg(windows)]
    {
        kill_submitters_windows()
    }
}

#[cfg(unix)]
fn kill_submitters_unix() -> io::Result<u32> {
    // Use pkill to find and kill processes matching "ana telemetry-submit"
    // pkill returns 0 if processes were killed, 1 if none found
    let output = Command::new("pkill")
        .args(["-f", "ana telemetry-submit"])
        .output()?;

    // pkill returns 0 if processes were killed, 1 if none matched
    if output.status.success() {
        Ok(1) // pkill doesn't report count, report minimum
    } else {
        Ok(0)
    }
}

#[cfg(windows)]
fn kill_submitters_windows() -> io::Result<u32> {
    // Use WMIC to find PIDs, then taskkill
    let output = Command::new("wmic")
        .args([
            "process",
            "where",
            "commandline like '%ana%telemetry-submit%'",
            "get",
            "processid",
        ])
        .output()?;

    let output_str = String::from_utf8_lossy(&output.stdout);
    let mut killed = 0;

    for line in output_str.lines() {
        let line = line.trim();
        if let Ok(pid) = line.parse::<u32>() {
            let _ = Command::new("taskkill")
                .args(["/F", "/PID", &pid.to_string()])
                .output();
            killed += 1;
        }
    }

    Ok(killed)
}
