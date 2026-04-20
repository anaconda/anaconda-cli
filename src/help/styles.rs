//! Help-specific styles built on top of shared UI styles.

use console::Style;

use crate::ui::styles::{
    AMBER, BLUE, BOX_BG, BOX_BORDER, BOX_TEXT, DESC, DIM, GREEN, RED, hex_color,
};

/// Styles for help output matching UX design
#[allow(dead_code)]
pub(super) enum HelpStyle {
    Section,    // green headers
    Command,    // blue command names
    Desc,       // gray descriptions
    Dim,        // dim gray for comments/hints
    Error,      // error red
    Warning,    // warning yellow
    BoxBorder,  // dim border on box background
    BoxDesc,    // light description text on box background
    BoxCommand, // blue command text on box background
}

impl HelpStyle {
    pub fn style(&self) -> Style {
        let box_bg = hex_color(BOX_BG);
        match self {
            Self::Section => Style::new().fg(hex_color(GREEN)).bold(),
            Self::Command => Style::new().fg(hex_color(BLUE)),
            Self::Desc => Style::new().fg(hex_color(DESC)),
            Self::Dim => Style::new().fg(hex_color(DIM)),
            Self::Error => Style::new().fg(hex_color(RED)),
            Self::Warning => Style::new().fg(hex_color(AMBER)),
            Self::BoxBorder => Style::new().fg(hex_color(BOX_BORDER)).bg(box_bg),
            Self::BoxDesc => Style::new().fg(hex_color(BOX_TEXT)).bg(box_bg),
            Self::BoxCommand => Style::new().fg(hex_color(BLUE)).bg(box_bg).bold(),
        }
    }
}
