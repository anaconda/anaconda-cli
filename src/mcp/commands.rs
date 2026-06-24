use clap::Subcommand;

/// Result of resolving an MCP command.
pub enum McpAction {
    /// Show help for a subcommand path
    #[allow(dead_code)]
    ShowHelp(String),
    /// Run the anaconda-mcp command with args
    Run(Vec<String>),
}

#[derive(Subcommand)]
pub enum McpCommands {
    /// Start MCP servers from configuration file
    #[command(trailing_var_arg = true)]
    Serve {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// List supported AI clients and their configuration options
    #[command(trailing_var_arg = true)]
    Clients {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Configure AI clients to use Anaconda MCP
    #[command(trailing_var_arg = true)]
    Setup {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Remove Anaconda MCP from AI client configurations
    #[command(trailing_var_arg = true)]
    Remove {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Manage Terms of Service acceptance
    #[command(trailing_var_arg = true)]
    Terms {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

impl McpCommands {
    /// Convert the command into an action.
    pub fn into_action(self) -> McpAction {
        match self {
            McpCommands::Serve { args } => {
                let mut cmd_args = vec!["serve".to_string()];
                cmd_args.extend(args);
                McpAction::Run(cmd_args)
            }
            McpCommands::Clients { args } => {
                let mut cmd_args = vec!["clients".to_string()];
                cmd_args.extend(args);
                McpAction::Run(cmd_args)
            }
            McpCommands::Setup { args } => {
                let mut cmd_args = vec!["setup".to_string()];
                cmd_args.extend(args);
                McpAction::Run(cmd_args)
            }
            McpCommands::Remove { args } => {
                let mut cmd_args = vec!["remove".to_string()];
                cmd_args.extend(args);
                McpAction::Run(cmd_args)
            }
            McpCommands::Terms { args } => {
                let mut cmd_args = vec!["terms".to_string()];
                cmd_args.extend(args);
                McpAction::Run(cmd_args)
            }
        }
    }
}
