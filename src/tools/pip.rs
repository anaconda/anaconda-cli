//! Configure pip to use Anaconda's wheels index.

use std::process::Command;

use crate::auth;
use crate::config::Config;

/// Configure pip to use Anaconda's wheels index with authentication.
pub fn configure(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    // Check if user is logged in and has an API key
    let api_key = auth::get_api_key(config)?
        .ok_or("Login required to configure pip. Run `ana login` first.")?;

    configure_pip(config, &api_key)?;

    Ok(())
}

/// Remove pip configuration for Anaconda's wheels index.
pub fn deconfigure() -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("pip")
        .args(["config", "unset", "global.index-url"])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // "no such key" means it wasn't configured, which is fine
        if !stderr.contains("no such key") {
            return Err(format!("Failed to deconfigure pip: {}", stderr).into());
        }
    }

    println!("Removed pip index-url configuration");
    Ok(())
}

/// Configure pip to use Anaconda's package index with authentication.
fn configure_pip(config: &Config, api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Build authenticated URL by inserting credentials into the index URL
    // e.g., https://example.com/path -> https://__token__:API_KEY@example.com/path
    let authenticated_url = build_authenticated_url(&config.pip_index_url, api_key)?;

    // Configure pip global index-url
    let output = Command::new("pip")
        .args(["config", "set", "global.index-url", &authenticated_url])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to configure pip: {}", stderr).into());
    }

    println!("Configured pip to use {}", config.pip_index_url);
    Ok(())
}

/// Build an authenticated URL by inserting token credentials.
fn build_authenticated_url(url: &str, api_key: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Parse the URL to extract components
    let prefix_end = url.find("://").ok_or("Invalid URL: missing scheme")?;
    let scheme = &url[..prefix_end];
    let rest = &url[prefix_end + 3..]; // skip "://"

    // Find where the host ends (at first '/' or end of string)
    let host_end = rest.find('/').unwrap_or(rest.len());
    let host = &rest[..host_end];
    let path = &rest[host_end..];

    Ok(format!(
        "{}://__token__:{}@{}{}",
        scheme, api_key, host, path
    ))
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

        assert_eq!(result, "https://__token__:my-key@pypi.org");
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
        assert!(result.unwrap_err().to_string().contains("missing scheme"));
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
