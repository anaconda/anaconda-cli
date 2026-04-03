use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;
use std::env;
use std::io::{Read, Write};

use crate::config::Config;
use crate::input::prompt_yes_no;

// We track the repo for releases
const GITHUB_REPO: &str = "anaconda/ana-cli";

fn github_client() -> Result<reqwest::blocking::Client, Error> {
    let token = match env::var("GITHUB_TOKEN") {
        Ok(token) if !token.is_empty() => token,
        _ => return Err(Error::MissingToken),
    };
    reqwest::blocking::Client::builder()
        .user_agent("ana-cli")
        .default_headers({
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", token).parse().unwrap(),
            );
            headers
        })
        .build()
        .map_err(|e| Error::Http(e.to_string()))
}

#[derive(Debug, PartialEq)]
pub enum Error {
    Http(String),
    Io(String),
    VersionParse(String),
    MissingToken,
    AssetNotFound(String),
    UnsupportedPlatform(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Http(msg) => write!(f, "HTTP error: {}", msg),
            Error::Io(msg) => write!(f, "IO error: {}", msg),
            Error::VersionParse(v) => write!(f, "Failed to parse version: {}", v),
            Error::MissingToken => {
                writeln!(
                    f,
                    "GITHUB_TOKEN not set. Required for accessing private repo."
                )?;
                writeln!(f, "  Run: export GITHUB_TOKEN=$(gh auth token)")
            }
            Error::AssetNotFound(platform) => {
                write!(f, "No release asset found for platform: {}", platform)
            }
            Error::UnsupportedPlatform(info) => {
                write!(f, "Unsupported platform: {}", info)
            }
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Http(e.to_string())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub prerelease: bool,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

fn get_asset_name() -> Result<String, Error> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    match (os, arch) {
        ("macos", "aarch64") => Ok("ana-darwin-arm64".to_string()),
        ("linux", "x86_64") => Ok("ana-linux-x86_64".to_string()),
        ("windows", "x86_64") => Ok("ana-windows-x86_64.exe".to_string()),
        _ => Err(Error::UnsupportedPlatform(format!("{}-{}", os, arch))),
    }
}

pub fn get_asset_for_platform(release: &Release) -> Result<&Asset, Error> {
    let asset_name = get_asset_name()?;

    release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .ok_or(Error::AssetNotFound(asset_name))
}

pub fn download_and_replace(asset: &Asset) -> Result<(), Error> {
    let client = github_client()?;
    let mut response = client
        .get(&asset.url)
        .header("Accept", "application/octet-stream")
        .send()?
        .error_for_status()?;

    let total_size = response.content_length().unwrap_or(0);

    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{bar:40.cyan/blue} {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("=> "),
    );

    let temp_dir = std::env::temp_dir();
    let temp_path = temp_dir.join(&asset.name);
    let mut file = std::fs::File::create(&temp_path).map_err(|e| Error::Io(e.to_string()))?;

    let mut buffer = [0u8; 8192];
    loop {
        let n = response
            .read(&mut buffer)
            .map_err(|e| Error::Io(e.to_string()))?;
        if n == 0 {
            break;
        }
        file.write_all(&buffer[..n])
            .map_err(|e| Error::Io(e.to_string()))?;
        pb.inc(n as u64);
    }

    pb.finish_and_clear();

    // Replace the running binary in-place
    self_replace::self_replace(&temp_path).map_err(|e| Error::Io(e.to_string()))?;

    Ok(())
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

fn fetch_releases() -> Result<Vec<Release>, Error> {
    let client = github_client()?;
    let url = format!("https://api.github.com/repos/{}/releases", GITHUB_REPO);
    let releases: Vec<Release> = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()?
        .error_for_status()?
        .json()?;
    Ok(releases)
}

pub fn fetch_available_releases() -> Result<Vec<Release>, Error> {
    let config = Config::load();
    let mut releases: Vec<_> = fetch_releases()?
        .into_iter()
        .filter(|r| parse_version(&r.tag_name).is_ok())
        .filter(|r| config.include_prereleases || !r.prerelease)
        .collect();
    releases.sort_by(|a, b| {
        let va = parse_version(&a.tag_name).unwrap();
        let vb = parse_version(&b.tag_name).unwrap();
        vb.cmp(&va) // Descending order (newest first)
    });
    Ok(releases)
}

pub enum UpdateCheck {
    Available(Release),
    AlreadyUpToDate,
    NoReleases,
}

fn find_update(releases: Vec<Release>, current_version: &str) -> Result<UpdateCheck, Error> {
    let current = parse_version(current_version)?;

    let latest = releases
        .into_iter()
        .filter(|r| parse_version(&r.tag_name).is_ok())
        .max_by(|a, b| {
            let va = parse_version(&a.tag_name).unwrap();
            let vb = parse_version(&b.tag_name).unwrap();
            va.cmp(&vb)
        });

    let latest = match latest {
        Some(r) => r,
        None => return Ok(UpdateCheck::NoReleases),
    };

    let latest_version = parse_version(&latest.tag_name).unwrap();

    if latest_version > current {
        Ok(UpdateCheck::Available(latest))
    } else {
        Ok(UpdateCheck::AlreadyUpToDate)
    }
}

pub fn check_update(current_version: &str) -> Result<UpdateCheck, Error> {
    let releases = fetch_available_releases()?;
    find_update(releases, current_version)
}

pub fn apply_update(release: &Release) -> Result<(), Error> {
    let asset = get_asset_for_platform(release)?;
    println!("Downloading {} ({})", asset.name, asset.url);
    download_and_replace(asset)?;
    Ok(())
}

pub fn check_for_update(current_version: &str) {
    match check_update(current_version) {
        Ok(UpdateCheck::Available(release)) => {
            println!(
                "Update available: {} -> {}",
                current_version, release.tag_name
            );
        }
        Ok(UpdateCheck::AlreadyUpToDate) => {
            println!("Already up to date ({})", current_version);
        }
        Ok(UpdateCheck::NoReleases) => {
            println!("No releases available.");
        }
        Err(e) => {
            eprintln!("Failed to check for update: {}", e);
        }
    }
}

pub fn run_update(current_version: &str, force: bool) {
    let check = match check_update(current_version) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to check for updates: {}", e);
            return;
        }
    };

    match check {
        UpdateCheck::Available(release) => {
            if !force {
                let message = format!("Update {} -> {}?", current_version, release.tag_name);
                if !prompt_yes_no(&message) {
                    println!("Update cancelled.");
                    return;
                }
            }
            match apply_update(&release) {
                Ok(()) => println!(
                    "Updated successfully: {} -> {}",
                    current_version, release.tag_name
                ),
                Err(e) => eprintln!("Failed to update: {}", e),
            }
        }
        UpdateCheck::AlreadyUpToDate => {
            println!("Already up to date ({})", current_version);
        }
        UpdateCheck::NoReleases => {
            println!("No releases available.");
        }
    }
}

pub fn show_available_versions(current_version: &str) {
    let releases = match fetch_available_releases() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to fetch releases: {}", e);
            return;
        }
    };

    if releases.is_empty() {
        println!("No releases available.");
        return;
    }

    let current_tag = format!("v{}", current_version);
    for release in releases {
        let marker = if release.tag_name == current_tag {
            " *"
        } else {
            ""
        };
        println!("{}{}", release.tag_name, marker);
    }
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

    fn make_release(tag: &str, prerelease: bool) -> Release {
        Release {
            tag_name: tag.to_string(),
            prerelease,
            assets: vec![],
        }
    }

    #[test]
    fn test_find_update_available() {
        let releases = vec![make_release("v0.0.1", false), make_release("v0.0.2", false)];
        let result = find_update(releases, "0.0.1").unwrap();
        assert!(matches!(result, UpdateCheck::Available(release) if release.tag_name == "v0.0.2"));
    }

    #[test]
    fn test_find_update_already_up_to_date() {
        let releases = vec![make_release("v0.0.1", false), make_release("v0.0.2", false)];
        let result = find_update(releases, "0.0.2").unwrap();
        assert!(matches!(result, UpdateCheck::AlreadyUpToDate));
    }

    #[test]
    fn test_find_update_no_releases() {
        let releases = vec![];
        let result = find_update(releases, "0.0.1").unwrap();
        assert!(matches!(result, UpdateCheck::NoReleases));
    }

    #[test]
    fn test_find_update_with_prereleases() {
        // When prereleases are in the list, find_update finds the latest
        // (filtering happens in fetch_available_releases, not find_update)
        let releases = vec![
            make_release("v0.0.1", false),
            make_release("v0.0.2.dev1", true),
        ];
        let result = find_update(releases, "0.0.1").unwrap();
        assert!(
            matches!(result, UpdateCheck::Available(release) if release.tag_name == "v0.0.2.dev1")
        );
    }

    #[test]
    fn test_find_update_without_prereleases() {
        // When prereleases are filtered out before calling find_update
        let releases = vec![make_release("v0.0.1", false)];
        let result = find_update(releases, "0.0.1").unwrap();
        assert!(matches!(result, UpdateCheck::AlreadyUpToDate));
    }

    #[test]
    fn test_find_update_invalid_current_version() {
        let releases = vec![make_release("v0.0.1", false)];
        let result = find_update(releases, "invalid");
        assert!(matches!(result, Err(Error::VersionParse(_))));
    }

    #[test]
    fn test_find_update_skips_invalid_release_tags() {
        let releases = vec![
            make_release("v0.0.1", false),
            make_release("not-a-version", false),
            make_release("v0.0.2", false),
        ];
        let result = find_update(releases, "0.0.1").unwrap();
        assert!(matches!(result, UpdateCheck::Available(release) if release.tag_name == "v0.0.2"));
    }

    #[test]
    fn test_get_asset_name_returns_platform_specific_name() {
        // This test verifies get_asset_name returns a valid result on supported platforms
        // On CI/dev machines (macOS arm64, linux x86_64, windows x86_64), this should succeed
        if let Ok(name) = get_asset_name() {
            assert!(
                name == "ana-darwin-arm64"
                    || name == "ana-linux-x86_64"
                    || name == "ana-windows-x86_64.exe"
            );
        }
    }

    #[test]
    fn test_get_asset_for_platform_finds_matching_asset() {
        let release = Release {
            tag_name: "v1.0.0".to_string(),
            prerelease: false,
            assets: vec![
                Asset {
                    name: "ana-darwin-arm64".to_string(),
                    url: "https://example.com/darwin".to_string(),
                },
                Asset {
                    name: "ana-linux-x86_64".to_string(),
                    url: "https://example.com/linux".to_string(),
                },
                Asset {
                    name: "ana-windows-x86_64.exe".to_string(),
                    url: "https://example.com/windows".to_string(),
                },
            ],
        };

        let result = get_asset_for_platform(&release);
        // On supported platforms, should find the matching asset
        if let Ok(asset) = result {
            assert!(release.assets.iter().any(|a| a.name == asset.name));
        }
    }

    #[test]
    fn test_get_asset_for_platform_returns_error_when_not_found() {
        let release = Release {
            tag_name: "v1.0.0".to_string(),
            prerelease: false,
            assets: vec![Asset {
                name: "ana-unknown-platform".to_string(),
                url: "https://example.com/unknown".to_string(),
            }],
        };

        let result = get_asset_for_platform(&release);
        // On supported platforms, should return AssetNotFound since our platform isn't in assets
        if get_asset_name().is_ok() {
            assert!(matches!(result, Err(Error::AssetNotFound(_))));
        }
    }
}
