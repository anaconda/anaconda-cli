//! Register locally served models as a Kilo provider.
//!
//! `ana lm run` adds an `anaconda-cli` provider (OpenAI-compatible, pointing
//! at llama-server) to Kilo's global config with one entry per model, and
//! `ana lm delete` removes those entries again. Comments and formatting in the
//! config file are preserved.

// Registration is only used by `ana lm run`, which requires tool installation.
#![cfg_attr(not(tool_install), allow(dead_code))]

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::store::file_name;
use crate::mcp::clients;

/// Provider key in Kilo's `provider` map.
pub const PROVIDER_ID: &str = "anaconda-cli";
const PROVIDER_NAME: &str = "Anaconda CLI";
const PROVIDER_NPM: &str = "@ai-sdk/openai-compatible";

/// Default maximum output tokens advertised to Kilo.
const DEFAULT_OUTPUT_LIMIT: u64 = 8192;

/// A model entry in the Kilo provider.
#[derive(Debug, Clone, PartialEq)]
pub struct KiloModel {
    /// Model ID used by Kilo and served by llama-server (via `--alias`).
    pub id: String,
    /// Display name in Kilo's model picker.
    pub name: String,
    /// Context window in tokens, if known.
    pub context: Option<u64>,
}

impl KiloModel {
    /// Describe a GGUF file stored under `models_dir`.
    pub fn for_gguf(models_dir: &Path, gguf: &Path, context: Option<u64>) -> Self {
        Self {
            id: model_id(gguf),
            name: display_name(models_dir, gguf),
            context,
        }
    }

    fn to_json(&self) -> Value {
        let mut entry = json!({
            "name": self.name,
            "tool_call": true,
        });
        if let Some(context) = self.context {
            entry["limit"] = json!({
                "context": context,
                "output": context.min(DEFAULT_OUTPUT_LIMIT),
            });
        }
        entry
    }
}

/// Stable model ID derived from a GGUF filename, e.g.
/// `Qwen_Qwen2.5-0.5B-Instruct-q4_k_m.gguf` -> `qwen-qwen2-5-0-5b-instruct-q4-k-m`.
pub fn model_id(gguf: &Path) -> String {
    let stem = gguf
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut id = String::with_capacity(stem.len());
    for c in stem.chars() {
        if c.is_ascii_alphanumeric() {
            id.push(c.to_ascii_lowercase());
        } else if !id.ends_with('-') {
            id.push('-');
        }
    }
    id.trim_matches('-').to_string()
}

/// Human-readable name, e.g. `Qwen2.5-0.5B-Instruct (Q4_K_M)`.
///
/// Uses the model directory name when the file lives in `models_dir`,
/// otherwise the file stem, plus the quantization when it can be detected.
fn display_name(models_dir: &Path, gguf: &Path) -> String {
    let stem = gguf
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let quant = stem.rsplit('-').next().filter(|q| looks_like_quant(q));

    let base = match gguf.parent() {
        Some(dir) if dir.parent() == Some(models_dir) => file_name(dir),
        _ => match quant {
            Some(q) => stem[..stem.len() - q.len()]
                .trim_end_matches('-')
                .to_string(),
            None => stem.clone(),
        },
    };

    match quant {
        Some(q) => format!("{} ({})", base, q.to_ascii_uppercase()),
        None => base,
    }
}

/// Heuristic for GGUF quantization suffixes: `q4_k_m`, `iq4_xs`, `f16`, `bf16`.
fn looks_like_quant(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    let rest = ["iq", "q", "bf", "f"]
        .iter()
        .find_map(|p| lower.strip_prefix(p));
    rest.is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit()))
}

/// Outcome of registering a model with Kilo.
#[derive(Debug)]
pub struct Registered {
    pub config_path: PathBuf,
    /// True when the model entry was newly added (first run).
    pub added: bool,
}

/// Ensure the `anaconda-cli` provider and the model entry exist in Kilo's
/// global config, and point the provider at `base_url`.
///
/// Returns `Ok(None)` when Kilo isn't installed (no `~/.config/kilo`), so we
/// never create config for an app the user doesn't have.
pub fn register(
    model: &KiloModel,
    base_url: &str,
    api_key: Option<&str>,
) -> miette::Result<Option<Registered>> {
    let config_path = clients::config_path("kilo")?;
    if !config_path.parent().is_some_and(Path::is_dir) {
        return Ok(None);
    }

    let text = std::fs::read_to_string(&config_path).unwrap_or_default();
    let (updated, added) = upsert_model(&config_path, &text, model, base_url, api_key)?;
    if updated != text {
        clients::save_jsonc(&config_path, &updated)?;
    }
    Ok(Some(Registered { config_path, added }))
}

/// Remove model entries from every Kilo config file that has them. Drops the
/// provider entirely once it has no models left.
///
/// Returns the config files that were modified.
pub fn unregister(model_ids: &[String]) -> miette::Result<Vec<PathBuf>> {
    let mut modified = Vec::new();
    for path in clients::config_paths("kilo")? {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Some(updated) = remove_models(&path, &text, model_ids)? {
            clients::save_jsonc(&path, &updated)?;
            modified.push(path);
        }
    }
    Ok(modified)
}

/// Set a property on a CST object, replacing any existing value.
fn set_prop(obj: &jsonc_parser::cst::CstObject, key: &str, value: &Value) {
    match obj.get(key) {
        Some(prop) => prop.set_value(clients::to_cst_input(value)),
        None => {
            obj.append(key, clients::to_cst_input(value));
        }
    }
}

/// Add the provider/model to a config document. Returns the new text and
/// whether the model entry was newly added. Existing model entries are left
/// untouched so user edits survive.
fn upsert_model(
    path: &Path,
    text: &str,
    model: &KiloModel,
    base_url: &str,
    api_key: Option<&str>,
) -> miette::Result<(String, bool)> {
    let root = clients::parse_jsonc(path, text)?;
    let providers = root.object_value_or_set().object_value_or_set("provider");

    let Some(provider) = providers.object_value(PROVIDER_ID) else {
        let mut options = json!({ "baseURL": base_url });
        if let Some(key) = api_key {
            options["apiKey"] = json!(key);
        }
        let entry = json!({
            "npm": PROVIDER_NPM,
            "name": PROVIDER_NAME,
            "options": options,
            "models": { model.id.clone(): model.to_json() },
        });
        providers.append(PROVIDER_ID, clients::to_cst_input(&entry));
        return Ok((root.to_string(), true));
    };

    if provider.get("npm").is_none() {
        provider.append("npm", clients::to_cst_input(&json!(PROVIDER_NPM)));
    }
    if provider.get("name").is_none() {
        provider.append("name", clients::to_cst_input(&json!(PROVIDER_NAME)));
    }

    // The server address can change between runs (--host/--port).
    let options = provider.object_value_or_set("options");
    let current_url = options
        .get("baseURL")
        .and_then(|p| p.value())
        .and_then(|v| v.to_serde_value());
    if current_url != Some(json!(base_url)) {
        set_prop(&options, "baseURL", &json!(base_url));
    }
    if let Some(key) = api_key {
        set_prop(&options, "apiKey", &json!(key));
    }

    let models = provider.object_value_or_set("models");
    let added = models.get(&model.id).is_none();
    if added {
        models.append(&model.id, clients::to_cst_input(&model.to_json()));
    }

    Ok((root.to_string(), added))
}

/// Remove model entries from a config document. Returns `None` when nothing
/// changed.
fn remove_models(path: &Path, text: &str, model_ids: &[String]) -> miette::Result<Option<String>> {
    let root = clients::parse_jsonc(path, text)?;
    let Some(providers) = root.object_value().and_then(|o| o.object_value("provider")) else {
        return Ok(None);
    };
    let Some(models) = providers
        .object_value(PROVIDER_ID)
        .and_then(|p| p.object_value("models"))
    else {
        return Ok(None);
    };

    let mut changed = false;
    for id in model_ids {
        if let Some(prop) = models.get(id) {
            prop.remove();
            changed = true;
        }
    }
    if !changed {
        return Ok(None);
    }

    if models.properties().is_empty()
        && let Some(provider) = providers.get(PROVIDER_ID)
    {
        provider.remove();
    }

    Ok(Some(root.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "http://127.0.0.1:8080/v1";

    fn qwen() -> KiloModel {
        KiloModel {
            id: "qwen-qwen2-5-0-5b-instruct-q4-k-m".to_string(),
            name: "Qwen2.5-0.5B-Instruct (Q4_K_M)".to_string(),
            context: Some(32768),
        }
    }

    fn parse(text: &str) -> Value {
        jsonc_parser::parse_to_serde_value(text, &Default::default()).unwrap()
    }

    #[test]
    fn test_model_id() {
        assert_eq!(
            model_id(Path::new("/m/Qwen_Qwen2.5-0.5B-Instruct-q4_k_m.gguf")),
            "qwen-qwen2-5-0-5b-instruct-q4-k-m"
        );
    }

    #[test]
    fn test_display_name_in_models_dir() {
        let models = Path::new("/home/.ana/models");
        let gguf = models.join("Qwen2.5-0.5B-Instruct/Qwen_Qwen2.5-0.5B-Instruct-q4_k_m.gguf");
        assert_eq!(
            display_name(models, &gguf),
            "Qwen2.5-0.5B-Instruct (Q4_K_M)"
        );
    }

    #[test]
    fn test_display_name_outside_models_dir() {
        let models = Path::new("/home/.ana/models");
        assert_eq!(
            display_name(models, Path::new("/tmp/Llama-3.2-3B-Instruct-Q4_K_M.gguf")),
            "Llama-3.2-3B-Instruct (Q4_K_M)"
        );
        assert_eq!(
            display_name(models, Path::new("/tmp/custom.gguf")),
            "custom"
        );
    }

    #[test]
    fn test_looks_like_quant() {
        for q in ["q4_k_m", "Q8_0", "iq4_xs", "f16", "bf16"] {
            assert!(looks_like_quant(q), "{q}");
        }
        for q in ["Instruct", "0.5B", "final", "q"] {
            assert!(!looks_like_quant(q), "{q}");
        }
    }

    #[test]
    fn test_upsert_creates_provider() {
        let path = Path::new("kilo.jsonc");
        let (text, added) = upsert_model(path, "", &qwen(), URL, None).unwrap();
        assert!(added);
        let config = parse(&text);
        let provider = &config["provider"][PROVIDER_ID];
        assert_eq!(provider["npm"], PROVIDER_NPM);
        assert_eq!(provider["name"], PROVIDER_NAME);
        assert_eq!(provider["options"], json!({ "baseURL": URL }));
        assert_eq!(
            provider["models"]["qwen-qwen2-5-0-5b-instruct-q4-k-m"],
            json!({
                "name": "Qwen2.5-0.5B-Instruct (Q4_K_M)",
                "tool_call": true,
                "limit": { "context": 32768, "output": 8192 },
            })
        );
    }

    #[test]
    fn test_upsert_preserves_other_config_and_comments() {
        let path = Path::new("kilo.jsonc");
        let original = r#"{
  // my settings
  "model": "kilo/some-model",
  "provider": {
    "kilo-desktop": { "name": "Kilo Desktop" }
  }
}"#;
        let (text, _) = upsert_model(path, original, &qwen(), URL, None).unwrap();
        assert!(text.contains("// my settings"));
        let config = parse(&text);
        assert_eq!(config["model"], "kilo/some-model");
        assert_eq!(config["provider"]["kilo-desktop"]["name"], "Kilo Desktop");
        assert!(config["provider"][PROVIDER_ID].is_object());
    }

    #[test]
    fn test_upsert_second_model_and_url_update() {
        let path = Path::new("kilo.jsonc");
        let (text, _) = upsert_model(path, "", &qwen(), URL, None).unwrap();
        let other = KiloModel {
            id: "other".to_string(),
            name: "Other".to_string(),
            context: None,
        };
        let new_url = "http://127.0.0.1:9000/v1";
        let (text, added) = upsert_model(path, &text, &other, new_url, Some("secret")).unwrap();
        assert!(added);
        let provider = &parse(&text)["provider"][PROVIDER_ID];
        assert_eq!(provider["options"]["baseURL"], new_url);
        assert_eq!(provider["options"]["apiKey"], "secret");
        assert!(provider["models"]["qwen-qwen2-5-0-5b-instruct-q4-k-m"].is_object());
        assert_eq!(
            provider["models"]["other"],
            json!({ "name": "Other", "tool_call": true })
        );
    }

    #[test]
    fn test_upsert_existing_model_is_noop() {
        let path = Path::new("kilo.jsonc");
        let (text, _) = upsert_model(path, "", &qwen(), URL, None).unwrap();
        let (again, added) = upsert_model(path, &text, &qwen(), URL, None).unwrap();
        assert!(!added);
        assert_eq!(again, text);
    }

    #[test]
    fn test_remove_model_keeps_provider_with_others() {
        let path = Path::new("kilo.jsonc");
        let (text, _) = upsert_model(path, "", &qwen(), URL, None).unwrap();
        let other = KiloModel {
            id: "other".to_string(),
            name: "Other".to_string(),
            context: None,
        };
        let (text, _) = upsert_model(path, &text, &other, URL, None).unwrap();

        let text = remove_models(path, &text, &["other".to_string()])
            .unwrap()
            .unwrap();
        let models = &parse(&text)["provider"][PROVIDER_ID]["models"];
        assert!(models.get("other").is_none());
        assert!(models.get("qwen-qwen2-5-0-5b-instruct-q4-k-m").is_some());
    }

    #[test]
    fn test_remove_last_model_drops_provider() {
        let path = Path::new("kilo.jsonc");
        let original = r#"{ "provider": { "kilo-desktop": { "name": "Kilo Desktop" } } }"#;
        let (text, _) = upsert_model(path, original, &qwen(), URL, None).unwrap();
        let text = remove_models(path, &text, &[qwen().id]).unwrap().unwrap();
        let config = parse(&text);
        assert!(config["provider"].get(PROVIDER_ID).is_none());
        assert_eq!(config["provider"]["kilo-desktop"]["name"], "Kilo Desktop");
    }

    #[test]
    fn test_remove_absent_is_none() {
        let path = Path::new("kilo.jsonc");
        assert!(remove_models(path, "{}", &[qwen().id]).unwrap().is_none());
        let (text, _) = upsert_model(path, "", &qwen(), URL, None).unwrap();
        assert!(
            remove_models(path, &text, &["missing".to_string()])
                .unwrap()
                .is_none()
        );
    }
}
