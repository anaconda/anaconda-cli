use std::collections::HashMap;

use console::{Color, Style, Term};

use crate::VERSION;

/// Convert a hex color string to a console Color
fn hex_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap();
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap();
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap();
    Color::TrueColor(r, g, b)
}

/// Check if demo mode is enabled via ANA_DEMO=true
fn is_demo_mode() -> bool {
    std::env::var("ANA_DEMO")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
}

/// Styles for help output matching UX design
struct HelpStyles {
    section: Style, // #3fb950 - green headers
    command: Style, // #79c0ff - blue command names
    desc: Style,    // #8b949e - gray descriptions
    dim: Style,     // #6e7681 - dim gray for comments/hints
    error: Style,   // #f85149 - error red
    warning: Style, // #d29922 - warning yellow
}

impl HelpStyles {
    fn new() -> Self {
        Self {
            section: Style::new().fg(hex_color("#3fb950")).bold(),
            command: Style::new().fg(hex_color("#79c0ff")),
            desc: Style::new().fg(hex_color("#8b949e")),
            dim: Style::new().fg(hex_color("#6e7681")),
            error: Style::new().fg(hex_color("#f85149")),
            warning: Style::new().fg(hex_color("#d29922")),
        }
    }
}

/// Command definition for help output
struct HelpCommand {
    name: &'static str,
    desc: &'static str,
    prototype: bool,
}

impl HelpCommand {
    const fn real(name: &'static str, desc: &'static str) -> Self {
        Self {
            name,
            desc,
            prototype: false,
        }
    }

    const fn proto(name: &'static str, desc: &'static str) -> Self {
        Self {
            name,
            desc,
            prototype: true,
        }
    }
}

/// Section definition for help output
struct HelpSection {
    name: &'static str,
    commands: &'static [HelpCommand],
    advanced_start: Option<usize>, // Index where "advanced" subsection starts
}

/// Print a command row: "  command      description"
fn print_command_row(term: &Term, styles: &HelpStyles, name: &str, desc: &str) {
    let styled_name = styles.command.apply_to(name);
    let styled_desc = styles.desc.apply_to(desc);
    let _ = term.write_line(&format!("  {styled_name:<20} {styled_desc}"));
}

/// Print a section header
fn print_section(term: &Term, styles: &HelpStyles, name: &str) {
    let _ = term.write_line(&styles.section.apply_to(name).to_string());
}

/// Print the examples/quick-start code block
fn print_examples_block(
    term: &Term,
    styles: &HelpStyles,
    examples: &[(&str, &str)],
    demo_mode: bool,
) {
    for (comment, command) in examples {
        // Skip demo examples in non-demo mode
        if !demo_mode
            && (command.contains("install")
                || command.contains("jupyter")
                || command.contains("build")
                || command.contains("deploy")
                || command.contains("model")
                || command.contains("search")
                || command.contains("download"))
        {
            continue;
        }
        let _ = term.write_line(&format!(
            "    {}",
            styles.dim.apply_to(format!("# {comment}"))
        ));
        let _ = term.write_line(&format!("    {command}"));
    }
}

// Common commands for concise help (demo mode)
const COMMON_COMMANDS: &[HelpCommand] = &[
    HelpCommand::proto(
        "install",
        "Install a tool -- conda, pixi, uv, pip, Jupyter, Desktop",
    ),
    HelpCommand::proto("jupyter", "Launch a pre-configured Jupyter instance"),
    HelpCommand::proto("model", "Discover, pull, and manage AI models"),
    HelpCommand::proto("build", "Build containers, packages, or PyScript apps"),
    HelpCommand::proto(
        "deploy",
        "Deploy to SageMaker, Snowflake, Databricks, and more",
    ),
];

// All sections for full help
const HELP_SECTIONS: &[HelpSection] = &[
    HelpSection {
        name: "TOOLCHAIN",
        commands: &[
            HelpCommand::proto(
                "install",
                "Install a tool -- conda, pixi, uv, pip, Jupyter, or Anaconda Desktop",
            ),
            HelpCommand::proto("update", "Update one or all installed tools"),
            HelpCommand::proto("configure", "Apply or change settings for your tools"),
            HelpCommand::proto("uninstall", "Remove an installed tool"),
            HelpCommand::proto("tools", "List what's installed and at which version"),
            HelpCommand::real("config", "Show or edit current ana configuration"),
            HelpCommand::real("self", "Manage the ana installation itself"),
        ],
        advanced_start: None,
    },
    HelpSection {
        name: "DEVELOP",
        commands: &[
            HelpCommand::proto("jupyter", "Launch a pre-configured Jupyter instance"),
            HelpCommand::proto(
                "model",
                "Discover, pull, and manage AI models from Anaconda's vetted catalog",
            ),
            HelpCommand::proto(
                "build",
                "Build containers, packages, or PyScript apps -- includes signing and CVE scanning",
            ),
            HelpCommand::proto(
                "deploy",
                "Deploy to SageMaker, Snowflake, Databricks, Vertex AI, or Azure ML",
            ),
        ],
        advanced_start: None,
    },
    HelpSection {
        name: "PACKAGES",
        commands: &[
            HelpCommand::proto("search", "Search for packages in your Anaconda repository"),
            HelpCommand::proto("show", "Show information about a package or object"),
            HelpCommand::proto(
                "download",
                "Download packages from your Anaconda repository",
            ),
            HelpCommand::proto("upload", "Upload packages to your Anaconda repository"),
            HelpCommand::proto("remove", "Remove a package or object from your repository"),
            // Advanced subsection starts here
            HelpCommand::proto("copy", "Copy packages from one account to another"),
            HelpCommand::proto("move", "Move packages between labels"),
            HelpCommand::proto("label", "Manage your Anaconda repository channels"),
            HelpCommand::proto("package", "Anaconda repository package utilities"),
            HelpCommand::proto(
                "repo",
                "Repository operations: channel, copy, mirror, move, search, upload",
            ),
        ],
        advanced_start: Some(5),
    },
    HelpSection {
        name: "ACCOUNT",
        commands: &[
            HelpCommand::real(
                "login / logout",
                "Connect or disconnect from the Anaconda platform",
            ),
            HelpCommand::real("whoami", "Show your current logged-in account"),
            HelpCommand::real("auth", "Manage your Anaconda authentication"),
            HelpCommand::proto("org", "Interact with anaconda.org"),
            HelpCommand::proto("sites", "Manage your Anaconda site configuration"),
            HelpCommand::proto("token", "Manage your Anaconda repo tokens"),
        ],
        advanced_start: None,
    },
];

/// Concise help shown when running `ana` with no arguments
pub fn print_concise_help() {
    let styles = HelpStyles::new();
    let term = Term::stdout();
    let demo_mode = is_demo_mode();

    // Header
    let _ = term.write_line(&format!("ana {VERSION}"));
    let tagline = if demo_mode {
        "Manage your toolchain, AI models, builds, and deployments from one place."
    } else {
        "The Anaconda command-line interface."
    };
    let _ = term.write_line(&styles.desc.apply_to(tagline).to_string());
    let _ = term.write_line("");

    if demo_mode {
        // Quick start section (demo only)
        print_section(&term, &styles, "QUICK START");
        print_examples_block(
            &term,
            &styles,
            &[
                ("set up your full toolchain", "ana install all"),
                ("launch jupyter", "ana jupyter"),
                (
                    "build and deploy your app",
                    "ana build && ana deploy --target snowflake",
                ),
            ],
            demo_mode,
        );
        let _ = term.write_line("");

        // Common commands section (demo only)
        print_section(&term, &styles, "COMMON COMMANDS");
        for cmd in COMMON_COMMANDS {
            print_command_row(&term, &styles, cmd.name, cmd.desc);
        }
        let _ = term.write_line("");
    } else {
        // Real commands only
        print_section(&term, &styles, "COMMANDS");
        print_command_row(&term, &styles, "login", "Log in to Anaconda");
        print_command_row(&term, &styles, "logout", "Log out from Anaconda");
        print_command_row(
            &term,
            &styles,
            "whoami",
            "Display information about the logged-in user",
        );
        print_command_row(&term, &styles, "auth", "Authentication commands");
        print_command_row(&term, &styles, "config", "Show current configuration");
        print_command_row(&term, &styles, "self", "Manage the ana installation");
        let _ = term.write_line("");

        print_section(&term, &styles, "OPTIONS");
        let _ = term.write_line(&format!(
            "  {}  {}",
            styles
                .command
                .apply_to("-V, --version".to_string() + &" ".repeat(7)),
            styles.desc.apply_to("Show the ana version and exit")
        ));
        let _ = term.write_line(&format!(
            "  {}  {}",
            styles
                .command
                .apply_to("-h, --help".to_string() + &" ".repeat(10)),
            styles.desc.apply_to("Show this message and exit")
        ));
        let _ = term.write_line("");
    }

    // Footer
    let run_help = format!(
        "Run {} for the full command list",
        styles.command.apply_to("ana --help")
    );
    let docs_link = styles.section.apply_to("-> docs.anaconda.com");
    let _ = term.write_line(&styles.dim.apply_to(run_help).to_string());
    let _ = term.write_line(&docs_link.to_string());
}

/// Full help shown when running `ana --help`
pub fn print_full_help(subcommands: HashMap<String, String>) {
    let styles = HelpStyles::new();
    let term = Term::stdout();
    let demo_mode = is_demo_mode();

    // Header
    let _ = term.write_line(&format!("ana {VERSION}"));
    let tagline = if demo_mode {
        "Manage your toolchain, AI models, builds, and deployments from one place."
    } else {
        "The Anaconda command-line interface."
    };
    let _ = term.write_line(&styles.desc.apply_to(tagline).to_string());
    let _ = term.write_line("");

    // Examples section (demo mode only)
    if demo_mode {
        print_section(&term, &styles, "EXAMPLES");
        print_examples_block(
            &term,
            &styles,
            &[
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
            ],
            demo_mode,
        );
        let _ = term.write_line("");
    }

    // Print each section
    for section in HELP_SECTIONS {
        // Filter commands based on demo mode
        let visible_commands: Vec<_> = section
            .commands
            .iter()
            .enumerate()
            .filter(|(_, cmd)| demo_mode || !cmd.prototype)
            .collect();

        // Skip empty sections
        if visible_commands.is_empty() {
            continue;
        }

        print_section(&term, &styles, section.name);

        for (idx, cmd) in visible_commands {
            // Print "advanced" label if we've reached that point
            if let Some(adv_start) = section.advanced_start {
                if idx == adv_start && demo_mode {
                    let _ = term.write_line(&styles.dim.apply_to("  advanced").to_string());
                }
            }

            // Get description from clap if available, otherwise use fallback
            let base_name = cmd.name.split(" / ").next().unwrap_or(cmd.name);
            let desc = subcommands
                .get(base_name)
                .map(|s| s.as_str())
                .unwrap_or(cmd.desc);
            print_command_row(&term, &styles, cmd.name, desc);
        }
        let _ = term.write_line("");
    }

    // Global options section
    print_section(
        &term,
        &styles,
        if demo_mode {
            "GLOBAL OPTIONS"
        } else {
            "OPTIONS"
        },
    );
    if demo_mode {
        let _ = term.write_line(&format!(
            "  {}  {}",
            styles
                .command
                .apply_to("--at <site>".to_string() + &" ".repeat(11)),
            styles
                .desc
                .apply_to("Select configured site by name or domain")
        ));
        let _ = term.write_line(&format!(
            "  {}  {}",
            styles
                .command
                .apply_to("-v, --verbose".to_string() + &" ".repeat(7)),
            styles
                .desc
                .apply_to("Print debug information to the console")
        ));
    }
    let _ = term.write_line(&format!(
        "  {}  {}",
        styles
            .command
            .apply_to("-V, --version".to_string() + &" ".repeat(7)),
        styles.desc.apply_to("Show the ana version and exit")
    ));
    let _ = term.write_line(&format!(
        "  {}  {}",
        styles
            .command
            .apply_to("-h, --help".to_string() + &" ".repeat(10)),
        styles.desc.apply_to("Show this message and exit")
    ));
    let _ = term.write_line("");

    // Typo hint box (demo mode only)
    if demo_mode {
        let _ = term.write_line(
            &styles
                .desc
                .apply_to("Typo? ana will suggest the closest command.")
                .to_string(),
        );
        let _ = term.write_line(&format!("    {}", styles.dim.apply_to("# example")));
        let _ = term.write_line(&format!(
            "    {} {}",
            styles.error.apply_to("error:"),
            styles.desc.apply_to("unknown command \"instal\"")
        ));
        let _ = term.write_line(&format!(
            "    {} {}",
            styles.warning.apply_to("tip:"),
            styles.desc.apply_to(format!(
                "did you mean {}?",
                styles.command.apply_to("install")
            ))
        ));
        let _ = term.write_line("");
    }

    // Footer
    let run_cmd = format!(
        "Run {} or {} for more",
        styles.command.apply_to("ana <command> --help"),
        styles.command.apply_to("ana help <command>")
    );
    let _ = term.write_line(&styles.dim.apply_to(run_cmd).to_string());
    let _ = term.write_line(&styles.section.apply_to("-> docs.anaconda.com").to_string());
}

/// Help for `ana self` subcommand
pub fn print_self_help() {
    let styles = HelpStyles::new();
    let term = Term::stdout();

    let _ = term.write_line("Manage the ana installation");
    let _ = term.write_line("");
    let _ = term.write_line(
        &styles
            .dim
            .apply_to("Usage: ana self <command> [options]")
            .to_string(),
    );
    let _ = term.write_line("");

    print_section(&term, &styles, "COMMANDS");
    print_command_row(&term, &styles, "update", "Update ana to the latest version");
}

/// Help for `ana auth` subcommand
pub fn print_auth_help() {
    let styles = HelpStyles::new();
    let term = Term::stdout();

    let _ = term.write_line("Authentication commands");
    let _ = term.write_line("");
    let _ = term.write_line(
        &styles
            .dim
            .apply_to("Usage: ana auth <command> [options]")
            .to_string(),
    );
    let _ = term.write_line("");

    print_section(&term, &styles, "COMMANDS");
    print_command_row(
        &term,
        &styles,
        "api-key",
        "Display the API key for the logged-in user",
    );
    print_command_row(&term, &styles, "login", "Log in to Anaconda");
    print_command_row(&term, &styles, "logout", "Log out from Anaconda");
    print_command_row(
        &term,
        &styles,
        "whoami",
        "Display information about the logged-in user",
    );
}
