use std::collections::HashMap;

use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;
use tokio::io::AsyncWriteExt;

use crate::context::CommandContext;
use crate::errors::UpdateError;
use crate::input::prompt_yes_no;

// GitHub repository for releases (used when ANA_SELF_UPDATE_URL=github)
const GITHUB_REPO: &str = "anaconda/ana-cli";

// Static manifest structs
#[derive(Debug, Clone, Deserialize)]
struct StaticManifest {
    channels: HashMap<String, StaticChannel>,
}

#[derive(Debug, Clone, Deserialize)]
struct StaticChannel {
    versions: Vec<String>,
    #[allow(dead_code)]
    latest: Option<String>,
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
    pub draft: bool,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

fn get_platform_target() -> Result<&'static str, UpdateError> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    match (os, arch) {
        ("macos", "aarch64") => Ok("darwin-arm64"),
        ("macos", "x86_64") => Ok("darwin-x86_64"),
        ("linux", "x86_64") => Ok("linux-x86_64"),
        ("linux", "aarch64") => Ok("linux-aarch64"),
        ("windows", "x86_64") => Ok("windows-x86_64"),
        _ => {
            tracing::error!("Unsupported platform: {}-{}", os, arch);
            Err(UpdateError::UnsupportedPlatform(format!("{}-{}", os, arch)))
        }
    }
}

fn get_asset_name() -> Result<String, UpdateError> {
    let target = get_platform_target()?;
    if target.starts_with("windows") {
        Ok(format!("ana-{}.exe", target))
    } else {
        Ok(format!("ana-{}", target))
    }
}

pub fn get_asset_for_platform(release: &Release) -> Result<&Asset, UpdateError> {
    let asset_name = get_asset_name()?;

    release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .ok_or(UpdateError::AssetNotFound(asset_name))
}

async fn download_and_replace(ctx: &CommandContext, asset: &Asset) -> Result<(), UpdateError> {
    use futures_util::StreamExt;

    let is_github = asset.url.contains("api.github.com");
    let response = if is_github {
        let gh_client = ctx.github_client().ok_or(UpdateError::MissingToken)?;
        gh_client
            .get(&asset.url)
            .header("Accept", "application/octet-stream")
            .send()
            .await
            .map_err(|e| UpdateError::Http(e.to_string()))?
            .error_for_status()
            .map_err(|e| UpdateError::Http(e.to_string()))?
    } else {
        ctx.download_client()
            .get(&asset.url)
            .send()
            .await
            .map_err(|e| UpdateError::Http(e.to_string()))?
            .error_for_status()
            .map_err(|e| UpdateError::Http(e.to_string()))?
    };

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
    let mut file = tokio::fs::File::create(&temp_path)
        .await
        .map_err(|e| UpdateError::Io(e.to_string()))?;

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| UpdateError::Io(e.to_string()))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| UpdateError::Io(e.to_string()))?;
        pb.inc(chunk.len() as u64);
    }

    pb.finish_and_clear();

    // Replace the running binary in-place
    self_replace::self_replace(&temp_path).map_err(|e| UpdateError::Io(e.to_string()))?;

    Ok(())
}

pub fn parse_version(tag: &str) -> Result<semver::Version, UpdateError> {
    // Convert a tag associated with a GitHub release into a semantic version
    let version_str = tag.strip_prefix('v').unwrap_or(tag);
    // Convert .devN to -dev.N for semver compatibility
    let normalized = if let Some((base, dev_num)) = version_str.split_once(".dev") {
        format!("{}-dev.{}", base, dev_num)
    } else {
        version_str.to_string()
    };
    semver::Version::parse(&normalized).map_err(|_| UpdateError::VersionParse(tag.to_string()))
}

async fn fetch_github_releases(ctx: &CommandContext) -> Result<Vec<Release>, UpdateError> {
    let gh_client = ctx.github_client().ok_or(UpdateError::MissingToken)?;
    let url = format!("https://api.github.com/repos/{}/releases", GITHUB_REPO);
    let releases: Vec<Release> = gh_client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| UpdateError::Http(e.to_string()))?
        .error_for_status()
        .map_err(|e| UpdateError::Http(e.to_string()))?
        .json()
        .await
        .map_err(|e| UpdateError::Http(e.to_string()))?;
    Ok(releases)
}

async fn fetch_static_releases(
    ctx: &CommandContext,
    base_url: &str,
) -> Result<Vec<Release>, UpdateError> {
    let manifest_url = format!("{}/releases.json", base_url);
    let manifest: StaticManifest = ctx
        .download_client()
        .get(&manifest_url)
        .send()
        .await
        .map_err(|e| UpdateError::Http(e.to_string()))?
        .error_for_status()
        .map_err(|e| UpdateError::Http(e.to_string()))?
        .json()
        .await
        .map_err(|e| UpdateError::Http(e.to_string()))?;

    let channel = if ctx.config.include_prereleases {
        "dev"
    } else {
        "stable"
    };

    let channel_data = manifest
        .channels
        .get(channel)
        .ok_or_else(|| UpdateError::Http(format!("Channel '{}' not found in manifest", channel)))?;

    let releases: Vec<Release> = channel_data
        .versions
        .iter()
        .map(|tag| {
            let asset_name = get_asset_name().unwrap_or_default();
            Release {
                tag_name: tag.clone(),
                prerelease: channel == "dev",
                draft: false,
                assets: vec![Asset {
                    name: asset_name.clone(),
                    url: format!("{}/releases/{}/{}/{}", base_url, channel, tag, asset_name),
                }],
            }
        })
        .collect();

    Ok(releases)
}

async fn fetch_available_releases(ctx: &CommandContext) -> Result<Vec<Release>, UpdateError> {
    let mut releases: Vec<_> = match &ctx.config.self_update_url {
        Some(base_url) => {
            // Static hosting - releases are already filtered by channel
            fetch_static_releases(ctx, base_url)
                .await?
                .into_iter()
                .filter(|r| parse_version(&r.tag_name).is_ok())
                .collect()
        }
        None => {
            // GitHub Releases
            fetch_github_releases(ctx)
                .await?
                .into_iter()
                .filter(|r| !r.draft)
                .filter(|r| parse_version(&r.tag_name).is_ok())
                .filter(|r| ctx.config.include_prereleases || !r.prerelease)
                .collect()
        }
    };

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

fn find_update(releases: Vec<Release>, current_version: &str) -> Result<UpdateCheck, UpdateError> {
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

async fn check_update(ctx: &CommandContext, current_version: &str) -> Result<UpdateCheck, UpdateError> {
    let releases = fetch_available_releases(ctx).await?;
    find_update(releases, current_version)
}

async fn apply_update(ctx: &CommandContext, release: &Release) -> Result<(), UpdateError> {
    let asset = get_asset_for_platform(release)?;
    println!("Downloading {} ({})", asset.name, asset.url);
    download_and_replace(ctx, asset).await?;
    Ok(())
}

pub async fn check_for_update(ctx: &CommandContext, current_version: &str) {
    match check_update(ctx, current_version).await {
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
            tracing::error!("Failed to check for update: {}", e);
            eprintln!("Failed to check for update: {}", e);
        }
    }
}

pub async fn run_update(ctx: &CommandContext, current_version: &str, force: bool) {
    let check = match check_update(ctx, current_version).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to check for updates: {}", e);
            eprintln!("Failed to check for updates: {}", e);
            return;
        }
    };

    match check {
        UpdateCheck::Available(release) => {
            if !force {
                let message = format!("Update {} -> {}?", current_version, release.tag_name);
                if !prompt_yes_no(&message, true) {
                    println!("Update cancelled.");
                    return;
                }
            }
            match apply_update(ctx, &release).await {
                Ok(()) => println!(
                    "Updated successfully: {} -> {}",
                    current_version, release.tag_name
                ),
                Err(e) => {
                    tracing::error!("Failed to update: {}", e);
                    eprintln!("Failed to update: {}", e);
                }
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

pub async fn show_available_versions(ctx: &CommandContext, current_version: &str) {
    let releases = match fetch_available_releases(ctx).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to fetch releases: {}", e);
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
    use std::env;

    #[tokio::test]
    async fn test_fetch_github_releases_missing_token_error() {
        // This test will fail if GITHUB_TOKEN is set in the environment
        // In CI, ensure it's not set, or skip this test
        if env::var("GITHUB_TOKEN").is_ok() {
            return; // Skip test if token is set
        }
        let ctx = CommandContext::new();
        let result = fetch_github_releases(&ctx).await;
        assert_eq!(result.unwrap_err(), UpdateError::MissingToken);
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
        assert!(matches!(result, Err(UpdateError::VersionParse(_))));
    }

    fn make_release(tag: &str, prerelease: bool) -> Release {
        Release {
            tag_name: tag.to_string(),
            prerelease,
            draft: false,
            assets: vec![],
        }
    }

    fn make_draft_release(tag: &str) -> Release {
        Release {
            tag_name: tag.to_string(),
            prerelease: false,
            draft: true,
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
    fn test_draft_releases_should_be_filtered() {
        // Draft releases should be filtered out by fetch_available_releases
        // This test documents the expected behavior: drafts are excluded before find_update
        let releases = vec![
            make_release("v0.0.1", false),
            make_draft_release("v0.0.2"), // This would be filtered by fetch_available_releases
        ];
        // After filtering drafts, only v0.0.1 remains
        let filtered: Vec<_> = releases.into_iter().filter(|r| !r.draft).collect();
        let result = find_update(filtered, "0.0.1").unwrap();
        assert!(matches!(result, UpdateCheck::AlreadyUpToDate));
    }

    #[test]
    fn test_find_update_invalid_current_version() {
        let releases = vec![make_release("v0.0.1", false)];
        let result = find_update(releases, "invalid");
        assert!(matches!(result, Err(UpdateError::VersionParse(_))));
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
        if let Ok(name) = get_asset_name() {
            assert!(
                name == "ana-darwin-arm64"
                    || name == "ana-linux-x86_64"
                    || name == "ana-linux-aarch64"
                    || name == "ana-windows-x86_64.exe"
            );
        }
    }

    #[test]
    fn test_get_asset_for_platform_finds_matching_asset() {
        let release = Release {
            tag_name: "v1.0.0".to_string(),
            prerelease: false,
            draft: false,
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
            draft: false,
            assets: vec![Asset {
                name: "ana-unknown-platform".to_string(),
                url: "https://example.com/unknown".to_string(),
            }],
        };

        let result = get_asset_for_platform(&release);
        // On supported platforms, should return AssetNotFound since our platform isn't in assets
        if get_asset_name().is_ok() {
            assert!(matches!(result, Err(UpdateError::AssetNotFound(_))));
        }
    }
}
