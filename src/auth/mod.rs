//! Authentication module.

mod actions;
mod api_keys;
mod errors;
mod keyring;

pub use actions::{login, logout, show_api_key, whoami};
pub use keyring::get_api_key;
