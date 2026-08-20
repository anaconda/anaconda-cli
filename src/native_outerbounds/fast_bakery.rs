use std::path::Path;

use miette::Result;

use crate::context::CommandContext;
use crate::ui::status;

pub async fn get_login_password(ctx: &CommandContext) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let password = ob.fast_bakery_auth().get_login_password().await?;
    println!("{}", password);

    Ok(())
}

pub async fn configure_docker_login(
    ctx: &CommandContext,
    registry_url: &str,
    output: Option<&str>,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let path = ob
        .fast_bakery_auth()
        .configure_docker_login(registry_url, output.map(Path::new))
        .await?;

    status::success(&format!(
        "Configured Docker login for {} in {}",
        registry_url,
        path.display()
    ));

    Ok(())
}
