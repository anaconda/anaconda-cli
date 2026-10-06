//! Terminal input utilities.
//!
//! Provides reusable components for interactive terminal input:
//! - `KeyListener`: Background key detection with Ctrl+C handling
//! - `prompt_yes_no`: Line-based yes/no confirmation prompt
//! - `multiselect`: Interactive checkbox list with key hints
//! - `search_select`: Interactive single-select list with a search box

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
                    if let Key::Char(c) = key
                        && keys.contains(&c.to_ascii_lowercase())
                    {
                        let _ = tx.send(c);
                        break;
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

/// Truncate plain text to a display width, appending an ellipsis when cut.
///
/// Lines are truncated before styling so a wrapped prompt can never occupy
/// more terminal rows than the redraw logic accounts for.
fn fit(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    console::truncate_str(text, width, "…").to_string()
}

/// Best-effort terminal width in columns.
///
/// `Term::stderr()` size detection fails when stderr is not a tty (e.g. piped
/// through a task runner) even though the user is interacting through a tty
/// on stdin, so fall back to stdout and then stdin before giving up.
fn terminal_width(term: &Term) -> usize {
    term.size_checked()
        .or_else(|| Term::stdout().size_checked())
        .map(|(_, cols)| usize::from(cols))
        .or_else(stdin_width)
        .unwrap_or(80)
}

#[cfg(unix)]
fn stdin_width() -> Option<usize> {
    unsafe {
        let mut winsize: libc::winsize = std::mem::zeroed();
        // FIXME: ".into()" works around a libc bug (mirrors the console crate)
        #[allow(clippy::useless_conversion)]
        if libc::ioctl(libc::STDIN_FILENO, libc::TIOCGWINSZ.into(), &mut winsize) == 0
            && winsize.ws_col > 0
        {
            Some(usize::from(winsize.ws_col))
        } else {
            None
        }
    }
}

#[cfg(not(unix))]
fn stdin_width() -> Option<usize> {
    None
}

/// Interactive multi-select checkbox list.
///
/// Renders a bold prompt, a checkbox list, and a key-hint line below the
/// list. Arrow keys move the cursor, space toggles the active item, and
/// enter confirms the selection. Escape or Ctrl+C aborts, returning an
/// error. Styling matches the dialoguer `ColorfulTheme` look used
/// previously: `[x]` prefixes in green, the active row in cyan.
///
/// Every line is truncated to the terminal width so no line soft-wraps;
/// otherwise `clear_last_lines` would under-clear and redraws would leave
/// duplicated prompt text behind.
///
/// All output goes to stderr so stdout stays clean for machine-readable
/// output. The caller is expected to have verified stdin is a terminal.
#[cfg_attr(not(tool_install), allow(dead_code))]
pub fn multiselect(prompt: &str, items: &[&str], defaults: &[bool]) -> Result<Vec<usize>, String> {
    use std::fmt::Write as _;

    if items.is_empty() {
        return Ok(Vec::new());
    }

    let term = Term::stderr();
    let width = terminal_width(&term);
    let mut cursor = 0usize;
    let mut checked: Vec<bool> = items
        .iter()
        .enumerate()
        .map(|(i, _)| defaults.get(i).copied().unwrap_or(false))
        .collect();
    // Number of lines drawn in the previous frame (prompt + items + hint).
    let mut rendered = 0usize;

    term.hide_cursor().map_err(|e| e.to_string())?;
    let outcome = loop {
        if rendered > 0
            && let Err(e) = term.clear_last_lines(rendered)
        {
            break Err(e.to_string());
        }

        let mut frame = format!(
            "{} {}\n",
            console::style("?").for_stderr().yellow(),
            console::style(fit(prompt, width.saturating_sub(2)))
                .for_stderr()
                .bold()
        );
        for (i, item) in items.iter().enumerate() {
            let prefix = if checked[i] {
                console::style("  [x]").for_stderr().green()
            } else {
                console::style("  [ ]").for_stderr().dim()
            };
            let fitted = fit(item, width.saturating_sub(6));
            let label = if i == cursor {
                console::style(fitted).for_stderr().cyan()
            } else {
                console::style(fitted).for_stderr()
            };
            let _ = writeln!(frame, "{prefix} {label}");
        }
        let _ = writeln!(
            frame,
            "  {}",
            console::style(fit(
                "↑/↓ arrows to navigate · space to select · enter to complete",
                width.saturating_sub(2)
            ))
            .for_stderr()
            .dim()
        );
        rendered = items.len() + 2;

        if let Err(e) = term.write_str(&frame).and_then(|_| term.flush()) {
            break Err(e.to_string());
        }

        match term.read_key() {
            Ok(Key::ArrowUp) => cursor = (cursor + items.len() - 1) % items.len(),
            Ok(Key::ArrowDown) => cursor = (cursor + 1) % items.len(),
            Ok(Key::Char(' ')) => checked[cursor] = !checked[cursor],
            Ok(Key::Enter) => {
                let _ = term.clear_last_lines(rendered);
                let selections: Vec<&str> = items
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| checked[*i])
                    .map(|(_, item)| *item)
                    .collect();
                let fitted_prompt = fit(prompt, width.saturating_sub(2));
                let mut finished = format!(
                    "{} {}",
                    console::style("✔").for_stderr().green(),
                    console::style(&fitted_prompt).for_stderr().bold()
                );
                if !selections.is_empty() {
                    let used = 2 + console::measure_text_width(&fitted_prompt) + 1;
                    let fitted = fit(&selections.join(", "), width.saturating_sub(used));
                    if !fitted.is_empty() {
                        let _ =
                            write!(finished, " {}", console::style(fitted).for_stderr().green());
                    }
                }
                let _ = term.write_line(&finished);
                let _ = term.flush();
                break Ok(checked
                    .iter()
                    .enumerate()
                    .filter(|&(_, &is_checked)| is_checked)
                    .map(|(i, _)| i)
                    .collect());
            }
            Ok(Key::Escape) => break Err("cancelled by user".to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                break Err("cancelled by user".to_string());
            }
            Err(e) => break Err(e.to_string()),
            _ => {}
        }
    };

    let _ = term.show_cursor();
    let _ = term.flush();
    outcome
}

/// An entry in a [`search_select`] list.
#[cfg_attr(not(tool_install), allow(dead_code))]
pub struct SearchItem {
    /// Text the query is matched against (e.g. a model name).
    pub key: String,
    /// Text displayed for the row. May include extra details beyond `key`.
    pub label: String,
}

/// What to do after a key press in [`search_select`].
#[derive(Debug, PartialEq)]
enum SearchAction {
    Continue,
    Select(usize),
    Cancel,
}

/// Query, cursor, and scroll state for [`search_select`], kept separate from
/// rendering so it can be unit tested.
#[derive(Debug, Default)]
struct SearchState {
    query: String,
    /// Index into the current matches.
    cursor: usize,
    /// First visible match (scroll position).
    offset: usize,
}

#[cfg_attr(not(tool_install), allow(dead_code))]
impl SearchState {
    /// Indices of items whose key contains every whitespace-separated query
    /// term (case-insensitive).
    fn matches(&self, items: &[SearchItem]) -> Vec<usize> {
        let terms: Vec<String> = self
            .query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                let key = item.key.to_lowercase();
                terms.iter().all(|t| key.contains(t))
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Apply a key press. `matches` are the matches before the key, and
    /// `height` is the number of visible rows.
    fn handle_key(&mut self, key: Key, matches: &[usize], height: usize) -> SearchAction {
        let last = matches.len().saturating_sub(1);
        match key {
            Key::Escape | Key::CtrlC | Key::Char('\u{3}') => return SearchAction::Cancel,
            Key::Enter => {
                if let Some(&index) = matches.get(self.cursor) {
                    return SearchAction::Select(index);
                }
            }
            Key::Char(c) if !c.is_control() => {
                self.query.push(c);
                self.cursor = 0;
                self.offset = 0;
            }
            Key::Backspace => {
                if self.query.pop().is_some() {
                    self.cursor = 0;
                    self.offset = 0;
                }
            }
            Key::ArrowUp if !matches.is_empty() => {
                self.cursor = if self.cursor == 0 {
                    last
                } else {
                    self.cursor - 1
                };
            }
            Key::ArrowDown if !matches.is_empty() => {
                self.cursor = if self.cursor >= last {
                    0
                } else {
                    self.cursor + 1
                };
            }
            Key::PageUp => self.cursor = self.cursor.saturating_sub(height.max(1)),
            Key::PageDown => self.cursor = (self.cursor + height.max(1)).min(last),
            Key::Home => self.cursor = 0,
            Key::End => self.cursor = last,
            _ => {}
        }
        self.scroll_into_view(height);
        SearchAction::Continue
    }

    /// Adjust the scroll offset so the cursor row is visible.
    fn scroll_into_view(&mut self, height: usize) {
        let height = height.max(1);
        if self.cursor < self.offset {
            self.offset = self.cursor;
        } else if self.cursor >= self.offset + height {
            self.offset = self.cursor + 1 - height;
        }
    }
}

/// Interactive single-select list with a search box.
///
/// Typing filters the list (every whitespace-separated term must appear in
/// an item's `key`), arrow/page keys move the highlight, enter selects, and
/// escape or Ctrl+C cancels. The list scrolls to fit the terminal height.
///
/// Returns the index of the selected item, or `Ok(None)` when cancelled. All
/// output goes to stderr; the caller is expected to have verified stdin is a
/// terminal.
#[cfg_attr(not(tool_install), allow(dead_code))]
pub fn search_select(prompt: &str, items: &[SearchItem]) -> Result<Option<usize>, String> {
    use std::fmt::Write as _;

    let term = Term::stderr();
    let width = terminal_width(&term);
    let rows = term
        .size_checked()
        .or_else(|| Term::stdout().size_checked())
        .map(|(rows, _)| usize::from(rows))
        .unwrap_or(24);
    // Leave room for the prompt, query, and hint lines.
    let height = rows.saturating_sub(4).clamp(3, 15);

    let mut state = SearchState::default();
    let mut rendered = 0usize;

    term.hide_cursor().map_err(|e| e.to_string())?;
    let outcome = loop {
        if rendered > 0
            && let Err(e) = term.clear_last_lines(rendered)
        {
            break Err(e.to_string());
        }

        let matches = state.matches(items);
        let mut frame = format!(
            "{} {}\n",
            console::style("?").for_stderr().yellow(),
            console::style(fit(prompt, width.saturating_sub(2)))
                .for_stderr()
                .bold()
        );

        let query_line = if state.query.is_empty() {
            console::style("type to search".to_string())
                .for_stderr()
                .dim()
                .to_string()
        } else {
            format!(
                "{}{}",
                fit(&state.query, width.saturating_sub(5)),
                console::style("▏").for_stderr().cyan()
            )
        };
        let _ = writeln!(
            frame,
            "  {} {}",
            console::style("›").for_stderr().cyan(),
            query_line
        );

        let visible = matches.iter().enumerate().skip(state.offset).take(height);
        let mut lines = 0;
        for (pos, &index) in visible {
            let label = fit(&items[index].label, width.saturating_sub(4));
            if pos == state.cursor {
                let _ = writeln!(
                    frame,
                    "  {} {}",
                    console::style("❯").for_stderr().cyan(),
                    console::style(label).for_stderr().cyan()
                );
            } else {
                let _ = writeln!(frame, "    {label}");
            }
            lines += 1;
        }
        if matches.is_empty() {
            let _ = writeln!(
                frame,
                "    {}",
                console::style("No matches").for_stderr().dim()
            );
            lines += 1;
        }

        let hint = format!(
            "{}/{} · ↑/↓ to navigate · enter to select · esc to cancel",
            matches.len(),
            items.len()
        );
        let _ = writeln!(
            frame,
            "  {}",
            console::style(fit(&hint, width.saturating_sub(2)))
                .for_stderr()
                .dim()
        );
        rendered = lines + 3;

        if let Err(e) = term.write_str(&frame).and_then(|_| term.flush()) {
            break Err(e.to_string());
        }

        let key = match term.read_key() {
            Ok(key) => key,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => Key::CtrlC,
            Err(e) => break Err(e.to_string()),
        };

        match state.handle_key(key, &matches, height) {
            SearchAction::Continue => {}
            SearchAction::Cancel => {
                let _ = term.clear_last_lines(rendered);
                break Ok(None);
            }
            SearchAction::Select(index) => {
                let _ = term.clear_last_lines(rendered);
                let fitted_prompt = fit(prompt, width.saturating_sub(2));
                let used = 2 + console::measure_text_width(&fitted_prompt) + 1;
                let _ = term.write_line(&format!(
                    "{} {} {}",
                    console::style("✔").for_stderr().green(),
                    console::style(&fitted_prompt).for_stderr().bold(),
                    console::style(fit(&items[index].key, width.saturating_sub(used)))
                        .for_stderr()
                        .green()
                ));
                break Ok(Some(index));
            }
        }
    };

    let _ = term.show_cursor();
    let _ = term.flush();
    outcome
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

    mod fit {
        use super::*;

        #[test]
        fn short_text_is_unchanged() {
            assert_eq!(fit("hello", 10), "hello");
            assert_eq!(fit("hello", 5), "hello");
        }

        #[test]
        fn truncation_never_exceeds_the_width() {
            let text = "Select agents to configure with the Anaconda MCP service";
            for width in 1..=40 {
                let fitted = fit(text, width);
                assert!(
                    console::measure_text_width(&fitted) <= width,
                    "width {width}"
                );
                assert!(fitted.ends_with('…'));
            }
        }

        #[test]
        fn zero_width_returns_empty() {
            assert_eq!(fit("hello", 0), "");
        }

        #[test]
        fn wide_characters_never_exceed_display_width() {
            // "日本語" is 3 chars but 6 display columns.
            let fitted = fit("日本語テスト", 8);
            assert!(console::measure_text_width(&fitted) <= 8);
            assert!(fitted.ends_with('…'));
        }
    }

    mod search_state {
        use super::*;

        fn items(keys: &[&str]) -> Vec<SearchItem> {
            keys.iter()
                .map(|k| SearchItem {
                    key: k.to_string(),
                    label: format!("{k}  details"),
                })
                .collect()
        }

        fn type_str(state: &mut SearchState, items: &[SearchItem], s: &str) {
            for c in s.chars() {
                let m = state.matches(items);
                state.handle_key(Key::Char(c), &m, 5);
            }
        }

        #[test]
        fn empty_query_matches_everything() {
            let items = items(&["a", "b", "c"]);
            assert_eq!(SearchState::default().matches(&items), vec![0, 1, 2]);
        }

        #[test]
        fn query_terms_are_anded_and_case_insensitive() {
            let items = items(&[
                "Qwen/Qwen2.5-0.5B-Instruct",
                "Qwen/Qwen3-8B",
                "google/gemma-2-2b",
            ]);
            let mut state = SearchState::default();
            type_str(&mut state, &items, "QWEN");
            assert_eq!(state.matches(&items), vec![0, 1]);
            type_str(&mut state, &items, " instruct");
            assert_eq!(state.matches(&items), vec![0]);
        }

        #[test]
        fn query_matches_key_not_label() {
            let items = items(&["alpha"]);
            let mut state = SearchState::default();
            type_str(&mut state, &items, "details");
            assert!(state.matches(&items).is_empty());
        }

        #[test]
        fn typing_and_backspace_reset_cursor() {
            let items = items(&["aa", "ab", "ac"]);
            let mut state = SearchState::default();
            let m = state.matches(&items);
            state.handle_key(Key::ArrowDown, &m, 5);
            state.handle_key(Key::ArrowDown, &m, 5);
            assert_eq!(state.cursor, 2);
            type_str(&mut state, &items, "a");
            assert_eq!(state.cursor, 0);
            let m = state.matches(&items);
            state.handle_key(Key::ArrowDown, &m, 5);
            state.handle_key(Key::Backspace, &m, 5);
            assert_eq!((state.cursor, state.query.as_str()), (0, ""));
        }

        #[test]
        fn arrows_wrap_around() {
            let items = items(&["a", "b", "c"]);
            let mut state = SearchState::default();
            let m = state.matches(&items);
            state.handle_key(Key::ArrowUp, &m, 5);
            assert_eq!(state.cursor, 2);
            state.handle_key(Key::ArrowDown, &m, 5);
            assert_eq!(state.cursor, 0);
        }

        #[test]
        fn scrolling_keeps_cursor_visible() {
            let keys: Vec<String> = (0..10).map(|i| format!("m{i}")).collect();
            let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
            let items = items(&refs);
            let mut state = SearchState::default();
            let m = state.matches(&items);
            for _ in 0..4 {
                state.handle_key(Key::ArrowDown, &m, 3);
            }
            assert_eq!((state.cursor, state.offset), (4, 2));
            state.handle_key(Key::Home, &m, 3);
            assert_eq!((state.cursor, state.offset), (0, 0));
            state.handle_key(Key::End, &m, 3);
            assert_eq!((state.cursor, state.offset), (9, 7));
            state.handle_key(Key::PageUp, &m, 3);
            assert_eq!(state.cursor, 6);
            // Wrapping from the top jumps the view to the end.
            state.handle_key(Key::Home, &m, 3);
            state.handle_key(Key::ArrowUp, &m, 3);
            assert_eq!((state.cursor, state.offset), (9, 7));
        }

        #[test]
        fn enter_selects_highlighted_item() {
            let items = items(&["Qwen3-8B", "gemma-2-2b", "Qwen2.5"]);
            let mut state = SearchState::default();
            type_str(&mut state, &items, "qwen");
            let m = state.matches(&items);
            state.handle_key(Key::ArrowDown, &m, 5);
            assert_eq!(state.handle_key(Key::Enter, &m, 5), SearchAction::Select(2));
        }

        #[test]
        fn enter_with_no_matches_does_nothing() {
            let items = items(&["a"]);
            let mut state = SearchState::default();
            type_str(&mut state, &items, "zzz");
            let m = state.matches(&items);
            assert_eq!(state.handle_key(Key::Enter, &m, 5), SearchAction::Continue);
        }

        #[test]
        fn escape_and_ctrl_c_cancel() {
            let items = items(&["a"]);
            let m = SearchState::default().matches(&items);
            for key in [Key::Escape, Key::CtrlC, Key::Char('\u{3}')] {
                assert_eq!(
                    SearchState::default().handle_key(key, &m, 5),
                    SearchAction::Cancel
                );
            }
        }
    }

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
