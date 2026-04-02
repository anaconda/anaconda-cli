use std::collections::HashMap;
use std::env::consts::{ARCH, OS};
use std::time::Instant;

use anaconda_otel_rs::signals::{increment_counter, record_histogram, shutdown_telemetry};
use clap::{CommandFactory, Parser, Subcommand};
use console::{Style, Term};
use indoc::formatdoc;
use opentelemetry::Value;

use crate::VERSION;
use crate::anaconda_cli;
use crate::auth;
use crate::config::{self, Config};
use crate::update;

/// Build base telemetry attributes with system information.
fn system_attrs() -> HashMap<String, Value> {
    let mut attrs = HashMap::new();
    attrs.insert("os".to_string(), OS.into());
    attrs.insert("arch".to_string(), ARCH.into());
    attrs.insert("version".to_string(), VERSION.into());
    attrs
}

pub async fn execute() {
    // Suppress telemetry logs by default to avoid leaking errors when telemetry fails
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new("anaconda_otel_rs=off,opentelemetry=off,reqwest=off")
    });
    tracing_subscriber::fmt().with_env_filter(filter).init();

    config::setup_telemetry();

    let result = parse().execute().await;

    shutdown_telemetry();

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

/// Action to be performed, returned by parse()
pub enum Action {
    ShowConciseHelp,
    ShowFullHelp,
    ShowSelfHelp,
    ShowAuthHelp,
    ShowVersion,
    ShowConfig,
    Login,
    Logout,
    ShowApiKey,
    Whoami,
    Update { force: bool },
    CheckForUpdate,
    ShowAvailableVersions,
    Bootstrap,
    OrgProxy { args: Vec<String> },
}

impl Action {
    fn match_action_name(&self) -> &'static str {
        match self {
            Action::ShowConciseHelp => "help.concise",
            Action::ShowFullHelp => "help.full",
            Action::ShowSelfHelp => "self.help",
            Action::ShowAuthHelp => "auth.help",
            Action::ShowVersion => "version",
            Action::ShowConfig => "config",
            Action::Login => "login",
            Action::Logout => "logout",
            Action::ShowApiKey => "auth.api-key",
            Action::Whoami => "whoami",
            Action::Update { .. } => "self.update",
            Action::CheckForUpdate => "self.update.check",
            Action::ShowAvailableVersions => "self.update.list",
            Action::Bootstrap => "bootstrap",
            Action::OrgProxy { .. } => "org",
        }
    }

    /// Execute the action with telemetry middleware
    pub async fn execute(self) -> Result<(), Box<dyn std::error::Error>> {
        let name = self.match_action_name();
        let mut attrs = system_attrs();
        attrs.insert("command".to_string(), name.into());
        increment_counter("cli_command_invoked", 1, attrs.clone());

        let start = Instant::now();
        let result = self.run().await;
        let duration_ms = start.elapsed().as_millis() as f64;

        match &result {
            Ok(_) => {
                increment_counter("cli_command_success", 1, attrs.clone());
                record_histogram("cli_command_success_duration_ms", duration_ms, attrs);
            }
            Err(_) => {
                increment_counter("cli_command_failure", 1, attrs.clone());
                record_histogram("cli_command_failure_duration_ms", duration_ms, attrs);
            }
        }

        result
    }

    async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            Action::ShowConciseHelp => {
                print_concise_help();
                Ok(())
            }
            Action::ShowFullHelp => {
                print_full_help();
                Ok(())
            }
            Action::ShowSelfHelp => {
                print_self_help();
                Ok(())
            }
            Action::ShowAuthHelp => {
                print_auth_help();
                Ok(())
            }
            Action::ShowVersion => {
                println!("{}", VERSION);
                Ok(())
            }
            Action::ShowConfig => {
                Config::load().print_table();
                Ok(())
            }
            Action::Bootstrap => Ok(anaconda_cli::run_bootstrap()?),
            Action::OrgProxy { args } => Ok(anaconda_cli::run_subcommand("org", &args)?),
            Action::Login => Ok(auth::login().await?),
            Action::Logout => Ok(auth::logout()?),
            Action::ShowApiKey => Ok(auth::show_api_key()?),
            Action::Whoami => Ok(auth::whoami().await?),
            Action::Update { force } => {
                update::run_update(VERSION, force).await;
                Ok(())
            }
            Action::CheckForUpdate => {
                update::check_for_update(VERSION).await;
                Ok(())
            }
            Action::ShowAvailableVersions => {
                update::show_available_versions(VERSION).await;
                Ok(())
            }
        }
    }
}

/// Parse CLI arguments and return the action to perform.
/// Exits the process on unrecoverable errors (unknown commands, etc.)
pub fn parse() -> Action {
    match Cli::try_parse() {
        Ok(cli) => match cli.command {
            None => Action::ShowConciseHelp,
            Some(Commands::Bootstrap) => Action::Bootstrap,
            Some(Commands::Config) => Action::ShowConfig,
            Some(Commands::Login) => Action::Login,
            Some(Commands::Logout) => Action::Logout,
            Some(Commands::Whoami) => Action::Whoami,
            Some(Commands::Auth { command }) => match command {
                None => Action::ShowAuthHelp,
                Some(AuthCommands::ApiKey) => Action::ShowApiKey,
                Some(AuthCommands::Login) => Action::Login,
                Some(AuthCommands::Logout) => Action::Logout,
                Some(AuthCommands::Whoami) => Action::Whoami,
            },
            Some(Commands::Self_ { command }) => match command {
                None => Action::ShowSelfHelp,
                Some(SelfCommands::Update { yes, check, list }) => {
                    if check {
                        Action::CheckForUpdate
                    } else if list {
                        Action::ShowAvailableVersions
                    } else {
                        Action::Update { force: yes }
                    }
                }
            },
            Some(Commands::Org { args }) => Action::OrgProxy { args },
        },
        Err(e) => handle_parse_error(e),
    }
}

fn handle_parse_error(e: clap::Error) -> Action {
    if e.kind() == clap::error::ErrorKind::DisplayHelp {
        return Action::ShowFullHelp;
    }
    if e.kind() == clap::error::ErrorKind::DisplayVersion {
        return Action::ShowVersion;
    }

    // Handle unknown subcommand errors with custom format
    let err_str = e.to_string();
    if err_str.contains("unrecognized subcommand") {
        let args: Vec<String> = std::env::args().collect();
        if args.len() > 1 && args[1] == "self" {
            if args.len() > 2 {
                eprintln!("Unknown self command: {}", args[2]);
            }
        } else if args.len() > 1 {
            eprintln!("Unknown command: {}", args[1]);
        }
        std::process::exit(1);
    }

    // For other errors, use clap's error handling
    e.exit();
}

/// Styles for help output matching UX design
struct HelpStyles {
    section: Style,   // #3fb950 - green headers
    command: Style,   // #79c0ff - blue command names
    desc: Style,      // #8b949e - gray descriptions
    dim: Style,       // #6e7681 - dim gray for comments/hints
    error: Style,     // #f85149 - error red
    warning: Style,   // #d29922 - warning yellow
}

impl HelpStyles {
    fn new() -> Self {
        Self {
            section: Style::new().color256(77).bold(),
            command: Style::new().color256(117),
            desc: Style::new().color256(245),
            dim: Style::new().color256(242),
            error: Style::new().color256(203),
            warning: Style::new().color256(178),
        }
    }
}

/// Print a command row: "  command      description"
fn print_command_row(term: &Term, styles: &HelpStyles, name: &str, desc: &str) {
    let styled_name = styles.command.apply_to(name);
    let styled_desc = styles.desc.apply_to(desc);
    let _ = term.write_line(&format!("  {styled_name:<20} {styled_desc}"));
}

/// Print a section header
fn print_section(term: &Term, styles: &HelpStyles, name: &str) {
    let _ = term.write_line(&styles.section.apply_to(name).to_string());
}

/// Print the examples/quick-start code block
fn print_examples_block(term: &Term, styles: &HelpStyles, examples: &[(&str, &str)]) {
    for (comment, command) in examples {
        let _ = term.write_line(&format!("    {}", styles.dim.apply_to(format!("# {comment}"))));
        let _ = term.write_line(&format!("    {command}"));
    }
}

/// Concise help shown when running `ana` with no arguments
pub fn print_concise_help() {
    let styles = HelpStyles::new();
    let term = Term::stdout();

    // Header
    let _ = term.write_line(&format!("ana {VERSION}"));
    let _ = term.write_line(&styles.desc.apply_to("Manage your toolchain, AI models, builds, and deployments from one place.").to_string());
    let _ = term.write_line("");

    // Quick start section
    print_section(&term, &styles, "QUICK START");
    print_examples_block(&term, &styles, &[
        ("set up your full toolchain", "ana install all"),
        ("launch jupyter", "ana jupyter"),
        ("build and deploy your app", "ana build && ana deploy --target snowflake"),
    ]);
    let _ = term.write_line("");

    // Common commands section
    print_section(&term, &styles, "COMMON COMMANDS");
    print_command_row(&term, &styles, "install", "Install a tool -- conda, pixi, uv, pip, Jupyter, Desktop");
    print_command_row(&term, &styles, "jupyter", "Launch a pre-configured Jupyter instance");
    print_command_row(&term, &styles, "model", "Discover, pull, and manage AI models");
    print_command_row(&term, &styles, "build", "Build containers, packages, or PyScript apps");
    print_command_row(&term, &styles, "deploy", "Deploy to SageMaker, Snowflake, Databricks, and more");
    let _ = term.write_line("");

    // Footer
    let run_help = format!("Run {} for the full command list", styles.command.apply_to("ana --help"));
    let docs_link = styles.section.apply_to("-> docs.anaconda.com");
    let _ = term.write_line(&styles.dim.apply_to(run_help).to_string());
    let _ = term.write_line(&docs_link.to_string());
}

/// Full help shown when running `ana --help`
pub fn print_full_help() {
    let styles = HelpStyles::new();
    let term = Term::stdout();

    // Get clap command for introspection
    let cmd = Cli::command();
    let subcommands: HashMap<&str, String> = cmd
        .get_subcommands()
        .map(|s| (s.get_name(), s.get_about().map(|a| a.to_string()).unwrap_or_default()))
        .collect();

    // Helper to get description or fallback
    let desc = |name: &str, fallback: &str| -> String {
        subcommands.get(name).map(|s| s.as_str()).unwrap_or(fallback).to_string()
    };

    // Header
    let _ = term.write_line(&format!("ana {VERSION}"));
    let _ = term.write_line(&styles.desc.apply_to("Manage your toolchain, AI models, builds, and deployments from one place.").to_string());
    let _ = term.write_line("");

    // Examples section
    print_section(&term, &styles, "EXAMPLES");
    print_examples_block(&term, &styles, &[
        ("set up your full toolchain", "ana install all"),
        ("search for and download a package", "ana search numpy && ana download numpy"),
        ("browse and pull an AI model", "ana model search llama"),
        ("build and deploy your app", "ana build && ana deploy --target snowflake"),
    ]);
    let _ = term.write_line("");

    // Toolchain section
    print_section(&term, &styles, "TOOLCHAIN");
    print_command_row(&term, &styles, "install", &desc("install", "Install a tool -- conda, pixi, uv, pip, Jupyter, or Anaconda Desktop"));
    print_command_row(&term, &styles, "update", &desc("update", "Update one or all installed tools"));
    print_command_row(&term, &styles, "configure", &desc("configure", "Apply or change settings for your tools"));
    print_command_row(&term, &styles, "uninstall", &desc("uninstall", "Remove an installed tool"));
    print_command_row(&term, &styles, "tools", &desc("tools", "List what's installed and at which version"));
    print_command_row(&term, &styles, "config", &desc("config", "Show or edit current ana configuration"));
    print_command_row(&term, &styles, "self", &desc("self", "Manage the ana installation itself"));
    let _ = term.write_line("");

    // Develop section
    print_section(&term, &styles, "DEVELOP");
    print_command_row(&term, &styles, "jupyter", &desc("jupyter", "Launch a pre-configured Jupyter instance"));
    print_command_row(&term, &styles, "model", &desc("model", "Discover, pull, and manage AI models from Anaconda's vetted catalog"));
    print_command_row(&term, &styles, "build", &desc("build", "Build containers, packages, or PyScript apps -- includes signing and CVE scanning"));
    print_command_row(&term, &styles, "deploy", &desc("deploy", "Deploy to SageMaker, Snowflake, Databricks, Vertex AI, or Azure ML"));
    let _ = term.write_line("");

    // Packages section
    print_section(&term, &styles, "PACKAGES");
    print_command_row(&term, &styles, "search", &desc("search", "Search for packages in your Anaconda repository"));
    print_command_row(&term, &styles, "show", &desc("show", "Show information about a package or object"));
    print_command_row(&term, &styles, "download", &desc("download", "Download packages from your Anaconda repository"));
    print_command_row(&term, &styles, "upload", &desc("upload", "Upload packages to your Anaconda repository"));
    print_command_row(&term, &styles, "remove", &desc("remove", "Remove a package or object from your repository"));
    let _ = term.write_line(&styles.dim.apply_to("  advanced").to_string());
    print_command_row(&term, &styles, "copy", &desc("copy", "Copy packages from one account to another"));
    print_command_row(&term, &styles, "move", &desc("move", "Move packages between labels"));
    print_command_row(&term, &styles, "label", &desc("label", "Manage your Anaconda repository channels"));
    print_command_row(&term, &styles, "package", &desc("package", "Anaconda repository package utilities"));
    print_command_row(&term, &styles, "repo", &desc("repo", "Repository operations: channel, copy, mirror, move, search, upload"));
    let _ = term.write_line("");

    // Account section
    print_section(&term, &styles, "ACCOUNT");
    print_command_row(&term, &styles, "login / logout", &desc("login", "Connect or disconnect from the Anaconda platform"));
    print_command_row(&term, &styles, "whoami", &desc("whoami", "Show your current logged-in account"));
    print_command_row(&term, &styles, "auth", &desc("auth", "Manage your Anaconda authentication"));
    print_command_row(&term, &styles, "org", &desc("org", "Interact with anaconda.org"));
    print_command_row(&term, &styles, "sites", &desc("sites", "Manage your Anaconda site configuration"));
    print_command_row(&term, &styles, "token", &desc("token", "Manage your Anaconda repo tokens"));
    let _ = term.write_line("");

    // Global options section
    print_section(&term, &styles, "GLOBAL OPTIONS");
    let _ = term.write_line(&format!("  {}  {}",
        styles.command.apply_to("--at <site>".to_string() + &" ".repeat(11)),
        styles.desc.apply_to("Select configured site by name or domain")));
    let _ = term.write_line(&format!("  {}  {}",
        styles.command.apply_to("-v, --verbose".to_string() + &" ".repeat(7)),
        styles.desc.apply_to("Print debug information to the console")));
    let _ = term.write_line(&format!("  {}  {}",
        styles.command.apply_to("-V, --version".to_string() + &" ".repeat(7)),
        styles.desc.apply_to("Show the ana version and exit")));
    let _ = term.write_line(&format!("  {}  {}",
        styles.command.apply_to("-h, --help".to_string() + &" ".repeat(10)),
        styles.desc.apply_to("Show this message and exit")));
    let _ = term.write_line("");

    // Typo hint box
    let _ = term.write_line(&styles.desc.apply_to("Typo? ana will suggest the closest command.").to_string());
    let _ = term.write_line(&format!("    {} {}", styles.dim.apply_to("# example"), ""));
    let _ = term.write_line(&format!("    {} {}",
        styles.error.apply_to("error:"),
        styles.desc.apply_to("unknown command \"instal\"")));
    let _ = term.write_line(&format!("    {} {}",
        styles.warning.apply_to("tip:"),
        styles.desc.apply_to(format!("did you mean {}?", styles.command.apply_to("install")))));
    let _ = term.write_line("");

    // Footer
    let run_cmd = format!("Run {} or {} for more",
        styles.command.apply_to("ana <command> --help"),
        styles.command.apply_to("ana help <command>"));
    let _ = term.write_line(&styles.dim.apply_to(run_cmd).to_string());
    let _ = term.write_line(&styles.section.apply_to("-> docs.anaconda.com").to_string());
}

pub fn print_self_help() {
    println!(
        "{}",
        formatdoc! {"
        Manage the installation

        Usage: ana self <command> [options]

        Commands:
          update    Update ana to the latest version
        "}
    );
}

pub fn print_auth_help() {
    println!(
        "{}",
        formatdoc! {"
        Authentication commands

        Usage: ana auth <command> [options]

        Commands:
          api-key   Display the API key for the logged-in user
          login     Log in to Anaconda
          logout    Log out from Anaconda
          whoami    Display information about the logged-in user
        "}
    );
}

#[derive(Parser)]
#[command(
    name = "ana",
    version = VERSION,
    about = "",
    long_about = None,
    subcommand_required = false,
    arg_required_else_help = false,
    disable_help_subcommand = true,
    override_usage = "ana [command] [options]",
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Authentication commands
    #[command(
        subcommand_required = false,
        arg_required_else_help = false,
        override_usage = "ana auth <command> [options]"
    )]
    Auth {
        #[command(subcommand)]
        command: Option<AuthCommands>,
    },

    /// Install the Anaconda CLI
    Bootstrap,

    /// Show current configuration
    Config,

    /// Log in to Anaconda
    Login,

    /// Log out from Anaconda
    Logout,

    /// Display information about the logged-in user
    Whoami,

    /// Manage the ana installation
    #[command(
        subcommand_required = false,
        arg_required_else_help = false,
        override_usage = "ana self <command> [options]"
    )]
    Self_ {
        #[command(subcommand)]
        command: Option<SelfCommands>,
    },

    /// Interact with anaconda.org
    #[command(
        trailing_var_arg = true,
        override_usage = "ana org <command> [options]"
    )]
    Org {
        /// Arguments to pass to anaconda org
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
enum AuthCommands {
    /// Display the API key for the logged-in user
    ApiKey,

    /// Log in to Anaconda
    Login,

    /// Log out from Anaconda
    Logout,

    /// Display information about the logged-in user
    Whoami,
}

#[derive(Subcommand)]
enum SelfCommands {
    /// Update ana to the latest version
    Update {
        /// Skip confirmation prompt
        #[arg(short = 'y', long = "yes")]
        yes: bool,

        /// Check if an update is available
        #[arg(long, conflicts_with_all = ["yes", "list"])]
        check: bool,

        /// List available versions
        #[arg(long, conflicts_with_all = ["yes", "check"])]
        list: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_parses() {
        // Verify clap setup is valid
        Cli::command().debug_assert();
    }
}
