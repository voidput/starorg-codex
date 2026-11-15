//! The streamable-HTTP transport (MCP 2025-06-18), for the app: loopback only, bearer token.
//!
//! A small `std::net` HTTP/1.1 server, no HTTP crate. One endpoint, `/mcp`:
//!
//! - `POST` a JSON-RPC message; the answer comes back as one `application/json` body (this
//!   server never streams, so no SSE), or `202 Accepted` for a notification.
//! - `GET` is `405` (no server-initiated stream); `DELETE` ends the session.
//!
//! Every request needs `Authorization: Bearer <token>` (`401` otherwise). The socket is bound
//! to `127.0.0.1` only, a browser `Origin` other than localhost is refused (`403`, against
//! DNS rebinding), bodies are capped at 1 MiB, and each connection is closed after its
//! answer. [`HttpServer::stop`] (or dropping it) stops accepting and joins the thread.

use crate::protocol::{Handled, PROTOCOL_VERSIONS, Server};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const PATH: &str = "/mcp";
const MAX_BODY: usize = 1 << 20;
const MAX_HEAD: usize = 16 << 10;
const MAX_CONNECTIONS: usize = 8;

/// One HTTP request the server answered, for the app's request log.
#[derive(Debug, Clone, PartialEq)]
pub struct RequestLog {
    pub status: u16,
    /// JSON-RPC method, when the body had one.
    pub method: Option<String>,
    pub tool: Option<String>,
    pub ok: bool,
    pub millis: u64,
}

pub type OnRequest = Arc<dyn Fn(RequestLog) + Send + Sync>;

/// A random 256-bit token, hex.
pub fn new_token() -> Result<String, String> {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).map_err(|e| format!("no randomness for the token: {e}"))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

/// Equal without leaking where they differ.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub struct HttpServer {
    addr: SocketAddr,
    token: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

struct Shared {
    server: Server,
    token: String,
    sessions: Mutex<Vec<String>>,
    on_request: Option<OnRequest>,
    open: AtomicUsize,
}

impl HttpServer {
    /// Listen on `127.0.0.1:<port>` (0 = any free port).
    pub fn start(
        server: Server,
        port: u16,
        token: String,
        on_request: Option<OnRequest>,
    ) -> std::io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Shared {
            server,
            token: token.clone(),
            sessions: Mutex::new(Vec::new()),
            on_request,
            open: AtomicUsize::new(0),
        });
        let flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("mcp-http".into())
            .spawn(move || accept_loop(listener, shared, flag))?;
        Ok(Self {
            addr,
            token,
            stop,
            thread: Some(thread),
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn url(&self) -> String {
        format!("http://{}{PATH}", self.addr)
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for HttpServer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// One counted open connection; dropping it frees the count, also on unwind.
struct Slot(Arc<Shared>);

impl Slot {
    fn take(shared: Arc<Shared>) -> Self {
        shared.open.fetch_add(1, Ordering::SeqCst);
        Self(shared)
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.open.fetch_sub(1, Ordering::SeqCst);
    }
}

fn accept_loop(listener: TcpListener, shared: Arc<Shared>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, peer)) => {
                // Bound to loopback, so this holds; checked anyway.
                if !peer.ip().is_loopback() {
                    continue;
                }
                if shared.open.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
                    let _ = respond(&stream, 503, &[], b"busy");
                    continue;
                }
                let slot = Slot::take(shared.clone());
                // The slot is freed when the closure is dropped: after `connection` returns,
                // when it panics, or when the thread cannot be spawned.
                let _ = std::thread::Builder::new()
                    .name("mcp-http-conn".into())
                    .spawn(move || {
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
                        connection(&stream, &slot.0);
                    });
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(25)),
        }
    }
}

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

fn read_request(stream: &TcpStream) -> Result<Request, (u16, &'static str)> {
    let mut r = BufReader::new(stream);
    let mut head = Vec::new();
    loop {
        let mut line = Vec::new();
        let n = r
            .by_ref()
            .take((MAX_HEAD - head.len().min(MAX_HEAD)) as u64 + 1)
            .read_until(b'\n', &mut line)
            .map_err(|_| (400, "unreadable request"))?;
        if n == 0 {
            return Err((400, "incomplete request"));
        }
        head.extend_from_slice(&line);
        if head.len() > MAX_HEAD {
            return Err((431, "headers too large"));
        }
        if line == b"\r\n" || line == b"\n" {
            break;
        }
    }
    let text = String::from_utf8(head).map_err(|_| (400, "headers are not UTF-8"))?;
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let (method, path) = (
        parts.next().unwrap_or("").to_string(),
        parts.next().unwrap_or("").to_string(),
    );
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    let mut req = Request {
        method,
        path,
        headers,
        body: Vec::new(),
    };
    if req
        .header("transfer-encoding")
        .is_some_and(|t| !t.eq_ignore_ascii_case("identity"))
    {
        return Err((411, "send a Content-Length body"));
    }
    let len: usize = match req.header("content-length") {
        Some(v) => v.parse().map_err(|_| (400, "bad Content-Length"))?,
        None => 0,
    };
    if len > MAX_BODY {
        return Err((413, "body too large"));
    }
    req.body = vec![0; len];
    r.read_exact(&mut req.body)
        .map_err(|_| (400, "body shorter than Content-Length"))?;
    Ok(req)
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        411 => "Length Required",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        431 => "Request Header Fields Too Large",
        503 => "Service Unavailable",
        _ => "Error",
    }
}

fn respond(
    mut stream: &TcpStream,
    status: u16,
    headers: &[(&str, String)],
    body: &[u8],
) -> std::io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {status} {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n",
        reason(status),
        body.len()
    );
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

/// Only pages served from this computer may call (a browser sends `Origin`; CLIs do not).
fn origin_ok(origin: Option<&str>) -> bool {
    let Some(o) = origin else {
        return true;
    };
    let rest = o
        .strip_prefix("http://")
        .or_else(|| o.strip_prefix("https://"));
    let host = rest.map(|r| r.split('/').next().unwrap_or(""));
    match host {
        Some(h) => {
            let name = h.rsplit_once(':').map_or(h, |(n, port)| {
                if port.bytes().all(|b| b.is_ascii_digit()) {
                    n
                } else {
                    h
                }
            });
            matches!(name, "127.0.0.1" | "localhost" | "[::1]")
        }
        None => false,
    }
}

fn connection(stream: &TcpStream, s: &Shared) -> Option<RequestLog> {
    let started = Instant::now();
    let mut log = RequestLog {
        status: 0,
        method: None,
        tool: None,
        ok: false,
        millis: 0,
    };
    // Logged before the answer goes out, so a client that has its answer sees it logged.
    let finish = |mut log: RequestLog, status: u16, headers: &[(&str, String)], body: &[u8]| {
        log.status = status;
        log.millis = started.elapsed().as_millis() as u64;
        if let Some(cb) = &s.on_request {
            cb(log.clone());
        }
        let _ = respond(stream, status, headers, body);
        Some(log)
    };
    let text = |m: &str| m.as_bytes().to_vec();
    let req = match read_request(stream) {
        Ok(r) => r,
        Err((status, msg)) => return finish(log, status, &[], &text(msg)),
    };
    if !origin_ok(req.header("origin")) {
        return finish(log, 403, &[], &text("origin not allowed"));
    }
    let auth = req
        .header("authorization")
        .and_then(|a| {
            a.strip_prefix("Bearer ")
                .or_else(|| a.strip_prefix("bearer "))
        })
        .map(str::trim);
    if !auth.is_some_and(|t| same(t.as_bytes(), s.token.as_bytes())) {
        return finish(
            log,
            401,
            &[("WWW-Authenticate", "Bearer".to_string())],
            &text("missing or wrong bearer token"),
        );
    }
    let path = req.path.split('?').next().unwrap_or("");
    if path != PATH {
        return finish(log, 404, &[], &text("not found; the endpoint is /mcp"));
    }
    if let Some(v) = req.header("mcp-protocol-version")
        && !PROTOCOL_VERSIONS.contains(&v)
    {
        return finish(log, 400, &[], &text("unsupported MCP-Protocol-Version"));
    }
    let session = req.header("mcp-session-id").map(str::to_string);
    let known = |id: &str| {
        s.sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .any(|x| x == id)
    };
    match req.method.as_str() {
        "GET" => finish(log, 405, &[("Allow", "POST, DELETE".to_string())], &[]),
        "DELETE" => {
            if let Some(id) = &session {
                s.sessions
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .retain(|x| x != id);
            }
            log.ok = true;
            finish(log, 200, &[], &[])
        }
        "POST" => {
            if let Some(id) = &session
                && !known(id)
            {
                return finish(log, 404, &[], &text("unknown or ended session"));
            }
            let ct = req.header("content-type").unwrap_or("");
            if !ct.to_ascii_lowercase().starts_with("application/json") {
                return finish(
                    log,
                    415,
                    &[],
                    &text("Content-Type must be application/json"),
                );
            }
            let msg: serde_json::Value = match serde_json::from_slice(&req.body) {
                Ok(v) => v,
                Err(e) => {
                    let body = serde_json::json!({"jsonrpc": "2.0", "id": null,
                        "error": {"code": crate::protocol::code::PARSE, "message": format!("parse error: {e}")}});
                    return finish(
                        log,
                        400,
                        &[("Content-Type", "application/json".into())],
                        body.to_string().as_bytes(),
                    );
                }
            };
            let initialize =
                msg.get("method").and_then(serde_json::Value::as_str) == Some("initialize");
            let mut handled: Vec<Handled> = Vec::new();
            let answer = s.server.handle(&msg, &mut handled);
            if let Some(h) = handled.first() {
                log.method = Some(h.method.clone());
                log.tool = h.tool.clone();
                log.ok = h.ok;
            } else {
                log.ok = true;
            }
            match answer {
                None => finish(log, 202, &[], &[]),
                Some(a) => {
                    let mut headers = vec![("Content-Type", "application/json".to_string())];
                    if initialize
                        && a.get("result").is_some()
                        && let Ok(id) = new_token()
                    {
                        let id = id[..32].to_string();
                        let mut list = s
                            .sessions
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        list.push(id.clone());
                        // A handful of clients at most; forget the oldest.
                        while list.len() > 32 {
                            list.remove(0);
                        }
                        headers.push(("Mcp-Session-Id", id));
                    }
                    finish(log, 200, &headers, a.to_string().as_bytes())
                }
            }
        }
        _ => finish(log, 405, &[("Allow", "POST, DELETE".to_string())], &[]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins() {
        assert!(origin_ok(None));
        assert!(origin_ok(Some("http://localhost:5173")));
        assert!(origin_ok(Some("http://127.0.0.1")));
        assert!(!origin_ok(Some("https://evil.example")));
        assert!(!origin_ok(Some("http://localhost.evil.example")));
        assert!(!origin_ok(Some("null")));
    }

    #[test]
    fn tokens() {
        let a = new_token().unwrap();
        assert_eq!(a.len(), 64);
        assert_ne!(a, new_token().unwrap());
        assert!(same(b"abc", b"abc") && !same(b"abc", b"abd") && !same(b"abc", b"ab"));
    }
}
