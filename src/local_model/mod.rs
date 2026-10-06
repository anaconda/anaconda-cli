mod commands;
mod delete;
mod gguf;
mod kilo;
mod pull;
mod run;
mod server;
mod store;

pub use commands::LocalModelCommands;

use crate::context::CommandContext;

/// Run an `ana lm` subcommand.
pub async fn run(ctx: &mut CommandContext, command: LocalModelCommands) -> miette::Result<()> {
    match command {
        LocalModelCommands::List { json } => list(ctx, json).await,
        LocalModelCommands::Pull {
            model,
            format,
            file,
            quant,
        } => {
            let format = format.ok_or_else(|| miette::miette!("--format is required"))?;
            pull::pull(
                ctx,
                pull::PullOptions {
                    model: &model,
                    format,
                    file: file.as_deref(),
                    quant: quant.as_deref(),
                },
            )
            .await
        }
        LocalModelCommands::Run {
            model,
            quant,
            host,
            port,
            llama_args,
        } => {
            run::run(
                ctx,
                run::RunOptions {
                    model: &model,
                    quant: quant.as_deref(),
                    host: &host,
                    port,
                    extra_args: &llama_args,
                },
            )
            .await
        }
        LocalModelCommands::Delete {
            model,
            quant,
            force,
        } => delete::delete(&model, quant.as_deref(), force),
    }
}

async fn list(_ctx: &mut CommandContext, json: bool) -> miette::Result<()> {
    // TODO: Implement listing of local models
    println!("ana lm list (json={json}): not yet implemented");
    Ok(())
}
