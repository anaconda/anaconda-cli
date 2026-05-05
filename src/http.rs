//! HTTP client utilities with logging middleware.

use std::env;

use reqwest::{Request, Response};
use reqwest_middleware::{Middleware, Next, Result};

use crate::auth;
use crate::config::Config;

/// Middleware that logs HTTP requests and responses.
pub struct LoggingMiddleware;

#[async_trait::async_trait]
impl Middleware for LoggingMiddleware {
    async fn handle(
        &self,
        req: Request,
        extensions: &mut http::Extensions,
        next: Next<'_>,
    ) -> Result<Response> {
        let method = req.method().clone();
        let url = req.url().clone();

        tracing::debug!("--> {} {}", method, url);

        let result = next.run(req, extensions).await;

        match &result {
            Ok(response) => {
                tracing::debug!("<-- {} {} {}", method, url, response.status());
            }
            Err(e) => {
                tracing::error!("<-- {} {} failed: {}", method, url, e);
            }
        }

        result
    }
}

/// HTTP client with base URL and logging middleware.
pub struct Client {
    inner: reqwest_middleware::ClientWithMiddleware,
    base_url: String,
}

impl Client {
    /// Create a new client with base URL, user-agent, and logging middleware.
    pub fn new(
        builder: reqwest::ClientBuilder,
        base_url: impl Into<String>,
    ) -> std::result::Result<Self, reqwest::Error> {
        let client = builder.user_agent(crate::ua::user_agent()).build()?;
        let inner = reqwest_middleware::ClientBuilder::new(client)
            .with(LoggingMiddleware)
            .build();
        Ok(Self {
            inner,
            base_url: base_url.into(),
        })
    }

    /// Create a client from config with optional auth headers.
    pub fn from_config() -> std::result::Result<Self, reqwest::Error> {
        let config = Config::load();
        let mut builder = reqwest::Client::builder();
        if let Ok(Some(api_key)) = auth::get_api_key(&config) {
            builder = builder.default_headers(bearer_header(&api_key));
        }
        Self::new(builder, config.base_url())
    }

    /// Resolve a URL - prepends base_url for relative paths, passes through full URLs.
    fn resolve_url(&self, url: &str) -> String {
        if url.starts_with("http://") || url.starts_with("https://") {
            url.to_string()
        } else {
            format!("{}{}", self.base_url, url)
        }
    }

    /// GET request.
    pub fn get(&self, url: &str) -> reqwest_middleware::RequestBuilder {
        self.inner.get(self.resolve_url(url))
    }

    /// POST request.
    #[allow(dead_code)]
    pub fn post(&self, url: &str) -> reqwest_middleware::RequestBuilder {
        self.inner.post(self.resolve_url(url))
    }

    /// PUT request.
    #[allow(dead_code)]
    pub fn put(&self, url: &str) -> reqwest_middleware::RequestBuilder {
        self.inner.put(self.resolve_url(url))
    }

    /// PATCH request.
    #[allow(dead_code)]
    pub fn patch(&self, url: &str) -> reqwest_middleware::RequestBuilder {
        self.inner.patch(self.resolve_url(url))
    }

    /// DELETE request.
    #[allow(dead_code)]
    pub fn delete(&self, url: &str) -> reqwest_middleware::RequestBuilder {
        self.inner.delete(self.resolve_url(url))
    }
}

/// Create a `HeaderMap` with a sensitive `Authorization: Bearer` header.
pub fn bearer_header(token: &str) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    let mut value = reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token)).unwrap();
    value.set_sensitive(true);
    headers.insert(reqwest::header::AUTHORIZATION, value);
    headers
}

/// Get Cloudflare Zero Trust headers if environment variables are set.
/// Returns None if either CF_ACCESS_CLIENT_ID or CF_ACCESS_CLIENT_SECRET is missing.
pub fn cloudflare_headers() -> Option<reqwest::header::HeaderMap> {
    let client_id = env::var("CF_ACCESS_CLIENT_ID").ok()?;
    let client_secret = env::var("CF_ACCESS_CLIENT_SECRET").ok()?;

    if client_id.is_empty() || client_secret.is_empty() {
        return None;
    }

    let mut headers = reqwest::header::HeaderMap::new();

    if let Ok(id_value) = reqwest::header::HeaderValue::from_str(&client_id) {
        headers.insert("CF-Access-Client-Id", id_value);
    }

    if let Ok(mut secret_value) = reqwest::header::HeaderValue::from_str(&client_secret) {
        secret_value.set_sensitive(true);
        headers.insert("CF-Access-Client-Secret", secret_value);
    }

    Some(headers)
}

/// Build an HTTP client with user-agent and logging middleware (no base URL).
/// If CF_ACCESS_CLIENT_ID and CF_ACCESS_CLIENT_SECRET environment variables are set,
/// Cloudflare Zero Trust headers are automatically included.
pub fn build_client(
    builder: reqwest::ClientBuilder,
) -> std::result::Result<reqwest_middleware::ClientWithMiddleware, reqwest::Error> {
    let builder = if let Some(cf_headers) = cloudflare_headers() {
        builder.default_headers(cf_headers)
    } else {
        builder
    };
    let client = builder.user_agent(crate::ua::user_agent()).build()?;
    Ok(reqwest_middleware::ClientBuilder::new(client)
        .with(LoggingMiddleware)
        .build())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_agent_contains_aau_tokens() {
        let ua = crate::ua::user_agent();

        // Verify AAU version marker is present
        assert!(
            ua.contains("aau/"),
            "User-Agent should contain aau/ version marker: {}",
            ua
        );

        // Verify client token (c/) is present - always generated
        assert!(
            ua.contains(" c/"),
            "User-Agent should contain client token (c/): {}",
            ua
        );

        // Verify session token (s/) is present - always generated per-process
        assert!(
            ua.contains(" s/"),
            "User-Agent should contain session token (s/): {}",
            ua
        );
    }

    #[test]
    fn test_user_agent_has_ana_prefix() {
        let ua = crate::ua::user_agent();
        assert!(
            ua.starts_with("ana/"),
            "User-Agent should start with ana/: {}",
            ua
        );
    }

    #[test]
    fn test_user_agent_has_rattler_version() {
        let ua = crate::ua::user_agent();
        assert!(
            ua.contains("rattler/"),
            "User-Agent should contain rattler/: {}",
            ua
        );
    }

    #[test]
    fn test_client_uses_user_agent() {
        // Build a client and verify it was constructed with our user-agent
        let client = Client::new(reqwest::Client::builder(), "https://example.com").unwrap();

        // The client was built successfully with our user-agent
        // We can verify the user-agent string is valid
        let ua = crate::ua::user_agent();
        assert!(!ua.is_empty());
        assert!(ua.contains("aau/"));

        // Verify we can create a request (doesn't send it)
        let _request = client.get("/test");
    }

    #[test]
    fn test_build_client_uses_user_agent() {
        // Build a standalone client and verify it was constructed
        let client = build_client(reqwest::Client::builder()).unwrap();

        // Verify the user-agent contains AAU tokens
        let ua = crate::ua::user_agent();
        assert!(ua.contains("aau/"), "User-Agent missing aau/: {}", ua);
        assert!(
            ua.contains(" c/"),
            "User-Agent missing client token: {}",
            ua
        );
        assert!(
            ua.contains(" s/"),
            "User-Agent missing session token: {}",
            ua
        );

        // Verify we can create a request
        let _request = client.get("https://example.com/test");
    }

    #[test]
    fn test_user_agent_token_format() {
        let ua = crate::ua::user_agent();

        // Find all tokens after aau/ and validate their format
        let parts: Vec<&str> = ua.split_whitespace().collect();
        let aau_idx = parts.iter().position(|p| p.starts_with("aau/"));

        assert!(aau_idx.is_some(), "No aau/ marker found in: {}", ua);

        // Each token after aau/ should be in format: single_char/base64url_value
        for part in &parts[aau_idx.unwrap() + 1..] {
            assert!(part.contains('/'), "Token should contain '/': {}", part);

            let (prefix, value) = part.split_once('/').unwrap();
            assert_eq!(
                prefix.len(),
                1,
                "Token prefix should be single char: {}",
                part
            );
            assert!(
                !value.is_empty(),
                "Token value should not be empty: {}",
                part
            );
            // Base64url characters only
            assert!(
                value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "Token value should be base64url: {}",
                part
            );
        }
    }
}
