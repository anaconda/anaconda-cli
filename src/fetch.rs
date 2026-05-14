use std::io::{self, Read as _};

use miette::{Context, IntoDiagnostic, miette};

use crate::auth;
use crate::context::CommandContext;

/// Read JSON input, supporting:
/// - Direct JSON string: `{"foo":"bar"}`
/// - Read from file: `@filename.json`
/// - Read from stdin: `-` or `@-`
fn read_json_input(input: &str) -> miette::Result<String> {
    let input = input.trim();
    if input == "-" || input == "@-" {
        let mut buffer = String::new();
        io::stdin()
            .read_to_string(&mut buffer)
            .into_diagnostic()
            .context("Failed to read JSON from stdin")?;
        Ok(buffer)
    } else if let Some(path) = input.strip_prefix('@') {
        std::fs::read_to_string(path)
            .into_diagnostic()
            .context(format!("Failed to read JSON from file: {}", path))
    } else {
        Ok(input.to_string())
    }
}

pub async fn api_fetch(
    ctx: &CommandContext,
    method: &str,
    url: &str,
    query_args: Option<&str>,
    data: Option<&str>,
    json: Option<&str>,
) -> miette::Result<()> {
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
        let json_content = read_json_input(body)?;
        let parsed: serde_json::Value = serde_json::from_str(&json_content)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_read_json_input_direct_string() {
        let result = read_json_input(r#"{"foo":"bar"}"#).unwrap();
        assert_eq!(result, r#"{"foo":"bar"}"#);
    }

    #[test]
    fn test_read_json_input_trims_whitespace() {
        let result = read_json_input("  {\"foo\":\"bar\"}  ").unwrap();
        assert_eq!(result, r#"{"foo":"bar"}"#);
    }

    #[test]
    fn test_read_json_input_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.json");
        let mut file = std::fs::File::create(&file_path).unwrap();
        writeln!(file, r#"{{"test": "value"}}"#).unwrap();

        let input = format!("@{}", file_path.display());
        let result = read_json_input(&input).unwrap();
        assert!(result.contains(r#""test": "value""#));
    }

    #[test]
    fn test_read_json_input_file_not_found() {
        let result = read_json_input("@nonexistent_file.json");
        assert!(result.is_err());
    }
}
