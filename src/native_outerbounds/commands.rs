use clap::Subcommand;

#[derive(Debug)]
pub enum ObnAction {
    ShowHelp(String),

    // Configure
    Configure {
        encoded_config: String,
        echo: bool,
        force: bool,
    },
    ServicePrincipalConfigure {
        name: Option<String>,
        deployment_domain: Option<String>,
        perimeter: Option<String>,
        jwt_token: Option<String>,
        github_actions: bool,
        echo: bool,
        force: bool,
    },

    // Check
    Check {
        no_config: bool,
        output: Option<String>,
        workstation: bool,
        latency: bool,
        latency_requests: u32,
        latency_timeout: f64,
    },

    // Perimeter
    PerimeterList,
    PerimeterShowCurrent,
    PerimeterSwitch {
        output: Option<String>,
        id: Option<String>,
        force: bool,
    },

    // App
    AppList {
        project: Option<String>,
        branch: Option<String>,
        name: Option<String>,
        tags: Vec<String>,
        format: Option<String>,
        auth_type: Option<String>,
    },
    AppInfo {
        id: String,
        format: Option<String>,
    },
    AppDelete {
        ids: Vec<String>,
    },
    AppLogs {
        id: String,
        worker_id: Option<String>,
        previous: bool,
    },

    // Integrations
    IntegrationsList {
        perimeter: Option<String>,
    },
    IntegrationsGet {
        name: String,
        perimeter: Option<String>,
    },
    IntegrationsDelete {
        name: String,
        perimeter: Option<String>,
    },
    IntegrationsListPrivatePypi {
        perimeter: Option<String>,
    },
    IntegrationsListPrivateConda {
        perimeter: Option<String>,
    },

    // FlowProject
    FlowprojectGetMetadata {
        project: String,
        branch: String,
    },
    FlowprojectDeleteMetadata {
        project: String,
        branch: String,
    },
    FlowprojectListTemplates {
        project: String,
        branch: String,
    },
    FlowprojectTeardownBranch {
        project: String,
        branch: String,
        dry_run: bool,
    },

    // Secrets
    SecretsGetMetadata {
        integration_name: String,
    },
    SecretsGet {
        integration_name: String,
        json: bool,
    },
    SecretsGetMany {
        integration_names: Vec<String>,
        json: bool,
    },

    // Tutorials
    TutorialsPull {
        url: String,
        destination: Option<String>,
        verify_hash: Option<String>,
        force: bool,
    },

    // Workstations
    WorkstationsList,
    WorkstationsHibernate {
        workstation_id: String,
    },
    WorkstationsRestart {
        workstation_id: String,
    },
    WorkstationsGenerateToken,
    WorkstationsGetNamespace {
        workstation_id: String,
    },
    WorkstationsGetLinks,
    WorkstationsConfigureKubeconfig {
        binary_path: Option<String>,
        kubeconfig_path: Option<String>,
    },
    WorkstationsPrepareSsh {
        workstation_id: String,
    },
    WorkstationsInstallKubectl {
        install_dir: Option<String>,
        version: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ObnCommands {
    /// Decode Outerbounds Platform configuration strings
    Configure {
        /// Base64-encoded configuration string
        encoded_config: String,

        /// Print decoded configuration to stdout
        #[arg(long, short = 'e')]
        echo: bool,

        /// Force overwrite of existing configuration
        #[arg(long, short = 'f')]
        force: bool,
    },

    /// Authenticate service principals using JWT minted by their IDPs and configure Metaflow
    #[command(name = "service-principal-configure")]
    ServicePrincipalConfigure {
        /// The name of service principals to authenticate
        #[arg(long, short = 'n')]
        name: Option<String>,

        /// The full domain of the target Outerbounds Platform deployment (eg. 'foo.obp.outerbounds.com')
        #[arg(long)]
        deployment_domain: Option<String>,

        /// The name of the perimeter to authenticate the service principal in
        #[arg(long)]
        perimeter: Option<String>,

        /// The JWT token that will be used to authenticate against the OBP Auth Server
        #[arg(long, short = 't')]
        jwt_token: Option<String>,

        /// Set if the command is being run in a GitHub Actions environment
        #[arg(long)]
        github_actions: bool,

        /// Print decoded configuration to stdout
        #[arg(long, short = 'e')]
        echo: bool,

        /// Force overwrite of existing configuration
        #[arg(long, short = 'f')]
        force: bool,
    },

    /// Check packages and configuration for common errors
    Check {
        /// Skip validating local Metaflow configuration
        #[arg(long, short = 'n')]
        no_config: bool,

        /// Show output in the specified format
        #[arg(long, short = 'o')]
        output: Option<String>,

        /// Check whether all workstation dependencies are installed correctly
        #[arg(long, short = 'w')]
        workstation: bool,

        /// Check API latency for Workstations, Auth Server, and EKS endpoints
        #[arg(long, short = 'l')]
        latency: bool,

        /// Number of requests per endpoint for latency check
        #[arg(long, default_value = "10")]
        latency_requests: u32,

        /// Connection timeout in seconds for latency check requests
        #[arg(long, default_value = "10.0")]
        latency_timeout: f64,
    },

    /// Manage perimeters
    #[command(subcommand_required = false, arg_required_else_help = false)]
    Perimeter {
        #[command(subcommand)]
        command: Option<PerimeterCommands>,
    },

    /// Commands for managing deployed apps
    #[command(subcommand_required = false, arg_required_else_help = false)]
    App {
        #[command(subcommand)]
        command: Option<AppCommands>,
    },

    /// Manage resource integrations
    #[command(subcommand_required = false, arg_required_else_help = false)]
    Integrations {
        #[command(subcommand)]
        command: Option<IntegrationsCommands>,
    },

    /// Commands for pushing Deployments metadata
    #[command(subcommand_required = false, arg_required_else_help = false)]
    Flowproject {
        #[command(subcommand)]
        command: Option<FlowprojectCommands>,
    },

    /// Fetch secrets from cloud secret managers
    #[command(subcommand_required = false, arg_required_else_help = false)]
    Secrets {
        #[command(subcommand)]
        command: Option<SecretsCommands>,
    },

    /// Download tutorial content
    #[command(subcommand_required = false, arg_required_else_help = false)]
    Tutorials {
        #[command(subcommand)]
        command: Option<TutorialsCommands>,
    },

    /// Manage cloud workstations
    #[command(subcommand_required = false, arg_required_else_help = false)]
    Workstations {
        #[command(subcommand)]
        command: Option<WorkstationsCommands>,
    },
}

#[derive(Subcommand, Debug)]
pub enum PerimeterCommands {
    /// List all available perimeters
    List,

    /// Show current perimeter
    #[command(name = "show-current")]
    ShowCurrent,

    /// Switch current perimeter
    Switch {
        /// Show output in the specified format
        #[arg(long, short = 'o')]
        output: Option<String>,

        /// Perimeter name to switch to
        #[arg(long)]
        id: Option<String>,

        /// Force change the existing perimeter
        #[arg(long, short = 'f')]
        force: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum AppCommands {
    /// List apps in the Outerbounds Platform
    List {
        /// Filter apps by project
        #[arg(long)]
        project: Option<String>,

        /// Filter apps by branch
        #[arg(long)]
        branch: Option<String>,

        /// Filter apps by name
        #[arg(long)]
        name: Option<String>,

        /// Filter apps by tag. Format KEY=VALUE
        #[arg(long = "tag", value_name = "KEY=VALUE")]
        tags: Vec<String>,

        /// Format the output
        #[arg(long, value_parser = ["json", "text"])]
        format: Option<String>,

        /// Filter apps by Auth type
        #[arg(long, value_parser = ["Browser", "API", "BrowserAndApi"])]
        auth_type: Option<String>,
    },

    /// Get detailed information about an app from the Outerbounds Platform
    Info {
        /// App ID
        id: String,

        /// Format the output
        #[arg(long, value_parser = ["json", "text"])]
        format: Option<String>,
    },

    /// Delete an app/apps from the Outerbounds Platform
    Delete {
        /// App IDs to delete
        #[arg(required = true)]
        ids: Vec<String>,
    },

    /// Get logs for an app worker from the Outerbounds Platform
    Logs {
        /// App ID
        id: String,

        /// Worker ID (defaults to first worker)
        #[arg(long)]
        worker_id: Option<String>,

        /// Get logs from previous container instance
        #[arg(long)]
        previous: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum IntegrationsCommands {
    /// List all resource integrations
    List {
        /// Override the current perimeter
        #[arg(long)]
        perimeter: Option<String>,
    },

    /// Get a specific resource integration
    Get {
        /// Integration name
        name: String,

        /// Override the current perimeter
        #[arg(long)]
        perimeter: Option<String>,
    },

    /// Delete a resource integration
    Delete {
        /// Integration name
        name: String,

        /// Override the current perimeter
        #[arg(long)]
        perimeter: Option<String>,
    },

    /// List all private PyPI repositories
    #[command(name = "list-private-pypi")]
    ListPrivatePypi {
        /// Override the current perimeter
        #[arg(long)]
        perimeter: Option<String>,
    },

    /// List all private Conda channels
    #[command(name = "list-private-conda")]
    ListPrivateConda {
        /// Override the current perimeter
        #[arg(long)]
        perimeter: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum FlowprojectCommands {
    /// Get flowproject metadata for a project/branch
    #[command(name = "get-metadata")]
    GetMetadata {
        /// Project name
        #[arg(long)]
        project: String,

        /// Branch name
        #[arg(long)]
        branch: String,
    },

    /// Delete flowproject metadata for a project/branch
    #[command(name = "delete-metadata")]
    DeleteMetadata {
        /// Project name
        #[arg(long)]
        project: String,

        /// Branch name
        #[arg(long)]
        branch: String,
    },

    /// List deployed workflow templates for a project/branch
    #[command(name = "list-templates")]
    ListTemplates {
        /// Project name
        #[arg(long)]
        project: String,

        /// Branch name
        #[arg(long)]
        branch: String,
    },

    /// Tear down all deployed resources for a project/branch
    #[command(name = "teardown-branch")]
    TeardownBranch {
        /// Project name
        #[arg(long)]
        project: String,

        /// Branch name
        #[arg(long)]
        branch: String,

        /// Show what would be deleted without actually deleting
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum SecretsCommands {
    /// Get secret metadata (backend type, resource ID)
    #[command(name = "get-metadata")]
    GetMetadata {
        /// Integration name
        integration_name: String,
    },

    /// Fetch secret values from cloud secret manager
    Get {
        /// Integration name
        integration_name: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Fetch multiple secrets at once
    #[command(name = "get-many")]
    GetMany {
        /// Integration names
        #[arg(required = true)]
        integration_names: Vec<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum TutorialsCommands {
    /// Download and extract tutorials
    Pull {
        /// URL to download tutorials from
        url: String,

        /// Destination directory (defaults to current directory)
        #[arg(long)]
        destination: Option<String>,

        /// Expected SHA256 hash for verification
        #[arg(long)]
        verify_hash: Option<String>,

        /// Overwrite existing files
        #[arg(long, short = 'f')]
        force: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum WorkstationsCommands {
    /// List all workstations
    List,

    /// Hibernate a workstation
    Hibernate {
        /// Workstation ID
        workstation_id: String,
    },

    /// Restart a workstation
    Restart {
        /// Workstation ID
        workstation_id: String,
    },

    /// Generate kubectl ExecCredential token
    #[command(name = "generate-token")]
    GenerateToken,

    /// Get Kubernetes namespace for a workstation
    #[command(name = "get-namespace")]
    GetNamespace {
        /// Workstation ID
        workstation_id: String,
    },

    /// Get relevant links (Metaflow UI, etc.)
    #[command(name = "get-links")]
    GetLinks,

    /// Configure kubeconfig for workstation access
    #[command(name = "configure-kubeconfig")]
    ConfigureKubeconfig {
        /// Path to ana binary (for exec credential plugin)
        #[arg(long)]
        binary_path: Option<String>,

        /// Path to kubeconfig file
        #[arg(long)]
        kubeconfig_path: Option<String>,
    },

    /// Configure SSH access to a workstation
    #[command(name = "prepare-ssh")]
    PrepareSsh {
        /// Workstation ID
        workstation_id: String,
    },

    /// Install kubectl binary
    #[command(name = "install-kubectl")]
    InstallKubectl {
        /// Installation directory
        #[arg(long)]
        install_dir: Option<String>,

        /// kubectl version to install
        #[arg(long)]
        version: Option<String>,
    },
}

impl ObnCommands {
    pub fn into_action(self) -> ObnAction {
        match self {
            ObnCommands::Configure {
                encoded_config,
                echo,
                force,
            } => ObnAction::Configure {
                encoded_config,
                echo,
                force,
            },
            ObnCommands::ServicePrincipalConfigure {
                name,
                deployment_domain,
                perimeter,
                jwt_token,
                github_actions,
                echo,
                force,
            } => ObnAction::ServicePrincipalConfigure {
                name,
                deployment_domain,
                perimeter,
                jwt_token,
                github_actions,
                echo,
                force,
            },
            ObnCommands::Check {
                no_config,
                output,
                workstation,
                latency,
                latency_requests,
                latency_timeout,
            } => ObnAction::Check {
                no_config,
                output,
                workstation,
                latency,
                latency_requests,
                latency_timeout,
            },
            ObnCommands::Perimeter { command } => match command {
                None => ObnAction::ShowHelp("obn perimeter".to_string()),
                Some(PerimeterCommands::List) => ObnAction::PerimeterList,
                Some(PerimeterCommands::ShowCurrent) => ObnAction::PerimeterShowCurrent,
                Some(PerimeterCommands::Switch { output, id, force }) => {
                    ObnAction::PerimeterSwitch { output, id, force }
                }
            },
            ObnCommands::App { command } => match command {
                None => ObnAction::ShowHelp("obn app".to_string()),
                Some(AppCommands::List {
                    project,
                    branch,
                    name,
                    tags,
                    format,
                    auth_type,
                }) => ObnAction::AppList {
                    project,
                    branch,
                    name,
                    tags,
                    format,
                    auth_type,
                },
                Some(AppCommands::Info { id, format }) => ObnAction::AppInfo { id, format },
                Some(AppCommands::Delete { ids }) => ObnAction::AppDelete { ids },
                Some(AppCommands::Logs {
                    id,
                    worker_id,
                    previous,
                }) => ObnAction::AppLogs {
                    id,
                    worker_id,
                    previous,
                },
            },
            ObnCommands::Integrations { command } => match command {
                None => ObnAction::ShowHelp("obn integrations".to_string()),
                Some(IntegrationsCommands::List { perimeter }) => {
                    ObnAction::IntegrationsList { perimeter }
                }
                Some(IntegrationsCommands::Get { name, perimeter }) => {
                    ObnAction::IntegrationsGet { name, perimeter }
                }
                Some(IntegrationsCommands::Delete { name, perimeter }) => {
                    ObnAction::IntegrationsDelete { name, perimeter }
                }
                Some(IntegrationsCommands::ListPrivatePypi { perimeter }) => {
                    ObnAction::IntegrationsListPrivatePypi { perimeter }
                }
                Some(IntegrationsCommands::ListPrivateConda { perimeter }) => {
                    ObnAction::IntegrationsListPrivateConda { perimeter }
                }
            },
            ObnCommands::Flowproject { command } => match command {
                None => ObnAction::ShowHelp("obn flowproject".to_string()),
                Some(FlowprojectCommands::GetMetadata { project, branch }) => {
                    ObnAction::FlowprojectGetMetadata { project, branch }
                }
                Some(FlowprojectCommands::DeleteMetadata { project, branch }) => {
                    ObnAction::FlowprojectDeleteMetadata { project, branch }
                }
                Some(FlowprojectCommands::ListTemplates { project, branch }) => {
                    ObnAction::FlowprojectListTemplates { project, branch }
                }
                Some(FlowprojectCommands::TeardownBranch {
                    project,
                    branch,
                    dry_run,
                }) => ObnAction::FlowprojectTeardownBranch {
                    project,
                    branch,
                    dry_run,
                },
            },
            ObnCommands::Secrets { command } => match command {
                None => ObnAction::ShowHelp("obn secrets".to_string()),
                Some(SecretsCommands::GetMetadata { integration_name }) => {
                    ObnAction::SecretsGetMetadata { integration_name }
                }
                Some(SecretsCommands::Get {
                    integration_name,
                    json,
                }) => ObnAction::SecretsGet {
                    integration_name,
                    json,
                },
                Some(SecretsCommands::GetMany {
                    integration_names,
                    json,
                }) => ObnAction::SecretsGetMany {
                    integration_names,
                    json,
                },
            },
            ObnCommands::Tutorials { command } => match command {
                None => ObnAction::ShowHelp("obn tutorials".to_string()),
                Some(TutorialsCommands::Pull {
                    url,
                    destination,
                    verify_hash,
                    force,
                }) => ObnAction::TutorialsPull {
                    url,
                    destination,
                    verify_hash,
                    force,
                },
            },
            ObnCommands::Workstations { command } => match command {
                None => ObnAction::ShowHelp("obn workstations".to_string()),
                Some(WorkstationsCommands::List) => ObnAction::WorkstationsList,
                Some(WorkstationsCommands::Hibernate { workstation_id }) => {
                    ObnAction::WorkstationsHibernate { workstation_id }
                }
                Some(WorkstationsCommands::Restart { workstation_id }) => {
                    ObnAction::WorkstationsRestart { workstation_id }
                }
                Some(WorkstationsCommands::GenerateToken) => ObnAction::WorkstationsGenerateToken,
                Some(WorkstationsCommands::GetNamespace { workstation_id }) => {
                    ObnAction::WorkstationsGetNamespace { workstation_id }
                }
                Some(WorkstationsCommands::GetLinks) => ObnAction::WorkstationsGetLinks,
                Some(WorkstationsCommands::ConfigureKubeconfig {
                    binary_path,
                    kubeconfig_path,
                }) => ObnAction::WorkstationsConfigureKubeconfig {
                    binary_path,
                    kubeconfig_path,
                },
                Some(WorkstationsCommands::PrepareSsh { workstation_id }) => {
                    ObnAction::WorkstationsPrepareSsh { workstation_id }
                }
                Some(WorkstationsCommands::InstallKubectl {
                    install_dir,
                    version,
                }) => ObnAction::WorkstationsInstallKubectl {
                    install_dir,
                    version,
                },
            },
        }
    }
}
