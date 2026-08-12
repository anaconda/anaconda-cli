use miette::Result;

use super::commands::ObnAction;
use super::{
    app, check, configure, flowproject, integrations, perimeter, secrets, tutorials, workstations,
};
use crate::context::CommandContext;
use crate::help;

pub async fn run(
    ctx: &CommandContext,
    action: ObnAction,
    config_dir: &str,
    profile: Option<&str>,
) -> Result<()> {
    match action {
        ObnAction::ShowHelp(path) => {
            help::print_subcommand_help(&get_subcommand(&path), &path);
            Ok(())
        }
        ObnAction::Configure {
            encoded_config,
            echo,
            force,
        } => configure::configure(&encoded_config, config_dir, profile, echo, force).await,
        ObnAction::ServicePrincipalConfigure {
            name,
            deployment_domain,
            perimeter,
            jwt_token,
            github_actions,
            echo,
            force,
        } => {
            configure::service_principal_configure(
                name.as_deref(),
                deployment_domain.as_deref(),
                perimeter.as_deref(),
                jwt_token.as_deref(),
                github_actions,
                config_dir,
                profile,
                echo,
                force,
            )
            .await
        }
        ObnAction::Check {
            no_config,
            output,
            workstation,
            latency,
            latency_requests,
            latency_timeout,
        } => {
            check::check(
                ctx,
                no_config,
                output.as_deref(),
                workstation,
                latency,
                latency_requests,
                latency_timeout,
            )
            .await
        }
        ObnAction::PerimeterList => perimeter::list(ctx).await,
        ObnAction::PerimeterShowCurrent => perimeter::show_current(ctx).await,
        ObnAction::PerimeterSwitch { output, id, force } => {
            perimeter::switch(config_dir, profile, output.as_deref(), id.as_deref(), force).await
        }
        ObnAction::AppList {
            project,
            branch,
            name,
            tags,
            format,
            auth_type,
        } => {
            app::list(
                ctx,
                project.as_deref(),
                branch.as_deref(),
                name.as_deref(),
                &tags,
                format.as_deref(),
                auth_type.as_deref(),
            )
            .await
        }
        ObnAction::AppInfo { id, format } => app::info(ctx, &id, format.as_deref()).await,
        ObnAction::AppDelete { ids } => app::delete(ctx, &ids).await,
        ObnAction::AppLogs {
            id,
            worker_id,
            previous,
        } => app::logs(ctx, &id, worker_id.as_deref(), previous).await,
        ObnAction::IntegrationsList { perimeter } => {
            integrations::list(ctx, perimeter.as_deref()).await
        }
        ObnAction::IntegrationsGet { name, perimeter } => {
            integrations::get(ctx, &name, perimeter.as_deref()).await
        }
        ObnAction::IntegrationsDelete { name, perimeter } => {
            integrations::delete(ctx, &name, perimeter.as_deref()).await
        }
        ObnAction::IntegrationsListPrivatePypi { perimeter } => {
            integrations::list_private_pypi(ctx, perimeter.as_deref()).await
        }
        ObnAction::IntegrationsListPrivateConda { perimeter } => {
            integrations::list_private_conda(ctx, perimeter.as_deref()).await
        }

        // Flowproject
        ObnAction::FlowprojectGetMetadata { id } => {
            flowproject::get_metadata(ctx, id.as_deref()).await
        }
        ObnAction::FlowprojectDeleteMetadata { id, output } => {
            flowproject::delete_metadata(ctx, &id, output.as_deref()).await
        }
        ObnAction::FlowprojectListTemplates { id, output } => {
            flowproject::list_templates(ctx, &id, output.as_deref()).await
        }
        ObnAction::FlowprojectTeardownBranch { id, dry_run, output } => {
            flowproject::teardown_branch(ctx, &id, dry_run, output.as_deref()).await
        }

        // Secrets
        ObnAction::SecretsGet {
            secret_ids,
            format,
            role,
            file,
        } => {
            secrets::get(
                ctx,
                &secret_ids,
                format.as_deref(),
                role.as_deref(),
                file.as_deref(),
            )
            .await
        }

        // Tutorials
        ObnAction::TutorialsPull {
            url,
            destination_dir,
            force_overwrite,
        } => tutorials::pull(ctx, &url, &destination_dir, force_overwrite).await,

        // Workstations
        ObnAction::WorkstationsList => workstations::list(ctx).await,
        ObnAction::WorkstationsHibernate { workstation_id } => {
            workstations::hibernate(ctx, &workstation_id).await
        }
        ObnAction::WorkstationsRestart { workstation_id } => {
            workstations::restart(ctx, &workstation_id).await
        }
        ObnAction::WorkstationsGenerateToken => workstations::generate_token(ctx).await,
        ObnAction::WorkstationsGetNamespace { workstation_id } => {
            workstations::get_namespace(ctx, &workstation_id).await
        }
        ObnAction::WorkstationsGetLinks => workstations::get_links(ctx).await,
        ObnAction::WorkstationsConfigureKubeconfig {
            binary_path,
            kubeconfig_path,
        } => {
            workstations::configure_kubeconfig(
                ctx,
                binary_path.as_deref(),
                kubeconfig_path.as_deref(),
            )
            .await
        }
        ObnAction::WorkstationsPrepareSsh { workstation_id } => {
            workstations::prepare_ssh(ctx, &workstation_id).await
        }
        ObnAction::WorkstationsInstallKubectl {
            install_dir,
            version,
        } => workstations::install_kubectl(ctx, install_dir.as_deref(), version.as_deref()).await,
    }
}

fn get_subcommand(path: &str) -> clap::Command {
    use clap::CommandFactory;

    #[derive(clap::Parser)]
    struct DummyCli {
        #[command(subcommand)]
        command: super::commands::ObnCommands,
    }

    let parts: Vec<&str> = path.split_whitespace().collect();
    let mut cmd = DummyCli::command();

    // Skip "obn" prefix if present
    let parts = if parts.first() == Some(&"obn") {
        &parts[1..]
    } else {
        &parts[..]
    };

    for part in parts {
        let subcmd = cmd
            .get_subcommands()
            .find(|s| s.get_name() == *part)
            .cloned()
            .unwrap_or_else(|| cmd.clone());
        cmd = subcmd;
    }

    cmd
}
