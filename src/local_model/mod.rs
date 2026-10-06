mod catalog;
mod commands;
mod delete;
mod gguf;
mod kilo;
mod list;
mod picker;
mod pull;
mod run;
mod server;
mod store;

pub use commands::LocalModelCommands;

use crate::context::CommandContext;

/// Run an `ana lm` subcommand.
pub async fn run(ctx: &mut CommandContext, command: LocalModelCommands) -> miette::Result<()> {
    match command {
        LocalModelCommands::List {
            local,
            name,
            publisher,
            purpose,
            tags,
            sizes,
            file,
            json,
        } => {
            let filter = catalog::CatalogFilter {
                name,
                publisher,
                purpose,
                tags,
                sizes: sizes.iter().map(|s| s.name().to_string()).collect(),
                file,
            };
            list::list(ctx, local, &filter, json).await
        }
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
                    model: model.as_deref(),
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
