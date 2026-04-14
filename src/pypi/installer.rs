//! Wheel installation into a conda prefix.
//!
//! Installs Python wheel files (.whl) into a prefix's site-packages
//! directory, following PEP 427 conventions. Wheels are cached locally
//! by SHA256 to avoid redundant downloads.
//!
//! See `pypi/mod.rs` for planned caching and installation improvements.

use std::io::Read;
use std::path::{Path, PathBuf};

/// Local wheel cache, storing wheels keyed by their SHA256 hash.
pub struct WheelCache {
    dir: PathBuf,
}

impl WheelCache {
    /// Create a new wheel cache. Creates the directory if it doesn't exist.
    pub fn new() -> Result<Self, String> {
        let dir = dirs::cache_dir()
            .ok_or_else(|| "Could not determine cache directory".to_string())?
            .join("ana")
            .join("wheels");
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create wheel cache dir: {}", e))?;
        Ok(Self { dir })
    }

    /// Look up a cached wheel by its expected SHA256 hash.
    /// Returns the wheel bytes if found and hash matches.
    pub fn get(&self, expected_sha256: &str) -> Option<Vec<u8>> {
        let path = self.dir.join(format!("{}.whl", expected_sha256));
        let bytes = std::fs::read(&path).ok()?;
        // Validate hash — cache could be corrupted
        if sha256_hex(&bytes) == expected_sha256 {
            Some(bytes)
        } else {
            // Remove corrupted entry
            let _ = std::fs::remove_file(&path);
            None
        }
    }

    /// Store a wheel in the cache, keyed by its SHA256 hash.
    pub fn put(&self, sha256: &str, bytes: &[u8]) {
        let path = self.dir.join(format!("{}.whl", sha256));
        let _ = std::fs::write(&path, bytes);
    }
}

/// Install a wheel from bytes into the given prefix.
///
/// The prefix should be a conda environment root (e.g. `.ana/envs/default`).
/// Wheels are installed into `{prefix}/lib/pythonX.Y/site-packages/`.
pub fn install_wheel(
    prefix: &Path,
    wheel_bytes: &[u8],
    python_version: (u32, u32),
) -> Result<(), String> {
    let site_packages = find_site_packages(prefix, python_version)?;

    let reader = std::io::Cursor::new(wheel_bytes);
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| format!("Failed to open wheel: {}", e))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read wheel entry: {}", e))?;

        let outpath = site_packages.join(file.name());

        if file.is_dir() {
            std::fs::create_dir_all(&outpath)
                .map_err(|e| format!("Failed to create directory {}: {}", outpath.display(), e))?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    format!("Failed to create directory {}: {}", parent.display(), e)
                })?;
            }

            let mut content = Vec::new();
            file.read_to_end(&mut content)
                .map_err(|e| format!("Failed to read {}: {}", file.name(), e))?;

            std::fs::write(&outpath, &content)
                .map_err(|e| format!("Failed to write {}: {}", outpath.display(), e))?;

            // Preserve executable permission on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = file.unix_mode() {
                    std::fs::set_permissions(&outpath, std::fs::Permissions::from_mode(mode)).ok();
                }
            }
        }
    }

    Ok(())
}

/// Download a wheel from a URL, verify its hash, and return the bytes.
/// If a cache is provided, stores the wheel after download.
pub async fn download_wheel(
    client: &reqwest::Client,
    url: &str,
    expected_sha256: Option<&str>,
    cache: Option<&WheelCache>,
) -> Result<Vec<u8>, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to download wheel: {}", e))?
        .error_for_status()
        .map_err(|e| format!("Wheel download failed: {}", e))?;

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read wheel bytes: {}", e))?;

    // Verify hash if provided
    if let Some(expected) = expected_sha256 {
        let actual = sha256_hex(&bytes);
        if actual != expected {
            return Err(format!(
                "SHA256 mismatch: expected {}, got {}",
                expected, actual
            ));
        }
    }

    let bytes = bytes.to_vec();

    // Cache the wheel for future use
    if let (Some(cache), Some(hash)) = (cache, expected_sha256) {
        cache.put(hash, &bytes);
    }

    Ok(bytes)
}

/// Download a wheel from a URL and install it.
pub async fn download_and_install(
    client: &reqwest::Client,
    url: &str,
    prefix: &Path,
    python_version: (u32, u32),
    expected_sha256: Option<&str>,
) -> Result<(), String> {
    let bytes = download_wheel(client, url, expected_sha256, None).await?;
    install_wheel(prefix, &bytes, python_version)
}

/// Find the site-packages directory for a given prefix and Python version.
fn find_site_packages(prefix: &Path, python_version: (u32, u32)) -> Result<PathBuf, String> {
    let (major, minor) = python_version;

    // Try Unix-style path first
    let unix_path = prefix.join(format!("lib/python{}.{}/site-packages", major, minor));
    if unix_path.is_dir() {
        return Ok(unix_path);
    }

    // Try Windows-style path
    let win_path = prefix.join("Lib").join("site-packages");
    if win_path.is_dir() {
        return Ok(win_path);
    }

    // Create Unix-style if neither exists
    std::fs::create_dir_all(&unix_path)
        .map_err(|e| format!("Failed to create site-packages: {}", e))?;
    Ok(unix_path)
}

/// Compute the SHA256 hex digest of some bytes.
fn sha256_hex(data: &[u8]) -> String {
    use std::fmt::Write;
    // Use the raw SHA-256 from the ring-like interface that rattler_digest provides,
    // or fall back to a manual implementation.
    // For now, use a simple implementation.
    let digest = <sha2::Sha256 as sha2::Digest>::digest(data);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        write!(hex, "{:02x}", byte).unwrap();
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_site_packages_creates_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let site = find_site_packages(tmp.path(), (3, 12)).unwrap();
        assert!(site.ends_with("lib/python3.12/site-packages"));
        assert!(site.is_dir());
    }

    #[test]
    fn test_find_site_packages_existing_unix() {
        let tmp = tempfile::tempdir().unwrap();
        let expected = tmp.path().join("lib/python3.11/site-packages");
        std::fs::create_dir_all(&expected).unwrap();
        let site = find_site_packages(tmp.path(), (3, 11)).unwrap();
        assert_eq!(site, expected);
    }

    #[test]
    fn test_sha256_hex() {
        let hash = sha256_hex(b"hello");
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn test_sha256_hex_empty() {
        let hash = sha256_hex(b"");
        assert_eq!(hash.len(), 64, "SHA256 hex is always 64 chars");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_sha256_hex_deterministic() {
        let h1 = sha256_hex(b"test data");
        let h2 = sha256_hex(b"test data");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_find_site_packages_different_versions() {
        let tmp = tempfile::tempdir().unwrap();
        let site = find_site_packages(tmp.path(), (3, 8)).unwrap();
        assert!(site.ends_with("lib/python3.8/site-packages"));
    }

    #[test]
    fn test_install_wheel_minimal() {
        // Create a minimal valid ZIP (wheel) with one file
        let tmp = tempfile::tempdir().unwrap();
        let prefix = tmp.path();

        let buf = Vec::new();
        let cursor = std::io::Cursor::new(buf);
        let mut zip = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("test_pkg-1.0.0.dist-info/METADATA", options)
            .unwrap();
        std::io::Write::write_all(&mut zip, b"Name: test_pkg\nVersion: 1.0.0\n").unwrap();
        let cursor = zip.finish().unwrap();
        let wheel_bytes = cursor.into_inner();

        let result = install_wheel(prefix, &wheel_bytes, (3, 12));
        assert!(result.is_ok(), "install_wheel failed: {:?}", result);

        let metadata_path =
            prefix.join("lib/python3.12/site-packages/test_pkg-1.0.0.dist-info/METADATA");
        assert!(metadata_path.exists());
    }

    #[test]
    fn test_install_wheel_invalid_zip() {
        let tmp = tempfile::tempdir().unwrap();
        let result = install_wheel(tmp.path(), b"not a zip file", (3, 12));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to open wheel"));
    }

    #[test]
    fn test_wheel_cache_put_and_get() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = WheelCache {
            dir: tmp.path().to_path_buf(),
        };
        let data = b"fake wheel data";
        let hash = sha256_hex(data);

        cache.put(&hash, data);
        let cached = cache.get(&hash);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap(), data);
    }

    #[test]
    fn test_wheel_cache_miss() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = WheelCache {
            dir: tmp.path().to_path_buf(),
        };
        assert!(cache.get("nonexistent_hash").is_none());
    }

    #[test]
    fn test_wheel_cache_corrupted() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = WheelCache {
            dir: tmp.path().to_path_buf(),
        };
        let hash = sha256_hex(b"original data");
        // Write corrupted data under the correct hash filename
        std::fs::write(tmp.path().join(format!("{}.whl", hash)), b"corrupted").unwrap();
        assert!(cache.get(&hash).is_none());
        // Corrupted file should be removed
        assert!(!tmp.path().join(format!("{}.whl", hash)).exists());
    }
}
