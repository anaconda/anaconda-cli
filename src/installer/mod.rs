use std::collections::HashMap;
use std::path::Path;

use futures_util::StreamExt;
use miette::miette;
use reqwest_middleware::ClientWithMiddleware;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::context::CommandContext;
use crate::ui::progress::build_progress_bar;

const MINICONDA_BASE_URL: &str = "https://repo.anaconda.com/miniconda/";

struct Target {
    filename: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct FileEntry {
    sha256: Option<String>,
    size: Option<u64>,
    #[allow(dead_code)]
    md5: Option<String>,
    #[allow(dead_code)]
    mtime: Option<f64>,
}

/// Map a `(os, arch)` pair to the canonical Miniconda installer for that platform.
/// Parameterized on os/arch so it's pure and table-testable; production callers
/// pass `std::env::consts::{OS, ARCH}`.
fn detect_target(base_url: &str, os: &str, arch: &str) -> miette::Result<Target> {
    let (platform, file_arch, ext) = match (os, arch) {
        ("macos", "aarch64") => ("MacOSX", "arm64", "sh"),
        ("macos", "x86_64") => ("MacOSX", "x86_64", "sh"),
        ("linux", "x86_64") => ("Linux", "x86_64", "sh"),
        ("linux", "aarch64") => ("Linux", "aarch64", "sh"),
        ("windows", "x86_64") => ("Windows", "x86_64", "exe"),
        _ => {
            return Err(miette!(
                "no Miniconda installer available for {}/{}",
                os,
                arch
            ));
        }
    };

    let filename = format!("Miniconda3-latest-{}-{}.{}", platform, file_arch, ext);
    let url = format!("{}{}", base_url, filename);
    Ok(Target { filename, url })
}

async fn fetch_manifest(
    client: &ClientWithMiddleware,
    base_url: &str,
) -> miette::Result<HashMap<String, FileEntry>> {
    let manifest_url = format!("{}/.files.json", base_url.trim_end_matches('/'));

    let resp = client
        .get(&manifest_url)
        .send()
        .await
        .map_err(|e| miette!("failed to fetch manifest: {}", e))?
        .error_for_status()
        .map_err(|e| miette!("manifest request failed: {}", e))?;

    let manifest: HashMap<String, FileEntry> = resp
        .json()
        .await
        .map_err(|e| miette!("failed to parse manifest: {}", e))?;

    Ok(manifest)
}

fn expected_for<'a>(
    manifest: &'a HashMap<String, FileEntry>,
    filename: &str,
) -> miette::Result<&'a FileEntry> {
    let entry = manifest.get(filename).ok_or_else(|| {
        miette!(
            "filename '{}' not in manifest — platform map may be stale",
            filename
        )
    })?;

    match &entry.sha256 {
        None => Err(miette!(
            "no SHA256 checksum for '{}' — refusing unverified download",
            filename
        )),
        Some(s) if s.is_empty() => Err(miette!(
            "no SHA256 checksum for '{}' — refusing unverified download",
            filename
        )),
        _ => Ok(entry),
    }
}

async fn download_and_verify(
    client: &ClientWithMiddleware,
    url: &str,
    expected_sha: &str,
    dest: &Path,
) -> miette::Result<()> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| miette!("download failed: {}", e))?
        .error_for_status()
        .map_err(|e| miette!("download request failed: {}", e))?;

    let total_size = resp.content_length().unwrap_or(0);

    let temp_path = dest.with_extension("tmp");

    let pb = build_progress_bar(total_size);

    let mut file = tokio::fs::File::create(&temp_path)
        .await
        .map_err(|e| miette!("failed to create temp file: {}", e))?;

    let mut hasher = Sha256::new();
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| miette!("download error: {}", e))?;
        hasher.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|e| miette!("write error: {}", e))?;
        pb.inc(chunk.len() as u64);
    }

    pb.finish_and_clear();

    // Flush and close the file handle before the rename. On Windows, renaming a
    // file that still has an open handle fails with a sharing violation — and
    // this command targets Windows via the `.exe` path.
    file.flush()
        .await
        .map_err(|e| miette!("failed to flush temp file: {}", e))?;
    drop(file);

    let actual_sha = format!("{:x}", hasher.finalize());
    finalize_verified_download(&temp_path, &actual_sha, expected_sha, dest).await
}

/// Given a fully-written temp file and its computed checksum, verify it against
/// the expected checksum: atomic-rename to `dest` on match, delete the temp file
/// on mismatch. Comparison is case-insensitive (hex digests may be either case).
async fn finalize_verified_download(
    temp_path: &Path,
    actual_sha: &str,
    expected_sha: &str,
    dest: &Path,
) -> miette::Result<()> {
    if !actual_sha.eq_ignore_ascii_case(expected_sha) {
        let _ = tokio::fs::remove_file(temp_path).await;
        return Err(miette!(
            "checksum mismatch for '{}'\n  expected: {}\n  actual:   {}",
            dest.display(),
            expected_sha,
            actual_sha
        ));
    }

    tokio::fs::rename(temp_path, dest)
        .await
        .map_err(|e| miette!("failed to move file to destination: {}", e))?;

    Ok(())
}

fn format_size(bytes: u64) -> String {
    let mb = bytes as f64 / 1_000_000.0;
    format!("{:.1} MB", mb)
}

fn run_command(filename: &str) -> String {
    if filename.ends_with(".exe") {
        format!(r#"start "" ".\{filename}""#)
    } else {
        format!("bash ./{filename}")
    }
}

pub async fn run(ctx: &CommandContext, base_url: Option<&str>) -> miette::Result<()> {
    let base_url = base_url.unwrap_or(MINICONDA_BASE_URL);
    let target = detect_target(base_url, std::env::consts::OS, std::env::consts::ARCH)?;

    let dest = std::env::current_dir()
        .map_err(|e| miette!("failed to get current directory: {}", e))?
        .join(&target.filename);

    if dest.exists() {
        return Err(miette!(
            "./{} already exists. Remove it if you want to continue.",
            target.filename
        ));
    }

    let client = ctx.download_client();

    let manifest = fetch_manifest(client, base_url).await?;
    let entry = expected_for(&manifest, &target.filename)?;

    let expected_sha = entry.sha256.as_deref().unwrap();
    let size_label = entry.size.map(format_size).unwrap_or_default();

    let size_part = if size_label.is_empty() {
        String::new()
    } else {
        format!(" ({})", size_label)
    };
    eprintln!("Downloading {}{}", target.filename, size_part);

    download_and_verify(client, &target.url, expected_sha, &dest).await?;

    let dest_display = if cfg!(windows) {
        format!(".\\{}", target.filename)
    } else {
        format!("./{}", target.filename)
    };

    println!("Downloaded {}{} to:", target.filename, size_part);
    println!("    {}", dest_display);
    println!("SHA256 verified.");
    println!();
    println!("To install, run:");
    println!("    {}", run_command(&target.filename));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_target_supported_combos() {
        let cases = [
            ("macos", "aarch64", "Miniconda3-latest-MacOSX-arm64.sh"),
            ("macos", "x86_64", "Miniconda3-latest-MacOSX-x86_64.sh"),
            ("linux", "x86_64", "Miniconda3-latest-Linux-x86_64.sh"),
            ("linux", "aarch64", "Miniconda3-latest-Linux-aarch64.sh"),
            ("windows", "x86_64", "Miniconda3-latest-Windows-x86_64.exe"),
        ];

        for (os, arch, expected_filename) in cases {
            let result = detect_target("https://example.com/miniconda/", os, arch);
            assert!(result.is_ok(), "expected Ok for {}/{}", os, arch);
            assert_eq!(result.unwrap().filename, expected_filename);
        }
    }

    #[test]
    fn test_detect_target_url() {
        let target = detect_target("https://example.com/miniconda/", "linux", "x86_64").unwrap();
        assert_eq!(
            target.url,
            "https://example.com/miniconda/Miniconda3-latest-Linux-x86_64.sh"
        );
    }

    #[test]
    fn test_detect_target_unsupported_combo() {
        let result = detect_target("https://example.com/", "linux", "mips");
        assert!(result.is_err());
    }

    #[test]
    fn test_expected_for_key_missing() {
        let manifest = HashMap::new();
        let result = expected_for(&manifest, "Miniconda3-latest-Linux-x86_64.sh");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not in manifest"));
    }

    #[test]
    fn test_expected_for_empty_sha256() {
        let mut manifest = HashMap::new();
        manifest.insert(
            "Miniconda3-latest-Linux-x86_64.sh".to_string(),
            FileEntry {
                sha256: Some(String::new()),
                size: None,
                md5: None,
                mtime: None,
            },
        );
        let result = expected_for(&manifest, "Miniconda3-latest-Linux-x86_64.sh");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("refusing unverified")
        );
    }

    #[test]
    fn test_expected_for_none_sha256() {
        let mut manifest = HashMap::new();
        manifest.insert(
            "Miniconda3-latest-Linux-x86_64.sh".to_string(),
            FileEntry {
                sha256: None,
                size: None,
                md5: None,
                mtime: None,
            },
        );
        let result = expected_for(&manifest, "Miniconda3-latest-Linux-x86_64.sh");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("refusing unverified")
        );
    }

    #[test]
    fn test_expected_for_valid_entry() {
        let mut manifest = HashMap::new();
        manifest.insert(
            "Miniconda3-latest-Linux-x86_64.sh".to_string(),
            FileEntry {
                sha256: Some("abc123".to_string()),
                size: Some(1234),
                md5: None,
                mtime: None,
            },
        );
        let result = expected_for(&manifest, "Miniconda3-latest-Linux-x86_64.sh");
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_command_sh() {
        assert_eq!(
            run_command("Miniconda3-latest-MacOSX-arm64.sh"),
            "bash ./Miniconda3-latest-MacOSX-arm64.sh"
        );
    }

    #[test]
    fn test_run_command_exe() {
        assert_eq!(
            run_command("Miniconda3-latest-Windows-x86_64.exe"),
            r#"start "" ".\Miniconda3-latest-Windows-x86_64.exe""#
        );
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(127_400_000), "127.4 MB");
        assert_eq!(format_size(163_179_296), "163.2 MB");
    }

    #[test]
    fn test_manifest_deserialization() {
        let json = r#"{
            "Miniconda3-latest-Linux-x86_64.sh": {
                "md5": "5eb314581f476f57526204386ea87af8",
                "mtime": 1777399036.7642996,
                "sha256": "2284bafb7863a23411b19874d216e237964d4b32dd9beb6807fa8b2d84570961",
                "size": 163179296
            },
            "Miniconda3-latest-MacOSX-arm64.sh": {
                "md5": "deadbeef",
                "mtime": 1777399036.0,
                "sha256": "cafebabe1234",
                "size": 127400000
            }
        }"#;

        let manifest: HashMap<String, FileEntry> = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.len(), 2);

        let entry = manifest.get("Miniconda3-latest-Linux-x86_64.sh").unwrap();
        assert_eq!(
            entry.sha256.as_deref(),
            Some("2284bafb7863a23411b19874d216e237964d4b32dd9beb6807fa8b2d84570961")
        );
        assert_eq!(entry.size, Some(163179296));
    }

    #[tokio::test]
    async fn test_finalize_deletes_temp_on_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("test.sh");
        let temp = dest.with_extension("tmp");

        tokio::fs::write(&temp, b"fake content").await.unwrap();
        let actual_sha = format!("{:x}", Sha256::digest(b"fake content"));
        let expected_sha = "0".repeat(64);

        let result = finalize_verified_download(&temp, &actual_sha, &expected_sha, &dest).await;

        assert!(result.is_err(), "mismatch should return an error");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("checksum mismatch"),
            "error should mention checksum mismatch"
        );
        assert!(!temp.exists(), "temp file should be deleted on mismatch");
        assert!(!dest.exists(), "dest file should not exist on mismatch");
    }

    #[tokio::test]
    async fn test_finalize_renames_on_match() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("test.sh");
        let temp = dest.with_extension("tmp");

        tokio::fs::write(&temp, b"fake content").await.unwrap();
        let actual_sha = format!("{:x}", Sha256::digest(b"fake content"));

        // expected provided in UPPERCASE to exercise case-insensitive comparison
        let expected_sha = actual_sha.to_uppercase();

        let result = finalize_verified_download(&temp, &actual_sha, &expected_sha, &dest).await;

        assert!(result.is_ok(), "matching checksum should succeed");
        assert!(!temp.exists(), "temp file should be gone after rename");
        assert!(dest.exists(), "dest file should exist after rename");
        let contents = tokio::fs::read(&dest).await.unwrap();
        assert_eq!(contents, b"fake content");
    }
}
