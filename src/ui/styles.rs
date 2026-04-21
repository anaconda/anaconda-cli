//! Shared color and style definitions for CLI output.
//!
//! All colors match the UX design spec.

use console::{Color, Style, StyledObject};

/// UI colors from the design spec.
#[derive(Clone, Copy)]
pub enum UiColor {
    Green,
    Red,
    Amber,
    Blue,
    Dim,
    Desc,
    BoxBg,
    BoxBorder,
    BoxText,
}

impl UiColor {
    /// Get the hex value for this color.
    fn hex(&self) -> &'static str {
        match self {
            Self::Green => "#3fb950",
            Self::Red => "#f85149",
            Self::Amber => "#e3b341",
            Self::Blue => "#79c0ff",
            Self::Dim => "#6e7681",
            Self::Desc => "#8b949e",
            Self::BoxBg => "#161b22",
            Self::BoxBorder => "#30363d",
            Self::BoxText => "#e6edf3",
        }
    }

    /// Convert to a console Color.
    pub fn color(&self) -> Color {
        hex_to_color(self.hex())
    }

    /// Get a Style with this color as foreground.
    pub fn style(&self) -> Style {
        Style::new().fg(self.color())
    }

    /// Apply this color to text, returning a styled object.
    ///
    /// Shorthand for `UiColor::Red.style().apply_to(text)`.
    pub fn apply_to<T>(&self, val: T) -> StyledObject<T> {
        self.style().apply_to(val)
    }

    /// Get a bold Style with this color as foreground.
    pub fn bold(&self) -> Style {
        self.style().bold()
    }

    /// Get a Style with this color as foreground and another as background.
    pub fn on(&self, bg: UiColor) -> Style {
        self.style().bg(bg.color())
    }
}

/// Convert a hex color string to a console Color.
fn hex_to_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap();
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap();
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap();
    Color::TrueColor(r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_color_green() {
        match UiColor::Green.color() {
            Color::TrueColor(r, g, b) => {
                assert_eq!((r, g, b), (63, 185, 80));
            }
            _ => panic!("Expected TrueColor"),
        }
    }

    #[test]
    fn test_ui_color_blue() {
        match UiColor::Blue.color() {
            Color::TrueColor(r, g, b) => {
                assert_eq!((r, g, b), (121, 192, 255));
            }
            _ => panic!("Expected TrueColor"),
        }
    }
}
