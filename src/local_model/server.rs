//! Shared background llama-server for `ana lm run`.
//!
//! All models are served by a single llama-server in router mode. Each model
//! is a section in a presets file (`~/.ana/lm/models.ini`), named by its Kilo
//! model ID, and the router loads models on demand by the request's `model`
//! field. Server details are recorded in `~/.ana/lm/server.json` so later runs
//! can reuse the server, or restart it when the model list or settings change.

// Only used by `ana lm run`, which requires tool installation; presets are
// also cleaned up by `ana lm delete`.
#![cfg_attr(not(tool_install), allow(dead_code))]

use std::path::{Path, PathBuf};

use miette::{IntoDiagnostic, WrapErr, miette};
use serde::{Deserialize, Serialize};

/// Files used to manage the shared server.
pub struct ServerFiles {
    pub presets: PathBuf,
    pub state: PathBuf,
    pub log: PathBuf,
}

impl ServerFiles {
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            presets: dir.join("models.ini"),
            state: dir.join("server.json"),
            log: dir.join("server.log"),
        }
    }

    /// Default location: `~/.ana/lm`.
    pub fn default_location() -> Self {
        Self::in_dir(&crate::paths::ana_home().join("lm"))
    }
}

// ---------------------------------------------------------------------------
// Presets file
// ---------------------------------------------------------------------------

/// llama-server router presets: INI sections named by model ID, each holding
/// llama-server options (`model = /path/to/file.gguf`).
#[derive(Debug, Default, PartialEq)]
pub struct Presets {
    sections: Vec<(String, Vec<(String, String)>)>,
}

impl Presets {
    pub fn parse(text: &str) -> Self {
        let mut sections: Vec<(String, Vec<(String, String)>)> = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                sections.push((name.trim().to_string(), Vec::new()));
            } else if let Some((key, value)) = line.split_once('=')
                && let Some((_, values)) = sections.last_mut()
            {
                values.push((key.trim().to_string(), value.trim().to_string()));
            }
        }
        Self { sections }
    }

    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> miette::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).into_diagnostic()?;
        }
        std::fs::write(path, self.render())
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to write {}", path.display()))
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for (name, values) in &self.sections {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!("[{name}]\n"));
            for (key, value) in values {
                out.push_str(&format!("{key} = {value}\n"));
            }
        }
        out
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }

    /// Add a model, or point an existing one at `model_path`. Returns true if
    /// anything changed.
    pub fn upsert_model(&mut self, id: &str, model_path: &Path) -> bool {
        let path = model_path.to_string_lossy().into_owned();
        match self.sections.iter_mut().find(|(name, _)| name == id) {
            Some((_, values)) => match values.iter_mut().find(|(k, _)| k == "model") {
                Some((_, v)) if *v == path => false,
                Some((_, v)) => {
                    *v = path;
                    true
                }
                None => {
                    values.push(("model".to_string(), path));
                    true
                }
            },
            None => {
                self.sections
                    .push((id.to_string(), vec![("model".to_string(), path)]));
                true
            }
        }
    }

    /// Remove models by ID. Returns true if anything was removed.
    pub fn remove(&mut self, ids: &[String]) -> bool {
        let before = self.sections.len();
        self.sections.retain(|(name, _)| !ids.contains(name));
        self.sections.len() != before
    }
}

/// Remove models from the default presets file. Returns true if it changed.
pub fn remove_presets(ids: &[String]) -> miette::Result<bool> {
    let files = ServerFiles::default_location();
    let mut presets = Presets::load(&files.presets);
    if !presets.remove(ids) {
        return Ok(false);
    }
    presets.save(&files.presets)?;
    Ok(true)
}

// ---------------------------------------------------------------------------
// Server state
// ---------------------------------------------------------------------------

/// Recorded details of the running server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerState {
    pub pid: u32,
    pub host: String,
    pub port: u16,
    /// Extra arguments passed through to llama-server.
    pub args: Vec<String>,
    /// Hash of the presets file the server was started with.
    pub presets_hash: String,
}

impl ServerState {
    /// True when a server started with `self` can serve a request for `other`
    /// without restarting.
    fn same_config(&self, other: &ServerState) -> bool {
        self.host == other.host
            && self.port == other.port
            && self.args == other.args
            && self.presets_hash == other.presets_hash
    }
}

fn load_state(path: &Path) -> Option<ServerState> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_state(path: &Path, state: &ServerState) -> miette::Result<()> {
    let text = serde_json::to_string_pretty(state).into_diagnostic()?;
    std::fs::write(path, text)
        .into_diagnostic()
        .wrap_err_with(|| format!("Failed to write {}", path.display()))
}

fn hash_text(text: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

// ---------------------------------------------------------------------------
// Starting / stopping
// ---------------------------------------------------------------------------

/// How the server ended up running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerAction {
    /// An already-running server with the same configuration was reused.
    Reused,
    /// No server was running; a new one was started.
    Started,
    /// A server was running with a different configuration and was replaced.
    Restarted,
}

/// A running shared server.
#[derive(Debug)]
pub struct RunningServer {
    pub pid: u32,
    pub action: ServerAction,
    pub log: PathBuf,
}

/// Ensure the shared server is running with the current presets, host, port
/// and extra arguments, starting or restarting it as needed.
#[cfg(tool_install)]
pub async fn ensure_running(
    ctx: &crate::context::CommandContext,
    files: &ServerFiles,
    server_bin: &Path,
    host: &str,
    port: u16,
    extra_args: &[String],
) -> miette::Result<RunningServer> {
    let presets_text = std::fs::read_to_string(&files.presets).unwrap_or_default();
    let mut desired = ServerState {
        pid: 0,
        host: host.to_string(),
        port,
        args: extra_args.to_vec(),
        presets_hash: hash_text(&presets_text),
    };

    let mut action = ServerAction::Started;
    if let Some(current) = load_state(&files.state)
        && process::is_llama_server(current.pid)
    {
        if current.same_config(&desired) {
            return Ok(RunningServer {
                pid: current.pid,
                action: ServerAction::Reused,
                log: files.log.clone(),
            });
        }
        process::terminate(current.pid)?;
        action = ServerAction::Restarted;
    }

    // Fail fast if something else owns the port; otherwise the readiness
    // check could be answered by the other server.
    ensure_port_free(host, port)?;

    let mut args = vec![
        "--host".to_string(),
        host.to_string(),
        "--port".to_string(),
        port.to_string(),
        "--models-preset".to_string(),
        files.presets.to_string_lossy().into_owned(),
    ];
    args.extend(extra_args.iter().cloned());

    let mut child = process::spawn_detached(server_bin, &args, &files.log)?;
    desired.pid = child.id();
    save_state(&files.state, &desired)?;

    wait_until_ready(ctx, &mut child, host, port, &files.log).await?;

    Ok(RunningServer {
        pid: desired.pid,
        action,
        log: files.log.clone(),
    })
}

/// Poll the server's `/health` endpoint until it responds, failing early if
/// the process exits (e.g. the port is already in use).
#[cfg(tool_install)]
async fn wait_until_ready(
    ctx: &crate::context::CommandContext,
    child: &mut std::process::Child,
    host: &str,
    port: u16,
    log: &Path,
) -> miette::Result<()> {
    use std::time::{Duration, Instant};

    const TIMEOUT: Duration = Duration::from_secs(60);
    let url = format!("http://{}:{}/health", super::run::client_host(host), port);
    let client = ctx.unauthenticated_client(Duration::from_secs(2));
    let start = Instant::now();

    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(miette!(
                help = format!("Server log ({}):\n{}", log.display(), log_tail(log, 15)),
                "llama-server exited during startup ({})",
                status
            ));
        }
        if let Ok(resp) = client.get(&url).send().await
            && resp.status().is_success()
            && matches!(child.try_wait(), Ok(None))
        {
            return Ok(());
        }
        if start.elapsed() > TIMEOUT {
            return Err(miette!(
                help = format!("Check the server log: {}", log.display()),
                "llama-server did not become ready within {}s",
                TIMEOUT.as_secs()
            ));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Error if `host:port` can't be bound (another process is listening).
fn ensure_port_free(host: &str, port: u16) -> miette::Result<()> {
    let bind_host = host.trim_start_matches('[').trim_end_matches(']');
    match std::net::TcpListener::bind((bind_host, port)) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => Err(miette!(
            help = "Choose another port with --port",
            "Port {} on {} is already in use by another process",
            port,
            host
        )),
        Err(e) => Err(miette!("Cannot bind {}:{}: {}", host, port, e)),
    }
}

fn log_tail(path: &Path, lines: usize) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..]
        .iter()
        .map(|l| format!("  {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Command a user can run to stop the server.
pub fn stop_command(pid: u32) -> String {
    if cfg!(windows) {
        format!("taskkill /PID {pid} /F")
    } else {
        format!("kill {pid}")
    }
}

/// Platform process helpers.
mod process {
    use std::path::Path;
    use std::process::{Child, Command, Stdio};

    use miette::{IntoDiagnostic, WrapErr, miette};

    /// Spawn `bin` detached from the terminal, logging to `log`. The process
    /// keeps running after ana exits and isn't killed by Ctrl+C in the shell.
    pub fn spawn_detached(bin: &Path, args: &[String], log: &Path) -> miette::Result<Child> {
        if let Some(parent) = log.parent() {
            std::fs::create_dir_all(parent).into_diagnostic()?;
        }
        let out = std::fs::File::create(log)
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to create {}", log.display()))?;
        let err = out.try_clone().into_diagnostic()?;

        let mut cmd = Command::new(bin);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::from(out))
            .stderr(Stdio::from(err));

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
        }

        cmd.spawn()
            .map_err(|e| miette!("Failed to start {}: {}", bin.display(), e))
    }

    /// True when `pid` is a running llama-server process. Guards against
    /// acting on a reused PID from a stale state file.
    pub fn is_llama_server(pid: u32) -> bool {
        command_line(pid).is_some_and(|cmd| cmd.contains("llama-server"))
    }

    #[cfg(unix)]
    fn command_line(pid: u32) -> Option<String> {
        let out = Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "command="])
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    }

    #[cfg(windows)]
    fn command_line(pid: u32) -> Option<String> {
        let out = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        text.contains(&format!("\"{pid}\"")).then_some(text)
    }

    /// Stop a server and wait for it to exit.
    #[cfg(unix)]
    pub fn terminate(pid: u32) -> miette::Result<()> {
        use std::time::{Duration, Instant};

        let pid_t = pid as libc::pid_t;
        // SAFETY: sending a signal to a PID we verified is llama-server.
        unsafe { libc::kill(pid_t, libc::SIGTERM) };
        let start = Instant::now();
        while is_llama_server(pid) {
            if start.elapsed() > Duration::from_secs(10) {
                // SAFETY: as above; force-kill after the grace period.
                unsafe { libc::kill(pid_t, libc::SIGKILL) };
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }

    #[cfg(windows)]
    pub fn terminate(pid: u32) -> miette::Result<()> {
        Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output()
            .into_diagnostic()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_presets_roundtrip() {
        let text = "[a]\nmodel = /m/a.gguf\n\n[b]\nmodel = /m/with space/b.gguf\nctx-size = 4096\n";
        let presets = Presets::parse(text);
        assert_eq!(presets.render(), text);
    }

    #[test]
    fn test_presets_parse_ignores_comments() {
        let presets = Presets::parse("; comment\n# other\n[a]\nmodel = /m/a.gguf\n");
        assert_eq!(presets.render(), "[a]\nmodel = /m/a.gguf\n");
    }

    #[test]
    fn test_presets_upsert() {
        let mut presets = Presets::default();
        assert!(presets.upsert_model("a", Path::new("/m/a.gguf")));
        assert!(!presets.upsert_model("a", Path::new("/m/a.gguf")));
        assert!(presets.upsert_model("a", Path::new("/m/a2.gguf")));
        assert!(presets.upsert_model("b", Path::new("/m/b.gguf")));
        assert_eq!(
            presets.render(),
            "[a]\nmodel = /m/a2.gguf\n\n[b]\nmodel = /m/b.gguf\n"
        );
    }

    #[test]
    fn test_presets_remove() {
        let mut presets = Presets::parse("[a]\nmodel = /a\n\n[b]\nmodel = /b\n");
        assert!(!presets.remove(&["missing".to_string()]));
        assert!(presets.remove(&["a".to_string()]));
        assert_eq!(presets.render(), "[b]\nmodel = /b\n");
        assert!(presets.remove(&["b".to_string()]));
        assert!(presets.is_empty());
    }

    #[test]
    fn test_presets_save_and_load() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("lm/models.ini");
        let mut presets = Presets::default();
        presets.upsert_model("a", Path::new("/m/a.gguf"));
        presets.save(&path).unwrap();
        assert_eq!(Presets::load(&path), presets);
        assert_eq!(
            Presets::load(&tmp.path().join("missing")),
            Presets::default()
        );
    }

    #[test]
    fn test_state_roundtrip_and_same_config() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("server.json");
        let state = ServerState {
            pid: 42,
            host: "127.0.0.1".to_string(),
            port: 8080,
            args: vec!["-c".to_string(), "4096".to_string()],
            presets_hash: hash_text("[a]\n"),
        };
        save_state(&path, &state).unwrap();
        let loaded = load_state(&path).unwrap();
        assert_eq!(loaded, state);

        let mut other = state.clone();
        other.pid = 7;
        assert!(state.same_config(&other));
        other.presets_hash = hash_text("[a]\n[b]\n");
        assert!(!state.same_config(&other));
    }

    #[test]
    fn test_ensure_port_free() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let err = ensure_port_free("127.0.0.1", port).unwrap_err();
        assert!(err.to_string().contains("already in use"));
        drop(listener);
        assert!(ensure_port_free("127.0.0.1", port).is_ok());
    }

    #[test]
    fn test_stop_command() {
        if cfg!(windows) {
            assert_eq!(stop_command(42), "taskkill /PID 42 /F");
        } else {
            assert_eq!(stop_command(42), "kill 42");
        }
    }

    #[test]
    fn test_is_llama_server_false_for_self() {
        assert!(!process::is_llama_server(std::process::id()));
    }
}
