//! Cross-platform detached process spawning for telemetry submission.

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
