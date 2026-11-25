//! HTTPS transport for [`starorg_core::source::dump`] (addendum §1).
//!
//! This is the only crate with an HTTP client outside the updater plugin. The CLI never
//! links it; the app links it for its network switches. For the dumps (Developer mode's Diff
//! while "Download published LIVE builds for Diff" is on; prebuilt datasets of the installed
//! build while Settings -> Game data -> "Download prebuilt game data" is on, the default,
//! `starorg_mcp::prebuilt`) and the update check it talks to two hosts, both read only:
//!
//! - `api.github.com`: the dumps repo's release list and the release for a tag, the commit a
//!   tag points at, the commit's tree (git blob ids, which the core verifies every file
//!   against), and the app's own release list ([`releases`], used only when "Check for
//!   updates" is on);
//! - `raw.githubusercontent.com`: the dataset files at that commit.
//!
//! Requests carry a generic `User-Agent` and nothing about the user or the install: no
//! token, no cookie, no install id.
//!
//! [`starorg`] is the StarOrg Tools client (sign-in and blueprint/hangar sync), used by the
//! app only while Settings -> Network -> "Connect StarOrg Tools" is on. It talks to
//! `starorg.tools` or `dev.starorg.tools` and sends the user's token there and nowhere else.
//!
//! [`uex`] downloads UEX Corp market data (`api.uexcorp.space`, two public endpoints) into
//! the folder WHERE TO GET reads, only while Settings -> Network -> "Download UEX market
//! data" is on. It sends the user's UEX token, when one is set, there and nowhere else.

pub mod releases;
pub mod starorg;
pub mod uex;

use serde::Deserialize;
use starorg_core::source::SourceError;
use starorg_core::source::dump::{DUMPS_REPO, DumpTransport, ReleaseInfo, TreeEntry};
use std::io::Read;
use std::time::Duration;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

const API: &str = "https://api.github.com";
const RAW: &str = "https://raw.githubusercontent.com";
/// Largest API response read (the tree listing of a build is about 10 KB).
const MAX_API_BYTES: u64 = 4 * 1024 * 1024;

/// GitHub transport for the dumps repo.
pub struct GitHubTransport {
    agent: ureq::Agent,
    repo: String,
}

impl GitHubTransport {
    pub fn new() -> Self {
        Self::for_repo(DUMPS_REPO)
    }

    pub fn for_repo(repo: &str) -> Self {
        let tls = TlsConfig::builder()
            .provider(TlsProvider::NativeTls)
            .root_certs(RootCerts::PlatformVerifier)
            .build();
        let agent = ureq::Agent::config_builder()
            .tls_config(tls)
            .https_only(true)
            .timeout_global(Some(Duration::from_secs(60)))
            // An unreachable GitHub fails fast: callers fall back to local data.
            .timeout_connect(Some(Duration::from_secs(10)))
            .user_agent(concat!("starorg-codex/", env!("CARGO_PKG_VERSION")))
            .http_status_as_error(false)
            .build()
            .into();
        Self {
            agent,
            repo: repo.to_string(),
        }
    }

    fn get(&self, url: &str, max: u64) -> Result<Option<Vec<u8>>, SourceError> {
        let net = |e: ureq::Error| SourceError::Unavailable(format!("{url}: {e}"));
        let resp = self
            .agent
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .call()
            .map_err(net)?;
        let status = resp.status().as_u16();
        if status == 404 {
            return Ok(None);
        }
        if status != 200 {
            return Err(SourceError::Unavailable(format!("{url}: HTTP {status}")));
        }
        let mut body = Vec::new();
        resp.into_body()
            .into_reader()
            .take(max + 1)
            .read_to_end(&mut body)
            .map_err(SourceError::Io)?;
        if body.len() as u64 > max {
            return Err(SourceError::Check(format!("{url}: more than {max} bytes")));
        }
        Ok(Some(body))
    }

    fn get_json<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<Option<T>, SourceError> {
        match self.get(url, MAX_API_BYTES)? {
            None => Ok(None),
            Some(b) => serde_json::from_slice(&b)
                .map(Some)
                .map_err(|e| SourceError::Check(format!("{url}: {e}"))),
        }
    }
}

impl Default for GitHubTransport {
    fn default() -> Self {
        Self::new()
    }
}

/// Only characters that can appear in a tag, commit or dataset path; refuses anything that
/// could change the URL's meaning.
fn url_safe(s: &str) -> Result<&str, SourceError> {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '/'))
        && !s.contains("..")
    {
        Ok(s)
    } else {
        Err(SourceError::Check(format!("refusing URL part {s:?}")))
    }
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
}

impl ApiRelease {
    fn info(self) -> ReleaseInfo {
        ReleaseInfo {
            tag: self.tag_name,
            title: self.name.unwrap_or_default(),
            body: self.body.unwrap_or_default(),
            published: self.published_at,
        }
    }
}

#[derive(Deserialize)]
struct ApiCommit {
    sha: String,
}

#[derive(Deserialize)]
struct ApiTree {
    #[serde(default)]
    tree: Vec<ApiTreeEntry>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Deserialize)]
struct ApiTreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    sha: String,
    #[serde(default)]
    size: Option<u64>,
}

/// Releases per page of the dumps repo's list (GitHub's maximum).
const RELEASES_PER_PAGE: usize = 100;
/// Pages followed: one release per build, so 300 covers years of LIVE, PTU and HOTFIX builds
/// and older LIVE builds stay listed once newer PTU releases push them down.
const RELEASE_PAGES: usize = 3;

impl DumpTransport for GitHubTransport {
    fn releases(&self) -> Result<Vec<ReleaseInfo>, SourceError> {
        let mut out = Vec::new();
        for page in 1..=RELEASE_PAGES {
            let url = format!(
                "{API}/repos/{}/releases?per_page={RELEASES_PER_PAGE}&page={page}",
                self.repo
            );
            let got = self.get_json::<Vec<ApiRelease>>(&url)?.unwrap_or_default();
            let last = got.len() < RELEASES_PER_PAGE;
            out.extend(got.into_iter().filter(|r| !r.draft).map(ApiRelease::info));
            if last {
                break;
            }
        }
        Ok(out)
    }

    fn release(&self, tag: &str) -> Result<Option<ReleaseInfo>, SourceError> {
        let url = format!("{API}/repos/{}/releases/tags/{}", self.repo, url_safe(tag)?);
        Ok(self.get_json::<ApiRelease>(&url)?.map(ApiRelease::info))
    }

    fn tag_commit(&self, tag: &str) -> Result<String, SourceError> {
        let url = format!("{API}/repos/{}/commits/{}", self.repo, url_safe(tag)?);
        self.get_json::<ApiCommit>(&url)?
            .map(|c| c.sha)
            .ok_or_else(|| SourceError::Unavailable(format!("tag {tag} not found")))
    }

    fn tree(&self, commit: &str) -> Result<Vec<TreeEntry>, SourceError> {
        let url = format!(
            "{API}/repos/{}/git/trees/{}?recursive=1",
            self.repo,
            url_safe(commit)?
        );
        let t: ApiTree = self
            .get_json(&url)?
            .ok_or_else(|| SourceError::Unavailable(format!("tree {commit} not found")))?;
        if t.truncated {
            return Err(SourceError::Check(format!(
                "tree {commit} listing truncated"
            )));
        }
        Ok(t.tree
            .into_iter()
            .filter(|e| e.kind == "blob")
            .map(|e| TreeEntry {
                path: e.path,
                sha: e.sha,
                size: e.size.unwrap_or(0),
            })
            .collect())
    }

    fn file(&self, commit: &str, path: &str, max: u64) -> Result<Vec<u8>, SourceError> {
        let url = format!(
            "{RAW}/{}/{}/{}",
            self.repo,
            url_safe(commit)?,
            url_safe(path)?
        );
        self.get(&url, max)?
            .ok_or_else(|| SourceError::Unavailable(format!("{path} not found at {commit}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_parts_are_checked() {
        assert!(url_safe("4.10.200.58746").is_ok());
        assert!(url_safe("data/game-ships.json").is_ok());
        assert!(url_safe("../x").is_err());
        assert!(url_safe("a?b").is_err());
        assert!(url_safe("a b").is_err());
        assert!(url_safe("").is_err());
    }

    /// Real network: the published LIVE 4.10.200.58746 dump fetches and verifies.
    #[test]
    #[ignore = "network"]
    fn live_release_resolves() {
        let t = GitHubTransport::new();
        let r = t.release("4.10.200.58746").unwrap().unwrap();
        assert!(r.body.contains("sc-alpha-4.10.0"));
        let c = t.tag_commit("4.10.200.58746").unwrap();
        assert_eq!(c.len(), 40);
        let tree = t.tree(&c).unwrap();
        assert!(tree.iter().any(|e| e.path == "data/game-ships.json"));
        assert!(t.release("0.0.0.0").unwrap().is_none());
        let live = starorg_core::source::dump::live_releases(&t).unwrap();
        assert!(live.iter().any(|r| r.tag == "4.10.200.58746"));
    }
}
