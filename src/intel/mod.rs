//! `ana intel`: Anaconda package intelligence for agents and people.
//!
//! Read-only lookups ported from bigbend's remote MCP service
//! (`apps/remote-mcp`), exposed as plain commands that agents run in a shell.
//! Output is JSON when stdout is not a terminal (or with `--json`) and
//! readable text otherwise; see `output.rs`.

mod commands;
mod errors;
mod output;
mod package_info;
mod render;
mod schemas;
mod search_forum;

use std::io::IsTerminal;
use std::time::Duration;

pub use commands::IntelCommands;

use crate::auth;
use crate::config::Config;
use crate::context::CommandContext;
use output::OutputMode;

/// Per-request timeout for backend calls. Matches the remote service.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// True when an API key is stored for the configured domain.
fn has_credentials(config: &Config) -> bool {
    matches!(auth::get_api_key(config), Ok(Some(_)))
}

/// Run an `ana intel` subcommand.
///
/// These commands bypass the central login gate (`Action::requires_login`):
/// its prompt writes to stdout and starts an interactive device flow, which
/// would corrupt JSON and hang an agent. Instead, a person at a terminal gets
/// the usual login prompt here, and everyone else gets an `unauthenticated`
/// error from the command itself.
pub async fn run(ctx: &mut CommandContext, command: IntelCommands) -> miette::Result<()> {
    let mode = OutputMode::detect(command.force_json());
    match command {
        IntelCommands::PackageInfo {
            name,
            version,
            channel,
            platform,
            python_version,
            ..
        } => {
            if mode == OutputMode::Text
                && std::io::stdin().is_terminal()
                && !has_credentials(&ctx.config)
            {
                auth::ensure_logged_in(ctx).await?;
            }
            let query = package_info::PackageQuery {
                // clap requires the name unless `--help` was given, and help
                // never reaches `run`. An empty name would still be rejected
                // by the request-path guard.
                name: name.unwrap_or_default(),
                version,
                channel,
                platform,
                python_version,
            };
            let result = package_info::package_info(ctx, &query).await;
            output::emit(mode, result, render::package_info)
        }
        IntelCommands::SearchForum {
            query,
            page,
            page_size,
            ..
        } => {
            let result = search_forum::search_forum(ctx, &query.join(" "), page, page_size).await;
            output::emit(mode, result, render::search_forum)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::IntelError;
    use crate::http::Client;
    use serde_json::{Value, json};
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Context pointing at the mock server, with an isolated keyring.
    fn test_ctx(server: &MockServer, logged_in: bool) -> (CommandContext, tempfile::TempDir) {
        test_ctx_with_client(server, logged_in, server.uri())
    }

    /// Same, but the authenticated client points at `client_url`.
    fn test_ctx_with_client(
        server: &MockServer,
        logged_in: bool,
        client_url: String,
    ) -> (CommandContext, tempfile::TempDir) {
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
        let client = Client::new(reqwest::Client::builder(), client_url).unwrap();
        (CommandContext::with_client(config, client), dir)
    }

    /// A URL whose port was free a moment ago, so connections are refused.
    fn closed_url() -> String {
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", closed.local_addr().unwrap());
        drop(closed);
        url
    }

    async fn info(ctx: &CommandContext, name: &str, channel: Option<&str>) -> Value {
        let query = package_info::PackageQuery {
            name: name.into(),
            version: None,
            channel: channel.map(String::from),
            platform: None,
            python_version: None,
        };
        match package_info::package_info(ctx, &query).await {
            Ok(response) => serde_json::to_value(response).unwrap(),
            Err(error) => serde_json::to_value(error).unwrap(),
        }
    }

    async fn forum(ctx: &CommandContext, query: &str, page_size: i64) -> Value {
        match search_forum::search_forum(ctx, query, 1, page_size).await {
            Ok(response) => serde_json::to_value(response).unwrap(),
            Err(error) => serde_json::to_value(error).unwrap(),
        }
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

        let (ctx, _dir) = test_ctx(&server, true);
        let value = info(&ctx, "numpy", None).await;
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
        let (ctx, _dir) = test_ctx(&server, false);
        assert_eq!(info(&ctx, "numpy", None).await["error"], "unauthenticated");
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn package_info_maps_401_to_unauthenticated() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let (ctx, _dir) = test_ctx(&server, true);
        assert_eq!(info(&ctx, "numpy", None).await["error"], "unauthenticated");
    }

    #[tokio::test]
    async fn package_info_unreachable_is_search_unavailable() {
        let server = MockServer::start().await;
        let (ctx, _dir) = test_ctx_with_client(&server, true, closed_url());
        assert_eq!(
            info(&ctx, "numpy", None).await["error"],
            "search_unavailable"
        );
        // A named channel is not misreported as missing during an outage.
        assert_eq!(
            info(&ctx, "numpy", Some("main")).await["error"],
            "search_unavailable"
        );
    }

    #[tokio::test]
    async fn package_info_guards_request_paths() {
        // clap rejects these first; the guard keeps URLs safe for any caller.
        let server = MockServer::start().await;
        let (ctx, _dir) = test_ctx(&server, true);
        assert_eq!(info(&ctx, "../etc", None).await["error"], "invalid_request");
        assert_eq!(
            info(&ctx, "numpy", Some("../x")).await["error"],
            "invalid_request"
        );
        assert!(server.received_requests().await.unwrap().is_empty());
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

    async fn mount_not_found(server: &MockServer) {
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
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
    async fn package_info_fetches_all_files_when_paged() {
        let server = MockServer::start().await;
        mount_package(&server, "pytorch", Some("2.5.0")).await;
        let files = "/api/ce/packages/main/artifacts/conda/pytorch/files";
        Mock::given(method("GET"))
            .and(path(files))
            .and(query_param("limit", "200"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"total_count": 201, "items": []})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(files))
            .and(query_param("limit", "201"))
            .respond_with(one_file_page(201, "linux-64"))
            .expect(1)
            .mount(&server)
            .await;
        mount_not_found(&server).await;

        let (ctx, _dir) = test_ctx(&server, true);
        assert_eq!(
            info(&ctx, "pytorch", None).await["platforms"],
            json!(["linux-64"])
        );
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
        mount_not_found(&server).await;

        let (ctx, _dir) = test_ctx(&server, true);
        let value = info(&ctx, "pytorch", None).await;
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
        mount_not_found(&server).await;

        let (ctx, _dir) = test_ctx(&server, true);
        assert_eq!(
            info(&ctx, "pytorch", None).await["platforms"],
            json!(["linux-64"])
        );
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
        mount_not_found(&server).await;

        let (ctx, _dir) = test_ctx(&server, true);
        assert_eq!(
            info(&ctx, "mystery", None).await["platforms"],
            json!(["linux-64"])
        );
        let requests = server.received_requests().await.unwrap();
        assert!(
            !requests.iter().any(
                |r| r.url.path() == files && r.url.query().unwrap_or("").contains("limit=5000")
            ),
            "follow-up request must not be sent without a version"
        );
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

        let (ctx, _dir) = test_ctx(&server, false);
        let value = forum(&ctx, "solver error", 10).await;
        assert_eq!(value["total"], 1);
        assert_eq!(
            value["topics"][0]["url"],
            "https://forum.anaconda.com/t/solver-error/7"
        );
        assert_eq!(value["pagination"]["total_pages"], 1);
        // The API key is never sent to the forum search endpoint.
        let request = &server.received_requests().await.unwrap()[0];
        assert!(!request.headers.contains_key("authorization"));
    }

    #[tokio::test]
    async fn search_forum_http_error_is_search_unavailable() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/search/forum"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let (ctx, _dir) = test_ctx(&server, false);
        assert_eq!(forum(&ctx, "x", 10).await["error"], "search_unavailable");
    }

    #[tokio::test]
    async fn search_forum_invalid_paging_is_invalid_request() {
        let server = MockServer::start().await;
        let (ctx, _dir) = test_ctx(&server, false);
        assert_eq!(forum(&ctx, "x", 500).await["error"], "invalid_request");
    }

    // `run` under `cargo test`: stdout is captured (not a terminal), so this
    // exercises the JSON path agents use.

    #[tokio::test]
    async fn run_reports_json_errors_without_prompting() {
        let server = MockServer::start().await;
        let (mut ctx, _dir) = test_ctx(&server, false);
        let command = IntelCommands::PackageInfo {
            name: Some("numpy".into()),
            version: None,
            channel: None,
            platform: None,
            python_version: None,
            json: false,
        };
        let error = run(&mut ctx, command).await.unwrap_err();
        assert!(matches!(
            error.downcast_ref::<IntelError>(),
            Some(IntelError::Reported(message)) if message.starts_with("unauthenticated")
        ));
    }

    #[tokio::test]
    async fn run_joins_query_words() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/search/forum"))
            .and(query_param("query", "solver error"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"total": 0, "topics": []})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let (mut ctx, _dir) = test_ctx(&server, false);
        let command = IntelCommands::SearchForum {
            query: vec!["solver".into(), "error".into()],
            page: 1,
            page_size: 10,
            json: true,
        };
        run(&mut ctx, command).await.unwrap();
    }
}
