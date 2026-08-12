use miette::Result;

use crate::context::CommandContext;
use crate::ui::status;

use super::output::{create_table, print_table};

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

pub async fn switch(ctx: &CommandContext, perimeter_id: &str, force: bool) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;
    let result = ob.perimeter().switch(perimeter_id, force).await?;

    if result.success {
        status::success(&format!("Switched to perimeter: {}", result.perimeter));
    } else {
        println!("Already on perimeter: {}", result.perimeter);
    }

    Ok(())
}
