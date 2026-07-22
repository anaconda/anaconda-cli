use clap::Subcommand;

/// Result of resolving a Code command.
pub enum CodeAction {
    /// Show help for a subcommand path
    #[allow(dead_code)]
    ShowHelp(String),
    /// Run the kilo binary with args
    Run(Vec<String>),
}

#[derive(Subcommand)]
pub enum CodeCommands {
    /// Launch Kilo Code
    #[command(trailing_var_arg = true)]
    Launch {
        /// Arguments to pass to kilo
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

impl CodeCommands {
    /// Convert the command into an action.
    pub fn into_action(self) -> CodeAction {
        match self {
            CodeCommands::Launch { args } => CodeAction::Run(args),
        }
    }
}
