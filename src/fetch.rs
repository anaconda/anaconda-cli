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
    if url.is_empty() {
        return Err(miette!("URL cannot be empty"));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn validate_method(method: &str) -> miette::Result<()> {
        let method_upper = method.to_uppercase();
        match method_upper.as_str() {
            "GET" | "POST" | "PUT" | "PATCH" | "DELETE" => Ok(()),
            _ => Err(miette!("Unsupported HTTP method: {}", method)),
        }
    }

    fn parse_json(json: &str) -> miette::Result<serde_json::Value> {
        serde_json::from_str(json)
            .into_diagnostic()
            .context("Invalid JSON")
    }

    #[test]
    fn test_validate_method_valid() {
        assert!(validate_method("GET").is_ok());
        assert!(validate_method("POST").is_ok());
        assert!(validate_method("PUT").is_ok());
        assert!(validate_method("PATCH").is_ok());
        assert!(validate_method("DELETE").is_ok());
    }

    #[test]
    fn test_validate_method_case_insensitive() {
        assert!(validate_method("get").is_ok());
        assert!(validate_method("Post").is_ok());
        assert!(validate_method("pUt").is_ok());
    }

    #[test]
    fn test_validate_method_invalid() {
        let result = validate_method("INVALID");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Unsupported HTTP method: INVALID"));
    }

    #[test]
    fn test_validate_method_head_unsupported() {
        let result = validate_method("HEAD");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_json_valid() {
        let result = parse_json(r#"{"key": "value"}"#);
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["key"], "value");
    }

    #[test]
    fn test_parse_json_valid_array() {
        let result = parse_json(r#"[1, 2, 3]"#);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_json_invalid() {
        let result = parse_json("not valid json");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid JSON"));
    }

    #[test]
    fn test_parse_json_empty() {
        let result = parse_json("");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_json_unclosed_brace() {
        let result = parse_json(r#"{"key": "value""#);
        assert!(result.is_err());
    }
}
