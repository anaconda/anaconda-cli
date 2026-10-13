use std::path::{Path, PathBuf};

use miette::{IntoDiagnostic, WrapErr, miette};

use super::store::{self, file_name, format_size};
use crate::input::prompt_yes_no;
use crate::ui::status;

/// What `ana lm delete` will remove.
#[derive(Debug, PartialEq)]
struct DeletePlan {
    /// The model's directory under `~/.ana/models`.
    model_dir: PathBuf,
    /// Paths to remove. Either `[model_dir]` or a set of GGUF files inside it.
    targets: Vec<PathBuf>,
}

impl DeletePlan {
    fn removes_whole_dir(&self) -> bool {
        self.targets == [self.model_dir.clone()]
    }

    /// Kilo model IDs for every GGUF file this plan removes.
    fn kilo_model_ids(&self) -> miette::Result<Vec<String>> {
        let ggufs = if self.removes_whole_dir() {
            store::gguf_files(&self.model_dir)?
        } else {
            self.targets.clone()
        };
        Ok(ggufs.iter().map(|p| super::kilo::model_id(p)).collect())
    }
}

/// Delete a downloaded model, or a single quantization of it.
pub fn delete(model: &str, quant: Option<&str>, force: bool) -> miette::Result<()> {
    let models_dir = crate::paths::models_dir();
    let plan = plan_delete(&models_dir, model, quant)?;

    let total: u64 = plan.targets.iter().map(|p| store::disk_size(p)).sum();
    eprintln!("The following will be deleted:");
    for target in &plan.targets {
        eprintln!(
            "  {} ({})",
            target.display(),
            format_size(store::disk_size(target))
        );
    }
    eprintln!();

    let prompt = format!("Delete {} and free {}?", model, format_size(total));
    if !force && !prompt_yes_no(&prompt, false) {
        eprintln!("Aborted.");
        return Ok(());
    }

    // Capture Kilo model IDs before the files are gone.
    let model_ids = plan.kilo_model_ids()?;

    execute(&plan)?;

    let what = match quant {
        Some(q) if !plan.removes_whole_dir() => format!("{} ({})", model, q),
        _ => model.to_string(),
    };
    status::success(&format!("Deleted {}, freed {}", what, format_size(total)));

    match super::kilo::unregister(&model_ids) {
        Ok(modified) => {
            for path in modified {
                status::success(&format!(
                    "Removed from the {} provider in {}",
                    status::highlight(super::kilo::PROVIDER_ID),
                    path.display()
                ));
            }
        }
        Err(e) => status::warn(&format!("Could not update Kilo config: {e}")),
    }

    // Drop it from the shared server's presets; the next `ana lm run`
    // restarts the server without it.
    if let Err(e) = super::server::remove_presets(&model_ids) {
        status::warn(&format!("Could not update server presets: {e}"));
    }
    Ok(())
}

/// Work out what to delete without touching the filesystem.
///
/// Without `quant`, the whole model directory is removed. With `quant`, only
/// GGUF files whose name contains it (case-insensitive) are removed.
fn plan_delete(models_dir: &Path, model: &str, quant: Option<&str>) -> miette::Result<DeletePlan> {
    let model_dir = store::find_model_dir(models_dir, model)
        .ok_or_else(|| miette!("Model '{}' not found in {}", model, models_dir.display()))?;

    let Some(q) = quant else {
        return Ok(DeletePlan {
            targets: vec![model_dir.clone()],
            model_dir,
        });
    };

    let all = store::gguf_files(&model_dir)?;
    let mut targets = all.clone();
    store::filter_by_quant(&mut targets, q);

    if targets.is_empty() {
        let help = if all.is_empty() {
            "This model has no GGUF files. Omit --quant to delete the whole model.".to_string()
        } else {
            format!(
                "Downloaded files:\n{}",
                all.iter()
                    .map(|p| format!("  {}", file_name(p)))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };
        return Err(miette!(
            help = help,
            "No GGUF file for '{}' matches quantization '{}'",
            model,
            q
        ));
    }

    Ok(DeletePlan { model_dir, targets })
}

/// Remove everything in the plan, then clean up the model directory if empty.
fn execute(plan: &DeletePlan) -> miette::Result<()> {
    for target in &plan.targets {
        let result = if target.is_dir() {
            std::fs::remove_dir_all(target)
        } else {
            std::fs::remove_file(target)
        };
        result
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to delete {}", target.display()))?;
    }

    // Removing the last quant leaves an empty model directory behind.
    if plan.model_dir.is_dir()
        && std::fs::read_dir(&plan.model_dir).is_ok_and(|mut entries| entries.next().is_none())
    {
        std::fs::remove_dir(&plan.model_dir)
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to delete {}", plan.model_dir.display()))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn test_plan_whole_model() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("Qwen2.5-0.5B-Instruct");
        touch(&dir.join("model-q4_k_m.gguf"));
        let plan = plan_delete(tmp.path(), "Qwen/Qwen2.5-0.5B-Instruct", None).unwrap();
        assert_eq!(plan.model_dir, dir);
        assert!(plan.removes_whole_dir());
    }

    #[test]
    fn test_plan_single_quant() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("m");
        touch(&dir.join("model-q4_k_m.gguf"));
        touch(&dir.join("model-q8_0.gguf"));
        let plan = plan_delete(tmp.path(), "m", Some("Q8_0")).unwrap();
        assert_eq!(plan.targets, vec![dir.join("model-q8_0.gguf")]);
        assert!(!plan.removes_whole_dir());
    }

    #[test]
    fn test_kilo_model_ids() {
        let tmp = tempfile::tempdir().unwrap();
        touch(&tmp.path().join("m/Org_M-q4_k_m.gguf"));
        touch(&tmp.path().join("m/Org_M-q8_0.gguf"));

        let whole = plan_delete(tmp.path(), "m", None).unwrap();
        assert_eq!(
            whole.kilo_model_ids().unwrap(),
            vec!["org-m-q4-k-m", "org-m-q8-0"]
        );

        let single = plan_delete(tmp.path(), "m", Some("q8_0")).unwrap();
        assert_eq!(single.kilo_model_ids().unwrap(), vec!["org-m-q8-0"]);
    }

    #[test]
    fn test_plan_missing_model() {
        let tmp = tempfile::tempdir().unwrap();
        let err = plan_delete(tmp.path(), "Org/Missing", None).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_plan_quant_no_match() {
        let tmp = tempfile::tempdir().unwrap();
        touch(&tmp.path().join("m/model-q4_k_m.gguf"));
        let err = plan_delete(tmp.path(), "m", Some("q6_k")).unwrap_err();
        assert!(err.to_string().contains("matches quantization"));
    }

    #[test]
    fn test_plan_rejects_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let models = tmp.path().join("models");
        std::fs::create_dir_all(&models).unwrap();
        assert!(plan_delete(&models, "..", None).is_err());
        assert!(plan_delete(&models, "models/..", None).is_err());
    }

    #[test]
    fn test_execute_single_quant_keeps_others() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("m");
        touch(&dir.join("model-q4_k_m.gguf"));
        touch(&dir.join("model-q8_0.gguf"));
        let plan = plan_delete(tmp.path(), "m", Some("q8_0")).unwrap();
        execute(&plan).unwrap();
        assert!(dir.join("model-q4_k_m.gguf").exists());
        assert!(!dir.join("model-q8_0.gguf").exists());
    }

    #[test]
    fn test_execute_last_quant_removes_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("m");
        touch(&dir.join("model-q4_k_m.gguf"));
        let plan = plan_delete(tmp.path(), "m", Some("q4_k_m")).unwrap();
        execute(&plan).unwrap();
        assert!(!dir.exists());
    }

    #[test]
    fn test_execute_whole_model() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("m");
        touch(&dir.join("model-q4_k_m.gguf"));
        touch(&dir.join("model.safetensors"));
        let plan = plan_delete(tmp.path(), "m", None).unwrap();
        execute(&plan).unwrap();
        assert!(!dir.exists());
        assert!(tmp.path().exists());
    }
}
