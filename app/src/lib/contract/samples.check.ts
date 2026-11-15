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
