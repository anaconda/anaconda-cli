//! Read-only client for the Anaconda model catalog on the platform API.
//!
//! Uses the same credentials as the outerbounds CLI (`ana platform`):
//! `METAFLOW_SERVICE_AUTH_KEY` from `$METAFLOW_HOME/config[_<profile>].json`,
//! with the API server and perimeter taken from the perimeter's remote
//! metaflow config.

use std::path::{Path, PathBuf};
use std::time::Duration;

use miette::{IntoDiagnostic, WrapErr, miette};
use serde::Deserialize;
use serde_json::Value;

use crate::context::CommandContext;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const PAGE_SIZE: usize = 100;

/// A model in the catalog.
#[derive(Debug, Clone, Deserialize)]
pub struct CatalogModel {
    pub name: String,
    #[serde(default)]
    pub source: Option<CatalogSource>,
    #[serde(default)]
    pub num_parameters: Option<u64>,
    #[serde(default)]
    pub context_window_size: Option<u64>,
    #[serde(default)]
    pub supports_tool_calling: Option<bool>,
    /// Task the model was trained for, e.g. `text-generation`.
    #[serde(default)]
    pub trained_for: Option<String>,
    #[serde(default)]
    pub tags: Vec<CatalogTag>,
    #[serde(default)]
    pub quantized_files: Vec<CatalogFile>,
}

impl CatalogModel {
    /// Publisher (source organization), e.g. `Qwen`.
    pub fn publisher(&self) -> Option<&str> {
        self.source
            .as_ref()
            .and_then(|s| s.name.as_deref())
            .filter(|s| !s.is_empty())
    }

    /// Tag names, e.g. `["chat", "tool-calling", "size-tiny"]`.
    pub fn tag_names(&self) -> Vec<&str> {
        self.tags.iter().filter_map(|t| t.name.as_deref()).collect()
    }

    /// Size class from the `size-*` tag, e.g. `tiny`.
    pub fn size_class(&self) -> Option<&str> {
        self.tag_names()
            .into_iter()
            .find_map(|t| t.strip_prefix(SIZE_TAG_PREFIX))
    }

    /// Name accepted by `ana lm pull`/`run`, e.g. `Qwen/Qwen2.5-0.5B-Instruct`.
    pub fn full_name(&self) -> String {
        match self.publisher() {
            Some(source) => format!("{}/{}", source, self.name),
            None => self.name.clone(),
        }
    }

    /// GGUF files, sorted by size (smallest first).
    pub fn gguf_files(&self) -> Vec<&CatalogFile> {
        let mut files: Vec<&CatalogFile> = self
            .quantized_files
            .iter()
            .filter(|f| f.format.as_deref() == Some("gguf"))
            .collect();
        files.sort_by_key(|f| f.size_bytes.unwrap_or(u64::MAX));
        files
    }

    /// True when the catalog has non-GGUF (e.g. safetensors) files.
    pub fn has_non_gguf(&self) -> bool {
        self.quantized_files
            .iter()
            .any(|f| f.format.as_deref() != Some("gguf"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CatalogSource {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CatalogTag {
    #[serde(default)]
    pub name: Option<String>,
}

/// Size classes are published as `size-<class>` tags.
const SIZE_TAG_PREFIX: &str = "size-";

/// Filters for listing catalog models. All set filters must match.
#[derive(Debug, Default, Clone)]
pub struct CatalogFilter {
    /// Free-text name search.
    pub name: Option<String>,
    /// Publisher (source organization), exact match, case-insensitive.
    pub publisher: Option<String>,
    /// Purpose (`trained_for`), exact match, case-insensitive.
    pub purpose: Option<String>,
    /// Tags; the model must have every one.
    pub tags: Vec<String>,
    /// Size classes (`tiny`, `small`, ...); the model must match any one.
    pub sizes: Vec<String>,
    /// File format or quantization (`gguf`, `safetensors`, `q4_k_m`, ...).
    pub file: Option<String>,
}

impl CatalogFilter {
    /// Query parameters the catalog API filters on server-side.
    ///
    /// Repeated `tags` params are OR'ed by the API, so at most one tag is sent
    /// to narrow results; [`CatalogFilter::matches`] applies the full filter.
    fn query_params(&self) -> Vec<(&'static str, String)> {
        let mut params = Vec::new();
        if let Some(v) = &self.name {
            params.push(("search", v.clone()));
        }
        if let Some(v) = &self.publisher {
            params.push(("source_name", v.clone()));
        }
        if let Some(v) = &self.purpose {
            params.push(("trained_for", v.clone()));
        }
        let narrowing_tag = self
            .tags
            .first()
            .cloned()
            .or_else(|| match self.sizes.as_slice() {
                [size] => Some(format!("{SIZE_TAG_PREFIX}{size}")),
                _ => None,
            });
        if let Some(tag) = narrowing_tag {
            params.push(("tags", tag));
        }
        params
    }

    /// True when `model` satisfies every filter.
    pub fn matches(&self, model: &CatalogModel) -> bool {
        let eq = |a: &str, b: &str| a.eq_ignore_ascii_case(b);

        if let Some(name) = &self.name
            && !model
                .full_name()
                .to_ascii_lowercase()
                .contains(&name.to_ascii_lowercase())
        {
            return false;
        }
        if let Some(publisher) = &self.publisher
            && !model.publisher().is_some_and(|p| eq(p, publisher))
        {
            return false;
        }
        if let Some(purpose) = &self.purpose
            && !model.trained_for.as_deref().is_some_and(|p| eq(p, purpose))
        {
            return false;
        }

        let tags = model.tag_names();
        if !self
            .tags
            .iter()
            .all(|want| tags.iter().any(|t| eq(t, want)))
        {
            return false;
        }
        if !self.sizes.is_empty()
            && !model
                .size_class()
                .is_some_and(|s| self.sizes.iter().any(|want| eq(s, want)))
        {
            return false;
        }

        if let Some(file) = &self.file {
            let want = file.to_ascii_lowercase();
            // Accept the singular spelling used by `ana lm pull --format`.
            let want = if want == "safetensor" {
                "safetensors".to_string()
            } else {
                want
            };
            let has_file = model.quantized_files.iter().any(|f| {
                f.format.as_deref().is_some_and(|v| eq(v, &want))
                    || f.quant_method.as_deref().is_some_and(|v| eq(v, &want))
            });
            if !has_file {
                return false;
            }
        }

        true
    }
}

/// A downloadable file (GGUF quant or safetensors collection).
#[derive(Debug, Clone, Deserialize)]
pub struct CatalogFile {
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub quant_method: Option<String>,
    #[serde(default)]
    pub size_bytes: Option<u64>,
    #[serde(default)]
    pub max_ram_usage: Option<u64>,
}

/// Where and how to reach the catalog API.
#[derive(Debug, PartialEq)]
struct CatalogEndpoint {
    /// e.g. `https://api.example.com/v1/perimeters/default/catalog`
    base_url: String,
    api_key: String,
}

/// List catalog models matching `filter`.
pub async fn list_models(
    ctx: &CommandContext,
    filter: &CatalogFilter,
) -> miette::Result<Vec<CatalogModel>> {
    let endpoint = resolve_endpoint(ctx).await?;
    let query: String = filter
        .query_params()
        .iter()
        .map(|(k, v)| format!("&{}={}", k, urlencoding_component(v)))
        .collect();
    let mut models = Vec::new();

    loop {
        let url = format!(
            "{}/models?limit={}&offset={}{}",
            endpoint.base_url,
            PAGE_SIZE,
            models.len(),
            query
        );

        let page: Value = get_json(ctx, &url, &endpoint.api_key).await?;
        let result = &page["result"];
        let data: Vec<CatalogModel> = serde_json::from_value(result["data"].clone())
            .into_diagnostic()
            .wrap_err("Unexpected model catalog response")?;
        let total = result["total"].as_u64().unwrap_or(0) as usize;

        let fetched = data.len();
        models.extend(data);
        if fetched == 0 || models.len() >= total {
            break;
        }
    }

    models.retain(|m| filter.matches(m));
    Ok(models)
}

async fn get_json(ctx: &CommandContext, url: &str, api_key: &str) -> miette::Result<Value> {
    let resp = ctx
        .download_client()
        .get(url)
        .header("x-api-key", api_key)
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|e| miette!("Failed to reach the model catalog: {e}"))?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(miette!(
            help = "Re-configure your platform credentials with `ana platform configure`",
            "Model catalog rejected the platform credentials (HTTP {})",
            status.as_u16()
        ));
    }
    if !status.is_success() {
        return Err(miette!("Model catalog request failed (HTTP {})", status));
    }
    resp.json()
        .await
        .map_err(|e| miette!("Invalid model catalog response: {e}"))
}

/// Resolve the catalog endpoint from the outerbounds/metaflow config.
async fn resolve_endpoint(ctx: &CommandContext) -> miette::Result<CatalogEndpoint> {
    let config_dir = config_dir();
    let profile = std::env::var("METAFLOW_PROFILE")
        .ok()
        .filter(|p| !p.is_empty());
    let local = read_local_config(&config_dir, profile.as_deref())?;

    let api_key = local
        .get("METAFLOW_SERVICE_AUTH_KEY")
        .and_then(Value::as_str)
        .ok_or_else(not_configured)?
        .to_string();

    // The perimeter's remote config holds the API server and perimeter name.
    let config = match perimeter_config_url(&config_dir, profile.as_deref(), &local) {
        Some(url) => get_json(ctx, &url, &api_key)
            .await
            .wrap_err("Failed to load the platform perimeter config")?["config"]
            .clone(),
        None => local,
    };

    let base_url = catalog_base_url(&config).ok_or_else(|| {
        miette!(
            help = "Re-configure your platform credentials with `ana platform configure`",
            "Platform config is missing OBP_API_SERVER or OBP_PERIMETER"
        )
    })?;

    Ok(CatalogEndpoint { base_url, api_key })
}

fn not_configured() -> miette::Report {
    miette!(
        help = "Configure them with `ana platform configure <token>`",
        "Anaconda platform credentials not found in {}",
        config_dir().display()
    )
}

/// `$METAFLOW_HOME` or `~/.metaflowconfig`.
fn config_dir() -> PathBuf {
    std::env::var("METAFLOW_HOME")
        .ok()
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::paths::home_dir().join(".metaflowconfig"))
}

fn read_local_config(config_dir: &Path, profile: Option<&str>) -> miette::Result<Value> {
    let file = match profile {
        Some(p) => format!("config_{p}.json"),
        None => "config.json".to_string(),
    };
    let text = std::fs::read_to_string(config_dir.join(file)).map_err(|_| not_configured())?;
    serde_json::from_str(&text).map_err(|_| not_configured())
}

/// The perimeter-specific config URL: from `ob_config[_<profile>].json`
/// (in `$OBP_CONFIG_DIR` or the config dir), else `OBP_METAFLOW_CONFIG_URL`.
fn perimeter_config_url(config_dir: &Path, profile: Option<&str>, local: &Value) -> Option<String> {
    let ob_dir = std::env::var("OBP_CONFIG_DIR")
        .ok()
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| config_dir.to_path_buf());
    let file = match profile {
        Some(p) => format!("ob_config_{p}.json"),
        None => "ob_config.json".to_string(),
    };
    let ob_config: Option<Value> = std::fs::read_to_string(ob_dir.join(file))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());

    [
        "OB_CURRENT_PERIMETER_MF_CONFIG_URL",
        "OB_CURRENT_PERIMETER_URL",
    ]
    .iter()
    .find_map(|k| {
        ob_config
            .as_ref()?
            .get(*k)?
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    })
    .or_else(|| {
        local
            .get("OBP_METAFLOW_CONFIG_URL")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    })
}

/// `https://<OBP_API_SERVER>/v1/perimeters/<OBP_PERIMETER>/catalog`.
fn catalog_base_url(config: &Value) -> Option<String> {
    let perimeter = config.get("OBP_PERIMETER")?.as_str()?;
    let server = config
        .get("OBP_API_SERVER")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            config
                .get("OBP_INTEGRATIONS_URL")
                .and_then(Value::as_str)
                .and_then(|u| u.strip_suffix("/integrations"))
                .map(str::to_string)
        })?;
    let server = if server.starts_with("http://") || server.starts_with("https://") {
        server
    } else {
        format!("https://{server}")
    };
    Some(format!(
        "{}/v1/perimeters/{}/catalog",
        server.trim_end_matches('/'),
        perimeter
    ))
}

/// Percent-encode a query parameter value.
fn urlencoding_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_catalog_base_url() {
        let cfg = json!({ "OBP_API_SERVER": "api.example.com", "OBP_PERIMETER": "default" });
        assert_eq!(
            catalog_base_url(&cfg).unwrap(),
            "https://api.example.com/v1/perimeters/default/catalog"
        );
    }

    #[test]
    fn test_catalog_base_url_from_integrations() {
        let cfg = json!({
            "OBP_INTEGRATIONS_URL": "https://api.example.com/integrations",
            "OBP_PERIMETER": "p1",
        });
        assert_eq!(
            catalog_base_url(&cfg).unwrap(),
            "https://api.example.com/v1/perimeters/p1/catalog"
        );
    }

    #[test]
    fn test_catalog_base_url_missing() {
        assert!(catalog_base_url(&json!({ "OBP_API_SERVER": "x" })).is_none());
    }

    #[test]
    fn test_perimeter_config_url_prefers_ob_config() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("ob_config.json"),
            r#"{"OB_CURRENT_PERIMETER_MF_CONFIG_URL": "https://a/ob"}"#,
        )
        .unwrap();
        let local = json!({ "OBP_METAFLOW_CONFIG_URL": "https://a/local" });
        temp_env::with_var_unset("OBP_CONFIG_DIR", || {
            assert_eq!(
                perimeter_config_url(tmp.path(), None, &local).as_deref(),
                Some("https://a/ob")
            );
        });
    }

    #[test]
    fn test_perimeter_config_url_falls_back_to_local() {
        let tmp = tempfile::tempdir().unwrap();
        let local = json!({ "OBP_METAFLOW_CONFIG_URL": "https://a/local" });
        temp_env::with_var_unset("OBP_CONFIG_DIR", || {
            assert_eq!(
                perimeter_config_url(tmp.path(), Some("dev"), &local).as_deref(),
                Some("https://a/local")
            );
            assert_eq!(perimeter_config_url(tmp.path(), None, &json!({})), None);
        });
    }

    #[test]
    fn test_read_local_config_profile() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("config_dev.json"),
            r#"{"METAFLOW_SERVICE_AUTH_KEY": "k"}"#,
        )
        .unwrap();
        let cfg = read_local_config(tmp.path(), Some("dev")).unwrap();
        assert_eq!(cfg["METAFLOW_SERVICE_AUTH_KEY"], "k");
        assert!(read_local_config(tmp.path(), None).is_err());
    }

    #[test]
    fn test_model_parsing_and_helpers() {
        let model: CatalogModel = serde_json::from_value(json!({
            "name": "Qwen2.5-0.5B-Instruct",
            "source": { "name": "Qwen" },
            "num_parameters": 500000000u64,
            "context_window_size": 32768,
            "supports_tool_calling": true,
            "quantized_files": [
                { "format": "gguf", "quant_method": "q8_0", "size_bytes": 500 },
                { "format": "gguf", "quant_method": "q4_k_m", "size_bytes": 300 },
                { "format": "safetensors", "quant_method": "safetensors", "size_bytes": 900 },
            ],
            "unknown_field": "ignored",
        }))
        .unwrap();
        assert_eq!(model.full_name(), "Qwen/Qwen2.5-0.5B-Instruct");
        let quants: Vec<_> = model
            .gguf_files()
            .iter()
            .map(|f| f.quant_method.clone().unwrap())
            .collect();
        assert_eq!(quants, vec!["q4_k_m", "q8_0"]);
        assert!(model.has_non_gguf());
    }

    fn sample_model() -> CatalogModel {
        serde_json::from_value(json!({
            "name": "Qwen2.5-0.5B-Instruct",
            "source": { "name": "Qwen" },
            "trained_for": "text-generation",
            "tags": [
                { "id": 3, "name": "chat" },
                { "id": 5, "name": "tool-calling" },
                { "id": 42, "name": "size-tiny" },
            ],
            "quantized_files": [
                { "format": "gguf", "quant_method": "q4_k_m" },
                { "format": "gguf", "quant_method": "q8_0" },
            ],
        }))
        .unwrap()
    }

    #[test]
    fn test_model_tags_and_size() {
        let m = sample_model();
        assert_eq!(m.publisher(), Some("Qwen"));
        assert_eq!(m.tag_names(), vec!["chat", "tool-calling", "size-tiny"]);
        assert_eq!(m.size_class(), Some("tiny"));
    }

    #[test]
    fn test_filter_empty_matches_everything() {
        assert!(CatalogFilter::default().matches(&sample_model()));
    }

    #[test]
    fn test_filter_name_publisher_purpose() {
        let m = sample_model();
        let f =
            |name: Option<&str>, publisher: Option<&str>, purpose: Option<&str>| CatalogFilter {
                name: name.map(String::from),
                publisher: publisher.map(String::from),
                purpose: purpose.map(String::from),
                ..Default::default()
            };
        assert!(f(Some("0.5b-instruct"), None, None).matches(&m));
        assert!(f(Some("qwen/qwen2.5"), None, None).matches(&m));
        assert!(!f(Some("llama"), None, None).matches(&m));
        assert!(f(None, Some("qwen"), None).matches(&m));
        assert!(!f(None, Some("google"), None).matches(&m));
        assert!(f(None, None, Some("Text-Generation")).matches(&m));
        assert!(!f(None, None, Some("sentence-similarity")).matches(&m));
    }

    #[test]
    fn test_filter_tags_are_anded() {
        let m = sample_model();
        let tags = |t: &[&str]| CatalogFilter {
            tags: t.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        assert!(tags(&["chat"]).matches(&m));
        assert!(tags(&["chat", "TOOL-CALLING"]).matches(&m));
        assert!(!tags(&["chat", "reasoning"]).matches(&m));
    }

    #[test]
    fn test_filter_sizes_are_ored() {
        let m = sample_model();
        let sizes = |s: &[&str]| CatalogFilter {
            sizes: s.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        assert!(sizes(&["tiny"]).matches(&m));
        assert!(sizes(&["small", "tiny"]).matches(&m));
        assert!(!sizes(&["large"]).matches(&m));
    }

    #[test]
    fn test_filter_file() {
        let m = sample_model();
        let file = |f: &str| CatalogFilter {
            file: Some(f.to_string()),
            ..Default::default()
        };
        assert!(file("gguf").matches(&m));
        assert!(file("Q8_0").matches(&m));
        assert!(!file("safetensors").matches(&m));
        assert!(!file("safetensor").matches(&m));
        assert!(!file("q6_k").matches(&m));
    }

    #[test]
    fn test_filter_query_params() {
        let f = CatalogFilter {
            name: Some("qwen".to_string()),
            publisher: Some("Qwen".to_string()),
            purpose: Some("text-generation".to_string()),
            tags: vec!["chat".to_string(), "reasoning".to_string()],
            sizes: vec!["tiny".to_string()],
            file: Some("gguf".to_string()),
        };
        assert_eq!(
            f.query_params(),
            vec![
                ("search", "qwen".to_string()),
                ("source_name", "Qwen".to_string()),
                ("trained_for", "text-generation".to_string()),
                ("tags", "chat".to_string()),
            ]
        );

        // A single size narrows server-side when no tags are given.
        let one_size = CatalogFilter {
            sizes: vec!["tiny".to_string()],
            ..Default::default()
        };
        assert_eq!(
            one_size.query_params(),
            vec![("tags", "size-tiny".to_string())]
        );

        // Multiple sizes are OR'ed client-side only.
        let two_sizes = CatalogFilter {
            sizes: vec!["tiny".to_string(), "small".to_string()],
            ..Default::default()
        };
        assert!(two_sizes.query_params().is_empty());
    }

    #[test]
    fn test_urlencoding_component() {
        assert_eq!(urlencoding_component("qwen 2.5/x"), "qwen%202.5%2Fx");
    }
}
