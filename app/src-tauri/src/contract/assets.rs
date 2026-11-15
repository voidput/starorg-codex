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
//! - `asset_extract`, `asset_export_sounds`, `asset_export_model` and `asset_model_view`
//!   stream [`JOB_EVENT`] progress (job kind `dev`) and are cancelled with `cancel_job`.
//! - The Explorer's Model tab (addendum 7.20): `asset_model_info` (estimates and the cache, no
//!   worker), `asset_model_view` (the Print shell GLB at one LOD into the asset cache, in a
//!   fresh worker), then `asset_model_chunk` hands its bytes over as raw IPC binary (an
//!   `ArrayBuffer` in the webview, no JSON or base64), [`MODEL_CHUNK`] at a time.

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
    /// `audio/ogg` (Vorbis `.ogg`, Opus `.opus`) or `audio/wav` (PCM, IMA ADPCM); empty when
    /// the sound stays a `.wem` (`note` says why).
    pub mime: String,
    /// Base64 of the converted file; empty when not playable.
    pub data: String,
    pub bytes: u64,
    pub duration: Option<f64>,
    pub cached: bool,
    /// Why it is not playable (its codec named), when it is not.
    pub note: Option<String>,
    /// The WEM's codec and the file it became: `Vorbis (.ogg)`, `PCM (.wav)`, `fmt 0x8311 (.wem)`.
    #[serde(default)]
    pub format: String,
    /// Shown when the webview refuses to play it: what is missing on this system (Linux:
    /// GStreamer's gst-plugins-good). `None` when nothing is known to be missing.
    #[serde(default)]
    pub hint: Option<String>,
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

/// The most `asset_model_chunk` returns at once (8 MiB).
pub const MODEL_CHUNK: u32 = 8 * 1024 * 1024;

/// What the memory guard says about one start (`starorg_core::memlimit::check`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MemoryFit {
    Ok,
    /// More than the memory limit allows: refused.
    OverLimit,
    /// Under the limit, more than is free now: asks first (`needs-decision`).
    LowFree,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetModelInfoRequest {
    pub target: ChannelRef,
    pub what: String,
    /// The record's short name (memory estimates know capital ships by name); may be empty.
    pub name: String,
}

/// One step of the viewer's detail slider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetModelLod {
    /// 0 = most detail.
    pub lod: u32,
    /// The LOD it shows: `lod`, or the one below it the mesh has (LOD 4 of a mesh with 0-3 is
    /// LOD 3); known after a first export of the record.
    pub uses: u32,
    /// Peak memory of exporting it (bytes).
    pub need: u64,
    pub fit: MemoryFit,
    /// In the asset cache: shown without starting a worker.
    pub cached: bool,
    /// The cached GLB's size.
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetModelInfo {
    pub what: String,
    /// LOD 0 to 4, in order.
    pub lods: Vec<AssetModelLod>,
    /// The LODs the mesh has; empty until an export of it this session read them.
    pub available: Vec<u32>,
    /// Where the slider starts: LOD 3, or the least detailed one the mesh has below that.
    pub default_lod: u32,
    /// The memory limit (bytes), `None` when off.
    pub limit: Option<u64>,
    pub free: Option<u64>,
    /// The largest GLB the viewer keeps (a quarter of the asset cache's cap).
    pub max_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetModelViewRequest {
    pub target: ChannelRef,
    pub what: String,
    pub name: String,
    pub lod: u32,
    /// Export even with less free memory than it may need (after `needs-decision`).
    pub force: bool,
}

/// A Print shell GLB in the asset cache, ready for `asset_model_chunk`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetModelView {
    pub what: String,
    /// The LOD it has (send this to `asset_model_chunk`).
    pub lod: u32,
    /// The LOD asked for.
    pub asked: u32,
    /// The LODs the mesh has (empty when not known).
    pub available: Vec<u32>,
    pub bytes: u64,
    /// Answered from the asset cache, no worker started.
    pub cached: bool,
    pub triangles: Option<u64>,
    /// Export time and peak memory (not when cached).
    pub ms: Option<u64>,
    pub peak_rss: Option<u64>,
    /// Bytes per `asset_model_chunk` call.
    pub chunk: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetModelChunkRequest {
    pub target: ChannelRef,
    pub what: String,
    /// [`AssetModelView::lod`].
    pub lod: u32,
    pub offset: u64,
    /// At most [`MODEL_CHUNK`].
    pub len: u32,
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
    /// The Model tab's slider: per LOD the memory estimate, the guard and the cache (never
    /// starts a worker).
    fn asset_model_info(&self, req: AssetModelInfoRequest) -> CmdResult<AssetModelInfo>;
    /// The record's Print shell GLB at this LOD into the asset cache (a fresh worker, stopped
    /// afterwards; none when cached). The memory guard as for exports.
    fn asset_model_view(
        &self,
        req: AssetModelViewRequest,
        progress: Progress<'_>,
    ) -> CmdResult<AssetModelView>;
    /// Bytes of a GLB `asset_model_view` put in the cache (raw binary over IPC).
    fn asset_model_chunk(&self, req: AssetModelChunkRequest) -> CmdResult<Vec<u8>>;
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
    asset_model_info(req: AssetModelInfoRequest) -> AssetModelInfo;
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
    asset_model_view(req: AssetModelViewRequest) -> AssetModelView;
}

/// Raw bytes, not JSON: the webview gets an `ArrayBuffer`.
#[tauri::command(rename_all = "snake_case")]
pub async fn asset_model_chunk(
    state: tauri::State<'_, BackendState>,
    req: AssetModelChunkRequest,
) -> CmdResult<tauri::ipc::Response> {
    run(&state, move |b| b.asset_model_chunk(req))
        .await
        .map(tauri::ipc::Response::new)
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
    "asset_model_info",
    "asset_model_view",
    "asset_model_chunk",
    "asset_cache_status",
    "asset_cache_pick",
    "asset_cache_reset",
    "asset_cache_clear",
    "asset_cache_move",
];

/// [`MemoryFit`] of `memlimit::check`.
pub fn memory_fit(f: starorg_core::memlimit::Fit) -> MemoryFit {
    use starorg_core::memlimit::Fit;
    match f {
        Fit::Ok => MemoryFit::Ok,
        Fit::OverLimit { .. } => MemoryFit::OverLimit,
        Fit::LowFree { .. } => MemoryFit::LowFree,
    }
}

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
