mod anaconda_cli;
mod auth;
mod cli;
mod config;
mod context;
mod diagnostics;
mod error_handler;
pub mod errors;
mod feature;
mod feedback;
mod fetch;
mod help;
mod http;
mod input;
mod installer;
mod mcp;
#[cfg(unix)]
mod outerbounds;
mod paths;
mod qr;
mod table;
mod telemetry;
mod tools;
mod ua;
mod ui;
mod update;
mod utils;

pub const VERSION: &str = env!("PKG_VERSION");

fn prepare_runtime() {
    #[cfg(unix)]
    {
        // Reset SIGPIPE to default behavior so the process terminates cleanly when
        // output is piped to commands like `head` or `grep -q`. Rust ignores SIGPIPE
        // by default, which causes panics on broken pipe errors.
        unsafe {
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
        }

        // Raise RLIMIT_NOFILE for rattler installations. On macOS, respects
        // kern.maxfilesperproc (the real hard ceiling).
        match rlimit::increase_nofile_limit(2048) {
            Ok(n) => tracing::debug!(limit = n, "RLIMIT_NOFILE raised"),
            Err(e) => tracing::warn!(error = %e, "Failed to raise RLIMIT_NOFILE"),
        }
    }
}

#[tokio::main]
async fn main() {
    // Apply platform-specific runtime modifications
    prepare_runtime();

    // Install custom error handler before any errors can occur
    error_handler::CliErrorHandler::install();

    let config = config::Config::load();
    #[allow(clippy::let_unit_value)]
    let _diagnostics_guard = diagnostics::init(&config);
    cli::execute().await;

    // Flush any AAU tokens that were deferred because their target
    // directory didn't exist yet (e.g. environment tokens for a newly
    // created conda environment).
    if let Err(e) = ua::finalize_deferred_writes() {
        tracing::error!("Failed to flush deferred AAU token writes: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_is_set() {
        assert!(!VERSION.is_empty());
    }
}
