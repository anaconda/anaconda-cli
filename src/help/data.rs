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
        name: "TOOLCHAIN",
        commands: &[
            "tool",
            "bootstrap",
            "feature",
            // TODO(mattkram): Hiding config from help until we fully implement CRUD
            // "config",
            "self",
        ],
    },
    // TODO(mattkram): Removed PACKAGES section from help until we can comprehensively
    //                 define the wrappers.
    // HelpSection {
    //     name: "PACKAGES",
    //     commands: &["org"],
    // },
    HelpSection {
        name: "ACCOUNT",
        commands: &["login", "logout", "whoami", "auth"],
    },
    HelpSection {
        name: "API",
        commands: &["api"],
    },
];

/// Examples for the help output (using real commands)
pub(super) const HELP_EXAMPLES: &[(&str, &str)] = &[
    ("Log into your Anaconda account", "ana login"),
    ("Install a tool", "ana tool install pixi"),
    ("Manage your ana version", "ana self update"),
];

/// Subcommand examples keyed by command path
/// Path format matches the space-separated command hierarchy (e.g., "self update")
const SUBCOMMAND_EXAMPLES: &[(&str, &[(&str, &str)])] = &[(
    "self update",
    &[
        ("Update to the latest version", "ana self update"),
        ("Update to a specific version", "ana self update v0.0.8"),
    ],
)];

/// Get examples for a specific subcommand by its path
pub(super) fn get_subcommand_examples(path: &str) -> Option<&'static [(&'static str, &'static str)]> {
    SUBCOMMAND_EXAMPLES
        .iter()
        .find(|(p, _)| *p == path)
        .map(|(_, examples)| *examples)
}

/// Get all command names defined in help sections (for testing)
#[cfg(test)]
pub fn get_all_section_commands() -> Vec<&'static str> {
    HELP_SECTIONS
        .iter()
        .flat_map(|s| s.commands.iter().copied())
        .collect()
}
