use clap::Subcommand;

use super::search_forum::{DEFAULT_PAGE, DEFAULT_PAGE_SIZE};

#[derive(Subcommand)]
pub enum IntelCommands {
    /// Show versions, platforms, dependencies, build variants, and CVEs for a
    /// conda package
    #[command(override_usage = "ana intel package-info <NAME> [options]")]
    PackageInfo {
        /// Exact package name, e.g. numpy
        name: String,

        /// Exact version, e.g. 2.5.1 [default: latest]
        #[arg(long)]
        version: Option<String>,

        /// Channel, e.g. main [default: search main, main-x and msys2]
        #[arg(long)]
        channel: Option<String>,

        /// Platform subdir, e.g. linux-64 or osx-arm64; adds a note if unavailable
        #[arg(long)]
        platform: Option<String>,

        /// Python version, e.g. 3.12; adds a note if unsupported
        #[arg(long)]
        python_version: Option<String>,

        /// Print JSON even in a terminal (JSON is the default when output is
        /// piped)
        #[arg(long)]
        json: bool,
    },

    /// Search the Anaconda community forum for troubleshooting threads
    #[command(override_usage = "ana intel search-forum <QUERY>... [options]")]
    SearchForum {
        /// Search text; error messages and symptom phrases work well
        #[arg(required = true, num_args = 1..)]
        query: Vec<String>,

        /// Page of results, starting at 1
        #[arg(long, default_value_t = DEFAULT_PAGE, allow_negative_numbers = true)]
        page: i64,

        /// Threads per page (1-100)
        #[arg(long, default_value_t = DEFAULT_PAGE_SIZE, allow_negative_numbers = true)]
        page_size: i64,

        /// Print JSON even in a terminal (JSON is the default when output is
        /// piped)
        #[arg(long)]
        json: bool,
    },
}

impl IntelCommands {
    /// Telemetry action name.
    pub fn action_name(&self) -> &'static str {
        match self {
            IntelCommands::PackageInfo { .. } => "intel.package-info",
            IntelCommands::SearchForum { .. } => "intel.search-forum",
        }
    }

    /// Whether `--json` was passed.
    pub fn force_json(&self) -> bool {
        match self {
            IntelCommands::PackageInfo { json, .. } | IntelCommands::SearchForum { json, .. } => {
                *json
            }
        }
    }
}
