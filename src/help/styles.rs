//! Help-specific styles built on top of shared UI styles.

use console::Style;

use crate::ui::styles::UiColor;

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
        let box_bg = UiColor::BoxBg.color();
        match self {
            Self::Section => Style::new().fg(UiColor::Green.color()).bold(),
            Self::Command => Style::new().fg(UiColor::Blue.color()),
            Self::Desc => Style::new().fg(UiColor::Desc.color()),
            Self::Dim => Style::new().fg(UiColor::Dim.color()),
            Self::Error => Style::new().fg(UiColor::Red.color()),
            Self::Warning => Style::new().fg(UiColor::Amber.color()),
            Self::BoxBorder => Style::new().fg(UiColor::BoxBorder.color()).bg(box_bg),
            Self::BoxDesc => Style::new().fg(UiColor::BoxText.color()).bg(box_bg),
            Self::BoxCommand => Style::new().fg(UiColor::Blue.color()).bg(box_bg).bold(),
        }
    }
}
