use crate::config::Config;
use crate::ui::status;

const GITHUB_ISSUES_URL: &str = "https://github.com/anaconda/ana-cli/issues/new/choose";

pub fn open_feedback(config: &Config) {
    eprintln!(
        "{} {}",
        status::dim("Opening"),
        status::highlight(GITHUB_ISSUES_URL)
    );

    if !config.open_browser {
        return;
    }

    if let Err(e) = webbrowser::open(GITHUB_ISSUES_URL) {
        status::error(&format!("Failed to open browser: {}", e));
    }
}
