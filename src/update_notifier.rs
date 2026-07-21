//! Background update notification system.
//!
//! Checks for new versions and notifies users without blocking CLI execution.
//! Results are cached to avoid repeated API calls.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::context::CommandContext;
use crate::paths::ana_home;
use crate::update::{fetch_latest_version, parse_version};

const CACHE_FILE: &str = "update-cache.json";
const DEFAULT_CHECK_INTERVAL_HOURS: u64 = 24;
const DEFAULT_NOTIFY_INTERVAL_HOURS: u64 = 24;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateCache {
    latest_version: String,
    current_version: String,
    checked_at: DateTime<Utc>,
    #[serde(default)]
    notified_at: Option<DateTime<Utc>>,
}

fn cache_path() -> PathBuf {
    ana_home().join(CACHE_FILE)
}

fn check_interval() -> Duration {
    let hours = std::env::var("ANA_UPDATE_CHECK_INTERVAL_HOURS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_CHECK_INTERVAL_HOURS);
    Duration::from_secs(hours * 3600)
}

fn notify_interval() -> Duration {
    let hours = std::env::var("ANA_UPDATE_NOTIFY_INTERVAL_HOURS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_NOTIFY_INTERVAL_HOURS);
    Duration::from_secs(hours * 3600)
}

pub fn update_check_enabled() -> bool {
    std::env::var("ANA_UPDATE_CHECK")
        .map(|v| {
            let v = v.trim().to_lowercase();
            !(v.is_empty() || v == "0" || v == "false")
        })
        .unwrap_or(true)
}

fn read_cache() -> Option<UpdateCache> {
    let path = cache_path();
    let contents = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn write_cache(cache: &UpdateCache) {
    let path = cache_path();

    if let Some(parent) = path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        tracing::debug!("Failed to create cache directory: {}", e);
        return;
    }

    let temp_path = path.with_extension("json.tmp");
    let contents = match serde_json::to_string_pretty(cache) {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!("Failed to serialize cache: {}", e);
            return;
        }
    };

    if let Err(e) = fs::write(&temp_path, contents) {
        tracing::debug!("Failed to write cache temp file: {}", e);
        return;
    }

    if let Err(e) = fs::rename(&temp_path, &path) {
        tracing::debug!("Failed to rename cache file: {}", e);
        let _ = fs::remove_file(&temp_path);
    }
}

fn is_cache_fresh(cache: &UpdateCache, current_version: &str) -> bool {
    if cache.current_version != current_version {
        return false;
    }

    let elapsed = Utc::now().signed_duration_since(cache.checked_at);
    let interval = check_interval();
    elapsed.num_seconds() < interval.as_secs() as i64
}

fn is_newer_version(latest: &str, current: &str) -> bool {
    let latest_v = match parse_version(latest) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let current_v = match parse_version(current) {
        Ok(v) => v,
        Err(_) => return false,
    };
    latest_v > current_v
}

fn should_notify(cache: &UpdateCache) -> bool {
    let Some(notified_at) = cache.notified_at else {
        return true;
    };

    let elapsed = Utc::now().signed_duration_since(notified_at);
    let interval = notify_interval();
    elapsed.num_seconds() >= interval.as_secs() as i64
}

fn mark_notified(cache: &UpdateCache) {
    let updated = UpdateCache {
        notified_at: Some(Utc::now()),
        ..cache.clone()
    };
    write_cache(&updated);
}

/// Check if an update is available and should be notified, using cache when possible.
/// Returns the latest version string if newer than current and notification is due, None otherwise.
pub async fn check_for_update(ctx: &CommandContext, current_version: &str) -> Option<String> {
    if let Some(cache) = read_cache()
        && is_cache_fresh(&cache, current_version)
    {
        if is_newer_version(&cache.latest_version, current_version) && should_notify(&cache) {
            return Some(cache.latest_version);
        }
        return None;
    }

    let latest = match fetch_latest_version(ctx).await {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!("Failed to fetch latest version: {}", e);
            return None;
        }
    };

    let cache = UpdateCache {
        latest_version: latest.clone(),
        current_version: current_version.to_string(),
        checked_at: Utc::now(),
        notified_at: None,
    };
    write_cache(&cache);

    if is_newer_version(&latest, current_version) {
        Some(latest)
    } else {
        None
    }
}

/// Display update notification to user and record notification time.
pub fn show_notification(current: &str, latest: &str) {
    use crate::ui::status;

    if let Some(cache) = read_cache() {
        mark_notified(&cache);
    }

    status::blank_line();
    status::warn(&format!(
        "A new version of ana is available: {} (you have v{})",
        status::highlight(latest),
        current
    ));
    status::tip(&format!(
        "Update with: {}",
        status::highlight("ana self update")
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer_version_newer() {
        assert!(is_newer_version("v0.0.2", "0.0.1"));
    }

    #[test]
    fn test_is_newer_version_same() {
        assert!(!is_newer_version("v0.0.1", "0.0.1"));
    }

    #[test]
    fn test_is_newer_version_older() {
        assert!(!is_newer_version("v0.0.1", "0.0.2"));
    }

    #[test]
    fn test_is_newer_version_invalid_latest() {
        assert!(!is_newer_version("invalid", "0.0.1"));
    }

    #[test]
    fn test_is_newer_version_invalid_current() {
        assert!(!is_newer_version("v0.0.1", "invalid"));
    }

    #[test]
    fn test_is_cache_fresh_same_version_recent() {
        let cache = UpdateCache {
            latest_version: "v0.0.2".to_string(),
            current_version: "0.0.1".to_string(),
            checked_at: Utc::now(),
            notified_at: None,
        };
        assert!(is_cache_fresh(&cache, "0.0.1"));
    }

    #[test]
    fn test_is_cache_fresh_different_version() {
        let cache = UpdateCache {
            latest_version: "v0.0.2".to_string(),
            current_version: "0.0.1".to_string(),
            checked_at: Utc::now(),
            notified_at: None,
        };
        assert!(!is_cache_fresh(&cache, "0.0.2"));
    }

    #[test]
    fn test_is_cache_fresh_old_cache() {
        let cache = UpdateCache {
            latest_version: "v0.0.2".to_string(),
            current_version: "0.0.1".to_string(),
            checked_at: Utc::now() - chrono::Duration::hours(25),
            notified_at: None,
        };
        assert!(!is_cache_fresh(&cache, "0.0.1"));
    }

    #[test]
    fn test_should_notify_never_notified() {
        let cache = UpdateCache {
            latest_version: "v0.0.2".to_string(),
            current_version: "0.0.1".to_string(),
            checked_at: Utc::now(),
            notified_at: None,
        };
        assert!(should_notify(&cache));
    }

    #[test]
    fn test_should_notify_recently_notified() {
        let cache = UpdateCache {
            latest_version: "v0.0.2".to_string(),
            current_version: "0.0.1".to_string(),
            checked_at: Utc::now(),
            notified_at: Some(Utc::now()),
        };
        assert!(!should_notify(&cache));
    }

    #[test]
    fn test_should_notify_old_notification() {
        let cache = UpdateCache {
            latest_version: "v0.0.2".to_string(),
            current_version: "0.0.1".to_string(),
            checked_at: Utc::now(),
            notified_at: Some(Utc::now() - chrono::Duration::hours(25)),
        };
        assert!(should_notify(&cache));
    }

    #[test]
    fn test_update_check_enabled_default() {
        temp_env::with_var_unset("ANA_UPDATE_CHECK", || {
            assert!(update_check_enabled());
        });
    }

    #[test]
    fn test_update_check_enabled_false() {
        temp_env::with_var("ANA_UPDATE_CHECK", Some("false"), || {
            assert!(!update_check_enabled());
        });
    }

    #[test]
    fn test_update_check_enabled_zero() {
        temp_env::with_var("ANA_UPDATE_CHECK", Some("0"), || {
            assert!(!update_check_enabled());
        });
    }

    #[test]
    fn test_update_check_enabled_true() {
        temp_env::with_var("ANA_UPDATE_CHECK", Some("true"), || {
            assert!(update_check_enabled());
        });
    }

    #[test]
    fn test_check_interval_default() {
        temp_env::with_var_unset("ANA_UPDATE_CHECK_INTERVAL_HOURS", || {
            assert_eq!(check_interval(), Duration::from_secs(24 * 3600));
        });
    }

    #[test]
    fn test_check_interval_custom() {
        temp_env::with_var("ANA_UPDATE_CHECK_INTERVAL_HOURS", Some("12"), || {
            assert_eq!(check_interval(), Duration::from_secs(12 * 3600));
        });
    }

    #[test]
    fn test_notify_interval_default() {
        temp_env::with_var_unset("ANA_UPDATE_NOTIFY_INTERVAL_HOURS", || {
            assert_eq!(notify_interval(), Duration::from_secs(24 * 3600));
        });
    }

    #[test]
    fn test_notify_interval_custom() {
        temp_env::with_var("ANA_UPDATE_NOTIFY_INTERVAL_HOURS", Some("12"), || {
            assert_eq!(notify_interval(), Duration::from_secs(12 * 3600));
        });
    }
}
