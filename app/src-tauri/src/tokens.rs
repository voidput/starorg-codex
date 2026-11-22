//! Where the app's secrets live: the StarOrg Tools sign-in token (addendum §5, one per
//! environment) and the user's UEX API token. The OS keyring (Windows Credential Manager,
//! the Secret Service on Linux), or, when there is none, an owner-only file in the app's
//! config folder. Never in `app.json`, `config.toml`, the log or diagnostics, and never
//! returned to the webview.

use crate::contract::types::{StarOrgEnv, TokenStoreKind};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Keyring service name; the entry's user is `starorg-tools-<environment>` or `uex`.
pub const SERVICE: &str = "starorg-codex";

/// Which secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// The StarOrg Tools token of an environment.
    StarOrg(StarOrgEnv),
    /// The UEX API token (Settings -> Network -> "Download UEX market data").
    Uex,
}

impl From<StarOrgEnv> for Slot {
    fn from(e: StarOrgEnv) -> Self {
        Self::StarOrg(e)
    }
}

impl Slot {
    /// File name stem: `production`, `dev`, `uex`.
    fn name(self) -> &'static str {
        match self {
            Self::StarOrg(StarOrgEnv::Production) => "production",
            Self::StarOrg(StarOrgEnv::Dev) => "dev",
            Self::Uex => "uex",
        }
    }

    fn keyring_user(self) -> String {
        match self {
            Self::StarOrg(_) => format!("starorg-tools-{}", self.name()),
            Self::Uex => "uex".into(),
        }
    }
}

pub trait TokenStore: Send + Sync {
    fn get(&self, slot: Slot) -> Result<Option<(String, TokenStoreKind)>, String>;
    /// Returns where it was put.
    fn set(&self, slot: Slot, token: &str) -> Result<TokenStoreKind, String>;
    /// Gone from every place it could be.
    fn delete(&self, slot: Slot) -> Result<(), String>;
}

/// Owner-only files `<dir>/<slot>.token` (0600 in a 0700 folder on Unix; on Windows the
/// per-user app folder's ACL).
pub struct FileStore {
    dir: PathBuf,
}

impl FileStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn path(&self, slot: Slot) -> PathBuf {
        self.dir.join(format!("{}.token", slot.name()))
    }
}

#[cfg(unix)]
fn write_private(dir: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path)
}

#[cfg(not(unix))]
fn write_private(dir: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    starorg_core::fsutil::write_atomic(path, bytes)
}

impl TokenStore for FileStore {
    fn get(&self, slot: Slot) -> Result<Option<(String, TokenStoreKind)>, String> {
        match std::fs::read_to_string(self.path(slot)) {
            Ok(t) if !t.trim().is_empty() => Ok(Some((t.trim().to_string(), TokenStoreKind::File))),
            Ok(_) => Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("token file: {e}")),
        }
    }

    fn set(&self, slot: Slot, token: &str) -> Result<TokenStoreKind, String> {
        write_private(&self.dir, &self.path(slot), token.as_bytes())
            .map_err(|e| format!("token file: {e}"))?;
        Ok(TokenStoreKind::File)
    }

    fn delete(&self, slot: Slot) -> Result<(), String> {
        match std::fs::remove_file(self.path(slot)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("token file: {e}")),
        }
    }
}

/// The OS keyring first, the file when the keyring cannot be used.
pub struct KeyringOrFile {
    file: FileStore,
}

impl KeyringOrFile {
    pub fn new(file_dir: impl Into<PathBuf>) -> Self {
        Self {
            file: FileStore::new(file_dir),
        }
    }

    fn entry(slot: Slot) -> Result<keyring::Entry, keyring::Error> {
        keyring::Entry::new(SERVICE, &slot.keyring_user())
    }
}

impl TokenStore for KeyringOrFile {
    fn get(&self, slot: Slot) -> Result<Option<(String, TokenStoreKind)>, String> {
        if let Ok(e) = Self::entry(slot)
            && let Ok(t) = e.get_password()
            && !t.is_empty()
        {
            return Ok(Some((t, TokenStoreKind::Keyring)));
        }
        self.file.get(slot)
    }

    fn set(&self, slot: Slot, token: &str) -> Result<TokenStoreKind, String> {
        if let Ok(e) = Self::entry(slot)
            && e.set_password(token).is_ok()
            // Some Secret Service setups accept a write and lose it; read it back.
            && e.get_password().is_ok_and(|t| t == token)
        {
            let _ = self.file.delete(slot);
            return Ok(TokenStoreKind::Keyring);
        }
        self.file.set(slot, token)
    }

    fn delete(&self, slot: Slot) -> Result<(), String> {
        if let Ok(e) = Self::entry(slot) {
            match e.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                // No keyring at all: nothing can be there.
                Err(keyring::Error::NoStorageAccess(_))
                | Err(keyring::Error::PlatformFailure(_)) => {}
                Err(err) => return Err(format!("keyring: {err}")),
            }
        }
        self.file.delete(slot)
    }
}

/// In memory (tests, and the browser mock).
#[derive(Default)]
pub struct MemoryStore(Mutex<Vec<(Slot, String)>>);

impl TokenStore for MemoryStore {
    fn get(&self, slot: Slot) -> Result<Option<(String, TokenStoreKind)>, String> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .find(|(e, _)| *e == slot)
            .map(|(_, t)| (t.clone(), TokenStoreKind::Keyring)))
    }

    fn set(&self, slot: Slot, token: &str) -> Result<TokenStoreKind, String> {
        let mut v = self.0.lock().unwrap_or_else(|p| p.into_inner());
        v.retain(|(e, _)| *e != slot);
        v.push((slot, token.to_string()));
        Ok(TokenStoreKind::Keyring)
    }

    fn delete(&self, slot: Slot) -> Result<(), String> {
        self.0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .retain(|(e, _)| *e != slot);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEV: Slot = Slot::StarOrg(StarOrgEnv::Dev);
    const PROD: Slot = Slot::StarOrg(StarOrgEnv::Production);

    #[test]
    fn file_store_is_per_environment_and_owner_only() {
        let t = tempfile::tempdir().unwrap();
        let s = FileStore::new(t.path().join("starorg-tokens"));
        assert_eq!(s.get(DEV).unwrap(), None);
        let secret = format!("value-{}", std::process::id());
        assert_eq!(s.set(DEV, &secret).unwrap(), TokenStoreKind::File);
        assert_eq!(
            s.get(DEV).unwrap(),
            Some((secret.clone(), TokenStoreKind::File))
        );
        assert_eq!(s.get(PROD).unwrap(), None);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&s.path(DEV)), 0o600);
            assert_eq!(mode(&t.path().join("starorg-tokens")), 0o700);
        }
        s.delete(DEV).unwrap();
        s.delete(DEV).unwrap();
        assert_eq!(s.get(DEV).unwrap(), None);
    }

    #[test]
    fn memory_store_replaces_per_environment() {
        let s = MemoryStore::default();
        s.set(PROD, "a").unwrap();
        s.set(PROD, "b").unwrap();
        s.set(DEV, "c").unwrap();
        assert_eq!(s.get(PROD).unwrap().unwrap().0, "b");
        s.delete(PROD).unwrap();
        assert!(s.get(PROD).unwrap().is_none());
        assert!(s.get(DEV).unwrap().is_some());
    }

    #[test]
    fn the_uex_token_has_its_own_file_and_keyring_entry() {
        let t = tempfile::tempdir().unwrap();
        let s = FileStore::new(t.path());
        assert_eq!(s.path(Slot::Uex), t.path().join("uex.token"));
        assert_eq!(Slot::Uex.keyring_user(), "uex");
        assert_eq!(DEV.keyring_user(), "starorg-tools-dev");
        s.set(Slot::Uex, "u").unwrap();
        s.set(PROD, "p").unwrap();
        s.delete(Slot::Uex).unwrap();
        assert_eq!(s.get(Slot::Uex).unwrap(), None);
        assert_eq!(s.get(PROD).unwrap().unwrap().0, "p");
    }
}
