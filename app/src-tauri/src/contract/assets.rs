//! Developer mode assets in contract form (addendum 7.16): the Files browser, a record's
//! sounds and model export, and the asset cache. Self-contained like [`super::dev`]: types,
//! the [`AssetBackend`] half of the backend and the commands live here; `real/assets.rs`
//! and `mock_assets.rs` implement it. Mirrored in `app/src/lib/contract/assets.ts`.
//!
//! - Every call but the cache ones fails with `dev-disabled` while Developer mode is off.
//! - Nothing is read until the user opens the Files browser, a Sounds panel or exports a
//!   model: the real backend starts the `asset-worker` on the first such call, and answers
//!   from the asset cache without a worker when it can.
//! - The webview never sends a path: extraction and exports go through native dialogs.
//! - `asset_extract`, `asset_export_sounds` and `asset_export_model` stream [`JOB_EVENT`]
//!   progress (job kind `dev`) and are cancelled with `cancel_job`.

use super::types::*;
use super::{BackendState, Progress, run};
use serde::{Deserialize, Serialize};
use tauri::Emitter;

pub use super::dev::WorkerState;
pub use starorg_data::assets::audio::{
    RecordSounds, SoundInfo, SoundRef, SourceKind, TriggerHit, TriggerInfo, TriggerSearch,
};
pub use starorg_data::assets::files::{DirPage, FileEntry, FileSearch, PreviewKind};
pub use starorg_data::assets::model::ModelPreset;

/// Shown next to every export button (and in the CLI help).
pub const PERSONAL_USE: &str =
    "For personal use: these are Cloud Imperium's assets; don't redistribute.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetStatus {
    pub state: WorkerState,
    pub channel: Option<String>,
    /// Archive entries, once the worker is open.
    pub entries: u32,
    /// The DataCore is read (first sounds or model request).
    pub datacore: bool,
    /// The Wwise/ATL index is built (first sounds request).
    pub audio: bool,
    pub memory: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetListRequest {
    pub target: ChannelRef,
    /// `""` = the root; `Data\Objects` or `Data/Objects`.
    pub dir: String,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetSearchRequest {
    pub target: ChannelRef,
    pub query: String,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetPathRequest {
    pub target: ChannelRef,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetPreview {
    pub path: String,
    pub size: u64,
    pub kind: PreviewKind,
    pub format: String,
    pub text: Option<String>,
    pub truncated: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Images: a `data:` URL (PNG or JPEG, at most 1024 px a side).
    pub image: Option<String>,
    pub note: Option<String>,
    /// Answered from the asset cache (no worker).
    pub cached: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetExtractRequest {
    pub target: ChannelRef,
    /// Files and folders (a folder with everything below it).
    pub paths: Vec<String>,
    /// Textures as PNG (mips merged), CryXmlB as XML.
    pub convert: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetExportResult {
    /// The dialog was closed (nothing written).
    pub cancelled: bool,
    /// Shown, never sent back.
    pub path: Option<String>,
    pub files: u32,
    pub bytes: u64,
    /// Copied from the asset cache, no worker started.
    pub cached: bool,
    /// The first 20 failures.
    pub failed: Vec<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetRecordRequest {
    pub target: ChannelRef,
    /// A record GUID (or name).
    pub what: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetSoundRequest {
    pub target: ChannelRef,
    pub sound: SoundRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetAudio {
    /// The cache key (the webview keeps a small LRU by it).
    pub key: String,
    /// `audio/ogg`; empty when the sound is not Vorbis (export it as `.wem`).
    pub mime: String,
    /// Base64 of the Ogg file; empty when not playable.
    pub data: String,
    pub bytes: u64,
    pub duration: Option<f64>,
    pub cached: bool,
    pub note: Option<String>,
}

/// The Explorer's Sounds search (7.17): ATL triggers by name, event or bank.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetSoundSearchRequest {
    pub target: ChannelRef,
    pub query: String,
    pub limit: u32,
}

/// One trigger and the sounds it plays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetTriggerRequest {
    pub target: ChannelRef,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetSoundPick {
    pub trigger: String,
    pub sound: SoundRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetExportSoundsRequest {
    pub target: ChannelRef,
    pub what: String,
    /// Empty = every playable sound of the record.
    pub sounds: Vec<AssetSoundPick>,
    /// Also the original `.wem` files.
    pub wem: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetExportModelRequest {
    pub target: ChannelRef,
    pub what: String,
    /// The record's short name, for the save dialog's file name (may be empty).
    pub name: String,
    pub preset: ModelPreset,
    pub lod: u32,
    /// Also an STL next to the GLB.
    pub stl: bool,
    /// Export even with less free memory than the preset may need (after `needs-decision`).
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetCachePrevious {
    pub path: String,
    pub bytes: u64,
    pub files: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetCacheStatus {
    pub path: String,
    pub default_path: String,
    pub is_default: bool,
    pub bytes: u64,
    pub files: u64,
    pub max_bytes: u64,
    /// On another drive than the app's cache folder (fine).
    pub other_drive: bool,
    /// The folder used before the last change, while it still holds cache files: the UI
    /// offers "Move existing cache" (`asset_cache_move`) or leaves it.
    pub previous: Option<AssetCachePrevious>,
    /// Why the folder cannot be used (inside a game folder, a link); every cache use is then
    /// refused until it is changed or reset.
    pub problem: Option<String>,
}

/// The Developer mode assets half of the backend.
pub trait AssetBackend {
    /// The asset worker's state (never starts it).
    fn asset_status(&self, req: ChannelRef) -> CmdResult<AssetStatus>;
    /// Stop the asset worker (frees its memory).
    fn asset_close(&self, req: ChannelRef) -> CmdResult<AssetStatus>;
    fn asset_list(&self, req: AssetListRequest) -> CmdResult<DirPage>;
    fn asset_search(&self, req: AssetSearchRequest) -> CmdResult<FileSearch>;
    fn asset_preview(&self, req: AssetPathRequest) -> CmdResult<AssetPreview>;
    /// Native folder dialog, then the files.
    fn asset_extract(
        &self,
        req: AssetExtractRequest,
        progress: Progress<'_>,
    ) -> CmdResult<AssetExportResult>;
    /// The record's audio triggers (and its loadout's) with their sounds.
    fn asset_sounds(&self, req: AssetRecordRequest) -> CmdResult<RecordSounds>;
    /// Triggers by name (7.17); starts the asset worker and reads only the audio index.
    fn asset_sound_search(&self, req: AssetSoundSearchRequest) -> CmdResult<TriggerSearch>;
    fn asset_trigger(&self, req: AssetTriggerRequest) -> CmdResult<TriggerInfo>;
    /// One sound decoded for playing.
    fn asset_sound(&self, req: AssetSoundRequest) -> CmdResult<AssetAudio>;
    /// Native folder dialog, then the sounds as `.ogg` (and `.wem`).
    fn asset_export_sounds(
        &self,
        req: AssetExportSoundsRequest,
        progress: Progress<'_>,
    ) -> CmdResult<AssetExportResult>;
    /// Native save dialog, then the GLB (in a fresh worker, stopped afterwards).
    fn asset_export_model(
        &self,
        req: AssetExportModelRequest,
        progress: Progress<'_>,
    ) -> CmdResult<AssetExportResult>;
    fn asset_cache_status(&self) -> CmdResult<AssetCacheStatus>;
    /// Native folder dialog; validated (writable, not in a game folder) and saved.
    fn asset_cache_pick(&self) -> CmdResult<AssetCacheStatus>;
    fn asset_cache_reset(&self) -> CmdResult<AssetCacheStatus>;
    fn asset_cache_clear(&self) -> CmdResult<AssetCacheStatus>;
    /// Move the previous folder's cache files into the current one.
    fn asset_cache_move(&self) -> CmdResult<AssetCacheStatus>;
}

macro_rules! asset_commands {
    ($( $name:ident ( $($arg:ident : $ty:ty),* ) -> $ret:ty ;)*) => {
        $(
            #[tauri::command(rename_all = "snake_case")]
            pub async fn $name(
                state: tauri::State<'_, BackendState>,
                $($arg: $ty),*
            ) -> CmdResult<$ret> {
                run(&state, move |b| b.$name($($arg),*)).await
            }
        )*
    };
}

macro_rules! asset_progress_commands {
    ($( $name:ident ( $arg:ident : $ty:ty ) -> $ret:ty ;)*) => {
        $(
            #[tauri::command(rename_all = "snake_case")]
            pub async fn $name<R: tauri::Runtime>(
                app: tauri::AppHandle<R>,
                state: tauri::State<'_, BackendState>,
                $arg: $ty,
            ) -> CmdResult<$ret> {
                run(&state, move |b| {
                    let emit = move |p: JobProgress| {
                        let _ = app.emit(JOB_EVENT, p);
                    };
                    b.$name($arg, &emit)
                })
                .await
            }
        )*
    };
}

asset_commands! {
    asset_status(req: ChannelRef) -> AssetStatus;
    asset_close(req: ChannelRef) -> AssetStatus;
    asset_list(req: AssetListRequest) -> DirPage;
    asset_search(req: AssetSearchRequest) -> FileSearch;
    asset_preview(req: AssetPathRequest) -> AssetPreview;
    asset_sounds(req: AssetRecordRequest) -> RecordSounds;
    asset_sound_search(req: AssetSoundSearchRequest) -> TriggerSearch;
    asset_trigger(req: AssetTriggerRequest) -> TriggerInfo;
    asset_sound(req: AssetSoundRequest) -> AssetAudio;
    asset_cache_status() -> AssetCacheStatus;
    asset_cache_pick() -> AssetCacheStatus;
    asset_cache_reset() -> AssetCacheStatus;
    asset_cache_clear() -> AssetCacheStatus;
    asset_cache_move() -> AssetCacheStatus;
}

asset_progress_commands! {
    asset_extract(req: AssetExtractRequest) -> AssetExportResult;
    asset_export_sounds(req: AssetExportSoundsRequest) -> AssetExportResult;
    asset_export_model(req: AssetExportModelRequest) -> AssetExportResult;
}

/// Command names, in handler order (the TS client's `ASSET_COMMANDS` matches).
pub const ASSET_COMMAND_NAMES: &[&str] = &[
    "asset_status",
    "asset_close",
    "asset_list",
    "asset_search",
    "asset_preview",
    "asset_extract",
    "asset_sounds",
    "asset_sound_search",
    "asset_trigger",
    "asset_sound",
    "asset_export_sounds",
    "asset_export_model",
    "asset_cache_status",
    "asset_cache_pick",
    "asset_cache_reset",
    "asset_cache_clear",
    "asset_cache_move",
];

/// The sounds an "Export all" writes: every playable one, once per media.
pub fn playable_picks(s: &RecordSounds) -> Vec<AssetSoundPick> {
    let mut seen = std::collections::HashSet::new();
    s.triggers
        .iter()
        .flat_map(|t| {
            t.sounds
                .iter()
                .filter(|x| x.playable)
                .map(move |x| (t.name.clone(), x.sound.clone()))
        })
        .filter(|(_, s)| seen.insert(s.key()))
        .map(|(trigger, sound)| AssetSoundPick { trigger, sound })
        .collect()
}
