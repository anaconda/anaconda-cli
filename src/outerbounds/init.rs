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

const OBPROJECT_TOML_TEMPLATE: &str = r#"platform = "{platform}"
project = "{project}"
title = "{title}"
"#;

const README_TEMPLATE: &str = r#"# {title}

An Outerbounds project.

## Getting Started

1. Deploy the project:
   ```bash
   ana ob deploy
   ```

## Project Structure

- `flows/hello_flow/` - Example Metaflow workflow
- `deployments/hello_app/` - Example FastAPI application

## Running Locally

### Run the flow locally:
```bash
cd flows/hello_flow
python flow.py run
```

### Run the app locally:
```bash
cd deployments/hello_app
pip install -r requirements.txt
uvicorn app:app --reload
```
"#;

const PYPROJECT_TOML_TEMPLATE: &str = r#"[project]
name = "{project}"
version = "0.1.0"
description = "{title}"
requires-python = "==3.12"
dependencies = [
    "fastapi>=0.100.0",
    "uvicorn[standard]>=0.23.0",
]

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"
"#;

const FLOW_PY: &str = r##"from metaflow import step, card, current
from metaflow.cards import Markdown
from obproject import ProjectFlow


class HelloFlow(ProjectFlow):
    """A simple example flow that demonstrates Outerbounds basics."""

    @step
    def start(self):
        """Initialize the flow with a greeting."""
        self.message = "Hello from Outerbounds!"
        print(self.message)
        self.next(self.process)

    @step
    def process(self):
        """Process the message."""
        self.processed = self.message.upper()
        print(f"Processed: {self.processed}")
        self.next(self.end)

    @card
    @step
    def end(self):
        """Finish the flow and display results in a card."""
        current.card.append(Markdown("# Flow Complete"))
        current.card.append(Markdown(f"**Original message:** {self.message}"))
        current.card.append(Markdown(f"**Processed message:** {self.processed}"))
        print(f"Flow complete! Final message: {self.processed}")


if __name__ == "__main__":
    HelloFlow()
"##;

const FLOW_README: &str = r#"# Hello Flow

A simple Outerbounds workflow that demonstrates the basics.

## Running Locally

```bash
python flow.py run
```

## Running on Outerbounds

```bash
python flow.py --with kubernetes run
```
"#;

const APP_PY: &str = r#"from fastapi import FastAPI

app = FastAPI(title="Hello App", description="A simple example API")


@app.get("/")
async def root():
    """Return a welcome message."""
    return {"message": "Hello from Outerbounds!"}


@app.get("/health")
async def health():
    """Health check endpoint."""
    return {"status": "healthy"}


@app.get("/greet/{name}")
async def greet(name: str):
    """Greet a user by name."""
    return {"message": f"Hello, {name}!"}
"#;

const APP_CONFIG_YAML: &str = r#"name: hello-app
port: 8000
auth_type: Browser
commands:
  - uvicorn deployments.hello_app.app:app --host 0.0.0.0 --port 8000
resources:
  cpu: "0.5"
  memory: "512Mi"
"#;

const APP_REQUIREMENTS_TXT: &str = r#"fastapi>=0.100.0
uvicorn[standard]>=0.23.0
"#;

const APP_README: &str = r#"# Hello App

A simple FastAPI application deployed to Outerbounds.

## Endpoints

- `GET /` - Welcome message
- `GET /health` - Health check
- `GET /greet/{name}` - Personalized greeting

## Running Locally

```bash
pip install -r requirements.txt
uvicorn app:app --reload
```

Then visit http://localhost:8000
"#;

const GITIGNORE: &str = r#"# Python
__pycache__/
*.py[cod]
*$py.class
*.so
.Python
*.egg-info/
dist/
build/

# Virtual environments
.venv/
venv/
ENV/

# IDE
.idea/
.vscode/
*.swp
*.swo

# Metaflow
.metaflow/

# OS
.DS_Store
Thumbs.db
"#;

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
            if p.is_empty() {
                detected
            } else {
                p
            }
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

    // Write root files
    let obproject_content = OBPROJECT_TOML_TEMPLATE
        .replace("{platform}", &platform)
        .replace("{project}", &project_name)
        .replace("{title}", &title);
    fs::write(project_path.join("obproject.toml"), obproject_content)
        .map_err(|e| format!("Failed to write obproject.toml: {}", e))?;

    let readme_content = README_TEMPLATE.replace("{title}", &title);
    fs::write(project_path.join("README.md"), readme_content)
        .map_err(|e| format!("Failed to write README.md: {}", e))?;

    let pyproject_content = PYPROJECT_TOML_TEMPLATE
        .replace("{project}", &project_name)
        .replace("{title}", &title);
    fs::write(project_path.join("pyproject.toml"), pyproject_content)
        .map_err(|e| format!("Failed to write pyproject.toml: {}", e))?;

    fs::write(project_path.join(".gitignore"), GITIGNORE)
        .map_err(|e| format!("Failed to write .gitignore: {}", e))?;

    // Create example flow
    let flow_dir = project_path.join("flows/hello_flow");
    fs::create_dir_all(&flow_dir)
        .map_err(|e| format!("Failed to create flows directory: {}", e))?;
    fs::write(flow_dir.join("flow.py"), FLOW_PY)
        .map_err(|e| format!("Failed to write flow.py: {}", e))?;
    fs::write(flow_dir.join("README.md"), FLOW_README)
        .map_err(|e| format!("Failed to write flow README: {}", e))?;

    // Create example app
    let app_dir = project_path.join("deployments/hello_app");
    fs::create_dir_all(&app_dir)
        .map_err(|e| format!("Failed to create deployments directory: {}", e))?;
    fs::write(app_dir.join("app.py"), APP_PY)
        .map_err(|e| format!("Failed to write app.py: {}", e))?;
    fs::write(app_dir.join("config.yaml"), APP_CONFIG_YAML)
        .map_err(|e| format!("Failed to write config.yaml: {}", e))?;
    fs::write(app_dir.join("requirements.txt"), APP_REQUIREMENTS_TXT)
        .map_err(|e| format!("Failed to write requirements.txt: {}", e))?;
    fs::write(app_dir.join("README.md"), APP_README)
        .map_err(|e| format!("Failed to write app README: {}", e))?;

    // Create src directory
    fs::create_dir_all(project_path.join("src"))
        .map_err(|e| format!("Failed to create src directory: {}", e))?;

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
