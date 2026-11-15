//! The mock backends' update checker: scripted releases, a fixed clock and version, no
//! network. `app/src/lib/contract/mock.ts` uses the same fixture (`MOCK_RELEASES`).

use super::*;

/// The mock app's version: fixed (not the crate's), so the samples stay the same across
/// version bumps.
pub const MOCK_VERSION: &str = "0.1.0";
/// The mock's "now" (mock.rs `NOW`, 2026-11-14T21:30:05Z).
pub const MOCK_NOW: u64 = 1_794_691_805;

/// A draft, a pre-release, the newest stable release with release-please notes (and some
/// markup that must not survive), an older one.
pub const MOCK_RELEASES: &str = r####"[
  {"tag_name":"v0.3.0","name":"draft","body":"not published","draft":true,"prerelease":false},
  {"tag_name":"v0.2.1-beta.1","name":"StarOrg Codex 0.2.1 beta 1","body":"### Bug Fixes\n\n* **app:** beta fix","draft":false,"prerelease":true,"published_at":"2026-11-13T18:00:00Z"},
  {"tag_name":"v0.2.0","name":"StarOrg Codex 0.2.0","body":"## [0.2.0](https://github.com/voidput/starorg-codex/compare/v0.1.0...v0.2.0) (2026-11-12)\n\n### Features\n\n* **app:** check for updates from GitHub releases ([#15](https://github.com/voidput/starorg-codex/issues/15))\n* **app:** install signed updates on AppImage and Windows <img src=x onerror=alert(1)>\n\n### Bug Fixes\n\n* **core:** keep the `user.cfg` order when re-applying","draft":false,"prerelease":false,"published_at":"2026-11-12T16:20:00Z"},
  {"tag_name":"v0.1.0","name":"StarOrg Codex 0.1.0","body":"First release.","draft":false,"prerelease":false,"published_at":"2026-11-01T12:00:00Z"}
]"####;

struct Scripted;

impl ReleaseFetch for Scripted {
    fn fetch(&self, repo: &str) -> Result<Reply, String> {
        // Any repo other than the default looks private, so the error state can be seen.
        Ok(if repo == DEFAULT_REPO {
            Reply {
                status: 200,
                rate_limit_remaining: Some(59),
                body: MOCK_RELEASES.as_bytes().to_vec(),
            }
        } else {
            Reply {
                status: 404,
                rate_limit_remaining: Some(58),
                body: br#"{"message":"Not Found"}"#.to_vec(),
            }
        })
    }
}

struct Fixed;

impl Clock for Fixed {
    fn now(&self) -> u64 {
        MOCK_NOW
    }
}

/// Opening is a no-op; installing is refused (nothing to install in a mock).
struct MockHost;

impl UpdateHost for MockHost {
    fn open_url(&self, _url: &str) -> Result<(), String> {
        Ok(())
    }
    fn install(&self, _manifest_url: &str, _version: &str) -> Result<(), String> {
        Err("The mock backend does not install updates.".into())
    }
}

pub fn updater() -> Updater {
    let u = Updater::new(
        Store::Memory(Box::default()),
        Box::new(Scripted),
        Box::new(Fixed),
        Version::parse(MOCK_VERSION).expect("semver"),
        (
            UpdateInstall::InApp,
            "Mock data: updates would be downloaded, checked against the signing key and installed when you choose.".into(),
        ),
    );
    u.host().set(MockHost);
    u
}
