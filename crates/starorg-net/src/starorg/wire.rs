//! Just enough HTTP/1.1 for the loopback callback (and the test server): one request per
//! connection, bounded sizes, `Connection: close`.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// Largest request head accepted.
pub const MAX_HEAD: usize = 16 * 1024;
/// Largest request body accepted (the test server's JSON bodies).
pub const MAX_BODY: usize = 1024 * 1024;

#[derive(Debug, Clone, Default)]
pub struct Request {
    pub method: String,
    /// Path without the query.
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn param(&self, k: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.as_str())
    }

    pub fn header(&self, k: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(k))
            .map(|(_, v)| v.as_str())
    }
}

/// `%XX` and `+` decoding; `None` for broken escapes or non-UTF-8.
pub fn decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => {
                let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// Percent-encode everything but RFC 3986 unreserved characters.
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn parse_query(q: &str) -> Option<Vec<(String, String)>> {
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            Some((decode(k)?, decode(v)?))
        })
        .collect()
}

/// Read one request. `None` for anything malformed or too large.
pub fn read_request(stream: &mut TcpStream) -> Option<Request> {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p;
        }
        if buf.len() > MAX_HEAD {
            return None;
        }
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = std::str::from_utf8(&buf[..head_end]).ok()?;
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_string();
    let target = first.next()?;
    if !first.next()?.starts_with("HTTP/1.") {
        return None;
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    let mut req = Request {
        method,
        path: path.to_string(),
        query: parse_query(query)?,
        headers,
        body: buf[head_end + 4..].to_vec(),
    };
    let len: usize = req
        .header("content-length")
        .map(|v| v.parse().ok())
        .unwrap_or(Some(0))?;
    if len > MAX_BODY {
        return None;
    }
    while req.body.len() < len {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        req.body.extend_from_slice(&chunk[..n]);
    }
    req.body.truncate(len);
    Some(req)
}

pub fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        302 => "Found",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        422 => "Unprocessable Entity",
        500 => "Internal Server Error",
        _ => "Status",
    }
}

pub fn respond(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    extra: &[(&str, &str)],
    body: &[u8],
) {
    let mut head = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n",
        reason(status),
        body.len()
    );
    for (k, v) in extra {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_round_trip() {
        let s = "http://127.0.0.1:5000/callback";
        assert_eq!(decode(&encode(s)).as_deref(), Some(s));
        assert_eq!(encode("a b/c"), "a%20b%2Fc");
        let q = parse_query("code=a%2Bb&state=xyz&flag").unwrap();
        assert_eq!(q[0], ("code".into(), "a+b".into()));
        assert_eq!(q[2], ("flag".into(), String::new()));
        assert!(parse_query("x=%zz").is_none());
    }
}
