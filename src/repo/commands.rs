use clap::Subcommand;

/// Result of resolving a Repo command.
pub enum RepoAction {
    /// Show help for a subcommand path
    #[allow(dead_code)]
    ShowHelp(String),
    /// Run the anaconda repo command with args
    Run(Vec<String>),
}

#[derive(Subcommand)]
pub enum RepoCommands {
    /// Manage your Anaconda repository channels
    #[command(trailing_var_arg = true)]
    Channel {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Download artifacts
    #[command(trailing_var_arg = true)]
    Download {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Remove an object from your Package Security Manager repository
    #[command(trailing_var_arg = true)]
    Remove {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Upload packages to your repository
    #[command(trailing_var_arg = true)]
    Upload {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

impl RepoCommands {
    /// Convert the command into an action.
    pub fn into_action(self) -> RepoAction {
        match self {
            RepoCommands::Channel { args } => {
                let mut cmd_args = vec!["channel".to_string()];
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
            RepoCommands::Download { args } => {
                let mut cmd_args = vec!["download".to_string()];
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
            RepoCommands::Remove { args } => {
                let mut cmd_args = vec!["remove".to_string()];
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
            RepoCommands::Upload { args } => {
                let mut cmd_args = vec!["upload".to_string()];
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
        }
    }
}
