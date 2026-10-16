//! Immutable dependencies shared by the MCP tool handlers.

use std::time::Duration;

use crate::auth;
use crate::config::Config;
use crate::context::CommandContext;
use crate::http::Client;

/// Per-request timeout for backend calls. Matches the remote service.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// What the tools need to reach Anaconda's services. Nothing here changes
/// while the server runs.
///
/// rmcp requires the server handler to be `Clone + Send + Sync + 'static`, so
/// this holds owned, cheaply cloneable copies taken from `CommandContext`
/// (which is borrowed mutably and built lazily). Keeping it separate from the
/// rmcp handler also lets the tool functions be tested without the SDK.
#[derive(Clone)]
pub struct Backend {
    /// Authenticated client; reads the API key from the keyring per request,
    /// so `ana login`/`ana logout` take effect without restarting the server.
    pub client: Client,
    /// Unauthenticated client for public endpoints (forum search), so the API
    /// key is not sent where it is not needed.
    pub public_client: Client,
    /// Keyring location (for the login check) and `forum_base_url`.
    pub config: Config,
}

impl Backend {
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
