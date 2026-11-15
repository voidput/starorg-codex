//! The app's release list, for "Check for updates" (spec 11, 12; docs/updates.md).
//!
//! One GET of `https://api.github.com/repos/<owner>/<name>/releases`, with no credentials.
//! This module only moves bytes: the status, the rate-limit header and the body come back
//! as they are, and the app decides what they mean (`app/src-tauri/src/update`), so that
//! logic is tested without a network.

use std::io::Read;
use std::time::Duration;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

/// Largest release list read. 30 releases with long notes fit in well under 1 MiB.
pub const MAX_RELEASES_BYTES: u64 = 2 * 1024 * 1024;

/// What GitHub answered, uninterpreted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub status: u16,
    /// `x-ratelimit-remaining`, when sent (403/429 with `0` = rate limited).
    pub rate_limit_remaining: Option<u32>,
    pub body: Vec<u8>,
}

/// `owner/name` with only the characters GitHub allows in either part.
pub fn valid_repo(repo: &str) -> bool {
    let mut parts = repo.split('/');
    let ok = |p: Option<&str>| {
        p.is_some_and(|p| {
            !p.is_empty()
                && p.len() <= 100
                && p != "."
                && p != ".."
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
    };
    ok(parts.next()) && ok(parts.next()) && parts.next().is_none()
}

/// The API URL for a repo's release list (newest 30, GitHub's order).
pub fn releases_url(repo: &str) -> Option<String> {
    valid_repo(repo).then(|| format!("https://api.github.com/repos/{repo}/releases?per_page=30"))
}

/// Fetch the release list. `Err` = no HTTP answer at all (offline, DNS, TLS, timeout).
/// `user_agent` is the app name and version only.
pub fn fetch_releases(repo: &str, user_agent: &str) -> Result<Reply, String> {
    let url = releases_url(repo).ok_or_else(|| format!("refusing repo {repo:?}"))?;
    let tls = TlsConfig::builder()
        .provider(TlsProvider::NativeTls)
        .root_certs(RootCerts::PlatformVerifier)
        .build();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(tls)
        .https_only(true)
        .timeout_global(Some(Duration::from_secs(20)))
        .user_agent(user_agent)
        .http_status_as_error(false)
        .build()
        .into();
    let resp = agent
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call()
        .map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let rate_limit_remaining = resp
        .headers()
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse().ok());
    let mut body = Vec::new();
    resp.into_body()
        .into_reader()
        .take(MAX_RELEASES_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    if body.len() as u64 > MAX_RELEASES_BYTES {
        // Not an answer we read; the app reports it as a bad response.
        body.clear();
        return Ok(Reply {
            status: 0,
            rate_limit_remaining,
            body,
        });
    }
    Ok(Reply {
        status,
        rate_limit_remaining,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_names_are_checked() {
        assert!(valid_repo("voidput/starorg-codex"));
        assert!(valid_repo("a.b/c_d-e"));
        for bad in [
            "",
            "voidput",
            "voidput/",
            "/x",
            "a/b/c",
            "../x",
            "a/..",
            "a b/c",
            "a/b?x=1",
            "a/b#c",
            "https://github.com/a/b",
        ] {
            assert!(!valid_repo(bad), "{bad}");
        }
        assert_eq!(
            releases_url("voidput/starorg-codex").unwrap(),
            "https://api.github.com/repos/voidput/starorg-codex/releases?per_page=30"
        );
        assert!(releases_url("a/b/c").is_none());
    }

    /// Real network: the public Tauri repo lists releases without credentials.
    #[test]
    #[ignore = "network"]
    fn public_repo_lists_releases() {
        let r = fetch_releases("tauri-apps/tauri", "StarOrg-Codex/0.0.0").unwrap();
        assert_eq!(r.status, 200);
        assert!(r.body.starts_with(b"["));
    }
}
