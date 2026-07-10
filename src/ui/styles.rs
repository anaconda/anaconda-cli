//! Shared color and style definitions for CLI output.
//!
//! Uses ANSI colors so the terminal applies the user's color profile.
//! Colors are automatically disabled when output is not a TTY.

use owo_colors::{OwoColorize, Stream, Style};
use std::fmt::Display;

/// UI colors that map to standard ANSI colors.
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
    /// Get the ANSI color name for indicatif templates.
    pub fn ansi_name(&self) -> &'static str {
        match self {
            Self::Green => "green",
            Self::Red => "red",
            Self::Amber => "yellow",
            Self::Blue => "cyan",
            Self::Dim => "dim",
            Self::Desc => "white",
            Self::BoxBg => "black",
            Self::BoxBorder => "dim",
            Self::BoxText => "white",
        }
    }

    /// Get a Style with this color as foreground.
    pub fn style(&self) -> Style {
        match self {
            Self::Green => Style::new().green(),
            Self::Red => Style::new().red(),
            Self::Amber => Style::new().yellow(),
            Self::Blue => Style::new().cyan(),
            Self::Dim => Style::new().dimmed(),
            Self::Desc => Style::new(),
            Self::BoxBg => Style::new().on_black(),
            Self::BoxBorder => Style::new().dimmed(),
            Self::BoxText => Style::new().white(),
        }
    }

    /// Apply this color to text, returning a styled string.
    /// Respects NO_COLOR and TTY detection.
    pub fn apply_to<T: Display>(&self, val: T) -> String {
        val.if_supports_color(Stream::Stdout, |v| v.style(self.style()))
            .to_string()
    }

    /// Apply this color with bold to text, returning a styled string.
    /// Respects NO_COLOR and TTY detection.
    pub fn apply_bold<T: Display>(&self, val: T) -> String {
        val.if_supports_color(Stream::Stdout, |v| v.style(self.style().bold()))
            .to_string()
    }

    /// Get a bold Style with this color as foreground.
    pub fn bold(&self) -> Style {
        self.style().bold()
    }

    /// Get a Style with this color as foreground and another as background.
    pub fn on(&self, bg: UiColor) -> Style {
        match bg {
            UiColor::BoxBg => self.style().on_black(),
            _ => self.style(),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_to_returns_string() {
        let result = UiColor::Green.apply_to("test");
        assert!(result.contains("test"));
    }

    #[test]
    fn test_bold_style() {
        let style = UiColor::Red.bold();
        let result = "error".style(style).to_string();
        assert!(result.contains("error"));
    }
}
