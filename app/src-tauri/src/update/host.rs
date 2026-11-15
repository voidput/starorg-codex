//! The desktop side of updates: opening the release page and installing a signed update.
//!
//! Like the native dialogs ([`crate::contract::real::DialogSlot`]), the backend exists before
//! the Tauri app, so the implementation is put into a [`HostSlot`] in the app's `setup` hook.
//! Without one (tests, the browser mock) these calls report that they need the window.

use crate::contract::types::UpdateInstall;
use std::sync::{Arc, OnceLock};

/// The updater's minisign public key, from `app/src-tauri/updater-public-key.txt`.
///
/// The committed file holds a placeholder until the owner runs `npx tauri signer generate`
/// and pastes the public key there (docs/updates.md). With the placeholder, every install is
/// "download only": nothing can be installed in-app without a key to verify it.
pub const UPDATER_PUBKEY: &str = include_str!("../../updater-public-key.txt");

/// The committed placeholder (also what `tauri.conf.json` carries until release.yml merges
/// the real key in).
#[cfg(test)]
const PLACEHOLDER: &str = "REPLACE-WITH-THE-PUBLIC-KEY-FROM-npx-tauri-signer-generate";

/// A real `tauri signer generate` public key: base64 of the minisign `.pub` text, which
/// starts with "untrusted comment:".
pub fn pubkey_is_real(key: &str) -> bool {
    let k = key.trim();
    k.starts_with("dW50cnVzdGVkIGNvbW1lbnQ6")
        && k.len() >= 100
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='))
}

pub trait UpdateHost: Send + Sync {
    /// Open an already checked https://github.com URL in the system browser.
    fn open_url(&self, url: &str) -> Result<(), String>;
    /// Fetch `manifest_url` (`latest.json`), verify the signature against [`UPDATER_PUBKEY`],
    /// install `version` and restart. Blocking; on success it normally does not return.
    fn install(&self, manifest_url: &str, version: &str) -> Result<(), String>;
}

#[derive(Default, Clone)]
pub struct HostSlot(Arc<OnceLock<Box<dyn UpdateHost>>>);

impl HostSlot {
    pub fn set(&self, h: impl UpdateHost + 'static) {
        let _ = self.0.set(Box::new(h));
    }

    pub fn get(&self) -> Option<&dyn UpdateHost> {
        self.0.get().map(|b| b.as_ref())
    }
}

/// What this copy can do, from the bundle type Tauri stamps into the binary at bundling.
pub fn install_support() -> (UpdateInstall, String) {
    support_for(
        tauri::utils::platform::bundle_type(),
        pubkey_is_real(UPDATER_PUBKEY),
    )
    .map_or_else(
        |why| (UpdateInstall::DownloadOnly, why),
        |why| (UpdateInstall::InApp, why),
    )
}

/// `Ok` = in-app, `Err` = download only; the text says why.
fn support_for(
    bundle: Option<tauri::utils::config::BundleType>,
    has_key: bool,
) -> Result<String, String> {
    use tauri::utils::config::BundleType as B;
    match bundle {
        Some(B::AppImage | B::Nsis | B::Msi) if has_key => Ok(
            "Updates are downloaded, checked against the StarOrg Strings signing key and installed when you choose."
                .into(),
        ),
        Some(B::AppImage | B::Nsis | B::Msi) => Err(
            "This build has no updater key, so new versions are downloaded from the release page."
                .into(),
        ),
        Some(B::Deb | B::Rpm) => Err(
            "Installed as a .deb or .rpm package: update it with your package manager or from the release page."
                .into(),
        ),
        _ => Err(
            "This copy (portable, or built from source) is updated by downloading the new release."
                .into(),
        ),
    }
}

/// The real host, over the app handle.
pub struct TauriUpdateHost<R: tauri::Runtime>(pub tauri::AppHandle<R>);

impl<R: tauri::Runtime> UpdateHost for TauriUpdateHost<R> {
    fn open_url(&self, url: &str) -> Result<(), String> {
        open::that_detached(url).map_err(|e| e.to_string())
    }

    fn install(&self, manifest_url: &str, version: &str) -> Result<(), String> {
        use tauri_plugin_updater::UpdaterExt;
        let url: tauri::Url = manifest_url.parse().map_err(|e| format!("{e}"))?;
        let updater = self
            .0
            .updater_builder()
            .endpoints(vec![url])
            .map_err(|e| e.to_string())?
            .header("User-Agent", super::user_agent())
            .map_err(|e| e.to_string())?
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| e.to_string())?;
        let want = version.to_string();
        tauri::async_runtime::block_on(async move {
            let Some(u) = updater.check().await.map_err(|e| e.to_string())? else {
                return Err(
                    "The release has no installer for this system. Download it from the release page."
                        .to_string(),
                );
            };
            if u.version != want {
                return Err(format!(
                    "The release offers {} instead of {want}; not installing.",
                    u.version
                ));
            }
            // Verifies the signature against the configured key before installing.
            u.download_and_install(|_, _| {}, || {})
                .await
                .map_err(|e| e.to_string())
        })?;
        self.0.restart()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::utils::config::BundleType as B;

    #[test]
    fn only_appimage_and_windows_installers_with_a_key_install_in_app() {
        for b in [B::AppImage, B::Nsis, B::Msi] {
            assert!(support_for(Some(b.clone()), true).is_ok(), "{b:?}");
            assert!(
                support_for(Some(b.clone()), false).is_err(),
                "{b:?} without key"
            );
        }
        for b in [Some(B::Deb), Some(B::Rpm), None] {
            assert!(support_for(b.clone(), true).is_err(), "{b:?}");
        }
    }

    #[test]
    fn placeholders_are_not_keys() {
        assert!(!pubkey_is_real(PLACEHOLDER));
        assert!(!pubkey_is_real(""));
        let real = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDEyMzQ1Njc4OTBBQkNERUYKUldTN2Z3RUF4eXo2cExEbUVnPT0KUldTN2Z3RUF4eXo2cExEbUVnPT0K";
        assert!(pubkey_is_real(real));
        assert!(!pubkey_is_real("dW50cnVzdGVkIGNvbW1lbnQ6 short"));
    }
}
