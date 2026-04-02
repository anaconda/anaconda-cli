use std::collections::HashMap;

use console::Term;

use super::data::{HELP_SECTIONS, is_demo_mode};
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

/// Print the examples/quick-start code block
fn print_examples_block(term: &Term) {
    print_section(term, "Examples");
    let examples: &[(&str, &str)] = &[
        ("set up your full toolchain", "ana install all"),
        (
            "search for and download a package",
            "ana search numpy && ana download numpy",
        ),
        ("browse and pull an AI model", "ana model search llama"),
        (
            "build and deploy your app",
            "ana build && ana deploy --target snowflake",
        ),
    ];
    for (comment, command) in examples {
        let _ = term.write_line(&format!(
            "    {}",
            HelpStyle::Dim.style().apply_to(format!("# {comment}"))
        ));
        let _ = term.write_line(&format!("    {command}"));
    }
    let _ = term.write_line("");
}

/// Print typo hints (demo only)
fn print_typo_hints(term: &Term) {
    let _ = term.write_line(
        &HelpStyle::Desc
            .style()
            .apply_to("Typo? ana will suggest the closest command.")
            .to_string(),
    );
    let _ = term.write_line(&format!(
        "    {}",
        HelpStyle::Dim.style().apply_to("# example")
    ));
    let _ = term.write_line(&format!(
        "    {} {}",
        HelpStyle::Error.style().apply_to("error:"),
        HelpStyle::Desc
            .style()
            .apply_to("unknown command \"instal\"")
    ));
    let _ = term.write_line(&format!(
        "    {} {}",
        HelpStyle::Warning.style().apply_to("tip:"),
        HelpStyle::Desc.style().apply_to(format!(
            "did you mean {}?",
            HelpStyle::Command.style().apply_to("install")
        ))
    ));
    let _ = term.write_line("");
}

fn print_header(term: &Term) {
    let _ = term.write_line(&format!("ana {VERSION}"));
    let tagline = "Manage your toolchain, AI models, builds, and deployments from one place.";
    let _ = term.write_line(&HelpStyle::Desc.style().apply_to(tagline).to_string());
    let _ = term.write_line("");
}

/// Main help output
pub fn print_help(subcommands: HashMap<String, String>) {
    let term = Term::stdout();
    let demo_mode = is_demo_mode();

    // Header block
    print_header(&term);

    // Examples section (demo mode only)
    if demo_mode {
        print_examples_block(&term);
    }

    // Print each section
    for section in HELP_SECTIONS {
        let visible_commands: Vec<_> = section
            .commands
            .iter()
            .filter(|cmd| demo_mode || !cmd.prototype)
            .collect();

        if visible_commands.is_empty() {
            continue;
        }

        print_section(&term, section.name);

        for cmd in visible_commands {
            let base_name = cmd.name.split(" / ").next().unwrap_or(cmd.name);
            let desc = subcommands
                .get(base_name)
                .map(|s| s.as_str())
                .unwrap_or(cmd.desc);
            print_command_row(&term, cmd.name, desc);
        }
        let _ = term.write_line("");
    }

    // Options section
    print_section(&term, "Options");
    if demo_mode {
        print_command_row(
            &term,
            "--at <site>",
            "Select configured site by name or domain",
        );
        print_command_row(
            &term,
            "-v, --verbose",
            "Print debug information to the console",
        );
    }
    print_command_row(&term, "-V, --version", "Show the ana version and exit");
    print_command_row(&term, "-h, --help", "Show this message and exit");
    let _ = term.write_line("");

    // Typo hint box (demo mode only)
    if demo_mode {
        print_typo_hints(&term);
    }

    // Footer
    let run_cmd = format!(
        "Run {} or {} for more",
        HelpStyle::Command.style().apply_to("ana <command> --help"),
        HelpStyle::Command.style().apply_to("ana help <command>")
    );
    let _ = term.write_line(&HelpStyle::Dim.style().apply_to(run_cmd).to_string());
    let _ = term.write_line(
        &HelpStyle::Section
            .style()
            .apply_to("-> docs.anaconda.com")
            .to_string(),
    );
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
