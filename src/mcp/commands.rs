use clap::{Subcommand, ValueEnum};
use std::fmt;

/// Supported AI clients for MCP configuration.
#[derive(Clone, ValueEnum)]
pub enum McpClient {
    #[value(name = "claude-code")]
    ClaudeCode,
    #[value(name = "claude-desktop")]
    ClaudeDesktop,
    #[value(name = "cursor")]
    Cursor,
    #[value(name = "opencode")]
    Opencode,
    #[value(name = "vscode")]
    Vscode,
    #[value(name = "windsurf")]
    Windsurf,
}

impl fmt::Display for McpClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            McpClient::ClaudeCode => write!(f, "claude-code"),
            McpClient::ClaudeDesktop => write!(f, "claude-desktop"),
            McpClient::Cursor => write!(f, "cursor"),
            McpClient::Opencode => write!(f, "opencode"),
            McpClient::Vscode => write!(f, "vscode"),
            McpClient::Windsurf => write!(f, "windsurf"),
        }
    }
}

/// Scope for MCP client configuration.
#[derive(Clone, ValueEnum)]
pub enum McpScope {
    #[value(name = "global")]
    Global,
    #[value(name = "project")]
    Project,
}

impl fmt::Display for McpScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            McpScope::Global => write!(f, "global"),
            McpScope::Project => write!(f, "project"),
        }
    }
}

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
    Serve {
        /// Path to mcp_compose.toml file
        #[arg(short = 'c', long = "config")]
        config: Option<String>,

        /// Host to bind to
        #[arg(long, default_value = "0.0.0.0")]
        host: String,

        /// Port to bind to
        #[arg(long, default_value_t = 8000)]
        port: u32,

        /// Delay in seconds added before serving
        #[arg(long, default_value_t = 0)]
        delay: u32,

        /// Additional arguments passed to the serve command
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// List supported AI clients and their configuration options
    Clients {
        /// Project directory to check for project-scoped installs
        #[arg(long)]
        project_dir: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Configure AI clients to use Anaconda MCP
    Setup {
        /// Client to configure (can be repeated)
        #[arg(long, value_enum)]
        client: Vec<McpClient>,

        /// Name for the MCP server entry
        #[arg(short = 'n', long, default_value = "anaconda-mcp")]
        name: String,

        /// Install globally or in the current project
        #[arg(long, value_enum, default_value = "global")]
        scope: McpScope,

        /// Project directory for --scope project
        #[arg(long)]
        project_dir: Option<String>,

        /// Don't create a backup of the existing config file
        #[arg(long)]
        no_backup: bool,

        /// Overwrite existing server configuration if present
        #[arg(short = 'f', long)]
        force: bool,

        /// Output result as JSON
        #[arg(long)]
        json: bool,
    },

    /// Remove Anaconda MCP from AI client configurations
    Remove {
        /// Client to remove from (can be repeated)
        #[arg(long, value_enum)]
        client: Vec<McpClient>,

        /// Name of the MCP server entry to remove
        #[arg(short = 'n', long, default_value = "anaconda-mcp")]
        name: String,

        /// Remove from global or project config
        #[arg(long, value_enum, default_value = "global")]
        scope: McpScope,

        /// Project directory for --scope project
        #[arg(long)]
        project_dir: Option<String>,

        /// Don't create a backup of the existing config file
        #[arg(long)]
        no_backup: bool,

        /// Output result as JSON
        #[arg(long)]
        json: bool,
    },

    /// Manage Terms of Service acceptance
    #[command(subcommand_required = false, arg_required_else_help = false)]
    Terms {
        #[command(subcommand)]
        command: Option<McpTermsCommands>,

        /// Output in JSON format (when no subcommand is given)
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum McpTermsCommands {
    /// Check whether the Terms of Service have been accepted
    Status {
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// Accept the Terms of Service
    Accept {
        /// Output in JSON format
        #[arg(long)]
        json: bool,

        /// Consent to be contacted for feedback
        #[arg(long)]
        consent: bool,
    },
}

impl McpCommands {
    /// Convert the command into an action.
    pub fn into_action(self) -> McpAction {
        match self {
            McpCommands::Serve {
                config,
                host,
                port,
                delay,
                args: extra_args,
            } => {
                let mut args = vec!["serve".to_string()];
                if let Some(c) = config {
                    args.push("-c".to_string());
                    args.push(c);
                }
                args.push("--host".to_string());
                args.push(host);
                args.push("--port".to_string());
                args.push(port.to_string());
                args.push("--delay".to_string());
                args.push(delay.to_string());
                args.extend(extra_args);
                McpAction::Run(args)
            }
            McpCommands::Clients { project_dir, json } => {
                let mut args = vec!["clients".to_string()];
                if let Some(dir) = project_dir {
                    args.push("--project-dir".to_string());
                    args.push(dir);
                }
                if json {
                    args.push("--json".to_string());
                }
                McpAction::Run(args)
            }
            McpCommands::Setup {
                client,
                name,
                scope,
                project_dir,
                no_backup,
                force,
                json,
            } => {
                let mut args = vec!["setup".to_string()];
                for c in client {
                    args.push("--client".to_string());
                    args.push(c.to_string());
                }
                args.push("--name".to_string());
                args.push(name);
                args.push("--scope".to_string());
                args.push(scope.to_string());
                if let Some(dir) = project_dir {
                    args.push("--project-dir".to_string());
                    args.push(dir);
                }
                if no_backup {
                    args.push("--no-backup".to_string());
                }
                if force {
                    args.push("--force".to_string());
                }
                if json {
                    args.push("--json".to_string());
                }
                McpAction::Run(args)
            }
            McpCommands::Remove {
                client,
                name,
                scope,
                project_dir,
                no_backup,
                json,
            } => {
                let mut args = vec!["remove".to_string()];
                for c in client {
                    args.push("--client".to_string());
                    args.push(c.to_string());
                }
                args.push("--name".to_string());
                args.push(name);
                args.push("--scope".to_string());
                args.push(scope.to_string());
                if let Some(dir) = project_dir {
                    args.push("--project-dir".to_string());
                    args.push(dir);
                }
                if no_backup {
                    args.push("--no-backup".to_string());
                }
                if json {
                    args.push("--json".to_string());
                }
                McpAction::Run(args)
            }
            McpCommands::Terms { command, json } => match command {
                None => {
                    let mut args = vec!["terms".to_string()];
                    if json {
                        args.push("--json".to_string());
                    }
                    McpAction::Run(args)
                }
                Some(terms_cmd) => terms_cmd.into_action(),
            },
        }
    }
}

impl McpTermsCommands {
    /// Convert the terms command into an action.
    pub fn into_action(self) -> McpAction {
        match self {
            McpTermsCommands::Status { json } => {
                let mut args = vec!["terms".to_string(), "status".to_string()];
                if json {
                    args.push("--json".to_string());
                }
                McpAction::Run(args)
            }
            McpTermsCommands::Accept { json, consent } => {
                let mut args = vec!["terms".to_string(), "accept".to_string()];
                if json {
                    args.push("--json".to_string());
                }
                if consent {
                    args.push("--consent".to_string());
                }
                McpAction::Run(args)
            }
        }
    }
}
