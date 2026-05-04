use std::process::Command;

use crate::auth;
use crate::config::Config;

/// Get the base URL for uv auth by removing /simple/ suffix if present.
fn get_base_url(pip_index_url: &str) -> &str {
    pip_index_url
        .trim_end_matches('/')
        .trim_end_matches("/simple")
        .trim_end_matches('/')
}

/// Configure uv to use Anaconda's wheels index with authentication.
/// Caller should verify uv is installed before calling.
pub fn configure(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let api_key = auth::get_api_key(config)?
        .ok_or("Login required to configure uv. Run `ana login` first.")?;

    let base_url = get_base_url(&config.pip_index_url);

    let output = Command::new("uv")
        .args(["auth", "login", base_url, "--token", &api_key])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to configure uv auth: {}", stderr).into());
    }

    Ok(())
}

/// Remove uv configuration for Anaconda's wheels index.
/// Caller should verify uv is installed before calling.
pub fn deconfigure(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let base_url = get_base_url(&config.pip_index_url);

    let output = Command::new("uv")
        .args(["auth", "logout", base_url])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Ignore "not logged in" errors
        if !stderr.contains("not logged in") && !stderr.contains("No credentials") {
            return Err(format!("Failed to deconfigure uv auth: {}", stderr).into());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_base_url_with_simple_suffix() {
        let url = "https://repo.example.com/wheels/simple/";
        assert_eq!(get_base_url(url), "https://repo.example.com/wheels");
    }

    #[test]
    fn test_get_base_url_with_simple_no_trailing_slash() {
        let url = "https://repo.example.com/wheels/simple";
        assert_eq!(get_base_url(url), "https://repo.example.com/wheels");
    }

    #[test]
    fn test_get_base_url_without_simple() {
        let url = "https://repo.example.com/wheels/";
        assert_eq!(get_base_url(url), "https://repo.example.com/wheels");
    }

    #[test]
    fn test_get_base_url_no_trailing_slash() {
        let url = "https://repo.example.com/wheels";
        assert_eq!(get_base_url(url), "https://repo.example.com/wheels");
    }

    #[test]
    fn test_get_base_url_real_wheels_url() {
        let url = "https://repo-latest.dev-us-east-1.anaconda.cloud/repo/wheels-test/simple/";
        assert_eq!(
            get_base_url(url),
            "https://repo-latest.dev-us-east-1.anaconda.cloud/repo/wheels-test"
        );
    }
}
