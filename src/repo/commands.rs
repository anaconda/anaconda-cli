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
    Channels {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Upload packages to your repository
    Upload {
        /// Target channel in format org/channel
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
            RepoCommands::Channels { args } => {
                let mut cmd_args = vec!["channels".to_string()];
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channels_create_builds_args() {
        let cmd = RepoCommands::Channels {
            args: vec![
                "create".to_string(),
                "--private".to_string(),
                "org/channel".to_string(),
            ],
        };
        match cmd.into_action() {
            RepoAction::Run(args) => {
                assert_eq!(args, vec!["channels", "create", "--private", "org/channel"]);
            }
            _ => panic!("Expected Run action"),
        }
    }

    #[test]
    fn test_channels_remove_builds_args() {
        let cmd = RepoCommands::Channels {
            args: vec!["remove".to_string(), "org/channel".to_string()],
        };
        match cmd.into_action() {
            RepoAction::Run(args) => {
                assert_eq!(args, vec!["channels", "remove", "org/channel"]);
            }
            _ => panic!("Expected Run action"),
        }
    }

    #[test]
    fn test_upload_with_channel_builds_args() {
        let cmd = RepoCommands::Upload {
            channel: Some("org/channel".to_string()),
            no_progress: false,
            args: vec!["package.tar.gz".to_string()],
        };
        match cmd.into_action() {
            RepoAction::Run(args) => {
                assert_eq!(
                    args,
                    vec!["upload", "--channel", "org/channel", "package.tar.gz"]
                );
            }
            _ => panic!("Expected Run action"),
        }
    }

    #[test]
    fn test_upload_with_no_progress_builds_args() {
        let cmd = RepoCommands::Upload {
            channel: Some("org/channel".to_string()),
            no_progress: true,
            args: vec!["package.tar.gz".to_string()],
        };
        match cmd.into_action() {
            RepoAction::Run(args) => {
                assert_eq!(
                    args,
                    vec![
                        "upload",
                        "--channel",
                        "org/channel",
                        "--no-progress",
                        "package.tar.gz"
                    ]
                );
            }
            _ => panic!("Expected Run action"),
        }
    }

    #[test]
    fn test_upload_without_channel_builds_args() {
        let cmd = RepoCommands::Upload {
            channel: None,
            no_progress: false,
            args: vec!["package.tar.gz".to_string()],
        };
        match cmd.into_action() {
            RepoAction::Run(args) => {
                assert_eq!(args, vec!["upload", "package.tar.gz"]);
            }
            _ => panic!("Expected Run action"),
        }
    }
}
