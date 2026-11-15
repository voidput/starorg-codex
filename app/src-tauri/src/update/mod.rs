//! "Check for updates" (spec 11, 12; docs/updates.md).
//!
//! - Off by default. With `network.check_updates` off nothing here makes a request; the last
//!   result is kept and shown as it was.
//! - On: at most one request per 24 h when the app starts, plus "Check now". One GET of the
//!   repo's release list ([`starorg_net::releases`]), no credentials, `User-Agent:
//!   StarOrg-Strings/<version>` only.
//! - Drafts are ignored; pre-releases only with "Include pre-releases". Versions compare by
//!   semver precedence (build metadata ignored).
//! - Release notes are reduced to plain text blocks ([`notes`]); the release page URL is
//!   built here from the checked repo and tag, never taken from the response.
//! - Installing (AppImage, NSIS, MSI with a real key) goes through `tauri-plugin-updater`,
//!   which verifies the minisign signature before anything runs ([`host`]).
//!
//! Everything that decides (selection, throttle, skip, error mapping) is plain code over a
//! [`ReleaseFetch`] and a [`Clock`], so tests run without a network.

pub mod host;
#[cfg(any(test, feature = "mock-backend"))]
pub mod mock;
pub mod notes;

pub use host::{HostSlot, UpdateHost};

use crate::contract::types::{
    AppError, CmdResult, ErrorCode, ReleaseView, UpdateCheck, UpdateInstall, UpdatePrefs,
    UpdateProblem, UpdateStatus,
};
use semver::Version;
use serde::{Deserialize, Serialize};
use starorg_net::releases::{Reply, valid_repo};
use std::path::PathBuf;
use std::sync::Mutex;

/// The source repo; Q1 may move public releases elsewhere (a mirror is a setting).
pub const DEFAULT_REPO: &str = "voidput/starorg-strings";
/// The start-up check runs at most this often.
pub const CHECK_EVERY_SECS: u64 = 24 * 60 * 60;

/// The only identifying header sent: the app's name and version.
pub fn user_agent() -> String {
    format!("StarOrg-Strings/{}", env!("CARGO_PKG_VERSION"))
}

/// One HTTP GET of a repo's release list. `Err` = no HTTP answer (offline).
pub trait ReleaseFetch: Send + Sync {
    fn fetch(&self, repo: &str) -> Result<Reply, String>;
}

/// The real request, over `starorg-net`.
pub struct GitHubFetch;

impl ReleaseFetch for GitHubFetch {
    fn fetch(&self, repo: &str) -> Result<Reply, String> {
        starorg_net::releases::fetch_releases(repo, &user_agent())
    }
}

/// Seconds since the Unix epoch.
pub trait Clock: Send + Sync {
    fn now(&self) -> u64;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> u64 {
        starorg_core::fsutil::unix_now()
    }
}

/// What is kept between runs (`app-update.json` in the data folder for the real backend).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub include_prereleases: bool,
    #[serde(default)]
    pub last_checked: Option<u64>,
    #[serde(default)]
    pub check: Option<UpdateCheck>,
    #[serde(default)]
    pub skipped_version: Option<String>,
}

pub enum Store {
    File(PathBuf),
    Memory(Box<Mutex<Saved>>),
}

impl Store {
    fn load(&self) -> Saved {
        match self {
            // A missing or unreadable file is a fresh start, never an error.
            Store::File(p) => starorg_core::fsutil::read_json(p)
                .ok()
                .flatten()
                .unwrap_or_default(),
            Store::Memory(m) => m.lock().unwrap_or_else(|p| p.into_inner()).clone(),
        }
    }

    fn save(&self, s: &Saved) -> CmdResult<()> {
        match self {
            Store::File(p) => starorg_core::fsutil::write_json(p, s)
                .map_err(|e| AppError::new(ErrorCode::Internal, e.to_string())),
            Store::Memory(m) => {
                *m.lock().unwrap_or_else(|p| p.into_inner()) = s.clone();
                Ok(())
            }
        }
    }
}

/// The update checker one backend owns.
pub struct Updater {
    store: Store,
    fetch: Box<dyn ReleaseFetch>,
    clock: Box<dyn Clock>,
    current: Version,
    install: (UpdateInstall, String),
    host: HostSlot,
    /// One check or write at a time.
    busy: Mutex<()>,
}

impl Updater {
    pub fn new(
        store: Store,
        fetch: Box<dyn ReleaseFetch>,
        clock: Box<dyn Clock>,
        current: Version,
        install: (UpdateInstall, String),
    ) -> Self {
        Self {
            store,
            fetch,
            clock,
            current,
            install,
            host: HostSlot::default(),
            busy: Mutex::new(()),
        }
    }

    /// The real checker: GitHub, the system clock, this build's version and install kind.
    pub fn github(state_file: PathBuf) -> Self {
        Self::new(
            Store::File(state_file),
            Box::new(GitHubFetch),
            Box::new(SystemClock),
            Version::parse(env!("CARGO_PKG_VERSION")).expect("crate version is semver"),
            host::install_support(),
        )
    }

    /// The real checker over another HTTP layer (tests: a fake that never touches the network).
    pub fn with_fetch(state_file: PathBuf, fetch: Box<dyn ReleaseFetch>) -> Self {
        Self::new(
            Store::File(state_file),
            fetch,
            Box::new(SystemClock),
            Version::parse(env!("CARGO_PKG_VERSION")).expect("crate version is semver"),
            host::install_support(),
        )
    }

    /// Where the window's opener and installer go once the app exists.
    pub fn host(&self) -> HostSlot {
        self.host.clone()
    }

    pub fn with_host(mut self, host: HostSlot) -> Self {
        self.host = host;
        self
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.busy.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn prefs(s: &Saved) -> UpdatePrefs {
        UpdatePrefs {
            repo: s.repo.clone().unwrap_or_else(|| DEFAULT_REPO.to_string()),
            include_prereleases: s.include_prereleases,
        }
    }

    fn view(&self, enabled: bool, s: &Saved) -> UpdateStatus {
        let mut check = s.check.clone().unwrap_or(UpdateCheck::NotChecked);
        // After an update the saved "available" may be this version or older.
        if let UpdateCheck::Available { release } = &check
            && parse_version(&release.version)
                .is_none_or(|v| v.cmp_precedence(&self.current).is_le())
        {
            check = UpdateCheck::UpToDate;
        }
        UpdateStatus {
            enabled,
            current_version: self.current.to_string(),
            prefs: Self::prefs(s),
            last_checked: s.last_checked.map(starorg_core::fsutil::rfc3339),
            check,
            skipped_version: s.skipped_version.clone(),
            install: self.install.0,
            install_note: self.install.1.clone(),
        }
    }

    pub fn status(&self, enabled: bool) -> CmdResult<UpdateStatus> {
        let _g = self.lock();
        Ok(self.view(enabled, &self.store.load()))
    }

    /// `force = false`: the start-up check; nothing is requested when the switch is off or
    /// the last check is less than 24 h old. `force = true` ("Check now") needs the switch.
    pub fn check(&self, enabled: bool, force: bool) -> CmdResult<UpdateStatus> {
        let _g = self.lock();
        let mut s = self.store.load();
        if !enabled {
            if force {
                return Err(network_off());
            }
            return Ok(self.view(enabled, &s));
        }
        let now = self.clock.now();
        if !force && !due(s.last_checked, now) {
            return Ok(self.view(enabled, &s));
        }
        let prefs = Self::prefs(&s);
        let result = match self.fetch.fetch(&prefs.repo) {
            Ok(reply) => map_reply(&reply, &prefs, &self.current),
            Err(_) => failed(UpdateProblem::Offline, &prefs.repo, None),
        };
        s.last_checked = Some(now);
        s.check = Some(result);
        self.store.save(&s)?;
        Ok(self.view(enabled, &s))
    }

    /// A new repo or pre-release rule forgets the last result, so the next check runs.
    pub fn set_prefs(&self, enabled: bool, p: UpdatePrefs) -> CmdResult<UpdateStatus> {
        let _g = self.lock();
        let repo = p.repo.trim().trim_end_matches('/').to_string();
        let repo = repo
            .strip_prefix("https://github.com/")
            .map(str::to_string)
            .unwrap_or(repo);
        if !valid_repo(&repo) {
            return Err(AppError::new(
                ErrorCode::Invalid,
                "Enter the repository as owner/name, for example voidput/starorg-strings.",
            ));
        }
        let mut s = self.store.load();
        let old = Self::prefs(&s);
        s.repo = (repo != DEFAULT_REPO).then_some(repo.clone());
        s.include_prereleases = p.include_prereleases;
        if old.repo != repo || old.include_prereleases != p.include_prereleases {
            s.last_checked = None;
            s.check = None;
        }
        self.store.save(&s)?;
        Ok(self.view(enabled, &s))
    }

    /// No banner for exactly this version. An empty string clears the skip.
    pub fn skip(&self, enabled: bool, version: String) -> CmdResult<UpdateStatus> {
        let _g = self.lock();
        let mut s = self.store.load();
        let v = version.trim();
        s.skipped_version = if v.is_empty() {
            None
        } else {
            Some(
                parse_version(v)
                    .ok_or_else(|| {
                        AppError::new(ErrorCode::Invalid, format!("not a version: {v}"))
                    })?
                    .to_string(),
            )
        };
        self.store.save(&s)?;
        Ok(self.view(enabled, &s))
    }

    fn available(&self, s: &Saved) -> CmdResult<ReleaseView> {
        match self.view(true, s).check {
            UpdateCheck::Available { release } => Ok(release),
            _ => Err(AppError::new(
                ErrorCode::NotFound,
                "No newer version is known. Check for updates first.",
            )),
        }
    }

    /// The newest release's page in the system browser. Only `https://github.com/<repo>/
    /// releases/...` is ever opened.
    pub fn open_release_page(&self) -> CmdResult<()> {
        let _g = self.lock();
        let s = self.store.load();
        let r = self.available(&s)?;
        if !allowed_release_url(&r.url, &Self::prefs(&s).repo) {
            return Err(AppError::new(
                ErrorCode::Invalid,
                "refusing to open that link",
            ));
        }
        let host = self.host.get().ok_or_else(no_window)?;
        host.open_url(&r.url).map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("cannot open the browser: {e}"))
        })
    }

    /// Download, verify and install the available version, then restart (AppImage, NSIS,
    /// MSI). Other installs get `invalid` with the reason; they use the release page.
    pub fn install(&self, enabled: bool) -> CmdResult<UpdateStatus> {
        if !enabled {
            return Err(network_off());
        }
        let (r, repo) = {
            let _g = self.lock();
            let s = self.store.load();
            (self.available(&s)?, Self::prefs(&s).repo)
        };
        if self.install.0 != UpdateInstall::InApp {
            return Err(AppError::new(ErrorCode::Invalid, self.install.1.clone()));
        }
        let host = self.host.get().ok_or_else(no_window)?;
        host.install(&latest_json_url(&repo, &r.tag), &r.version)
            .map_err(|e| AppError::new(ErrorCode::Internal, e))?;
        self.status(enabled)
    }
}

fn network_off() -> AppError {
    AppError::new(
        ErrorCode::NetworkOff,
        "Turn on \"Check for updates\" under Settings -> Network first.",
    )
}

fn no_window() -> AppError {
    AppError::new(ErrorCode::NotImplemented, "This needs the app window.")
}

/// The start-up check is due when there was none, or the last one is 24 h old (or the clock
/// went backwards).
pub fn due(last: Option<u64>, now: u64) -> bool {
    match last {
        None => true,
        Some(t) => now < t || now - t >= CHECK_EVERY_SECS,
    }
}

/// `v1.2.3`, `V1.2.3` or `1.2.3[-pre][+build]`.
pub fn parse_version(tag: &str) -> Option<Version> {
    let t = tag.trim();
    let t = t
        .strip_prefix('v')
        .or_else(|| t.strip_prefix('V'))
        .unwrap_or(t);
    Version::parse(t).ok()
}

/// `https://github.com/<repo>/releases/tag/<tag>`.
pub fn release_url(repo: &str, tag: &str) -> String {
    format!("https://github.com/{repo}/releases/tag/{tag}")
}

/// The updater manifest of one release (tauri-action uploads `latest.json`).
pub fn latest_json_url(repo: &str, tag: &str) -> String {
    format!("https://github.com/{repo}/releases/download/{tag}/latest.json")
}

/// Only release pages of the checked repo on github.com, over https.
pub fn allowed_release_url(url: &str, repo: &str) -> bool {
    let Some(path) = url.strip_prefix("https://github.com/") else {
        return false;
    };
    let Some(rest) = path
        .strip_prefix(repo)
        .and_then(|r| r.strip_prefix("/releases"))
    else {
        return false;
    };
    valid_repo(repo)
        && (rest.is_empty() || rest.starts_with('/'))
        && rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '-' | '_' | '+'))
        && !rest.contains("..")
}

#[derive(Debug, Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    published_at: Option<String>,
}

/// The tag characters that go into a URL path unchanged.
fn url_safe_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 128
        && tag
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
}

/// The newest release this install may move to, if newer than `current`.
fn pick<'a>(
    list: &'a [ApiRelease],
    current: &Version,
    include_prereleases: bool,
) -> Option<(Version, &'a ApiRelease)> {
    list.iter()
        .filter(|r| !r.draft && url_safe_tag(&r.tag_name))
        .filter_map(|r| parse_version(&r.tag_name).map(|v| (v, r)))
        .filter(|(v, r)| include_prereleases || (!r.prerelease && v.pre.is_empty()))
        .max_by(|a, b| a.0.cmp_precedence(&b.0))
        .filter(|(v, _)| v.cmp_precedence(current).is_gt())
}

fn failed(problem: UpdateProblem, repo: &str, status: Option<u16>) -> UpdateCheck {
    let message = match problem {
        UpdateProblem::NotVisible => format!(
            "Can't see releases for {repo}: the repo is private, renamed or has no public releases yet. Nothing is wrong with this install."
        ),
        UpdateProblem::RateLimited => {
            "GitHub's limit for checks without sign-in is used up for now. The next check will try again.".to_string()
        }
        UpdateProblem::Offline => {
            "Can't reach GitHub: you look offline. The next check will try again.".to_string()
        }
        UpdateProblem::BadResponse => format!(
            "GitHub's answer could not be read{}. The next check will try again.",
            status.map(|s| format!(" (HTTP {s})")).unwrap_or_default()
        ),
    };
    UpdateCheck::Failed { problem, message }
}

/// HTTP answer -> result. 404/401 = not visible (private repo, spec Q1); 403/429 with the
/// rate limit used up = rate limited; other 403 = not visible; anything else not 200 or not
/// a release list = bad response.
pub fn map_reply(reply: &Reply, prefs: &UpdatePrefs, current: &Version) -> UpdateCheck {
    let repo = prefs.repo.as_str();
    match reply.status {
        200 => {}
        401 | 404 => return failed(UpdateProblem::NotVisible, repo, None),
        403 | 429 if reply.rate_limit_remaining == Some(0) || reply.status == 429 => {
            return failed(UpdateProblem::RateLimited, repo, None);
        }
        403 => return failed(UpdateProblem::NotVisible, repo, None),
        s => return failed(UpdateProblem::BadResponse, repo, Some(s)),
    }
    let Ok(list) = serde_json::from_slice::<Vec<ApiRelease>>(&reply.body) else {
        return failed(UpdateProblem::BadResponse, repo, None);
    };
    match pick(&list, current, prefs.include_prereleases) {
        None => UpdateCheck::UpToDate,
        Some((v, r)) => {
            let (notes, notes_truncated) = notes::sanitize(r.body.as_deref().unwrap_or(""));
            let title = notes::sanitize(r.name.as_deref().unwrap_or(""))
                .0
                .into_iter()
                .next()
                .map(|b| b.text)
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| format!("StarOrg Strings {v}"));
            UpdateCheck::Available {
                release: ReleaseView {
                    version: v.to_string(),
                    tag: r.tag_name.clone(),
                    title,
                    url: release_url(repo, &r.tag_name),
                    published: r
                        .published_at
                        .clone()
                        .filter(|p| p.len() <= 40 && p.chars().all(|c| c.is_ascii_graphic())),
                    prerelease: r.prerelease || !v.pre.is_empty(),
                    notes,
                    notes_truncated,
                },
            }
        }
    }
}

#[cfg(test)]
mod tests;
