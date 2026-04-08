use std::collections::HashMap;

use console::Term;

use super::data::{HELP_EXAMPLES, HELP_SECTIONS};
use super::styles::HelpStyle;
use crate::VERSION;

/// Print a command row: "  command      description"
fn print_command_row(term: &Term, name: &str, desc: &str) {
    let styled_name = HelpStyle::Command.style().apply_to(name);
    let styled_desc = HelpStyle::Desc.style().apply_to(desc);
    let _ = term.write_line(&format!("  {styled_name:<20} {styled_desc}"));
}

/// Print a section header
fn print_section(term: &Term, name: &str) {
    let _ = term.write_line(
        &HelpStyle::Section
            .style()
            .apply_to(name.to_uppercase())
            .to_string(),
    );
}

/// Print the header at the top of the help output
fn print_header(term: &Term) {
    let _ = term.write_line(&format!(
        "{} {}",
        HelpStyle::Command.style().apply_to("ana"),
        VERSION
    ));
    let tagline = "Manage your Anaconda toolchain and account.";
    let _ = term.write_line(&HelpStyle::Desc.style().apply_to(tagline).to_string());
    let _ = term.write_line("");
    let _ = term.write_line(
        &HelpStyle::Desc
            .style()
            .apply_to("Usage: ana [OPTIONS] COMMAND [ARGS]...")
            .to_string(),
    );
    let _ = term.write_line("");
}

/// Print the examples block in a styled box with rounded corners
fn print_examples_block(term: &Term) {
    print_section(term, "EXAMPLES");

    let margin = "  ";
    let inner_width: usize = 76;
    let cmd_indent = "  "; // Indent for command lines
    let border = HelpStyle::BoxBorder.style();
    let bg = HelpStyle::BoxDesc.style(); // For consistent background

    // Box-drawing characters for rounded corners
    let horizontal = "─";

    // Top border
    let _ = term.write_line(&format!(
        "{margin}{}{}{}",
        border.apply_to("╭"),
        border.apply_to(horizontal.repeat(inner_width)),
        border.apply_to("╮")
    ));

    // Content lines
    for (i, (desc, command)) in HELP_EXAMPLES.iter().enumerate() {
        // Description line (as shell comment)
        let comment = format!("# {desc}");
        let padding = inner_width.saturating_sub(comment.len() + 1);
        let padded_desc = format!(" {comment}{}", " ".repeat(padding));
        let _ = term.write_line(&format!(
            "{margin}{}{}{}",
            border.apply_to("│"),
            HelpStyle::BoxDesc.style().apply_to(&padded_desc),
            border.apply_to("│")
        ));

        // Command line (indented)
        let cmd_with_indent = format!("{cmd_indent}{command}");
        let padding = inner_width.saturating_sub(cmd_with_indent.len() + 1);
        let padded_cmd = format!(" {cmd_with_indent}{}", " ".repeat(padding));
        let _ = term.write_line(&format!(
            "{margin}{}{}{}",
            border.apply_to("│"),
            HelpStyle::BoxCommand.style().apply_to(&padded_cmd),
            border.apply_to("│")
        ));

        // Add spacing between examples (except after last)
        if i < HELP_EXAMPLES.len() - 1 {
            let empty_line = " ".repeat(inner_width);
            let _ = term.write_line(&format!(
                "{margin}{}{}{}",
                border.apply_to("│"),
                bg.apply_to(&empty_line),
                border.apply_to("│")
            ));
        }
    }

    // Bottom border
    let _ = term.write_line(&format!(
        "{margin}{}{}{}",
        border.apply_to("╰"),
        border.apply_to(horizontal.repeat(inner_width)),
        border.apply_to("╯")
    ));
    let _ = term.write_line("");
}

fn print_section_blocks(term: &Term, subcommands: &HashMap<String, String>) {
    for section in HELP_SECTIONS {
        print_section(term, section.name);

        for cmd in section.commands {
            let desc = subcommands.get(cmd.name).map(|s| s.as_str()).unwrap_or("");
            print_command_row(term, cmd.name, desc);
        }

        let _ = term.write_line("");
    }
}

fn print_options_block(term: &Term) {
    print_section(term, "OPTIONS");
    print_command_row(
        term,
        "-v, --verbose",
        "Increase verbosity (can be repeated)",
    );
    print_command_row(term, "-V, --version", "Show the ana version and exit");
    print_command_row(term, "-h, --help", "Show this message and exit");
    let _ = term.write_line("");
}

/// Print the footer at bottom of help output
fn print_footer(term: &Term) {
    let _ = term.write_line(&format!(
        "{} {}",
        HelpStyle::Desc
            .style()
            .apply_to("Full documentation and guides at"),
        HelpStyle::Section.style().apply_to("→ docs.anaconda.com"),
    ));
}

/// Main help output
pub fn print_help(subcommands: HashMap<String, String>) {
    let term = Term::stdout();

    print_header(&term);
    print_examples_block(&term);
    print_section_blocks(&term, &subcommands);
    print_options_block(&term);
    print_footer(&term);
}

/// Help for `ana self` subcommand
pub fn print_self_help() {
    let term = Term::stdout();

    let _ = term.write_line("Manage the ana installation");
    let _ = term.write_line("");
    let _ = term.write_line(
        &HelpStyle::Dim
            .style()
            .apply_to("Usage: ana self <command> [options]")
            .to_string(),
    );
    let _ = term.write_line("");

    print_section(&term, "COMMANDS");
    print_command_row(&term, "update", "Update ana to the latest version");
}

/// Help for `ana auth` subcommand
pub fn print_auth_help() {
    let term = Term::stdout();

    let _ = term.write_line("Authentication commands");
    let _ = term.write_line("");
    let _ = term.write_line(
        &HelpStyle::Dim
            .style()
            .apply_to("Usage: ana auth <command> [options]")
            .to_string(),
    );
    let _ = term.write_line("");

    print_section(&term, "COMMANDS");
    print_command_row(
        &term,
        "api-key",
        "Display the API key for the logged-in user",
    );
    print_command_row(&term, "login", "Log in to Anaconda");
    print_command_row(&term, "logout", "Log out from Anaconda");
    print_command_row(
        &term,
        "whoami",
        "Display information about the logged-in user",
    );
}
