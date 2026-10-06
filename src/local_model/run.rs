use std::path::{Path, PathBuf};

use miette::miette;

#[cfg_attr(not(tool_install), allow(unused_imports))]
use super::kilo::KiloModel;
#[cfg_attr(not(tool_install), allow(unused_imports))]
use super::server;
use super::store::{self, file_name};
use crate::context::CommandContext;

/// Name of the managed tool providing `llama-server`.
#[cfg_attr(not(tool_install), allow(dead_code))]
const LLAMA_TOOL: &str = "llama.cpp";

/// Quantization pulled when the model isn't available locally and the user
/// didn't request one. `outerbounds mc pull` requires an explicit quant for GGUF.
#[cfg_attr(not(tool_install), allow(dead_code))]
pub const DEFAULT_QUANT: &str = "q4_k_m";

/// Options for `ana lm run`.
#[cfg_attr(not(tool_install), allow(dead_code))]
pub struct RunOptions<'a> {
    pub model: &'a str,
    pub quant: Option<&'a str>,
    pub host: &'a str,
    pub port: u16,
    pub extra_args: &'a [String],
}

/// Serve a local GGUF model from the shared background `llama-server`.
///
/// 1. Resolves the model in `~/.ana/models`, pulling it via `outerbounds mc pull`
///    if it isn't downloaded yet.
/// 2. Ensures llama.cpp is installed as a managed tool.
/// 3. Adds the model to the router presets and the Kilo provider.
/// 4. Starts the shared server in the background (or reuses/restarts it) and
///    prints how to stop it.
#[cfg(tool_install)]
pub async fn run(ctx: &mut CommandContext, opts: RunOptions<'_>) -> miette::Result<()> {
    use crate::ui::status;

    let models_dir = crate::paths::models_dir();

    let model_path = match lookup_model(&models_dir, opts.model, opts.quant)? {
        Lookup::Found(path) => path,
        // Several quants are downloaded and none was requested: prefer the default.
        Lookup::Ambiguous(candidates) => match opts.quant {
            None => match lookup_model(&models_dir, opts.model, Some(DEFAULT_QUANT))? {
                Lookup::Found(path) => path,
                _ => return Err(ambiguous_error(opts.model, &candidates)),
            },
            Some(_) => return Err(ambiguous_error(opts.model, &candidates)),
        },
        Lookup::NotDownloaded => {
            let quant = opts.quant.unwrap_or(DEFAULT_QUANT);
            status::info(&format!(
                "Model {} ({}) not found locally, downloading...",
                status::highlight(opts.model),
                quant
            ));
            super::pull::pull(
                ctx,
                super::pull::PullOptions {
                    model: opts.model,
                    format: super::commands::ModelFormat::Gguf,
                    file: None,
                    quant: Some(quant),
                },
            )
            .await?;

            match lookup_model(&models_dir, opts.model, Some(quant))? {
                Lookup::Found(path) => path,
                Lookup::Ambiguous(candidates) => {
                    return Err(ambiguous_error(opts.model, &candidates));
                }
                Lookup::NotDownloaded => {
                    return Err(miette!(
                        "Downloaded '{}' but could not locate its GGUF file in {}",
                        opts.model,
                        models_dir.display()
                    ));
                }
            }
        }
    };

    crate::tools::install::ensure_tool(ctx, LLAMA_TOOL).await?;

    let context = flag_value(opts.extra_args, &["-c", "--ctx-size"])
        .and_then(|c| c.parse::<u64>().ok())
        .filter(|&c| c > 0)
        .or_else(|| super::gguf::context_length(&model_path));
    let kilo_model = KiloModel::for_gguf(&models_dir, &model_path, context);

    // The router serves each preset under its section name, so the model is
    // reachable by the same ID it's registered with in Kilo.
    let files = server::ServerFiles::default_location();
    let mut presets = server::Presets::load(&files.presets);
    if presets.upsert_model(&kilo_model.id, &model_path) {
        presets.save(&files.presets)?;
    }

    // Native conda packages install binaries to `Library/bin` on Windows.
    let bin_subdir = if cfg!(windows) {
        Path::new("Library").join("bin")
    } else {
        PathBuf::from("bin")
    };
    let server_bin = crate::tools::tool_binary_path(LLAMA_TOOL, &bin_subdir, "llama-server")?;

    let running = server::ensure_running(
        ctx,
        &files,
        &server_bin,
        opts.host,
        opts.port,
        opts.extra_args,
    )
    .await?;

    // Only point Kilo at the server once it's actually up.
    let api_key = flag_value(opts.extra_args, &["--api-key"])
        .and_then(|k| k.split(',').next().map(str::to_string));
    register_with_kilo(&kilo_model, opts.host, opts.port, api_key.as_deref());

    let api_url = format!("http://{}:{}/v1", client_host(opts.host), opts.port);
    let state = match running.action {
        server::ServerAction::Reused => "already running",
        server::ServerAction::Started => "started",
        server::ServerAction::Restarted => "restarted to pick up changes",
    };
    status::success(&format!(
        "Serving {} as {}",
        status::highlight(&file_name(&model_path)),
        status::highlight(&kilo_model.id)
    ));
    eprintln!("  Server:  {} (pid {}, {})", api_url, running.pid, state);
    eprintln!("  Logs:    {}", running.log.display());
    eprintln!(
        "  Stop:    {}",
        status::highlight(&server::stop_command(running.pid))
    );
    Ok(())
}

#[cfg(not(tool_install))]
pub async fn run(_ctx: &mut CommandContext, _opts: RunOptions<'_>) -> miette::Result<()> {
    Err(crate::errors::ToolManagementUnavailableError.into())
}

/// Add the model to the Kilo provider. Failures are reported as warnings and
/// never prevent the server from starting.
#[cfg(tool_install)]
fn register_with_kilo(model: &KiloModel, host: &str, port: u16, api_key: Option<&str>) {
    use crate::ui::status;

    let base_url = format!("http://{}:{}/v1", client_host(host), port);
    match super::kilo::register(model, &base_url, api_key) {
        Ok(Some(registered)) if registered.added => status::success(&format!(
            "Added {} to the {} provider in {}",
            status::highlight(&model.id),
            status::highlight(super::kilo::PROVIDER_ID),
            registered.config_path.display()
        )),
        Ok(_) => {}
        Err(e) => status::warn(&format!("Could not update Kilo config: {e}")),
    }
}

/// Address clients should connect to. Wildcard bind addresses aren't
/// connectable, so map them to loopback.
#[cfg_attr(not(tool_install), allow(dead_code))]
pub(super) fn client_host(host: &str) -> &str {
    match host {
        "0.0.0.0" => "127.0.0.1",
        "::" | "[::]" => "[::1]",
        other => other,
    }
}

/// Value of the last occurrence of any of `names` in `args`, accepting both
/// `--flag value` and `--flag=value`.
#[cfg_attr(not(tool_install), allow(dead_code))]
fn flag_value(args: &[String], names: &[&str]) -> Option<String> {
    let mut found = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if names.contains(&arg.as_str()) {
            found = iter.next().cloned();
        } else if let Some((name, value)) = arg.split_once('=')
            && names.contains(&name)
        {
            found = Some(value.to_string());
        }
    }
    found
}

/// Result of looking up a model locally.
#[derive(Debug, PartialEq)]
#[cfg_attr(not(tool_install), allow(dead_code))]
enum Lookup {
    /// Exactly one matching GGUF file was found.
    Found(PathBuf),
    /// The model (or the requested quantization) isn't downloaded.
    NotDownloaded,
    /// Several GGUF files match; the user must pick one with `--quant`.
    Ambiguous(Vec<PathBuf>),
}

/// Look up a model in `models_dir`.
///
/// `model` may be:
/// - a path to a `.gguf` file, or
/// - a model name, either the catalog name (`Qwen/Qwen2.5-0.5B-Instruct`) or
///   the local directory name (`Qwen2.5-0.5B-Instruct`).
///
/// When `quant` is given, only GGUF files whose name contains it
/// (case-insensitive) are considered.
#[cfg_attr(not(tool_install), allow(dead_code))]
fn lookup_model(models_dir: &Path, model: &str, quant: Option<&str>) -> miette::Result<Lookup> {
    let direct = Path::new(model);
    if direct.is_file() {
        return Ok(Lookup::Found(direct.to_path_buf()));
    }
    if direct
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf"))
    {
        return Err(miette!("Model file not found: {}", model));
    }

    let Some(model_dir) = store::find_model_dir(models_dir, model) else {
        return Ok(Lookup::NotDownloaded);
    };

    let mut ggufs = store::gguf_files(&model_dir)?;
    if let Some(q) = quant {
        store::filter_by_quant(&mut ggufs, q);
    }

    Ok(match ggufs.len() {
        0 => Lookup::NotDownloaded,
        1 => Lookup::Found(ggufs.remove(0)),
        _ => Lookup::Ambiguous(ggufs),
    })
}

#[cfg_attr(not(tool_install), allow(dead_code))]
fn ambiguous_error(model: &str, candidates: &[PathBuf]) -> miette::Report {
    miette!(
        help = format!(
            "Select one with --quant. Matching files:\n{}",
            candidates
                .iter()
                .map(|p| format!("  {}", file_name(p)))
                .collect::<Vec<_>>()
                .join("\n")
        ),
        "Multiple GGUF files found for '{}'",
        model
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }

    #[test]
    fn test_lookup_direct_file_path() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("custom.gguf");
        touch(&file);
        let found = lookup_model(tmp.path(), file.to_str().unwrap(), None).unwrap();
        assert_eq!(found, Lookup::Found(file));
    }

    #[test]
    fn test_lookup_missing_gguf_path_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("nope.gguf");
        let err = lookup_model(tmp.path(), missing.to_str().unwrap(), None).unwrap_err();
        assert!(err.to_string().contains("Model file not found"));
    }

    #[test]
    fn test_lookup_catalog_name_short_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("Qwen2.5-0.5B-Instruct/model-q4_k_m.gguf");
        touch(&file);
        let found = lookup_model(tmp.path(), "Qwen/Qwen2.5-0.5B-Instruct", None).unwrap();
        assert_eq!(found, Lookup::Found(file));
    }

    #[test]
    fn test_lookup_catalog_name_safe_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp
            .path()
            .join("Qwen_Qwen2.5-0.5B-Instruct/model-q4_k_m.gguf");
        touch(&file);
        let found = lookup_model(tmp.path(), "Qwen/Qwen2.5-0.5B-Instruct", None).unwrap();
        assert_eq!(found, Lookup::Found(file));
    }

    #[test]
    fn test_lookup_local_dir_name() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("Qwen2.5-0.5B-Instruct/model-q4_k_m.gguf");
        touch(&file);
        let found = lookup_model(tmp.path(), "Qwen2.5-0.5B-Instruct", None).unwrap();
        assert_eq!(found, Lookup::Found(file));
    }

    #[test]
    fn test_lookup_ignores_non_gguf_files() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("m/model-q4_k_m.gguf");
        touch(&file);
        touch(&tmp.path().join("m/model.safetensors"));
        let found = lookup_model(tmp.path(), "m", None).unwrap();
        assert_eq!(found, Lookup::Found(file));
    }

    #[test]
    fn test_lookup_multiple_is_ambiguous() {
        let tmp = tempfile::tempdir().unwrap();
        touch(&tmp.path().join("m/model-q4_k_m.gguf"));
        touch(&tmp.path().join("m/model-q8_0.gguf"));
        let found = lookup_model(tmp.path(), "m", None).unwrap();
        assert!(matches!(found, Lookup::Ambiguous(c) if c.len() == 2));
    }

    #[test]
    fn test_lookup_multiple_with_quant() {
        let tmp = tempfile::tempdir().unwrap();
        touch(&tmp.path().join("m/model-q4_k_m.gguf"));
        let q8 = tmp.path().join("m/model-q8_0.gguf");
        touch(&q8);
        let found = lookup_model(tmp.path(), "m", Some("Q8_0")).unwrap();
        assert_eq!(found, Lookup::Found(q8));
    }

    #[test]
    fn test_lookup_quant_not_downloaded() {
        let tmp = tempfile::tempdir().unwrap();
        touch(&tmp.path().join("m/model-q4_k_m.gguf"));
        let found = lookup_model(tmp.path(), "m", Some("q6_k")).unwrap();
        assert_eq!(found, Lookup::NotDownloaded);
    }

    #[test]
    fn test_lookup_missing_model_not_downloaded() {
        let tmp = tempfile::tempdir().unwrap();
        let found = lookup_model(tmp.path(), "Org/Missing", None).unwrap();
        assert_eq!(found, Lookup::NotDownloaded);
    }

    #[test]
    fn test_lookup_no_gguf_files_not_downloaded() {
        let tmp = tempfile::tempdir().unwrap();
        touch(&tmp.path().join("m/model.safetensors"));
        let found = lookup_model(tmp.path(), "m", None).unwrap();
        assert_eq!(found, Lookup::NotDownloaded);
    }

    #[test]
    fn test_flag_value() {
        let args: Vec<String> = ["--ctx-size", "4096", "--api-key=abc", "-c", "8192"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            flag_value(&args, &["-c", "--ctx-size"]),
            Some("8192".to_string())
        );
        assert_eq!(flag_value(&args, &["--api-key"]), Some("abc".to_string()));
        assert_eq!(flag_value(&args, &["--alias"]), None);
    }

    #[test]
    fn test_client_host() {
        assert_eq!(client_host("0.0.0.0"), "127.0.0.1");
        assert_eq!(client_host("127.0.0.1"), "127.0.0.1");
        assert_eq!(client_host("::"), "[::1]");
    }
}
