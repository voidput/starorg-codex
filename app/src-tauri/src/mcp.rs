//! Settings -> Experimental -> "Local MCP server" (docs/mcp.md): the app's side of the
//! `starorg-mcp` server, shared by both backends.
//!
//! Off by default and never switched on by itself: the switch is not saved, so every start
//! of the app begins with the server off. When on, a streamable-HTTP server listens on
//! `127.0.0.1` (a free port) and needs a bearer token made fresh for that start. It stops
//! when switched off and when the app exits (`crate::run`). Its requests and extraction
//! notes go to Activity.

use crate::contract::types::{LogLevel, McpRequest, McpStatus};
use starorg_mcp::http::{HttpServer, RequestLog, new_token};
use starorg_mcp::store::StoreConfig;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// Requests kept for the Activity view.
pub const REQUESTS_KEEP: usize = 100;

#[derive(Default)]
struct Shared {
    requests: Mutex<VecDeque<McpRequest>>,
    /// Lines for the Activity log, taken by the backend (`take_log`).
    pending: Mutex<Vec<(LogLevel, String)>>,
}

#[derive(Default)]
struct Running {
    server: Option<HttpServer>,
    started: Option<String>,
    channels: Vec<String>,
    error: Option<String>,
}

#[derive(Default)]
pub struct McpHost {
    state: Mutex<Running>,
    shared: Arc<Shared>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `claude mcp add` for Claude Code.
pub fn claude_code_config(url: &str, token: &str) -> String {
    format!(
        "claude mcp add --transport http starorg-strings {url} --header \"Authorization: Bearer {token}\""
    )
}

/// `claude_desktop_config.json` entry: Claude Desktop starts stdio servers, so it reaches
/// this one through the `mcp-remote` bridge (an npm package the user's npx fetches).
pub fn claude_desktop_config(url: &str, token: &str) -> String {
    let v = serde_json::json!({"mcpServers": {"starorg-strings": {
        "command": "npx",
        "args": ["-y", "mcp-remote", url, "--header", "Authorization:${STARORG_MCP_AUTH}"],
        "env": {"STARORG_MCP_AUTH": format!("Bearer {token}")}
    }}});
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

/// The same tools without the app: the CLI over stdio (no token; any MCP client).
pub const STDIO_COMMAND: &str = "starorg-strings mcp";

impl McpHost {
    pub fn status(&self) -> McpStatus {
        let s = lock(&self.state);
        let (url, token) = match &s.server {
            Some(h) => (Some(h.url()), Some(h.token().to_string())),
            None => (None, None),
        };
        McpStatus {
            running: s.server.is_some(),
            claude_code: url
                .as_deref()
                .zip(token.as_deref())
                .map(|(u, t)| claude_code_config(u, t)),
            claude_desktop: url
                .as_deref()
                .zip(token.as_deref())
                .map(|(u, t)| claude_desktop_config(u, t)),
            url,
            token,
            started: s.started.clone(),
            error: s.error.clone(),
            channels: s.channels.clone(),
            stdio: STDIO_COMMAND.into(),
            requests: lock(&self.shared.requests).iter().rev().cloned().collect(),
        }
    }

    fn note(&self, level: LogLevel, msg: String) {
        lock(&self.shared.pending).push((level, msg));
    }

    /// Start the server with `cfg()` (when not running). A failure leaves it off with the
    /// reason in `error`.
    pub fn enable(&self, cfg: impl FnOnce() -> Result<StoreConfig, String>) -> McpStatus {
        {
            let mut s = lock(&self.state);
            if s.server.is_none() {
                let started = (|| -> Result<(HttpServer, Vec<String>), String> {
                    let cfg = cfg()?;
                    let channels: Vec<String> =
                        cfg.channels.iter().map(|c| c.name.clone()).collect();
                    let shared = self.shared.clone();
                    let notify: starorg_mcp::store::Notify = Arc::new(move |m: String| {
                        lock(&shared.pending).push((LogLevel::Info, m));
                    });
                    let server = starorg_mcp::server(cfg, Some(notify));
                    let shared = self.shared.clone();
                    let on_request: starorg_mcp::http::OnRequest =
                        Arc::new(move |r: RequestLog| {
                            let mut q = lock(&shared.requests);
                            q.push_back(McpRequest {
                                time: starorg_mcp::iso_now(),
                                method: r.method,
                                tool: r.tool,
                                status: r.status,
                                ok: r.ok,
                                millis: r.millis,
                            });
                            while q.len() > REQUESTS_KEEP {
                                q.pop_front();
                            }
                        });
                    let token = new_token()?;
                    let h = HttpServer::start(server, 0, token, Some(on_request))
                        .map_err(|e| format!("cannot listen on 127.0.0.1: {e}"))?;
                    Ok((h, channels))
                })();
                match started {
                    Ok((h, channels)) => {
                        let url = h.url();
                        s.server = Some(h);
                        s.started = Some(starorg_mcp::iso_now());
                        s.error = None;
                        s.channels = channels;
                        drop(s);
                        self.note(
                            LogLevel::Info,
                            format!("Local MCP server started on {url} (this computer only)"),
                        );
                    }
                    Err(e) => {
                        s.error = Some(e.clone());
                        drop(s);
                        self.note(
                            LogLevel::Error,
                            format!("Local MCP server not started: {e}"),
                        );
                    }
                }
            }
        }
        self.status()
    }

    pub fn disable(&self) -> McpStatus {
        let stopped = {
            let mut s = lock(&self.state);
            let h = s.server.take();
            s.started = None;
            h
        };
        if let Some(mut h) = stopped {
            h.stop();
            self.note(LogLevel::Info, "Local MCP server stopped".into());
        }
        self.status()
    }

    /// Lines for the Activity log since the last call.
    pub fn take_log(&self) -> Vec<(LogLevel, String)> {
        std::mem::take(&mut *lock(&self.shared.pending))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use starorg_mcp::worker::{Extractor, Measurement};
    use std::io::{Read, Write};
    use std::path::Path;

    struct NoExtract;
    impl Extractor for NoExtract {
        fn extract(&self, _: &Path, d: &str, _: &Path) -> Result<Measurement, String> {
            Err(format!("{d}: no game in this test"))
        }
    }

    fn cfg(dir: &Path) -> StoreConfig {
        StoreConfig {
            channels: vec![],
            default_channel: None,
            cache_dir: dir.join("mcp"),
            uex_dir: None,
            our_outputs: vec![],
            extractor: Arc::new(NoExtract),
        }
    }

    fn post(url: &str, token: Option<&str>) -> u16 {
        let addr = url
            .trim_start_matches("http://")
            .trim_end_matches("/mcp")
            .to_string();
        let mut s = std::net::TcpStream::connect(addr).unwrap();
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"status","arguments":{}}}"#;
        let auth = token
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        write!(
            s,
            "POST /mcp HTTP/1.1\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        out.split_whitespace().nth(1).unwrap().parse().unwrap()
    }

    #[test]
    fn off_by_default_on_loopback_with_a_token_and_stops() {
        let dir = tempfile::tempdir().unwrap();
        let host = McpHost::default();
        let st = host.status();
        assert!(!st.running && st.url.is_none() && st.token.is_none());
        assert!(st.claude_code.is_none());

        let on = host.enable(|| Ok(cfg(dir.path())));
        assert!(on.running, "{:?}", on.error);
        let url = on.url.clone().unwrap();
        assert!(url.starts_with("http://127.0.0.1:") && url.ends_with("/mcp"));
        let token = on.token.clone().unwrap();
        assert_eq!(token.len(), 64);
        assert!(on.claude_code.as_deref().unwrap().contains(&token));
        assert!(on.claude_desktop.as_deref().unwrap().contains("mcp-remote"));
        // Enabling again keeps the same server and token.
        assert_eq!(host.enable(|| Ok(cfg(dir.path()))).token.unwrap(), token);

        assert_eq!(post(&url, None), 401);
        assert_eq!(post(&url, Some(&token)), 200);
        let reqs = host.status().requests;
        assert_eq!(reqs[0].tool.as_deref(), Some("status"));
        assert_eq!(reqs[1].status, 401);

        let off = host.disable();
        assert!(!off.running && off.url.is_none());
        let addr = url
            .trim_start_matches("http://")
            .trim_end_matches("/mcp")
            .to_string();
        assert!(std::net::TcpStream::connect(addr).is_err());
        let log: Vec<String> = host.take_log().into_iter().map(|(_, m)| m).collect();
        assert!(log[0].starts_with("Local MCP server started on http://127.0.0.1:"));
        assert_eq!(log.last().unwrap(), "Local MCP server stopped");
        assert!(host.take_log().is_empty());
        // A new start gets a new token.
        let again = host.enable(|| Ok(cfg(dir.path())));
        assert_ne!(again.token.unwrap(), token);
        host.disable();
    }

    #[test]
    fn a_failed_start_stays_off_with_the_reason() {
        let host = McpHost::default();
        let st = host.enable(|| Err("no install".into()));
        assert!(!st.running);
        assert_eq!(st.error.as_deref(), Some("no install"));
        assert_eq!(host.take_log()[0].0, LogLevel::Error);
    }
}
