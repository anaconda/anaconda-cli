use std::collections::HashMap;

use console::Term;

use super::data::{HELP_SECTIONS, HelpExample, get_main_examples, get_subcommand_examples};
use super::styles::HelpStyle;
use crate::VERSION;

const GLOBAL_INDENT: usize = 2;
const TAGLINE: &'static str = "Manage your Anaconda toolchain and account.";
const DOCS_URL: &'static str = "https://anaconda.com/docs";

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

fn is_positional(a: &clap::Arg) -> bool {
    a.get_long().is_none() && a.get_short().is_none()
}

fn is_builtin_arg(a: &clap::Arg) -> bool {
    a.get_id() == "help" || a.get_id() == "version"
}

/// Build a usage string for a command
fn build_usage_string(cmd: &clap::Command, path: &str) -> String {
    let user_args: Vec<_> = cmd.get_arguments().filter(|a| !is_builtin_arg(a)).collect();
    let has_options = user_args.iter().any(|a| !is_positional(a));
    let has_subcommands = cmd.get_subcommands().next().is_some();

    let positionals: String = user_args
        .iter()
        .filter(|a| is_positional(a))
        .map(|a| format!("[{}]", a.get_id().as_str().to_uppercase()))
        .collect::<Vec<_>>()
        .join(" ");

    if has_subcommands {
        format!("Usage: ana {} [OPTIONS] COMMAND [ARGS]...", path)
    } else {
        let mut parts = vec![format!("Usage: ana {}", path)];
        if has_options {
            parts.push("[OPTIONS]".to_string());
        }
        if !positionals.is_empty() {
            parts.push(positionals);
        }
        parts.join(" ")
    }
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
fn print_examples_block(term: &Term, examples: Vec<HelpExample>) {
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
    for (i, example) in examples.iter().enumerate() {
        // Description line (as shell comment)
        let desc = &example.desc;
        let command = &example.command;
        let comment = format!("{desc}");
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
        if i < examples.len() - 1 {
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
    print_command_row(term, "-V, --version", "Show the ana version");
    print_command_row(term, "-h, --help", "Show this message");
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
        HelpStyle::Section.style().apply_to(format!("→ {DOCS_URL}")),
    ));
    let _ = term.write_line("");
}

/// Main help output
pub fn print_help(subcommands: HashMap<String, String>) {
    let term = Term::stdout();

    print_header(&term);
    print_examples_block(&term, get_main_examples());
    print_section_blocks(&term, &subcommands);
    print_options_block(&term);
    print_footer(&term);
}

/// Help for a subcommand (e.g., `ana self`, `ana auth`, `ana self update`)
/// `path` is the full command path like "self update"
pub fn print_subcommand_help(cmd: &clap::Command, path: &str) {
    let term = Term::stdout();
    let ind = left_margin();

    // Description
    if let Some(about) = cmd.get_about() {
        let _ = term.write_line(&format!("{}{}", ind, about));
        let _ = term.write_line("");
    }

    let usage = build_usage_string(cmd, path);
    let _ = term.write_line(&format!(
        "{}{}",
        ind,
        HelpStyle::Dim.style().apply_to(usage)
    ));
    let _ = term.write_line("");

    // Examples (if available for this subcommand)
    if let Some(examples) = get_subcommand_examples(path) {
        print_examples_block(&term, examples);
    }

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
        let _ = term.write_line("");
    }

    // Options section - always show at least -h, --help
    let option_args: Vec<_> = cmd
        .get_arguments()
        .filter(|a| !is_builtin_arg(a))
        .filter(|a| a.get_long().is_some() || a.get_short().is_some())
        .collect();
    print_section(&term, "OPTIONS");
    for arg in option_args {
        let short = arg
            .get_short()
            .map(|s| format!("-{}, ", s))
            .unwrap_or_default();
        let long = arg
            .get_long()
            .map(|l| format!("--{}", l))
            .unwrap_or_default();
        let name = format!("{}{}", short, long);
        let desc = arg.get_help().map(|h| h.to_string()).unwrap_or_default();
        print_command_row(&term, &name, &desc);
    }
    print_command_row(&term, "-h, --help", "Show this message");
    let _ = term.write_line("");

    print_footer(&term);
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Arg, Command};

    #[test]
    fn test_usage_simple_command_no_args() {
        let cmd = Command::new("test");
        let usage = build_usage_string(&cmd, "foo");
        assert_eq!(usage, "Usage: ana foo");
    }

    #[test]
    fn test_usage_with_positional_arg() {
        let cmd = Command::new("test").arg(Arg::new("name").required(true));
        let usage = build_usage_string(&cmd, "auth login");
        assert_eq!(usage, "Usage: ana auth login [NAME]");
    }

    #[test]
    fn test_usage_with_options() {
        let cmd = Command::new("test").arg(Arg::new("verbose").long("verbose"));
        let usage = build_usage_string(&cmd, "self update");
        assert_eq!(usage, "Usage: ana self update [OPTIONS]");
    }

    #[test]
    fn test_usage_with_subcommands() {
        let cmd = Command::new("test").subcommand(Command::new("sub"));
        let usage = build_usage_string(&cmd, "self");
        assert_eq!(usage, "Usage: ana self [OPTIONS] COMMAND [ARGS]...");
    }

    #[test]
    fn test_usage_with_options_and_positional() {
        let cmd = Command::new("test")
            .arg(Arg::new("file").required(true))
            .arg(Arg::new("force").long("force"));
        let usage = build_usage_string(&cmd, "upload");
        assert_eq!(usage, "Usage: ana upload [OPTIONS] [FILE]");
    }

    #[test]
    fn test_usage_excludes_builtin_help_version() {
        let cmd = Command::new("test")
            .disable_help_flag(false)
            .disable_version_flag(false);
        let usage = build_usage_string(&cmd, "simple");
        assert_eq!(usage, "Usage: ana simple");
    }
}
