use clap::{Subcommand, ValueEnum};
use std::fmt;

/// Supported AI clients for MCP configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum McpClient {
    #[value(name = "claude-code")]
    ClaudeCode,
    #[value(name = "codex")]
    Codex,
    #[value(name = "cursor")]
    Cursor,
    #[value(name = "kilo")]
    Kilo,
    #[value(name = "opencode")]
    Opencode,
    #[value(name = "vscode")]
    Vscode,
    #[value(name = "windsurf")]
    Windsurf,
}

impl McpClient {
    pub fn name(&self) -> &'static str {
        match self {
            McpClient::ClaudeCode => "claude-code",
            McpClient::Codex => "codex",
            McpClient::Cursor => "cursor",
            McpClient::Kilo => "kilo",
            McpClient::Opencode => "opencode",
            McpClient::Vscode => "vscode",
            McpClient::Windsurf => "windsurf",
        }
    }
}

impl fmt::Display for McpClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Subcommand)]
pub enum McpCommands {
    /// List supported AI clients and their configuration status
    Clients {
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

        /// Don't create a backup of the existing config file
        #[arg(long)]
        no_backup: bool,

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
