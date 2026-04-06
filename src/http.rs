//! HTTP client utilities with logging middleware.

use reqwest::{Request, Response};
use reqwest_middleware::{Middleware, Next, Result};

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
    /// Create a new client with base URL and logging middleware.
    pub fn new(
        builder: reqwest::ClientBuilder,
        base_url: impl Into<String>,
    ) -> std::result::Result<Self, reqwest::Error> {
        let client = builder.build()?;
        let inner = reqwest_middleware::ClientBuilder::new(client)
            .with(LoggingMiddleware)
            .build();
        Ok(Self {
            inner,
            base_url: base_url.into(),
        })
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

/// Build an HTTP client with logging middleware (no base URL).
pub fn build_client(
    builder: reqwest::ClientBuilder,
) -> std::result::Result<reqwest_middleware::ClientWithMiddleware, reqwest::Error> {
    let client = builder.build()?;
    Ok(reqwest_middleware::ClientBuilder::new(client)
        .with(LoggingMiddleware)
        .build())
}
