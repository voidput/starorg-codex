/**
 * Type-checks samples.gen.json (real output of the Rust mock, written by
 * `UPDATE_CONTRACT_SAMPLES=1 cargo test -p starorg-strings-app`) against the TS mirror.
 * `npm run check` fails when a field is missing, renamed, of another JSON type, or extra.
 * String-union values are checked by the Rust test `ts_unions_list_every_variant`.
 */
import samples from './samples.gen.json';
import type * as T from './types';

interface Samples {
  contract: number;
  job_event: string;
  error: T.AppError;
  overview_first_run: T.Overview;
  overview: T.Overview;
  discover_installs: T.InstallSummary[];
  pick_install_folder: T.InstallSummary | null;
  plan_setup: T.SetupPlan;
  setup: T.SetupResult;
  channel_status: T.ChannelSummary;
  apply: T.ApplyReport;
  job_progress: T.JobProgress;
  list_backups: T.BackupList;
  backup_now: T.BackupView;
  remove_changes: T.ApplyReport;
  profile_schema: T.ProfileSchema;
  list_overrides: T.OverrideEntry[];
  add_user_patch: T.PatchView;
  export_user_patch: T.PatchExport;
  build_preview: T.PreviewItem[];
  get_profile: T.Profile;
  query_strings: T.StringsPage;
  query_strings_by_size: T.StringsPage;
  read_string_groups: T.GroupState;
  string_detail: T.StringDetail;
  set_override: T.OverrideResult;
  set_override_rejected: T.OverrideResult;
  list_patches: T.PatchView[];
  activity: T.LogEntry[];
  get_settings: T.Settings;
  list_characters: T.CharactersView;
  owned_blueprints: T.OwnedBlueprint[];
  import_smart_citizen: T.ImportReport;
  starorg_status: T.StarOrgStatus;
  starorg_pull: T.SyncReport;
  starorg_push: T.SyncReport;
  starorg_sync: T.SyncRun;
  set_starorg_guild: T.StarOrgStatus;
  patch_detail: T.PatchDetail;
  export_diagnostics: T.Diagnostics;
  save_diagnostics: string | null;
  hooks_status: T.HooksStatus;
  doctor: T.DoctorReport;
  fleet_view: T.FleetView;
  fleet_import: T.FleetImportReport;
  fleet_import_applied: T.FleetImportReport;
  fleet_match: T.FleetView;
  save_ship_names: T.FleetView;
  fleet_clear: T.FleetView;
  update_status: T.UpdateStatus;
  check_for_updates: T.UpdateStatus;
  check_for_updates_failed: T.UpdateStatus;
  dev_status: T.DevStatus;
  dev_open: T.DevStatus;
  dev_types: T.TypeCount[];
  dev_tree: T.TreePage;
  dev_tree_type: T.TreePage;
  dev_tree_path: T.TreePage;
  dev_tree_folder: T.TreePage;
  dev_search: T.SearchPage;
  dev_search_guid: T.SearchPage;
  dev_search_type: T.SearchPage;
  dev_record: T.RecordView;
  dev_record_cut: T.RecordView;
  dev_record_pointer: T.RecordView;
  dev_export_record: T.DevExportResult;
  dev_export_type: T.DevExportResult;
  dev_sources: T.DevSource[];
  dev_add_source: T.DevSource[];
  dev_dataset: T.DatasetInfo;
  dev_table: T.TableInfo;
  dev_table_kv: T.TableInfo;
  dev_rows: T.RowsPage;
  dev_rows_filtered: T.RowsPage;
  dev_rows_group: T.RowsPage;
  dev_table_weapons: T.TableInfo;
  dev_groups: T.GroupPage;
  dev_groups_open: T.GroupPage;
  dev_row: T.DevRowDetail;
  dev_row_loc: T.DevRowDetail;
  dev_diff: T.DevDiffSummary;
  dev_diff_summary: T.DevDiffSummary;
  dev_diff_records: T.DevDiffRecords;
  dev_diff_records_added: T.DevDiffRecords;
  dev_diff_records_group: T.DevDiffRecords;
  dev_diff_groups: T.GroupPage;
  dev_diff_groups_open: T.GroupPage;
  dev_diff_record: T.DevFieldDiff;
  dev_rows_readable: T.RowsPage;
  dev_diff_records_query: T.DevDiffRecords;
  dev_diff_record_ship: T.DevFieldDiff;
  dev_close: T.DevStatus;
  asset_cache_clear: T.AssetCacheStatus;
  asset_cache_move: T.AssetCacheStatus;
  asset_cache_pick: T.AssetCacheStatus;
  asset_cache_reset: T.AssetCacheStatus;
  asset_cache_status: T.AssetCacheStatus;
  asset_close: T.AssetStatus;
  asset_export_model: T.AssetExportResult;
  asset_export_model_cached: T.AssetExportResult;
  asset_export_model_full: T.AssetExportResult;
  asset_export_sounds: T.AssetExportResult;
  asset_extract: T.AssetExportResult;
  asset_list: T.DirPage;
  asset_list_data: T.DirPage;
  asset_list_page: T.DirPage;
  asset_preview: T.AssetPreview;
  asset_preview_audio: T.AssetPreview;
  asset_preview_cached: T.AssetPreview;
  asset_preview_cryxml: T.AssetPreview;
  asset_preview_none: T.AssetPreview;
  asset_preview_text: T.AssetPreview;
  asset_search: T.FileSearch;
  asset_sound: T.AssetAudio;
  asset_sound_cached: T.AssetAudio;
  asset_sounds: T.RecordSounds;
  asset_sounds_item: T.RecordSounds;
  asset_sounds_none: T.RecordSounds;
  asset_status: T.AssetStatus;
  asset_status_open: T.AssetStatus;
  mcp_status: T.McpStatus;
  set_mcp_enabled: T.McpStatus;
  memory_status: T.MemoryStatus;
  set_memory_limit: T.MemoryStatus;
  open_link: null;
}

/** String unions become `string` (JSON imports infer `string`), structure stays. */
type Widen<X> = X extends string
  ? string
  : X extends number
    ? number
    : X extends boolean
      ? boolean
      : X extends null
        ? null
        : X extends readonly (infer U)[]
          ? Widen<U>[]
          : X extends object
            ? { [K in keyof X]: Widen<X[K]> }
            : X;

type KeysOf<B> = B extends unknown ? keyof B : never;
type At<B, K extends PropertyKey> = B extends unknown ? (K extends keyof B ? B[K] : never) : never;

/** Rejects keys that exist in the JSON but not in the TS type (a field Rust added). */
type NoExtra<A, B> = A extends readonly (infer U)[]
  ? NoExtra<U, B extends readonly (infer V)[] ? V : never>[]
  : A extends object
    ? [Exclude<keyof A, KeysOf<B>>] extends [never]
      ? { [K in keyof A]: NoExtra<A[K], At<B, K>> }
      : { __extra_fields_not_in_types_ts: Exclude<keyof A, KeysOf<B>> }
    : A;

export const checkedSamples: Widen<Samples> = samples;
export const noExtraFields: NoExtra<typeof samples, Samples> = samples;
