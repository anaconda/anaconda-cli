//! API key storage using a JSON file-based keyring.
//!
//! Compatible with anaconda-auth's keyring format.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

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

/// Repo token stored in the keyring.
/// Compatible with anaconda-auth's RepoToken model.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct RepoToken {
    token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    org_name: Option<String>,
}

/// Credential stored in the keyring (before base64 encoding).
/// Compatible with anaconda-auth's TokenInfo model.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Credential {
    domain: String,
    api_key: String,
    #[serde(default)]
    repo_tokens: Vec<RepoToken>,
    version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    username: Option<String>,
}

/// Save an API key and optional user info to the keyring file.
pub(crate) fn save_credential(
    config: &Config,
    api_key: &str,
    user_id: Option<&str>,
    username: Option<&str>,
) -> Result<(), AuthError> {
    let path = &config.keyring_path;

    // Create parent directories if they don't exist
    if let Some(parent) = path.parent() {
        create_secure_dir(parent)?;
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
        user_id: user_id.map(|s| s.to_string()),
        username: username.map(|s| s.to_string()),
    };
    let credential_json =
        serde_json::to_string(&credential).map_err(|e| AuthError::Keyring(e.to_string()))?;
    let encoded = BASE64_STANDARD.encode(credential_json);

    // Insert into keyring
    keyring
        .entry(KEYRING_KEY.to_string())
        .or_default()
        .insert(config.domain.clone(), encoded);

    // Write keyring with secure permissions
    let keyring_json =
        serde_json::to_string(&keyring).map_err(|e| AuthError::Keyring(e.to_string()))?;
    write_secure_file(path, &keyring_json)?;

    Ok(())
}

/// Get the API key from the keyring file.
pub fn get_api_key(config: &Config) -> Result<Option<String>, AuthError> {
    Ok(get_credential(config)?.map(|c| c.api_key))
}

/// Get the user_id from the keyring file.
pub fn get_user_id(config: &Config) -> Result<Option<String>, AuthError> {
    Ok(get_credential(config)?.and_then(|c| c.user_id))
}

/// Get the full credential from the keyring file.
fn get_credential(config: &Config) -> Result<Option<Credential>, AuthError> {
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

    Ok(Some(credential))
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
        // Write updated keyring with secure permissions
        let keyring_json =
            serde_json::to_string(&keyring).map_err(|e| AuthError::Keyring(e.to_string()))?;
        write_secure_file(path, &keyring_json)?;
    }

    Ok(())
}

/// Create a directory with secure permissions (0700 on Unix).
fn create_secure_dir(path: &Path) -> Result<(), AuthError> {
    if path.exists() {
        return Ok(());
    }

    fs::create_dir_all(path).map_err(|e| keyring_permission_error("create directory", path, e))?;

    // Set directory permissions to 0700 (owner read/write/execute only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o700);
        fs::set_permissions(path, perms)
            .map_err(|e| keyring_permission_error("set directory permissions", path, e))?;
    }

    Ok(())
}

/// Write a file with secure permissions (0600 on Unix).
fn write_secure_file(path: &Path, contents: &str) -> Result<(), AuthError> {
    // On Unix, create file with restricted permissions from the start
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| keyring_permission_error("write", path, e))?;
        file.write_all(contents.as_bytes())
            .map_err(|e| keyring_permission_error("write", path, e))?;
    }

    #[cfg(not(unix))]
    {
        fs::write(path, contents).map_err(|e| keyring_permission_error("write", path, e))?;
    }

    Ok(())
}

/// Load the keyring from disk, or return an empty one if it doesn't exist.
fn load_keyring(config: &Config) -> Result<Keyring, AuthError> {
    let path = &config.keyring_path;

    if !path.exists() {
        return Ok(Keyring::new());
    }

    let contents =
        fs::read_to_string(path).map_err(|e| keyring_permission_error("read", path, e))?;

    // Handle empty file as empty keyring
    if contents.trim().is_empty() {
        return Ok(Keyring::new());
    }

    let keyring: Keyring =
        serde_json::from_str(&contents).map_err(|e| AuthError::Keyring(e.to_string()))?;

    Ok(keyring)
}

/// Helper to create an AuthError from an IO error.
fn keyring_io_error(operation: &str, error: io::Error) -> AuthError {
    AuthError::Keyring(format!("Failed to {} keyring: {}", operation, error))
}

/// Helper to create an AuthError from a permission-related IO error with actionable guidance.
fn keyring_permission_error(operation: &str, path: &Path, error: io::Error) -> AuthError {
    let base_msg = format!("Failed to {} keyring: {}", operation, error);

    // Provide actionable guidance for permission denied errors
    #[cfg(unix)]
    if error.kind() == io::ErrorKind::PermissionDenied
        && let Some(parent) = path.parent()
    {
        return AuthError::Keyring(format!(
            "{}.\n\nTo fix this, run:\n  chmod 700 {} && chmod 600 {}",
            base_msg,
            parent.display(),
            path.display()
        ));
    }

    AuthError::Keyring(base_msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Test helper to save an API key without user_id or username.
    fn save_api_key(config: &Config, api_key: &str) -> Result<(), AuthError> {
        save_credential(config, api_key, None, None)
    }

    fn test_config_with_keyring(path: PathBuf, domain: &str) -> Config {
        Config {
            domain: domain.to_string(),
            client_id: "test-client".to_string(),
            ssl_verify: true,
            open_browser: false,
            keyring_path: path,
            use_https: true,
            metrics_endpoint: "https://metrics.example.com".to_string(),
            metrics_public_endpoint: "https://public.metrics.example.com".to_string(),
            metrics_export_interval_ms: 1000,
            metrics_console_exporter: false,
            metrics_skip_internet_check: true,
            include_prereleases: false,
            pip_index_url: "https://repo.anaconda.cloud/repo/anaconda-wheels/simple".to_string(),
            self_update_url: Some("https://example.com".to_string()),
            auto_update_tools: None,
            #[cfg(feature = "diagnostics")]
            sentry_disabled: false,
            #[cfg(feature = "diagnostics")]
            sentry_environment: "test".to_string(),
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
            user_id: Some("user-123".to_string()),
            username: None,
        };

        let json = serde_json::to_string(&credential).unwrap();
        let parsed: Credential = serde_json::from_str(&json).unwrap();

        assert_eq!(credential, parsed);
    }

    #[test]
    fn test_credential_without_user_id() {
        // Test backwards compatibility - old credentials without user_id should still parse
        let json = r#"{"domain":"test.com","api_key":"ak-123","repo_tokens":[],"version":2}"#;
        let credential: Credential = serde_json::from_str(json).unwrap();

        assert_eq!(credential.domain, "test.com");
        assert_eq!(credential.api_key, "ak-123");
        assert_eq!(credential.user_id, None);
    }

    #[test]
    fn test_credential_username_variations() {
        // Test various username values that might appear in anaconda-auth credentials

        // Explicit null should parse as None
        let json_null = r#"{"domain":"test.com","api_key":"ak-123","repo_tokens":[],"version":2,"username":null}"#;
        let cred: Credential = serde_json::from_str(json_null).unwrap();
        assert_eq!(cred.username, None);

        // Missing username should parse as None
        let json_missing =
            r#"{"domain":"test.com","api_key":"ak-123","repo_tokens":[],"version":2}"#;
        let cred: Credential = serde_json::from_str(json_missing).unwrap();
        assert_eq!(cred.username, None);

        // Normal username
        let json_normal = r#"{"domain":"test.com","api_key":"ak-123","repo_tokens":[],"version":2,"username":"alice"}"#;
        let cred: Credential = serde_json::from_str(json_normal).unwrap();
        assert_eq!(cred.username, Some("alice".to_string()));
    }

    #[test]
    fn test_parse_anaconda_auth_credential_format() {
        // Test compatibility with anaconda-auth's TokenInfo format.
        // anaconda-auth uses `username` while ana-cli uses `user_id`.
        // repo_tokens are objects with `token` and `org_name` fields.
        let json = r#"{
            "domain": "anaconda.com",
            "api_key": "ak-xyz",
            "username": "testuser",
            "repo_tokens": [
                {"token": "rt-token-1", "org_name": "myorg"},
                {"token": "rt-token-2", "org_name": null}
            ],
            "version": 2
        }"#;

        let credential: Credential = serde_json::from_str(json).unwrap();

        assert_eq!(credential.domain, "anaconda.com");
        assert_eq!(credential.api_key, "ak-xyz");
        assert_eq!(credential.user_id, None);
        assert_eq!(credential.username, Some("testuser".to_string()));
        assert_eq!(credential.repo_tokens.len(), 2);
        assert_eq!(credential.repo_tokens[0].token, "rt-token-1");
        assert_eq!(
            credential.repo_tokens[0].org_name,
            Some("myorg".to_string())
        );
        assert_eq!(credential.repo_tokens[1].token, "rt-token-2");
        assert_eq!(credential.repo_tokens[1].org_name, None);
    }

    #[test]
    fn test_get_user_id_ignores_username() {
        let dir = tempfile::tempdir().unwrap();
        let keyring_path = dir.path().join("keyring");
        let config = test_config_with_keyring(keyring_path.clone(), "test.com");

        // Simulate an anaconda-auth credential with only `username` set
        let credential = Credential {
            domain: "test.com".to_string(),
            api_key: "ak-123".to_string(),
            repo_tokens: vec![],
            version: 2,
            user_id: None,
            username: Some("testuser".to_string()),
        };
        let credential_json = serde_json::to_string(&credential).unwrap();
        let encoded = BASE64_STANDARD.encode(credential_json);

        let mut keyring: Keyring = HashMap::new();
        keyring
            .entry(KEYRING_KEY.to_string())
            .or_default()
            .insert("test.com".to_string(), encoded);

        let keyring_json = serde_json::to_string(&keyring).unwrap();
        fs::write(&keyring_path, keyring_json).unwrap();

        // get_user_id should return None when only username is set
        assert_eq!(get_user_id(&config).unwrap(), None);
    }

    #[test]
    fn test_get_user_id_returns_user_id() {
        let dir = tempfile::tempdir().unwrap();
        let keyring_path = dir.path().join("keyring");
        let config = test_config_with_keyring(keyring_path.clone(), "test.com");

        // Credential with user_id set
        let credential = Credential {
            domain: "test.com".to_string(),
            api_key: "ak-123".to_string(),
            repo_tokens: vec![],
            version: 2,
            user_id: Some("user-123".to_string()),
            username: Some("testuser".to_string()),
        };
        let credential_json = serde_json::to_string(&credential).unwrap();
        let encoded = BASE64_STANDARD.encode(credential_json);

        let mut keyring: Keyring = HashMap::new();
        keyring
            .entry(KEYRING_KEY.to_string())
            .or_default()
            .insert("test.com".to_string(), encoded);

        let keyring_json = serde_json::to_string(&keyring).unwrap();
        fs::write(&keyring_path, keyring_json).unwrap();

        // get_user_id should return user_id
        assert_eq!(get_user_id(&config).unwrap(), Some("user-123".to_string()));
    }

    #[test]
    fn test_repo_token_serialization() {
        let token = RepoToken {
            token: "rt-123".to_string(),
            org_name: Some("myorg".to_string()),
        };

        let json = serde_json::to_string(&token).unwrap();
        let parsed: RepoToken = serde_json::from_str(&json).unwrap();

        assert_eq!(token, parsed);
    }

    #[test]
    fn test_repo_token_without_org_name() {
        let json = r#"{"token": "rt-456"}"#;
        let token: RepoToken = serde_json::from_str(json).unwrap();

        assert_eq!(token.token, "rt-456");
        assert_eq!(token.org_name, None);
    }

    #[test]
    fn test_credential_none_user_id_not_serialized() {
        // Verify that None user_id/username are omitted from JSON (not serialized as null)
        let credential = Credential {
            domain: "test.com".to_string(),
            api_key: "ak-123".to_string(),
            repo_tokens: vec![],
            version: 2,
            user_id: None,
            username: None,
        };

        let json = serde_json::to_string(&credential).unwrap();
        assert!(
            !json.contains("user_id"),
            "user_id should not appear in JSON when None"
        );
        assert!(
            !json.contains("username"),
            "username should not appear in JSON when None"
        );
    }

    #[test]
    fn test_save_and_get_user_id() {
        let dir = tempfile::tempdir().unwrap();
        let keyring_path = dir.path().join("keyring");
        let config = test_config_with_keyring(keyring_path, "test.com");

        // Save credential with user_id
        save_credential(&config, "test-api-key", Some("user-456"), None).unwrap();

        // Retrieve user_id
        assert_eq!(get_user_id(&config).unwrap(), Some("user-456".to_string()));
    }

    #[test]
    fn test_get_user_id_returns_none_when_not_set() {
        let dir = tempfile::tempdir().unwrap();
        let keyring_path = dir.path().join("keyring");
        let config = test_config_with_keyring(keyring_path, "test.com");

        // Save credential without user_id or username
        save_credential(&config, "test-api-key", None, None).unwrap();

        // user_id should be None
        assert_eq!(get_user_id(&config).unwrap(), None);
    }

    #[cfg(unix)]
    #[test]
    fn test_keyring_file_has_secure_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let parent_dir = dir.path().join("secure_dir");
        let keyring_path = parent_dir.join("keyring");
        let config = test_config_with_keyring(keyring_path.clone(), "test.com");

        save_api_key(&config, "test-key").unwrap();

        // Check directory permissions are 0700 (owner read/write/execute only)
        let dir_perms = fs::metadata(&parent_dir).unwrap().permissions();
        assert_eq!(
            dir_perms.mode() & 0o777,
            0o700,
            "Directory should have 0700 permissions"
        );

        // Check file permissions are 0600 (owner read/write only)
        let file_perms = fs::metadata(&keyring_path).unwrap().permissions();
        assert_eq!(
            file_perms.mode() & 0o777,
            0o600,
            "Keyring file should have 0600 permissions"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_permission_error_includes_fix_instructions() {
        let path = Path::new("/some/path/keyring");
        let error = keyring_permission_error(
            "write",
            path,
            io::Error::new(io::ErrorKind::PermissionDenied, "Permission denied"),
        );

        let msg = error.to_string();
        assert!(
            msg.contains("Permission denied"),
            "Should include original error"
        );
        assert!(msg.contains("chmod 700"), "Should include directory fix");
        assert!(msg.contains("chmod 600"), "Should include file fix");
    }
}
