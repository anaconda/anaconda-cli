//! API key storage using a JSON file-based keyring.
//!
//! Compatible with anaconda-auth's keyring format.

use std::collections::HashMap;
use std::fs;
use std::io;

use base64::prelude::*;
use serde::{Deserialize, Serialize};

use super::errors::AuthError;
use crate::config::Config;

// TODO(mattkram): Decide whether now is the time to rename this key, or whether
//                 we even need it in JSON files case (probably)
const KEYRING_KEY: &str = "Anaconda Cloud";
const CREDENTIAL_VERSION: u32 = 2;

/// Top-level keyring structure.
type Keyring = HashMap<String, HashMap<String, String>>;

/// Credential stored in the keyring (before base64 encoding).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Credential {
    domain: String,
    api_key: String,
    repo_tokens: Vec<String>,
    version: u32,
}

/// Save an API key to the keyring file.
pub fn save_api_key(config: &Config, api_key: &str) -> Result<(), AuthError> {
    let path = &config.keyring_path;

    // Create parent directories if they don't exist
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| keyring_io_error("create directory", e))?;
    }

    // TODO(mattkram): The keyring access should be encapsulated in a struct

    // Load existing keyring or create new one
    let mut keyring = load_keyring(config)?;

    // Create credential and encode
    let credential = Credential {
        domain: config.domain.clone(),
        api_key: api_key.to_string(),
        repo_tokens: vec![],
        version: CREDENTIAL_VERSION,
    };
    let credential_json =
        serde_json::to_string(&credential).map_err(|e| AuthError::Keyring(e.to_string()))?;
    let encoded = BASE64_STANDARD.encode(credential_json);

    // Insert into keyring
    keyring
        .entry(KEYRING_KEY.to_string())
        .or_default()
        .insert(config.domain.clone(), encoded);

    // Write keyring
    let keyring_json =
        serde_json::to_string(&keyring).map_err(|e| AuthError::Keyring(e.to_string()))?;
    fs::write(path, keyring_json).map_err(|e| keyring_io_error("write", e))?;

    Ok(())
}

/// Get the API key from the keyring file.
pub fn get_api_key(config: &Config) -> Result<Option<String>, AuthError> {
    let keyring = load_keyring(config)?;

    let Some(domains) = keyring.get(KEYRING_KEY) else {
        return Ok(None);
    };

    let Some(encoded) = domains.get(&config.domain) else {
        return Ok(None);
    };

    let decoded = BASE64_STANDARD
        .decode(encoded)
        .map_err(|e| AuthError::Keyring(format!("Failed to decode credential: {}", e)))?;

    let credential: Credential = serde_json::from_slice(&decoded)
        .map_err(|e| AuthError::Keyring(format!("Failed to parse credential: {}", e)))?;

    Ok(Some(credential.api_key))
}

/// Delete the API key from the keyring file.
pub fn delete_api_key(config: &Config) -> Result<(), AuthError> {
    let path = &config.keyring_path;

    if !path.exists() {
        return Ok(());
    }

    let mut keyring = load_keyring(config)?;

    if let Some(domains) = keyring.get_mut(KEYRING_KEY) {
        domains.remove(&config.domain);

        // Remove the top-level key if no domains remain
        if domains.is_empty() {
            keyring.remove(KEYRING_KEY);
        }
    }

    if keyring.is_empty() {
        // Remove file if keyring is empty
        fs::remove_file(path).map_err(|e| keyring_io_error("delete", e))?;
    } else {
        // Write updated keyring
        let keyring_json =
            serde_json::to_string(&keyring).map_err(|e| AuthError::Keyring(e.to_string()))?;
        fs::write(path, keyring_json).map_err(|e| keyring_io_error("write", e))?;
    }

    Ok(())
}

/// Load the keyring from disk, or return an empty one if it doesn't exist.
fn load_keyring(config: &Config) -> Result<Keyring, AuthError> {
    let path = &config.keyring_path;

    if !path.exists() {
        return Ok(Keyring::new());
    }

    let contents = fs::read_to_string(path).map_err(|e| keyring_io_error("read", e))?;
    let keyring: Keyring =
        serde_json::from_str(&contents).map_err(|e| AuthError::Keyring(e.to_string()))?;

    Ok(keyring)
}

/// Helper to create an AuthError from an IO error.
fn keyring_io_error(operation: &str, error: io::Error) -> AuthError {
    AuthError::Keyring(format!("Failed to {} keyring: {}", operation, error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_config_with_keyring(path: PathBuf, domain: &str) -> Config {
        Config {
            domain: domain.to_string(),
            client_id: "test-client".to_string(),
            ssl_verify: true,
            open_browser: false,
            keyring_path: path,
            use_https: true,
            metrics_endpoint: "https://metrics.example.com".to_string(),
            metrics_export_interval_ms: 1000,
            metrics_console_exporter: false,
            metrics_skip_internet_check: true,
            include_prereleases: false,
        }
    }

    #[test]
    fn test_save_and_get_api_key() {
        let dir = tempfile::tempdir().unwrap();
        let keyring_path = dir.path().join("keyring");
        let config = test_config_with_keyring(keyring_path, "test.com");

        // Initially no key
        assert_eq!(get_api_key(&config).unwrap(), None);

        // Save a key
        save_api_key(&config, "test-api-key-12345").unwrap();

        // Retrieve the key
        assert_eq!(
            get_api_key(&config).unwrap(),
            Some("test-api-key-12345".to_string())
        );
    }

    #[test]
    fn test_save_creates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let keyring_path = dir.path().join("nested").join("dirs").join("keyring");
        let config = test_config_with_keyring(keyring_path.clone(), "test.com");

        save_api_key(&config, "test-key").unwrap();

        assert!(keyring_path.exists());
    }

    #[test]
    fn test_delete_api_key() {
        let dir = tempfile::tempdir().unwrap();
        let keyring_path = dir.path().join("keyring");
        let config = test_config_with_keyring(keyring_path.clone(), "test.com");

        // Save then delete
        save_api_key(&config, "test-key").unwrap();
        assert!(keyring_path.exists());

        delete_api_key(&config).unwrap();
        // File should be removed since keyring is empty
        assert!(!keyring_path.exists());
    }

    #[test]
    fn test_delete_nonexistent_keyring() {
        let dir = tempfile::tempdir().unwrap();
        let keyring_path = dir.path().join("nonexistent");
        let config = test_config_with_keyring(keyring_path, "test.com");

        // Should not error when deleting nonexistent file
        delete_api_key(&config).unwrap();
    }

    #[test]
    fn test_multiple_domains() {
        let dir = tempfile::tempdir().unwrap();
        let keyring_path = dir.path().join("keyring");

        let config1 = test_config_with_keyring(keyring_path.clone(), "domain1.com");
        let config2 = test_config_with_keyring(keyring_path.clone(), "domain2.com");

        // Save keys for both domains
        save_api_key(&config1, "key1").unwrap();
        save_api_key(&config2, "key2").unwrap();

        // Both should be retrievable
        assert_eq!(get_api_key(&config1).unwrap(), Some("key1".to_string()));
        assert_eq!(get_api_key(&config2).unwrap(), Some("key2".to_string()));

        // Delete one, other should remain
        delete_api_key(&config1).unwrap();
        assert_eq!(get_api_key(&config1).unwrap(), None);
        assert_eq!(get_api_key(&config2).unwrap(), Some("key2".to_string()));
        assert!(keyring_path.exists()); // File still exists

        // Delete the other
        delete_api_key(&config2).unwrap();
        assert!(!keyring_path.exists()); // File removed
    }

    #[test]
    fn test_get_api_key_returns_correct_domain() {
        let dir = tempfile::tempdir().unwrap();
        let keyring_path = dir.path().join("keyring");

        // Save keys for multiple domains
        let config_a = test_config_with_keyring(keyring_path.clone(), "a.com");
        let config_b = test_config_with_keyring(keyring_path.clone(), "b.com");
        let config_c = test_config_with_keyring(keyring_path.clone(), "c.com");

        save_api_key(&config_a, "key-for-a").unwrap();
        save_api_key(&config_b, "key-for-b").unwrap();
        save_api_key(&config_c, "key-for-c").unwrap();

        // Each config should only return its own key
        assert_eq!(
            get_api_key(&config_a).unwrap(),
            Some("key-for-a".to_string())
        );
        assert_eq!(
            get_api_key(&config_b).unwrap(),
            Some("key-for-b".to_string())
        );
        assert_eq!(
            get_api_key(&config_c).unwrap(),
            Some("key-for-c".to_string())
        );

        // A domain with no key should return None
        let config_unknown = test_config_with_keyring(keyring_path.clone(), "unknown.com");
        assert_eq!(get_api_key(&config_unknown).unwrap(), None);
    }

    #[test]
    fn test_keyring_json_format() {
        let dir = tempfile::tempdir().unwrap();
        let keyring_path = dir.path().join("keyring");
        let config = test_config_with_keyring(keyring_path.clone(), "example.com");

        save_api_key(&config, "my-api-key").unwrap();

        // Read raw file and verify structure
        let contents = fs::read_to_string(&keyring_path).unwrap();
        let keyring: serde_json::Value = serde_json::from_str(&contents).unwrap();

        // Should have "Anaconda Cloud" -> "example.com" -> base64
        let encoded = keyring["Anaconda Cloud"]["example.com"].as_str().unwrap();
        let decoded = BASE64_STANDARD.decode(encoded).unwrap();
        let credential: Credential = serde_json::from_slice(&decoded).unwrap();

        assert_eq!(credential.domain, "example.com");
        assert_eq!(credential.api_key, "my-api-key");
        assert_eq!(credential.version, 2);
        assert!(credential.repo_tokens.is_empty());
    }

    #[test]
    fn test_credential_serialization() {
        let credential = Credential {
            domain: "test.com".to_string(),
            api_key: "ak-123".to_string(),
            repo_tokens: vec![],
            version: 2,
        };

        let json = serde_json::to_string(&credential).unwrap();
        let parsed: Credential = serde_json::from_str(&json).unwrap();

        assert_eq!(credential, parsed);
    }
}
