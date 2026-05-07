use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::errors::OuterboundsNotConfiguredError;
use crate::input::prompt_input;

#[derive(Default)]
pub struct InitOptions {
    pub path: Option<String>,
    pub name: Option<String>,
    pub title: Option<String>,
    pub no_git_init: bool,
}

impl InitOptions {
    /// Create InitOptions from pre-parsed arguments (after clap validation)
    pub fn from_args(args: &[String]) -> Self {
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
            } else if arg == "--no-git-init" {
                opts.no_git_init = true;
            } else if !arg.starts_with('-') && opts.path.is_none() {
                opts.path = Some(arg.clone());
            }
            i += 1;
        }

        opts
    }
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

/// Check if Outerbounds is configured by looking for ~/.metaflowconfig/config.json
pub fn is_configured() -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    // The main metaflow config file - this is what outerbounds CLI expects
    home.join(".metaflowconfig/config.json").exists()
}

/// Returns an error if Outerbounds is not configured.
pub fn ensure_configured() -> miette::Result<()> {
    if is_configured() {
        Ok(())
    } else {
        Err(OuterboundsNotConfiguredError.into())
    }
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

fn write_template(
    path: &Path,
    template: &str,
    replacements: &[(&str, &str)],
) -> miette::Result<()> {
    let mut content = template.to_string();
    for (from, to) in replacements {
        content = content.replace(from, to);
    }
    fs::write(path, content)
        .map_err(|e| miette::miette!("Failed to write {}: {}", path.display(), e))
}

/// Expand ~ to home directory in a path string
fn expand_tilde(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    } else if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    Path::new(path).to_path_buf()
}

pub fn init_project(opts: InitOptions) -> miette::Result<()> {
    // Determine project path, prompting if in existing git repo without explicit path
    let project_path = if let Some(ref p) = opts.path {
        expand_tilde(p)
    } else {
        // Check if we're in an existing git repository
        let cwd = std::env::current_dir()
            .map_err(|e| miette::miette!("Failed to get current directory: {}", e))?;
        if cwd.join(".git").exists() {
            crate::ui::status::warn("This directory is already a git repository.");
            let prompt = format!("Project path [{}]", cwd.display());
            let input = prompt_input(&prompt).map_err(|e| miette::miette!("{}", e))?;
            if input.is_empty() {
                cwd
            } else {
                expand_tilde(&input)
            }
        } else {
            cwd
        }
    };

    // Skip git init if project path already has .git
    let no_git_init = opts.no_git_init || project_path.join(".git").exists();

    // Check Outerbounds configuration
    ensure_configured()?;

    // Get platform from config
    let platform = detect_platform().expect("config was validated");

    if project_path.join("obproject.toml").exists() {
        return Err(miette::miette!(
            "obproject.toml already exists in this directory"
        ));
    }

    let project_name = if let Some(name) = opts.name {
        if !is_valid_project_name(&name) {
            return Err(miette::miette!(
                "Invalid project name. Use only lowercase letters, numbers, and underscores."
            ));
        }
        name
    } else {
        loop {
            let name = prompt_input("Project name (lowercase, underscores allowed)")
                .map_err(|e| miette::miette!("{}", e))?;
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
            return Err(miette::miette!("Title cannot be empty."));
        }
        t
    } else {
        loop {
            let t = prompt_input("Project title").map_err(|e| miette::miette!("{}", e))?;
            if !t.is_empty() {
                break t;
            }
            eprintln!("Title cannot be empty.");
        }
    };

    if project_path != Path::new(".") {
        fs::create_dir_all(&project_path)
            .map_err(|e| miette::miette!("Failed to create directory: {}", e))?;
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
    write_template(&project_path.join(".gitignore"), templates::GITIGNORE, &[])?;

    // Create example flow
    let flow_dir = project_path.join("flows/hello_flow");
    fs::create_dir_all(&flow_dir)
        .map_err(|e| miette::miette!("Failed to create flows directory: {}", e))?;
    write_template(&flow_dir.join("flow.py"), templates::FLOW_PY, &[])?;
    write_template(&flow_dir.join("README.md"), templates::FLOW_README, &[])?;

    // Create example app
    let app_dir = project_path.join("deployments/hello_app");
    fs::create_dir_all(&app_dir)
        .map_err(|e| miette::miette!("Failed to create deployments directory: {}", e))?;
    write_template(&app_dir.join("app.py"), templates::APP_PY, &[])?;
    write_template(
        &app_dir.join("config.yaml"),
        templates::APP_CONFIG_YAML,
        &[],
    )?;
    write_template(
        &app_dir.join("requirements.txt"),
        templates::APP_REQUIREMENTS_TXT,
        &[],
    )?;
    write_template(&app_dir.join("README.md"), templates::APP_README, &[])?;

    // Initialize git repository unless --no-git-init was specified or already in a git repo
    if !no_git_init {
        use std::process::Command;

        let git_init = Command::new("git")
            .args(["init"])
            .current_dir(&project_path)
            .output();

        match git_init {
            Ok(output) if output.status.success() => {
                // Stage all files
                let _ = Command::new("git")
                    .args(["add", "."])
                    .current_dir(&project_path)
                    .output();

                // Create initial commit
                let _ = Command::new("git")
                    .args(["commit", "-m", "Initial commit"])
                    .current_dir(&project_path)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_is_valid_project_name_valid() {
        assert!(is_valid_project_name("my_project"));
        assert!(is_valid_project_name("test123"));
        assert!(is_valid_project_name("a"));
        assert!(is_valid_project_name("hello_world_123"));
        assert!(is_valid_project_name("123")); // digits only is valid
    }

    #[test]
    fn test_is_valid_project_name_invalid() {
        assert!(!is_valid_project_name("")); // empty
        assert!(!is_valid_project_name("MyProject")); // uppercase
        assert!(!is_valid_project_name("my-project")); // hyphen
        assert!(!is_valid_project_name("my project")); // space
        assert!(!is_valid_project_name("hello.world")); // dot
    }

    #[test]
    fn test_init_options_from_args_full() {
        let args: Vec<String> = vec![
            "--name".into(),
            "myproj".into(),
            "--title".into(),
            "My Project".into(),
        ];
        let opts = InitOptions::from_args(&args);

        assert_eq!(opts.name, Some("myproj".into()));
        assert_eq!(opts.title, Some("My Project".into()));
        assert!(!opts.no_git_init);
        assert!(opts.path.is_none());
    }

    #[test]
    fn test_init_options_from_args_short_flags() {
        let args: Vec<String> = vec!["-n".into(), "proj".into(), "-t".into(), "Title".into()];
        let opts = InitOptions::from_args(&args);

        assert_eq!(opts.name, Some("proj".into()));
        assert_eq!(opts.title, Some("Title".into()));
    }

    #[test]
    fn test_init_options_from_args_with_path() {
        let args: Vec<String> = vec!["mypath".into(), "--name".into(), "proj".into()];
        let opts = InitOptions::from_args(&args);

        assert_eq!(opts.path, Some("mypath".into()));
        assert_eq!(opts.name, Some("proj".into()));
    }

    #[test]
    fn test_init_options_from_args_no_git_init() {
        let args: Vec<String> = vec!["--no-git-init".into()];
        let opts = InitOptions::from_args(&args);

        assert!(opts.no_git_init);
    }

    #[test]
    fn test_write_template_with_replacements() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.txt");

        let template = "Project: {project}, Platform: {platform}, Title: {title}";
        let replacements = [
            ("{project}", "myproj"),
            ("{platform}", "example.com"),
            ("{title}", "My Title"),
        ];

        write_template(&path, template, &replacements).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(
            content,
            "Project: myproj, Platform: example.com, Title: My Title"
        );
    }

    #[test]
    fn test_write_template_no_replacements() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.txt");

        let template = "Hello, world!";
        write_template(&path, template, &[]).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "Hello, world!");
    }

    /// Create a mock Outerbounds config in the given home directory
    fn create_mock_config(home: &Path, platform: &str) {
        let config_dir = home.join(".metaflowconfig");
        fs::create_dir_all(&config_dir).unwrap();

        // Create config.json (required for is_configured check)
        fs::write(config_dir.join("config.json"), "{}").unwrap();

        // Create ob_config.json with platform URL
        let ob_config = format!(
            r#"{{"OB_CURRENT_PERIMETER_MF_CONFIG_URL": "https://api.{}/v1/perimeters/default/metaflowconfigs/default"}}"#,
            platform
        );
        fs::write(config_dir.join("ob_config.json"), ob_config).unwrap();
    }

    #[test]
    fn test_init_project_creates_structure() {
        let tmp = TempDir::new().unwrap();
        let project_path = tmp.path().join("test_project");

        create_mock_config(tmp.path(), "test.outerbounds.com");

        temp_env::with_var("HOME", Some(tmp.path().to_str().unwrap()), || {
            let opts = InitOptions {
                path: Some(project_path.to_string_lossy().into()),
                name: Some("test_proj".into()),
                title: Some("Test Project".into()),
                no_git_init: true,
            };

            init_project(opts).unwrap();

            // Verify root files
            assert!(project_path.join("obproject.toml").exists());
            assert!(project_path.join("pyproject.toml").exists());
            assert!(project_path.join("README.md").exists());
            assert!(project_path.join(".gitignore").exists());

            // Verify flow structure
            assert!(project_path.join("flows/hello_flow/flow.py").exists());
            assert!(project_path.join("flows/hello_flow/README.md").exists());

            // Verify app structure
            assert!(project_path.join("deployments/hello_app/app.py").exists());
            assert!(
                project_path
                    .join("deployments/hello_app/config.yaml")
                    .exists()
            );
            assert!(
                project_path
                    .join("deployments/hello_app/requirements.txt")
                    .exists()
            );
            assert!(
                project_path
                    .join("deployments/hello_app/README.md")
                    .exists()
            );
        });
    }

    #[test]
    fn test_init_project_template_substitution() {
        let tmp = TempDir::new().unwrap();
        let project_path = tmp.path().join("subst_test");

        create_mock_config(tmp.path(), "cool.outerbounds.com");

        temp_env::with_var("HOME", Some(tmp.path().to_str().unwrap()), || {
            let opts = InitOptions {
                path: Some(project_path.to_string_lossy().into()),
                name: Some("my_cool_project".into()),
                title: Some("My Cool Project".into()),
                no_git_init: true,
            };

            init_project(opts).unwrap();

            let obproject = fs::read_to_string(project_path.join("obproject.toml")).unwrap();
            assert!(obproject.contains("my_cool_project"));
            assert!(obproject.contains("cool.outerbounds.com"));

            let readme = fs::read_to_string(project_path.join("README.md")).unwrap();
            assert!(readme.contains("My Cool Project"));
        });
    }

    #[test]
    fn test_init_project_fails_if_exists() {
        let tmp = TempDir::new().unwrap();

        create_mock_config(tmp.path(), "platform.com");

        // Create existing obproject.toml
        fs::write(tmp.path().join("obproject.toml"), "existing").unwrap();

        temp_env::with_var("HOME", Some(tmp.path().to_str().unwrap()), || {
            let opts = InitOptions {
                path: Some(tmp.path().to_string_lossy().into()),
                name: Some("proj".into()),
                title: Some("Title".into()),
                no_git_init: true,
            };

            let result = init_project(opts);
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains("already exists"),
                "error should mention 'already exists': {}",
                err
            );
        });
    }

    #[test]
    fn test_init_project_invalid_name_fails() {
        let tmp = TempDir::new().unwrap();

        create_mock_config(tmp.path(), "platform.com");

        temp_env::with_var("HOME", Some(tmp.path().to_str().unwrap()), || {
            let opts = InitOptions {
                path: Some(tmp.path().to_string_lossy().into()),
                name: Some("Invalid-Name".into()), // invalid due to uppercase and hyphen
                title: Some("Title".into()),
                no_git_init: true,
            };

            let result = init_project(opts);
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains("Invalid project name"),
                "error should mention 'Invalid project name': {}",
                err
            );
        });
    }

    #[test]
    fn test_init_project_not_configured_fails() {
        let tmp = TempDir::new().unwrap();

        // Set HOME to temp dir so no config is found
        temp_env::with_var("HOME", Some(tmp.path().to_str().unwrap()), || {
            let opts = InitOptions {
                path: Some(tmp.path().to_string_lossy().into()),
                name: Some("proj".into()),
                title: Some("Title".into()),
                no_git_init: true,
            };

            let result = init_project(opts);
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains("not configured"),
                "error should mention not configured: {}",
                err
            );
        });
    }

    #[test]
    fn test_expand_tilde() {
        let tmp = TempDir::new().unwrap();

        temp_env::with_var("HOME", Some(tmp.path().to_str().unwrap()), || {
            // Test ~/path expansion
            let expanded = expand_tilde("~/projects/test");
            assert_eq!(expanded, tmp.path().join("projects/test"));

            // Test ~ alone
            let expanded = expand_tilde("~");
            assert_eq!(expanded, tmp.path().to_path_buf());

            // Test regular path (no tilde)
            let expanded = expand_tilde("/absolute/path");
            assert_eq!(expanded, Path::new("/absolute/path").to_path_buf());

            // Test relative path (no tilde)
            let expanded = expand_tilde("relative/path");
            assert_eq!(expanded, Path::new("relative/path").to_path_buf());

            // Test path starting with ~ but not ~/
            let expanded = expand_tilde("~notahome/path");
            assert_eq!(expanded, Path::new("~notahome/path").to_path_buf());
        });
    }
}
