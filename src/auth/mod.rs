//! Authentication module.

mod actions;
mod api_keys;
mod errors;
mod keyring;
mod responses;

pub use actions::{ApiClient, login, logout, show_api_key, whoami};
pub use keyring::get_api_key;
