/// Section definition for help output
pub(super) struct HelpSection {
    pub(super) name: &'static str,
    pub(super) commands: &'static [&'static str],
}

/// Help sections with commands (only real, implemented commands)
/// TODO(mattkram): It would be more ergonomic to define sections alongside each
///                 subcommand but the implementation of that is complicated. For
///                 now, assuming YAGNI and asserting inclusing via unit tests.
pub(super) const HELP_SECTIONS: &[HelpSection] = &[
    HelpSection {
        name: "PROJECT",
        commands: &["prepare", "run", "shell"],
    },
    HelpSection {
        name: "ACCOUNT",
        commands: &["login", "logout", "whoami", "auth"],
    },
    HelpSection {
        name: "PACKAGES",
        commands: &["org"],
    },
    HelpSection {
        name: "TOOLCHAIN",
        commands: &["bootstrap", "config", "self"],
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
        .flat_map(|s| s.commands.iter().copied())
        .collect()
}
