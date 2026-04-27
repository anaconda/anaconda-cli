use std::collections::HashMap;

use console::Term;

use super::data::{HELP_EXAMPLES, HELP_SECTIONS};
use super::styles::HelpStyle;
use crate::VERSION;

const GLOBAL_INDENT: usize = 2;
const TAGLINE: &'static str = "Manage your Anaconda toolchain and account.";

/// Create a string of spaces for the global left_margin
fn left_margin() -> String {
    " ".repeat(GLOBAL_INDENT)
}

/// Print a command row: "    command      description"
fn print_command_row(term: &Term, name: &str, desc: &str) {
    let styled_name = HelpStyle::Command.style().apply_to(name);
    let styled_desc = HelpStyle::Desc.style().apply_to(desc);
    let _ = term.write_line(&format!(
        "{}  {styled_name:<20} {styled_desc}",
        left_margin()
    ));
}

/// Print a section header
fn print_section(term: &Term, name: &str) {
    let _ = term.write_line(&format!(
        "{}{}",
        left_margin(),
        HelpStyle::Section.style().apply_to(name.to_uppercase())
    ));
}

/// Print the header at the top of the help output
fn print_header(term: &Term) {
    let ind = left_margin();
    let _ = term.write_line(&format!(
        "{}{} {}",
        ind,
        HelpStyle::Command.style().apply_to("ana"),
        VERSION
    ));
    let _ = term.write_line(&format!(
        "{}{}",
        ind,
        HelpStyle::Desc.style().apply_to(TAGLINE)
    ));
    let _ = term.write_line("");
    let _ = term.write_line(&format!(
        "{}{}",
        ind,
        HelpStyle::Desc
            .style()
            .apply_to("Usage: ana [OPTIONS] COMMAND [ARGS]...")
    ));
    let _ = term.write_line("");
}

/// Print the examples block in a styled box with rounded corners
fn print_examples_block(term: &Term) {
    print_section(term, "EXAMPLES");

    let margin = left_margin();
    let inner_width: usize = 76;
    let cmd_left_margin = "  "; // Indent for command lines
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

        // Command line (left_margined)
        let cmd_with_left_margin = format!("{cmd_left_margin}{command}");
        let padding = inner_width.saturating_sub(cmd_with_left_margin.len() + 1);
        let padded_cmd = format!(" {cmd_with_left_margin}{}", " ".repeat(padding));
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
            let desc = subcommands.get(*cmd).map(|s| s.as_str()).unwrap_or("");
            print_command_row(term, cmd, desc);
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
        "{}{} {}",
        left_margin(),
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

/// Help for a subcommand (e.g., `ana self`, `ana auth`, `ana bootstrap`)
pub fn print_subcommand_help(cmd: &clap::Command) {
    let term = Term::stdout();
    let ind = left_margin();

    // Description
    if let Some(about) = cmd.get_about() {
        let _ = term.write_line(&format!("{}{}", ind, about));
        let _ = term.write_line("");
    }

    // Usage - render and ensure it starts with "ana "
    let usage = cmd.clone().render_usage().to_string();
    let usage = if usage.starts_with("Usage: ana ") {
        usage
    } else {
        usage.replacen("Usage: ", "Usage: ana ", 1)
    };
    let _ = term.write_line(&format!(
        "{}{}",
        ind,
        HelpStyle::Dim.style().apply_to(usage)
    ));
    let _ = term.write_line("");

    // Commands (only if there are subcommands)
    let subcommands: Vec<_> = cmd.get_subcommands().collect();
    if !subcommands.is_empty() {
        print_section(&term, "COMMANDS");
        for subcmd in subcommands {
            let name = subcmd.get_name();
            let desc = subcmd
                .get_about()
                .map(|a| a.to_string())
                .unwrap_or_default();
            print_command_row(&term, name, &desc);
        }
    }
}
