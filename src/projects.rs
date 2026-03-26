//! Project discovery and detection. This is a basic stub, where projspec integration
//! might fit in.

use std::path::Path;

/// Detected project type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectType {
    Pixi,
}

impl ProjectType {
    /// Returns the tool name needed to run this project type.
    pub fn tool_name(&self) -> &'static str {
        match self {
            ProjectType::Pixi => "pixi",
        }
    }
}

/// Detect the project type in the given directory.
fn detect(dir: &Path) -> Option<ProjectType> {
    // Check for pixi project
    if dir.join("pixi.toml").exists() {
        return Some(ProjectType::Pixi);
    }

    None
}

/// Detect the project type in the current directory.
pub fn detect_current() -> Option<ProjectType> {
    detect(Path::new("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_detect_pixi_project() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("pixi.toml"), "[workspace]").unwrap();

        assert_eq!(detect(tmp.path()), Some(ProjectType::Pixi));
    }

    #[test]
    fn test_detect_no_project() {
        let tmp = tempfile::tempdir().unwrap();

        assert_eq!(detect(tmp.path()), None);
    }

    #[test]
    fn test_tool_name() {
        assert_eq!(ProjectType::Pixi.tool_name(), "pixi");
    }
}
