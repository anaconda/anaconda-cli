use miette::Result;

use super::commands::ObnAction;
use super::{
    app, check, configure, fast_bakery, flowproject, integrations, kubernetes, perimeter, secrets,
    tutorials, workstations,
};
use crate::context::CommandContext;
use crate::help;

pub async fn run(
    ctx: &CommandContext,
    action: ObnAction,
    config_dir: &str,
    profile: Option<&str>,
    verbose: u8,
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
            from_obproject_toml,
            toml_path,
            echo,
            force,
        } => {
            configure::service_principal_configure(
                name.as_deref(),
                deployment_domain.as_deref(),
                perimeter.as_deref(),
                jwt_token.as_deref(),
                github_actions,
                from_obproject_toml,
                &toml_path,
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
                verbose > 0,
            )
            .await
        }
        ObnAction::PerimeterList { output } => perimeter::list(ctx, output.as_deref()).await,
        ObnAction::PerimeterShowCurrent { output } => {
            perimeter::show_current(ctx, output.as_deref()).await
        }
        ObnAction::PerimeterEnsureCloudCreds {
            cspr_override,
            output,
        } => {
            perimeter::ensure_cloud_creds(ctx, cspr_override.as_deref(), output.as_deref()).await
        }
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
        ObnAction::AppInfo {
            id,
            name,
            project,
            branch,
            format,
        } => {
            app::info(
                ctx,
                id.as_deref(),
                name.as_deref(),
                project.as_deref(),
                branch.as_deref(),
                format.as_deref(),
            )
            .await
        }
        ObnAction::AppDelete {
            ids,
            name,
            project,
            branch,
            tags,
            auto_approve,
        } => {
            app::delete(
                ctx,
                &ids,
                name.as_deref(),
                project.as_deref(),
                branch.as_deref(),
                &tags,
                auto_approve,
            )
            .await
        }
        ObnAction::AppDeploy {
            options,
            status_file,
        } => app::deploy(ctx, *options, status_file.as_deref()).await,
        ObnAction::AppLogs {
            id,
            name,
            project,
            branch,
            worker_id,
            previous,
            file,
        } => {
            app::logs(
                ctx,
                id.as_deref(),
                name.as_deref(),
                project.as_deref(),
                branch.as_deref(),
                worker_id.as_deref(),
                previous,
                file.as_deref(),
            )
            .await
        }
        ObnAction::IntegrationsList { perimeter } => {
            integrations::list(ctx, perimeter.as_deref()).await
        }
        ObnAction::IntegrationsGet {
            name,
            perimeter,
            show_secret_values,
        } => integrations::get(ctx, &name, perimeter.as_deref(), show_secret_values).await,
        ObnAction::IntegrationsDelete { name, perimeter } => {
            integrations::delete(ctx, &name, perimeter.as_deref()).await
        }
        ObnAction::IntegrationsListPrivatePypi { perimeter } => {
            integrations::list_private_pypi(ctx, perimeter.as_deref()).await
        }
        ObnAction::IntegrationsListPrivateConda { perimeter } => {
            integrations::list_private_conda(ctx, perimeter.as_deref()).await
        }

        // Integration creation
        ObnAction::IntegrationsAnacondaCreate {
            name,
            description,
            perimeter,
        } => {
            integrations::anaconda_create(ctx, &name, description.as_deref(), perimeter.as_deref())
                .await
        }
        ObnAction::IntegrationsArtifactoryCreate {
            name,
            description,
            url,
            username,
            password,
            perimeter,
        } => {
            integrations::artifactory_create(
                ctx,
                &name,
                description.as_deref(),
                &url,
                &username,
                &password,
                perimeter.as_deref(),
            )
            .await
        }
        ObnAction::IntegrationsAzureArtifactsCreate {
            name,
            description,
            organization,
            project,
            feed,
            username,
            pat,
            perimeter,
        } => {
            integrations::azure_artifacts_create(
                ctx,
                &name,
                description.as_deref(),
                &organization,
                &project,
                &feed,
                &username,
                &pat,
                perimeter.as_deref(),
            )
            .await
        }
        ObnAction::IntegrationsCodeArtifactsCreate {
            name,
            description,
            domain_name,
            domain_owner,
            aws_region,
            target_role,
            perimeter,
        } => {
            integrations::code_artifacts_create(
                ctx,
                &name,
                description.as_deref(),
                &domain_name,
                &domain_owner,
                &aws_region,
                target_role.as_deref(),
                perimeter.as_deref(),
            )
            .await
        }
        ObnAction::IntegrationsContainerRegistryCreate {
            name,
            description,
            registry_domain,
            target_role_arn,
            use_task_role,
            username,
            password,
            perimeter,
        } => {
            integrations::container_registry_create(
                ctx,
                &name,
                description.as_deref(),
                &registry_domain,
                target_role_arn.as_deref(),
                use_task_role,
                username.as_deref(),
                password.as_deref(),
                perimeter.as_deref(),
            )
            .await
        }
        ObnAction::IntegrationsCustomSecretCreate {
            name,
            description,
            secrets,
            perimeter,
        } => {
            integrations::custom_secret_create(
                ctx,
                &name,
                description.as_deref(),
                &secrets,
                perimeter.as_deref(),
            )
            .await
        }
        ObnAction::IntegrationsCustomSecretUpdate {
            name,
            description,
            secrets,
            perimeter,
        } => {
            integrations::custom_secret_update(
                ctx,
                &name,
                description.as_deref(),
                &secrets,
                perimeter.as_deref(),
            )
            .await
        }
        ObnAction::IntegrationsGitPypiRepositoryCreate {
            name,
            description,
            repository_url,
            username,
            password,
            perimeter,
        } => {
            integrations::git_pypi_repository_create(
                ctx,
                &name,
                description.as_deref(),
                &repository_url,
                username.as_deref(),
                password.as_deref(),
                perimeter.as_deref(),
            )
            .await
        }
        ObnAction::IntegrationsGitlabArtifactsCreate {
            name,
            description,
            gitlab_url,
            project_id,
            username,
            password,
            perimeter,
        } => {
            integrations::gitlab_artifacts_create(
                ctx,
                &name,
                description.as_deref(),
                &gitlab_url,
                &project_id,
                username.as_deref(),
                password.as_deref(),
                perimeter.as_deref(),
            )
            .await
        }
        ObnAction::IntegrationsPrivateCondaChannelsAdd {
            channel_name,
            host_integration_name,
            is_default,
            perimeter,
        } => {
            integrations::private_conda_channels_add(
                ctx,
                &channel_name,
                &host_integration_name,
                is_default,
                perimeter.as_deref(),
            )
            .await
        }
        ObnAction::IntegrationsPrivatePypiRepositoriesAdd {
            repository_name,
            host_integration_name,
            is_default,
            perimeter,
        } => {
            integrations::private_pypi_repositories_add(
                ctx,
                &repository_name,
                &host_integration_name,
                is_default,
                perimeter.as_deref(),
            )
            .await
        }
        ObnAction::IntegrationsS3ProxyCreate {
            name,
            description,
            bucket_name,
            endpoint_url,
            region,
            access_key_id,
            secret_access_key,
            perimeter,
        } => {
            integrations::s3_proxy_create(
                ctx,
                &name,
                description.as_deref(),
                &bucket_name,
                &endpoint_url,
                &region,
                &access_key_id,
                &secret_access_key,
                perimeter.as_deref(),
            )
            .await
        }

        // Integration updates
        ObnAction::IntegrationsS3ProxyUpdate {
            name,
            description,
            bucket_name,
            endpoint_url,
            region,
            access_key_id,
            secret_access_key,
            perimeter,
        } => {
            integrations::s3_proxy_update(
                ctx,
                &name,
                description.as_deref(),
                bucket_name.as_deref(),
                endpoint_url.as_deref(),
                region.as_deref(),
                access_key_id.as_deref(),
                secret_access_key.as_deref(),
                perimeter.as_deref(),
            )
            .await
        }
        ObnAction::IntegrationsCodeArtifactsUpdate {
            name,
            description,
            domain_name,
            domain_owner,
            aws_region,
            target_role,
            perimeter,
        } => {
            integrations::code_artifacts_update(
                ctx,
                &name,
                description.as_deref(),
                domain_name.as_deref(),
                domain_owner.as_deref(),
                aws_region.as_deref(),
                target_role.as_deref(),
                perimeter.as_deref(),
            )
            .await
        }
        ObnAction::IntegrationsArtifactoryUpdate {
            name,
            description,
            url,
            username,
            password,
            perimeter,
        } => {
            integrations::artifactory_update(
                ctx,
                &name,
                description.as_deref(),
                url.as_deref(),
                username.as_deref(),
                password.as_deref(),
                perimeter.as_deref(),
            )
            .await
        }
        ObnAction::IntegrationsAzureArtifactsUpdate {
            name,
            description,
            organization,
            project,
            username,
            pat,
            perimeter,
        } => {
            integrations::azure_artifacts_update(
                ctx,
                &name,
                description.as_deref(),
                organization.as_deref(),
                project.as_deref(),
                username.as_deref(),
                pat.as_deref(),
                perimeter.as_deref(),
            )
            .await
        }
        ObnAction::IntegrationsGitlabArtifactsUpdate {
            name,
            description,
            gitlab_url,
            project_id,
            username,
            password,
            perimeter,
        } => {
            integrations::gitlab_artifacts_update(
                ctx,
                &name,
                description.as_deref(),
                gitlab_url.as_deref(),
                project_id.as_deref(),
                username.as_deref(),
                password.as_deref(),
                perimeter.as_deref(),
            )
            .await
        }
        ObnAction::IntegrationsContainerRegistryUpdate {
            name,
            description,
            registry_domain,
            target_role_arn,
            use_task_role,
            username,
            password,
            perimeter,
        } => {
            integrations::container_registry_update(
                ctx,
                &name,
                description.as_deref(),
                registry_domain.as_deref(),
                target_role_arn.as_deref(),
                use_task_role,
                username.as_deref(),
                password.as_deref(),
                perimeter.as_deref(),
            )
            .await
        }
        ObnAction::IntegrationsGitPypiRepositoryUpdate {
            name,
            description,
            repository_urls,
            username,
            password,
            perimeter,
        } => {
            integrations::git_pypi_repository_update(
                ctx,
                &name,
                description.as_deref(),
                &repository_urls,
                username.as_deref(),
                password.as_deref(),
                perimeter.as_deref(),
            )
            .await
        }
        ObnAction::IntegrationsPrivateCondaChannelsRemove {
            channel_name,
            perimeter,
        } => {
            integrations::private_conda_channels_remove(ctx, &channel_name, perimeter.as_deref())
                .await
        }
        ObnAction::IntegrationsPrivatePypiRepositoriesRemove {
            repository_name,
            perimeter,
        } => {
            integrations::private_pypi_repositories_remove(
                ctx,
                &repository_name,
                perimeter.as_deref(),
            )
            .await
        }

        // Fast Bakery
        ObnAction::FastBakeryGetLoginPassword => fast_bakery::get_login_password(ctx).await,
        ObnAction::FastBakeryConfigureDockerLogin {
            registry_url,
            output,
        } => fast_bakery::configure_docker_login(ctx, &registry_url, output.as_deref()).await,

        // Kubernetes
        ObnAction::KubernetesKill {
            flow_name,
            run_id,
            my_runs,
            dry_run,
            auto_approve,
            clear_everything,
        } => {
            kubernetes::kill(
                ctx,
                &flow_name,
                run_id.as_deref(),
                my_runs,
                dry_run,
                auto_approve,
                clear_everything,
            )
            .await
        }

        // Flowproject
        ObnAction::FlowprojectGetMetadata { id } => {
            flowproject::get_metadata(ctx, id.as_deref()).await
        }
        ObnAction::FlowprojectSetMetadata { json } => {
            flowproject::set_metadata(ctx, &json).await
        }
        ObnAction::FlowprojectDeleteMetadata { id, yes, output } => {
            flowproject::delete_metadata(ctx, &id, yes, output.as_deref()).await
        }
        ObnAction::FlowprojectListTemplates { id, output } => {
            flowproject::list_templates(ctx, &id, output.as_deref()).await
        }
        ObnAction::FlowprojectTeardownBranch {
            id,
            dry_run,
            yes,
            output,
        } => flowproject::teardown_branch(ctx, &id, dry_run, yes, output.as_deref()).await,

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
        ObnAction::WorkstationsList { output } => workstations::list(ctx, output.as_deref()).await,
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
        ObnAction::WorkstationsGetLinks {
            perimeter_id,
            output,
        } => {
            workstations::get_links(ctx, perimeter_id.as_deref(), output.as_deref()).await
        }
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
        ObnAction::WorkstationsPrepareSsh {
            workstation_id,
            setup_context,
            mode,
        } => workstations::prepare_ssh(ctx, &workstation_id, &setup_context, &mode).await,
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
