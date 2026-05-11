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
        match self {
            Self::Section => UiColor::Green.bold(),
            Self::Command => UiColor::Blue.style(),
            Self::Desc => UiColor::Desc.style(),
            Self::Dim => UiColor::Dim.style(),
            Self::Error => UiColor::Red.style(),
            Self::Warning => UiColor::Amber.style(),
            Self::BoxBorder => UiColor::BoxBorder.on(UiColor::BoxBg),
            Self::BoxDesc => UiColor::BoxText.on(UiColor::BoxBg),
            Self::BoxCommand => UiColor::Blue.on(UiColor::BoxBg).bold(),
        }
    }
}
