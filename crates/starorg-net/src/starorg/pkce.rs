//! PKCE (RFC 7636, S256), the app's `state`, and the loopback redirect rule (RFC 8252), as
//! SC-ORG-BOT's `apps/org_mgmt/api/routes/strings_auth.py` checks them:
//!
//! - verifier: 43-128 chars of `[A-Za-z0-9-._~]`; challenge: base64url(sha256), 43 chars;
//! - `state`: 16-128 chars of base64url;
//! - redirect: exactly `http://127.0.0.1:<port>/callback` or `http://[::1]:<port>/callback`,
//!   port 1024-65535 without a leading zero; never `localhost`.

use sha2::{Digest, Sha256};

const B64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// base64url without padding.
pub fn base64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = match chunk.len() {
            3 => (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8) | u32::from(chunk[2]),
            2 => (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8),
            _ => u32::from(chunk[0]) << 16,
        };
        let chars = chunk.len() + 1;
        for i in 0..chars {
            out.push(B64URL[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

fn random(n: usize) -> Result<Vec<u8>, String> {
    let mut b = vec![0u8; n];
    getrandom::fill(&mut b).map_err(|e| format!("no randomness for the sign-in: {e}"))?;
    Ok(b)
}

/// The S256 challenge for `verifier`.
pub fn challenge(verifier: &str) -> String {
    base64url(&Sha256::digest(verifier.as_bytes()))
}

/// One sign-in attempt's secrets. Never logged, never stored.
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
    pub state: String,
}

impl std::fmt::Debug for Pkce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Pkce { .. }")
    }
}

impl Pkce {
    /// 64 random bytes for the verifier (86 chars), 32 for the state (43 chars).
    pub fn new() -> Result<Self, String> {
        let verifier = base64url(&random(64)?);
        let state = base64url(&random(32)?);
        Ok(Self {
            challenge: challenge(&verifier),
            verifier,
            state,
        })
    }
}

pub fn valid_verifier(v: &str) -> bool {
    (43..=128).contains(&v.len())
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'))
}

fn b64url_chars(s: &str) -> bool {
    s.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn valid_challenge(c: &str) -> bool {
    c.len() == 43 && b64url_chars(c)
}

pub fn valid_state(s: &str) -> bool {
    (16..=128).contains(&s.len()) && b64url_chars(s)
}

/// The port of an accepted loopback redirect, else `None` (the server's `_REDIRECT_RE` plus
/// its port range).
pub fn loopback_redirect_port(uri: &str) -> Option<u16> {
    let rest = uri
        .strip_prefix("http://127.0.0.1:")
        .or_else(|| uri.strip_prefix("http://[::1]:"))?;
    let port = rest.strip_suffix("/callback")?;
    if port.is_empty()
        || port.len() > 5
        || port.starts_with('0')
        || !port.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let p: u32 = port.parse().ok()?;
    (1024..=65535).contains(&p).then_some(p as u16)
}

/// Constant-time equality (the callback's `state`).
pub fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s256_matches_rfc7636_appendix_b() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            challenge(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
    }

    #[test]
    fn fresh_pkce_passes_the_server_rules_and_is_random() {
        let a = Pkce::new().unwrap();
        let b = Pkce::new().unwrap();
        assert!(valid_verifier(&a.verifier), "{}", a.verifier.len());
        assert_eq!(a.verifier.len(), 86);
        assert!(valid_challenge(&a.challenge));
        assert_eq!(a.challenge, challenge(&a.verifier));
        assert!(valid_state(&a.state));
        assert_ne!(a.verifier, b.verifier);
        assert_ne!(a.state, b.state);
        assert_eq!(format!("{a:?}"), "Pkce { .. }");
    }

    #[test]
    fn only_loopback_callbacks_are_redirects() {
        assert_eq!(
            loopback_redirect_port("http://127.0.0.1:49152/callback"),
            Some(49152)
        );
        assert_eq!(
            loopback_redirect_port("http://[::1]:1024/callback"),
            Some(1024)
        );
        for bad in [
            "http://localhost:49152/callback",
            "https://127.0.0.1:49152/callback",
            "http://127.0.0.1/callback",
            "http://127.0.0.1:80/callback",
            "http://127.0.0.1:049152/callback",
            "http://127.0.0.1:70000/callback",
            "http://127.0.0.1:49152/callback/",
            "http://127.0.0.1:49152/other",
            "http://127.0.0.1:49152/callback?x=1",
            "http://127.0.0.2:49152/callback",
            "http://user@127.0.0.1:49152/callback",
        ] {
            assert_eq!(loopback_redirect_port(bad), None, "{bad}");
        }
    }

    #[test]
    fn state_compare_is_exact() {
        assert!(same("abc", "abc"));
        assert!(!same("abc", "abd"));
        assert!(!same("abc", "abcd"));
    }
}
