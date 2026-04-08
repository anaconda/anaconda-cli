/// Command definition for help output
pub(super) struct HelpCommand {
    pub(super) name: &'static str,
}

impl HelpCommand {
    const fn new(name: &'static str) -> Self {
        Self { name }
    }
}

/// Section definition for help output
pub(super) struct HelpSection {
    pub(super) name: &'static str,
    pub(super) commands: &'static [HelpCommand],
}

/// Help sections with commands (only real, implemented commands)
pub(super) const HELP_SECTIONS: &[HelpSection] = &[
    HelpSection {
        name: "ACCOUNT",
        commands: &[
            HelpCommand::new("login"),
            HelpCommand::new("logout"),
            HelpCommand::new("whoami"),
            HelpCommand::new("auth"),
        ],
    },
    HelpSection {
        name: "PACKAGES",
        commands: &[HelpCommand::new("org")],
    },
    HelpSection {
        name: "TOOLCHAIN",
        commands: &[
            HelpCommand::new("bootstrap"),
            HelpCommand::new("config"),
            HelpCommand::new("self"),
        ],
    },
];

/// Examples for the help output (using real commands)
pub(super) const HELP_EXAMPLES: &[(&str, &str)] = &[
    ("Log into your Anaconda account", "ana login"),
    ("Update ana to the latest version", "ana self update"),
];

/// Get all command names defined in help sections (for testing)
#[cfg(test)]
pub fn get_all_section_commands() -> Vec<&'static str> {
    HELP_SECTIONS
        .iter()
        .flat_map(|s| s.commands.iter().map(|c| c.name))
        .collect()
}
