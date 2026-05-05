//! Configure pip to use Anaconda's wheels index.

use std::process::Command;

use miette::{Context, IntoDiagnostic, miette};
use url::Url;

use crate::auth;
use crate::config::Config;
use crate::tools::utils::find_pip;

/// Configure pip to use Anaconda's wheels index with authentication.
/// Caller should verify pip is installed before calling.
pub fn configure(config: &Config) -> miette::Result<()> {
    let pip_cmd = find_pip().ok_or_else(|| miette!("pip not found in PATH"))?;

    let api_key = auth::get_api_key(config)
        .into_diagnostic()?
        .ok_or_else(|| miette!("Login required to configure pip. Run `ana login` first."))?;

    configure_pip(pip_cmd, config, &api_key)?;

    Ok(())
}

/// Remove pip configuration for Anaconda's wheels index.
/// Caller should verify pip is installed before calling.
pub fn deconfigure() -> miette::Result<()> {
    let pip_cmd = find_pip().ok_or_else(|| miette!("pip not found in PATH"))?;

    let output = Command::new(pip_cmd)
        .args(["config", "unset", "global.index-url"])
        .output()
        .into_diagnostic()
        .context("Failed to run pip config")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // "no such key" means it wasn't configured, which is fine
        if !stderr.contains("no such key") {
            return Err(miette!("Failed to deconfigure pip: {}", stderr));
        }
    }

    Ok(())
}

/// Configure pip to use Anaconda's package index with authentication.
fn configure_pip(pip_cmd: &str, config: &Config, api_key: &str) -> miette::Result<()> {
    // Build authenticated URL by inserting credentials into the index URL
    // e.g., https://example.com/path -> https://__token__:API_KEY@example.com/path
    let authenticated_url = build_authenticated_url(&config.pip_index_url, api_key)?;

    // Configure pip global index-url
    let output = Command::new(pip_cmd)
        .args(["config", "set", "global.index-url", &authenticated_url])
        .output()
        .into_diagnostic()
        .context("Failed to run pip config")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(miette!("Failed to configure pip: {}", stderr));
    }

    Ok(())
}

/// Build an authenticated URL by inserting token credentials.
fn build_authenticated_url(url_str: &str, api_key: &str) -> miette::Result<String> {
    let mut url = Url::parse(url_str)
        .into_diagnostic()
        .context("Invalid URL")?;
    url.set_username("__token__")
        .map_err(|_| miette!("Cannot set username on URL"))?;
    url.set_password(Some(api_key))
        .map_err(|_| miette!("Cannot set password on URL"))?;
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_authenticated_url_with_path() {
        let url = "https://example.com/repo/simple/";
        let api_key = "test-api-key";

        let result = build_authenticated_url(url, api_key).unwrap();

        assert_eq!(
            result,
            "https://__token__:test-api-key@example.com/repo/simple/"
        );
    }

    #[test]
    fn test_build_authenticated_url_without_path() {
        let url = "https://pypi.org";
        let api_key = "my-key";

        let result = build_authenticated_url(url, api_key).unwrap();

        assert_eq!(result, "https://__token__:my-key@pypi.org/");
    }

    #[test]
    fn test_build_authenticated_url_http_scheme() {
        let url = "http://localhost:8080/simple/";
        let api_key = "local-key";

        let result = build_authenticated_url(url, api_key).unwrap();

        assert_eq!(result, "http://__token__:local-key@localhost:8080/simple/");
    }

    #[test]
    fn test_build_authenticated_url_missing_scheme() {
        let url = "example.com/path";
        let api_key = "key";

        let result = build_authenticated_url(url, api_key);

        assert!(result.is_err());
    }

    #[test]
    fn test_build_authenticated_url_real_wheels_url() {
        let url = "https://repo-latest.dev-us-east-1.anaconda.cloud/repo/wheels-test/simple/";
        let api_key = "abc123";

        let result = build_authenticated_url(url, api_key).unwrap();

        assert_eq!(
            result,
            "https://__token__:abc123@repo-latest.dev-us-east-1.anaconda.cloud/repo/wheels-test/simple/"
        );
    }
}
