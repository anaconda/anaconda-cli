use std::fs;
use std::path::Path;

use console::Term;
use serde::Deserialize;

use crate::help::styles::HelpStyle;
use crate::help::{left_margin, print_command_row, print_section};
use crate::input::prompt_input;

#[derive(Default)]
pub struct InitOptions {
    pub path: Option<String>,
    pub name: Option<String>,
    pub title: Option<String>,
    pub platform: Option<String>,
    pub no_git_init: bool,
}

impl InitOptions {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut opts = InitOptions::default();
        let mut i = 0;

        while i < args.len() {
            let arg = &args[i];
            if arg == "--name" || arg == "-n" {
                i += 1;
                opts.name = args.get(i).cloned();
            } else if arg == "--title" || arg == "-t" {
                i += 1;
                opts.title = args.get(i).cloned();
            } else if arg == "--platform" || arg == "-p" {
                i += 1;
                opts.platform = args.get(i).cloned();
            } else if arg == "--no-git-init" {
                opts.no_git_init = true;
            } else if arg == "--help" || arg == "-h" {
                return Err("help".to_string());
            } else if !arg.starts_with('-') && opts.path.is_none() {
                opts.path = Some(arg.clone());
            } else if arg.starts_with('-') {
                return Err(format!("Unknown option: {}", arg));
            }
            i += 1;
        }

        Ok(opts)
    }
}

pub fn print_init_help() {
    let term = Term::stdout();
    let ind = left_margin();

    // Description
    let _ = term.write_line(&format!("{}Initialize a new Outerbounds project", ind));
    let _ = term.write_line("");

    // Usage
    let _ = term.write_line(&format!(
        "{}{}",
        ind,
        HelpStyle::Dim
            .style()
            .apply_to("Usage: ana ob init [PATH] [OPTIONS]")
    ));
    let _ = term.write_line("");

    // Arguments
    print_section(&term, "ARGUMENTS");
    print_command_row(
        &term,
        "[PATH]",
        "Directory to create the project in (default: current directory)",
    );
    let _ = term.write_line("");

    // Options
    print_section(&term, "OPTIONS");
    print_command_row(
        &term,
        "-n, --name <NAME>",
        "Project name (lowercase, underscores allowed)",
    );
    print_command_row(&term, "-t, --title <TITLE>", "Project title");
    print_command_row(
        &term,
        "-p, --platform <URL>",
        "Platform URL (auto-detected from config)",
    );
    print_command_row(
        &term,
        "    --no-git-init",
        "Skip git repository initialization",
    );
    print_command_row(&term, "-h, --help", "Print help");
    let _ = term.write_line("");
}

// Embedded template files
mod templates {
    // Root files
    pub const OBPROJECT_TOML: &str = include_str!("../../templates/ob_project/obproject.toml");
    pub const README_MD: &str = include_str!("../../templates/ob_project/README.md");
    pub const PYPROJECT_TOML: &str = include_str!("../../templates/ob_project/pyproject.toml");
    pub const GITIGNORE: &str = include_str!("../../templates/ob_project/.gitignore");

    // Flow files
    pub const FLOW_PY: &str = include_str!("../../templates/ob_project/flows/hello_flow/flow.py");
    pub const FLOW_README: &str =
        include_str!("../../templates/ob_project/flows/hello_flow/README.md");

    // App files
    pub const APP_PY: &str =
        include_str!("../../templates/ob_project/deployments/hello_app/app.py");
    pub const APP_CONFIG_YAML: &str =
        include_str!("../../templates/ob_project/deployments/hello_app/config.yaml");
    pub const APP_REQUIREMENTS_TXT: &str =
        include_str!("../../templates/ob_project/deployments/hello_app/requirements.txt");
    pub const APP_README: &str =
        include_str!("../../templates/ob_project/deployments/hello_app/README.md");
}

#[derive(Deserialize)]
struct ObConfig {
    #[serde(rename = "OB_CURRENT_PERIMETER_MF_CONFIG_URL")]
    config_url: Option<String>,
}

fn detect_platform() -> Option<String> {
    let home = dirs::home_dir()?;
    let config_path = home.join(".metaflowconfig/ob_config.json");
    let content = fs::read_to_string(config_path).ok()?;
    let config: ObConfig = serde_json::from_str(&content).ok()?;

    // Extract platform from URL like:
    // https://api.merced.obp.outerbounds.com/v1/perimeters/default/metaflowconfigs/default
    // -> merced.obp.outerbounds.com
    let url = config.config_url?;
    let url = url.strip_prefix("https://api.")?;
    let platform = url.split('/').next()?;
    Some(platform.to_string())
}

fn write_template(path: &Path, template: &str, replacements: &[(&str, &str)]) -> Result<(), String> {
    let mut content = template.to_string();
    for (from, to) in replacements {
        content = content.replace(from, to);
    }
    fs::write(path, content).map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

pub fn init_project(opts: InitOptions) -> Result<(), String> {
    let project_path = opts.path.as_ref().map(Path::new).unwrap_or(Path::new("."));
    let no_git_init = opts.no_git_init;

    if project_path.join("obproject.toml").exists() {
        return Err("obproject.toml already exists in this directory".to_string());
    }

    // Check if we're in non-interactive mode (all required params provided)
    let non_interactive = opts.name.is_some() && opts.title.is_some();

    let project_name = if let Some(name) = opts.name {
        if !is_valid_project_name(&name) {
            return Err(
                "Invalid project name. Use only lowercase letters, numbers, and underscores."
                    .to_string(),
            );
        }
        name
    } else {
        loop {
            let name = prompt_input("Project name (lowercase, underscores allowed)")?;
            if is_valid_project_name(&name) {
                break name;
            }
            eprintln!(
                "Invalid project name. Use only lowercase letters, numbers, and underscores."
            );
        }
    };

    let title = if let Some(t) = opts.title {
        if t.is_empty() {
            return Err("Title cannot be empty.".to_string());
        }
        t
    } else {
        loop {
            let t = prompt_input("Project title")?;
            if !t.is_empty() {
                break t;
            }
            eprintln!("Title cannot be empty.");
        }
    };

    let platform = if let Some(p) = opts.platform {
        p
    } else if let Some(detected) = detect_platform() {
        if non_interactive {
            detected
        } else {
            let prompt = format!("Platform URL [{}]", detected);
            let p = prompt_input(&prompt)?;
            if p.is_empty() { detected } else { p }
        }
    } else {
        loop {
            let p = prompt_input("Platform URL (e.g., my-company.outerbounds.com)")?;
            if !p.is_empty() {
                break p;
            }
            eprintln!("Platform URL cannot be empty.");
        }
    };

    if project_path != Path::new(".") {
        fs::create_dir_all(project_path)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let replacements: &[(&str, &str)] = &[
        ("{platform}", &platform),
        ("{project}", &project_name),
        ("{title}", &title),
    ];

    // Write root files
    write_template(
        &project_path.join("obproject.toml"),
        templates::OBPROJECT_TOML,
        replacements,
    )?;
    write_template(
        &project_path.join("README.md"),
        templates::README_MD,
        replacements,
    )?;
    write_template(
        &project_path.join("pyproject.toml"),
        templates::PYPROJECT_TOML,
        replacements,
    )?;
    write_template(
        &project_path.join(".gitignore"),
        templates::GITIGNORE,
        &[],
    )?;

    // Create example flow
    let flow_dir = project_path.join("flows/hello_flow");
    fs::create_dir_all(&flow_dir)
        .map_err(|e| format!("Failed to create flows directory: {}", e))?;
    write_template(&flow_dir.join("flow.py"), templates::FLOW_PY, &[])?;
    write_template(&flow_dir.join("README.md"), templates::FLOW_README, &[])?;

    // Create example app
    let app_dir = project_path.join("deployments/hello_app");
    fs::create_dir_all(&app_dir)
        .map_err(|e| format!("Failed to create deployments directory: {}", e))?;
    write_template(&app_dir.join("app.py"), templates::APP_PY, &[])?;
    write_template(&app_dir.join("config.yaml"), templates::APP_CONFIG_YAML, &[])?;
    write_template(
        &app_dir.join("requirements.txt"),
        templates::APP_REQUIREMENTS_TXT,
        &[],
    )?;
    write_template(&app_dir.join("README.md"), templates::APP_README, &[])?;

    // Initialize git repository unless --no-git-init was specified
    if !no_git_init {
        use std::process::Command;

        let git_init = Command::new("git")
            .args(["init"])
            .current_dir(project_path)
            .output();

        match git_init {
            Ok(output) if output.status.success() => {
                // Stage all files
                let _ = Command::new("git")
                    .args(["add", "."])
                    .current_dir(project_path)
                    .output();

                // Create initial commit
                let _ = Command::new("git")
                    .args(["commit", "-m", "Initial commit"])
                    .current_dir(project_path)
                    .output();
            }
            _ => {
                eprintln!("Warning: Failed to initialize git repository");
            }
        }
    }

    println!("Created Outerbounds project '{}'", project_name);
    println!();
    println!("Project structure:");
    println!("  {}/", project_path.display());
    println!("  ├── obproject.toml");
    println!("  ├── pyproject.toml");
    println!("  ├── README.md");
    println!("  ├── flows/hello_flow/");
    println!("  │   ├── flow.py");
    println!("  │   └── README.md");
    println!("  └── deployments/hello_app/");
    println!("      ├── app.py");
    println!("      ├── config.yaml");
    println!("      ├── requirements.txt");
    println!("      └── README.md");
    println!();
    println!("The flow includes the @anaconda_conda decorator for using");
    println!("Anaconda's main channel with Metaflow steps.");
    println!();
    println!("Next steps:");
    println!("  1. cd {}", project_path.display());
    println!("  2. ana ob deploy");

    Ok(())
}

fn is_valid_project_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}
