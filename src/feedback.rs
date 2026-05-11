use crate::ui::status;

const GITHUB_ISSUES_URL: &str = "https://github.com/anaconda/ana-cli/issues/new/choose";

pub fn open_feedback() {
    eprintln!(
        "{} {}",
        status::dim("Opening"),
        status::highlight(GITHUB_ISSUES_URL)
    );
    if let Err(e) = webbrowser::open(GITHUB_ISSUES_URL) {
        status::error(&format!("Failed to open browser: {}", e));
    }
}
