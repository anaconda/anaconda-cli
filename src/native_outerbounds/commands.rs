use clap::Subcommand;
use outerbounds::commands::{DeployOptions, ReadinessCondition};

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
        from_obproject_toml: bool,
        toml_path: String,
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
    PerimeterList {
        output: Option<String>,
    },
    PerimeterShowCurrent {
        output: Option<String>,
    },
    PerimeterEnsureCloudCreds {
        cspr_override: Option<String>,
        output: Option<String>,
    },
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
        id: Option<String>,
        name: Option<String>,
        project: Option<String>,
        branch: Option<String>,
        format: Option<String>,
    },
    AppDelete {
        ids: Vec<String>,
        name: Option<String>,
        project: Option<String>,
        branch: Option<String>,
        tags: Vec<String>,
        auto_approve: bool,
    },
    AppDeploy {
        options: Box<DeployOptions>,
        status_file: Option<String>,
    },
    AppLogs {
        id: Option<String>,
        name: Option<String>,
        project: Option<String>,
        branch: Option<String>,
        worker_id: Option<String>,
        previous: bool,
        file: Option<String>,
    },

    // Integrations
    IntegrationsList {
        perimeter: Option<String>,
    },
    IntegrationsGet {
        name: String,
        perimeter: Option<String>,
        show_secret_values: bool,
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

    // Integration creation
    IntegrationsAnacondaCreate {
        name: String,
        description: Option<String>,
        perimeter: Option<String>,
    },
    IntegrationsArtifactoryCreate {
        name: String,
        description: Option<String>,
        url: String,
        username: String,
        password: String,
        perimeter: Option<String>,
    },
    IntegrationsAzureArtifactsCreate {
        name: String,
        description: Option<String>,
        organization: String,
        project: String,
        feed: String,
        username: String,
        pat: String,
        perimeter: Option<String>,
    },
    IntegrationsCodeArtifactsCreate {
        name: String,
        description: Option<String>,
        domain_name: String,
        domain_owner: String,
        aws_region: String,
        target_role: Option<String>,
        perimeter: Option<String>,
    },
    IntegrationsContainerRegistryCreate {
        name: String,
        description: Option<String>,
        registry_domain: String,
        target_role_arn: Option<String>,
        use_task_role: bool,
        username: Option<String>,
        password: Option<String>,
        perimeter: Option<String>,
    },
    IntegrationsCustomSecretCreate {
        name: String,
        description: Option<String>,
        secrets: Vec<String>,
        perimeter: Option<String>,
    },
    IntegrationsCustomSecretUpdate {
        name: String,
        description: Option<String>,
        secrets: Vec<String>,
        perimeter: Option<String>,
    },
    IntegrationsGitPypiRepositoryCreate {
        name: String,
        description: Option<String>,
        repository_url: String,
        username: Option<String>,
        password: Option<String>,
        perimeter: Option<String>,
    },
    IntegrationsGitlabArtifactsCreate {
        name: String,
        description: Option<String>,
        gitlab_url: String,
        project_id: String,
        username: Option<String>,
        password: Option<String>,
        perimeter: Option<String>,
    },
    IntegrationsPrivateCondaChannelsAdd {
        channel_name: String,
        host_integration_name: String,
        is_default: bool,
        perimeter: Option<String>,
    },
    IntegrationsPrivatePypiRepositoriesAdd {
        repository_name: String,
        host_integration_name: String,
        is_default: bool,
        perimeter: Option<String>,
    },
    IntegrationsS3ProxyCreate {
        name: String,
        description: Option<String>,
        bucket_name: String,
        endpoint_url: String,
        region: String,
        access_key_id: String,
        secret_access_key: String,
        perimeter: Option<String>,
    },

    // Integration updates
    IntegrationsS3ProxyUpdate {
        name: String,
        description: Option<String>,
        bucket_name: Option<String>,
        endpoint_url: Option<String>,
        region: Option<String>,
        access_key_id: Option<String>,
        secret_access_key: Option<String>,
        perimeter: Option<String>,
    },
    IntegrationsCodeArtifactsUpdate {
        name: String,
        description: Option<String>,
        domain_name: Option<String>,
        domain_owner: Option<String>,
        aws_region: Option<String>,
        target_role: Option<String>,
        perimeter: Option<String>,
    },
    IntegrationsArtifactoryUpdate {
        name: String,
        description: Option<String>,
        url: Option<String>,
        username: Option<String>,
        password: Option<String>,
        perimeter: Option<String>,
    },
    IntegrationsAzureArtifactsUpdate {
        name: String,
        description: Option<String>,
        organization: Option<String>,
        project: Option<String>,
        username: Option<String>,
        pat: Option<String>,
        perimeter: Option<String>,
    },
    IntegrationsGitlabArtifactsUpdate {
        name: String,
        description: Option<String>,
        gitlab_url: Option<String>,
        project_id: Option<String>,
        username: Option<String>,
        password: Option<String>,
        perimeter: Option<String>,
    },
    IntegrationsContainerRegistryUpdate {
        name: String,
        description: Option<String>,
        registry_domain: Option<String>,
        target_role_arn: Option<String>,
        use_task_role: bool,
        username: Option<String>,
        password: Option<String>,
        perimeter: Option<String>,
    },
    IntegrationsGitPypiRepositoryUpdate {
        name: String,
        description: Option<String>,
        repository_urls: Vec<String>,
        username: Option<String>,
        password: Option<String>,
        perimeter: Option<String>,
    },
    IntegrationsPrivateCondaChannelsRemove {
        channel_name: String,
        perimeter: Option<String>,
    },
    IntegrationsPrivatePypiRepositoriesRemove {
        repository_name: String,
        perimeter: Option<String>,
    },

    // Fast Bakery
    FastBakeryGetLoginPassword,
    FastBakeryConfigureDockerLogin {
        registry_url: String,
        output: Option<String>,
    },

    // Kubernetes
    KubernetesKill {
        flow_name: String,
        run_id: Option<String>,
        my_runs: bool,
        dry_run: bool,
        auto_approve: bool,
        clear_everything: bool,
    },

    // FlowProject
    FlowprojectGetMetadata {
        id: Option<String>,
    },
    FlowprojectSetMetadata {
        json: String,
    },
    FlowprojectDeleteMetadata {
        id: String,
        yes: bool,
        output: Option<String>,
    },
    FlowprojectListTemplates {
        id: String,
        output: Option<String>,
    },
    FlowprojectTeardownBranch {
        id: String,
        dry_run: bool,
        yes: bool,
        output: Option<String>,
    },

    // Secrets
    SecretsGet {
        secret_ids: Vec<String>,
        format: Option<String>,
        role: Option<String>,
        file: Option<String>,
    },

    // Tutorials
    TutorialsPull {
        url: String,
        destination_dir: String,
        force_overwrite: bool,
    },

    // Workstations
    WorkstationsList {
        output: Option<String>,
    },
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
    WorkstationsGetLinks {
        perimeter_id: Option<String>,
        output: Option<String>,
    },
    WorkstationsConfigureKubeconfig {
        binary_path: Option<String>,
        kubeconfig_path: Option<String>,
    },
    WorkstationsPrepareSsh {
        workstation_id: String,
        setup_context: String,
        mode: String,
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

        /// Read --name, --deployment-domain, and --perimeter from obproject.toml if not provided
        #[arg(long)]
        from_obproject_toml: bool,

        /// Path to obproject.toml (used with --from-obproject-toml)
        #[arg(long, default_value = "obproject.toml")]
        toml_path: String,

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

    /// Commands for interacting with Fast Bakery
    #[command(name = "fast-bakery", subcommand_required = false, arg_required_else_help = false)]
    FastBakery {
        #[command(subcommand)]
        command: Option<FastBakeryCommands>,
    },

    /// Commands for interacting with Kubernetes
    #[command(subcommand_required = false, arg_required_else_help = false)]
    Kubernetes {
        #[command(subcommand)]
        command: Option<KubernetesCommands>,
    },
}

#[derive(Subcommand, Debug)]
pub enum FastBakeryCommands {
    /// Get the Docker login password for the Fast Bakery registry
    #[command(name = "get-login-password")]
    GetLoginPassword,

    /// Configure Docker login for the Fast Bakery registry
    #[command(name = "configure-docker-login")]
    ConfigureDockerLogin {
        /// Fast Bakery registry URL (without http:// prefix)
        #[arg(long)]
        registry_url: String,

        /// Path to the Docker config file
        #[arg(long)]
        output: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum KubernetesCommands {
    /// Kill pods/jobs/jobsets for a specific flow
    Kill {
        /// Flow name to kill pods for
        #[arg(long)]
        flow_name: String,

        /// Specific run ID to kill pods for
        #[arg(long)]
        run_id: Option<String>,

        /// Only kill runs by the current user
        #[arg(long)]
        my_runs: bool,

        /// Show what would be killed without actually killing
        #[arg(long)]
        dry_run: bool,

        /// Skip confirmation prompt
        #[arg(long)]
        auto_approve: bool,

        /// Force delete ALL matching resources regardless of their status
        #[arg(long)]
        clear_everything: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum PerimeterCommands {
    /// List all available perimeters
    List {
        /// Show output in the specified format
        #[arg(long, short = 'o')]
        output: Option<String>,
    },

    /// Show current perimeter
    #[command(name = "show-current")]
    ShowCurrent {
        /// Show output in the specified format
        #[arg(long, short = 'o')]
        output: Option<String>,
    },

    /// Ensure cloud credentials are set up for the current shell (workstation only)
    #[command(name = "ensure-cloud-creds")]
    EnsureCloudCreds {
        /// CSPR role ARN to use instead of the platform-provided one
        #[arg(long)]
        cspr_override: Option<String>,

        /// Show output in the specified format
        #[arg(long, short = 'o')]
        output: Option<String>,
    },

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
        id: Option<String>,

        /// Get info for app by name
        #[arg(long)]
        name: Option<String>,

        /// Scope app lookup by project
        #[arg(long)]
        project: Option<String>,

        /// Scope app lookup by branch
        #[arg(long)]
        branch: Option<String>,

        /// Format the output
        #[arg(long, value_parser = ["json", "text"])]
        format: Option<String>,
    },

    /// Delete an app/apps from the Outerbounds Platform
    Delete {
        /// App IDs to delete
        ids: Vec<String>,

        /// Filter app to delete by name
        #[arg(long)]
        name: Option<String>,

        /// Filter apps to delete by project
        #[arg(long)]
        project: Option<String>,

        /// Filter apps to delete by branch
        #[arg(long)]
        branch: Option<String>,

        /// Filter apps to delete by tag. Format KEY=VALUE
        #[arg(long = "tag", value_name = "KEY=VALUE")]
        tags: Vec<String>,

        /// Do not prompt for confirmation
        #[arg(long)]
        auto_approve: bool,
    },

    /// Deploy an app to the Outerbounds Platform
    #[command(trailing_var_arg = true)]
    Deploy(Box<AppDeployArgs>),

    /// Get logs for an app worker from the Outerbounds Platform
    Logs {
        /// App ID
        id: Option<String>,

        /// Get logs for app by name
        #[arg(long)]
        name: Option<String>,

        /// Scope app lookup by project
        #[arg(long)]
        project: Option<String>,

        /// Scope app lookup by branch
        #[arg(long)]
        branch: Option<String>,

        /// Worker ID (defaults to first worker)
        #[arg(long)]
        worker_id: Option<String>,

        /// Get logs from previous container instance
        #[arg(long)]
        previous: bool,

        /// Save logs to file
        #[arg(long)]
        file: Option<String>,
    },
}

#[derive(clap::Args, Debug)]
pub struct AppDeployArgs {
    /// Path to config file (YAML or JSON)
    #[arg(long)]
    config_file: Option<String>,

    /// Source directory to package (defaults to current directory)
    #[arg(long)]
    src_dir: Option<String>,

    /// App name
    #[arg(long)]
    name: Option<String>,

    /// Port the app listens on
    #[arg(long)]
    port: Option<u16>,

    /// App description
    #[arg(long)]
    description: Option<String>,

    /// App endpoint type
    #[arg(long)]
    app_type: Option<String>,

    /// Docker image to use as base
    #[arg(long)]
    image: Option<String>,

    /// Command to run the app (after --)
    #[arg(allow_hyphen_values = true)]
    commands: Vec<String>,

    /// Path to requirements.txt (relative to src_dir)
    #[arg(long)]
    dep_from_requirements: Option<String>,

    /// Path to pyproject.toml (relative to src_dir)
    #[arg(long)]
    dep_from_pyproject: Option<String>,

    /// Python version
    #[arg(long)]
    python: Option<String>,

    /// Skip dependency baking
    #[arg(long)]
    no_deps: bool,

    /// CPU allocation (e.g., "1", "500m")
    #[arg(long)]
    cpu: Option<String>,

    /// Memory allocation (e.g., "2048Mi", "4Gi")
    #[arg(long)]
    memory: Option<String>,

    /// GPU allocation (e.g., "1")
    #[arg(long)]
    gpu: Option<String>,

    /// Disk size
    #[arg(long)]
    disk: Option<String>,

    /// Shared memory size
    #[arg(long)]
    shared_memory: Option<String>,

    /// Fixed number of replicas (mutually exclusive with min/max)
    #[arg(long, conflicts_with_all = ["min_replicas", "max_replicas"])]
    fixed_replicas: Option<u32>,

    /// Minimum replicas
    #[arg(long)]
    min_replicas: Option<u32>,

    /// Maximum replicas
    #[arg(long)]
    max_replicas: Option<u32>,

    /// Scaling RPM threshold
    #[arg(long)]
    scaling_rpm: Option<u32>,

    /// Auth type
    #[arg(long, value_parser = ["Browser", "API", "BrowserAndApi"])]
    auth_type: Option<String>,

    /// Public access
    #[arg(long, overrides_with = "private_access")]
    public_access: bool,

    /// Private access
    #[arg(long)]
    private_access: bool,

    /// Secret integrations to attach
    #[arg(long = "secret")]
    secrets: Vec<String>,

    /// Tags as KEY=VALUE
    #[arg(long = "tag", value_name = "KEY=VALUE")]
    tags: Vec<String>,

    /// Environment variables as KEY=VALUE
    #[arg(long = "env", value_name = "KEY=VALUE")]
    env: Vec<String>,

    /// Compute pools
    #[arg(long = "compute-pools")]
    compute_pools: Vec<String>,

    /// Readiness condition
    #[arg(long, value_parser = ["at_least_one_running", "all_running", "fully_finished", "async"])]
    readiness_condition: Option<String>,

    /// Deployment timeout in seconds
    #[arg(long)]
    deployment_timeout: Option<u64>,

    /// Time (in seconds) to monitor the deployment for readiness after the readiness condition is met
    #[arg(long)]
    readiness_wait_time: Option<u64>,

    /// Force upgrade even if an update is in progress
    #[arg(long)]
    force_upgrade: bool,

    /// Skip code packaging (use image's embedded code)
    #[arg(long)]
    skip_code_package: bool,

    /// Path to the source code to deploy with the app (can be specified multiple times)
    #[arg(long = "package-src-path")]
    package_src_paths: Vec<String>,

    /// File suffixes to include in the code package (comma-separated)
    #[arg(long)]
    package_suffixes: Option<String>,

    /// Project name
    #[arg(long)]
    project: Option<String>,

    /// Branch name
    #[arg(long)]
    branch: Option<String>,

    /// Path to a file where the final deployment status will be written
    #[arg(long)]
    status_file: Option<String>,
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

        /// Fetch and display secret values from the cloud provider
        #[arg(long)]
        show_secret_values: bool,
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
    #[command(name = "list-private-pypi-repositories")]
    ListPrivatePypiRepositories {
        /// Override the current perimeter
        #[arg(long)]
        perimeter: Option<String>,
    },

    /// List all private Conda channels
    #[command(name = "list-private-conda-channels")]
    ListPrivateConda {
        /// Override the current perimeter
        #[arg(long)]
        perimeter: Option<String>,
    },

    /// Create an Anaconda integration
    #[command(subcommand)]
    Anaconda(AnacondaCommands),

    /// Create an Artifactory integration
    #[command(subcommand)]
    Artifactory(ArtifactoryCommands),

    /// Create an Azure Artifacts integration
    #[command(name = "azure-artifacts", subcommand)]
    AzureArtifacts(AzureArtifactsCommands),

    /// Create a CodeArtifacts integration
    #[command(name = "code-artifacts", subcommand)]
    CodeArtifacts(CodeArtifactsCommands),

    /// Create a Container Registry integration
    #[command(name = "container-registry", subcommand)]
    ContainerRegistry(ContainerRegistryCommands),

    /// Create a Custom Secret integration
    #[command(name = "custom-secret", subcommand)]
    CustomSecret(CustomSecretCommands),

    /// Create a Git PyPI Repository integration
    #[command(name = "git-pypi-repository", subcommand)]
    GitPypiRepository(GitPypiRepositoryCommands),

    /// Create a GitLab Artifacts integration
    #[command(name = "gitlab-artifacts", subcommand)]
    GitlabArtifacts(GitlabArtifactsCommands),

    /// Add private Conda channels
    #[command(name = "private-conda-channels", subcommand)]
    PrivateCondaChannels(PrivateCondaChannelsCommands),

    /// Add private PyPI repositories
    #[command(name = "private-pypi-repositories", subcommand)]
    PrivatePypiRepositories(PrivatePypiRepositoriesCommands),

    /// Create an S3 Proxy integration
    #[command(name = "s3-proxy", subcommand)]
    S3Proxy(S3ProxyCommands),
}

// Integration subcommands

#[derive(Subcommand, Debug)]
pub enum AnacondaCommands {
    /// Create an Anaconda integration
    Create {
        /// Integration name
        name: String,
        /// Description of the integration
        #[arg(long)]
        description: Option<String>,
        /// Perimeter ID
        #[arg(long)]
        perimeter: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ArtifactoryCommands {
    /// Create an Artifactory integration
    Create {
        /// Integration name
        name: String,
        /// Description of the integration
        #[arg(long)]
        description: Option<String>,
        /// Artifactory URL
        #[arg(long)]
        url: String,
        /// Username for authentication
        #[arg(long)]
        username: String,
        /// Password/API key for authentication
        #[arg(long)]
        password: String,
        /// Perimeter ID
        #[arg(long)]
        perimeter: Option<String>,
    },
    /// Update an Artifactory integration
    Update {
        /// Integration name
        name: String,
        /// Description of the integration
        #[arg(long)]
        description: Option<String>,
        /// Artifactory URL
        #[arg(long)]
        url: Option<String>,
        /// Username for authentication
        #[arg(long)]
        username: Option<String>,
        /// Password/API key for authentication
        #[arg(long)]
        password: Option<String>,
        /// Perimeter ID
        #[arg(long)]
        perimeter: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum AzureArtifactsCommands {
    /// Create an Azure Artifacts integration
    Create {
        /// Integration name
        name: String,
        /// Description of the integration
        #[arg(long)]
        description: Option<String>,
        /// Azure DevOps organization
        #[arg(long)]
        organization: String,
        /// Azure DevOps project
        #[arg(long)]
        project: String,
        /// Azure Artifacts feed name
        #[arg(long)]
        feed: String,
        /// Username for authentication
        #[arg(long)]
        username: String,
        /// Personal Access Token
        #[arg(long)]
        pat: String,
        /// Perimeter ID
        #[arg(long)]
        perimeter: Option<String>,
    },
    /// Update an Azure Artifacts integration
    Update {
        /// Integration name
        name: String,
        /// Description of the integration
        #[arg(long)]
        description: Option<String>,
        /// Azure DevOps organization
        #[arg(long)]
        organization: Option<String>,
        /// Azure DevOps project
        #[arg(long)]
        project: Option<String>,
        /// Username for authentication
        #[arg(long)]
        username: Option<String>,
        /// Personal Access Token
        #[arg(long)]
        pat: Option<String>,
        /// Perimeter ID
        #[arg(long)]
        perimeter: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum CodeArtifactsCommands {
    /// Create a CodeArtifacts integration
    Create {
        /// Integration name
        name: String,
        /// Description of the integration
        #[arg(long)]
        description: Option<String>,
        /// CodeArtifacts domain name
        #[arg(long)]
        domain_name: String,
        /// CodeArtifacts domain owner (AWS account ID)
        #[arg(long)]
        domain_owner: String,
        /// AWS region
        #[arg(long)]
        aws_region: String,
        /// Target IAM role ARN for cross-account access
        #[arg(long)]
        target_role: Option<String>,
        /// Perimeter ID
        #[arg(long)]
        perimeter: Option<String>,
    },
    /// Update a CodeArtifacts integration
    Update {
        /// Integration name
        name: String,
        /// Description of the integration
        #[arg(long)]
        description: Option<String>,
        /// CodeArtifacts domain name
        #[arg(long)]
        domain_name: Option<String>,
        /// CodeArtifacts domain owner (AWS account ID)
        #[arg(long)]
        domain_owner: Option<String>,
        /// AWS region
        #[arg(long)]
        aws_region: Option<String>,
        /// Target IAM role ARN for cross-account access
        #[arg(long)]
        target_role: Option<String>,
        /// Perimeter ID
        #[arg(long)]
        perimeter: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ContainerRegistryCommands {
    /// Create a Container Registry integration
    Create {
        /// Integration name
        name: String,
        /// Description of the integration
        #[arg(long)]
        description: Option<String>,
        /// Registry domain (e.g., docker.io, gcr.io)
        #[arg(long)]
        registry_domain: String,
        /// Target Role ARN (for AWS ECR authentication)
        #[arg(long)]
        target_role_arn: Option<String>,
        /// Use the task's IAM role for authentication (for AWS ECR)
        #[arg(long)]
        use_task_role: bool,
        /// Username for registry authentication (for non-ECR registries)
        #[arg(long)]
        username: Option<String>,
        /// Password for registry authentication (for non-ECR registries)
        #[arg(long)]
        password: Option<String>,
        /// Perimeter ID
        #[arg(long)]
        perimeter: Option<String>,
    },
    /// Update a Container Registry integration
    Update {
        /// Integration name
        name: String,
        /// Description of the integration
        #[arg(long)]
        description: Option<String>,
        /// Registry domain (e.g., docker.io, gcr.io)
        #[arg(long)]
        registry_domain: Option<String>,
        /// Target Role ARN (for AWS ECR authentication)
        #[arg(long)]
        target_role_arn: Option<String>,
        /// Use the task's IAM role for authentication (for AWS ECR)
        #[arg(long)]
        use_task_role: bool,
        /// Username for registry authentication (for non-ECR registries)
        #[arg(long)]
        username: Option<String>,
        /// Password for registry authentication (for non-ECR registries)
        #[arg(long)]
        password: Option<String>,
        /// Perimeter ID
        #[arg(long)]
        perimeter: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum CustomSecretCommands {
    /// Create a Custom Secret integration
    Create {
        /// Integration name
        name: String,
        /// Description of the integration
        #[arg(long)]
        description: Option<String>,
        /// Secret key-value pairs (format: key=value)
        #[arg(long = "secret", short = 's')]
        secrets: Vec<String>,
        /// Perimeter ID
        #[arg(long)]
        perimeter: Option<String>,
    },
    /// Update a Custom Secret integration
    Update {
        /// Integration name
        name: String,
        /// Description of the integration
        #[arg(long)]
        description: Option<String>,
        /// Secret key-value pairs (format: key=value)
        #[arg(long = "secret", short = 's')]
        secrets: Vec<String>,
        /// Perimeter ID
        #[arg(long)]
        perimeter: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum GitPypiRepositoryCommands {
    /// Create a Git PyPI Repository integration
    Create {
        /// Integration name
        name: String,
        /// Description of the integration
        #[arg(long)]
        description: Option<String>,
        /// Git repository URL
        #[arg(long)]
        repository_url: String,
        /// Username for authentication
        #[arg(long)]
        username: Option<String>,
        /// Password for authentication
        #[arg(long)]
        password: Option<String>,
        /// Perimeter ID
        #[arg(long)]
        perimeter: Option<String>,
    },
    /// Update a Git PyPI Repository integration
    Update {
        /// Integration name
        name: String,
        /// Description of the integration
        #[arg(long)]
        description: Option<String>,
        /// Git repository URL (can be specified multiple times)
        #[arg(long = "repository-url", short = 'r')]
        repository_urls: Vec<String>,
        /// Username for authentication
        #[arg(long)]
        username: Option<String>,
        /// Password for authentication
        #[arg(long)]
        password: Option<String>,
        /// Perimeter ID
        #[arg(long)]
        perimeter: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum GitlabArtifactsCommands {
    /// Create a GitLab Artifacts integration
    Create {
        /// Integration name
        name: String,
        /// Description of the integration
        #[arg(long)]
        description: Option<String>,
        /// GitLab URL (defaults to gitlab.com)
        #[arg(long, default_value = "gitlab.com")]
        gitlab_url: String,
        /// GitLab project ID
        #[arg(long)]
        project_id: String,
        /// Username for authentication
        #[arg(long)]
        username: Option<String>,
        /// Password/token for authentication
        #[arg(long)]
        password: Option<String>,
        /// Perimeter ID
        #[arg(long)]
        perimeter: Option<String>,
    },
    /// Update a GitLab Artifacts integration
    Update {
        /// Integration name
        name: String,
        /// Description of the integration
        #[arg(long)]
        description: Option<String>,
        /// GitLab URL
        #[arg(long)]
        gitlab_url: Option<String>,
        /// GitLab project ID
        #[arg(long)]
        project_id: Option<String>,
        /// Username for authentication
        #[arg(long)]
        username: Option<String>,
        /// Password/token for authentication
        #[arg(long)]
        password: Option<String>,
        /// Perimeter ID
        #[arg(long)]
        perimeter: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum PrivateCondaChannelsCommands {
    /// Add a private Conda channel
    Add {
        /// Channel name
        #[arg(long)]
        channel_name: String,
        /// Host integration name (credentials)
        #[arg(long)]
        host_integration_name: String,
        /// Set as default channel
        #[arg(long)]
        is_default: bool,
        /// Perimeter ID
        #[arg(long)]
        perimeter: Option<String>,
    },
    /// Remove a private Conda channel
    Remove {
        /// Channel name to remove
        #[arg(long)]
        channel_name: String,
        /// Perimeter ID
        #[arg(long)]
        perimeter: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum PrivatePypiRepositoriesCommands {
    /// Add a private PyPI repository
    Add {
        /// Repository name
        #[arg(long)]
        repository_name: String,
        /// Host integration name (credentials)
        #[arg(long)]
        host_integration_name: String,
        /// Set as default repository
        #[arg(long)]
        is_default: bool,
        /// Perimeter ID
        #[arg(long)]
        perimeter: Option<String>,
    },
    /// Remove a private PyPI repository
    Remove {
        /// Repository name to remove
        #[arg(long)]
        repository_name: String,
        /// Perimeter ID
        #[arg(long)]
        perimeter: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum S3ProxyCommands {
    /// Create an S3 Proxy integration
    Create {
        /// Integration name
        name: String,
        /// Description of the integration
        #[arg(long)]
        description: Option<String>,
        /// S3 bucket name
        #[arg(long)]
        bucket_name: String,
        /// S3 endpoint URL
        #[arg(long)]
        endpoint_url: String,
        /// AWS region
        #[arg(long)]
        region: String,
        /// AWS Access Key ID
        #[arg(long)]
        access_key_id: String,
        /// AWS Secret Access Key
        #[arg(long)]
        secret_access_key: String,
        /// Perimeter ID
        #[arg(long)]
        perimeter: Option<String>,
    },
    /// Update an S3 Proxy integration
    Update {
        /// Integration name
        name: String,
        /// Description of the integration
        #[arg(long)]
        description: Option<String>,
        /// S3 bucket name
        #[arg(long)]
        bucket_name: Option<String>,
        /// S3 endpoint URL
        #[arg(long)]
        endpoint_url: Option<String>,
        /// AWS region
        #[arg(long)]
        region: Option<String>,
        /// AWS Access Key ID
        #[arg(long)]
        access_key_id: Option<String>,
        /// AWS Secret Access Key
        #[arg(long)]
        secret_access_key: Option<String>,
        /// Perimeter ID
        #[arg(long)]
        perimeter: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum FlowprojectCommands {
    /// Get flowproject metadata for a project/branch
    #[command(name = "get-metadata")]
    GetMetadata {
        /// The ID for this deployment
        #[arg(long)]
        id: Option<String>,
    },

    /// Set flowproject metadata
    #[command(name = "set-metadata")]
    SetMetadata {
        /// Metadata as a JSON string
        json: String,
    },

    /// Delete flowproject metadata for a project/branch
    #[command(name = "delete-metadata")]
    DeleteMetadata {
        /// project/branch identifier
        #[arg(long)]
        id: String,

        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,

        /// Output format
        #[arg(long, short = 'o')]
        output: Option<String>,
    },

    /// List deployed workflow templates for a project/branch
    #[command(name = "list-templates")]
    ListTemplates {
        /// project/branch identifier
        #[arg(long)]
        id: String,

        /// Output format
        #[arg(long, short = 'o')]
        output: Option<String>,
    },

    /// Tear down all deployed resources for a project/branch
    #[command(name = "teardown-branch")]
    TeardownBranch {
        /// project/branch identifier
        #[arg(long)]
        id: String,

        /// Print what would be deleted without deleting
        #[arg(long)]
        dry_run: bool,

        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,

        /// Output format
        #[arg(long, short = 'o')]
        output: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum SecretsCommands {
    /// Get secrets
    Get {
        /// Secret IDs (integration names)
        secret_ids: Vec<String>,

        /// Format of the output (text, json, shell)
        #[arg(long, value_parser = ["text", "json", "shell"])]
        format: Option<String>,

        /// Any additional IAM role required to access the secrets
        #[arg(long)]
        role: Option<String>,

        /// The file to write the output to
        #[arg(long, short = 'f')]
        file: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum TutorialsCommands {
    /// Pull Outerbounds tutorials
    Pull {
        /// URL to pull the tutorials from
        #[arg(long)]
        url: String,

        /// Directory to download tutorials to
        #[arg(long)]
        destination_dir: String,

        /// Overwrite all existing files across all tutorials
        #[arg(long)]
        force_overwrite: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum WorkstationsCommands {
    /// List all workstations
    List {
        /// Show output in the specified format
        #[arg(long, short = 'o', value_parser = ["json", "text"])]
        output: Option<String>,
    },

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
    GetLinks {
        /// The ID of the perimeter to use (defaults to the current perimeter)
        #[arg(long)]
        perimeter_id: Option<String>,

        /// Show output in the specified format
        #[arg(long, short = 'o', value_parser = ["json", "text"])]
        output: Option<String>,
    },

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
        /// Workstation ID (required for local setup)
        #[arg(default_value = "")]
        workstation_id: String,

        /// The context to use for the setup command
        #[arg(long, short = 'c', default_value = "local", value_parser = ["local", "remote"])]
        setup_context: String,

        /// The mode in which the command is being run
        #[arg(long, default_value = "workstation-connect", value_parser = ["workstation-connect", "workstation-init"])]
        mode: String,
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

impl AppDeployArgs {
    fn into_deploy_options(self) -> DeployOptions {
        let readiness_condition = self.readiness_condition.as_deref().map(|r| match r {
            "all_running" => ReadinessCondition::AllRunning,
            "fully_finished" => ReadinessCondition::FullyFinished,
            "async" => ReadinessCondition::Async,
            _ => ReadinessCondition::AtLeastOneRunning,
        });

        let public_access = if self.public_access {
            Some(true)
        } else if self.private_access {
            Some(false)
        } else {
            None
        };

        let non_empty = |v: Vec<String>| if v.is_empty() { None } else { Some(v) };

        DeployOptions {
            config_file: self.config_file,
            src_dir: self.src_dir,
            package_src_paths: non_empty(self.package_src_paths),
            package_suffixes: self
                .package_suffixes
                .map(|s| s.split(',').map(|p| p.trim().to_string()).collect()),
            name: self.name,
            port: self.port,
            description: self.description,
            app_type: self.app_type,
            image: self.image,
            commands: non_empty(self.commands),
            dep_from_requirements: self.dep_from_requirements,
            dep_from_pyproject: self.dep_from_pyproject,
            python: self.python,
            no_deps: self.no_deps,
            cpu: self.cpu,
            memory: self.memory,
            gpu: self.gpu,
            disk: self.disk,
            shared_memory: self.shared_memory,
            fixed_replicas: self.fixed_replicas,
            min_replicas: self.min_replicas,
            max_replicas: self.max_replicas,
            scaling_rpm: self.scaling_rpm,
            auth_type: self.auth_type,
            public_access,
            secrets: non_empty(self.secrets),
            tags: non_empty(self.tags),
            env: non_empty(self.env),
            compute_pools: non_empty(self.compute_pools),
            readiness_condition,
            deployment_timeout: self.deployment_timeout,
            readiness_wait_time: self.readiness_wait_time,
            force_upgrade: self.force_upgrade,
            skip_code_package: self.skip_code_package,
            project: self.project,
            branch: self.branch,
        }
    }
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
                from_obproject_toml,
                toml_path,
                echo,
                force,
            } => ObnAction::ServicePrincipalConfigure {
                name,
                deployment_domain,
                perimeter,
                jwt_token,
                github_actions,
                from_obproject_toml,
                toml_path,
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
                Some(PerimeterCommands::List { output }) => ObnAction::PerimeterList { output },
                Some(PerimeterCommands::ShowCurrent { output }) => {
                    ObnAction::PerimeterShowCurrent { output }
                }
                Some(PerimeterCommands::EnsureCloudCreds {
                    cspr_override,
                    output,
                }) => ObnAction::PerimeterEnsureCloudCreds {
                    cspr_override,
                    output,
                },
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
                Some(AppCommands::Info {
                    id,
                    name,
                    project,
                    branch,
                    format,
                }) => ObnAction::AppInfo {
                    id,
                    name,
                    project,
                    branch,
                    format,
                },
                Some(AppCommands::Delete {
                    ids,
                    name,
                    project,
                    branch,
                    tags,
                    auto_approve,
                }) => ObnAction::AppDelete {
                    ids,
                    name,
                    project,
                    branch,
                    tags,
                    auto_approve,
                },
                Some(AppCommands::Deploy(args)) => {
                    let status_file = args.status_file.clone();
                    ObnAction::AppDeploy {
                        options: Box::new(args.into_deploy_options()),
                        status_file,
                    }
                },
                Some(AppCommands::Logs {
                    id,
                    name,
                    project,
                    branch,
                    worker_id,
                    previous,
                    file,
                }) => ObnAction::AppLogs {
                    id,
                    name,
                    project,
                    branch,
                    worker_id,
                    previous,
                    file,
                },
            },
            ObnCommands::Integrations { command } => match command {
                None => ObnAction::ShowHelp("obn integrations".to_string()),
                Some(IntegrationsCommands::List { perimeter }) => {
                    ObnAction::IntegrationsList { perimeter }
                }
                Some(IntegrationsCommands::Get {
                    name,
                    perimeter,
                    show_secret_values,
                }) => ObnAction::IntegrationsGet {
                    name,
                    perimeter,
                    show_secret_values,
                },
                Some(IntegrationsCommands::Delete { name, perimeter }) => {
                    ObnAction::IntegrationsDelete { name, perimeter }
                }
                Some(IntegrationsCommands::ListPrivatePypiRepositories { perimeter }) => {
                    ObnAction::IntegrationsListPrivatePypi { perimeter }
                }
                Some(IntegrationsCommands::ListPrivateConda { perimeter }) => {
                    ObnAction::IntegrationsListPrivateConda { perimeter }
                }
                Some(IntegrationsCommands::Anaconda(AnacondaCommands::Create {
                    name,
                    description,
                    perimeter,
                })) => ObnAction::IntegrationsAnacondaCreate {
                    name,
                    description,
                    perimeter,
                },
                Some(IntegrationsCommands::Artifactory(ArtifactoryCommands::Create {
                    name,
                    description,
                    url,
                    username,
                    password,
                    perimeter,
                })) => ObnAction::IntegrationsArtifactoryCreate {
                    name,
                    description,
                    url,
                    username,
                    password,
                    perimeter,
                },
                Some(IntegrationsCommands::AzureArtifacts(AzureArtifactsCommands::Create {
                    name,
                    description,
                    organization,
                    project,
                    feed,
                    username,
                    pat,
                    perimeter,
                })) => ObnAction::IntegrationsAzureArtifactsCreate {
                    name,
                    description,
                    organization,
                    project,
                    feed,
                    username,
                    pat,
                    perimeter,
                },
                Some(IntegrationsCommands::CodeArtifacts(CodeArtifactsCommands::Create {
                    name,
                    description,
                    domain_name,
                    domain_owner,
                    aws_region,
                    target_role,
                    perimeter,
                })) => ObnAction::IntegrationsCodeArtifactsCreate {
                    name,
                    description,
                    domain_name,
                    domain_owner,
                    aws_region,
                    target_role,
                    perimeter,
                },
                Some(IntegrationsCommands::ContainerRegistry(
                    ContainerRegistryCommands::Create {
                        name,
                        description,
                        registry_domain,
                        target_role_arn,
                        use_task_role,
                        username,
                        password,
                        perimeter,
                    },
                )) => ObnAction::IntegrationsContainerRegistryCreate {
                    name,
                    description,
                    registry_domain,
                    target_role_arn,
                    use_task_role,
                    username,
                    password,
                    perimeter,
                },
                Some(IntegrationsCommands::CustomSecret(CustomSecretCommands::Create {
                    name,
                    description,
                    secrets,
                    perimeter,
                })) => ObnAction::IntegrationsCustomSecretCreate {
                    name,
                    description,
                    secrets,
                    perimeter,
                },
                Some(IntegrationsCommands::CustomSecret(CustomSecretCommands::Update {
                    name,
                    description,
                    secrets,
                    perimeter,
                })) => ObnAction::IntegrationsCustomSecretUpdate {
                    name,
                    description,
                    secrets,
                    perimeter,
                },
                Some(IntegrationsCommands::GitPypiRepository(
                    GitPypiRepositoryCommands::Create {
                        name,
                        description,
                        repository_url,
                        username,
                        password,
                        perimeter,
                    },
                )) => ObnAction::IntegrationsGitPypiRepositoryCreate {
                    name,
                    description,
                    repository_url,
                    username,
                    password,
                    perimeter,
                },
                Some(IntegrationsCommands::GitlabArtifacts(GitlabArtifactsCommands::Create {
                    name,
                    description,
                    gitlab_url,
                    project_id,
                    username,
                    password,
                    perimeter,
                })) => ObnAction::IntegrationsGitlabArtifactsCreate {
                    name,
                    description,
                    gitlab_url,
                    project_id,
                    username,
                    password,
                    perimeter,
                },
                Some(IntegrationsCommands::PrivateCondaChannels(
                    PrivateCondaChannelsCommands::Add {
                        channel_name,
                        host_integration_name,
                        is_default,
                        perimeter,
                    },
                )) => ObnAction::IntegrationsPrivateCondaChannelsAdd {
                    channel_name,
                    host_integration_name,
                    is_default,
                    perimeter,
                },
                Some(IntegrationsCommands::PrivatePypiRepositories(
                    PrivatePypiRepositoriesCommands::Add {
                        repository_name,
                        host_integration_name,
                        is_default,
                        perimeter,
                    },
                )) => ObnAction::IntegrationsPrivatePypiRepositoriesAdd {
                    repository_name,
                    host_integration_name,
                    is_default,
                    perimeter,
                },
                Some(IntegrationsCommands::S3Proxy(S3ProxyCommands::Create {
                    name,
                    description,
                    bucket_name,
                    endpoint_url,
                    region,
                    access_key_id,
                    secret_access_key,
                    perimeter,
                })) => ObnAction::IntegrationsS3ProxyCreate {
                    name,
                    description,
                    bucket_name,
                    endpoint_url,
                    region,
                    access_key_id,
                    secret_access_key,
                    perimeter,
                },
                Some(IntegrationsCommands::S3Proxy(S3ProxyCommands::Update {
                    name,
                    description,
                    bucket_name,
                    endpoint_url,
                    region,
                    access_key_id,
                    secret_access_key,
                    perimeter,
                })) => ObnAction::IntegrationsS3ProxyUpdate {
                    name,
                    description,
                    bucket_name,
                    endpoint_url,
                    region,
                    access_key_id,
                    secret_access_key,
                    perimeter,
                },
                Some(IntegrationsCommands::CodeArtifacts(CodeArtifactsCommands::Update {
                    name,
                    description,
                    domain_name,
                    domain_owner,
                    aws_region,
                    target_role,
                    perimeter,
                })) => ObnAction::IntegrationsCodeArtifactsUpdate {
                    name,
                    description,
                    domain_name,
                    domain_owner,
                    aws_region,
                    target_role,
                    perimeter,
                },
                Some(IntegrationsCommands::Artifactory(ArtifactoryCommands::Update {
                    name,
                    description,
                    url,
                    username,
                    password,
                    perimeter,
                })) => ObnAction::IntegrationsArtifactoryUpdate {
                    name,
                    description,
                    url,
                    username,
                    password,
                    perimeter,
                },
                Some(IntegrationsCommands::AzureArtifacts(AzureArtifactsCommands::Update {
                    name,
                    description,
                    organization,
                    project,
                    username,
                    pat,
                    perimeter,
                })) => ObnAction::IntegrationsAzureArtifactsUpdate {
                    name,
                    description,
                    organization,
                    project,
                    username,
                    pat,
                    perimeter,
                },
                Some(IntegrationsCommands::GitlabArtifacts(GitlabArtifactsCommands::Update {
                    name,
                    description,
                    gitlab_url,
                    project_id,
                    username,
                    password,
                    perimeter,
                })) => ObnAction::IntegrationsGitlabArtifactsUpdate {
                    name,
                    description,
                    gitlab_url,
                    project_id,
                    username,
                    password,
                    perimeter,
                },
                Some(IntegrationsCommands::ContainerRegistry(
                    ContainerRegistryCommands::Update {
                        name,
                        description,
                        registry_domain,
                        target_role_arn,
                        use_task_role,
                        username,
                        password,
                        perimeter,
                    },
                )) => ObnAction::IntegrationsContainerRegistryUpdate {
                    name,
                    description,
                    registry_domain,
                    target_role_arn,
                    use_task_role,
                    username,
                    password,
                    perimeter,
                },
                Some(IntegrationsCommands::GitPypiRepository(
                    GitPypiRepositoryCommands::Update {
                        name,
                        description,
                        repository_urls,
                        username,
                        password,
                        perimeter,
                    },
                )) => ObnAction::IntegrationsGitPypiRepositoryUpdate {
                    name,
                    description,
                    repository_urls,
                    username,
                    password,
                    perimeter,
                },
                Some(IntegrationsCommands::PrivateCondaChannels(
                    PrivateCondaChannelsCommands::Remove {
                        channel_name,
                        perimeter,
                    },
                )) => ObnAction::IntegrationsPrivateCondaChannelsRemove {
                    channel_name,
                    perimeter,
                },
                Some(IntegrationsCommands::PrivatePypiRepositories(
                    PrivatePypiRepositoriesCommands::Remove {
                        repository_name,
                        perimeter,
                    },
                )) => ObnAction::IntegrationsPrivatePypiRepositoriesRemove {
                    repository_name,
                    perimeter,
                },
            },
            ObnCommands::FastBakery { command } => match command {
                None => ObnAction::ShowHelp("obn fast-bakery".to_string()),
                Some(FastBakeryCommands::GetLoginPassword) => ObnAction::FastBakeryGetLoginPassword,
                Some(FastBakeryCommands::ConfigureDockerLogin {
                    registry_url,
                    output,
                }) => ObnAction::FastBakeryConfigureDockerLogin {
                    registry_url,
                    output,
                },
            },
            ObnCommands::Kubernetes { command } => match command {
                None => ObnAction::ShowHelp("obn kubernetes".to_string()),
                Some(KubernetesCommands::Kill {
                    flow_name,
                    run_id,
                    my_runs,
                    dry_run,
                    auto_approve,
                    clear_everything,
                }) => ObnAction::KubernetesKill {
                    flow_name,
                    run_id,
                    my_runs,
                    dry_run,
                    auto_approve,
                    clear_everything,
                },
            },
            ObnCommands::Flowproject { command } => match command {
                None => ObnAction::ShowHelp("obn flowproject".to_string()),
                Some(FlowprojectCommands::GetMetadata { id }) => {
                    ObnAction::FlowprojectGetMetadata { id }
                }
                Some(FlowprojectCommands::SetMetadata { json }) => {
                    ObnAction::FlowprojectSetMetadata { json }
                }
                Some(FlowprojectCommands::DeleteMetadata { id, yes, output }) => {
                    ObnAction::FlowprojectDeleteMetadata { id, yes, output }
                }
                Some(FlowprojectCommands::ListTemplates { id, output }) => {
                    ObnAction::FlowprojectListTemplates { id, output }
                }
                Some(FlowprojectCommands::TeardownBranch {
                    id,
                    dry_run,
                    yes,
                    output,
                }) => ObnAction::FlowprojectTeardownBranch {
                    id,
                    dry_run,
                    yes,
                    output,
                }
            },
            ObnCommands::Secrets { command } => match command {
                None => ObnAction::ShowHelp("obn secrets".to_string()),
                Some(SecretsCommands::Get {
                    secret_ids,
                    format,
                    role,
                    file,
                }) => ObnAction::SecretsGet {
                    secret_ids,
                    format,
                    role,
                    file,
                },
            },
            ObnCommands::Tutorials { command } => match command {
                None => ObnAction::ShowHelp("obn tutorials".to_string()),
                Some(TutorialsCommands::Pull {
                    url,
                    destination_dir,
                    force_overwrite,
                }) => ObnAction::TutorialsPull {
                    url,
                    destination_dir,
                    force_overwrite,
                },
            },
            ObnCommands::Workstations { command } => match command {
                None => ObnAction::ShowHelp("obn workstations".to_string()),
                Some(WorkstationsCommands::List { output }) => {
                    ObnAction::WorkstationsList { output }
                }
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
                Some(WorkstationsCommands::GetLinks {
                    perimeter_id,
                    output,
                }) => ObnAction::WorkstationsGetLinks {
                    perimeter_id,
                    output,
                },
                Some(WorkstationsCommands::ConfigureKubeconfig {
                    binary_path,
                    kubeconfig_path,
                }) => ObnAction::WorkstationsConfigureKubeconfig {
                    binary_path,
                    kubeconfig_path,
                },
                Some(WorkstationsCommands::PrepareSsh {
                    workstation_id,
                    setup_context,
                    mode,
                }) => ObnAction::WorkstationsPrepareSsh {
                    workstation_id,
                    setup_context,
                    mode,
                },
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
