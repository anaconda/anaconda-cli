//! Status output utilities for consistent CLI feedback.
//!
//! Provides styled output functions matching the UX design spec:
//! - Success: green checkmark (✓)
//! - Error: red "error:" prefix
//! - Warning: amber exclamation (!)
//! - Info: plain text
//! - Waiting: dim text for in-progress states

use super::styles::UiColor;

/// Print a success message with green checkmark.
///
/// Example output: `✓ Authentication complete`
pub fn success(msg: &str) {
    eprintln!("{} {}", UiColor::Green.apply_to("✓"), msg);
}

/// Print a celebratory success message with party popper.
///
/// Example output: `🎉 You can now install packages from the main-x channel!`
pub fn celebrate(msg: &str) {
    eprintln!("{} {}", UiColor::Green.apply_to("🎉"), msg);
}

/// Print an error message with red "error:" prefix.
///
/// Example output: `error: not logged in`
pub fn error(msg: &str) {
    eprintln!("{} {}", UiColor::Red.apply_to("error:"), msg);
}

/// Print a warning message with amber exclamation.
///
/// Example output: `! To fully revoke your token visit anaconda.com/settings/tokens`
pub fn warn(msg: &str) {
    eprintln!("{} {}", UiColor::Amber.apply_to("!"), msg);
}

/// Return an experimental note styled in amber.
///
/// Example output: `Note: This feature is experimental.`
pub fn note_experimental(msg: &str) -> String {
    UiColor::Amber.apply_to(msg).to_string()
}

/// Print an info message (plain text).
///
/// Example output: `Opening anaconda.com in your browser...`
pub fn info(msg: &str) {
    eprintln!("{}", msg);
}

/// Print a waiting/in-progress message in dim text.
///
/// Example output: `Waiting for authentication...`
pub fn waiting(msg: &str) {
    eprintln!("{}", UiColor::Dim.apply_to(msg));
}

/// Print a tip in dim text.
///
/// Example output: `Tip: Use --pip or --uv to configure only one tool.`
pub fn tip(msg: &str) {
    eprintln!("{}", UiColor::Dim.apply_to(&format!("Tip: {}", msg)));
}

/// Return text styled as highlighted (blue).
///
/// Use this for values like usernames, emails, commands.
pub fn highlight(text: &str) -> String {
    UiColor::Blue.apply_to(text).to_string()
}

/// Return a green checkmark.
#[cfg(feature = "unstable")]
pub fn checkmark() -> String {
    UiColor::Green.apply_to("✓").to_string()
}

/// Return text styled as dim.
///
/// Use this for secondary information, hints.
pub fn dim(text: &str) -> String {
    UiColor::Dim.apply_to(text).to_string()
}

/// Print a blank line to stderr.
pub fn blank_line() {
    eprintln!();
}

/// Print a running message (amber, no newline) that can be updated in place.
///
/// Use `finish_running()` to complete the line with a success message.
pub fn running(msg: &str) {
    use std::io::Write;
    eprint!("{} {}", UiColor::Amber.apply_to("●"), msg);
    std::io::stderr().flush().unwrap();
}

/// Finish a running line by clearing it and printing a success message.
///
/// Should be called after `running()` to replace the line.
pub fn finish_running(msg: &str) {
    // Clear current line and move cursor to start
    eprint!("\r\x1b[K");
    // Print success message with newline
    eprintln!("{} {}", UiColor::Green.apply_to("✓"), msg);
}

/// Return text styled as a section header (green, uppercase).
///
/// Use this for section headers like "ACCOUNT", "SUBSCRIPTIONS".
pub fn section(name: &str) -> String {
    UiColor::Green
        .bold()
        .apply_to(name.to_uppercase())
        .to_string()
}
