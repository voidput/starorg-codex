//! Where the StarOrg Tools sign-in token lives (addendum §5): the OS keyring (Windows
//! Credential Manager, the Secret Service on Linux), or, when there is none, an owner-only
//! file in the app's config folder. One token per environment; never in `app.json`,
//! `config.toml`, the log or diagnostics.

use crate::contract::types::{StarOrgEnv, TokenStoreKind};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Keyring service name; the entry's user is `starorg-tools-<environment>`.
pub const SERVICE: &str = "starorg-codex";

fn env_name(env: StarOrgEnv) -> &'static str {
    match env {
        StarOrgEnv::Production => "production",
        StarOrgEnv::Dev => "dev",
    }
}

pub trait TokenStore: Send + Sync {
    fn get(&self, env: StarOrgEnv) -> Result<Option<(String, TokenStoreKind)>, String>;
    /// Returns where it was put.
    fn set(&self, env: StarOrgEnv, token: &str) -> Result<TokenStoreKind, String>;
    /// Gone from every place it could be.
    fn delete(&self, env: StarOrgEnv) -> Result<(), String>;
}

/// Owner-only files `<dir>/<environment>.token` (0600 in a 0700 folder on Unix; on Windows
/// the per-user app folder's ACL).
pub struct FileStore {
    dir: PathBuf,
}

impl FileStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn path(&self, env: StarOrgEnv) -> PathBuf {
        self.dir.join(format!("{}.token", env_name(env)))
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
    fn get(&self, env: StarOrgEnv) -> Result<Option<(String, TokenStoreKind)>, String> {
        match std::fs::read_to_string(self.path(env)) {
            Ok(t) if !t.trim().is_empty() => Ok(Some((t.trim().to_string(), TokenStoreKind::File))),
            Ok(_) => Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("token file: {e}")),
        }
    }

    fn set(&self, env: StarOrgEnv, token: &str) -> Result<TokenStoreKind, String> {
        write_private(&self.dir, &self.path(env), token.as_bytes())
            .map_err(|e| format!("token file: {e}"))?;
        Ok(TokenStoreKind::File)
    }

    fn delete(&self, env: StarOrgEnv) -> Result<(), String> {
        match std::fs::remove_file(self.path(env)) {
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

    fn entry(env: StarOrgEnv) -> Result<keyring::Entry, keyring::Error> {
        keyring::Entry::new(SERVICE, &format!("starorg-tools-{}", env_name(env)))
    }
}

impl TokenStore for KeyringOrFile {
    fn get(&self, env: StarOrgEnv) -> Result<Option<(String, TokenStoreKind)>, String> {
        if let Ok(e) = Self::entry(env)
            && let Ok(t) = e.get_password()
            && !t.is_empty()
        {
            return Ok(Some((t, TokenStoreKind::Keyring)));
        }
        self.file.get(env)
    }

    fn set(&self, env: StarOrgEnv, token: &str) -> Result<TokenStoreKind, String> {
        if let Ok(e) = Self::entry(env)
            && e.set_password(token).is_ok()
            // Some Secret Service setups accept a write and lose it; read it back.
            && e.get_password().is_ok_and(|t| t == token)
        {
            let _ = self.file.delete(env);
            return Ok(TokenStoreKind::Keyring);
        }
        self.file.set(env, token)
    }

    fn delete(&self, env: StarOrgEnv) -> Result<(), String> {
        if let Ok(e) = Self::entry(env) {
            match e.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                // No keyring at all: nothing can be there.
                Err(keyring::Error::NoStorageAccess(_))
                | Err(keyring::Error::PlatformFailure(_)) => {}
                Err(err) => return Err(format!("keyring: {err}")),
            }
        }
        self.file.delete(env)
    }
}

/// In memory (tests, and the browser mock).
#[derive(Default)]
pub struct MemoryStore(Mutex<Vec<(StarOrgEnv, String)>>);

impl TokenStore for MemoryStore {
    fn get(&self, env: StarOrgEnv) -> Result<Option<(String, TokenStoreKind)>, String> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .find(|(e, _)| *e == env)
            .map(|(_, t)| (t.clone(), TokenStoreKind::Keyring)))
    }

    fn set(&self, env: StarOrgEnv, token: &str) -> Result<TokenStoreKind, String> {
        let mut v = self.0.lock().unwrap_or_else(|p| p.into_inner());
        v.retain(|(e, _)| *e != env);
        v.push((env, token.to_string()));
        Ok(TokenStoreKind::Keyring)
    }

    fn delete(&self, env: StarOrgEnv) -> Result<(), String> {
        self.0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .retain(|(e, _)| *e != env);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_store_is_per_environment_and_owner_only() {
        let t = tempfile::tempdir().unwrap();
        let s = FileStore::new(t.path().join("starorg-tokens"));
        assert_eq!(s.get(StarOrgEnv::Dev).unwrap(), None);
        let secret = format!("value-{}", std::process::id());
        assert_eq!(
            s.set(StarOrgEnv::Dev, &secret).unwrap(),
            TokenStoreKind::File
        );
        assert_eq!(
            s.get(StarOrgEnv::Dev).unwrap(),
            Some((secret.clone(), TokenStoreKind::File))
        );
        assert_eq!(s.get(StarOrgEnv::Production).unwrap(), None);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&s.path(StarOrgEnv::Dev)), 0o600);
            assert_eq!(mode(&t.path().join("starorg-tokens")), 0o700);
        }
        s.delete(StarOrgEnv::Dev).unwrap();
        s.delete(StarOrgEnv::Dev).unwrap();
        assert_eq!(s.get(StarOrgEnv::Dev).unwrap(), None);
    }

    #[test]
    fn memory_store_replaces_per_environment() {
        let s = MemoryStore::default();
        s.set(StarOrgEnv::Production, "a").unwrap();
        s.set(StarOrgEnv::Production, "b").unwrap();
        s.set(StarOrgEnv::Dev, "c").unwrap();
        assert_eq!(s.get(StarOrgEnv::Production).unwrap().unwrap().0, "b");
        s.delete(StarOrgEnv::Production).unwrap();
        assert!(s.get(StarOrgEnv::Production).unwrap().is_none());
        assert!(s.get(StarOrgEnv::Dev).unwrap().is_some());
    }
}
