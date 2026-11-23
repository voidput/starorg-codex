//! This program's own file, for starting its workers (`dev-worker`, `asset-worker`,
//! extraction). On Linux an update or rebuild that replaces the file while the app runs
//! leaves `/proc/self/exe` pointing at `<path> (deleted)`: starting that fails with a bare
//! "No such file or directory". Starting the new file instead could pair this app with a
//! worker of another version, so the answer is a plain "restart" message.

use std::path::{Path, PathBuf};

/// What Linux appends to `/proc/self/exe` once the running file is replaced or removed.
const DELETED: &str = " (deleted)";

/// The message when the running program's file was replaced.
pub const REPLACED: &str =
    "StarOrg Codex was updated or rebuilt while it was open: restart it to read game data";

/// The running program's file, or why its workers cannot be started.
pub fn own_exe() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot find own binary: {e}"))?;
    check(&exe).map(Path::to_path_buf)
}

/// `exe` as long as it still names the running program's file.
fn check(exe: &Path) -> Result<&Path, String> {
    if exe.to_string_lossy().ends_with(DELETED) {
        return Err(REPLACED.to_string());
    }
    Ok(exe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_replaced_file_says_restart() {
        let gone = Path::new("/home/u/target/debug/starorg-codex-app (deleted)");
        assert_eq!(check(gone), Err(REPLACED.to_string()));
        let live = Path::new("/usr/bin/starorg-codex-app");
        assert_eq!(check(live), Ok(live));
    }

    #[test]
    fn the_test_binary_is_its_own_file() {
        let exe = own_exe().unwrap();
        assert!(exe.exists(), "{}", exe.display());
    }
}
