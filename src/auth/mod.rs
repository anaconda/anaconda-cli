//! Authentication module.

mod actions;
mod api_keys;
pub mod errors;
mod keyring;
pub mod responses;

pub use actions::{
    ensure_logged_in, has_premium_subscription, login, logout, show_api_key, whoami,
};
pub use keyring::{get_api_key, get_user_id};

#[cfg(test)]
pub(crate) use keyring::save_credential;
