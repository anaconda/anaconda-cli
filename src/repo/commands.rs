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
    /// Package Security Manager admin settings
    #[command(trailing_var_arg = true)]
    Admin {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Manage your Anaconda repository channels
    #[command(trailing_var_arg = true)]
    Channel {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Copy packages from one channel to another
    #[command(trailing_var_arg = true)]
    Copy {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Access Anaconda Repository cves
    #[command(trailing_var_arg = true)]
    Cves {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Download artifacts
    #[command(trailing_var_arg = true)]
    Download {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Manage your Anaconda repository mirrors
    #[command(trailing_var_arg = true)]
    Mirror {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Move packages from one channel to another
    #[command(trailing_var_arg = true)]
    Move {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Policies for filtering
    #[command(trailing_var_arg = true)]
    Policy {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Remove an object from your Package Security Manager repository
    #[command(trailing_var_arg = true)]
    Remove {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Download reports
    #[command(trailing_var_arg = true)]
    Report {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Get SBOM files
    #[command(trailing_var_arg = true)]
    Sbom {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Search in your Anaconda repository
    #[command(trailing_var_arg = true)]
    Search {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Manage service accounts
    #[command(trailing_var_arg = true, name = "service-accounts")]
    ServiceAccounts {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Return information about system
    #[command(trailing_var_arg = true)]
    System {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Upload packages to your repository
    #[command(trailing_var_arg = true)]
    Upload {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Return information about logged user
    #[command(trailing_var_arg = true)]
    Whoami {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Configure conda to use Anaconda Platform
    #[command(trailing_var_arg = true)]
    Wizard {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

impl RepoCommands {
    /// Convert the command into an action.
    pub fn into_action(self) -> RepoAction {
        match self {
            RepoCommands::Admin { args } => {
                let mut cmd_args = vec!["admin".to_string()];
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
            RepoCommands::Channel { args } => {
                let mut cmd_args = vec!["channel".to_string()];
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
            RepoCommands::Copy { args } => {
                let mut cmd_args = vec!["copy".to_string()];
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
            RepoCommands::Cves { args } => {
                let mut cmd_args = vec!["cves".to_string()];
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
            RepoCommands::Download { args } => {
                let mut cmd_args = vec!["download".to_string()];
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
            RepoCommands::Mirror { args } => {
                let mut cmd_args = vec!["mirror".to_string()];
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
            RepoCommands::Move { args } => {
                let mut cmd_args = vec!["move".to_string()];
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
            RepoCommands::Policy { args } => {
                let mut cmd_args = vec!["policy".to_string()];
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
            RepoCommands::Remove { args } => {
                let mut cmd_args = vec!["remove".to_string()];
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
            RepoCommands::Report { args } => {
                let mut cmd_args = vec!["report".to_string()];
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
            RepoCommands::Sbom { args } => {
                let mut cmd_args = vec!["sbom".to_string()];
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
            RepoCommands::Search { args } => {
                let mut cmd_args = vec!["search".to_string()];
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
            RepoCommands::ServiceAccounts { args } => {
                let mut cmd_args = vec!["service-accounts".to_string()];
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
            RepoCommands::System { args } => {
                let mut cmd_args = vec!["system".to_string()];
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
            RepoCommands::Upload { args } => {
                let mut cmd_args = vec!["upload".to_string()];
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
            RepoCommands::Whoami { args } => {
                let mut cmd_args = vec!["whoami".to_string()];
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
            RepoCommands::Wizard { args } => {
                let mut cmd_args = vec!["wizard".to_string()];
                cmd_args.extend(args);
                RepoAction::Run(cmd_args)
            }
        }
    }
}
