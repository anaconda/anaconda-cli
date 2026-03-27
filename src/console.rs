//! Console utilities for formatted output.

use std::fmt::Write;

// ANSI escape codes
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

/// Format rows as a unicode table with headers.
///
/// Each row is a (key, value) pair. The table auto-sizes columns to fit content.
/// Headers are displayed in bold.
pub fn format_table(headers: (&str, &str), rows: &[(&str, &str)]) -> String {
    let mut output = String::new();

    if rows.is_empty() {
        return output;
    }

    // Calculate column widths (including headers)
    let key_width = rows
        .iter()
        .map(|(k, _)| k.len())
        .chain(std::iter::once(headers.0.len()))
        .max()
        .unwrap_or(0);
    let val_width = rows
        .iter()
        .map(|(_, v)| v.len())
        .chain(std::iter::once(headers.1.len()))
        .max()
        .unwrap_or(0);

    let h_line = "─".repeat(key_width + 2);
    let v_line = "─".repeat(val_width + 2);

    // Top border
    writeln!(output, "┌{}┬{}┐", h_line, v_line).unwrap();

    // Header row (bold)
    writeln!(
        output,
        "│ {BOLD}{:<key_width$}{RESET} │ {BOLD}{:<val_width$}{RESET} │",
        headers.0, headers.1
    )
    .unwrap();

    // Separator
    writeln!(output, "├{}┼{}┤", h_line, v_line).unwrap();

    // Data rows
    for (key, val) in rows {
        writeln!(output, "│ {:<key_width$} │ {:<val_width$} │", key, val).unwrap();
    }

    // Bottom border
    write!(output, "└{}┴{}┘", h_line, v_line).unwrap();

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_table_empty() {
        assert_eq!(format_table(("Key", "Value"), &[]), "");
    }

    #[test]
    fn test_format_table_single_row() {
        let result = format_table(("Key", "Value"), &[("key", "value")]);
        assert!(result.contains("key"));
        assert!(result.contains("value"));
        assert!(result.starts_with('┌'));
        assert!(result.ends_with('┘'));
    }

    #[test]
    fn test_format_table_has_header() {
        let result = format_table(("Name", "Status"), &[("test", "ok")]);
        assert!(result.contains("Name"));
        assert!(result.contains("Status"));
        // Header separator
        assert!(result.contains("├"));
        assert!(result.contains("┼"));
        assert!(result.contains("┤"));
    }

    #[test]
    fn test_format_table_header_bold() {
        let result = format_table(("Key", "Value"), &[("a", "b")]);
        assert!(result.contains(BOLD));
        assert!(result.contains(RESET));
    }
}
