//! Interactive model picker for `ana lm run` without a model argument.

// Only used by `ana lm run`, which requires tool installation.
#![cfg_attr(not(tool_install), allow(dead_code))]

use std::path::Path;

use super::catalog::{self, CatalogFilter, CatalogModel};
use super::list::{downloaded_quants, format_context, format_params};
use super::store::format_size;
use crate::context::CommandContext;
use crate::input::SearchItem;

/// Let the user search the catalog and pick a model offering `quant`.
///
/// Returns the model's full name (e.g. `Qwen/Qwen2.5-0.5B-Instruct`), or
/// `None` if the user cancelled.
pub async fn pick_model(ctx: &CommandContext, quant: &str) -> miette::Result<Option<String>> {
    let spinner = indicatif::ProgressBar::new_spinner();
    spinner.set_message("Loading model catalog...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));
    let result = catalog::list_models(ctx, &CatalogFilter::default()).await;
    spinner.finish_and_clear();

    let mut models: Vec<CatalogModel> = result?
        .into_iter()
        .filter(|m| offers_quant(m, quant))
        .collect();
    if models.is_empty() {
        return Err(miette::miette!(
            "No catalog models offer the GGUF quantization '{}'",
            quant
        ));
    }
    models.sort_by_key(|m| m.full_name().to_ascii_lowercase());

    let items = build_items(&models, &crate::paths::models_dir(), quant);
    let prompt = format!("Select a model to run ({quant})");
    let selected = crate::input::search_select(&prompt, &items)
        .map_err(|e| miette::miette!("Model selection failed: {e}"))?;

    Ok(selected.map(|i| items[i].key.clone()))
}

/// True when `model` has a GGUF file with quantization `quant`.
fn offers_quant(model: &CatalogModel, quant: &str) -> bool {
    model.gguf_files().iter().any(|f| {
        f.quant_method
            .as_deref()
            .is_some_and(|q| q.eq_ignore_ascii_case(quant))
    })
}

/// One row per model: name, size, context, the quant's download size, and
/// whether it's already downloaded.
fn build_items(models: &[CatalogModel], models_dir: &Path, quant: &str) -> Vec<SearchItem> {
    let names: Vec<String> = models.iter().map(CatalogModel::full_name).collect();
    let name_width = names.iter().map(|n| n.len()).max().unwrap_or(0).min(40);

    models
        .iter()
        .zip(names)
        .map(|(model, name)| {
            let mut details = Vec::new();
            // First, so narrow terminals don't truncate it away.
            if downloaded_quants(models_dir, model)
                .iter()
                .any(|q| q.eq_ignore_ascii_case(quant))
            {
                details.push("✓ downloaded".to_string());
            }
            if let Some(n) = model.num_parameters {
                details.push(format_params(n));
            }
            if let Some(c) = model.context_window_size.filter(|&c| c > 0) {
                details.push(format!("{} ctx", format_context(c)));
            }
            let size = model
                .gguf_files()
                .iter()
                .find(|f| {
                    f.quant_method
                        .as_deref()
                        .is_some_and(|q| q.eq_ignore_ascii_case(quant))
                })
                .and_then(|f| f.size_bytes);
            if let Some(size) = size {
                details.push(format_size(size));
            }

            SearchItem {
                label: format!("{name:<name_width$}  {}", details.join(" · ")),
                key: name,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn model(name: &str, quants: &[&str]) -> CatalogModel {
        let files: Vec<_> = quants
            .iter()
            .map(|q| json!({ "format": "gguf", "quant_method": q, "size_bytes": 397_800_000u64 }))
            .collect();
        serde_json::from_value(json!({
            "name": name,
            "source": { "name": "Qwen" },
            "num_parameters": 500_000_000u64,
            "context_window_size": 32768,
            "quantized_files": files,
        }))
        .unwrap()
    }

    #[test]
    fn test_offers_quant() {
        let m = model("Qwen2.5-0.5B-Instruct", &["q4_k_m", "q8_0"]);
        assert!(offers_quant(&m, "q4_k_m"));
        assert!(offers_quant(&m, "Q8_0"));
        assert!(!offers_quant(&m, "q6_k"));
    }

    #[test]
    fn test_build_items() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("Qwen2.5-0.5B-Instruct");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Qwen_Qwen2.5-0.5B-Instruct-q4_k_m.gguf"), "x").unwrap();

        let models = vec![
            model("Qwen2.5-0.5B-Instruct", &["q4_k_m"]),
            model("Qwen3-0.6B", &["q4_k_m"]),
        ];
        let items = build_items(&models, tmp.path(), "q4_k_m");

        assert_eq!(items[0].key, "Qwen/Qwen2.5-0.5B-Instruct");
        assert_eq!(
            items[0].label,
            "Qwen/Qwen2.5-0.5B-Instruct  ✓ downloaded · 500M · 32K ctx · 397.8 MB"
        );
        assert_eq!(items[1].key, "Qwen/Qwen3-0.6B");
        assert_eq!(
            items[1].label,
            "Qwen/Qwen3-0.6B             500M · 32K ctx · 397.8 MB"
        );
    }
}
