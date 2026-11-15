//! The loopback redirect listener (RFC 8252 §7.3): `127.0.0.1` on a free port, path
//! `/callback`, one answer. The server sends the browser back here with `?code=&state=` (or
//! `?error=access_denied&state=` when the user cancels on Discord).

use super::StarOrgError;
use super::pkce;
use super::wire;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::time::{Duration, Instant};

const PAGE_OK: &str = "<!doctype html><meta charset=utf-8><title>StarOrg Codex</title><body style=\"font:16px system-ui;margin:3rem\"><h1>Signed in</h1><p>You can close this tab and go back to StarOrg Codex.</p>";
const PAGE_CANCELLED: &str = "<!doctype html><meta charset=utf-8><title>StarOrg Codex</title><body style=\"font:16px system-ui;margin:3rem\"><h1>Sign-in cancelled</h1><p>Nothing was linked. You can close this tab.</p>";
const PAGE_BAD: &str = "<!doctype html><meta charset=utf-8><title>StarOrg Codex</title><body style=\"font:16px system-ui;margin:3rem\"><h1>This sign-in was not started by StarOrg Codex</h1><p>Nothing was linked. Start again from the app.</p>";

pub struct Loopback {
    listener: TcpListener,
    port: u16,
}

impl Loopback {
    /// Bind `127.0.0.1:0` (an ephemeral port picked by the OS).
    pub fn bind() -> Result<Self, StarOrgError> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .map_err(|e| StarOrgError::Network(format!("cannot listen on 127.0.0.1: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| StarOrgError::Network(e.to_string()))?
            .port();
        listener
            .set_nonblocking(true)
            .map_err(|e| StarOrgError::Network(e.to_string()))?;
        Ok(Self { listener, port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.listener.local_addr().ok()
    }

    /// `http://127.0.0.1:<port>/callback`, the only form the server accepts besides `[::1]`.
    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}/callback", self.port)
    }

    /// Wait for the browser's return until `deadline` or `cancel()`. Other paths get a 404
    /// and the wait goes on; a `state` that is not ours ends the attempt (nothing is
    /// exchanged).
    pub fn wait(
        &self,
        expected_state: &str,
        deadline: Instant,
        cancel: &dyn Fn() -> bool,
    ) -> Result<String, StarOrgError> {
        loop {
            if cancel() {
                return Err(StarOrgError::Cancelled("sign-in cancelled".into()));
            }
            if Instant::now() >= deadline {
                return Err(StarOrgError::Timeout);
            }
            let (mut stream, peer) = match self.listener.accept() {
                Ok(x) => x,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(40));
                    continue;
                }
                Err(e) => return Err(StarOrgError::Network(e.to_string())),
            };
            if !peer.ip().is_loopback() {
                continue;
            }
            let _ = stream.set_nonblocking(false);
            let Some(req) = wire::read_request(&mut stream) else {
                wire::respond(&mut stream, 400, "text/plain", &[], b"bad request");
                continue;
            };
            if req.method != "GET" || req.path != "/callback" {
                wire::respond(&mut stream, 404, "text/plain", &[], b"not found");
                continue;
            }
            let state_ok = req
                .param("state")
                .is_some_and(|s| pkce::same(s, expected_state));
            if !state_ok {
                wire::respond(
                    &mut stream,
                    400,
                    "text/html; charset=utf-8",
                    &[],
                    PAGE_BAD.as_bytes(),
                );
                return Err(StarOrgError::StateMismatch);
            }
            if let Some(e) = req.param("error") {
                wire::respond(
                    &mut stream,
                    200,
                    "text/html; charset=utf-8",
                    &[],
                    PAGE_CANCELLED.as_bytes(),
                );
                return Err(StarOrgError::Cancelled(if e == "access_denied" {
                    "sign-in cancelled on Discord".into()
                } else {
                    "StarOrg Tools could not finish the sign-in".into()
                }));
            }
            match req.param("code").filter(|c| (16..=128).contains(&c.len())) {
                Some(code) => {
                    wire::respond(
                        &mut stream,
                        200,
                        "text/html; charset=utf-8",
                        &[],
                        PAGE_OK.as_bytes(),
                    );
                    return Ok(code.to_string());
                }
                None => {
                    wire::respond(
                        &mut stream,
                        400,
                        "text/html; charset=utf-8",
                        &[],
                        PAGE_BAD.as_bytes(),
                    );
                    return Err(StarOrgError::Protocol("callback without a code".into()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn get(port: u16, target: &str) -> String {
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(s, "GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        out
    }

    fn run(targets: Vec<String>) -> Result<String, StarOrgError> {
        let lb = Loopback::bind().unwrap();
        assert!(lb.local_addr().unwrap().ip().is_loopback());
        assert_eq!(
            pkce::loopback_redirect_port(&lb.redirect_uri()),
            Some(lb.port())
        );
        let port = lb.port();
        let t = std::thread::spawn(move || {
            for x in targets {
                get(port, &x);
            }
        });
        let r = lb.wait(
            "expected-state-0123456789",
            Instant::now() + Duration::from_secs(10),
            &|| false,
        );
        t.join().unwrap();
        r
    }

    #[test]
    fn returns_the_code_after_ignoring_other_paths() {
        let r = run(vec![
            "/favicon.ico".into(),
            "/callback/extra?code=x".into(),
            "/callback?code=one-time-code-abcdef&state=expected-state-0123456789".into(),
        ]);
        assert_eq!(r.unwrap(), "one-time-code-abcdef");
    }

    #[test]
    fn state_mismatch_is_rejected() {
        let r = run(vec![
            "/callback?code=one-time-code-abcdef&state=someone-elses-state-xx".into(),
        ]);
        assert!(matches!(r, Err(StarOrgError::StateMismatch)), "{r:?}");
        let r = run(vec!["/callback?code=one-time-code-abcdef".into()]);
        assert!(matches!(r, Err(StarOrgError::StateMismatch)), "{r:?}");
    }

    #[test]
    fn cancel_on_discord_and_timeout() {
        let r = run(vec![
            "/callback?error=access_denied&state=expected-state-0123456789".into(),
        ]);
        assert!(matches!(r, Err(StarOrgError::Cancelled(_))), "{r:?}");
        let lb = Loopback::bind().unwrap();
        let r = lb.wait("s", Instant::now() + Duration::from_millis(100), &|| false);
        assert!(matches!(r, Err(StarOrgError::Timeout)));
        let r = lb.wait("s", Instant::now() + Duration::from_secs(5), &|| true);
        assert!(matches!(r, Err(StarOrgError::Cancelled(_))));
    }
}
