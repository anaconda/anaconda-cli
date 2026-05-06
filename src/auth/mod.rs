//! Authentication module.

mod actions;
mod api_keys;
pub mod errors;
mod keyring;
mod responses;

pub use actions::{ensure_logged_in, login, logout, show_api_key, whoami};
pub use keyring::get_api_key;
