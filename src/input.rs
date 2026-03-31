//! Terminal input utilities.
//!
//! Provides reusable components for interactive terminal input:
//! - `KeyListener`: Background key detection with Ctrl+C handling
//! - `prompt_yes_no`: Line-based yes/no confirmation prompt

use std::sync::mpsc::{self, Receiver};
use std::thread;

use console::{Key, Term};

/// Background key listener for raw terminal input.
///
/// Spawns a thread that listens for specific keys and sends them through
/// a channel. Ctrl+C exits with code 130 (standard Unix SIGINT convention).
///
/// The listener manages terminal state internally — when dropped, the
/// terminal is restored to its original state.
pub struct KeyListener {
    _guard: TerminalGuard,
    rx: Receiver<char>,
}

impl KeyListener {
    /// Spawn a background listener for the specified keys.
    ///
    /// Keys are matched case-insensitively ('q' matches both 'q' and 'Q').
    /// Ctrl+C exits the process with code 130.
    ///
    /// Returns `None` if terminal state cannot be saved (e.g., not a TTY).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let listener = KeyListener::spawn(&['q'])?;
    /// // In a loop:
    /// if listener.try_recv().is_some() {
    ///     // 'q' or 'Q' was pressed
    /// }
    /// ```
    pub fn spawn(keys: &[char]) -> Option<Self> {
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

        Some(Self { _guard: guard, rx })
    }

    /// Check if a key was pressed without blocking.
    ///
    /// Returns `Some(char)` if a registered key was pressed, `None` otherwise.
    pub fn try_recv(&self) -> Option<char> {
        self.rx.try_recv().ok()
    }
}

/// Prompt the user for yes/no confirmation.
///
/// Displays `message` followed by `[y/N]` and waits for input.
/// Returns `true` only if the user enters "y" or "yes" (case-insensitive).
/// Returns `false` on empty input, "n", "no", or any read error.
///
/// This uses line-based input, so Ctrl+C is handled normally by the terminal.
pub fn prompt_yes_no(message: &str) -> bool {
    use std::io::Write;
    print!("{} [y/N] ", message);
    std::io::stdout().flush().unwrap();

    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_err() {
        return false;
    }

    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
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
struct TerminalGuard {
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
struct TerminalGuard;

#[cfg(not(unix))]
impl TerminalGuard {
    fn new() -> Option<Self> {
        Some(Self)
    }
}
