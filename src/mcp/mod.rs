mod clients;
mod commands;
mod setup;
mod state;
mod terms;

pub use commands::{McpCommands, McpTermsCommands};

use crate::context::CommandContext;

/// Run an `ana mcp` subcommand natively.
///
/// All subcommands except `terms` are gated on Terms of Service acceptance.
pub fn run(ctx: &mut CommandContext, command: McpCommands) -> miette::Result<()> {
    if !matches!(command, McpCommands::Terms { .. }) {
        terms::ensure_accepted(ctx)?;
    }

    match command {
        McpCommands::Clients { json } => setup::list_clients(json),
        McpCommands::Setup {
            client,
            name,
            no_backup,
            json,
        } => setup::setup(ctx, &client, &name, no_backup, json),
        McpCommands::Remove {
            client,
            name,
            no_backup,
            json,
        } => setup::remove(&client, &name, no_backup, json),
        McpCommands::Terms { command, json } => terms::run(ctx, command, json),
    }
}
