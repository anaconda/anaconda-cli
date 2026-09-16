mod clients;
mod commands;
mod setup;
mod state;

pub use commands::McpCommands;

use crate::context::CommandContext;

/// Run an `ana mcp` subcommand natively.
pub fn run(ctx: &mut CommandContext, command: McpCommands) -> miette::Result<()> {
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
    }
}
