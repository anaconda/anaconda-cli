/// Section definition for help output
pub(super) struct HelpSection {
    pub(super) name: &'static str,
    pub(super) commands: &'static [&'static str],
}

/// Example definition for help output
pub(super) struct HelpExample {
    pub(super) desc: String,
    pub(super) command: String,
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
            "api",
            "ob",
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
        name: "AI",
        commands: &["mcp"],
    },
];

/// Examples for the main help output
pub(super) fn get_main_examples() -> Vec<HelpExample> {
    vec![
        HelpExample {
            desc: "Log into your Anaconda account".to_string(),
            command: "ana login".to_string(),
        },
        HelpExample {
            desc: "Manage your ana version".to_string(),
            command: "ana self update".to_string(),
        },
        HelpExample {
            desc: "Provide feedback or report a bug".to_string(),
            command: "ana self feedback".to_string(),
        },
    ]
}

/// Get examples for a specific subcommand by its path
pub(super) fn get_subcommand_examples(path: &str) -> Option<Vec<HelpExample>> {
    match path {
        "self update" => Some(vec![
            HelpExample {
                desc: "Update to the latest version".to_string(),
                command: "ana self update".to_string(),
            },
            HelpExample {
                desc: "Update to a specific version".to_string(),
                command: format!("ana self update v{}", crate::VERSION),
            },
        ]),
        _ => None,
    }
}

/// Get all command names defined in help sections (for testing)
#[cfg(test)]
pub fn get_all_section_commands() -> Vec<&'static str> {
    HELP_SECTIONS
        .iter()
        .flat_map(|s| s.commands.iter().copied())
        .collect()
}
