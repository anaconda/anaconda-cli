use std::path::Path;

use miette::Result;

use crate::context::CommandContext;
use crate::ui::status;

pub async fn pull(
    ctx: &CommandContext,
    url: &str,
    destination_dir: &str,
    force_overwrite: bool,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let dest_path = Path::new(destination_dir);

    let result = ob.tutorials().pull(url, dest_path, force_overwrite).await?;

    status::success(&format!("Downloaded tutorials to {}", destination_dir));

    if result.files_extracted > 0 {
        println!("Files extracted: {}", result.files_extracted);
    }
    if result.files_skipped > 0 {
        println!("Files skipped (already exist): {}", result.files_skipped);
    }

    Ok(())
}
