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

/// Build an HTTP client with logging middleware.
pub fn build_client(
    builder: reqwest::ClientBuilder,
) -> std::result::Result<reqwest_middleware::ClientWithMiddleware, reqwest::Error> {
    let client = builder.build()?;
    Ok(reqwest_middleware::ClientBuilder::new(client)
        .with(LoggingMiddleware)
        .build())
}
