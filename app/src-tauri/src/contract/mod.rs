//! The app <-> core command contract (issue #15).
//!
//! - [`types`]: every request and response shape, mirrored in `app/src/lib/contract/types.ts`.
//! - [`Backend`]: what the UI can ask for. Commands are thin: they unpack the request, call the
//!   backend off the main thread and forward progress as [`types::JOB_EVENT`] events.
//! - [`real::RealBackend`]: the default, over `starorg-core` and `starorg-data`.
//! - `mock::MockBackend`: synthetic data and in-memory state, for UI work without a game
//!   install (feature `mock-backend`) and for the contract tests. [`crate::run`] picks one;
//!   the commands do not change.
//!
//! Command names are snake_case and take one argument named `req` (or none), so the TS client
//! is a straight `invoke(name, { req })`.

pub mod dev;
pub mod fleet;
#[cfg(any(test, feature = "mock-backend"))]
pub mod mock;
#[cfg(any(test, feature = "mock-backend"))]
pub mod mock_dev;
pub mod real;
pub mod schema;
pub mod strings_query;
pub mod types;

use std::sync::Arc;
use tauri::Emitter;
use types::*;

/// Called with each progress update of a long job.
pub type Progress<'a> = &'a (dyn Fn(JobProgress) + Send + Sync);

/// Everything the UI can ask of the core. Implementations must be cheap to call from many
/// threads; long jobs report through `progress`.
/// Developer mode (addendum 7) is the [`dev::DevBackend`] half.
pub trait Backend: dev::DevBackend + Send + Sync + 'static {
    fn overview(&self) -> CmdResult<Overview>;
    fn discover_installs(&self, deep: bool) -> CmdResult<Vec<InstallSummary>>;
    /// Opens the native folder picker in the real backend; the webview never sends a path.
    fn pick_install_folder(&self) -> CmdResult<Option<InstallSummary>>;
    /// The review step: what `setup` + apply would write, without writing game files (the
    /// real backend registers the install and fills its cache in the app folders). Real backend:
    /// `counts` come from an M1 dry-run apply per channel with the chosen preset (a channel
    /// already set up: with its own profile).
    fn plan_setup(&self, req: SetupRequest) -> CmdResult<SetupPlan>;
    /// Consent per channel. A channel already set up keeps its profile and setup time; only
    /// new channels get the chosen preset.
    fn setup(&self, req: SetupRequest) -> CmdResult<SetupResult>;
    fn channel_status(&self, req: ChannelRef) -> CmdResult<ChannelSummary>;

    /// Guards (game running, lock, Data.p4k busy) come back as `result: skipped`, not `Err`.
    fn apply(&self, req: ApplyRequest, progress: Progress<'_>) -> CmdResult<ApplyReport>;
    /// `needs-decision` while another tool's file is in place and `allow_foreign` is false.
    fn remove_changes(&self, req: RemoveRequest) -> CmdResult<ApplyReport>;
    fn undo_last_apply(&self, req: ChannelRef) -> CmdResult<ApplyReport>;
    fn restore_backup(&self, req: RestoreRequest) -> CmdResult<ApplyReport>;
    fn list_backups(&self, req: ChannelRef) -> CmdResult<BackupList>;
    fn backup_now(&self, req: ChannelRef) -> CmdResult<BackupView>;
    fn delete_backup(&self, req: BackupRef) -> CmdResult<BackupList>;
    /// Stops a running job at its next step; the job's call then fails with `cancelled`.
    fn cancel_job(&self, job: String) -> CmdResult<()>;

    fn build_preview(&self, req: PreviewRequest) -> CmdResult<Vec<PreviewItem>>;
    /// Labels for the core's stat line and tag element ids.
    fn profile_schema(&self) -> CmdResult<ProfileSchema>;
    fn get_profile(&self, req: ChannelRef) -> CmdResult<Profile>;
    fn preset_profile(&self, preset: Preset) -> CmdResult<Profile>;
    fn save_profile(&self, req: SaveProfileRequest) -> CmdResult<Profile>;
    fn copy_profile(&self, req: CopyProfileRequest) -> CmdResult<Profile>;

    fn query_strings(&self, req: StringsQuery) -> CmdResult<StringsPage>;
    /// Run the loc-reference pass for the channel's build (an extraction, about 1-3 s plus
    /// reading Data.p4k) when its groups are approximate; returns the new state. `busy` while
    /// another extraction runs.
    fn read_string_groups(&self, req: ChannelRef) -> CmdResult<GroupState>;
    fn string_detail(&self, req: KeyRequest) -> CmdResult<StringDetail>;
    fn set_override(&self, req: OverrideRequest) -> CmdResult<OverrideResult>;
    fn reset_override(&self, req: KeyRequest) -> CmdResult<StringDetail>;
    fn list_overrides(&self, req: ChannelRef) -> CmdResult<Vec<OverrideEntry>>;

    fn list_patches(&self, req: ChannelRef) -> CmdResult<Vec<PatchView>>;
    fn set_patch_enabled(&self, req: PatchToggle) -> CmdResult<PatchView>;
    fn patch_detail(&self, req: PatchRef) -> CmdResult<PatchDetail>;
    fn add_user_patch(&self, req: NewPatch) -> CmdResult<PatchView>;
    /// Real backend: file picker for a `.patch.toml`; the webview never sends a path.
    fn import_user_patch(&self, req: ChannelRef) -> CmdResult<PatchView>;
    /// Real backend: also a save dialog.
    fn export_user_patch(&self, req: PatchRef) -> CmdResult<PatchExport>;
    /// User patches only; built-in ones can be switched off, not deleted.
    fn delete_user_patch(&self, req: PatchRef) -> CmdResult<Vec<PatchView>>;

    fn activity(&self, limit: u32) -> CmdResult<Vec<LogEntry>>;

    fn get_settings(&self) -> CmdResult<Settings>;
    fn save_settings(&self, req: Settings) -> CmdResult<Settings>;

    fn list_characters(&self, req: ChannelRef) -> CmdResult<CharactersView>;
    fn set_active_character(&self, req: SetActiveCharacter) -> CmdResult<CharactersView>;
    fn owned_blueprints(&self, req: CharacterRequest) -> CmdResult<Vec<OwnedBlueprint>>;
    /// Real backend: file picker for Smart Citizen's `owned-blueprints.json`, then import.
    fn import_smart_citizen(&self, req: ImportRequest) -> CmdResult<ImportReport>;
    /// Deletes one character's owned set (`<geid>.json`) and its entry in the character list.
    fn forget_character(&self, req: CharacterRequest) -> CmdResult<CharactersView>;

    /// Catalogue, fleet and ship-name settings of one channel (addendum §6).
    fn fleet_view(&self, req: FleetViewRequest) -> CmdResult<FleetView>;
    /// Dry run first; a real import replaces the fleet (or merges) and appends new hulls to
    /// the shared order list.
    fn fleet_import(&self, req: FleetImportRequest) -> CmdResult<FleetImportReport>;
    fn fleet_match(&self, req: FleetMatchRequest) -> CmdResult<FleetView>;
    fn save_ship_names(&self, req: SaveShipNamesRequest) -> CmdResult<FleetView>;
    /// Forget the imported fleet; hand matches and the order list stay.
    fn fleet_clear(&self, req: ChannelRef) -> CmdResult<FleetView>;

    fn starorg_status(&self, channel: String) -> CmdResult<StarOrgStatus>;
    fn starorg_login(&self, channel: String) -> CmdResult<StarOrgStatus>;
    fn starorg_logout(&self, channel: String) -> CmdResult<StarOrgStatus>;
    fn set_sync_characters(&self, req: SyncCharactersRequest) -> CmdResult<StarOrgStatus>;
    /// StarOrg owned set -> one character's set (union). Refused while not connected.
    fn starorg_pull(&self, req: StarOrgPullRequest) -> CmdResult<SyncReport>;
    /// Union of the ticked characters' sets -> StarOrg. Never pushes removals.
    fn starorg_push(&self, channel: String) -> CmdResult<SyncReport>;

    fn hooks_status(&self) -> CmdResult<HooksStatus>;
    fn hooks_install(&self) -> CmdResult<HooksStatus>;
    fn hooks_uninstall(&self) -> CmdResult<HooksStatus>;

    fn doctor(&self) -> CmdResult<DoctorReport>;
    /// Redacted doctor + recent log, shown in full before it is copied or saved.
    fn export_diagnostics(&self) -> CmdResult<Diagnostics>;
    /// Save the same text through a native save dialog. The path shown back is for display;
    /// `None` = cancelled (or no dialog, as in the browser mock).
    fn save_diagnostics(&self) -> CmdResult<Option<String>>;

    // Updates (docs/updates.md): both backends delegate to a `crate::update::Updater`.
    fn update_status(&self) -> CmdResult<UpdateStatus>;
    /// `force: false` = the start-up check (switch on and 24 h since the last one), `true` =
    /// "Check now" (`network-off` while the switch is off).
    fn check_for_updates(&self, req: UpdateCheckRequest) -> CmdResult<UpdateStatus>;
    fn set_update_prefs(&self, req: UpdatePrefs) -> CmdResult<UpdateStatus>;
    /// "Skip this version"; an empty string clears it.
    fn skip_update(&self, version: String) -> CmdResult<UpdateStatus>;
    /// The available release's page in the system browser (https://github.com only).
    fn open_release_page(&self) -> CmdResult<()>;
    /// AppImage / NSIS / MSI with a real key: download, verify, install, restart.
    fn install_update(&self) -> CmdResult<UpdateStatus>;

    // Experimental local MCP server (docs/mcp.md): off at every start, loopback only.
    fn mcp_status(&self) -> CmdResult<McpStatus>;
    /// Start (`enabled: true`) or stop the server; returns its state.
    fn set_mcp_enabled(&self, req: McpToggle) -> CmdResult<McpStatus>;
}

/// Managed state: the selected backend.
#[derive(Clone)]
pub struct BackendState(pub Arc<dyn Backend>);

impl BackendState {
    pub fn new(b: impl Backend) -> Self {
        Self(Arc::new(b))
    }
}

/// Run a backend call on the blocking pool (real implementations read and hash files).
pub(crate) async fn run<T: Send + 'static>(
    state: &BackendState,
    f: impl FnOnce(&dyn Backend) -> CmdResult<T> + Send + 'static,
) -> CmdResult<T> {
    let b = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || f(b.as_ref()))
        .await
        .map_err(|e| AppError::new(ErrorCode::Internal, format!("backend task failed: {e}")))?
}

/// One `#[tauri::command]` per backend method that takes no progress callback.
macro_rules! commands {
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

commands! {
    overview() -> Overview;
    discover_installs(deep: bool) -> Vec<InstallSummary>;
    pick_install_folder() -> Option<InstallSummary>;
    plan_setup(req: SetupRequest) -> SetupPlan;
    setup(req: SetupRequest) -> SetupResult;
    channel_status(req: ChannelRef) -> ChannelSummary;
    remove_changes(req: RemoveRequest) -> ApplyReport;
    undo_last_apply(req: ChannelRef) -> ApplyReport;
    restore_backup(req: RestoreRequest) -> ApplyReport;
    list_backups(req: ChannelRef) -> BackupList;
    backup_now(req: ChannelRef) -> BackupView;
    delete_backup(req: BackupRef) -> BackupList;
    cancel_job(job: String) -> ();
    build_preview(req: PreviewRequest) -> Vec<PreviewItem>;
    profile_schema() -> ProfileSchema;
    get_profile(req: ChannelRef) -> Profile;
    preset_profile(preset: Preset) -> Profile;
    save_profile(req: SaveProfileRequest) -> Profile;
    copy_profile(req: CopyProfileRequest) -> Profile;
    query_strings(req: StringsQuery) -> StringsPage;
    read_string_groups(req: ChannelRef) -> GroupState;
    string_detail(req: KeyRequest) -> StringDetail;
    set_override(req: OverrideRequest) -> OverrideResult;
    reset_override(req: KeyRequest) -> StringDetail;
    list_overrides(req: ChannelRef) -> Vec<OverrideEntry>;
    list_patches(req: ChannelRef) -> Vec<PatchView>;
    set_patch_enabled(req: PatchToggle) -> PatchView;
    patch_detail(req: PatchRef) -> PatchDetail;
    add_user_patch(req: NewPatch) -> PatchView;
    import_user_patch(req: ChannelRef) -> PatchView;
    export_user_patch(req: PatchRef) -> PatchExport;
    delete_user_patch(req: PatchRef) -> Vec<PatchView>;
    activity(limit: u32) -> Vec<LogEntry>;
    get_settings() -> Settings;
    save_settings(req: Settings) -> Settings;
    list_characters(req: ChannelRef) -> CharactersView;
    set_active_character(req: SetActiveCharacter) -> CharactersView;
    owned_blueprints(req: CharacterRequest) -> Vec<OwnedBlueprint>;
    import_smart_citizen(req: ImportRequest) -> ImportReport;
    forget_character(req: CharacterRequest) -> CharactersView;
    fleet_view(req: FleetViewRequest) -> FleetView;
    fleet_import(req: FleetImportRequest) -> FleetImportReport;
    fleet_match(req: FleetMatchRequest) -> FleetView;
    save_ship_names(req: SaveShipNamesRequest) -> FleetView;
    fleet_clear(req: ChannelRef) -> FleetView;
    starorg_status(channel: String) -> StarOrgStatus;
    starorg_login(channel: String) -> StarOrgStatus;
    starorg_logout(channel: String) -> StarOrgStatus;
    set_sync_characters(req: SyncCharactersRequest) -> StarOrgStatus;
    starorg_pull(req: StarOrgPullRequest) -> SyncReport;
    starorg_push(channel: String) -> SyncReport;
    hooks_status() -> HooksStatus;
    hooks_install() -> HooksStatus;
    hooks_uninstall() -> HooksStatus;
    doctor() -> DoctorReport;
    export_diagnostics() -> Diagnostics;
    save_diagnostics() -> Option<String>;
    update_status() -> UpdateStatus;
    check_for_updates(req: UpdateCheckRequest) -> UpdateStatus;
    set_update_prefs(req: UpdatePrefs) -> UpdateStatus;
    skip_update(version: String) -> UpdateStatus;
    open_release_page() -> ();
    install_update() -> UpdateStatus;
    mcp_status() -> McpStatus;
    set_mcp_enabled(req: McpToggle) -> McpStatus;
}

/// Apply streams [`JOB_EVENT`] events while it runs.
#[tauri::command(rename_all = "snake_case")]
pub async fn apply<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, BackendState>,
    req: ApplyRequest,
) -> CmdResult<ApplyReport> {
    run(&state, move |b| {
        let emit = move |p: JobProgress| {
            let _ = app.emit(JOB_EVENT, p);
        };
        b.apply(req, &emit)
    })
    .await
}

/// Every contract command, for `tauri::generate_handler!` (see `crate::builder`).
#[macro_export]
macro_rules! contract_handler {
    ($($extra:path),* $(,)?) => {
        tauri::generate_handler![
            $($extra,)*
            $crate::contract::overview,
            $crate::contract::discover_installs,
            $crate::contract::pick_install_folder,
            $crate::contract::plan_setup,
            $crate::contract::setup,
            $crate::contract::channel_status,
            $crate::contract::apply,
            $crate::contract::remove_changes,
            $crate::contract::undo_last_apply,
            $crate::contract::restore_backup,
            $crate::contract::list_backups,
            $crate::contract::backup_now,
            $crate::contract::delete_backup,
            $crate::contract::cancel_job,
            $crate::contract::build_preview,
            $crate::contract::profile_schema,
            $crate::contract::get_profile,
            $crate::contract::preset_profile,
            $crate::contract::save_profile,
            $crate::contract::copy_profile,
            $crate::contract::query_strings,
            $crate::contract::read_string_groups,
            $crate::contract::string_detail,
            $crate::contract::set_override,
            $crate::contract::reset_override,
            $crate::contract::list_overrides,
            $crate::contract::list_patches,
            $crate::contract::set_patch_enabled,
            $crate::contract::patch_detail,
            $crate::contract::add_user_patch,
            $crate::contract::import_user_patch,
            $crate::contract::export_user_patch,
            $crate::contract::delete_user_patch,
            $crate::contract::activity,
            $crate::contract::get_settings,
            $crate::contract::save_settings,
            $crate::contract::list_characters,
            $crate::contract::set_active_character,
            $crate::contract::owned_blueprints,
            $crate::contract::import_smart_citizen,
            $crate::contract::forget_character,
            $crate::contract::fleet_view,
            $crate::contract::fleet_import,
            $crate::contract::fleet_match,
            $crate::contract::save_ship_names,
            $crate::contract::fleet_clear,
            $crate::contract::starorg_status,
            $crate::contract::starorg_login,
            $crate::contract::starorg_logout,
            $crate::contract::set_sync_characters,
            $crate::contract::starorg_pull,
            $crate::contract::starorg_push,
            $crate::contract::hooks_status,
            $crate::contract::hooks_install,
            $crate::contract::hooks_uninstall,
            $crate::contract::doctor,
            $crate::contract::export_diagnostics,
            $crate::contract::save_diagnostics,
            $crate::contract::update_status,
            $crate::contract::check_for_updates,
            $crate::contract::set_update_prefs,
            $crate::contract::skip_update,
            $crate::contract::open_release_page,
            $crate::contract::install_update,
            $crate::contract::dev::dev_status,
            $crate::contract::dev::dev_open,
            $crate::contract::dev::dev_close,
            $crate::contract::dev::dev_types,
            $crate::contract::dev::dev_tree,
            $crate::contract::dev::dev_search,
            $crate::contract::dev::dev_record,
            $crate::contract::dev::dev_export_record,
            $crate::contract::dev::dev_export_type,
            $crate::contract::dev::dev_sources,
            $crate::contract::dev::dev_add_source,
            $crate::contract::dev::dev_remove_source,
            $crate::contract::dev::dev_download_dump,
            $crate::contract::dev::dev_dataset,
            $crate::contract::dev::dev_table,
            $crate::contract::dev::dev_rows,
            $crate::contract::dev::dev_row,
            $crate::contract::dev::dev_diff,
            $crate::contract::dev::dev_diff_summary,
            $crate::contract::dev::dev_diff_records,
            $crate::contract::dev::dev_diff_record,
            $crate::contract::mcp_status,
            $crate::contract::set_mcp_enabled,
        ]
    };
}

/// Command names, in the order of [`contract_handler!`]. The TS client has the same list
/// (`COMMANDS` in `app/src/lib/contract/client.ts`); a test compares them.
pub const COMMAND_NAMES: &[&str] = &[
    "overview",
    "discover_installs",
    "pick_install_folder",
    "plan_setup",
    "setup",
    "channel_status",
    "apply",
    "remove_changes",
    "undo_last_apply",
    "restore_backup",
    "list_backups",
    "backup_now",
    "delete_backup",
    "cancel_job",
    "build_preview",
    "profile_schema",
    "get_profile",
    "preset_profile",
    "save_profile",
    "copy_profile",
    "query_strings",
    "read_string_groups",
    "string_detail",
    "set_override",
    "reset_override",
    "list_overrides",
    "list_patches",
    "set_patch_enabled",
    "patch_detail",
    "add_user_patch",
    "import_user_patch",
    "export_user_patch",
    "delete_user_patch",
    "activity",
    "get_settings",
    "save_settings",
    "list_characters",
    "set_active_character",
    "owned_blueprints",
    "import_smart_citizen",
    "forget_character",
    "fleet_view",
    "fleet_import",
    "fleet_match",
    "save_ship_names",
    "fleet_clear",
    "starorg_status",
    "starorg_login",
    "starorg_logout",
    "set_sync_characters",
    "starorg_pull",
    "starorg_push",
    "hooks_status",
    "hooks_install",
    "hooks_uninstall",
    "doctor",
    "export_diagnostics",
    "save_diagnostics",
    "update_status",
    "check_for_updates",
    "set_update_prefs",
    "skip_update",
    "open_release_page",
    "install_update",
    "dev_status",
    "dev_open",
    "dev_close",
    "dev_types",
    "dev_tree",
    "dev_search",
    "dev_record",
    "dev_export_record",
    "dev_export_type",
    "dev_sources",
    "dev_add_source",
    "dev_remove_source",
    "dev_download_dump",
    "dev_dataset",
    "dev_table",
    "dev_rows",
    "dev_row",
    "dev_diff",
    "dev_diff_summary",
    "dev_diff_records",
    "dev_diff_record",
    "mcp_status",
    "set_mcp_enabled",
];

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_dev;
