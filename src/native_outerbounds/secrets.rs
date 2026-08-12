use std::fs;

use miette::{miette, Result};
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

    let output_format = match format {
        Some("json") => SecretFormat::Json,
        Some("text") | None => SecretFormat::Text,
        Some(f) => return Err(miette!("Invalid format: {}. Use text or json.", f)),
    };

    let output = outerbounds::format_secrets(&secrets, output_format);

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
