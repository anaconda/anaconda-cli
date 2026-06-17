//! Custom error reporting handler for consistent CLI error formatting.

use miette::{Diagnostic, ReportHandler};

use crate::ui::styles::UiColor;

/// Custom miette ReportHandler that formats errors in our CLI style.
pub struct CliErrorHandler;

impl CliErrorHandler {
    /// Install this handler as the global miette report handler.
    pub fn install() {
        let _ = miette::set_hook(Box::new(|_| Box::new(CliErrorHandler)));
    }

    /// Find a help hint from the error or its related diagnostics.
    fn find_help(error: &dyn Diagnostic) -> Option<String> {
        if let Some(help) = error.help() {
            return Some(help.to_string());
        }

        if let Some(related) = error.related() {
            for r in related {
                if let Some(help) = r.help() {
                    return Some(help.to_string());
                }
            }
        }

        if let Some(source) = error.diagnostic_source()
            && let Some(help) = source.help()
        {
            return Some(help.to_string());
        }

        None
    }
}

impl ReportHandler for CliErrorHandler {
    fn debug(
        &self,
        error: &dyn Diagnostic,
        f: &mut core::fmt::Formatter<'_>,
    ) -> core::fmt::Result {
        write!(f, "{} {}", UiColor::Red.apply_to("✗ Error:"), error)?;

        if let Some(help) = Self::find_help(error) {
            write!(f, "\n{}", UiColor::Dim.apply_to(format!("Tip: {}", help)))?;
        }

        Ok(())
    }
}
