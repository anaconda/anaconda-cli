use std::path::Path;

use miette::Result;

use crate::context::CommandContext;
use crate::ui::status;

use super::output::{create_table, print_table};

pub async fn list(ctx: &CommandContext) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let result = ob.workstations().list().await?;

    if result.workstations.is_empty() {
        println!("No workstations found");
        return Ok(());
    }

    let mut table = create_table(&["ID", "Name", "Status", "Namespace"]);

    for ws in &result.workstations {
        let name = ws
            .spec
            .as_ref()
            .and_then(|s| s.workstation_name.as_deref())
            .unwrap_or("-");
        let ws_status = ws
            .status
            .as_ref()
            .and_then(|s| s.status_code.as_deref())
            .unwrap_or("unknown");
        let namespace = ws
            .kubernetes_metadata
            .as_ref()
            .and_then(|m| m.workstation_pod_namespace.as_deref())
            .unwrap_or("-");
        table.add_row(vec![&ws.instance_id, name, ws_status, namespace]);
    }

    print_table(table);
    Ok(())
}

pub async fn hibernate(ctx: &CommandContext, workstation_id: &str) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let result = ob.workstations().hibernate(workstation_id).await?;

    if result.success {
        status::success(&format!("Hibernating workstation: {}", workstation_id));
        if let Some(ref msg) = result.message {
            println!("{}", msg);
        }
    } else {
        let msg = result.message.as_deref().unwrap_or("Unknown error");
        status::warn(&format!(
            "Failed to hibernate {}: {}",
            workstation_id, msg
        ));
    }

    Ok(())
}

pub async fn restart(ctx: &CommandContext, workstation_id: &str) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let result = ob.workstations().restart(workstation_id).await?;

    if result.success {
        status::success(&format!("Restarting workstation: {}", workstation_id));
        if let Some(ref msg) = result.message {
            println!("{}", msg);
        }
    } else {
        let msg = result.message.as_deref().unwrap_or("Unknown error");
        status::warn(&format!(
            "Failed to restart {}: {}",
            workstation_id, msg
        ));
    }

    Ok(())
}

pub async fn generate_token(ctx: &CommandContext) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let credential = ob.workstations().generate_token().await?;

    let json = serde_json::to_string_pretty(&credential)
        .map_err(|e| miette::miette!("Failed to serialize credential: {}", e))?;
    println!("{}", json);

    Ok(())
}

pub async fn get_namespace(ctx: &CommandContext, workstation_id: &str) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let namespace = ob.workstations().get_namespace(workstation_id).await?;
    println!("{}", namespace);

    Ok(())
}

pub async fn get_links(ctx: &CommandContext) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let links = ob.workstations().get_relevant_links()?;

    if links.is_empty() {
        println!("No relevant links found");
        return Ok(());
    }

    let mut table = create_table(&["Label", "URL"]);

    for link in &links {
        table.add_row(vec![&link.label, &link.url]);
    }

    print_table(table);
    Ok(())
}

pub async fn configure_kubeconfig(
    ctx: &CommandContext,
    binary_path: Option<&str>,
    kubeconfig_path: Option<&str>,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let binary = binary_path.unwrap_or("ana");
    let kube_path = kubeconfig_path.map(Path::new);

    ob.workstations()
        .configure_kubeconfig(binary, kube_path)
        .await?;

    status::success("Kubeconfig configured for workstation access");

    Ok(())
}

pub async fn prepare_ssh(ctx: &CommandContext, workstation_id: &str) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let result = ob
        .workstations()
        .prepare_ssh_access_local(workstation_id)
        .await?;

    status::success("SSH access prepared");
    println!("Workstation: {}", result.workstation_id);
    println!("Namespace: {}", result.namespace);
    println!("Private key: {}", result.private_key_path.display());
    println!("Public key: {}", result.public_key_path.display());

    if result.ssh_config_updated {
        println!("SSH config updated");
    }

    println!("\nTo connect:");
    println!("  ssh {}", result.ssh_host);

    Ok(())
}

pub async fn install_kubectl(
    ctx: &CommandContext,
    install_dir: Option<&str>,
    version: Option<&str>,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let dir = install_dir.map(Path::new);
    let result = ob.workstations().install_kubectl(dir, version).await?;

    if result.already_installed {
        println!(
            "kubectl already installed at {}",
            result.install_path.display()
        );
    } else {
        status::success(&format!(
            "kubectl installed to {}",
            result.install_path.display()
        ));
    }

    if result.path_modified {
        println!("PATH was modified");
    }

    if result.reload_required {
        println!("Please reload your shell or run: source ~/.bashrc (or ~/.zshrc)");
    }

    if !result.message.is_empty() {
        println!("{}", result.message);
    }

    Ok(())
}
