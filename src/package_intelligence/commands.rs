use clap::Subcommand;

use super::package_info::{SEGMENT_RULE, is_safe_segment};
use super::search_forum::{DEFAULT_PAGE, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE};

/// Longest accepted value for any text argument (matches the remote service).
pub const MAX_ARG_LENGTH: usize = 256;

// Argument validation happens here, in clap value parsers, like the rest of
// the CLI. A bad argument is reported by clap on stderr with exit code 2
// before any request is made. Each parser returns the trimmed value.

/// Non-blank text of at most `MAX_ARG_LENGTH` characters, trimmed. A
/// whitespace-only filter would be an exact-match term that never matches,
/// which would read back as a missing package.
fn text_value(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("must not be empty or whitespace".to_string());
    }
    if value.chars().count() > MAX_ARG_LENGTH {
        return Err(format!("must be at most {MAX_ARG_LENGTH} characters"));
    }
    Ok(value.to_string())
}

/// A package or channel name: `text_value` that is also safe to put in a
/// request path.
fn conda_name(value: &str) -> Result<String, String> {
    let value = text_value(value)?;
    if is_safe_segment(&value) {
        Ok(value)
    } else {
        Err(SEGMENT_RULE.to_string())
    }
}

/// `ana package` subcommands.
#[derive(Subcommand)]
pub enum PackageCommands {
    /// Show versions, platforms, dependencies, build variants, and CVEs for a
    /// conda package
    #[command(override_usage = "ana package info <NAME> [options]")]
    Info {
        /// Exact package name, e.g. numpy
        // Optional only so `--help` works without it (the CLI's global help
        // flag is ours, not clap's); clap requires it otherwise.
        #[arg(value_parser = conda_name, required_unless_present = "help")]
        name: Option<String>,

        /// Exact version, e.g. 2.5.1 [default: latest]
        #[arg(long, value_parser = text_value)]
        version: Option<String>,

        /// Channel, e.g. main [default: search main, main-x and msys2]
        #[arg(long, value_parser = conda_name)]
        channel: Option<String>,

        /// Platform subdir, e.g. linux-64 or osx-arm64; adds a note if unavailable
        #[arg(long, value_parser = text_value)]
        platform: Option<String>,

        /// Python version, e.g. 3.12; adds a note if unsupported
        #[arg(long, value_parser = text_value)]
        python_version: Option<String>,

        /// Organization to resolve the package through (subscriber channels and
        /// policy disposition)
        #[arg(long, value_parser = text_value)]
        org: Option<String>,

        /// Print JSON even in a terminal (JSON is the default when output is
        /// piped)
        #[arg(long)]
        json: bool,
    },

    /// Show policy-filtered CVE detail for a package in your organization
    #[command(override_usage = "ana package security <NAME> [options]")]
    Security {
        /// Exact package name, e.g. requests
        #[arg(value_parser = conda_name, required_unless_present = "help")]
        name: Option<String>,

        /// Exact version, e.g. 2.32.3
        #[arg(long, value_parser = text_value, required_unless_present = "help")]
        version: Option<String>,

        /// Channel, e.g. main
        #[arg(long, value_parser = conda_name, required_unless_present = "help")]
        channel: Option<String>,

        /// Platform subdir, e.g. linux-64; sets per-CVE status_on_platform
        #[arg(long, value_parser = text_value)]
        platform: Option<String>,

        /// Page of CVEs, starting at 1
        #[arg(
            long,
            default_value_t = 1,
            allow_negative_numbers = true,
            value_parser = clap::value_parser!(i64).range(1..)
        )]
        page: i64,

        /// Organization (default: your business organization)
        #[arg(long, value_parser = text_value)]
        org: Option<String>,

        /// Print JSON even in a terminal (JSON is the default when output is
        /// piped)
        #[arg(long)]
        json: bool,
    },

    /// Show your organization's channels and their package policies
    #[command(override_usage = "ana package policy [options]")]
    Policy {
        /// Organization (default: your business organization)
        #[arg(long, value_parser = text_value)]
        org: Option<String>,

        /// Print JSON even in a terminal (JSON is the default when output is
        /// piped)
        #[arg(long)]
        json: bool,
    },
}

/// `ana forum` subcommands.
#[derive(Subcommand)]
pub enum ForumCommands {
    /// Search the Anaconda community forum for troubleshooting threads
    #[command(override_usage = "ana forum search <QUERY>... [options]")]
    Search {
        /// Search text; error messages and symptom phrases work well
        #[arg(required_unless_present = "help", num_args = 1..)]
        query: Vec<String>,

        /// Page of results, starting at 1
        #[arg(
            long,
            default_value_t = DEFAULT_PAGE,
            allow_negative_numbers = true,
            value_parser = clap::value_parser!(i64).range(1..)
        )]
        page: i64,

        /// Threads per page (1-100)
        #[arg(
            long,
            default_value_t = DEFAULT_PAGE_SIZE,
            allow_negative_numbers = true,
            value_parser = clap::value_parser!(i64).range(1..=MAX_PAGE_SIZE)
        )]
        page_size: i64,

        /// Print JSON even in a terminal (JSON is the default when output is
        /// piped)
        #[arg(long)]
        json: bool,
    },
}

impl PackageCommands {
    /// Telemetry action name.
    pub fn action_name(&self) -> &'static str {
        match self {
            PackageCommands::Info { .. } => "package.info",
            PackageCommands::Security { .. } => "package.security",
            PackageCommands::Policy { .. } => "package.policy",
        }
    }
}

impl ForumCommands {
    /// Telemetry action name.
    pub fn action_name(&self) -> &'static str {
        match self {
            ForumCommands::Search { .. } => "forum.search",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_value_trims_and_bounds() {
        assert_eq!(text_value("  2.0  ").unwrap(), "2.0");
        assert_eq!(text_value(&"x".repeat(MAX_ARG_LENGTH)).unwrap().len(), 256);
        for bad in ["", "   ", "\t\n"] {
            assert_eq!(
                text_value(bad).unwrap_err(),
                "must not be empty or whitespace"
            );
        }
        assert!(text_value(&"x".repeat(MAX_ARG_LENGTH + 1)).is_err());
        // Length is measured after trimming.
        let padded = format!("  {}  ", "x".repeat(MAX_ARG_LENGTH));
        assert!(text_value(&padded).is_ok());
    }

    #[test]
    fn conda_name_rejects_unsafe_segments() {
        assert_eq!(conda_name(" numpy ").unwrap(), "numpy");
        assert_eq!(conda_name("scikit-learn").unwrap(), "scikit-learn");
        for bad in ["../etc", ".", "..", "a/b", "a b", "a%2f", "numpy?x=1"] {
            assert_eq!(conda_name(bad).unwrap_err(), SEGMENT_RULE, "{bad}");
        }
        assert_eq!(
            conda_name("  ").unwrap_err(),
            "must not be empty or whitespace"
        );
    }
}
