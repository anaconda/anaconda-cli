use std::collections::HashMap;

use serde::Deserialize;
use tokio::io::AsyncWriteExt;

use crate::context::CommandContext;
use crate::errors::UpdateError;
use crate::ui::progress::build_progress_bar;

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
    pub published_at: Option<String>,
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
        ctx.github_client()
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
    let total_mb = total_size as f64 / 1_000_000.0;

    use crate::ui::styles::UiColor;

    eprintln!("  Downloading {} ({:.1} MB)", asset.name, total_mb);
    eprintln!("  {}", UiColor::Dim.apply_to(&asset.url));

    let pb = build_progress_bar(total_size);

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

    // Clear the "Downloading" and URL lines (move up 2 lines and clear each)
    use std::io::IsTerminal;
    if std::io::stderr().is_terminal() {
        eprint!("\x1b[2A\x1b[K\x1b[1B\x1b[K\x1b[1A");
    }

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
    let url = format!(
        "https://api.github.com/repos/{}/releases?per_page=100",
        GITHUB_REPO
    );
    let releases: Vec<Release> = ctx
        .github_client()
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
                published_at: None,
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

async fn check_update(
    ctx: &CommandContext,
    current_version: &str,
) -> Result<UpdateCheck, UpdateError> {
    let releases = fetch_available_releases(ctx).await?;
    find_update(releases, current_version)
}

async fn apply_update(ctx: &CommandContext, release: &Release) -> Result<(), UpdateError> {
    let asset = get_asset_for_platform(release)?;
    download_and_replace(ctx, asset).await?;
    Ok(())
}

/// Format a relative time string for display purposes only (e.g., "released 2 months ago").
/// Precision is approximate and intended for human guidance, not exact calculations.
fn format_relative_time_since(published_at: &str) -> String {
    use chrono::{DateTime, Datelike, Utc};
    let published: DateTime<Utc> = match published_at.parse() {
        Ok(dt) => dt,
        Err(_) => return String::new(),
    };
    let now = Utc::now();

    let days = now.signed_duration_since(published).num_days();
    if days < 2 {
        return if days == 0 {
            "released today".to_string()
        } else {
            "released yesterday".to_string()
        };
    }

    let years = now.year() - published.year();
    let months = years * 12 + (now.month() as i32 - published.month() as i32);

    if months < 1 {
        format!("released {} days ago", days)
    } else if months < 12 {
        if months == 1 {
            "released 1 month ago".to_string()
        } else {
            format!("released {} months ago", months)
        }
    } else if years == 1 {
        "released 1 year ago".to_string()
    } else {
        format!("released {} years ago", years)
    }
}

pub async fn check_for_update(ctx: &CommandContext, current_version: &str) {
    use crate::input::prompt_yes_no;
    use crate::ui::status;

    match check_update(ctx, current_version).await {
        Ok(UpdateCheck::Available(release)) => {
            eprintln!("  {}", status::section("UPDATE AVAILABLE"));
            eprintln!();
            let relative_time = release
                .published_at
                .as_ref()
                .map(|p| {
                    format!(
                        " {}",
                        status::dim(&format!("({})", format_relative_time_since(p)))
                    )
                })
                .unwrap_or_default();
            eprintln!(
                "  {:<10} {}{}",
                "Latest:",
                status::highlight(&release.tag_name),
                relative_time
            );
            eprintln!("  {:<10} v{}", "Current:", current_version);
            eprintln!();

            if prompt_yes_no("Do you want to update?", true) {
                let start = std::time::Instant::now();
                match apply_update(ctx, &release).await {
                    Ok(()) => {
                        print_update_success(current_version, &release.tag_name, start.elapsed())
                    }
                    Err(e) => {
                        tracing::error!("Failed to update: {}", e);
                        status::error(&format!("Failed to update: {}", e));
                    }
                }
            }
        }
        Ok(UpdateCheck::AlreadyUpToDate) => {
            eprintln!("  {}", status::section("UP TO DATE"));
            eprintln!();
            eprintln!("  {:<10} v{}", "Current:", current_version);
            eprintln!();
        }
        Ok(UpdateCheck::NoReleases) => {
            status::warn("No releases available.");
        }
        Err(e) => {
            tracing::error!("Failed to check for update: {}", e);
            status::error(&format!("Failed to check for update: {}", e));
        }
    }
}

fn print_update_success(current_version: &str, new_version: &str, elapsed: std::time::Duration) {
    use crate::ui::status;
    eprintln!();
    eprintln!(
        "  {} {}",
        status::section("UPDATED"),
        status::dim(&format!("{:.1}s", elapsed.as_secs_f64()))
    );
    eprintln!("  was v{} → now {}", current_version, new_version);
    eprintln!();
}

fn print_up_to_date(current_version: &str) {
    use crate::ui::status;
    eprintln!("  {}", status::section("UP TO DATE"));
    eprintln!();
    eprintln!("  {:<10} v{}", "Current:", current_version);
    eprintln!();
}

pub async fn run_update(
    ctx: &CommandContext,
    current_version: &str,
    target_version: Option<String>,
    force: bool,
) {
    use crate::ui::status;

    if let Some(version) = target_version {
        // Normalize version tag (ensure v prefix)
        let target_tag = if version.starts_with('v') {
            version
        } else {
            format!("v{}", version)
        };

        // Check if already on this version (unless --force is used)
        let current_tag = format!("v{}", current_version);
        if !force && target_tag == current_tag {
            print_up_to_date(current_version);
            return;
        }

        let releases = match fetch_available_releases(ctx).await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("Failed to fetch releases: {}", e);
                status::error(&format!("Failed to fetch releases: {}", e));
                return;
            }
        };

        let release = match releases.into_iter().find(|r| r.tag_name == target_tag) {
            Some(r) => r,
            None => {
                status::error(&format!(
                    "Version {} not found. Use --list to see available versions.",
                    target_tag
                ));
                return;
            }
        };

        let start = std::time::Instant::now();
        match apply_update(ctx, &release).await {
            Ok(()) => print_update_success(current_version, &release.tag_name, start.elapsed()),
            Err(e) => {
                tracing::error!("Failed to update: {}", e);
                status::error(&format!("Failed to update: {}", e));
            }
        }
    } else {
        // Update to latest version
        let check = match check_update(ctx, current_version).await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Failed to check for updates: {}", e);
                status::error(&format!("Failed to check for updates: {}", e));
                return;
            }
        };

        match check {
            UpdateCheck::Available(release) => {
                let start = std::time::Instant::now();
                match apply_update(ctx, &release).await {
                    Ok(()) => {
                        print_update_success(current_version, &release.tag_name, start.elapsed())
                    }
                    Err(e) => {
                        tracing::error!("Failed to update: {}", e);
                        status::error(&format!("Failed to update: {}", e));
                    }
                }
            }
            UpdateCheck::AlreadyUpToDate => {
                print_up_to_date(current_version);
            }
            UpdateCheck::NoReleases => {
                status::warn("No releases available.");
            }
        }
    }
}

fn format_release_date(published_at: &str) -> String {
    use chrono::{DateTime, Utc};
    let published: DateTime<Utc> = match published_at.parse() {
        Ok(dt) => dt,
        Err(_) => return String::new(),
    };
    published.format("%b %d, %Y").to_string()
}

pub async fn show_available_versions(ctx: &CommandContext, current_version: &str) {
    use crate::ui::status;

    let releases = match fetch_available_releases(ctx).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to fetch releases: {}", e);
            status::error(&format!("Failed to fetch releases: {}", e));
            return;
        }
    };

    if releases.is_empty() {
        status::warn("No releases available.");
        return;
    }

    let current_tag = format!("v{}", current_version);
    let latest_tag = releases.first().map(|r| r.tag_name.as_str());

    // Find longest version for alignment (+ 4 space gap)
    let pad_width = releases.iter().map(|r| r.tag_name.len()).max().unwrap_or(0) + 4;

    eprintln!("  {}", status::section("Available versions"));
    for release in &releases {
        let is_current = release.tag_name == current_tag;
        let is_latest = Some(release.tag_name.as_str()) == latest_tag;

        let date_str = release
            .published_at
            .as_ref()
            .map(|p| status::dim(&format_release_date(p)));

        let tag = if is_current {
            Some(status::section("Current"))
        } else if is_latest {
            Some(status::highlight("Latest"))
        } else {
            None
        };

        // Pad version for alignment only when dates are present
        let version_display = if date_str.is_some() {
            format!("{:<width$}", release.tag_name, width = pad_width)
        } else {
            release.tag_name.clone()
        };

        // Apply color after padding (ANSI codes break width formatting)
        let version_str = if is_current {
            status::section(&version_display)
        } else if is_latest {
            status::highlight(&version_display)
        } else {
            version_display
        };

        match (&date_str, &tag) {
            (Some(date), Some(t)) => eprintln!("  {}{}    {}", version_str, date, t),
            (Some(date), None) => eprintln!("  {}{}", version_str, date),
            (None, Some(t)) => eprintln!("  {}    {}", version_str, t),
            (None, None) => eprintln!("  {}", version_str),
        }
    }

    eprintln!();
    eprintln!("  Update to the latest version:");
    eprintln!("    {}", status::highlight("ana self update"));
    eprintln!();
    eprintln!("  Update to a specific version:");
    eprintln!(
        "    {} {}",
        status::highlight("ana self update"),
        status::dim("[VERSION]")
    );
    eprintln!();
}

#[cfg(test)]
mod tests {
    use super::*;

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
            published_at: None,
        }
    }

    fn make_draft_release(tag: &str) -> Release {
        Release {
            tag_name: tag.to_string(),
            prerelease: false,
            draft: true,
            assets: vec![],
            published_at: None,
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
            published_at: None,
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
            published_at: None,
        };

        let result = get_asset_for_platform(&release);
        // On supported platforms, should return AssetNotFound since our platform isn't in assets
        if get_asset_name().is_ok() {
            assert!(matches!(result, Err(UpdateError::AssetNotFound(_))));
        }
    }
}
