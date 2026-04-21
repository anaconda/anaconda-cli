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

/// Return text styled as highlighted (blue).
///
/// Use this for values like usernames, emails, commands.
pub fn highlight(text: &str) -> String {
    UiColor::Blue.apply_to(text).to_string()
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

/// Return text styled as a section header (green, uppercase).
///
/// Use this for section headers like "ACCOUNT", "SUBSCRIPTIONS".
pub fn section(name: &str) -> String {
    UiColor::Green.apply_to(name.to_uppercase()).to_string()
}
