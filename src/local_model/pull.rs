use std::path::Path;

use crate::context::CommandContext;

use super::commands::ModelFormat;

/// Options for `ana lm pull`.
pub struct PullOptions<'a> {
    pub model: &'a str,
    pub format: ModelFormat,
    pub file: Option<&'a str>,
    pub quant: Option<&'a str>,
}

/// Download a model into the ana-managed models directory by delegating to
/// `outerbounds mc pull` (the same command `ana platform mc pull` runs).
#[cfg(all(unix, tool_install))]
pub async fn pull(ctx: &mut CommandContext, opts: PullOptions<'_>) -> miette::Result<()> {
    use miette::{IntoDiagnostic, WrapErr};

    let models_dir = crate::paths::models_dir();
    std::fs::create_dir_all(&models_dir)
        .into_diagnostic()
        .wrap_err_with(|| format!("Failed to create {}", models_dir.display()))?;

    let args = build_args(&opts, &models_dir);
    crate::outerbounds::run(ctx, &args).await
}

#[cfg(not(all(unix, tool_install)))]
pub async fn pull(_ctx: &mut CommandContext, _opts: PullOptions<'_>) -> miette::Result<()> {
    Err(miette::miette!(
        "`ana lm pull` requires the outerbounds tool, which is not available on this build"
    ))
}

/// Build the argument list passed to the outerbounds CLI.
#[cfg_attr(not(all(unix, tool_install)), allow(dead_code))]
fn build_args(opts: &PullOptions<'_>, output_dir: &Path) -> Vec<String> {
    let mut args = vec![
        "mc".to_string(),
        "pull".to_string(),
        "--model".to_string(),
        opts.model.to_string(),
        "--format".to_string(),
        opts.format.to_string(),
        "--output-dir".to_string(),
        output_dir.to_string_lossy().into_owned(),
    ];
    if let Some(file) = opts.file {
        args.push("--file".to_string());
        args.push(file.to_string());
    }
    if let Some(quant) = opts.quant {
        args.push("--quant".to_string());
        args.push(quant.to_string());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_build_args_minimal() {
        let opts = PullOptions {
            model: "Qwen/Qwen2.5-0.5B-Instruct",
            format: ModelFormat::Safetensor,
            file: None,
            quant: None,
        };
        let args = build_args(&opts, &PathBuf::from("/test/ana/models"));
        assert_eq!(
            args,
            vec![
                "mc",
                "pull",
                "--model",
                "Qwen/Qwen2.5-0.5B-Instruct",
                "--format",
                "safetensor",
                "--output-dir",
                "/test/ana/models",
            ]
        );
    }

    #[test]
    fn test_build_args_gguf_with_file_and_quant() {
        let opts = PullOptions {
            model: "Qwen/Qwen2.5-7B-Instruct",
            format: ModelFormat::Gguf,
            file: Some("model.gguf"),
            quant: Some("q4_k_m"),
        };
        let args = build_args(&opts, &PathBuf::from("/m"));
        assert_eq!(
            args,
            vec![
                "mc",
                "pull",
                "--model",
                "Qwen/Qwen2.5-7B-Instruct",
                "--format",
                "gguf",
                "--output-dir",
                "/m",
                "--file",
                "model.gguf",
                "--quant",
                "q4_k_m",
            ]
        );
    }
}
