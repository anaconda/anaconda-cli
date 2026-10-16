mod clients;
mod commands;
mod server;
mod setup;
mod skill;
mod state;

pub use commands::McpCommands;

use crate::context::CommandContext;

/// Run an `ana mcp` subcommand natively.
pub async fn run(ctx: &mut CommandContext, command: McpCommands) -> miette::Result<()> {
    match command {
        McpCommands::Serve => server::serve(ctx).await,
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
