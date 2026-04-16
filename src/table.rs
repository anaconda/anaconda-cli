//! Table formatting utilities using comfy_table.

use comfy_table::{
    Attribute, Cell, Table, modifiers::UTF8_SOLID_INNER_BORDERS, presets::UTF8_FULL,
};

// Re-export commonly used types for convenience
pub use comfy_table::Color;

/// Create a new table with standard formatting (UTF8 borders, bold headers).
pub fn new<I, S>(headers: I) -> Table
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.apply_modifier(UTF8_SOLID_INNER_BORDERS);
    table.set_header(
        headers
            .into_iter()
            .map(|h| Cell::new(h.as_ref()).add_attribute(Attribute::Bold)),
    );
    table
}

/// Create a cell with the given content.
pub fn cell<S: AsRef<str>>(content: S) -> Cell {
    Cell::new(content.as_ref())
}
