/// Check if demo mode is enabled via ANA_DEMO=true
pub fn is_demo_mode() -> bool {
    std::env::var("ANA_DEMO")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
}

/// Command definition for help output
pub struct HelpCommand {
    pub name: &'static str,
    pub desc: &'static str,
    pub prototype: bool,
}

impl HelpCommand {
    pub const fn real(name: &'static str, desc: &'static str) -> Self {
        Self {
            name,
            desc,
            prototype: false,
        }
    }

    pub const fn proto(name: &'static str, desc: &'static str) -> Self {
        Self {
            name,
            desc,
            prototype: true,
        }
    }
}

/// Section definition for help output
pub struct HelpSection {
    pub name: &'static str,
    pub commands: &'static [HelpCommand],
    pub advanced_start: Option<usize>,
}

/// Help sections with commands
pub const HELP_SECTIONS: &[HelpSection] = &[
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
