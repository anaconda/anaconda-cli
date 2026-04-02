/// Check if demo mode is enabled via ANA_DEMO=true
pub(super) fn is_demo_mode() -> bool {
    std::env::var("ANA_DEMO")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
}

/// Command definition for help output
pub(super) struct HelpCommand {
    pub(super) name: &'static str,
    pub(super) desc: &'static str,
    pub(super) prototype: bool,
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
pub(super) struct HelpSection {
    pub(super) name: &'static str,
    pub(super) commands: &'static [HelpCommand],
}

/// Help sections with commands
/// This is where we map specific commands to sections. The HelpCommand::proto()
/// function is only used for ANA_DEMO=true purposes, while the HelpCommand::real()
/// must map to actual implementations.
///
/// TODO(mattkram): We still need to find a better way to create this without a
///                 hard-coded mapping.
pub(super) const HELP_SECTIONS: &[HelpSection] = &[
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
            HelpCommand::proto("copy", "Copy packages from one account to another"),
            HelpCommand::proto("move", "Move packages between labels"),
            HelpCommand::proto("label", "Manage your Anaconda repository channels"),
            HelpCommand::proto("package", "Anaconda repository package utilities"),
            HelpCommand::proto(
                "repo",
                "Repository operations: channel, copy, mirror, move, search, upload",
            ),
        ],
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
    },
];
