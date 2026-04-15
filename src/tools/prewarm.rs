//! Pre-warm macOS security cache and Python bytecode after tool installation.
//!
//! On macOS, the first `dlopen()` of a shared library triggers a Mach IPC
//! message to `syspolicyd`, which hashes the file and records the security
//! assessment. This takes ~0.1-0.2s per library and blocks the calling thread.
//! With ~100+ shared libraries in a typical conda prefix, the first invocation
//! of a tool can take 10-15 seconds longer than subsequent runs.
//!
//! This module provides a function to pre-warm that cache by dlopen/dlclose-ing
//! every shared library in a prefix, plus compiling Python bytecode if Python
//! is present. It is designed to be run as a detached background process so
//! that `ana bootstrap` returns immediately.

use std::path::Path;

/// Delay between compileall and dlopen sweep to let syspolicyd process
/// core Python library assessments before we add more to its queue.
#[cfg(target_os = "macos")]
const COMPILEALL_SETTLE_DELAY: std::time::Duration = std::time::Duration::from_millis(500);

/// Spawn a detached background process to pre-warm the given prefix.
/// Returns immediately; the pre-warming happens asynchronously.
pub fn spawn_background(prefix: &Path) {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!("Cannot determine own executable path for prewarm: {}", e);
            return;
        }
    };

    let prefix_str = match prefix.to_str() {
        Some(s) => s.to_string(),
        None => {
            tracing::debug!("Prefix path is not valid UTF-8, skipping prewarm");
            return;
        }
    };

    match std::process::Command::new(exe)
        .arg("_prewarm")
        .arg(&prefix_str)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            // Reap the child in a background thread so it doesn't become a
            // zombie if the parent stays alive (e.g. installing multiple tools).
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            eprintln!("   Starting background optimization for first use...");
        }
        Err(e) => {
            tracing::debug!("Failed to spawn prewarm process: {}", e);
        }
    }
}

/// Run the pre-warming process for a given prefix. This is the entry point
/// called by `ana _prewarm <prefix>`.
///
/// Order matters: compileall runs first because it launches the prefix's own
/// Python interpreter, which triggers syspolicyd assessment of python itself
/// and its core .so/.dylib dependencies. If the user runs a tool concurrently,
/// Python startup will already be fast. After a short delay (to let the core
/// assessments land), the dlopen sweep covers the remaining shared libraries.
pub fn run(prefix: &Path) {
    if !prefix.is_dir() {
        tracing::debug!("Prewarm prefix does not exist: {}", prefix.display());
        return;
    }

    compile_bytecode(prefix);

    #[cfg(target_os = "macos")]
    {
        std::thread::sleep(COMPILEALL_SETTLE_DELAY);
        warm_shared_libs(prefix);
    }
}

/// Pre-warm macOS security cache by dlopen/dlclose-ing every .so and .dylib.
fn warm_shared_libs(prefix: &Path) {
    #[cfg(target_os = "macos")]
    {
        use std::ffi::CString;

        let libs: Vec<_> = collect_files_by_extension(prefix, &["so", "dylib"])
            .into_iter()
            .filter_map(|p| p.to_str().and_then(|s| CString::new(s).ok()))
            .collect();

        if libs.is_empty() {
            return;
        }

        // Single-threaded: syspolicyd serializes assessments server-side,
        // so multiple threads just add contention (and compete with any
        // foreground process the user might launch).
        let mut count = 0u32;
        for c_path in &libs {
            // SAFETY: dlopen/dlclose are standard POSIX functions.
            // RTLD_LAZY avoids resolving symbols we don't need.
            unsafe {
                let handle = libc::dlopen(c_path.as_ptr(), libc::RTLD_LAZY);
                if !handle.is_null() {
                    libc::dlclose(handle);
                    count += 1;
                }
            }
        }

        tracing::debug!("Pre-warmed {} of {} shared libraries", count, libs.len());
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = prefix;
    }
}

/// Compile Python bytecode if the prefix contains a Python interpreter.
fn compile_bytecode(prefix: &Path) {
    // On Windows, conda puts executables in the prefix root; on Unix, in bin/
    let bin_dir = if cfg!(windows) {
        prefix.to_path_buf()
    } else {
        prefix.join("bin")
    };
    let python = ["python3", "python"]
        .iter()
        .map(|name| bin_dir.join(name))
        .find(|p| p.is_file());

    let python = match python {
        Some(p) => p,
        None => return,
    };

    // Find the site-packages directory to compile
    let lib_dir = prefix.join("lib");
    let target = find_site_packages(&lib_dir).unwrap_or(lib_dir);

    let result = std::process::Command::new(&python)
        // -qq: suppress all output except errors
        // -j 0: use all available CPUs (fine since we're a background process)
        // -o 0 -o 1 -o 2: compile all optimization levels so any python -O mode is covered
        .args([
            "-m",
            "compileall",
            "-qq",
            "-j",
            "0",
            "-o",
            "0",
            "-o",
            "1",
            "-o",
            "2",
        ])
        .arg(&target)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    match result {
        Ok(status) if status.success() => {
            tracing::debug!("Compiled bytecode in {}", target.display());
        }
        Ok(status) => {
            tracing::debug!("compileall exited with {}", status);
        }
        Err(e) => {
            tracing::debug!("Failed to run compileall: {}", e);
        }
    }
}

/// Find the site-packages directory under lib/pythonX.Y/
fn find_site_packages(lib_dir: &Path) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(lib_dir).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if name_str.starts_with("python") && entry.path().is_dir() {
            let sp = entry.path().join("site-packages");
            if sp.is_dir() {
                return Some(sp);
            }
        }
    }
    None
}

/// Recursively collect files matching given extensions.
fn collect_files_by_extension(dir: &Path, extensions: &[&str]) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    collect_recursive(dir, extensions, &mut files);
    files
}

fn collect_recursive(dir: &Path, extensions: &[&str], files: &mut Vec<std::path::PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        // Use entry.file_type() rather than path.is_dir()/is_file() to avoid
        // following symlinks (which could cause cycles) and extra stat calls.
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        let path = entry.path();
        if ft.is_dir() {
            collect_recursive(&path, extensions, files);
        } else if ft.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if extensions.contains(&ext) {
                    files.push(path);
                }
            }
        }
    }
}
