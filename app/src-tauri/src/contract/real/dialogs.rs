//! Native dialogs for the real backend (tauri-plugin-dialog).
//!
//! The webview never sends a path (spec 11): commands that need one open a dialog here, on
//! the Rust side, so the webview gets no dialog permission at all. The backend is built
//! before the Tauri app exists, so the dialog implementation is put into a [`DialogSlot`] in
//! the app's `setup` hook. Without one (tests, or before setup) every dialog is "cancelled".

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

/// A file type filter: label and extensions without the dot.
#[derive(Debug, Clone, Copy)]
pub struct FileFilter {
    pub name: &'static str,
    pub extensions: &'static [&'static str],
}

/// What the backend asks of the desktop. Blocking: called on the blocking pool, never on the
/// main thread. `None` = the user cancelled.
pub trait Dialogs: Send + Sync {
    fn pick_folder(&self, title: &str) -> Option<PathBuf>;
    fn pick_file(&self, title: &str, filter: FileFilter) -> Option<PathBuf>;
    fn save_file(&self, title: &str, file_name: &str, filter: FileFilter) -> Option<PathBuf>;
}

/// Set once in the Tauri `setup` hook; shared with the backend.
#[derive(Default, Clone)]
pub struct DialogSlot(Arc<OnceLock<Box<dyn Dialogs>>>);

impl DialogSlot {
    pub fn set(&self, d: impl Dialogs + 'static) {
        let _ = self.0.set(Box::new(d));
    }

    pub fn get(&self) -> Option<&dyn Dialogs> {
        self.0.get().map(|b| b.as_ref())
    }
}

/// The real dialogs, over the app handle.
pub struct TauriDialogs<R: tauri::Runtime>(pub tauri::AppHandle<R>);

impl<R: tauri::Runtime> Dialogs for TauriDialogs<R> {
    fn pick_folder(&self, title: &str) -> Option<PathBuf> {
        use tauri_plugin_dialog::DialogExt;
        self.0
            .dialog()
            .file()
            .set_title(title)
            .blocking_pick_folder()
            .and_then(|p| p.into_path().ok())
    }

    fn pick_file(&self, title: &str, filter: FileFilter) -> Option<PathBuf> {
        use tauri_plugin_dialog::DialogExt;
        self.0
            .dialog()
            .file()
            .set_title(title)
            .add_filter(filter.name, filter.extensions)
            .blocking_pick_file()
            .and_then(|p| p.into_path().ok())
    }

    fn save_file(&self, title: &str, file_name: &str, filter: FileFilter) -> Option<PathBuf> {
        use tauri_plugin_dialog::DialogExt;
        self.0
            .dialog()
            .file()
            .set_title(title)
            .set_file_name(file_name)
            .add_filter(filter.name, filter.extensions)
            .blocking_save_file()
            .and_then(|p| p.into_path().ok())
    }
}
