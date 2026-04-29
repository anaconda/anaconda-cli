use console::Term;

use super::styles::HelpStyle;
use super::term::{left_margin, print_command_row, print_section};

const TITLE: &str = "Outerbounds Platform CLI for managing ML infrastructure.";

const OB_EXAMPLES: &[(&str, &str)] = &[
    ("Create a new Outerbounds project", "ana ob init"),
    ("Deploy the current project", "ana ob deploy"),
    ("Open deployed app in browser", "ana ob app view --web"),
];

struct ObSubcommand {
    name: &'static str,
    desc: &'static str,
}

const OB_SUBCOMMANDS: &[ObSubcommand] = &[
    ObSubcommand {
        name: "app",
        desc: "Commands related to Outerbounds apps (+ ana: open, view)",
    },
    ObSubcommand {
        name: "check",
        desc: "Check packages and configuration for compatibility",
    },
    ObSubcommand {
        name: "deploy",
        desc: "Deploy the current project (ana, alias for obproject-deploy)",
    },
    ObSubcommand {
        name: "init",
        desc: "Create a new Outerbounds project (ana)",
    },
    ObSubcommand {
        name: "configure",
        desc: "Decode Outerbounds Platform configuration",
    },
    ObSubcommand {
        name: "fast-bakery",
        desc: "Commands for interacting with Fast Bakery",
    },
    ObSubcommand {
        name: "integrations",
        desc: "Manage resource integrations",
    },
    ObSubcommand {
        name: "kubernetes",
        desc: "Commands for interacting with Kubernetes",
    },
    ObSubcommand {
        name: "perimeter",
        desc: "Manage perimeters",
    },
    ObSubcommand {
        name: "service-principal-configure",
        desc: "Authenticate service principals using JWT",
    },
];

/// Print the examples block in a styled box with rounded corners
fn print_ob_examples_block(term: &Term) {
    print_section(term, "EXAMPLES");

    let margin = left_margin();
    let inner_width: usize = 76;
    let cmd_left_margin = "  ";
    let border = HelpStyle::BoxBorder.style();
    let bg = HelpStyle::BoxDesc.style();

    let horizontal = "─";

    // Top border
    let _ = term.write_line(&format!(
        "{margin}{}{}{}",
        border.apply_to("╭"),
        border.apply_to(horizontal.repeat(inner_width)),
        border.apply_to("╮")
    ));

    // Content lines
    for (i, (desc, command)) in OB_EXAMPLES.iter().enumerate() {
        // Description line
        let comment = format!("{desc}");
        let padding = inner_width.saturating_sub(comment.len() + 1);
        let padded_desc = format!(" {comment}{}", " ".repeat(padding));
        let _ = term.write_line(&format!(
            "{margin}{}{}{}",
            border.apply_to("│"),
            HelpStyle::BoxDesc.style().apply_to(&padded_desc),
            border.apply_to("│")
        ));

        // Command line
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
        if i < OB_EXAMPLES.len() - 1 {
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

/// Help for the outerbounds subcommand with custom styling
pub fn print_outerbounds_help() {
    let term = Term::stdout();
    let ind = left_margin();

    // Description
    let _ = term.write_line(&format!("{}{}", ind, TITLE));
    let _ = term.write_line("");

    // Usage
    let _ = term.write_line(&format!(
        "{}{}",
        ind,
        HelpStyle::Dim
            .style()
            .apply_to("Usage: ana ob [OPTIONS] COMMAND [ARGS]...")
    ));
    let _ = term.write_line("");

    // Examples
    print_ob_examples_block(&term);

    // Commands
    print_section(&term, "COMMANDS");
    for cmd in OB_SUBCOMMANDS {
        print_command_row(&term, cmd.name, cmd.desc);
    }
    let _ = term.write_line("");

    // Options
    print_section(&term, "OPTIONS");
    print_command_row(&term, "-h, --help", "Show this message and exit.");
    let _ = term.write_line("");
}
