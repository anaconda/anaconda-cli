//! Shared color and style definitions for CLI output.
//!
//! All colors match the UX design spec.

use console::{Color, Style};

// Design spec colors
pub const GREEN: &str = "#3fb950";
pub const RED: &str = "#f85149";
pub const AMBER: &str = "#e3b341";
pub const BLUE: &str = "#79c0ff";
pub const DIM: &str = "#6e7681";
pub const DESC: &str = "#8b949e";
pub const BOX_BG: &str = "#161b22";
pub const BOX_BORDER: &str = "#30363d";
pub const BOX_TEXT: &str = "#e6edf3";

/// Convert a hex color string to a console Color.
pub fn hex_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap();
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap();
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap();
    Color::TrueColor(r, g, b)
}

/// Get a style for a given hex color.
pub fn style_for(hex: &str) -> Style {
    Style::new().fg(hex_color(hex))
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

    #[test]
    fn test_hex_color_without_hash() {
        match hex_color("79c0ff") {
            Color::TrueColor(r, g, b) => {
                assert_eq!((r, g, b), (121, 192, 255));
            }
            _ => panic!("Expected TrueColor"),
        }
    }
}
