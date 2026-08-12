use std::path::Path;

use miette::{miette, Result};
use outerbounds::Outerbounds;

use crate::context::CommandContext;
use crate::ui::status;

use super::output::{create_table, print_table};

fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/")
        && let Some(home) = dirs::home_dir() {
            return path.replacen("~", &home.to_string_lossy(), 1);
        }
    path.to_string()
}

pub async fn list(ctx: &CommandContext) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;
    let result = ob.perimeter().list().await?;

    let mut table = create_table(&["ID", "Current"]);

    for perimeter in result.perimeters {
        let is_current = if perimeter.active { "✓" } else { "" };
        table.add_row(vec![&perimeter.id, is_current]);
    }

    print_table(table);
    Ok(())
}

pub async fn show_current(ctx: &CommandContext) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;
    let current = ob.perimeter().show_current().await?;

    match current {
        Some(perimeter_id) => {
            println!("Current perimeter: {}", perimeter_id);
        }
        None => {
            println!("No perimeter currently set");
        }
    }

    Ok(())
}

pub async fn switch(
    config_dir: &str,
    profile: Option<&str>,
    output: Option<&str>,
    id: Option<&str>,
    force: bool,
) -> Result<()> {
    let perimeter_id = id.ok_or_else(|| miette!("--id is required"))?;

    let config_path = expand_tilde(config_dir);
    let ob = Outerbounds::new(Some(Path::new(&config_path)), profile)
        .await
        .map_err(|e| miette!("{}", e))?;

    let result = ob
        .perimeter()
        .switch(perimeter_id, force)
        .await
        .map_err(|e| miette!("{}", e))?;

    if output == Some("json") {
        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| miette!("Failed to serialize response: {}", e))?;
        println!("{}", json);
        return Ok(());
    }

    if result.success {
        status::success(&format!("Switched to perimeter: {}", result.perimeter));
    } else {
        println!("Already on perimeter: {}", result.perimeter);
    }

    Ok(())
}
