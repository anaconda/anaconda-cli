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
    let url = normalize_url(url);
    if !is_valid_url(&url) {
        return Err(miette!(
            "Invalid URL: '{}'. URL must start with 'http://', 'https://', or '/' for relative API paths.",
            url
        ));
    }

    auth::ensure_logged_in(ctx).await?;

    let method_upper = method.to_uppercase();
    let mut request = match method_upper.as_str() {
        "GET" => ctx.client().get(&url),
        "POST" => ctx.client().post(&url),
        "PUT" => ctx.client().put(&url),
        "PATCH" => ctx.client().patch(&url),
        "DELETE" => ctx.client().delete(&url),
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
    eprintln!("{}", status);
    println!("{}", body);
    Ok(())
}

/// Normalize a URL argument, handling MSYS2/Git Bash path conversion on Windows.
///
/// Git Bash (MSYS2) automatically converts Unix-style paths starting with `/` to
/// Windows paths. For example, `/api/auth/passport` becomes
/// `C:/Program Files/Git/api/auth/passport`. This function detects that pattern
/// and recovers the original relative API path.
///
/// It also accepts bare relative paths (e.g., `api/auth/passport`) and prepends `/`.
fn normalize_url(url: &str) -> String {
    // Empty URL — return as-is so validation rejects it with a clear message
    if url.is_empty() {
        return url.to_string();
    }

    // Already a full URL — pass through unchanged
    if url.starts_with("http://") || url.starts_with("https://") {
        return url.to_string();
    }

    // Already a proper relative path
    if url.starts_with('/') {
        return url.to_string();
    }

    // Detect MSYS2-converted Windows paths.
    // Pattern: single drive letter followed by :/ or :\ (e.g., C:/Program Files/Git/api/...)
    if url.len() >= 3 {
        let bytes = url.as_bytes();
        if bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'/' || bytes[2] == b'\\')
        {
            // Look for known MSYS2 installation path prefixes and strip them.
            // Common patterns: "C:/Program Files/Git/", "C:/msys64/", etc.
            // We search for the first path component that looks like an API path
            // by finding "/api/", "/v1/", etc. after the drive prefix.
            let normalized = url.replace('\\', "/");

            // Strip the Windows drive + MSYS2 prefix by finding the API path.
            // MSYS2 prepends the Git install dir, so we look for the first segment
            // that could be an API path. The original argument started with `/`,
            // so the real path is everything from the first `/` after the MSYS2 prefix
            // that isn't part of the install directory structure.
            //
            // Strategy: find known MSYS2 base dirs and strip them.
            let msys_prefixes = [
                "/Program Files/Git/",
                "/Program Files (x86)/Git/",
                "/msys64/",
                "/msys32/",
                "/Git/",
            ];

            let after_drive = &normalized[2..]; // skip "C:" portion
            for prefix in &msys_prefixes {
                if let Some(rest) = after_drive.strip_prefix(&**prefix).or_else(|| {
                    // Case-insensitive match for the prefix
                    let lower = after_drive.to_lowercase();
                    let lower_prefix = prefix.to_lowercase();
                    if lower.starts_with(&lower_prefix) {
                        Some(&after_drive[prefix.len()..])
                    } else {
                        None
                    }
                }) {
                    return format!("/{}", rest);
                }
            }

            // If we didn't match a known MSYS2 prefix but it's clearly a Windows path,
            // it's not a valid URL — return as-is and let validation reject it.
            return url.to_string();
        }
    }

    // Bare relative path (e.g., "api/auth/passport") — prepend /
    format!("/{}", url)
}

fn is_valid_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://") || url.starts_with('/')
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use wiremock::matchers::{body_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::config::Config;
    use crate::http::Client;

    fn test_config(keyring_path: PathBuf, domain: &str) -> Config {
        Config {
            domain: domain.to_string(),
            client_id: "test-client".to_string(),
            ssl_verify: true,
            open_browser: false,
            keyring_path,
            use_https: true,
            metrics_endpoint: "https://metrics.example.com".to_string(),
            metrics_public_endpoint: "https://public.metrics.example.com".to_string(),
            metrics_export_interval_ms: 1000,
            metrics_console_exporter: false,
            metrics_skip_internet_check: true,
            include_prereleases: false,
            pip_index_url: "https://example.com/simple".to_string(),
            self_update_url: Some("https://example.com".to_string()),
            auto_update_tools: None,
            #[cfg(feature = "diagnostics")]
            sentry_disabled: false,
            #[cfg(feature = "diagnostics")]
            sentry_environment: "test".to_string(),
        }
    }

    async fn setup_test_context(mock_server: &MockServer) -> (CommandContext, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let keyring_path = dir.path().join("keyring");
        let config = test_config(keyring_path, "test.example.com");

        auth::save_credential(&config, "test-api-key", None, None).unwrap();

        let client = Client::new(reqwest::Client::builder(), mock_server.uri()).unwrap();
        let ctx = CommandContext::with_client(config, client);

        (ctx, dir)
    }

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

    #[tokio::test]
    async fn test_api_fetch_empty_url_rejected() {
        let mock_server = MockServer::start().await;
        let (ctx, _dir) = setup_test_context(&mock_server).await;

        let result = api_fetch(&ctx, "GET", "", None, None, None).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid URL"));
    }

    #[tokio::test]
    async fn test_api_fetch_invalid_method_rejected() {
        let mock_server = MockServer::start().await;
        let (ctx, _dir) = setup_test_context(&mock_server).await;

        let result = api_fetch(&ctx, "INVALID", "/test", None, None, None).await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Unsupported HTTP method")
        );
    }

    #[tokio::test]
    async fn test_api_fetch_get_request() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/test"))
            .and(header("X-Ana-Raw-Request", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_string("success"))
            .expect(1)
            .mount(&mock_server)
            .await;

        let (ctx, _dir) = setup_test_context(&mock_server).await;

        let result = api_fetch(&ctx, "GET", "/api/test", None, None, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_api_fetch_post_with_json_body() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/create"))
            .and(header("X-Ana-Raw-Request", "true"))
            .and(body_json(serde_json::json!({"name": "test"})))
            .respond_with(ResponseTemplate::new(201).set_body_string(r#"{"id": 1}"#))
            .expect(1)
            .mount(&mock_server)
            .await;

        let (ctx, _dir) = setup_test_context(&mock_server).await;

        let result = api_fetch(
            &ctx,
            "POST",
            "/api/create",
            None,
            None,
            Some(r#"{"name": "test"}"#),
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_api_fetch_invalid_json_rejected() {
        let mock_server = MockServer::start().await;
        let (ctx, _dir) = setup_test_context(&mock_server).await;

        let result = api_fetch(&ctx, "POST", "/api/test", None, None, Some("not json")).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid JSON"));
    }

    #[tokio::test]
    async fn test_api_fetch_with_query_params() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/search"))
            .and(query_param("foo", "bar"))
            .and(query_param("baz", "qux"))
            .respond_with(ResponseTemplate::new(200).set_body_string("found"))
            .expect(1)
            .mount(&mock_server)
            .await;

        let (ctx, _dir) = setup_test_context(&mock_server).await;

        let result = api_fetch(
            &ctx,
            "GET",
            "/api/search",
            Some("foo=bar,baz=qux"),
            None,
            None,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_api_fetch_put_request() {
        let mock_server = MockServer::start().await;

        Mock::given(method("PUT"))
            .and(path("/api/update"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        let (ctx, _dir) = setup_test_context(&mock_server).await;

        let result = api_fetch(&ctx, "PUT", "/api/update", None, None, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_api_fetch_delete_request() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/api/remove"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&mock_server)
            .await;

        let (ctx, _dir) = setup_test_context(&mock_server).await;

        let result = api_fetch(&ctx, "DELETE", "/api/remove", None, None, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_api_fetch_patch_request() {
        let mock_server = MockServer::start().await;

        Mock::given(method("PATCH"))
            .and(path("/api/patch"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        let (ctx, _dir) = setup_test_context(&mock_server).await;

        let result = api_fetch(&ctx, "patch", "/api/patch", None, None, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_api_fetch_method_case_insensitive() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/test"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        let (ctx, _dir) = setup_test_context(&mock_server).await;

        let result = api_fetch(&ctx, "get", "/api/test", None, None, None).await;
        assert!(result.is_ok());
    }

    // --- normalize_url tests ---

    #[test]
    fn test_normalize_url_passthrough_https() {
        assert_eq!(
            normalize_url("https://example.com/api"),
            "https://example.com/api"
        );
    }

    #[test]
    fn test_normalize_url_passthrough_http() {
        assert_eq!(
            normalize_url("http://example.com/api"),
            "http://example.com/api"
        );
    }

    #[test]
    fn test_normalize_url_passthrough_relative() {
        assert_eq!(normalize_url("/api/auth/passport"), "/api/auth/passport");
    }

    #[test]
    fn test_normalize_url_msys2_program_files() {
        assert_eq!(
            normalize_url("C:/Program Files/Git/api/auth/passport"),
            "/api/auth/passport"
        );
    }

    #[test]
    fn test_normalize_url_msys2_program_files_x86() {
        assert_eq!(
            normalize_url("C:/Program Files (x86)/Git/api/auth/passport"),
            "/api/auth/passport"
        );
    }

    #[test]
    fn test_normalize_url_msys2_msys64() {
        assert_eq!(
            normalize_url("C:/msys64/api/v1/packages"),
            "/api/v1/packages"
        );
    }

    #[test]
    fn test_normalize_url_msys2_msys32() {
        assert_eq!(normalize_url("D:/msys32/api/test"), "/api/test");
    }

    #[test]
    fn test_normalize_url_msys2_git_dir() {
        assert_eq!(
            normalize_url("C:/Git/api/auth/passport"),
            "/api/auth/passport"
        );
    }

    #[test]
    fn test_normalize_url_msys2_backslashes() {
        assert_eq!(
            normalize_url("C:\\Program Files\\Git\\api\\auth\\passport"),
            "/api/auth/passport"
        );
    }

    #[test]
    fn test_normalize_url_bare_path() {
        assert_eq!(normalize_url("api/auth/passport"), "/api/auth/passport");
    }

    #[test]
    fn test_normalize_url_bare_path_v1() {
        assert_eq!(normalize_url("v1/packages"), "/v1/packages");
    }

    #[test]
    fn test_normalize_url_unknown_windows_path_rejected() {
        // A Windows path without a known MSYS2 prefix is returned as-is
        // (will fail is_valid_url validation)
        assert_eq!(
            normalize_url("C:/Users/someone/random"),
            "C:/Users/someone/random"
        );
    }

    #[test]
    fn test_normalize_url_empty_string() {
        // Empty string is returned as-is so validation rejects it
        assert_eq!(normalize_url(""), "");
    }

    #[tokio::test]
    async fn test_api_fetch_msys2_converted_path() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/auth/passport"))
            .and(header("X-Ana-Raw-Request", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_string("passport_data"))
            .expect(1)
            .mount(&mock_server)
            .await;

        let (ctx, _dir) = setup_test_context(&mock_server).await;

        // Simulate what Git Bash does: converts /api/auth/passport to C:/Program Files/Git/api/auth/passport
        let result = api_fetch(
            &ctx,
            "GET",
            "C:/Program Files/Git/api/auth/passport",
            None,
            None,
            None,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_api_fetch_bare_path() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/auth/passport"))
            .and(header("X-Ana-Raw-Request", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_string("passport_data"))
            .expect(1)
            .mount(&mock_server)
            .await;

        let (ctx, _dir) = setup_test_context(&mock_server).await;

        let result = api_fetch(&ctx, "GET", "api/auth/passport", None, None, None).await;
        assert!(result.is_ok());
    }
}
