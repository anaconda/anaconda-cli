use std::path::Path;

use miette::Result;

use crate::context::CommandContext;
use crate::ui::status;

pub async fn pull(
    ctx: &CommandContext,
    url: &str,
    destination: Option<&str>,
    verify_hash: Option<&str>,
    force: bool,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let dest = destination.unwrap_or(".");
    let dest_path = Path::new(dest);

    let result = ob
        .tutorials()
        .pull_with_hash(url, dest_path, verify_hash, force)
        .await?;

    status::success(&format!("Downloaded tutorials to {}", dest));

    if result.files_extracted > 0 {
        println!("Files extracted: {}", result.files_extracted);
    }
    if result.files_skipped > 0 {
        println!("Files skipped (already exist): {}", result.files_skipped);
    }

    Ok(())
}
