//! `ana mcp serve`: a local, read-only Anaconda MCP server over stdio.
//!
//! Replaces the remote `https://anaconda.com/api/mcp` service. Tools call
//! Anaconda's public APIs with the user's stored API key; nothing is
//! installed and no environment is modified.
//!
//! stdout carries the JSON-RPC protocol. Nothing else may write to it while
//! the server runs: logs go to stderr (see `cli::execute`).

mod package_info;
mod schemas;
mod search_forum;
mod state;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::CommandContext;
use crate::errors::McpError;
use schemas::ToolError;
use state::ServerState;

pub const SERVER_NAME: &str = "anaconda-mcp";

pub const INSTRUCTIONS: &str = "\
Anaconda Package Intelligence: read-only tools for conda ecosystem context.
Start with package_info for any package question.
Use search_forum when someone hit an error others may have solved, or wants
community experience rather than official documentation.
If a tool returns `unauthenticated`, ask the user to run `ana login` in a
terminal, then retry.
These tools neither install packages nor modify environments.";

/// Arguments for `package_info`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct PackageInfoArgs {
    /// Exact package name, e.g. 'numpy'.
    pub name: String,
    /// Exact version, e.g. '2.5.1'. Omit for latest.
    #[serde(default)]
    pub version: Option<String>,
    /// Channel name, e.g. 'main'. Omit to search all default channels.
    #[serde(default)]
    pub channel: Option<String>,
    /// Platform subdir, e.g. 'linux-64' or 'osx-arm64'. Omit for all platforms.
    #[serde(default)]
    pub platform: Option<String>,
    /// Python version, e.g. '3.12'. Adds a compatibility note but does not filter results.
    #[serde(default)]
    pub python_version: Option<String>,
}

fn default_page() -> i64 {
    1
}

fn default_page_size() -> i64 {
    10
}

/// Arguments for `search_forum`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchForumArgs {
    /// Free-text forum search; error messages and symptom phrases work well.
    pub query: String,
    /// Page of results to return, starting at 1.
    #[serde(default = "default_page")]
    pub page: i64,
    /// Threads per page (1-100); oversized values are rejected, not clamped.
    /// Page and page_size cannot pass the 10000-result window.
    #[serde(default = "default_page_size")]
    pub page_size: i64,
}

/// Convert a tool outcome into an MCP result. Errors are tool-level
/// (`isError: true`) so the agent sees the structured code and message.
fn to_call_result<T: Serialize>(result: Result<T, ToolError>) -> CallToolResult {
    let (body, is_error) = match result {
        Ok(value) => (serde_json::to_string(&value), false),
        Err(error) => (serde_json::to_string(&error), true),
    };
    match (body, is_error) {
        (Ok(json), false) => CallToolResult::success(vec![ContentBlock::text(json)]),
        (Ok(json), true) => CallToolResult::error(vec![ContentBlock::text(json)]),
        (Err(e), _) => {
            tracing::error!(error = %e, "failed to serialize tool result");
            let fallback = ToolError::new(
                "internal_error",
                "The tool result could not be serialized. Try again later.",
            );
            CallToolResult::error(vec![ContentBlock::text(
                serde_json::to_string(&fallback).unwrap_or_default(),
            )])
        }
    }
}

#[derive(Clone)]
pub struct AnacondaMcp {
    state: ServerState,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl AnacondaMcp {
    pub fn new(state: ServerState) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Get pre-install intelligence for a specific package: versions, platforms, \
                       dependency constraints, build variants, and CVE summary. Use before \
                       installing complex packages to check compatibility and risk.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn package_info(&self, Parameters(args): Parameters<PackageInfoArgs>) -> CallToolResult {
        to_call_result(
            package_info::package_info(
                &self.state,
                &args.name,
                args.version.as_deref(),
                args.channel.as_deref(),
                args.platform.as_deref(),
                args.python_version.as_deref(),
            )
            .await,
        )
    }

    #[tool(
        description = "Search the Anaconda community forum for troubleshooting threads, \
                       workarounds, and discussions. Use when the user hit an error others may \
                       have solved or wants community experience with a tool or workflow.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn search_forum(&self, Parameters(args): Parameters<SearchForumArgs>) -> CallToolResult {
        to_call_result(
            search_forum::search_forum(&self.state, &args.query, args.page, args.page_size).await,
        )
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AnacondaMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(SERVER_NAME, crate::VERSION))
            .with_instructions(INSTRUCTIONS)
    }
}

/// Run the MCP server on stdin/stdout until the client disconnects.
pub async fn serve(ctx: &CommandContext) -> miette::Result<()> {
    let server = AnacondaMcp::new(ServerState::from_context(ctx));
    let service = server
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| McpError::Serve(e.to_string()))?;
    let reason = service
        .waiting()
        .await
        .map_err(|e| McpError::Serve(e.to_string()))?;
    tracing::debug!(?reason, "MCP server stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth;
    use crate::config::Config;
    use crate::http::Client;
    use rmcp::ClientHandler;
    use rmcp::model::{CallToolRequestParams, ClientConfig};
    use serde_json::{Value, json};
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[derive(Clone, Default)]
    struct TestClient;

    impl ClientHandler for TestClient {
        fn get_info(&self) -> ClientConfig {
            ClientConfig::default()
        }
    }

    /// Config pointing at the mock server, with an isolated keyring.
    fn test_state(server: &MockServer, logged_in: bool) -> (ServerState, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::load();
        let address = server.address();
        config.domain = format!("{}:{}", address.ip(), address.port());
        config.use_https = false;
        config.keyring_path = dir.path().join("keyring");
        config.forum_base_url = "https://forum.anaconda.com".to_string();
        if logged_in {
            auth::save_credential(&config, "test-api-key", None, None).unwrap();
        }
        let client = Client::new(reqwest::Client::builder(), server.uri()).unwrap();
        let state = ServerState {
            client: client.clone(),
            public_client: client,
            config,
        };
        (state, dir)
    }

    async fn call(state: ServerState, tool: &str, arguments: Value) -> CallToolResult {
        let (server_io, client_io) = tokio::io::duplex(64 * 1024);
        let server = tokio::spawn(async move {
            AnacondaMcp::new(state)
                .serve(server_io)
                .await
                .unwrap()
                .waiting()
                .await
                .unwrap();
        });
        let client = TestClient.serve(client_io).await.unwrap();
        let result = client
            .call_tool(
                CallToolRequestParams::new(tool.to_string())
                    .with_arguments(arguments.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        client.cancel().await.unwrap();
        server.await.unwrap();
        result
    }

    fn body(result: &CallToolResult) -> Value {
        let text = &result.content[0].as_text().unwrap().text;
        serde_json::from_str(text).unwrap()
    }

    #[tokio::test]
    async fn lists_tools_with_contract_argument_names() {
        let server = MockServer::start().await;
        let (state, _dir) = test_state(&server, true);
        let (server_io, client_io) = tokio::io::duplex(64 * 1024);
        let handle = tokio::spawn(async move {
            AnacondaMcp::new(state)
                .serve(server_io)
                .await
                .unwrap()
                .waiting()
                .await
                .unwrap();
        });
        let client = TestClient.serve(client_io).await.unwrap();

        let info = client.peer_info().unwrap();
        assert_eq!(info.server_info.as_ref().unwrap().name, SERVER_NAME);
        assert!(
            info.instructions
                .as_deref()
                .unwrap()
                .contains("package_info")
        );

        let tools = client.list_all_tools().await.unwrap();
        let mut names: Vec<_> = tools.iter().map(|t| t.name.to_string()).collect();
        names.sort();
        assert_eq!(names, ["package_info", "search_forum"]);

        let properties = |name: &str| -> Vec<String> {
            let tool = tools.iter().find(|t| t.name == name).unwrap();
            assert_eq!(
                tool.annotations.as_ref().unwrap().read_only_hint,
                Some(true)
            );
            let mut keys: Vec<_> = tool.input_schema["properties"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect();
            keys.sort();
            keys
        };
        assert_eq!(
            properties("package_info"),
            ["channel", "name", "platform", "python_version", "version"]
        );
        assert_eq!(properties("search_forum"), ["page", "page_size", "query"]);

        client.cancel().await.unwrap();
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn package_info_end_to_end() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/ce/packages/main/artifacts/conda/numpy"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "download_count": 10,
                "available_versions": ["2.4.1"],
                "metadata": {"version": "2.4.1", "subdirs": ["linux-64"], "license": "BSD-3-Clause"},
            })))
            .mount(&server)
            .await;
        for channel in ["main-x", "msys2"] {
            Mock::given(method("GET"))
                .and(path(format!(
                    "/api/ce/packages/{channel}/artifacts/conda/numpy"
                )))
                .respond_with(ResponseTemplate::new(404))
                .mount(&server)
                .await;
        }
        Mock::given(method("GET"))
            .and(path("/api/ce/packages/main/artifacts/conda/numpy/files"))
            .and(query_param("version", "2.4.1"))
            .and(query_param("limit", "200"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total_count": 1,
                "items": [{"metadata": {"repodata_record.json": {
                    "subdir": "linux-64",
                    "build": "py312h8456e4e_0",
                    "depends": ["python >=3.12,<3.13.0a0"],
                }, "cves": []}}],
            })))
            .mount(&server)
            .await;

        let (state, _dir) = test_state(&server, true);
        let result = call(state, "package_info", json!({"name": "numpy"})).await;
        assert_eq!(result.is_error, Some(false));
        let value = body(&result);
        assert_eq!(value["name"], "numpy");
        assert_eq!(value["version"], "2.4.1");
        assert_eq!(value["channel"], "main");
        assert_eq!(value["python_versions"], json!(["3.12"]));
        assert_eq!(value["cve_summary"], json!({"count": 0, "critical": 0}));
        assert!(value["policy_status"].is_null());
        assert_eq!(value["notes"], json!([]));
    }

    #[tokio::test]
    async fn package_info_requires_login() {
        let server = MockServer::start().await;
        let (state, _dir) = test_state(&server, false);
        let result = call(state, "package_info", json!({"name": "numpy"})).await;
        assert_eq!(result.is_error, Some(true));
        assert_eq!(body(&result)["error"], "unauthenticated");
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn package_info_maps_401_to_unauthenticated() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let (state, _dir) = test_state(&server, true);
        let result = call(state, "package_info", json!({"name": "numpy"})).await;
        assert_eq!(body(&result)["error"], "unauthenticated");
    }

    #[tokio::test]
    async fn package_info_unreachable_is_search_unavailable() {
        let server = MockServer::start().await;
        let (mut state, _dir) = test_state(&server, true);
        // Point at a port that was free a moment ago, so connections are refused.
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let closed_url = format!("http://{}", closed.local_addr().unwrap());
        drop(closed);
        state.client = Client::new(reqwest::Client::builder(), closed_url).unwrap();
        let result = call(state, "package_info", json!({"name": "numpy"})).await;
        assert_eq!(body(&result)["error"], "search_unavailable");
    }

    #[tokio::test]
    async fn package_info_rejects_path_traversal() {
        let server = MockServer::start().await;
        let (state, _dir) = test_state(&server, true);
        let result = call(state, "package_info", json!({"name": "../etc"})).await;
        assert_eq!(body(&result)["error"], "invalid_request");
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn package_info_fetches_all_files_when_paged() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/ce/packages/main/artifacts/conda/pytorch"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"metadata": {"version": "2.5.0", "subdirs": []}})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/ce/packages/main/artifacts/conda/pytorch/files"))
            .and(query_param("limit", "200"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"total_count": 201, "items": []})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/ce/packages/main/artifacts/conda/pytorch/files"))
            .and(query_param("limit", "201"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total_count": 201,
                "items": [{"metadata": {"repodata_record.json": {"subdir": "linux-64"}}}],
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let (state, _dir) = test_state(&server, true);
        let result = call(state, "package_info", json!({"name": "pytorch"})).await;
        assert_eq!(body(&result)["platforms"], json!(["linux-64"]));
    }

    /// Mount a `main`-only package whose metadata has the given version, and
    /// 404 for every other channel.
    async fn mount_package(server: &MockServer, name: &str, version: Option<&str>) {
        let metadata = match version {
            Some(v) => json!({"version": v, "subdirs": []}),
            None => json!({"subdirs": []}),
        };
        Mock::given(method("GET"))
            .and(path(format!(
                "/api/ce/packages/main/artifacts/conda/{name}"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"metadata": metadata})))
            .mount(server)
            .await;
    }

    fn one_file_page(total: u64, subdir: &str) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(json!({
            "total_count": total,
            "items": [{"metadata": {"repodata_record.json": {"subdir": subdir}}}],
        }))
    }

    #[tokio::test]
    async fn package_info_caps_follow_up_files_request() {
        let server = MockServer::start().await;
        mount_package(&server, "pytorch", Some("2.5.0")).await;
        let files = "/api/ce/packages/main/artifacts/conda/pytorch/files";
        Mock::given(method("GET"))
            .and(path(files))
            .and(query_param("limit", "200"))
            .respond_with(one_file_page(5000, "linux-64"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(files))
            .and(query_param("limit", "1000"))
            .respond_with(one_file_page(5000, "osx-arm64"))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let (state, _dir) = test_state(&server, true);
        let value = body(&call(state, "package_info", json!({"name": "pytorch"})).await);
        assert_eq!(value["platforms"], json!(["osx-arm64"]));
        assert!(value["notes"].as_array().unwrap().iter().any(|n| {
            n.as_str()
                .unwrap()
                .starts_with("Build details cover 1 of 5000")
        }));
    }

    #[tokio::test]
    async fn package_info_keeps_first_page_when_follow_up_fails() {
        let server = MockServer::start().await;
        mount_package(&server, "pytorch", Some("2.5.0")).await;
        let files = "/api/ce/packages/main/artifacts/conda/pytorch/files";
        Mock::given(method("GET"))
            .and(path(files))
            .and(query_param("limit", "200"))
            .respond_with(one_file_page(300, "linux-64"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(files))
            .and(query_param("limit", "300"))
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let (state, _dir) = test_state(&server, true);
        let value = body(&call(state, "package_info", json!({"name": "pytorch"})).await);
        assert_eq!(value["platforms"], json!(["linux-64"]));
    }

    #[tokio::test]
    async fn package_info_skips_follow_up_without_version() {
        let server = MockServer::start().await;
        mount_package(&server, "mystery", None).await;
        let files = "/api/ce/packages/main/artifacts/conda/mystery/files";
        Mock::given(method("GET"))
            .and(path(files))
            .and(query_param("limit", "200"))
            .respond_with(one_file_page(5000, "linux-64"))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let (state, _dir) = test_state(&server, true);
        let value = body(&call(state, "package_info", json!({"name": "mystery"})).await);
        assert_eq!(value["platforms"], json!(["linux-64"]));
        let requests = server.received_requests().await.unwrap();
        assert!(
            !requests.iter().any(
                |r| r.url.path() == files && r.url.query().unwrap_or("").contains("limit=5000")
            ),
            "follow-up request must not be sent without a version"
        );
    }

    #[tokio::test]
    async fn package_info_explicit_channel_outage_is_search_unavailable() {
        let server = MockServer::start().await;
        let (mut state, _dir) = test_state(&server, true);
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let closed_url = format!("http://{}", closed.local_addr().unwrap());
        drop(closed);
        state.client = Client::new(reqwest::Client::builder(), closed_url).unwrap();
        let result = call(
            state,
            "package_info",
            json!({"name": "numpy", "channel": "main"}),
        )
        .await;
        assert_eq!(body(&result)["error"], "search_unavailable");
    }

    #[tokio::test]
    async fn search_forum_end_to_end_without_login() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/search/forum"))
            .and(query_param("query", "solver error"))
            .and(query_param("page", "1"))
            .and(query_param("page_size", "10"))
            .and(query_param("spell_check", "false"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total": 1,
                "topics": [{
                    "id": 7,
                    "title": "Solver error",
                    "url": "https://forum.anaconda.com/t/solver-error/7",
                    "category": "Help",
                    "posts_count": 2,
                    "reply_count": 1,
                    "views": 50,
                    "created_at": "2024-01-02T03:04:05Z",
                    "last_posted_at": null,
                    "blurbs": [],
                }],
            })))
            .mount(&server)
            .await;

        let (state, _dir) = test_state(&server, false);
        let result = call(state, "search_forum", json!({"query": "solver error"})).await;
        assert_eq!(result.is_error, Some(false));
        let value = body(&result);
        assert_eq!(value["total"], 1);
        assert_eq!(
            value["topics"][0]["url"],
            "https://forum.anaconda.com/t/solver-error/7"
        );
        assert_eq!(value["pagination"]["total_pages"], 1);
    }

    #[tokio::test]
    async fn search_forum_http_error_is_search_unavailable() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/search/forum"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let (state, _dir) = test_state(&server, false);
        let result = call(state, "search_forum", json!({"query": "x"})).await;
        assert_eq!(result.is_error, Some(true));
        assert_eq!(body(&result)["error"], "search_unavailable");
    }

    #[tokio::test]
    async fn search_forum_invalid_paging_is_tool_error() {
        let server = MockServer::start().await;
        let (state, _dir) = test_state(&server, false);
        let result = call(
            state,
            "search_forum",
            json!({"query": "x", "page_size": 500}),
        )
        .await;
        assert_eq!(body(&result)["error"], "invalid_request");
    }
}
