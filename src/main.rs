use std::env;

use anaconda_otel_rs::{
    attributes::ResourceAttributes, config::Configuration, signals::initialize_telemetry,
};

mod cli;
mod config;
mod update;

pub const VERSION: &str = env!("PKG_VERSION");

fn setup_telemetry() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = Configuration::new(
        Some("https://metrics.auth.anacondaconnect.com/v1/metrics"),
        None,
    )?;

    // Set auth token from environment variable
    let api_key = env::var("OTEL_API_KEY").ok();
    config.set_auth_token(api_key);

    // Disable console exporter to use OTLP HTTP
    config.set_console_exporter(false);

    // Set a short export interval (1 second) so we don't have to wait 60s to see results
    config.set_metrics_export_interval_ms(1000);
    config.skip_internet_check = true;

    // 2. Setup resource attributes
    let attrs = ResourceAttributes::new("ana-cli", VERSION)?;

    // 3. Initialize telemetry with the "metrics" signal
    initialize_telemetry(config, attrs, vec!["metrics"])
        .map_err(|e| format!("Telemetry initialization failed: {}", e))?;

    Ok(())
}

fn main() {
    let _ = setup_telemetry();

    if let Err(e) = cli::parse().execute() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
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
