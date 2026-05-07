use console::Term;

use super::data::HelpExample;
use super::styles::HelpStyle;
use super::term::{left_margin, print_command_row, print_examples_block, print_section};

const TITLE: &str = "Outerbounds Platform CLI for managing ML infrastructure.";

fn get_ob_examples() -> Vec<HelpExample> {
    vec![
        HelpExample {
            desc: "Create a new Outerbounds project".to_string(),
            command: "ana ob init".to_string(),
        },
        HelpExample {
            desc: "Deploy the current project".to_string(),
            command: "ana ob deploy".to_string(),
        },
        HelpExample {
            desc: "Open deployed app in browser".to_string(),
            command: "ana ob app view --web".to_string(),
        },
    ]
}

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
        desc: "Deploy the current project (ana)",
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
    print_examples_block(&term, get_ob_examples());

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
