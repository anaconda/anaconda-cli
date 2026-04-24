//! API Client Code Generator
//!
//! Generates CLI commands and API client code from OpenAPI/Swagger specifications.
//!
//! Usage:
//!   cargo run --bin api-gen -- <owner/repo> [--output <dir>] [--branch <branch>]
//!
//! The tool scans the repo for swagger files in /apps/*/docs/

use reqwest::Client;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

// ============================================================================
// GitHub API Types
// ============================================================================

#[derive(Debug, Deserialize)]
struct GitHubContent {
    name: String,
    path: String,
    #[serde(rename = "type")]
    content_type: String,
    download_url: Option<String>,
}

// ============================================================================
// OpenAPI Schema Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct OpenApiSpec {
    pub openapi: Option<String>,
    pub swagger: Option<String>,
    pub info: Info,
    pub servers: Option<Vec<Server>>,
    pub paths: BTreeMap<String, PathItem>,
    pub components: Option<Components>,
}

#[derive(Debug, Deserialize)]
pub struct Info {
    pub title: String,
    pub version: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Server {
    pub url: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PathItem {
    pub get: Option<Operation>,
    pub post: Option<Operation>,
    pub put: Option<Operation>,
    pub delete: Option<Operation>,
    pub patch: Option<Operation>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub operation_id: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub parameters: Option<Vec<Parameter>>,
    pub request_body: Option<RequestBody>,
    pub responses: Option<BTreeMap<String, Response>>,
}

#[derive(Debug, Deserialize)]
pub struct Parameter {
    pub name: String,
    #[serde(rename = "in")]
    pub location: String,
    pub description: Option<String>,
    pub required: Option<bool>,
    pub schema: Option<Schema>,
}

#[derive(Debug, Deserialize)]
pub struct RequestBody {
    pub description: Option<String>,
    pub required: Option<bool>,
    pub content: Option<BTreeMap<String, MediaType>>,
}

#[derive(Debug, Deserialize)]
pub struct MediaType {
    pub schema: Option<Schema>,
}

#[derive(Debug, Deserialize)]
pub struct Response {
    pub description: Option<String>,
    pub content: Option<BTreeMap<String, MediaType>>,
}

#[derive(Debug, Deserialize)]
pub struct Components {
    pub schemas: Option<BTreeMap<String, Schema>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    #[serde(rename = "type")]
    pub schema_type: Option<String>,
    pub format: Option<String>,
    pub description: Option<String>,
    pub properties: Option<BTreeMap<String, Schema>>,
    pub required: Option<Vec<String>>,
    pub items: Option<Box<Schema>>,
    #[serde(rename = "$ref")]
    pub reference: Option<String>,
    #[serde(rename = "enum")]
    pub enum_values: Option<Vec<serde_json::Value>>,
}

// ============================================================================
// GitHub API Client
// ============================================================================

struct GitHubClient {
    client: Client,
    owner: String,
    repo: String,
    branch: String,
    token: Option<String>,
}

impl GitHubClient {
    fn new(owner: &str, repo: &str, branch: &str) -> Self {
        let token = std::env::var("GITHUB_TOKEN").ok();
        Self {
            client: Client::new(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            branch: branch.to_string(),
            token,
        }
    }

    async fn list_directory(&self, path: &str) -> Result<Vec<GitHubContent>, Box<dyn std::error::Error>> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
            self.owner, self.repo, path, self.branch
        );

        let mut request = self
            .client
            .get(&url)
            .header("User-Agent", "api-gen")
            .header("Accept", "application/vnd.github.v3+json");

        if let Some(token) = &self.token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request.send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("GitHub API error {}: {}", status, text).into());
        }

        let contents: Vec<GitHubContent> = response.json().await?;
        Ok(contents)
    }

    async fn fetch_file(&self, path: &str) -> Result<String, Box<dyn std::error::Error>> {
        // Use the API endpoint with raw media type for private repo support
        let url = format!(
            "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
            self.owner, self.repo, path, self.branch
        );

        let mut request = self
            .client
            .get(&url)
            .header("User-Agent", "api-gen")
            .header("Accept", "application/vnd.github.raw+json");

        if let Some(token) = &self.token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request.send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("Failed to fetch {}: {} - {}", path, status, text).into());
        }

        Ok(response.text().await?)
    }
}

// ============================================================================
// Swagger Discovery
// ============================================================================

struct DiscoveredSpec {
    app_name: String,
    file_path: String,
    content: String,
}

const SWAGGER_FILE_NAMES: &[&str] = &[
    "swagger.yaml",
    "swagger.yml",
    "openapi.yaml",
    "openapi.yml",
    "swagger.json",
    "openapi.json",
];

async fn discover_specs(github: &GitHubClient) -> Result<Vec<DiscoveredSpec>, Box<dyn std::error::Error>> {
    let mut specs = Vec::new();

    // List /apps directory
    eprintln!("Scanning /apps directory...");
    let apps = match github.list_directory("apps").await {
        Ok(contents) => contents,
        Err(e) => {
            eprintln!("Warning: Could not list /apps directory: {}", e);
            return Ok(specs);
        }
    };

    let app_dirs: Vec<_> = apps
        .into_iter()
        .filter(|c| c.content_type == "dir")
        .collect();

    eprintln!("Found {} apps: {}", app_dirs.len(),
        app_dirs.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "));

    // Check each app's /docs directory
    for app in app_dirs {
        let docs_path = format!("apps/{}/docs", app.name);
        eprintln!("  Checking {}/docs...", app.name);

        let docs_contents = match github.list_directory(&docs_path).await {
            Ok(contents) => contents,
            Err(_) => {
                eprintln!("    No docs directory found");
                continue;
            }
        };

        // Look for swagger files (prefer YAML over JSON, take first match only)
        let mut found_spec = false;
        for &swagger_name in SWAGGER_FILE_NAMES {
            if found_spec {
                break;
            }

            for file in &docs_contents {
                if file.content_type != "file" {
                    continue;
                }

                let file_lower = file.name.to_lowercase();
                if file_lower == swagger_name {
                    eprintln!("    Found: {}", file.name);

                    match github.fetch_file(&file.path).await {
                        Ok(content) => {
                            specs.push(DiscoveredSpec {
                                app_name: app.name.clone(),
                                file_path: file.path.clone(),
                                content,
                            });
                            found_spec = true;
                            break;
                        }
                        Err(e) => {
                            eprintln!("    Error fetching {}: {}", file.name, e);
                        }
                    }
                }
            }
        }
    }

    Ok(specs)
}

// ============================================================================
// Code Generation
// ============================================================================

#[derive(Debug)]
struct GeneratedCommand {
    name: String,
    method: String,
    path: String,
    description: Option<String>,
    parameters: Vec<GeneratedParam>,
    has_body: bool,
}

#[derive(Debug)]
struct GeneratedParam {
    name: String,
    rust_name: String,
    param_type: String,
    location: String,
    required: bool,
    description: Option<String>,
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 && !result.ends_with('_') {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        } else if c == '-' || c == ' ' || c == '.' || c == '/' {
            if !result.is_empty() && !result.ends_with('_') {
                result.push('_');
            }
        } else if c.is_alphanumeric() || c == '_' {
            result.push(c);
        }
        // Skip other special characters
    }
    result
}

fn to_pascal_case(s: &str) -> String {
    s.replace('-', "_")
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

/// Rust reserved keywords that need escaping with r#
const RUST_KEYWORDS: &[&str] = &[
    "type", "fn", "let", "mut", "ref", "self", "Self", "super", "crate",
    "const", "static", "struct", "enum", "trait", "impl", "for", "loop",
    "while", "if", "else", "match", "return", "break", "continue", "move",
    "async", "await", "dyn", "pub", "mod", "use", "extern", "unsafe", "where",
    "true", "false", "in", "as", "box", "priv", "final", "override", "abstract",
];

fn escape_keyword(name: &str) -> String {
    if RUST_KEYWORDS.contains(&name) {
        format!("r#{}", name)
    } else {
        name.to_string()
    }
}

/// Extract path parameter names from a URL path like "/users/{user_id}/posts/{post_id}"
fn extract_path_params(path: &str) -> Vec<String> {
    let mut params = Vec::new();
    let mut in_brace = false;
    let mut current_param = String::new();

    for c in path.chars() {
        if c == '{' {
            in_brace = true;
            current_param.clear();
        } else if c == '}' {
            if in_brace && !current_param.is_empty() {
                params.push(current_param.clone());
            }
            in_brace = false;
        } else if in_brace {
            current_param.push(c);
        }
    }

    params
}

fn to_kebab_case(s: &str) -> String {
    s.replace('_', "-")
}

/// Use the operation name from swagger, normalizing underscores
fn clean_command_name(name: &str) -> String {
    let mut result = to_snake_case(name);
    // Collapse multiple underscores to single
    while result.contains("__") {
        result = result.replace("__", "_");
    }
    // Remove leading/trailing underscores
    result = result.trim_matches('_').to_string();
    if result.is_empty() {
        "unnamed".to_string()
    } else {
        result
    }
}

fn schema_to_rust_type(schema: &Option<Schema>) -> String {
    match schema {
        Some(s) => match s.schema_type.as_deref() {
            Some("string") => "String".to_string(),
            Some("integer") => match s.format.as_deref() {
                Some("int64") => "i64".to_string(),
                _ => "i32".to_string(),
            },
            Some("number") => match s.format.as_deref() {
                Some("double") => "f64".to_string(),
                _ => "f32".to_string(),
            },
            Some("boolean") => "bool".to_string(),
            Some("array") => {
                let item_type = s
                    .items
                    .as_ref()
                    .map(|b| schema_to_rust_type(&Some(b.as_ref().clone())))
                    .unwrap_or_else(|| "String".to_string());
                format!("Vec<{item_type}>")
            }
            _ => "String".to_string(),
        },
        None => "String".to_string(),
    }
}

fn extract_commands(spec: &OpenApiSpec) -> Vec<GeneratedCommand> {
    let mut commands = Vec::new();
    let mut name_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for (path, path_item) in &spec.paths {
        // Skip admin endpoints
        if path.contains("/admin") || path.starts_with("admin") {
            continue;
        }

        let methods = [
            ("get", &path_item.get),
            ("post", &path_item.post),
            ("put", &path_item.put),
            ("delete", &path_item.delete),
            ("patch", &path_item.patch),
        ];

        for (method, operation) in methods {
            if let Some(op) = operation {
                let name = op
                    .operation_id
                    .clone()
                    .unwrap_or_else(|| format!("{method}_{}", path.replace('/', "_")));

                let parameters: Vec<GeneratedParam> = {
                    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();
                    let mut params: Vec<GeneratedParam> = Vec::new();

                    // First, add path parameters extracted from the URL
                    let path_param_names = extract_path_params(path);
                    for param_name in &path_param_names {
                        let snake_name = to_snake_case(param_name);
                        let rust_name = escape_keyword(&snake_name);
                        if !seen_names.contains(&rust_name) {
                            seen_names.insert(rust_name.clone());
                            params.push(GeneratedParam {
                                name: param_name.clone(),
                                rust_name,
                                param_type: "String".to_string(),
                                location: "path".to_string(),
                                required: true,
                                description: None,
                            });
                        }
                    }

                    // Then add parameters from swagger spec (query, header, etc.)
                    if let Some(swagger_params) = &op.parameters {
                        for p in swagger_params {
                            let snake_name = to_snake_case(&p.name);
                            let rust_name = escape_keyword(&snake_name);
                            // Skip duplicates (path params already added)
                            if seen_names.contains(&rust_name) {
                                continue;
                            }
                            seen_names.insert(rust_name.clone());
                            params.push(GeneratedParam {
                                name: p.name.clone(),
                                rust_name,
                                param_type: schema_to_rust_type(&p.schema),
                                location: p.location.clone(),
                                required: p.required.unwrap_or(false),
                                description: p.description.clone(),
                            });
                        }
                    }

                    params
                };

                let base_name = clean_command_name(&name);

                // Handle duplicate names by appending a counter
                let count = name_counts.entry(base_name.clone()).or_insert(0);
                *count += 1;
                let final_name = if *count > 1 {
                    format!("{}_{}", base_name, count)
                } else {
                    base_name.clone()
                };

                commands.push(GeneratedCommand {
                    name: final_name,
                    method: method.to_uppercase(),
                    path: path.clone(),
                    description: op.summary.clone().or(op.description.clone()),
                    parameters,
                    has_body: op.request_body.is_some(),
                });
            }
        }
    }

    commands
}

fn generate_cli_code(app_name: &str, spec: &OpenApiSpec, commands: &[GeneratedCommand]) -> String {
    let mut code = String::new();
    let module_name = to_snake_case(app_name);
    let struct_prefix = to_pascal_case(app_name);

    // Header
    code.push_str(&format!(
        r#"//! Generated API client for {} ({})
//! Version: {}
//!
//! This file was auto-generated by api-gen. Do not edit manually.

use clap::{{Parser, Subcommand}};
use crate::auth::ApiClient;

"#,
        spec.info.title, app_name, spec.info.version
    ));

    // CLI struct
    code.push_str(&format!(
        r#"#[derive(Parser)]
#[command(name = "{module_name}")]
pub struct {struct_prefix}Cli {{
    #[command(subcommand)]
    pub command: {struct_prefix}Commands,
}}

#[derive(Subcommand)]
pub enum {struct_prefix}Commands {{
"#
    ));

    // Generate enum variants
    for cmd in commands {
        let variant_name = to_pascal_case(&cmd.name);
        let kebab_name = to_kebab_case(&cmd.name);
        if let Some(desc) = &cmd.description {
            code.push_str(&format!("    /// {}\n", desc.replace('\n', " ")));
        }
        code.push_str(&format!("    #[command(name = \"{kebab_name}\")]\n"));
        code.push_str(&format!("    {variant_name}({variant_name}Args),\n"));
    }

    code.push_str("}\n\n");

    // Generate args structs
    for cmd in commands {
        let struct_name = format!("{}Args", to_pascal_case(&cmd.name));

        code.push_str("#[derive(Parser)]\n");
        code.push_str(&format!("pub struct {struct_name} {{\n"));

        for param in &cmd.parameters {
            if let Some(desc) = &param.description {
                code.push_str(&format!("    /// {}\n", desc.replace('\n', " ")));
            }

            let type_str = if param.required {
                param.param_type.clone()
            } else {
                format!("Option<{}>", param.param_type)
            };

            if !param.required {
                code.push_str("    #[arg(long)]\n");
            }

            code.push_str(&format!("    pub {}: {},\n", param.rust_name, type_str));
        }

        if cmd.has_body {
            code.push_str("    /// Request body as JSON\n");
            code.push_str("    #[arg(long)]\n");
            code.push_str("    pub json: Option<String>,\n");
        }

        code.push_str("}\n\n");
    }

    // Generate API client struct
    code.push_str(&format!(
        r#"pub struct {struct_prefix}Client<'a> {{
    client: &'a ApiClient,
}}

impl<'a> {struct_prefix}Client<'a> {{
    pub fn new(client: &'a ApiClient) -> Self {{
        Self {{ client }}
    }}

"#
    ));

    // Generate methods for each command
    for cmd in commands {
        let method_name = &cmd.name;
        let http_method = cmd.method.to_lowercase();

        // Extract path parameters from URL
        let path_param_names: Vec<String> = extract_path_params(&cmd.path);
        let mut seen_params: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Build function signature - start with path params from URL
        let mut params: Vec<String> = vec!["&self".to_string()];
        for param_name in &path_param_names {
            let rust_name = escape_keyword(&to_snake_case(param_name));
            if !seen_params.contains(&rust_name) {
                seen_params.insert(rust_name.clone());
                params.push(format!("{}: String", rust_name));
            }
        }

        // Add other parameters (query, header, etc.)
        for param in &cmd.parameters {
            if seen_params.contains(&param.rust_name) {
                continue; // Skip duplicates
            }
            seen_params.insert(param.rust_name.clone());
            let type_str = if param.required {
                format!("{}: {}", param.rust_name, param.param_type)
            } else {
                format!("{}: Option<{}>", param.rust_name, param.param_type)
            };
            params.push(type_str);
        }
        if cmd.has_body {
            params.push("json: Option<serde_json::Value>".to_string());
        }

        code.push_str(&format!(
            "    pub async fn {method_name}({}) -> Result<serde_json::Value, reqwest_middleware::Error> {{\n",
            params.join(", ")
        ));

        // Extract path parameters directly from the URL pattern
        let path_param_names: Vec<String> = extract_path_params(&cmd.path);

        if path_param_names.is_empty() {
            code.push_str(&format!("        let url = \"{}\";\n", cmd.path));
        } else {
            let mut url_expr = format!("let url = format!(\"{}\"", cmd.path);
            for param_name in &path_param_names {
                let rust_name = escape_keyword(&to_snake_case(param_name));
                url_expr.push_str(&format!(", {} = {}", param_name, rust_name));
            }
            url_expr.push_str(");");
            code.push_str(&format!("        {url_expr}\n"));
        }

        // Add query parameters
        let query_params: Vec<&GeneratedParam> = cmd
            .parameters
            .iter()
            .filter(|p| p.location == "query")
            .collect();

        let needs_mut = !query_params.is_empty() || cmd.has_body;

        // Start building request
        if needs_mut {
            code.push_str(&format!(
                "        let mut request = self.client.{http_method}(&url);\n"
            ));
        } else {
            code.push_str(&format!(
                "        let request = self.client.{http_method}(&url);\n"
            ));
        }

        for param in query_params {
            if param.required {
                code.push_str(&format!(
                    "        request = request.query(&[(\"{}\", &{})]);\n",
                    param.name, param.rust_name
                ));
            } else {
                code.push_str(&format!(
                    "        if let Some(v) = &{} {{ request = request.query(&[(\"{}\", v)]); }}\n",
                    param.rust_name, param.name
                ));
            }
        }

        // Add JSON body if present
        if cmd.has_body {
            code.push_str("        if let Some(j) = json { request = request.json(&j); }\n");
        }

        code.push_str("        let response = request.send().await?;\n");
        code.push_str("        let text = response.text().await?;\n");
        code.push_str("        Ok(serde_json::from_str(&text).unwrap_or_else(|_| serde_json::Value::String(text)))\n");
        code.push_str("    }\n\n");
    }

    code.push_str("}\n");

    code
}

// ============================================================================
// Module Generation
// ============================================================================

fn generate_mod_rs(modules: &[(String, String)]) -> String {
    let mut code = String::new();

    code.push_str("//! Generated API clients\n");
    code.push_str("//!\n");
    code.push_str("//! This file was auto-generated by api-gen. Do not edit manually.\n");
    code.push_str("//!\n");
    code.push_str("//! Usage: api <service> <command> [args]\n");
    code.push_str("//!\n");
    code.push_str("//! Example:\n");
    code.push_str("//!   api auth login --username foo --password bar\n");
    code.push_str("//!   api catalogs get-catalog-by-cid --id abc123\n\n");

    code.push_str("use clap::{Parser, Subcommand};\n\n");

    // Module declarations
    for (module_name, api_title) in modules {
        code.push_str(&format!("/// {}\n", api_title));
        code.push_str(&format!("pub mod {};\n", module_name));
    }

    code.push_str("\n");

    // Re-export clients for convenience
    code.push_str("// Re-export clients\n");
    for (module_name, _) in modules {
        let client_name = format!("{}Client", to_pascal_case(module_name));
        code.push_str(&format!("pub use {}::{};\n", module_name, client_name));
    }

    code.push_str("\n");

    // Generate unified API CLI
    code.push_str("/// Unified API CLI\n");
    code.push_str("#[derive(Parser)]\n");
    code.push_str("#[command(name = \"api\", about = \"API client for platform services\")]\n");
    code.push_str("pub struct ApiCli {\n");
    code.push_str("    #[command(subcommand)]\n");
    code.push_str("    pub command: ApiCommands,\n");
    code.push_str("}\n\n");

    // Generate ApiCommands enum with all services
    code.push_str("#[derive(Subcommand)]\n");
    code.push_str("pub enum ApiCommands {\n");

    for (module_name, api_title) in modules {
        let variant_name = to_pascal_case(module_name);
        let kebab_name = to_kebab_case(module_name);
        let commands_type = format!("{}Commands", variant_name);

        code.push_str(&format!("    /// {}\n", api_title));
        code.push_str(&format!("    #[command(name = \"{}\")]\n", kebab_name));
        code.push_str(&format!("    {variant_name} {{\n"));
        code.push_str("        #[command(subcommand)]\n");
        code.push_str(&format!("        command: {}::{},\n", module_name, commands_type));
        code.push_str("    },\n");
    }

    code.push_str("}\n");

    code
}

// ============================================================================
// API Executor Generation
// ============================================================================

/// Info needed to generate api.rs
struct ModuleInfo {
    module_name: String,
    api_title: String,
    commands: Vec<GeneratedCommand>,
}

fn generate_api_rs(modules: &[ModuleInfo]) -> String {
    let mut code = String::new();

    // Header
    code.push_str("//! API command execution\n");
    code.push_str("//!\n");
    code.push_str("//! This file was auto-generated by api-gen. Do not edit manually.\n");
    code.push_str("//!\n");
    code.push_str("//! Executes generated API commands against platform services.\n\n");

    // Imports
    code.push_str("use crate::auth::ApiClient;\n");
    code.push_str("use crate::config::Config;\n");
    code.push_str("use crate::context::CommandContext;\n");
    code.push_str("use crate::generated::ApiCommands;\n");

    // Import each module
    for m in modules {
        code.push_str(&format!("use crate::generated::{};\n", m.module_name));
    }
    code.push_str("\n");

    // Main execute function
    code.push_str(r#"/// Execute an API command
pub async fn execute(
    _ctx: &mut CommandContext,
    command: ApiCommands,
) -> Result<(), Box<dyn std::error::Error>> {
    let result = execute_command(command).await?;

    // Try to pretty print as JSON, otherwise print as-is
    match serde_json::to_string_pretty(&result) {
        Ok(formatted) => println!("{}", formatted),
        Err(_) => println!("{}", result),
    }
    Ok(())
}

"#);

    // execute_command function
    code.push_str("async fn execute_command(\n");
    code.push_str("    command: ApiCommands,\n");
    code.push_str(") -> Result<serde_json::Value, Box<dyn std::error::Error>> {\n");
    code.push_str("    match command {\n");

    for m in modules {
        let variant_name = to_pascal_case(&m.module_name);
        let func_name = format!("execute_{}", m.module_name);
        code.push_str(&format!(
            "        ApiCommands::{} {{ command }} => {}(command).await,\n",
            variant_name, func_name
        ));
    }

    code.push_str("    }\n");
    code.push_str("}\n\n");

    // Generate execute function for each module
    for m in modules {
        let module_name = &m.module_name;
        let struct_prefix = to_pascal_case(module_name);
        let commands_type = format!("{}::{}Commands", module_name, struct_prefix);
        let client_type = format!("{}::{}Client", module_name, struct_prefix);
        let func_name = format!("execute_{}", module_name);
        // Convert module name to kebab-case for URL (e.g., gateway_analytics -> gateway-analytics)
        let service_name = to_kebab_case(module_name);

        code.push_str(&format!(
            "async fn {}(\n    command: {},\n) -> Result<serde_json::Value, Box<dyn std::error::Error>> {{\n",
            func_name, commands_type
        ));
        code.push_str("    let config = Config::load();\n");
        code.push_str(&format!("    let base_url = format!(\"{{}}/api/{}\", config.base_url());\n", service_name));
        code.push_str("    let client = ApiClient::with_base_url(&config, &base_url)?;\n");
        code.push_str(&format!("    let api = {}::new(&client);\n\n", client_type));
        code.push_str("    match command {\n");

        for cmd in &m.commands {
            let variant_name = to_pascal_case(&cmd.name);
            let method_name = &cmd.name;

            // Build the argument list for the API call
            let path_params: Vec<String> = extract_path_params(&cmd.path);

            // Determine if we need to destructure args
            let has_args = !path_params.is_empty() || !cmd.parameters.is_empty() || cmd.has_body;

            if has_args {
                code.push_str(&format!(
                    "        {}::{}(args) => {{\n",
                    commands_type, variant_name
                ));

                // Build API call arguments
                let mut call_args: Vec<String> = Vec::new();
                let mut seen_args: std::collections::HashSet<String> = std::collections::HashSet::new();

                // Path params first
                for param_name in &path_params {
                    let rust_name = escape_keyword(&to_snake_case(param_name));
                    if !seen_args.contains(&rust_name) {
                        seen_args.insert(rust_name.clone());
                        call_args.push(format!("args.{}", rust_name));
                    }
                }

                // Other params
                for param in &cmd.parameters {
                    if !seen_args.contains(&param.rust_name) {
                        seen_args.insert(param.rust_name.clone());
                        call_args.push(format!("args.{}", param.rust_name));
                    }
                }

                // JSON body
                if cmd.has_body {
                    code.push_str("            let json = args.json.map(|s| serde_json::from_str(&s)).transpose()?;\n");
                    call_args.push("json".to_string());
                }

                code.push_str(&format!(
                    "            Ok(api.{}({}).await?)\n",
                    method_name,
                    call_args.join(", ")
                ));
                code.push_str("        }\n");
            } else {
                code.push_str(&format!(
                    "        {}::{}(_) => Ok(api.{}().await?),\n",
                    commands_type, variant_name, method_name
                ));
            }
        }

        code.push_str("    }\n");
        code.push_str("}\n\n");
    }

    code
}

// ============================================================================
// Main
// ============================================================================

fn print_usage() {
    eprintln!("Usage: api-gen <owner/repo> [--output <dir>] [--branch <branch>]");
    eprintln!();
    eprintln!("Scans a GitHub repository for OpenAPI specs in /apps/*/docs/");
    eprintln!("and generates Rust CLI code for each discovered API.");
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  <owner/repo>     GitHub repository (e.g., anaconda/my-api)");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --output <dir>   Output directory (default: stdout)");
    eprintln!("  --branch <name>  Git branch to use (default: main)");
    eprintln!();
    eprintln!("Environment:");
    eprintln!("  GITHUB_TOKEN     Optional token for private repos");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        print_usage();
        std::process::exit(if args.len() < 2 { 1 } else { 0 });
    }

    let repo_arg = &args[1];
    let output_dir = args
        .iter()
        .position(|a| a == "--output")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);

    let branch = args
        .iter()
        .position(|a| a == "--branch")
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
        .unwrap_or("main");

    // Parse owner/repo
    let parts: Vec<&str> = repo_arg.split('/').collect();
    if parts.len() != 2 {
        eprintln!("Error: Invalid repository format. Expected 'owner/repo'");
        std::process::exit(1);
    }
    let (owner, repo) = (parts[0], parts[1]);

    eprintln!("Repository: {}/{} (branch: {})", owner, repo, branch);

    let github = GitHubClient::new(owner, repo, branch);

    // Discover swagger specs
    let specs = discover_specs(&github).await?;

    if specs.is_empty() {
        eprintln!("No swagger files found in /apps/*/docs/");
        std::process::exit(1);
    }

    eprintln!("\nFound {} API specs", specs.len());

    // Clean up old generated files if outputting to directory
    if let Some(dir) = &output_dir {
        if dir.exists() {
            eprintln!("\nCleaning old generated files...");
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().map(|e| e == "rs").unwrap_or(false) {
                    fs::remove_file(&path)?;
                    eprintln!("  Removed: {}", path.display());
                }
            }
        }
        // Also remove api.rs from parent directory
        let api_path = dir.parent().unwrap_or(dir).join("api.rs");
        if api_path.exists() {
            fs::remove_file(&api_path)?;
            eprintln!("  Removed: {}", api_path.display());
        }
    }

    // Track generated modules with their commands
    let mut module_infos: Vec<ModuleInfo> = Vec::new();

    // Process each spec
    for discovered in &specs {
        eprintln!("\nProcessing: {} ({})", discovered.app_name, discovered.file_path);

        let spec: OpenApiSpec = if discovered.file_path.ends_with(".json") {
            serde_json::from_str(&discovered.content)?
        } else {
            serde_yaml::from_str(&discovered.content)?
        };

        eprintln!("  API: {} v{}", spec.info.title, spec.info.version);

        let commands = extract_commands(&spec);
        eprintln!("  Endpoints: {}", commands.len());

        for cmd in &commands {
            eprintln!("    {} {} -> {}()", cmd.method, cmd.path, cmd.name);
        }

        let code = generate_cli_code(&discovered.app_name, &spec, &commands);
        let module_name = to_snake_case(&discovered.app_name);

        // Output
        match &output_dir {
            Some(dir) => {
                fs::create_dir_all(dir)?;
                let file_name = format!("{}.rs", &module_name);
                let output_path = dir.join(&file_name);
                let mut file = fs::File::create(&output_path)?;
                file.write_all(code.as_bytes())?;
                eprintln!("  Generated: {}", output_path.display());
                module_infos.push(ModuleInfo {
                    module_name,
                    api_title: spec.info.title.clone(),
                    commands,
                });
            }
            None => {
                println!("// ============================================================================");
                println!("// {} ({})", discovered.app_name, discovered.file_path);
                println!("// ============================================================================\n");
                println!("{code}");
            }
        }
    }

    // Generate mod.rs and api.rs if outputting to directory
    if let Some(dir) = &output_dir {
        if !module_infos.is_empty() {
            // Generate mod.rs
            let modules_for_mod: Vec<(String, String)> = module_infos
                .iter()
                .map(|m| (m.module_name.clone(), m.api_title.clone()))
                .collect();
            let mod_rs = generate_mod_rs(&modules_for_mod);
            let mod_path = dir.join("mod.rs");
            let mut file = fs::File::create(&mod_path)?;
            file.write_all(mod_rs.as_bytes())?;
            eprintln!("\nGenerated: {}", mod_path.display());

            // Generate api.rs in parent directory (src/)
            let api_rs = generate_api_rs(&module_infos);
            let api_path = dir.parent().unwrap_or(dir).join("api.rs");
            let mut file = fs::File::create(&api_path)?;
            file.write_all(api_rs.as_bytes())?;
            eprintln!("Generated: {}", api_path.display());
        }
    }

    Ok(())
}
