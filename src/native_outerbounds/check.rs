use miette::Result;
use outerbounds::{CheckOptions, CheckStatus};

use crate::context::CommandContext;
use crate::ui::status;

pub async fn check(
    ctx: &CommandContext,
    no_config: bool,
    output: Option<&str>,
    workstation: bool,
    latency: bool,
    latency_requests: u32,
    latency_timeout: f64,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let opts = CheckOptions {
        no_config,
        workstation,
        python: false, // Python checks are handled differently in the Python CLI
        latency,
        latency_requests,
        latency_timeout,
    };

    let response = ob.check().check(opts).await?;

    // Handle JSON output format
    if output == Some("json") {
        let json = serde_json::to_string_pretty(&response)
            .map_err(|e| miette::miette!("Failed to serialize response: {}", e))?;
        println!("{}", json);
        return Ok(());
    }

    for result in &response.data.steps {
        let icon = match result.status {
            CheckStatus::Ok => "✓",
            CheckStatus::Fail => "✗",
            CheckStatus::Warn => "⚠",
        };

        println!("{} {}: {}", icon, result.name, result.message);

        if !result.help.is_empty() {
            tracing::debug!("Help: {}", result.help);
        }
    }

    // Print latency results if available
    if !response.latency.is_empty() {
        println!("\nLatency results:");
        for lat in &response.latency {
            print!("  {}: ", lat.endpoint);
            if let (Some(min), Some(max), Some(avg)) = (lat.min_ms, lat.max_ms, lat.avg_ms) {
                println!(
                    "min={:.1}ms, max={:.1}ms, avg={:.1}ms ({}/{} succeeded)",
                    min,
                    max,
                    avg,
                    lat.success_count,
                    lat.success_count + lat.error_count
                );
            } else {
                println!("no successful requests");
            }
            for err in &lat.errors {
                tracing::debug!("Error: {}", err);
            }
        }
    }

    match response.status {
        CheckStatus::Ok => {
            status::blank_line();
            status::success("All checks passed!");
        }
        CheckStatus::Warn => {
            status::blank_line();
            status::warn("Some checks have warnings");
        }
        CheckStatus::Fail => {
            status::blank_line();
            status::warn("Some checks failed");
        }
    }

    Ok(())
}
