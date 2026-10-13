//! `ana lm list`: browse the model catalog or show downloaded models.

use std::path::{Path, PathBuf};

use serde_json::json;

use super::catalog::{self, CatalogFilter, CatalogModel};
use super::store::{self, file_name, format_size};
use crate::context::CommandContext;
use crate::table::{self, Color};
use crate::ui::status;

/// List catalog models matching `filter` (default) or downloaded models (`local`).
pub async fn list(
    ctx: &CommandContext,
    local: bool,
    filter: &CatalogFilter,
    json: bool,
) -> miette::Result<()> {
    let models_dir = crate::paths::models_dir();
    if local {
        let models = local_models(&models_dir)?;
        if json {
            print_json(&local_json(&models));
        } else {
            print_local_table(&models, &models_dir);
        }
        return Ok(());
    }

    let mut models = catalog::list_models(ctx, filter).await?;
    models.sort_by_key(|m| m.full_name().to_ascii_lowercase());
    if json {
        print_json(&catalog_json(&models, &models_dir));
    } else {
        print_catalog_table(&models, &models_dir);
    }
    Ok(())
}

fn print_json(value: &serde_json::Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).expect("JSON values always serialize")
    );
}

// ---------------------------------------------------------------------------
// Catalog
// ---------------------------------------------------------------------------

/// GGUF quantizations of `model` that are downloaded locally.
pub(super) fn downloaded_quants(models_dir: &Path, model: &CatalogModel) -> Vec<String> {
    let Some(dir) = store::find_model_dir(models_dir, &model.full_name()) else {
        return Vec::new();
    };
    let files = store::gguf_files(&dir).unwrap_or_default();
    model
        .gguf_files()
        .iter()
        .filter_map(|f| f.quant_method.clone())
        .filter(|q| {
            let mut matching = files.clone();
            store::filter_by_quant(&mut matching, q);
            !matching.is_empty()
        })
        .collect()
}

fn print_catalog_table(models: &[CatalogModel], models_dir: &Path) {
    if models.is_empty() {
        status::info("No catalog models match the given filters.");
        return;
    }

    let mut t = table::new([
        "Model",
        "Params",
        "Context",
        "Tools",
        "GGUF quants",
        "Downloaded",
    ]);
    for model in models {
        let quants: Vec<String> = model
            .gguf_files()
            .iter()
            .filter_map(|f| f.quant_method.clone())
            .collect();
        let quants_cell = if quants.is_empty() {
            table::cell("safetensors only").fg(Color::DarkGrey)
        } else {
            table::cell(quants.join(", "))
        };
        let downloaded = downloaded_quants(models_dir, model);
        let tools = match model.supports_tool_calling {
            Some(true) => table::cell("✓").fg(Color::Green),
            _ => table::cell(""),
        };

        t.add_row([
            table::cell(model.full_name()),
            table::cell(model.num_parameters.map(format_params).unwrap_or_default()),
            table::cell(
                model
                    .context_window_size
                    .filter(|&c| c > 0)
                    .map(format_context)
                    .unwrap_or_default(),
            ),
            tools,
            quants_cell,
            table::cell(downloaded.join(", ")).fg(Color::Green),
        ]);
    }

    println!("{t}");
    let runnable = models.iter().filter(|m| !m.gguf_files().is_empty()).count();
    eprintln!(
        "{} models, {} runnable with llama.cpp (GGUF)",
        models.len(),
        runnable
    );
    status::tip(&format!(
        "Serve a model with {}",
        status::highlight("ana lm run <model> [--quant <quant>]")
    ));
}

fn catalog_json(models: &[CatalogModel], models_dir: &Path) -> serde_json::Value {
    let items: Vec<_> = models
        .iter()
        .map(|m| {
            let downloaded = downloaded_quants(models_dir, m);
            let gguf: Vec<_> = m
                .gguf_files()
                .iter()
                .map(|f| {
                    json!({
                        "quant": f.quant_method,
                        "size_bytes": f.size_bytes,
                        "max_ram_usage": f.max_ram_usage,
                        "downloaded": f
                            .quant_method
                            .as_ref()
                            .is_some_and(|q| downloaded.contains(q)),
                    })
                })
                .collect();
            json!({
                "name": m.full_name(),
                "publisher": m.publisher(),
                "purpose": m.trained_for,
                "tags": m.tag_names(),
                "size": m.size_class(),
                "num_parameters": m.num_parameters,
                "context_window_size": m.context_window_size,
                "supports_tool_calling": m.supports_tool_calling,
                "gguf": gguf,
                "has_safetensors": m.has_non_gguf(),
            })
        })
        .collect();
    json!(items)
}

/// `27000000000` -> `27B`, `500000000` -> `500M`, `1600000000000` -> `1.6T`.
pub(super) fn format_params(n: u64) -> String {
    let (value, unit) = match n {
        n if n >= 1_000_000_000_000 => (n as f64 / 1e12, "T"),
        n if n >= 1_000_000_000 => (n as f64 / 1e9, "B"),
        n if n >= 1_000_000 => (n as f64 / 1e6, "M"),
        n => return n.to_string(),
    };
    let s = format!("{value:.1}");
    format!("{}{unit}", s.trim_end_matches(".0"))
}

/// `32768` -> `32K`, `128000` -> `128K`, `1048576` -> `1M`.
///
/// Context windows are published both as round decimal numbers (`128000`) and
/// as powers of two (`131072`), so prefer whichever divides evenly.
pub(super) fn format_context(n: u64) -> String {
    if n >= 1_000_000 && n.is_multiple_of(1_000_000) {
        format!("{}M", n / 1_000_000)
    } else if n >= 1 << 20 && n.is_multiple_of(1 << 20) {
        format!("{}M", n >> 20)
    } else if n >= 1000 && n.is_multiple_of(1000) {
        format!("{}K", n / 1000)
    } else if n >= 1024 && n.is_multiple_of(1024) {
        format!("{}K", n / 1024)
    } else if n >= 1000 {
        format!("{}K", (n + 500) / 1000)
    } else {
        n.to_string()
    }
}

// ---------------------------------------------------------------------------
// Local
// ---------------------------------------------------------------------------

/// A downloaded GGUF file.
#[derive(Debug, PartialEq)]
struct LocalModel {
    /// Model directory name under `~/.ana/models`.
    model: String,
    path: PathBuf,
    size_bytes: u64,
    /// ID used by the shared server and the Kilo provider.
    kilo_id: String,
}

/// All GGUF files under `models_dir`, sorted by model then filename.
fn local_models(models_dir: &Path) -> miette::Result<Vec<LocalModel>> {
    let Ok(entries) = std::fs::read_dir(models_dir) else {
        return Ok(Vec::new());
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();

    let mut models = Vec::new();
    for dir in dirs {
        for gguf in store::gguf_files(&dir)? {
            models.push(LocalModel {
                model: file_name(&dir),
                size_bytes: store::disk_size(&gguf),
                kilo_id: super::kilo::model_id(&gguf),
                path: gguf,
            });
        }
    }
    Ok(models)
}

fn print_local_table(models: &[LocalModel], models_dir: &Path) {
    if models.is_empty() {
        status::info(&format!("No models downloaded in {}", models_dir.display()));
        status::tip(&format!(
            "Browse the catalog with {}",
            status::highlight("ana lm list")
        ));
        return;
    }

    let mut t = table::new(["Model", "File", "Size", "Model ID"]);
    for m in models {
        t.add_row([
            table::cell(&m.model),
            table::cell(file_name(&m.path)),
            table::cell(format_size(m.size_bytes)),
            table::cell(&m.kilo_id),
        ]);
    }
    println!("{t}");
    let total: u64 = models.iter().map(|m| m.size_bytes).sum();
    eprintln!(
        "{} {}, {} in {}",
        models.len(),
        if models.len() == 1 { "file" } else { "files" },
        format_size(total),
        models_dir.display()
    );
}

fn local_json(models: &[LocalModel]) -> serde_json::Value {
    json!(
        models
            .iter()
            .map(|m| json!({
                "model": m.model,
                "file": file_name(&m.path),
                "path": m.path,
                "size_bytes": m.size_bytes,
                "model_id": m.kilo_id,
            }))
            .collect::<Vec<_>>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_params() {
        assert_eq!(format_params(27_000_000_000), "27B");
        assert_eq!(format_params(500_000_000), "500M");
        assert_eq!(format_params(1_600_000_000_000), "1.6T");
        assert_eq!(format_params(1_500_000_000), "1.5B");
        assert_eq!(format_params(999), "999");
    }

    #[test]
    fn test_format_context() {
        assert_eq!(format_context(32768), "32K");
        assert_eq!(format_context(262144), "256K");
        assert_eq!(format_context(1048576), "1M");
        assert_eq!(format_context(131072), "128K");
        assert_eq!(format_context(128000), "128K");
        assert_eq!(format_context(32000), "32K");
        assert_eq!(format_context(40960), "40K");
        assert_eq!(format_context(1_000_000), "1M");
        assert_eq!(format_context(33_000_500), "33001K");
        assert_eq!(format_context(512), "512");
    }

    #[test]
    fn test_local_models() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("B-Model/B-Model-q4_k_m.gguf");
        let b = tmp.path().join("A-Model/Org_A-Model-q8_0.gguf");
        for p in [&a, &b] {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, vec![0u8; 10]).unwrap();
        }
        std::fs::write(tmp.path().join("A-Model/notes.txt"), "x").unwrap();

        let models = local_models(tmp.path()).unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].model, "A-Model");
        assert_eq!(models[0].kilo_id, "org-a-model-q8-0");
        assert_eq!(models[0].size_bytes, 10);
        assert_eq!(models[1].model, "B-Model");
    }

    #[test]
    fn test_local_models_missing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            local_models(&tmp.path().join("missing"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn test_downloaded_quants() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("Qwen2.5-0.5B-Instruct");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Qwen_Qwen2.5-0.5B-Instruct-q4_k_m.gguf"), "x").unwrap();

        let model: CatalogModel = serde_json::from_value(json!({
            "name": "Qwen2.5-0.5B-Instruct",
            "source": { "name": "Qwen" },
            "quantized_files": [
                { "format": "gguf", "quant_method": "q4_k_m", "size_bytes": 1 },
                { "format": "gguf", "quant_method": "q8_0", "size_bytes": 2 },
            ],
        }))
        .unwrap();
        assert_eq!(downloaded_quants(tmp.path(), &model), vec!["q4_k_m"]);
    }
}
