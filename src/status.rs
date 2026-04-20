//! Status output utilities for consistent CLI feedback.
//!
//! Provides styled output functions matching the UX design spec:
//! - Success: green checkmark (✓)
//! - Error: red "error:" prefix
//! - Warning: amber exclamation (!)
//! - Info: plain text
//! - Waiting: dim text for in-progress states

use console::{Color, Style};

// Design spec colors
const GREEN: &str = "#3fb950";
const RED: &str = "#f85149";
const AMBER: &str = "#e3b341";
const BLUE: &str = "#79c0ff";
const DIM: &str = "#6e7681";

/// Convert a hex color string to a console Color.
fn hex_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap();
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap();
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap();
    Color::TrueColor(r, g, b)
}

/// Get a style for a given hex color.
fn style_for(hex: &str) -> Style {
    Style::new().fg(hex_color(hex))
}

/// Print a success message with green checkmark.
///
/// Example output: `✓ Authentication complete`
pub fn success(msg: &str) {
    eprintln!("{} {}", style_for(GREEN).apply_to("✓"), msg);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_color() {
        match hex_color("#3fb950") {
            Color::TrueColor(r, g, b) => {
                assert_eq!((r, g, b), (63, 185, 80));
            }
            _ => panic!("Expected TrueColor"),
        }
    }
}
