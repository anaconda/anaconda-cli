use clap::Subcommand;

/// Result of resolving a Channel command.
pub enum ChannelAction {
    /// Show help for a subcommand path
    #[allow(dead_code)]
    ShowHelp(String),
    /// Run the anaconda channel command with args
    Run(Vec<String>),
}

#[derive(Subcommand)]
pub enum ChannelSubcommands {
    /// Create a new channel
    Create {
        /// Channel name in format org/channel
        #[arg(required_unless_present = "help", default_value = "")]
        channel: String,

        /// Create a private channel
        #[arg(long)]
        private: bool,

        /// Create a public channel
        #[arg(long)]
        public: bool,

        /// Create an authenticated channel
        #[arg(long)]
        authenticated: bool,
    },

    /// Remove a channel
    Remove {
        /// Channel in format org/channel
        #[arg(required_unless_present = "help", default_value = "")]
        channel: String,
    },

    /// Upload a package to a channel
    Upload {
        /// Target channel in format org/channel
        #[arg(short, long)]
        channel: Option<String>,

        /// Don't show upload progress
        #[arg(long)]
        no_progress: bool,

        /// Files to upload
        files: Vec<String>,
    },
}

impl ChannelSubcommands {
    /// Convert the command into an action.
    pub fn into_action(self) -> ChannelAction {
        match self {
            ChannelSubcommands::Create {
                channel,
                private,
                public,
                authenticated,
            } => {
                let mut cmd_args = vec!["create".to_string()];
                if private {
                    cmd_args.push("--private".to_string());
                }
                if public {
                    cmd_args.push("--public".to_string());
                }
                if authenticated {
                    cmd_args.push("--authenticated".to_string());
                }
                cmd_args.push(channel);
                ChannelAction::Run(cmd_args)
            }
            ChannelSubcommands::Remove { channel } => {
                ChannelAction::Run(vec!["remove".to_string(), channel])
            }
            ChannelSubcommands::Upload {
                channel,
                no_progress,
                files,
            } => {
                let mut cmd_args = vec!["upload".to_string()];
                if let Some(c) = channel {
                    cmd_args.push("--channel".to_string());
                    cmd_args.push(c);
                }
                if no_progress {
                    cmd_args.push("--no-progress".to_string());
                }
                cmd_args.extend(files);
                ChannelAction::Run(cmd_args)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_builds_args() {
        let cmd = ChannelSubcommands::Create {
            channel: "org/channel".to_string(),
            private: true,
            public: false,
            authenticated: false,
        };
        match cmd.into_action() {
            ChannelAction::Run(args) => {
                assert_eq!(args, vec!["create", "--private", "org/channel"]);
            }
            _ => panic!("Expected Run action"),
        }
    }

    #[test]
    fn test_remove_builds_args() {
        let cmd = ChannelSubcommands::Remove {
            channel: "org/channel".to_string(),
        };
        match cmd.into_action() {
            ChannelAction::Run(args) => {
                assert_eq!(args, vec!["remove", "org/channel"]);
            }
            _ => panic!("Expected Run action"),
        }
    }

    #[test]
    fn test_upload_with_channel_builds_args() {
        let cmd = ChannelSubcommands::Upload {
            channel: Some("org/channel".to_string()),
            no_progress: false,
            files: vec!["package.tar.gz".to_string()],
        };
        match cmd.into_action() {
            ChannelAction::Run(args) => {
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
        let cmd = ChannelSubcommands::Upload {
            channel: Some("org/channel".to_string()),
            no_progress: true,
            files: vec!["package.tar.gz".to_string()],
        };
        match cmd.into_action() {
            ChannelAction::Run(args) => {
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
        let cmd = ChannelSubcommands::Upload {
            channel: None,
            no_progress: false,
            files: vec!["package.tar.gz".to_string()],
        };
        match cmd.into_action() {
            ChannelAction::Run(args) => {
                assert_eq!(args, vec!["upload", "package.tar.gz"]);
            }
            _ => panic!("Expected Run action"),
        }
    }
}
