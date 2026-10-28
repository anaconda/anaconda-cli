use clap::Subcommand;

#[derive(Subcommand)]
pub enum MarketplaceCommands {
    /// Install a conda package as an MCP server and register it with kilo
    Install {
        /// Conda package providing the MCP server (match spec, e.g. mcp-server-fetch>=2026)
        #[arg(required_unless_present = "help", default_value = "")]
        package: String,

        /// Channel to solve against (can be repeated; default: main, conda-forge)
        #[arg(short = 'c', long)]
        channel: Vec<String>,

        /// Executable to run as the server, when the package installs several
        #[arg(long)]
        command: Option<String>,

        /// Name for the MCP server in kilo (default: the package name)
        #[arg(short = 'n', long)]
        name: Option<String>,

        /// Environment variable for the server (KEY=VALUE, can be repeated)
        #[arg(short = 'e', long)]
        env: Vec<String>,
    },

    /// List MCP servers installed from the marketplace
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Unregister an MCP server from kilo and uninstall it
    Remove {
        /// Name of the MCP server to remove
        #[arg(required_unless_present = "help", default_value = "")]
        name: String,
    },
}
