//! Developer mode in contract form (addendum 7.7): the DataCore explorer, the dataset viewer
//! and the dataset diff. Self-contained: types, the [`DevBackend`] half of the backend and
//! the commands live here; `real/dev.rs` and `mock_dev.rs` implement it. Mirrored in
//! `app/src/lib/contract/dev.ts`.
//!
//! Every command fails with [`ErrorCode::DevDisabled`] while Settings > Advanced >
//! "Developer mode" is off. Calls that take a while (`dev_open`, `dev_export_type`,
//! `dev_diff`, `dev_download_dump`) return when their job ends and stream [`JOB_EVENT`]
//! progress with the job id meanwhile, as `apply` does; `cancel_job` stops an export.

use super::types::*;
use super::{BackendState, Progress, run};
use serde::{Deserialize, Serialize};
use tauri::Emitter;

pub use starorg_core::dev::dataset::{
    ColType, Column, DatasetInfo, Fact, Row, RowFilter, RowSort, RowsPage, TableInfo, TableKind,
    TableSummary, ValueCount,
};
pub use starorg_core::dev::diff::{DiffRecord, DiffTable, FieldChange, Presence, RecordState};
pub use starorg_core::dev::explorer::{
    NodeKind, SearchHit, SearchPage, TreeNode, TreePage, TreeRoot, TypeCount,
};
pub use starorg_core::dev::group::{
    DataGroup, DiffCounts, DiffGroupBy, GroupLevel, GroupPage, GroupPick, GroupPreset,
};
pub use starorg_core::dev::readable::{LabelKind, Readable};
pub use starorg_core::dev::record::{LocText, RecordHeader, RecordView};
pub use starorg_core::dev::sections::{
    Change, FacetCounts, FacetValue, Filters, GroupHead, ListGroup, ListItem, ListPage, ListQuery,
    Matching, SectionStats, SortKey, Stat,
};

/// Records above this count get a size warning in the export dialog (7.4).
pub const EXPORT_WARN_RECORDS: u32 = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkerState {
    Stopped,
    Starting,
    Ready,
    /// Start failed or the worker stopped unexpectedly; `error` says why. Retry = `dev_open`.
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevStatus {
    pub state: WorkerState,
    /// The channel the worker serves.
    pub channel: Option<String>,
    pub build: Option<String>,
    pub records: u32,
    pub main: u32,
    pub types: u32,
    /// RFC 3339.
    pub started: Option<String>,
    pub start_ms: Option<u64>,
    /// Worker resident memory, bytes, where the OS reports it.
    pub memory: Option<u64>,
    pub error: Option<String>,
    /// The section index (7.17): build time, size, entities with models and sounds.
    #[serde(default)]
    pub sections: Option<SectionStats>,
    /// The channel the explorer is compared with (7.17), when one is.
    #[serde(default)]
    pub compared: Option<DevCompare>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevTreeRequest {
    pub target: ChannelRef,
    pub root: TreeRoot,
    /// `""` = the root; else a node id from a previous reply.
    pub node: String,
    pub offset: u32,
    pub limit: u32,
    pub main_only: bool,
    /// Sections view: hide AI, NPC, test and template variants (7.17).
    #[serde(default)]
    pub player_only: bool,
}

/// The item finder's list (7.17): a node of the Sections view (or every record), search
/// with field prefixes, filters, sort and grouping. Rows carry headline stats from the
/// build's GameData.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevListRequest {
    pub target: ChannelRef,
    pub query: ListQuery,
}

/// "Open owning record" for a geometry file of the Files browser (7.17).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevOwnerRequest {
    pub target: ChannelRef,
    /// Archive path (`Data\Objects\...\x.cga`).
    pub path: String,
}

/// Compare the explorer's channel with another channel's build (7.17): GameData tables
/// diffed as the Diff screen does; records added or changed get badges and a filter.
/// `other: None` clears the comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevCompareRequest {
    pub target: ChannelRef,
    pub other: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevCompare {
    pub other: String,
    pub added: u32,
    pub changed: u32,
    /// Only in the other channel (no record here to show).
    pub removed: u32,
    /// Marked records the explorer found by GUID.
    pub marked: u32,
    pub ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevSearchRequest {
    pub target: ChannelRef,
    pub query: String,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevRecordRequest {
    pub target: ChannelRef,
    /// A GUID, a record path or a record name.
    pub what: String,
    /// JSON pointer of the node to load (`""` = the record).
    pub pointer: String,
    /// Cut deeper nodes (expand loads them); `None` = whole when it fits in 1 MiB.
    pub depth: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevExportRecordRequest {
    pub target: ChannelRef,
    pub what: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevExportTypeRequest {
    pub target: ChannelRef,
    pub type_name: String,
    pub main_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevExportResult {
    /// True when the save dialog was cancelled (nothing written).
    pub cancelled: bool,
    /// Shown, never sent back.
    pub path: Option<String>,
    pub records: u32,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    /// Our own GameData of a channel's build, from the extraction cache.
    Build,
    /// A local folder (or one file) of `*.json`.
    Folder,
    /// A published LIVE build downloaded for Diff (addendum §1): `dump:<tag>`.
    Dump,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceState {
    Ready,
    /// Needs a switch that is off.
    Off,
    /// Not there (yet): the build's data was not read, the folder is gone.
    Missing,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevDatasetSummary {
    pub file: String,
    pub bytes: u64,
    pub tables: u32,
    pub rows: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevSource {
    /// `build:<channel>`, `folder:<n>`, `dump:<tag>`.
    pub id: String,
    pub kind: SourceKind,
    pub label: String,
    pub build: Option<String>,
    pub state: SourceState,
    /// Why it is not ready, or the folder shown.
    pub detail: Option<String>,
    pub datasets: Vec<DevDatasetSummary>,
}

/// A published LIVE build of `voidput/sc-gamedata-dumps` (addendum §1, Diff only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DumpRelease {
    /// `Data.Version` of the build, e.g. `4.10.200.58746`; its source id is `dump:<tag>`.
    pub tag: String,
    /// The release title, e.g. `4.10.0 LIVE (4.10.200.58746)`.
    pub title: String,
    /// Always `LIVE` (only LIVE builds are offered).
    pub channel: String,
    pub branch: Option<String>,
    /// When it was published (RFC 3339).
    pub published: Option<String>,
    /// Downloaded and on this computer: usable as a source with the switch off too.
    pub cached: bool,
}

/// The published LIVE builds as far as they could be listed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DumpReleases {
    pub releases: Vec<DumpRelease>,
    /// Set when GitHub could not be asked (offline, rate limited): `releases` then holds only
    /// the downloaded builds. Not an error: they still work.
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevDumpRequest {
    pub target: ChannelRef,
    pub tag: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevSourceRef {
    pub target: ChannelRef,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevDatasetRef {
    pub target: ChannelRef,
    pub source: String,
    pub dataset: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevTableRef {
    pub target: ChannelRef,
    pub source: String,
    pub dataset: String,
    pub table: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DevRowsRequest {
    pub target: ChannelRef,
    pub source: String,
    pub dataset: String,
    pub table: String,
    /// Columns shown and searched; empty = the table's default columns.
    pub columns: Vec<String>,
    pub search: String,
    pub filters: Vec<RowFilter>,
    pub sort: Option<RowSort>,
    pub offset: u32,
    pub limit: u32,
    /// Only the rows of this group tree node (addendum 7.15).
    #[serde(default)]
    pub group: Option<GroupPick>,
}

/// The Datasets group tree (addendum 7.15): the visible nodes for a group-by, with counts
/// that follow the same search and filters as the rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DevGroupsRequest {
    pub target: ChannelRef,
    pub source: String,
    pub dataset: String,
    pub table: String,
    pub by: Vec<GroupLevel>,
    /// Node ids whose children are listed.
    pub open: Vec<String>,
    pub columns: Vec<String>,
    pub search: String,
    pub filters: Vec<RowFilter>,
}

/// A Diff tree node to filter records by.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffGroupPick {
    pub by: DiffGroupBy,
    pub node: String,
}

/// The Diff group tree over the last comparison (addendum 7.15).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevDiffGroupsRequest {
    pub target: ChannelRef,
    pub by: DiffGroupBy,
    pub open: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevRowRequest {
    pub target: ChannelRef,
    pub source: String,
    pub dataset: String,
    pub table: String,
    pub row: u32,
}

/// A value in a row that names a DataCore record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExplorerLink {
    /// JSON pointer of the value in the row.
    pub pointer: String,
    pub value: String,
    /// What to open in the explorer (GUID or record name).
    pub what: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevRowDetail {
    pub row: u32,
    pub json: String,
    pub truncated: bool,
    pub loc: Vec<LocText>,
    pub links: Vec<ExplorerLink>,
    /// What GUIDs, record names, `@key`s and manufacturer codes in the row read as, by raw
    /// value (addendum 7.14).
    #[serde(default)]
    pub labels: std::collections::BTreeMap<String, Readable>,
    /// Rows of the same dataset this row names.
    #[serde(default)]
    pub row_links: Vec<RowLink>,
}

/// A row of the same dataset this row names (a ship's loadout naming its weapons by record
/// name). The Datasets row sheet and the Diff field pane list them to jump there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowLink {
    /// JSON pointers of the values naming it, in order (one line each in the row).
    pub pointers: Vec<String>,
    pub value: String,
    pub table: String,
    pub row: u32,
    /// Its display name (7.14), when it has one.
    pub title: Option<String>,
    /// Values of this row naming it ("4 Gatlings": one row, several mounts).
    pub count: u32,
    /// Diff only: the source the row is in (B's, or A's for a removed record).
    #[serde(default)]
    pub source: Option<String>,
    /// Diff only: the linked row's record in the comparison, when it changed.
    #[serde(default)]
    pub diff_id: Option<String>,
    #[serde(default)]
    pub diff_state: Option<RecordState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevDiffRequest {
    pub target: ChannelRef,
    pub a: String,
    pub b: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevDiffSummary {
    pub a: String,
    pub b: String,
    pub tables: Vec<DiffTable>,
    pub ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevDiffRecordsRequest {
    pub target: ChannelRef,
    pub dataset: String,
    pub table: String,
    pub state: Option<RecordState>,
    /// Only records with a changed path starting with this.
    pub field: Option<String>,
    /// Override the identity column (`None` = automatic).
    pub identity: Option<String>,
    /// Only records whose id or display name contains this (case-insensitive).
    #[serde(default)]
    pub query: Option<String>,
    pub offset: u32,
    pub limit: u32,
    /// Only records in this Diff tree node (its dataset and table must be these).
    #[serde(default)]
    pub group: Option<DiffGroupPick>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevDiffRecords {
    pub summary: DiffTable,
    pub records: Vec<DiffRecord>,
    pub offset: u32,
    pub total: u32,
    /// Changed fields (first path segment) with how many records change them.
    pub fields: Vec<ValueCount>,
    /// Columns usable as identity (scalar on both sides).
    pub identity_options: Vec<String>,
}

/// Search every table of the comparison by record name or id (the Diff screen's top search).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevDiffSearchRequest {
    pub target: ChannelRef,
    /// Case-insensitive, matched like the per-table record filter.
    pub query: String,
    pub state: Option<RecordState>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffSearchHit {
    pub dataset: String,
    pub table: String,
    pub record: DiffRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevDiffSearch {
    /// Tables in summary order, records in each table's order; at most `limit`.
    pub hits: Vec<DiffSearchHit>,
    /// Matches in all tables (more than `hits` when cut at `limit`).
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevDiffRecordRequest {
    pub target: ChannelRef,
    pub dataset: String,
    pub table: String,
    pub id: String,
    pub identity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DevFieldDiff {
    pub id: String,
    /// The record's display name (addendum 7.14).
    #[serde(default)]
    pub label: Option<String>,
    pub state: RecordState,
    pub changes: Vec<FieldChange>,
    /// Rows of the same dataset the record names (B's row, A's for a removed record).
    #[serde(default)]
    pub row_links: Vec<RowLink>,
}

/// The Developer mode half of the backend. Every method returns `dev-disabled` while the
/// setting is off.
pub trait DevBackend {
    fn dev_status(&self, req: ChannelRef) -> CmdResult<DevStatus>;
    /// Start (or restart for a new build) the worker for the channel; returns when ready.
    fn dev_open(&self, req: ChannelRef, progress: Progress<'_>) -> CmdResult<DevStatus>;
    /// Stop the worker (frees its memory).
    fn dev_close(&self, req: ChannelRef) -> CmdResult<DevStatus>;
    fn dev_types(&self, req: ChannelRef) -> CmdResult<Vec<TypeCount>>;
    fn dev_tree(&self, req: DevTreeRequest) -> CmdResult<TreePage>;
    /// The item finder's list (7.17).
    fn dev_list(&self, req: DevListRequest) -> CmdResult<ListPage>;
    /// Entities whose geometry is an archive file (7.17).
    fn dev_owner(&self, req: DevOwnerRequest) -> CmdResult<Vec<SearchHit>>;
    /// Compare with another channel (or clear it) (7.17).
    fn dev_compare(&self, req: DevCompareRequest) -> CmdResult<Option<DevCompare>>;
    fn dev_search(&self, req: DevSearchRequest) -> CmdResult<SearchPage>;
    fn dev_record(&self, req: DevRecordRequest) -> CmdResult<RecordView>;
    /// Native save dialog, then pretty JSON in StarBreaker's form.
    fn dev_export_record(&self, req: DevExportRecordRequest) -> CmdResult<DevExportResult>;
    /// Native save dialog, then one JSON array, streamed; `cancel_job` stops it.
    fn dev_export_type(
        &self,
        req: DevExportTypeRequest,
        progress: Progress<'_>,
    ) -> CmdResult<DevExportResult>;
    fn dev_sources(&self, req: ChannelRef) -> CmdResult<Vec<DevSource>>;
    /// Native folder dialog; at most 5 folders are remembered.
    fn dev_add_source(&self, req: ChannelRef) -> CmdResult<Vec<DevSource>>;
    fn dev_remove_source(&self, req: DevSourceRef) -> CmdResult<Vec<DevSource>>;
    /// The published LIVE builds, newest first. With "Download published LIVE builds for
    /// Diff" off: only the downloaded ones, and no request is made. When GitHub cannot be
    /// reached: the downloaded ones with `error` set.
    fn dev_dump_releases(&self, req: ChannelRef) -> CmdResult<DumpReleases>;
    /// Download, verify and cache one LIVE build (a job); `network-off` while the switch is
    /// off. Returns the sources, now with `dump:<tag>`.
    fn dev_download_dump(
        &self,
        req: DevDumpRequest,
        progress: Progress<'_>,
    ) -> CmdResult<Vec<DevSource>>;
    fn dev_dataset(&self, req: DevDatasetRef) -> CmdResult<DatasetInfo>;
    fn dev_table(&self, req: DevTableRef) -> CmdResult<TableInfo>;
    fn dev_rows(&self, req: DevRowsRequest) -> CmdResult<RowsPage>;
    fn dev_row(&self, req: DevRowRequest) -> CmdResult<DevRowDetail>;
    fn dev_groups(&self, req: DevGroupsRequest) -> CmdResult<GroupPage>;
    fn dev_diff(&self, req: DevDiffRequest, progress: Progress<'_>) -> CmdResult<DevDiffSummary>;
    fn dev_diff_summary(&self, req: ChannelRef) -> CmdResult<DevDiffSummary>;
    fn dev_diff_records(&self, req: DevDiffRecordsRequest) -> CmdResult<DevDiffRecords>;
    fn dev_diff_groups(&self, req: DevDiffGroupsRequest) -> CmdResult<GroupPage>;
    fn dev_diff_record(&self, req: DevDiffRecordRequest) -> CmdResult<DevFieldDiff>;
    fn dev_diff_search(&self, req: DevDiffSearchRequest) -> CmdResult<DevDiffSearch>;
}

pub fn disabled() -> AppError {
    AppError::new(
        ErrorCode::DevDisabled,
        "Developer mode is off (Settings > Advanced).",
    )
}

/// The refusal both backends give a dump download while the switch is off.
pub fn dumps_off() -> AppError {
    AppError::new(
        ErrorCode::NetworkOff,
        "Turn on Settings > Network > \"Download published LIVE builds for Diff\" first.",
    )
}

/// The answer for an unknown or non-LIVE tag (both backends).
pub fn dump_not_published(tag: &str, why: &str) -> AppError {
    AppError::new(
        ErrorCode::NotFound,
        format!("Could not download {}: {why}", dump_label(tag)),
    )
}

/// The answer for a tag that is not one path component (both backends).
pub fn bad_dump_tag(tag: &str) -> AppError {
    AppError::new(ErrorCode::Invalid, format!("bad build tag {tag:?}"))
}

/// `dump:<tag>` of a source id, when the tag is usable as one path component.
pub fn dump_tag(source: &str) -> Option<&str> {
    source
        .strip_prefix("dump:")
        .filter(|t| starorg_core::source::dump::valid_component(t))
}

/// The label of a downloaded LIVE build in the source lists.
pub fn dump_label(tag: &str) -> String {
    format!("LIVE {tag}")
}

macro_rules! dev_commands {
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

macro_rules! dev_progress_commands {
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

dev_commands! {
    dev_status(req: ChannelRef) -> DevStatus;
    dev_close(req: ChannelRef) -> DevStatus;
    dev_types(req: ChannelRef) -> Vec<TypeCount>;
    dev_tree(req: DevTreeRequest) -> TreePage;
    dev_list(req: DevListRequest) -> ListPage;
    dev_owner(req: DevOwnerRequest) -> Vec<SearchHit>;
    dev_compare(req: DevCompareRequest) -> Option<DevCompare>;
    dev_search(req: DevSearchRequest) -> SearchPage;
    dev_record(req: DevRecordRequest) -> RecordView;
    dev_export_record(req: DevExportRecordRequest) -> DevExportResult;
    dev_sources(req: ChannelRef) -> Vec<DevSource>;
    dev_add_source(req: ChannelRef) -> Vec<DevSource>;
    dev_remove_source(req: DevSourceRef) -> Vec<DevSource>;
    dev_dump_releases(req: ChannelRef) -> DumpReleases;
    dev_dataset(req: DevDatasetRef) -> DatasetInfo;
    dev_table(req: DevTableRef) -> TableInfo;
    dev_rows(req: DevRowsRequest) -> RowsPage;
    dev_row(req: DevRowRequest) -> DevRowDetail;
    dev_groups(req: DevGroupsRequest) -> GroupPage;
    dev_diff_summary(req: ChannelRef) -> DevDiffSummary;
    dev_diff_records(req: DevDiffRecordsRequest) -> DevDiffRecords;
    dev_diff_groups(req: DevDiffGroupsRequest) -> GroupPage;
    dev_diff_record(req: DevDiffRecordRequest) -> DevFieldDiff;
    dev_diff_search(req: DevDiffSearchRequest) -> DevDiffSearch;
}

dev_progress_commands! {
    dev_open(req: ChannelRef) -> DevStatus;
    dev_export_type(req: DevExportTypeRequest) -> DevExportResult;
    dev_download_dump(req: DevDumpRequest) -> Vec<DevSource>;
    dev_diff(req: DevDiffRequest) -> DevDiffSummary;
}

/// Shared by both backends: changed-field counts (first path segment) of a table diff.
pub fn field_counts(records: &[DiffRecord]) -> Vec<ValueCount> {
    let mut m: std::collections::BTreeMap<String, u32> = Default::default();
    for r in records {
        let mut seen = std::collections::BTreeSet::new();
        for f in &r.fields {
            let head = f
                .trim_start_matches('/')
                .split('/')
                .next()
                .unwrap_or_default()
                .to_string();
            if seen.insert(head.clone()) {
                *m.entry(head).or_default() += 1;
            }
        }
    }
    let mut v: Vec<ValueCount> = m
        .into_iter()
        .map(|(value, count)| ValueCount { value, count })
        .collect();
    v.sort_by(|a, b| b.count.cmp(&a.count).then(a.value.cmp(&b.value)));
    v
}

/// The part of each record name that tells records sharing a title apart: the common
/// leading `_` segments dropped, the rest with spaces ("PU AI CRIM" for
/// `AEGS_Tiburon_PU_AI_CRIM` next to `AEGS_Tiburon`). `None` for the record named by the
/// common part alone. Names that are not all different give `None` everywhere.
fn name_tails(names: &[&str]) -> Vec<Option<String>> {
    let distinct = names
        .iter()
        .map(|n| n.to_lowercase())
        .collect::<std::collections::HashSet<_>>()
        .len();
    if distinct != names.len() {
        return vec![None; names.len()];
    }
    let segs: Vec<Vec<&str>> = names.iter().map(|n| n.split('_').collect()).collect();
    let mut common = 0;
    while segs.iter().all(|s| s.len() > common)
        && segs
            .iter()
            .all(|s| s[common].eq_ignore_ascii_case(segs[0][common]))
    {
        common += 1;
    }
    segs.iter()
        .map(|s| {
            let tail = s[common..].join(" ");
            (!tail.trim().is_empty()).then(|| tail.trim().to_string())
        })
        .collect()
}

/// Shared by both backends: filter, page and trim one table's diff records. `title` gives a
/// record's display name (addendum 7.14); the text query matches it and the id. `name` gives
/// its record name (`recordName` / `className`), when the row has one. Titles that several
/// records of the result share are told apart by the differing end of their record names
/// ("Aegis Tiburon · PU AI CRIM" next to plain "Aegis Tiburon"), else by the short id
/// ("Aegis Javelin (AEGS_Javelin_AI_Escort)", or a GUID).
pub fn page_records(
    d: &starorg_core::dev::diff::TableDiff,
    req: &DevDiffRecordsRequest,
    identity_options: Vec<String>,
    in_group: &dyn Fn(&DiffRecord) -> bool,
    title: &dyn Fn(&DiffRecord) -> Option<String>,
    name: &dyn Fn(&DiffRecord) -> Option<String>,
) -> DevDiffRecords {
    let needle = req
        .query
        .as_deref()
        .map(|q| q.trim().to_lowercase())
        .filter(|q| !q.is_empty());
    let hits: Vec<(&DiffRecord, Option<String>)> = d
        .records
        .iter()
        .filter(|r| req.state.is_none_or(|s| r.state == s))
        .filter(|r| in_group(r))
        .filter(|r| {
            req.field.as_deref().is_none_or(|f| {
                let f = format!("/{}", f.trim_start_matches('/'));
                r.fields
                    .iter()
                    .any(|p| p == &f || p.starts_with(&format!("{f}/")))
            })
        })
        .map(|r| (r, title(r)))
        .filter(|(r, t)| match &needle {
            None => true,
            Some(n) => {
                r.id.to_lowercase().contains(n)
                    || t.as_deref().is_some_and(|t| t.to_lowercase().contains(n))
            }
        })
        .collect();
    // Shared titles: the record name of each, then the part that differs.
    let mut groups: std::collections::HashMap<&str, Vec<usize>> = std::collections::HashMap::new();
    for (i, t) in hits.iter().enumerate() {
        if let Some(t) = t.1.as_deref() {
            groups.entry(t).or_default().push(i);
        }
    }
    enum Suffix {
        Plain,
        Tail(String),
        ShortId,
    }
    let mut suffix: std::collections::HashMap<usize, Suffix> = std::collections::HashMap::new();
    for idx in groups.values().filter(|v| v.len() > 1) {
        // No record name: the id stands in, unless it is a GUID (nothing readable in it).
        let names: Option<Vec<String>> = idx
            .iter()
            .map(|&i| {
                let r = hits[i].0;
                name(r)
                    .or_else(|| {
                        (!starorg_core::dev::explorer::is_guid(&r.id)).then(|| r.id.clone())
                    })
                    .map(|n| starorg_core::model::short_name(&n).to_string())
            })
            .collect();
        let tails = match &names {
            Some(n) => name_tails(&n.iter().map(String::as_str).collect::<Vec<_>>()),
            None => vec![None; idx.len()],
        };
        // Told apart by name: the base record keeps the plain title. Otherwise: the short id.
        let by_name = tails.iter().any(Option::is_some);
        for (k, &i) in idx.iter().enumerate() {
            let label = match (&tails[k], by_name) {
                (Some(t), _) => Suffix::Tail(t.clone()),
                (None, true) => Suffix::Plain,
                (None, false) => Suffix::ShortId,
            };
            suffix.insert(i, label);
        }
    }
    let limit = (req.limit as usize).clamp(1, starorg_core::dev::explorer::MAX_PAGE);
    DevDiffRecords {
        summary: d.summary.clone(),
        records: hits
            .iter()
            .enumerate()
            .skip(req.offset as usize)
            .take(limit)
            .map(|(i, (r, t))| {
                let mut r = (*r).clone();
                r.fields.truncate(20);
                r.label = t.as_deref().map(|t| {
                    let short = starorg_core::model::short_name(&r.id);
                    match suffix.get(&i) {
                        Some(Suffix::Tail(tail)) => format!("{t} · {tail}"),
                        Some(Suffix::ShortId) if t != short => format!("{t} ({short})"),
                        _ => t.to_string(),
                    }
                });
                r
            })
            .collect(),
        offset: req.offset,
        total: hits.len() as u32,
        fields: field_counts(&d.records),
        identity_options,
    }
}

/// One side of a comparison: datasets by file name.
pub type DiffSideSets = [(String, std::sync::Arc<starorg_core::dev::dataset::Dataset>)];

/// Shared by both backends: one side's table of a comparison.
pub fn side_table<'a>(
    side: &'a DiffSideSets,
    dataset: &str,
    table: &str,
) -> Option<&'a starorg_core::dev::dataset::Table> {
    side.iter()
        .find(|x| x.0 == dataset)
        .and_then(|x| x.1.table(table))
}

/// Shared by both backends: the records of every table whose title or id contains the query,
/// labelled as the per-table list labels them. An empty query finds nothing.
pub fn search_tables(
    tables: &[starorg_core::dev::diff::TableDiff],
    a: &DiffSideSets,
    b: &DiffSideSets,
    req: &DevDiffSearchRequest,
    la: &dyn starorg_core::dev::readable::Lookup,
    lb: &dyn starorg_core::dev::readable::Lookup,
) -> DevDiffSearch {
    let query = req.query.trim();
    let limit = (req.limit as usize).clamp(1, starorg_core::dev::explorer::MAX_PAGE);
    let mut out = DevDiffSearch {
        hits: Vec::new(),
        total: 0,
    };
    if query.is_empty() {
        return out;
    }
    for td in tables {
        let (ds, tn) = (&td.summary.dataset, &td.summary.table);
        let (ta, tb) = (side_table(a, ds, tn), side_table(b, ds, tn));
        let id = td.summary.identity.as_deref();
        let page = page_records(
            td,
            &DevDiffRecordsRequest {
                target: req.target.clone(),
                dataset: ds.clone(),
                table: tn.clone(),
                state: req.state,
                field: None,
                identity: None,
                query: Some(query.to_string()),
                offset: 0,
                limit: (limit - out.hits.len().min(limit)).max(1) as u32,
                group: None,
            },
            Vec::new(),
            &|_| true,
            &|r| record_title(r, id, ta, tb, la, lb),
            &|r| record_name(r, ta, tb),
        );
        out.total += page.total;
        for r in page.records {
            if out.hits.len() < limit {
                out.hits.push(DiffSearchHit {
                    dataset: ds.clone(),
                    table: tn.clone(),
                    record: r,
                });
            }
        }
    }
    out
}

/// At most this many linked rows per row.
pub const MAX_ROW_LINKS: usize = 60;

/// Shared by both backends: the rows of `d` that row `row` of `table` names, titled.
pub fn row_links(
    d: &starorg_core::dev::dataset::Dataset,
    table: &str,
    row: u32,
    l: &dyn starorg_core::dev::readable::Lookup,
) -> Vec<RowLink> {
    use starorg_core::dev::rowrefs::{RowAt, RowIndex, refs_in};
    let Some(v) = d.table(table).and_then(|t| t.rows.get(row as usize)) else {
        return Vec::new();
    };
    let index = RowIndex::new(d);
    refs_in(v, &index, RowAt { table, row })
        .into_iter()
        .take(MAX_ROW_LINKS)
        .map(|r| RowLink {
            title: d
                .table(r.at.table)
                .and_then(|t| t.rows.get(r.at.row as usize))
                .and_then(|x| starorg_core::dev::readable::row_title(l, x, None)),
            pointers: r.pointers,
            value: r.value,
            table: r.at.table.to_string(),
            row: r.at.row,
            count: r.count,
            source: None,
            diff_id: None,
            diff_state: None,
        })
        .collect()
}

/// Shared by both backends: [`row_links`] of a diff record's row (B's, A's when removed),
/// each with its source and, when the linked row changed too, its record in the comparison.
#[allow(clippy::too_many_arguments)]
pub fn diff_row_links(
    tables: &[starorg_core::dev::diff::TableDiff],
    summary: &DevDiffSummary,
    a: &DiffSideSets,
    b: &DiffSideSets,
    dataset: &str,
    table: &str,
    rec: &DiffRecord,
    la: &dyn starorg_core::dev::readable::Lookup,
    lb: &dyn starorg_core::dev::readable::Lookup,
) -> Vec<RowLink> {
    let (side, row, source, l, on_b) = match (rec.b, rec.a) {
        (Some(r), _) => (b, r, &summary.b, lb, true),
        (None, Some(r)) => (a, r, &summary.a, la, false),
        (None, None) => return Vec::new(),
    };
    let Some(d) = side.iter().find(|x| x.0 == dataset).map(|x| &x.1) else {
        return Vec::new();
    };
    let mut links = row_links(d, table, row, l);
    for k in &mut links {
        k.source = Some(source.clone());
        let found = tables
            .iter()
            .find(|t| t.summary.dataset == dataset && t.summary.table == k.table)
            .and_then(|t| {
                t.records
                    .iter()
                    .find(|r| (if on_b { r.b } else { r.a }) == Some(k.row))
            });
        if let Some(r) = found {
            k.diff_id = Some(r.id.clone());
            k.diff_state = Some(r.state);
        }
    }
    links
}

/// Shared by both backends: the Diff tree of a comparison (addendum 7.15).
pub fn diff_tree(
    tables: &[starorg_core::dev::diff::TableDiff],
    a: &DiffSideSets,
    b: &DiffSideSets,
    req: &DevDiffGroupsRequest,
    label: starorg_core::dev::group::LabelFn<'_>,
) -> GroupPage {
    use starorg_core::dev::group::{DiffSide, diff_groups};
    let sides: Vec<DiffSide<'_>> = tables
        .iter()
        .map(|d| DiffSide {
            diff: d,
            a: side_table(a, &d.summary.dataset, &d.summary.table),
            b: side_table(b, &d.summary.dataset, &d.summary.table),
        })
        .collect();
    diff_groups(&sides, req.by, &req.open, label)
}

/// Shared by both backends: whether a record is in the picked Diff tree node. A node of
/// another dataset or table holds none of this table's records; a dataset node holds all.
pub fn diff_group_filter<'a>(
    req: &'a DevDiffRecordsRequest,
    a: &'a DiffSideSets,
    b: &'a DiffSideSets,
) -> impl Fn(&DiffRecord) -> bool + 'a {
    use starorg_core::dev::group::{diff_levels, diff_node, record_in};
    let ta = side_table(a, &req.dataset, &req.table);
    let tb = side_table(b, &req.dataset, &req.table);
    let pick = req.group.as_ref().and_then(|g| {
        let (ds, table, rest) = diff_node(&g.node)?;
        let ok = ds == req.dataset && table.as_deref().is_none_or(|t| t == req.table);
        Some((ok, diff_levels(g.by, ta, tb), rest))
    });
    move |r: &DiffRecord| match &pick {
        None => true,
        Some((false, _, _)) => false,
        Some((true, levels, rest)) => record_in(r, ta, tb, levels, rest),
    }
}

/// Shared by both backends: the display name of a diff record, from its row on B (else A).
pub fn record_title(
    r: &DiffRecord,
    identity: Option<&str>,
    a: Option<&starorg_core::dev::dataset::Table>,
    b: Option<&starorg_core::dev::dataset::Table>,
    la: &dyn starorg_core::dev::readable::Lookup,
    lb: &dyn starorg_core::dev::readable::Lookup,
) -> Option<String> {
    use starorg_core::dev::readable::row_title;
    fn row(
        t: Option<&starorg_core::dev::dataset::Table>,
        i: Option<u32>,
    ) -> Option<&serde_json::Value> {
        t?.rows.get(i? as usize)
    }
    row(b, r.b)
        .and_then(|x| row_title(lb, x, identity))
        .or_else(|| row(a, r.a).and_then(|x| row_title(la, x, identity)))
}

/// A diff record's record name (`recordName`, else `className`), B's row first.
pub fn record_name(
    r: &DiffRecord,
    a: Option<&starorg_core::dev::dataset::Table>,
    b: Option<&starorg_core::dev::dataset::Table>,
) -> Option<String> {
    let of = |t: Option<&starorg_core::dev::dataset::Table>, i: Option<u32>| {
        let row = t?.rows.get(i? as usize)?;
        ["recordName", "className"]
            .iter()
            .find_map(|k| row.get(*k).and_then(serde_json::Value::as_str))
            .map(str::to_string)
    };
    of(b, r.b).or_else(|| of(a, r.a))
}

/// Explorer links in a dataset row (7.5): GUID-shaped strings found in the index, and
/// `recordName` / `className` values equal to a record name.
pub fn explorer_links(
    row: &serde_json::Value,
    is_record: &dyn Fn(&str) -> Option<String>,
) -> Vec<ExplorerLink> {
    fn go(
        v: &serde_json::Value,
        path: &mut String,
        key: Option<&str>,
        f: &dyn Fn(&str) -> Option<String>,
        out: &mut Vec<ExplorerLink>,
    ) {
        if out.len() >= 200 {
            return;
        }
        match v {
            serde_json::Value::String(s) => {
                let guid = starorg_core::dev::explorer::is_guid(s);
                let named = matches!(key, Some("recordName" | "className"));
                if (guid || named)
                    && let Some(what) = f(s)
                {
                    out.push(ExplorerLink {
                        pointer: path.clone(),
                        value: s.clone(),
                        what,
                    });
                }
            }
            serde_json::Value::Array(a) => {
                for (i, x) in a.iter().enumerate() {
                    let len = path.len();
                    path.push_str(&format!("/{i}"));
                    go(x, path, None, f, out);
                    path.truncate(len);
                }
            }
            serde_json::Value::Object(m) => {
                for (k, x) in m {
                    let len = path.len();
                    path.push('/');
                    path.push_str(&k.replace('~', "~0").replace('/', "~1"));
                    go(x, path, Some(k), f, out);
                    path.truncate(len);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    go(row, &mut String::new(), None, is_record, &mut out);
    out
}

/// Loc keys in a dataset row: `@key` strings, or strings equal to a stock key
/// (case-insensitive), with their English text (7.5).
pub fn row_loc(row: &serde_json::Value, text_of: &dyn Fn(&str) -> Option<String>) -> Vec<LocText> {
    fn go(v: &serde_json::Value, f: &dyn Fn(&str) -> Option<String>, out: &mut Vec<LocText>) {
        if out.len() >= starorg_core::dev::record::MAX_LOC {
            return;
        }
        match v {
            serde_json::Value::String(s) if !s.is_empty() && s.len() < 200 => {
                let bare = s.strip_prefix('@').unwrap_or(s);
                if !bare.is_empty()
                    && !bare.contains(' ')
                    && !out.iter().any(|l| l.key == *s)
                    && let Some(text) = f(bare)
                {
                    out.push(LocText {
                        key: s.clone(),
                        text,
                    });
                }
            }
            serde_json::Value::Array(a) => a.iter().for_each(|x| go(x, f, out)),
            serde_json::Value::Object(m) => m.values().for_each(|x| go(x, f, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    go(row, text_of, &mut out);
    out
}
