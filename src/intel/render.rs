//! Text rendering for people running `ana intel` in a terminal.
//!
//! Plain labeled lines rather than tables: values are nested and long, and a
//! width-limited table would wrap or truncate them. Agents get JSON instead
//! (see `output.rs`), so this format is free to change.

use std::fmt::Write;

use super::schemas::{ForumSearchResponse, PackageInfoResponse};
use crate::ui::status;

const LABEL_WIDTH: usize = 11;

fn field(out: &mut String, label: &str, value: &str) {
    let _ = writeln!(out, "  {:<LABEL_WIDTH$} {value}", format!("{label}:"));
}

fn section(out: &mut String, title: &str, lines: impl IntoIterator<Item = String>) {
    let mut lines = lines.into_iter().peekable();
    if lines.peek().is_none() {
        return;
    }
    let _ = writeln!(out, "\n{title}");
    for line in lines {
        let _ = writeln!(out, "  {line}");
    }
}

/// `ana intel package-info` in a terminal.
pub fn package_info(info: &PackageInfoResponse) -> String {
    let mut out = String::new();
    let version = info.version.as_deref().unwrap_or("(version unknown)");
    let _ = writeln!(
        out,
        "{} {version} {}",
        status::highlight(&info.name),
        status::dim(&format!("({})", info.channel))
    );
    if let Some(summary) = &info.summary {
        field(&mut out, "Summary", summary);
    }
    if let Some(license) = &info.license {
        field(&mut out, "License", license);
    }
    if !info.platforms.is_empty() {
        field(&mut out, "Platforms", &info.platforms.join(", "));
    }
    if let Some(python) = &info.python_versions {
        field(&mut out, "Python", &python.join(", "));
    }
    if let Some(doc_url) = &info.doc_url {
        field(&mut out, "Docs", doc_url);
    }
    if let (Some(total), Some(channel)) = (info.download_count.total, &info.download_count.channel)
    {
        field(&mut out, "Downloads", &format!("{total} ({channel})"));
    }
    let cves = match (info.cve_summary.count, info.cve_summary.critical) {
        (Some(count), Some(critical)) => format!("{count} actionable ({critical} critical)"),
        _ => "unknown".to_string(),
    };
    field(&mut out, "CVEs", &cves);
    if let Some(status) = &info.policy_status {
        field(&mut out, "Policy", status);
    }
    if let Some(detail) = &info.policy_detail {
        field(&mut out, "", detail);
    }

    section(
        &mut out,
        "Dependency constraints",
        info.notable_constraints.iter().flatten().cloned(),
    );
    section(
        &mut out,
        "Build variants",
        info.build_variants.iter().flatten().map(|variant| {
            format!(
                "{:<12} {}",
                variant.label,
                status::dim(&format!("conda install \"{}\"", variant.selector))
            )
        }),
    );
    section(
        &mut out,
        "Notes",
        info.notes.iter().map(|note| format!("- {note}")),
    );
    out
}

/// `ana intel search-forum` in a terminal.
pub fn search_forum(results: &ForumSearchResponse) -> String {
    let mut out = String::new();
    if results.topics.is_empty() {
        let _ = writeln!(out, "No forum threads found.");
    } else {
        let _ = writeln!(
            out,
            "{} result(s), page {} of {}",
            results.total, results.pagination.page, results.pagination.total_pages
        );
        let first_number = (results.pagination.page - 1) * results.pagination.page_size + 1;
        for (offset, topic) in results.topics.iter().enumerate() {
            let _ = writeln!(
                out,
                "\n{}. {}",
                first_number + offset as i64,
                status::highlight(&topic.title)
            );
            if let Some(url) = &topic.url {
                let _ = writeln!(out, "   {url}");
            }
            let mut meta = vec![
                topic.category.clone(),
                format!("{} views", topic.views),
                format!("{} replies", topic.reply_count),
            ];
            if let Some(created) = &topic.created_at {
                meta.push(created.chars().take(10).collect());
            }
            let _ = writeln!(out, "   {}", status::dim(&meta.join(" · ")));
            if let Some(blurb) = topic.blurbs.first() {
                let _ = writeln!(out, "   \"{}\"", blurb.blurb);
            }
        }
    }
    section(
        &mut out,
        "Notes",
        results.notes.iter().map(|note| format!("- {note}")),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::super::schemas::{
        BuildVariant, CveSummary, DownloadCount, ForumBlurb, ForumPagination, ForumTopic,
    };
    use super::*;

    fn numpy() -> PackageInfoResponse {
        PackageInfoResponse {
            name: "numpy".into(),
            version: Some("2.4.1".into()),
            channel: "main".into(),
            platforms: vec!["linux-64".into(), "osx-arm64".into()],
            python_versions: Some(vec!["3.12".into(), "3.13".into()]),
            license: Some("BSD-3-Clause".into()),
            notable_constraints: Some(vec!["libcxx >=17".into()]),
            build_variants: Some(vec![
                BuildVariant {
                    label: "mkl".into(),
                    selector: "blas=*=mkl".into(),
                },
                BuildVariant {
                    label: "openblas".into(),
                    selector: "blas=*=openblas".into(),
                },
            ]),
            summary: Some("Array processing.".into()),
            doc_url: Some("https://numpy.org/doc".into()),
            download_count: DownloadCount {
                total: Some(10),
                channel: Some("main".into()),
            },
            cve_summary: CveSummary {
                count: Some(2),
                critical: Some(1),
            },
            policy_status: None,
            policy_detail: None,
            notes: vec!["Also available on: main-x (2.5.0)".into()],
        }
    }

    #[test]
    fn renders_package_info() {
        let text = package_info(&numpy());
        assert!(text.starts_with("numpy 2.4.1 (main)\n"), "{text}");
        assert!(
            text.contains("  Platforms:  linux-64, osx-arm64\n"),
            "{text}"
        );
        assert!(text.contains("  Python:     3.12, 3.13\n"));
        assert!(text.contains("  CVEs:       2 actionable (1 critical)\n"));
        assert!(text.contains("\nDependency constraints\n  libcxx >=17\n"));
        assert!(text.contains("mkl          conda install \"blas=*=mkl\""));
        assert!(text.contains("\nNotes\n  - Also available on: main-x (2.5.0)\n"));
        assert!(!text.contains("Policy"));
    }

    #[test]
    fn package_info_skips_missing_fields() {
        let mut info = numpy();
        info.version = None;
        info.summary = None;
        info.python_versions = None;
        info.notable_constraints = None;
        info.build_variants = None;
        info.notes = vec![];
        info.cve_summary = CveSummary {
            count: None,
            critical: None,
        };
        let text = package_info(&info);
        assert!(text.starts_with("numpy (version unknown) (main)\n"));
        assert!(!text.contains("Summary:"));
        assert!(!text.contains("Python:"));
        assert!(!text.contains("Dependency constraints"));
        assert!(!text.contains("Notes"));
        assert!(text.contains("  CVEs:       unknown\n"));
    }

    fn topic(id: i64, title: &str) -> ForumTopic {
        ForumTopic {
            id,
            title: title.into(),
            url: Some(format!("https://forum.anaconda.com/t/x/{id}")),
            category: "Product Help".into(),
            posts_count: 2,
            reply_count: 1,
            views: 50,
            created_at: Some("2024-01-02T03:04:05Z".into()),
            last_posted_at: None,
            blurbs: vec![ForumBlurb {
                username: "u".into(),
                blurb: "Try conda update".into(),
                post_number: 1,
            }],
        }
    }

    #[test]
    fn renders_forum_results_with_global_numbering() {
        let results = ForumSearchResponse {
            topics: vec![topic(7, "Solver error"), topic(8, "Killed")],
            total: 12,
            pagination: ForumPagination {
                page: 2,
                page_size: 10,
                total_pages: 2,
            },
            notes: vec!["a note".into()],
        };
        let text = search_forum(&results);
        assert!(text.starts_with("12 result(s), page 2 of 2\n"), "{text}");
        assert!(text.contains("\n11. Solver error\n   https://forum.anaconda.com/t/x/7\n"));
        assert!(text.contains("   Product Help · 50 views · 1 replies · 2024-01-02\n"));
        assert!(text.contains("   \"Try conda update\"\n"));
        assert!(text.contains("\n12. Killed\n"));
        assert!(text.ends_with("\nNotes\n  - a note\n"));
    }

    #[test]
    fn renders_empty_forum_results() {
        let results = ForumSearchResponse {
            topics: vec![],
            total: 0,
            pagination: ForumPagination {
                page: 1,
                page_size: 10,
                total_pages: 0,
            },
            notes: vec![],
        };
        assert_eq!(search_forum(&results), "No forum threads found.\n");
    }
}
