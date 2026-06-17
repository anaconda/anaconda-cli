//! Progress bar utilities for download and transfer operations.

use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

use super::styles::UiColor;

/// Build a styled progress bar for download operations.
pub fn build_progress_bar(total_size: u64) -> ProgressBar {
    let pb = ProgressBar::new(total_size);
    let dim = UiColor::Dim.hex();
    let dim_suffix = UiColor::Dim.apply_to("% |").to_string();
    let template = format!(
        "  {{bar:34.{}/{dim}}} {{percent:>2.{dim}}}{dim_suffix} {{elapsed:.{dim}}}",
        UiColor::Green.hex(),
    );
    pb.set_style(
        ProgressStyle::default_bar()
            .template(&template)
            .unwrap()
            .progress_chars("━━─"),
    );
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}
