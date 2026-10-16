//! Shared, cloneable state for the MCP tool handlers.

use std::time::Duration;

use crate::auth;
use crate::config::Config;
use crate::context::CommandContext;
use crate::http::Client;

/// Per-request timeout for backend calls. Matches the remote service.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// State owned by the MCP server for the lifetime of `ana mcp serve`.
#[derive(Clone)]
pub struct ServerState {
    /// Authenticated client; reads the API key from the keyring per request,
    /// so `ana login`/`ana logout` take effect without restarting the server.
    pub client: Client,
    /// Unauthenticated client for public endpoints (forum search).
    pub public_client: Client,
    pub config: Config,
}

impl ServerState {
    pub fn from_context(ctx: &CommandContext) -> Self {
        Self {
            client: ctx.client().clone(),
            public_client: ctx.unauthenticated_client(REQUEST_TIMEOUT).clone(),
            config: ctx.config.clone(),
        }
    }

    /// True when an API key is stored for the configured domain.
    pub fn has_credentials(&self) -> bool {
        matches!(auth::get_api_key(&self.config), Ok(Some(_)))
    }
}
