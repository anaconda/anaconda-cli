//! Terminal input utilities.
//!
//! Provides reusable components for interactive terminal input:
//! - `TerminalGuard`: RAII guard for terminal state restoration
//! - `KeyListener`: Background key detection with Ctrl+C handling

use std::sync::mpsc::{self, Receiver};
use std::thread;

use console::{Key, Term};

/// Background key listener for raw terminal input.
///
/// Spawns a thread that listens for specific keys and sends them through
/// a channel. Ctrl+C exits with code 130 (standard Unix SIGINT convention).
pub struct KeyListener;

impl KeyListener {
    /// Spawn a background listener for the specified keys.
    ///
    /// Keys are matched case-insensitively ('q' matches both 'q' and 'Q').
    /// Ctrl+C exits the process with code 130.
    ///
    /// Returns `None` if terminal state cannot be saved (e.g., not a TTY).
    /// The returned `TerminalGuard` must be kept alive as long as the listener
    /// is needed — dropping it restores the terminal to its original state.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let (_guard, rx) = KeyListener::spawn(&['q'])?;
    /// // In a loop:
    /// if rx.try_recv().is_ok() {
    ///     // 'q' or 'Q' was pressed
    /// }
    /// ```
    pub fn spawn(keys: &[char]) -> Option<(TerminalGuard, Receiver<char>)> {
        let guard = TerminalGuard::new()?;
        let (tx, rx) = mpsc::channel();
        let keys: Vec<char> = keys.iter().map(|c| c.to_ascii_lowercase()).collect();

        thread::spawn(move || {
            let term = Term::stdout();
            loop {
                if let Ok(key) = term.read_key() {
                    // Handle Ctrl+C (appears as '\x03' in raw mode)
                    if matches!(key, Key::Char('\x03')) {
                        drop(term);
                        std::process::exit(130);
                    }

                    // Check against registered keys (case-insensitive)
                    if let Key::Char(c) = key {
                        if keys.contains(&c.to_ascii_lowercase()) {
                            let _ = tx.send(c);
                            break;
                        }
                    }
                }
            }
        });

        Some((guard, rx))
    }
}

/// RAII guard for terminal state restoration.
///
/// The console crate's `read_key()` puts stdin into raw mode. If the
/// spawned keyboard-listener thread is still blocked in `read_key()` when
/// the process exits normally (successful auth, timeout, Ctrl-C), the
/// crate never restores termios, leaving the user's shell corrupted.
///
/// This guard captures termios before the read thread starts and restores
/// it on drop, regardless of how the calling function exits.
#[cfg(unix)]
pub struct TerminalGuard {
    saved: libc::termios,
    fd: std::os::unix::io::RawFd,
}

#[cfg(unix)]
impl TerminalGuard {
    fn new() -> Option<Self> {
        use std::os::unix::io::AsRawFd;
        let fd = std::io::stdin().as_raw_fd();
        let mut termios = std::mem::MaybeUninit::uninit();
        let rc = unsafe { libc::tcgetattr(fd, termios.as_mut_ptr()) };
        if rc == 0 {
            Some(Self {
                saved: unsafe { termios.assume_init() },
                fd,
            })
        } else {
            None
        }
    }
}

#[cfg(unix)]
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        unsafe {
            libc::tcsetattr(self.fd, libc::TCSADRAIN, &self.saved);
        }
    }
}

#[cfg(not(unix))]
pub struct TerminalGuard;

#[cfg(not(unix))]
impl TerminalGuard {
    fn new() -> Option<Self> {
        Some(Self)
    }
}
