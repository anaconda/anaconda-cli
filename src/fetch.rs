use miette::{Context, IntoDiagnostic, miette};

use crate::auth;
use crate::context::CommandContext;

pub async fn api_fetch(
    ctx: &CommandContext,
    method: &str,
    url: &str,
    query_args: Option<&str>,
    data: Option<&str>,
    json: Option<&str>,
) -> miette::Result<()> {
    if !is_valid_url(url) {
        return Err(miette!(
            "Invalid URL: '{}'. URL must start with 'http://', 'https://', or '/' for relative API paths.",
            url
        ));
    }

    auth::ensure_logged_in(ctx).await.into_diagnostic()?;

    let method_upper = method.to_uppercase();
    let mut request = match method_upper.as_str() {
        "GET" => ctx.client().get(url),
        "POST" => ctx.client().post(url),
        "PUT" => ctx.client().put(url),
        "PATCH" => ctx.client().patch(url),
        "DELETE" => ctx.client().delete(url),
        _ => return Err(miette!("Unsupported HTTP method: {}", method)),
    };
    request = request.header("X-Ana-Raw-Request", "true");
    if let Some(args) = query_args {
        let pairs: Vec<(&str, &str)> = args
            .split(',')
            .filter_map(|pair| pair.split_once('='))
            .collect();
        request = request.query(&pairs);
    }
    if let Some(body) = data {
        request = request.body(body.to_string());
    }
    if let Some(body) = json {
        let parsed: serde_json::Value = serde_json::from_str(body)
            .into_diagnostic()
            .context("Invalid JSON")?;
        request = request.json(&parsed);
    }
    let response = request.send().await.into_diagnostic()?;
    let status = response.status();
    let body = response.text().await.into_diagnostic()?;
    println!("{}", status);
    println!("{}", body);
    Ok(())
}

fn is_valid_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://") || url.starts_with('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_https_url() {
        assert!(is_valid_url("https://example.com/api"));
    }

    #[test]
    fn test_valid_http_url() {
        assert!(is_valid_url("http://example.com/api"));
    }

    #[test]
    fn test_valid_relative_path() {
        assert!(is_valid_url("/api/v1/packages"));
    }

    #[test]
    fn test_invalid_typo_httpp() {
        assert!(!is_valid_url("httpp://example.com"));
    }

    #[test]
    fn test_invalid_no_scheme() {
        assert!(!is_valid_url("example.com/api"));
    }

    #[test]
    fn test_invalid_ftp_scheme() {
        assert!(!is_valid_url("ftp://example.com"));
    }
}
