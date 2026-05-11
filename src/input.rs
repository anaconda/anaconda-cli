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
/// Displays `message` followed by `[Y/n]` or `[y/N]` depending on the default.
/// The default choice is highlighted and used when the user presses Enter.
///
/// This uses line-based input, so Ctrl+C is handled normally by the terminal.
pub fn prompt_yes_no(message: &str, default: bool) -> bool {
    use crate::ui::status;
    use std::io::Write;

    let prompt = if default {
        status::highlight("Y/n")
    } else {
        status::highlight("y/N")
    };
    print!("{} [{}] ", message, prompt);
    std::io::stdout().flush().unwrap();

    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_err() {
        return default;
    }

    parse_yes_no(&input, default)
}

/// Parse a yes/no response string.
///
/// Returns `true` for "y" or "yes" (case-insensitive).
/// Returns `false` for "n" or "no" (case-insensitive).
/// Returns the default for empty input or unrecognized values.
fn parse_yes_no(input: &str, default: bool) -> bool {
    match input.trim().to_lowercase().as_str() {
        "y" | "yes" => true,
        "n" | "no" => false,
        _ => default,
    }
}

/// Prompt the user for text input.
///
/// Displays `message` followed by `: ` and waits for input.
/// Returns the trimmed input string, or an error if reading fails.
pub fn prompt_input(message: &str) -> Result<String, String> {
    use std::io::Write;
    print!("{}: ", message);
    std::io::stdout().flush().map_err(|e| e.to_string())?;

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| e.to_string())?;

    Ok(input.trim().to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    mod parse_yes_no {
        use super::*;

        #[test]
        fn yes_inputs_return_true() {
            assert!(parse_yes_no("y", false));
            assert!(parse_yes_no("Y", false));
            assert!(parse_yes_no("yes", false));
            assert!(parse_yes_no("YES", false));
            assert!(parse_yes_no("Yes", false));
            assert!(parse_yes_no("yEs", false));
        }

        #[test]
        fn no_inputs_return_false() {
            assert!(!parse_yes_no("n", true));
            assert!(!parse_yes_no("N", true));
            assert!(!parse_yes_no("no", true));
            assert!(!parse_yes_no("NO", true));
            assert!(!parse_yes_no("No", true));
            assert!(!parse_yes_no("nO", true));
        }

        #[test]
        fn empty_input_returns_default() {
            assert!(parse_yes_no("", true));
            assert!(!parse_yes_no("", false));
        }

        #[test]
        fn whitespace_only_returns_default() {
            assert!(parse_yes_no("   ", true));
            assert!(!parse_yes_no("   ", false));
            assert!(parse_yes_no("\t", true));
            assert!(!parse_yes_no("\t", false));
            assert!(parse_yes_no("\n", true));
            assert!(!parse_yes_no("\n", false));
        }

        #[test]
        fn input_with_surrounding_whitespace_is_trimmed() {
            assert!(parse_yes_no("  y  ", false));
            assert!(parse_yes_no("\ty\n", false));
            assert!(parse_yes_no("  yes  ", false));
            assert!(!parse_yes_no("  n  ", true));
            assert!(!parse_yes_no("\tno\n", true));
        }

        #[test]
        fn unrecognized_input_returns_default() {
            assert!(parse_yes_no("yeah", true));
            assert!(!parse_yes_no("yeah", false));
            assert!(parse_yes_no("nope", true));
            assert!(!parse_yes_no("nope", false));
            assert!(parse_yes_no("maybe", true));
            assert!(!parse_yes_no("maybe", false));
            assert!(parse_yes_no("1", true));
            assert!(!parse_yes_no("0", false));
        }

        #[test]
        fn partial_matches_return_default() {
            // "ye" is not "yes"
            assert!(parse_yes_no("ye", true));
            assert!(!parse_yes_no("ye", false));
            // "yess" is not "yes"
            assert!(parse_yes_no("yess", true));
            assert!(!parse_yes_no("yess", false));
        }
    }
}
