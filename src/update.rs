use serde::Deserialize;
use std::env;

// We track the repo for releases
const GITHUB_REPO: &str = "anaconda/ana-cli";

fn get_github_token() -> Result<String, Error> {
    match env::var("GITHUB_TOKEN") {
        Ok(token) if !token.is_empty() => Ok(token),
        _ => Err(Error::MissingToken),
    }
}

#[derive(Debug, PartialEq)]
pub enum Error {
    Http(String),
    VersionParse(String),
    MissingToken,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Http(msg) => write!(f, "HTTP error: {}", msg),
            Error::VersionParse(v) => write!(f, "Failed to parse version: {}", v),
            Error::MissingToken => {
                writeln!(
                    f,
                    "GITHUB_TOKEN not set. Required for accessing private repo."
                )?;
                writeln!(f, "  Run: export GITHUB_TOKEN=$(gh auth token)")
            }
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Http(e.to_string())
    }
}

#[derive(Debug, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub prerelease: bool,
}

pub fn parse_version(tag: &str) -> Result<semver::Version, Error> {
    // Convert a tag associated with a GitHub release into a semantic version
    let version_str = tag.strip_prefix('v').unwrap_or(tag);
    // Convert .devN to -dev.N for semver compatibility
    let normalized = if let Some((base, dev_num)) = version_str.split_once(".dev") {
        format!("{}-dev.{}", base, dev_num)
    } else {
        version_str.to_string()
    };
    semver::Version::parse(&normalized).map_err(|_| Error::VersionParse(tag.to_string()))
}

pub fn fetch_releases() -> Result<Vec<Release>, Error> {
    let token = get_github_token()?;
    let client = reqwest::blocking::Client::new();
    let url = format!("https://api.github.com/repos/{}/releases", GITHUB_REPO);
    let releases: Vec<Release> = client
        .get(&url)
        .header("User-Agent", "ana-cli")
        .header("Accept", "application/vnd.github+json")
        .header("Authorization", format!("Bearer {}", token))
        .send()?
        .error_for_status()?
        .json()?;
    Ok(releases)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fetch_releases_missing_token_error() {
        // This test will fail if GITHUB_TOKEN is set in the environment
        // In CI, ensure it's not set, or skip this test
        if env::var("GITHUB_TOKEN").is_ok() {
            return; // Skip test if token is set
        }
        let result = fetch_releases();
        assert_eq!(result.unwrap_err(), Error::MissingToken);
    }

    #[test]
    fn test_parse_version_stable() {
        let version = parse_version("v1.2.3").unwrap();
        assert_eq!(version, semver::Version::new(1, 2, 3));
    }

    #[test]
    fn test_parse_version_dev() {
        let version = parse_version("v0.0.2.dev5").unwrap();
        assert_eq!(version.major, 0);
        assert_eq!(version.minor, 0);
        assert_eq!(version.patch, 2);
        assert!(!version.pre.is_empty());
    }

    #[test]
    fn test_parse_version_without_v_prefix() {
        let version = parse_version("1.0.0").unwrap();
        assert_eq!(version, semver::Version::new(1, 0, 0));
    }

    #[test]
    fn test_parse_version_invalid() {
        let result = parse_version("not-a-version");
        assert!(matches!(result, Err(Error::VersionParse(_))));
    }
}
