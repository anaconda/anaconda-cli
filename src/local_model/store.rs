//! Helpers for locating models stored under `~/.ana/models`.

use std::path::{Component, Path, PathBuf};

use miette::miette;

/// Find the directory a model is stored under in `models_dir`, if downloaded.
///
/// `model` may be the catalog name (`Qwen/Qwen2.5-0.5B-Instruct`) or the local
/// directory name (`Qwen2.5-0.5B-Instruct`). Only direct children of
/// `models_dir` are considered, so names like `..` can never escape it.
pub fn find_model_dir(models_dir: &Path, model: &str) -> Option<PathBuf> {
    model_dir_candidates(model)
        .into_iter()
        .map(|name| models_dir.join(name))
        .find(|dir| dir.is_dir())
}

/// Directory names a model may be stored under.
///
/// outerbounds stores models under the catalog name with `/` replaced by `_`;
/// catalog lookups by `<org>/<name>` may also resolve to a model named `<name>`.
/// Candidates that aren't a single plain path component (e.g. `..`) are dropped.
fn model_dir_candidates(model: &str) -> Vec<String> {
    let mut candidates = vec![model.replace(['/', '\\', ' '], "_")];
    if let Some(short) = model.rsplit('/').next()
        && short != model
    {
        candidates.push(short.to_string());
    }
    candidates.retain(|c| is_single_normal_component(c));
    candidates
}

fn is_single_normal_component(name: &str) -> bool {
    let mut components = Path::new(name).components();
    matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(_)), None)
    )
}

/// List the GGUF files directly inside `dir`, sorted by name.
pub fn gguf_files(dir: &Path) -> miette::Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| miette!("Failed to read {}: {}", dir.display(), e))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf"))
        })
        .collect();
    files.sort();
    Ok(files)
}

/// Keep only the files whose name contains `quant` (case-insensitive).
pub fn filter_by_quant(files: &mut Vec<PathBuf>, quant: &str) {
    let quant = quant.to_ascii_lowercase();
    files.retain(|p| file_name(p).to_ascii_lowercase().contains(&quant));
}

/// The final component of `path` as a string.
pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Total size in bytes of a file, or of all files under a directory.
pub fn disk_size(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if !meta.is_dir() {
        return meta.len();
    }
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| disk_size(&e.path()))
                .sum()
        })
        .unwrap_or(0)
}

/// Format a byte count for display (e.g. `379.4 MB`, `4.2 GB`).
pub fn format_size(bytes: u64) -> String {
    const MB: f64 = 1_000_000.0;
    const GB: f64 = 1_000_000_000.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else {
        format!("{:.1} MB", b / MB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_candidates_catalog_name() {
        assert_eq!(
            model_dir_candidates("Qwen/Qwen2.5-0.5B-Instruct"),
            vec!["Qwen_Qwen2.5-0.5B-Instruct", "Qwen2.5-0.5B-Instruct"]
        );
    }

    #[test]
    fn test_candidates_local_name() {
        assert_eq!(model_dir_candidates("Qwen2.5"), vec!["Qwen2.5"]);
    }

    #[test]
    fn test_candidates_reject_traversal() {
        assert!(model_dir_candidates("..").is_empty());
        assert!(model_dir_candidates(".").is_empty());
        assert!(model_dir_candidates("").is_empty());
        assert_eq!(model_dir_candidates("foo/.."), vec!["foo_.."]);
    }

    #[test]
    fn test_find_model_dir_never_escapes() {
        let tmp = tempfile::tempdir().unwrap();
        let models = tmp.path().join("models");
        std::fs::create_dir_all(&models).unwrap();
        assert_eq!(find_model_dir(&models, ".."), None);
        assert_eq!(find_model_dir(&models, "models/.."), None);
    }

    #[test]
    fn test_disk_size() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a"), vec![0u8; 10]).unwrap();
        std::fs::create_dir(tmp.path().join("sub")).unwrap();
        std::fs::write(tmp.path().join("sub/b"), vec![0u8; 5]).unwrap();
        assert_eq!(disk_size(tmp.path()), 15);
        assert_eq!(disk_size(&tmp.path().join("a")), 10);
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(379_400_000), "379.4 MB");
        assert_eq!(format_size(4_200_000_000), "4.2 GB");
    }
}
