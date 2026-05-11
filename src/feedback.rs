const GITHUB_ISSUES_URL: &str = "https://github.com/anaconda/ana-cli/issues";

pub fn open_feedback() {
    println!("Opening GitHub issues: {}", GITHUB_ISSUES_URL);
    if let Err(e) = webbrowser::open(GITHUB_ISSUES_URL) {
        eprintln!("Failed to open browser: {}", e);
    }
}
