use clap::Subcommand;

/// Result of resolving an outerbounds command.
pub enum ObAction {
    /// Show help for a subcommand path (e.g., "ob", "ob app", "ob flowproject")
    ShowHelp(String),
    /// Proxy args to the outerbounds CLI
    Proxy(Vec<String>),
}

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

impl ObCommands {
    /// Convert the command into an action (either show help or proxy args).
    pub fn into_action(self) -> ObAction {
        match self {
            ObCommands::Init {
                path,
                name,
                title,
                no_git_init,
            } => {
                let mut args = vec!["init".to_string()];
                if let Some(p) = path {
                    args.push(p);
                }
                if let Some(n) = name {
                    args.push("--name".to_string());
                    args.push(n);
                }
                if let Some(t) = title {
                    args.push("--title".to_string());
                    args.push(t);
                }
                if no_git_init {
                    args.push("--no-git-init".to_string());
                }
                ObAction::Proxy(args)
            }
            ObCommands::Deploy { args: deploy_args } => {
                let mut args = vec!["deploy".to_string()];
                args.extend(deploy_args);
                ObAction::Proxy(args)
            }
            ObCommands::App { command } => match command {
                None => ObAction::ShowHelp("ob app".to_string()),
                Some(app_cmd) => app_cmd.into_action(),
            },
            ObCommands::Check { args: check_args } => {
                let mut args = vec!["check".to_string()];
                args.extend(check_args);
                ObAction::Proxy(args)
            }
            ObCommands::Configure { args: cfg_args } => {
                let mut args = vec!["configure".to_string()];
                args.extend(cfg_args);
                ObAction::Proxy(args)
            }
            ObCommands::FastBakery { args: fb_args } => {
                let mut args = vec!["fast-bakery".to_string()];
                args.extend(fb_args);
                ObAction::Proxy(args)
            }
            ObCommands::Integrations { args: int_args } => {
                let mut args = vec!["integrations".to_string()];
                args.extend(int_args);
                ObAction::Proxy(args)
            }
            ObCommands::Kubernetes { args: k8s_args } => {
                let mut args = vec!["kubernetes".to_string()];
                args.extend(k8s_args);
                ObAction::Proxy(args)
            }
            ObCommands::Perimeter { args: perm_args } => {
                let mut args = vec!["perimeter".to_string()];
                args.extend(perm_args);
                ObAction::Proxy(args)
            }
            ObCommands::ServicePrincipalConfigure { args: spc_args } => {
                let mut args = vec!["service-principal-configure".to_string()];
                args.extend(spc_args);
                ObAction::Proxy(args)
            }
            ObCommands::Flowproject { command } => match command {
                None => ObAction::ShowHelp("ob flowproject".to_string()),
                Some(fp_cmd) => fp_cmd.into_action(),
            },
        }
    }
}

impl ObAppCommands {
    /// Convert the app command into an action.
    pub fn into_action(self) -> ObAction {
        match self {
            ObAppCommands::Open { name } => {
                ObAction::Proxy(vec!["app".to_string(), "open".to_string(), name])
            }
            ObAppCommands::View { web } => {
                let mut args = vec!["app".to_string(), "view".to_string()];
                if web {
                    args.push("--web".to_string());
                }
                ObAction::Proxy(args)
            }
            ObAppCommands::Delete { args: cmd_args } => {
                let mut args = vec!["app".to_string(), "delete".to_string()];
                args.extend(cmd_args);
                ObAction::Proxy(args)
            }
            ObAppCommands::Deploy { args: cmd_args } => {
                let mut args = vec!["app".to_string(), "deploy".to_string()];
                args.extend(cmd_args);
                ObAction::Proxy(args)
            }
            ObAppCommands::Info { args: cmd_args } => {
                let mut args = vec!["app".to_string(), "info".to_string()];
                args.extend(cmd_args);
                ObAction::Proxy(args)
            }
            ObAppCommands::List { args: cmd_args } => {
                let mut args = vec!["app".to_string(), "list".to_string()];
                args.extend(cmd_args);
                ObAction::Proxy(args)
            }
            ObAppCommands::Logs { args: cmd_args } => {
                let mut args = vec!["app".to_string(), "logs".to_string()];
                args.extend(cmd_args);
                ObAction::Proxy(args)
            }
        }
    }
}

impl ObFlowprojectCommands {
    /// Convert the flowproject command into an action.
    pub fn into_action(self) -> ObAction {
        match self {
            ObFlowprojectCommands::DeleteMetadata { args: dm_args } => {
                let mut args = vec!["flowproject".to_string(), "delete-metadata".to_string()];
                args.extend(dm_args);
                ObAction::Proxy(args)
            }
            ObFlowprojectCommands::GetMetadata { args: gm_args } => {
                let mut args = vec!["flowproject".to_string(), "get-metadata".to_string()];
                args.extend(gm_args);
                ObAction::Proxy(args)
            }
            ObFlowprojectCommands::ListTemplates { args: lt_args } => {
                let mut args = vec!["flowproject".to_string(), "list-templates".to_string()];
                args.extend(lt_args);
                ObAction::Proxy(args)
            }
            ObFlowprojectCommands::SetMetadata { args: sm_args } => {
                let mut args = vec!["flowproject".to_string(), "set-metadata".to_string()];
                args.extend(sm_args);
                ObAction::Proxy(args)
            }
            ObFlowprojectCommands::TeardownBranch { args: tb_args } => {
                let mut args = vec!["flowproject".to_string(), "teardown-branch".to_string()];
                args.extend(tb_args);
                ObAction::Proxy(args)
            }
        }
    }
}
