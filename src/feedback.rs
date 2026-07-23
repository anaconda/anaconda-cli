use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::config::Config;
use crate::ui::status;

const GITHUB_ISSUES_URL: &str = "https://github.com/anaconda/ana-cli/issues/new/choose";
const BROWSER_TIMEOUT: Duration = Duration::from_secs(2);

pub fn open_feedback(config: &Config) {
    eprintln!(
        "{} {}",
        status::dim("Opening"),
        status::highlight(GITHUB_ISSUES_URL)
    );

    if !config.open_browser {
        return;
    }

    let (tx, rx) = mpsc::channel();
    let url = GITHUB_ISSUES_URL.to_string();

    thread::spawn(move || {
        let result = webbrowser::open(&url);
        let _ = tx.send(result);
    });

    match rx.recv_timeout(BROWSER_TIMEOUT) {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            status::error(&format!("Failed to open browser: {}", e));
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            status::warn("Browser open timed out");
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            status::error("Browser thread disconnected unexpectedly");
        }
    }
}
