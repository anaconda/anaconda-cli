mod anaconda_cli;
mod auth;
mod cli;
mod config;
mod context;
mod diagnostics;
pub mod errors;
mod feature;
mod feedback;
mod fetch;
mod help;
mod http;
mod input;
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

pub const VERSION: &str = env!("PKG_VERSION");

/// Reset SIGPIPE to default behavior so the process terminates cleanly when
/// output is piped to commands like `head` or `grep -q`. Rust ignores SIGPIPE
/// by default, which causes panics on broken pipe errors.
#[cfg(unix)]
fn reset_sigpipe() {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn reset_sigpipe() {}

#[tokio::main]
async fn main() {
    reset_sigpipe();

    let config = config::Config::load();
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
