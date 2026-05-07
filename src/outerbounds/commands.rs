use clap::Subcommand;

#[derive(Subcommand)]
pub enum ObCommands {
    /// Create a new Outerbounds project
    Init {
        /// Path to create the project in
        path: Option<String>,

        /// Project name
        #[arg(short, long)]
        name: Option<String>,

        /// Project title
        #[arg(short, long)]
        title: Option<String>,

        /// Skip git initialization
        #[arg(long)]
        no_git_init: bool,
    },

    /// Deploy the current project
    #[command(trailing_var_arg = true)]
    Deploy {
        /// Arguments to pass to obproject-deploy
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Commands for Outerbounds apps
    #[command(subcommand_required = false, arg_required_else_help = false)]
    App {
        #[command(subcommand)]
        command: Option<ObAppCommands>,
    },

    /// Check packages and configuration for compatibility
    #[command(trailing_var_arg = true)]
    Check {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Decode Outerbounds Platform configuration
    #[command(trailing_var_arg = true)]
    Configure {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Commands for interacting with Fast Bakery
    #[command(trailing_var_arg = true, name = "fast-bakery")]
    FastBakery {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Manage resource integrations
    #[command(trailing_var_arg = true)]
    Integrations {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Commands for interacting with Kubernetes
    #[command(trailing_var_arg = true)]
    Kubernetes {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Manage perimeters
    #[command(trailing_var_arg = true)]
    Perimeter {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Authenticate service principals using JWT
    #[command(trailing_var_arg = true, name = "service-principal-configure")]
    ServicePrincipalConfigure {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Commands for pushing Deployments metadata
    #[command(subcommand_required = false, arg_required_else_help = false)]
    Flowproject {
        #[command(subcommand)]
        command: Option<ObFlowprojectCommands>,
    },
}

#[derive(Subcommand)]
pub enum ObAppCommands {
    /// Open a deployed app in the browser (ana-specific)
    Open {
        /// Name of the app to open
        name: String,
    },

    /// View the current project's deployed app (ana-specific)
    View {
        /// Open in browser
        #[arg(long)]
        web: bool,
    },

    /// Delete an app from the Outerbounds Platform
    #[command(trailing_var_arg = true)]
    Delete {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Deploy an app to the Outerbounds Platform
    #[command(trailing_var_arg = true)]
    Deploy {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Get detailed information about an app
    #[command(trailing_var_arg = true)]
    Info {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// List apps in the Outerbounds Platform
    #[command(trailing_var_arg = true)]
    List {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Get logs for an app worker
    #[command(trailing_var_arg = true)]
    Logs {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
pub enum ObFlowprojectCommands {
    /// Delete flowproject metadata for a project/branch
    #[command(
        trailing_var_arg = true,
        name = "delete-metadata",
        disable_help_flag = true
    )]
    DeleteMetadata {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Get flowproject metadata
    #[command(
        trailing_var_arg = true,
        name = "get-metadata",
        disable_help_flag = true
    )]
    GetMetadata {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// List deployed workflow templates for a project/branch
    #[command(
        trailing_var_arg = true,
        name = "list-templates",
        disable_help_flag = true
    )]
    ListTemplates {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Set flowproject metadata
    #[command(
        trailing_var_arg = true,
        name = "set-metadata",
        disable_help_flag = true
    )]
    SetMetadata {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Tear down all deployed resources for a project/branch
    #[command(
        trailing_var_arg = true,
        name = "teardown-branch",
        disable_help_flag = true
    )]
    TeardownBranch {
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },
}
