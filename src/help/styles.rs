use console::{Color, Style};

/// Convert a hex color string to a console Color
fn hex_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap();
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap();
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap();
    Color::TrueColor(r, g, b)
}

/// Styles for help output matching UX design
pub(super) enum HelpStyle {
    Section,    // green headers
    Command,    // blue command names
    Desc,       // gray descriptions
    Dim,        // dim gray for comments/hints
    Error,      // error red
    Warning,    // warning yellow
    BoxDim,     // dim text on box background
    BoxCommand, // command text on box background
}

impl HelpStyle {
    pub fn style(&self) -> Style {
        match self {
            Self::Section => Style::new().fg(hex_color("#3fb950")).bold(),
            Self::Command => Style::new().fg(hex_color("#79c0ff")),
            Self::Desc => Style::new().fg(hex_color("#8b949e")),
            Self::Dim => Style::new().fg(hex_color("#6e7681")),
            Self::Error => Style::new().fg(hex_color("#f85149")),
            Self::Warning => Style::new().fg(hex_color("#d29922")),
            Self::BoxDim => Style::new()
                .fg(hex_color("#6e7681"))
                .bg(hex_color("#21262d")),
            Self::BoxCommand => Style::new()
                .fg(hex_color("#79c0ff"))
                .bg(hex_color("#21262d")),
        }
    }
}
