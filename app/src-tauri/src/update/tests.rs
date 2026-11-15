//! Update checker tests over a fake HTTP layer and clock: no network.

use super::*;
use crate::contract::types::NoteKind;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

const T0: u64 = 1_800_000_000;

#[derive(Clone, Default)]
struct Fake {
    reply: Arc<Mutex<Option<Result<Reply, String>>>>,
    calls: Arc<AtomicUsize>,
    repos: Arc<Mutex<Vec<String>>>,
}

impl Fake {
    fn set(&self, r: Result<Reply, String>) {
        *self.reply.lock().unwrap() = Some(r);
    }
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl ReleaseFetch for Fake {
    fn fetch(&self, repo: &str) -> Result<Reply, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.repos.lock().unwrap().push(repo.to_string());
        self.reply
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| Err("no reply scripted".into()))
    }
}

#[derive(Clone)]
struct TestClock(Arc<AtomicU64>);

impl Clock for TestClock {
    fn now(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

fn ok(json: &str) -> Result<Reply, String> {
    Ok(Reply {
        status: 200,
        rate_limit_remaining: Some(59),
        body: json.as_bytes().to_vec(),
    })
}

fn status(code: u16, remaining: Option<u32>) -> Result<Reply, String> {
    Ok(Reply {
        status: code,
        rate_limit_remaining: remaining,
        body: br#"{"message":"x"}"#.to_vec(),
    })
}

const LIST: &str = r####"[
  {"tag_name":"v0.4.0","name":"draft","body":"","draft":true,"prerelease":false},
  {"tag_name":"v0.3.0-beta.1","name":"Beta","body":"beta","draft":false,"prerelease":true,"published_at":"2026-11-21T10:00:00Z"},
  {"tag_name":"v0.2.0","name":"StarOrg Strings 0.2.0","body":"### Features\n* **app:** update check <script>x</script>","draft":false,"prerelease":false,"published_at":"2026-11-20T10:00:00Z","html_url":"https://evil.example/x"},
  {"tag_name":"v0.1.5","name":"","body":"","draft":false,"prerelease":false},
  {"tag_name":"not-a-version","draft":false,"prerelease":false},
  {"tag_name":"v9.9.9/../x","draft":false,"prerelease":false}
]"####;

fn checker(current: &str) -> (Updater, Fake, TestClock) {
    let fake = Fake::default();
    let clock = TestClock(Arc::new(AtomicU64::new(T0)));
    let u = Updater::new(
        Store::Memory(Box::default()),
        Box::new(fake.clone()),
        Box::new(clock.clone()),
        Version::parse(current).unwrap(),
        (UpdateInstall::InApp, "test".into()),
    );
    (u, fake, clock)
}

fn available(s: &UpdateStatus) -> Option<&ReleaseView> {
    match &s.check {
        UpdateCheck::Available { release } => Some(release),
        _ => None,
    }
}

fn prefs(repo: &str, pre: bool) -> UpdatePrefs {
    UpdatePrefs {
        repo: repo.into(),
        include_prereleases: pre,
    }
}

#[test]
fn semver_compare_and_prerelease_rules() {
    let cur = Version::parse("0.1.0").unwrap();
    let r = |pre| map_reply(&ok(LIST).unwrap(), &prefs(DEFAULT_REPO, pre), &cur);
    // Stable only: 0.2.0 (draft 0.4.0, prerelease 0.3.0-beta.1 and junk tags ignored).
    let UpdateCheck::Available { release } = r(false) else {
        panic!()
    };
    assert_eq!(release.version, "0.2.0");
    assert_eq!(release.tag, "v0.2.0");
    assert!(!release.prerelease);
    // URL is built from the repo and tag, never the response's html_url.
    assert_eq!(
        release.url,
        "https://github.com/voidput/starorg-strings/releases/tag/v0.2.0"
    );
    assert_eq!(release.title, "StarOrg Strings 0.2.0");
    assert_eq!(release.published.as_deref(), Some("2026-11-20T10:00:00Z"));
    assert_eq!(release.notes[0].kind, NoteKind::Heading);
    assert_eq!(release.notes[1].text, "app: update check x");
    // With pre-releases: the beta is newer.
    let UpdateCheck::Available { release } = r(true) else {
        panic!()
    };
    assert_eq!(release.version, "0.3.0-beta.1");
    assert!(release.prerelease);

    // A prerelease tag not flagged as one on GitHub still counts as a prerelease.
    let unflagged = r#"[{"tag_name":"v0.2.0-rc.1","draft":false,"prerelease":false}]"#;
    assert_eq!(
        map_reply(&ok(unflagged).unwrap(), &prefs(DEFAULT_REPO, false), &cur),
        UpdateCheck::UpToDate
    );
    // Current is a prerelease: the final release of the same version is newer.
    let dev = Version::parse("0.2.0-dev0003").unwrap();
    let only = r#"[{"tag_name":"v0.2.0","draft":false,"prerelease":false}]"#;
    assert!(matches!(
        map_reply(&ok(only).unwrap(), &prefs(DEFAULT_REPO, false), &dev),
        UpdateCheck::Available { .. }
    ));
    // Same or older: up to date. Build metadata does not make a version newer.
    for cur in ["0.2.0", "0.2.0+build.7", "1.0.0"] {
        assert_eq!(
            map_reply(
                &ok(only).unwrap(),
                &prefs(DEFAULT_REPO, false),
                &Version::parse(cur).unwrap()
            ),
            UpdateCheck::UpToDate,
            "{cur}"
        );
    }
    let built = r#"[{"tag_name":"v0.2.0+linux","draft":false,"prerelease":false}]"#;
    assert_eq!(
        map_reply(
            &ok(built).unwrap(),
            &prefs(DEFAULT_REPO, false),
            &Version::parse("0.2.0").unwrap()
        ),
        UpdateCheck::UpToDate
    );
    // Numeric, not string, order: 0.10.0 > 0.9.0.
    let order = r#"[{"tag_name":"v0.9.0","draft":false,"prerelease":false},{"tag_name":"0.10.0","draft":false,"prerelease":false}]"#;
    let UpdateCheck::Available { release } =
        map_reply(&ok(order).unwrap(), &prefs(DEFAULT_REPO, false), &cur)
    else {
        panic!()
    };
    assert_eq!(release.version, "0.10.0");
    assert_eq!(parse_version("V1.2.3"), Version::parse("1.2.3").ok());
    assert_eq!(parse_version("release-1"), None);
}

#[test]
fn error_mapping() {
    let cur = Version::parse("0.1.0").unwrap();
    let p = prefs("someone/private-repo", false);
    let problem = |r: Result<Reply, String>| match map_reply(&r.unwrap(), &p, &cur) {
        UpdateCheck::Failed { problem, message } => (problem, message),
        other => panic!("{other:?}"),
    };
    let (pr, msg) = problem(status(404, Some(59)));
    assert_eq!(pr, UpdateProblem::NotVisible);
    assert!(
        msg.starts_with("Can't see releases for someone/private-repo: the repo is private"),
        "{msg}"
    );
    assert_eq!(problem(status(401, None)).0, UpdateProblem::NotVisible);
    assert_eq!(problem(status(403, Some(0))).0, UpdateProblem::RateLimited);
    assert_eq!(problem(status(429, None)).0, UpdateProblem::RateLimited);
    assert_eq!(problem(status(403, Some(12))).0, UpdateProblem::NotVisible);
    let (pr, msg) = problem(status(502, None));
    assert_eq!(pr, UpdateProblem::BadResponse);
    assert!(msg.contains("HTTP 502"), "{msg}");
    assert_eq!(problem(ok("<html>")).0, UpdateProblem::BadResponse);
    assert_eq!(
        problem(ok(r#"{"not":"a list"}"#)).0,
        UpdateProblem::BadResponse
    );
    // Too large (starorg-net reports status 0 with no body).
    assert_eq!(problem(status(0, None)).0, UpdateProblem::BadResponse);
    assert_eq!(
        map_reply(&ok("[]").unwrap(), &p, &cur),
        UpdateCheck::UpToDate
    );

    // No HTTP answer: offline, through the checker.
    let (u, fake, _) = checker("0.1.0");
    fake.set(Err("dns error".into()));
    let s = u.check(true, true).unwrap();
    let UpdateCheck::Failed { problem, message } = &s.check else {
        panic!()
    };
    assert_eq!(*problem, UpdateProblem::Offline);
    assert!(message.contains("offline"));
    assert!(
        !message.contains("dns"),
        "transport details stay out of the UI text"
    );
}

#[test]
fn nothing_is_requested_while_the_switch_is_off() {
    let (u, fake, _) = checker("0.1.0");
    fake.set(ok(LIST));
    let s = u.check(false, false).unwrap();
    assert_eq!(s.check, UpdateCheck::NotChecked);
    assert!(!s.enabled);
    assert_eq!(
        u.check(false, true).unwrap_err().code,
        ErrorCode::NetworkOff
    );
    assert_eq!(u.install(false).unwrap_err().code, ErrorCode::NetworkOff);
    u.status(false).unwrap();
    u.skip(false, "0.2.0".into()).unwrap();
    u.set_prefs(false, prefs("a/b", true)).unwrap();
    assert_eq!(fake.calls(), 0);
}

#[test]
fn start_up_check_runs_at_most_once_per_24_hours() {
    let (u, fake, clock) = checker("0.1.0");
    fake.set(ok(LIST));
    let s = u.check(true, false).unwrap();
    assert_eq!(fake.calls(), 1);
    assert_eq!(available(&s).unwrap().version, "0.2.0");
    assert_eq!(
        s.last_checked.as_deref(),
        Some(starorg_core::fsutil::rfc3339(T0).as_str())
    );
    // Restarts within 24 h: the saved result, no request.
    clock.0.store(T0 + CHECK_EVERY_SECS - 1, Ordering::SeqCst);
    let again = u.check(true, false).unwrap();
    assert_eq!(fake.calls(), 1);
    assert_eq!(again.check, s.check);
    // "Check now" always asks.
    u.check(true, true).unwrap();
    assert_eq!(fake.calls(), 2);
    // 24 h after the last request: due again.
    let last = T0 + CHECK_EVERY_SECS - 1;
    clock.0.store(last + CHECK_EVERY_SECS, Ordering::SeqCst);
    u.check(true, false).unwrap();
    assert_eq!(fake.calls(), 3);
    // A failed check counts too (no retry storm on every start while offline).
    fake.set(Err("offline".into()));
    clock.0.store(last + 3 * CHECK_EVERY_SECS, Ordering::SeqCst);
    u.check(true, false).unwrap();
    u.check(true, false).unwrap();
    assert_eq!(fake.calls(), 4);
    // A clock that went backwards does not block checks forever.
    assert!(due(Some(T0), T0 - 10));
    assert!(due(None, 0));
    assert!(!due(Some(T0), T0));
}

#[test]
fn skip_this_version_hides_only_that_version() {
    let (u, fake, _) = checker("0.1.0");
    fake.set(ok(LIST));
    u.check(true, true).unwrap();
    let s = u.skip(true, "v0.2.0".into()).unwrap();
    assert_eq!(s.skipped_version.as_deref(), Some("0.2.0"));
    // Still reported as available (Settings shows it); the banner compares with the skip.
    assert_eq!(available(&s).unwrap().version, "0.2.0");
    // A newer release is not skipped.
    fake.set(ok(
        r#"[{"tag_name":"v0.2.1","draft":false,"prerelease":false}]"#,
    ));
    let s = u.check(true, true).unwrap();
    assert_eq!(available(&s).unwrap().version, "0.2.1");
    assert_ne!(s.skipped_version.as_deref(), Some("0.2.1"));
    assert_eq!(
        u.skip(true, "junk".into()).unwrap_err().code,
        ErrorCode::Invalid
    );
    assert_eq!(u.skip(true, String::new()).unwrap().skipped_version, None);
}

#[test]
fn prefs_change_the_repo_and_reset_the_last_result() {
    let (u, fake, _) = checker("0.1.0");
    fake.set(ok(LIST));
    u.check(true, false).unwrap();
    let s = u
        .set_prefs(
            true,
            prefs(
                "https://github.com/mirror-org/starorg-strings-releases/",
                false,
            ),
        )
        .unwrap();
    assert_eq!(s.prefs.repo, "mirror-org/starorg-strings-releases");
    assert_eq!(s.check, UpdateCheck::NotChecked);
    assert_eq!(s.last_checked, None);
    // So the start-up check runs again, against the new repo.
    u.check(true, false).unwrap();
    assert_eq!(
        fake.repos.lock().unwrap().as_slice(),
        [
            "voidput/starorg-strings",
            "mirror-org/starorg-strings-releases"
        ]
    );
    for bad in ["", "nope", "a/b/c", "a b/c", "../x/y"] {
        assert_eq!(
            u.set_prefs(true, prefs(bad, false)).unwrap_err().code,
            ErrorCode::Invalid,
            "{bad}"
        );
    }
    // Same prefs again keep the result.
    let s = u
        .set_prefs(true, prefs("mirror-org/starorg-strings-releases", false))
        .unwrap();
    assert!(available(&s).is_some());
}

#[test]
fn after_updating_the_saved_result_is_up_to_date() {
    let fake = Fake::default();
    fake.set(ok(LIST));
    let store = Arc::new(Mutex::new(Saved::default()));
    let mk = |cur: &str| {
        let saved = store.lock().unwrap().clone();
        Updater::new(
            Store::Memory(Box::new(Mutex::new(saved))),
            Box::new(fake.clone()),
            Box::new(TestClock(Arc::new(AtomicU64::new(T0)))),
            Version::parse(cur).unwrap(),
            (UpdateInstall::DownloadOnly, "x".into()),
        )
    };
    let u = mk("0.1.0");
    u.check(true, true).unwrap();
    *store.lock().unwrap() = u.store.load();
    // Same saved state, newer running version.
    assert_eq!(
        mk("0.2.0").status(true).unwrap().check,
        UpdateCheck::UpToDate
    );
}

#[test]
fn saved_state_is_a_file_and_survives_restarts() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("app-update.json");
    let fake = Fake::default();
    fake.set(ok(LIST));
    let mk = || {
        Updater::new(
            Store::File(path.clone()),
            Box::new(fake.clone()),
            Box::new(TestClock(Arc::new(AtomicU64::new(T0)))),
            Version::parse("0.1.0").unwrap(),
            (UpdateInstall::DownloadOnly, "x".into()),
        )
    };
    mk().check(true, false).unwrap();
    mk().skip(true, "0.2.0".into()).unwrap();
    let s = mk().check(true, false).unwrap();
    assert_eq!(fake.calls(), 1, "second start within 24 h made a request");
    assert_eq!(s.skipped_version.as_deref(), Some("0.2.0"));
    assert!(available(&s).is_some());
    // A broken file is a fresh start.
    std::fs::write(&path, b"{not json").unwrap();
    assert_eq!(mk().status(true).unwrap().check, UpdateCheck::NotChecked);
}

#[derive(Default)]
struct RecordingHost {
    opened: Mutex<Vec<String>>,
    installs: Mutex<Vec<(String, String)>>,
}

struct HostRef(Arc<RecordingHost>);

impl UpdateHost for HostRef {
    fn open_url(&self, url: &str) -> Result<(), String> {
        self.0.opened.lock().unwrap().push(url.into());
        Ok(())
    }
    fn install(&self, manifest_url: &str, version: &str) -> Result<(), String> {
        self.0
            .installs
            .lock()
            .unwrap()
            .push((manifest_url.into(), version.into()));
        Ok(())
    }
}

#[test]
fn release_page_and_install_use_the_checked_release_only() {
    let (u, fake, _) = checker("0.1.0");
    let rec = Arc::new(RecordingHost::default());
    // No window yet: a clear refusal, nothing opened.
    fake.set(ok(LIST));
    assert_eq!(u.open_release_page().unwrap_err().code, ErrorCode::NotFound);
    u.check(true, true).unwrap();
    assert_eq!(
        u.open_release_page().unwrap_err().code,
        ErrorCode::NotImplemented
    );
    u.host().set(HostRef(rec.clone()));
    u.open_release_page().unwrap();
    u.install(true).unwrap();
    assert_eq!(
        rec.opened.lock().unwrap().as_slice(),
        ["https://github.com/voidput/starorg-strings/releases/tag/v0.2.0"]
    );
    assert_eq!(
        rec.installs.lock().unwrap().as_slice(),
        [(
            "https://github.com/voidput/starorg-strings/releases/download/v0.2.0/latest.json"
                .to_string(),
            "0.2.0".to_string()
        )]
    );

    // Download-only installs refuse with their reason.
    let d = Updater::new(
        Store::Memory(Box::default()),
        Box::new(fake.clone()),
        Box::new(SystemClock),
        Version::parse("0.1.0").unwrap(),
        (
            UpdateInstall::DownloadOnly,
            "Use your package manager.".into(),
        ),
    );
    d.check(true, true).unwrap();
    let e = d.install(true).unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid);
    assert_eq!(e.message, "Use your package manager.");
}

#[test]
fn only_github_release_pages_of_the_repo_open() {
    let repo = "voidput/starorg-strings";
    assert!(allowed_release_url(
        "https://github.com/voidput/starorg-strings/releases/tag/v0.2.0",
        repo
    ));
    assert!(allowed_release_url(
        "https://github.com/voidput/starorg-strings/releases",
        repo
    ));
    for bad in [
        "http://github.com/voidput/starorg-strings/releases/tag/v1",
        "https://github.com.evil.example/voidput/starorg-strings/releases",
        "https://evil.example/voidput/starorg-strings/releases",
        "https://github.com/voidput/starorg-strings-other/releases",
        "https://github.com/voidput/starorg-strings/releasesx",
        "https://github.com/voidput/starorg-strings/releases/../../../x",
        "https://github.com/voidput/starorg-strings/releases/tag/v1?x=<script>",
        "https://github.com/voidput/starorg-strings/releases/tag/v1#frag",
        "javascript:alert(1)",
        "",
    ] {
        assert!(!allowed_release_url(bad, repo), "{bad}");
    }
    assert!(!allowed_release_url(
        "https://github.com/a/b/releases",
        "a/b/c"
    ));
}

#[test]
fn user_agent_is_name_and_version_only() {
    assert_eq!(
        user_agent(),
        format!("StarOrg-Strings/{}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn store_builds_never_request_or_install() {
    let fake = Fake::default();
    fake.set(ok(LIST));
    let rec = Arc::new(RecordingHost::default());
    let u = Updater::new(
        Store::Memory(Box::default()),
        Box::new(fake.clone()),
        Box::new(SystemClock),
        Version::parse("0.1.0").unwrap(),
        (UpdateInstall::Store, host::STORE_NOTE.into()),
    );
    u.host().set(HostRef(rec.clone()));
    // Start-up check with the switch on: nothing requested, nothing to offer.
    let s = u.check(true, false).unwrap();
    assert_eq!(s.install, UpdateInstall::Store);
    assert_eq!(s.install_note, "Updates come from the Microsoft Store.");
    assert_eq!(s.check, UpdateCheck::NotChecked);
    // "Check now" and "Install" are refused with the Store note.
    let e = u.check(true, true).unwrap_err();
    assert_eq!(
        (e.code, e.message.as_str()),
        (ErrorCode::Invalid, host::STORE_NOTE)
    );
    let e = u.install(true).unwrap_err();
    assert_eq!(
        (e.code, e.message.as_str()),
        (ErrorCode::Invalid, host::STORE_NOTE)
    );
    assert_eq!(u.install(false).unwrap_err().message, host::STORE_NOTE);
    assert_eq!(fake.calls(), 0, "a Store build made a release request");
    assert!(
        rec.installs.lock().unwrap().is_empty(),
        "a Store build ran the updater"
    );
    assert!(rec.opened.lock().unwrap().is_empty());
}
