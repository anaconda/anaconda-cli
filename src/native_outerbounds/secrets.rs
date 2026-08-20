use std::fs;

use miette::{Result, miette};
use outerbounds::SecretFormat;

use crate::context::CommandContext;

pub async fn get(
    ctx: &CommandContext,
    secret_ids: &[String],
    format: Option<&str>,
    _role: Option<&str>,
    file: Option<&str>,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let names: Vec<&str> = secret_ids.iter().map(|s| s.as_str()).collect();
    let secrets = ob.secrets().get_many(&names).await?;

    let output = match format {
        Some("json") => outerbounds::format_secrets(&secrets, SecretFormat::Json),
        Some("text") | None => outerbounds::format_secrets(&secrets, SecretFormat::Text),
        Some("shell") => secrets
            .iter()
            .flat_map(|s| {
                s.values
                    .iter()
                    .map(|(k, v)| format!("export {}=\"{}\"", k, v))
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Some(f) => return Err(miette!("Invalid format: {}. Use text, json, or shell.", f)),
    };

    match file {
        Some(path) => {
            fs::write(path, &output)
                .map_err(|e| miette!("Failed to write to file {}: {}", path, e))?;
        }
        None => {
            print!("{}", output);
        }
    }

    Ok(())
}
