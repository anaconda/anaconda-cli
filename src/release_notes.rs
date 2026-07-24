//! Release notes fetching and display.
//!
//! Fetches structured release notes from anaconda.sh and displays them
//! in a user-friendly format after updates.

use serde::Deserialize;

use crate::context::CommandContext;
use crate::errors::UpdateError;
use crate::ui::status;

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseNotes {
    #[serde(default)]
    pub sections: Sections,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Sections {
    #[serde(default, rename = "what's_changed")]
    pub whats_changed: Vec<ChangeEntry>,
    #[serde(default)]
    pub bug_fixes: Vec<ChangeEntry>,
    #[serde(default)]
    pub new_features: Vec<ChangeEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChangeEntry {
    pub description: String,
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

fn strip_conventional_prefix(description: &str) -> &str {
    let prefixes = [
        "fix: ", "feat: ", "chore: ", "refac: ", "docs: ", "test: ", "build: ", "ci: ",
    ];
    for prefix in prefixes {
        if let Some(stripped) = description.strip_prefix(prefix) {
            return stripped;
        }
    }
    if let Some(pos) = description.find("): ") {
        return &description[pos + 3..];
    }
    description
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

    display_changelog_sections(&notes);
    eprintln!();
}

fn display_changelog_sections(notes: &ReleaseNotes) {
    let has_features = !notes.sections.new_features.is_empty();
    let has_fixes = !notes.sections.bug_fixes.is_empty();
    let has_changes = !notes.sections.whats_changed.is_empty();

    if !has_features && !has_fixes && !has_changes {
        eprintln!();
        eprintln!("  {}", status::dim("No notable changes."));
        return;
    }

    if has_features {
        eprintln!();
        eprintln!("  {}", status::highlight("New Features"));
        for entry in &notes.sections.new_features {
            let desc = strip_conventional_prefix(&entry.description);
            eprintln!("  • {}", desc);
        }
    }

    if has_fixes {
        eprintln!();
        eprintln!("  {}", status::highlight("Bug Fixes"));
        for entry in &notes.sections.bug_fixes {
            let desc = strip_conventional_prefix(&entry.description);
            eprintln!("  • {}", desc);
        }
    }

    if has_changes {
        eprintln!();
        eprintln!("  {}", status::highlight("Changes"));
        for entry in &notes.sections.whats_changed {
            let desc = strip_conventional_prefix(&entry.description);
            eprintln!("  • {}", desc);
        }
    }
}

pub fn display_release_notes(notes: &ReleaseNotes) {
    let has_features = !notes.sections.new_features.is_empty();
    let has_fixes = !notes.sections.bug_fixes.is_empty();
    let has_changes = !notes.sections.whats_changed.is_empty();

    if !has_features && !has_fixes && !has_changes {
        return;
    }

    eprintln!();
    eprintln!("  {}", status::section("WHAT'S NEW"));

    if has_features {
        for entry in &notes.sections.new_features {
            let desc = strip_conventional_prefix(&entry.description);
            eprintln!("  {} {}", status::highlight("•"), desc);
        }
    }

    if has_fixes {
        for entry in &notes.sections.bug_fixes {
            let desc = strip_conventional_prefix(&entry.description);
            eprintln!("  {} {}", status::dim("•"), desc);
        }
    }

    if has_changes {
        for entry in &notes.sections.whats_changed {
            let desc = strip_conventional_prefix(&entry.description);
            eprintln!("  {} {}", status::dim("•"), desc);
        }
    }
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
    fn test_strip_conventional_prefix_fix() {
        assert_eq!(
            strip_conventional_prefix("fix: Clarify shell restart needed"),
            "Clarify shell restart needed"
        );
    }

    #[test]
    fn test_strip_conventional_prefix_feat() {
        assert_eq!(
            strip_conventional_prefix("feat: Add new feature"),
            "Add new feature"
        );
    }

    #[test]
    fn test_strip_conventional_prefix_with_scope() {
        assert_eq!(
            strip_conventional_prefix("chore(deps): Update dependencies"),
            "Update dependencies"
        );
    }

    #[test]
    fn test_strip_conventional_prefix_no_prefix() {
        assert_eq!(
            strip_conventional_prefix("Just a description"),
            "Just a description"
        );
    }

    #[test]
    fn test_deserialize_release_notes() {
        let json = r#"{
            "tag": "v0.2.3",
            "sections": {
                "bug_fixes": [
                    {"description": "fix: Something broken", "author": "test", "pr_number": 1}
                ],
                "new_features": [
                    {"description": "feat: Cool feature", "author": "test", "pr_number": 2}
                ]
            }
        }"#;

        let notes: ReleaseNotes = serde_json::from_str(json).unwrap();
        assert_eq!(notes.sections.bug_fixes.len(), 1);
        assert_eq!(notes.sections.new_features.len(), 1);
    }

    #[test]
    fn test_deserialize_release_notes_empty_sections() {
        let json = r#"{
            "tag": "v0.2.3",
            "sections": {}
        }"#;

        let notes: ReleaseNotes = serde_json::from_str(json).unwrap();
        assert!(notes.sections.bug_fixes.is_empty());
        assert!(notes.sections.new_features.is_empty());
    }
}
