//! Release notes fetching and display.
//!
//! Fetches release notes from anaconda.sh and renders the markdown body
//! in the terminal.

use serde::Deserialize;
use termimad::MadSkin;

use crate::context::CommandContext;
use crate::errors::UpdateError;
use crate::ui::status;

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseNotes {
    #[serde(default)]
    pub body: String,
}

fn build_release_notes_url(base_url: &str, channel: &str, tag: &str) -> String {
    format!(
        "{}/releases/{}/{}/release-notes.json",
        base_url, channel, tag
    )
}

pub async fn fetch_release_notes(
    ctx: &CommandContext,
    tag: &str,
) -> Result<ReleaseNotes, UpdateError> {
    let base_url = ctx.config.self_update_url.as_deref().ok_or_else(|| {
        UpdateError::Http("Release notes not available for GitHub releases".to_string())
    })?;

    let channel = if ctx.config.include_prereleases {
        "dev"
    } else {
        "stable"
    };

    let url = build_release_notes_url(base_url, channel, tag);

    let response = ctx
        .download_client()
        .get(&url)
        .send()
        .await
        .map_err(|e| UpdateError::Http(e.to_string()))?;

    if !response.status().is_success() {
        return Err(UpdateError::Http(format!(
            "Failed to fetch release notes: {}",
            response.status()
        )));
    }

    response
        .json()
        .await
        .map_err(|e| UpdateError::Http(e.to_string()))
}

fn make_skin() -> MadSkin {
    let mut skin = MadSkin::default();
    skin.set_headers_fg(termimad::crossterm::style::Color::Green);
    skin.bold.set_fg(termimad::crossterm::style::Color::Blue);
    skin.italic.set_fg(termimad::crossterm::style::Color::DarkGrey);
    skin
}

fn strip_html_comments(body: &str) -> String {
    let mut result = String::with_capacity(body.len());
    let mut chars = body.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '<'
            && chars.peek() == Some(&'!')
            && chars.clone().take(3).collect::<String>() == "!--"
        {
            // Skip until -->
            chars.next(); // !
            chars.next(); // -
            chars.next(); // -
            loop {
                match chars.next() {
                    Some('-') if chars.peek() == Some(&'-') => {
                        chars.next(); // second -
                        if chars.peek() == Some(&'>') {
                            chars.next(); // >
                            break;
                        }
                    }
                    None => break,
                    _ => {}
                }
            }
        } else {
            result.push(c);
        }
    }

    result.trim().to_string()
}

fn render_markdown(body: &str) {
    let clean_body = strip_html_comments(body);
    let skin = make_skin();
    let text = skin.text(&clean_body, None);
    eprint!("{}", text);
}

pub async fn show_changelog(ctx: &CommandContext, current_version: &str, version: Option<String>) {
    let tag = match version {
        Some(v) => {
            if v.starts_with('v') {
                v
            } else {
                format!("v{}", v)
            }
        }
        None => match crate::update::fetch_latest_version(ctx).await {
            Ok(v) => v,
            Err(e) => {
                status::error(&format!("Failed to fetch latest version: {}", e));
                return;
            }
        },
    };

    let notes = match fetch_release_notes(ctx, &tag).await {
        Ok(n) => n,
        Err(e) => {
            tracing::debug!("Failed to fetch release notes: {}", e);
            status::error(&format!("No changelog available for {}", tag));
            return;
        }
    };

    let is_current = tag.trim_start_matches('v') == current_version;

    eprintln!();
    if is_current {
        eprintln!(
            "  {} {}",
            status::section(&format!("CHANGELOG {}", tag)),
            status::dim("(current)")
        );
    } else {
        eprintln!("  {}", status::section(&format!("CHANGELOG {}", tag)));
    }
    eprintln!();

    if notes.body.is_empty() {
        eprintln!("  {}", status::dim("No changelog available."));
    } else {
        render_markdown(&notes.body);
    }
    eprintln!();
}

pub fn display_release_notes(notes: &ReleaseNotes) {
    if notes.body.is_empty() {
        return;
    }

    eprintln!();
    eprintln!("  {}", status::section("WHAT'S NEW"));
    eprintln!();
    render_markdown(&notes.body);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_release_notes_url() {
        let url = build_release_notes_url("https://anaconda.sh", "stable", "v0.2.3");
        assert_eq!(
            url,
            "https://anaconda.sh/releases/stable/v0.2.3/release-notes.json"
        );
    }

    #[test]
    fn test_build_release_notes_url_dev_channel() {
        let url = build_release_notes_url("https://anaconda.sh", "dev", "v0.2.4.dev1");
        assert_eq!(
            url,
            "https://anaconda.sh/releases/dev/v0.2.4.dev1/release-notes.json"
        );
    }

    #[test]
    fn test_deserialize_release_notes() {
        let json = r###"{"tag": "v0.2.3", "body": "## Changes\n\n* fix: Something"}"###;

        let notes: ReleaseNotes = serde_json::from_str(json).unwrap();
        assert!(notes.body.contains("Changes"));
    }

    #[test]
    fn test_deserialize_release_notes_empty_body() {
        let json = r#"{
            "tag": "v0.2.3"
        }"#;

        let notes: ReleaseNotes = serde_json::from_str(json).unwrap();
        assert!(notes.body.is_empty());
    }
}
