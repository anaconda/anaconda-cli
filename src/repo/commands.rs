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

    /// Manage your Anaconda repository channels (alias for channel)
    #[command(trailing_var_arg = true, hide = true)]
    Channels {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Remove an object from your Package Security Manager repository
    Remove {
        /// Do not prompt removal
        #[arg(short, long)]
        force: bool,

        /// specs
        args: Vec<String>,
    },

    /// Upload packages to your repository
    Upload {
        /// Target channel(s), repeatable
        #[arg(short, long)]
        channel: Option<String>,

        /// Don't show upload progress
        #[arg(long)]
        no_progress: bool,

        /// Files to upload
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
            RepoCommands::Channels { args } => {
                let mut cmd_args = vec!["channel".to_string()];
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
            RepoCommands::Remove { force, args } => {
                let mut cmd_args = vec!["remove".to_string()];
                if force {
                    cmd_args.push("--force".to_string());
                }
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
            RepoCommands::Upload {
                channel,
                no_progress,
                args,
            } => {
                let mut cmd_args = vec!["upload".to_string()];
                if let Some(c) = channel {
                    cmd_args.push("--channel".to_string());
                    cmd_args.push(c);
                }
                if no_progress {
                    cmd_args.push("--no-progress".to_string());
                }
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
        }
    }
}
