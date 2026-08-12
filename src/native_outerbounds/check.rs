use miette::Result;
use outerbounds::{CheckOptions, CheckStatus};

use crate::context::CommandContext;
use crate::ui::status;

pub async fn check(
    ctx: &CommandContext,
    workstation: bool,
    python: bool,
    latency: bool,
) -> Result<()> {
    let ob = ctx.outerbounds_client().await?;

    let opts = CheckOptions {
        no_config: false,
        workstation,
        python,
        latency,
        latency_requests: 10,
        latency_timeout: 10.0,
    };

    let response = ob.check().check(opts).await?;

    for result in &response.data.steps {
        let icon = match result.status {
            CheckStatus::Ok => "✓",
            CheckStatus::Fail => "✗",
            CheckStatus::Warn => "⚠",
        };

        println!("{} {}: {}", icon, result.name, result.message);

        if !result.help.is_empty() {
            println!("    Help: {}", result.help);
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
                println!("    Error: {}", err);
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
