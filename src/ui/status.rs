//! Status output utilities for consistent CLI feedback.
//!
//! Provides styled output functions matching the UX design spec:
//! - Success: green checkmark (✓)
//! - Error: red "error:" prefix
//! - Warning: amber exclamation (!)
//! - Info: plain text
//! - Waiting: dim text for in-progress states

use super::styles::{AMBER, BLUE, DIM, GREEN, RED, style_for};

/// Print a success message with green checkmark.
///
/// Example output: `✓ Authentication complete`
pub fn success(msg: &str) {
    eprintln!("{} {}", style_for(GREEN).apply_to("✓"), msg);
}

/// Print an exciting success message with sparkles.
///
/// Use for final completion messages. Example output: `✨ You can now install packages!`
pub fn great_success(msg: &str) {
    eprintln!("{} {}", style_for(GREEN).apply_to("✨"), msg);
}

/// Print an error message with red "error:" prefix.
///
/// Example output: `error: not logged in`
pub fn error(msg: &str) {
    eprintln!("{} {}", style_for(RED).apply_to("error:"), msg);
}

/// Print a warning message with amber exclamation.
///
/// Example output: `! To fully revoke your token visit anaconda.com/settings/tokens`
pub fn warn(msg: &str) {
    eprintln!("{} {}", style_for(AMBER).apply_to("!"), msg);
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
    eprintln!("{}", style_for(DIM).apply_to(msg));
}

/// Return text styled as highlighted (blue).
///
/// Use this for values like usernames, emails, commands.
pub fn highlight(text: &str) -> String {
    style_for(BLUE).apply_to(text).to_string()
}

/// Return text styled as dim.
///
/// Use this for secondary information, hints.
pub fn dim(text: &str) -> String {
    style_for(DIM).apply_to(text).to_string()
}
