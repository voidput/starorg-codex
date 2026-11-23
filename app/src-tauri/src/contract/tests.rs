//! Contract tests: the TS mirror matches the Rust shapes, commands work over IPC, and the mock
//! behaves like the spec says the real backend will.

use super::mock::{MOCK_STRING_COUNT, MockBackend};
use super::*;
use serde_json::{Value, json};
use std::path::PathBuf;

pub(super) fn app_src() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/lib/contract")
}

fn live() -> ChannelRef {
    ChannelRef {
        install: "inst-7f3a".into(),
        channel: "LIVE".into(),
    }
}

fn hotfix() -> ChannelRef {
    ChannelRef {
        channel: "HOTFIX".into(),
        ..live()
    }
}

fn noop(_: JobProgress) {}

fn set_up(b: &MockBackend) {
    b.setup(SetupRequest {
        install: "inst-7f3a".into(),
        channels: vec![
            ChannelConsent {
                channel: "LIVE".into(),
                consent: true,
                switch_language: false,
            },
            ChannelConsent {
                channel: "HOTFIX".into(),
                consent: true,
                switch_language: false,
            },
            ChannelConsent {
                channel: "PTU".into(),
                consent: false,
                switch_language: false,
            },
        ],
        preset: Preset::Recommended,
        fix_known_bugs: true,
    })
    .unwrap();
}

fn query(search: &str) -> StringsQuery {
    StringsQuery {
        target: live(),
        search: search.into(),
        sources: vec![],
        categories: vec![],
        changed_only: false,
        sort: StringSort::Key,
        offset: 0,
        limit: 3,
        groups: vec![],
        manufacturers: vec![],
        sizes: vec![],
        group_by: GroupBy::Type,
    }
}

/// One sample of every response, in a fixed scripted state. Keys are command names.
fn samples() -> Value {
    let b = MockBackend::default();
    let first_overview = b.overview().unwrap();
    let discovered = b.discover_installs(false).unwrap();
    let picked = b.pick_install_folder().unwrap();
    let plan = b
        .plan_setup(SetupRequest {
            install: "inst-7f3a".into(),
            channels: vec![
                ChannelConsent {
                    channel: "LIVE".into(),
                    consent: true,
                    switch_language: false,
                },
                ChannelConsent {
                    channel: "PTU".into(),
                    consent: true,
                    switch_language: true,
                },
            ],
            preset: Preset::Recommended,
            fix_known_bugs: true,
        })
        .unwrap();
    set_up(&b);
    let setup_result =
        json!({ "install": "inst-7f3a", "set_up": ["LIVE", "HOTFIX"], "skipped": ["PTU"] });
    let mut events = std::sync::Mutex::new(Vec::new());
    let record = |p: JobProgress| events.lock().unwrap().push(p);
    let applied = b
        .apply(
            ApplyRequest {
                target: live(),
                dry_run: false,
            },
            &record,
        )
        .unwrap();
    let event = events.get_mut().unwrap().first().cloned().unwrap();
    b.apply(
        ApplyRequest {
            target: live(),
            dry_run: false,
        },
        &noop,
    )
    .unwrap();
    let ov = b
        .set_override(OverrideRequest {
            target: live(),
            key: "item_NameSYN_Cooler_S2_00001".into(),
            value: "My cooler".into(),
        })
        .unwrap();
    let rejected = b
        .set_override(OverrideRequest {
            target: live(),
            key: "item_NameSYN_Cooler_S2_00001".into(),
            value: "Open <EM4>tag".into(),
        })
        .unwrap();
    let user_patch = b
        .add_user_patch(NewPatch {
            target: live(),
            title: "Sample label fix".into(),
            description: "Synthetic user patch.".into(),
            key: "ui_SYN_Label_00007".into(),
            op: LocOp::Replace,
            find: Some("Label".into()),
            value: "Sign".into(),
        })
        .unwrap();
    let exported = b
        .export_user_patch(PatchRef {
            target: live(),
            id: user_patch.id.clone(),
        })
        .unwrap();
    let mut settings = b.get_settings().unwrap();
    settings.network.starorg = true;
    settings.starorg.push_blueprints = true;
    settings.starorg.pull_blueprints = true;
    settings.starorg.pull_fleet = true;
    let settings = b.save_settings(settings).unwrap();
    let (fleet_dry, fleet_import, fleet_match, fleet_saved, fleet_clear, fleet_view) =
        fleet_samples(&b);
    let err = b
        .apply(
            ApplyRequest {
                target: ChannelRef {
                    channel: "PTU".into(),
                    ..live()
                },
                dry_run: false,
            },
            &noop,
        )
        .unwrap_err();
    let mut v = json!({
        "contract": CONTRACT_VERSION,
        "job_event": JOB_EVENT,
        "error": err,
        "overview_first_run": first_overview,
        "overview": b.overview().unwrap(),
        "discover_installs": discovered,
        "pick_install_folder": picked,
        "plan_setup": plan,
        "setup": setup_result,
        "channel_status": b.channel_status(live()).unwrap(),
        "apply": applied,
        "job_progress": event,
        "list_backups": b.list_backups(live()).unwrap(),
        "backup_now": b.backup_now(hotfix()).unwrap(),
        "remove_changes": b.remove_changes(RemoveRequest { target: hotfix(), allow_foreign: false }).unwrap(),
        "profile_schema": b.profile_schema().unwrap(),
        "list_overrides": b.list_overrides(live()).unwrap(),
        "add_user_patch": user_patch,
        "export_user_patch": exported,
        "build_preview": b.build_preview(PreviewRequest { target: live(), profile: None, preset: Some(Preset::Everything) }).unwrap(),
        "get_profile": b.get_profile(live()).unwrap(),
        "query_strings": b.query_strings(query("cooler")).unwrap(),
        "query_strings_by_size": b.query_strings(StringsQuery { group_by: GroupBy::Size, sizes: vec![2], sort: StringSort::Group, ..query("cooler") }).unwrap(),
        "read_string_groups": b.read_string_groups(hotfix()).unwrap(),
        "string_detail": b.string_detail(KeyRequest { target: live(), key: "vehicle_DescSYN_Ship_00000".into() }).unwrap(),
        "set_override": ov,
        "set_override_rejected": rejected,
        "list_patches": b.list_patches(live()).unwrap(),
        "activity": b.activity(5).unwrap(),
        "get_settings": settings,
        "list_characters": b.list_characters(live()).unwrap(),
        "owned_blueprints": b.owned_blueprints(CharacterRequest { target: live(), character: "1000000000001".into() }).unwrap(),
        "import_smart_citizen": b.import_smart_citizen(ImportRequest { target: live(), character: "1000000000002".into(), dry_run: true, reuse_last: false, replace: false }).unwrap(),
        "starorg_status": b.starorg_login("LIVE".into()).unwrap(),
        "starorg_pull": b.starorg_pull(StarOrgPullRequest { channel: "LIVE".into(), character: "1000000000001".into() }).unwrap(),
        "starorg_push": b.starorg_push("LIVE".into()).unwrap(),
        "patch_detail": b.patch_detail(PatchRef { target: live(), id: "syn-moved-key".into() }).unwrap(),
        "export_diagnostics": b.export_diagnostics().unwrap(),
        "save_diagnostics": b.save_diagnostics().unwrap(),
        "hooks_status": b.hooks_status().unwrap(),
        "doctor": b.doctor().unwrap(),
    });
    v["rescan_logs"] = json!(
        b.rescan_logs(RescanRequest {
            target: live(),
            dry_run: true,
            replace: false
        })
        .unwrap()
    );
    // Separate inserts: one json! with every sample hits the macro recursion limit.
    for (k, x) in [
        ("fleet_view", json!(fleet_view)),
        ("fleet_import", json!(fleet_dry)),
        ("fleet_import_applied", json!(fleet_import)),
        ("fleet_match", json!(fleet_match)),
        ("save_ship_names", json!(fleet_saved)),
        ("fleet_clear", json!(fleet_clear)),
    ] {
        v[k] = x;
    }
    // From a fresh mock: the TS mock seeds its owned sets from this sample.
    let fresh = MockBackend::default();
    v["blueprint_finder"] = json!(
        fresh
            .blueprint_finder(BlueprintFinderRequest {
                install: live().install,
                channels: vec![]
            })
            .unwrap()
    );
    v["blueprint_finder_live"] = json!(
        fresh
            .blueprint_finder(BlueprintFinderRequest {
                install: live().install,
                channels: vec!["LIVE".into()]
            })
            .unwrap()
    );
    v["starorg_sync"] = json!(b.starorg_sync("LIVE".into()).unwrap());
    v["set_starorg_guild"] = json!(
        b.set_starorg_guild(StarOrgGuildRequest {
            channel: "LIVE".into(),
            guild: "100000000000000001".into(),
        })
        .unwrap()
    );
    // Outside json! (its recursion limit).
    v["query_strings_by_owner"] = json!(
        b.query_strings(StringsQuery {
            sort: StringSort::Owner,
            ..query("ui_SYN")
        })
        .unwrap()
    );
    let (off, available, failed) = update_samples();
    v["update_status"] = json!(off);
    v["check_for_updates"] = json!(available);
    v["check_for_updates_failed"] = json!(failed);
    for (k, x) in super::tests_dev::dev_samples() {
        v[k] = x;
    }
    v["open_link"] = json!(b.open_link(ExternalLink::Support).unwrap());
    for (k, x) in super::tests_assets::asset_samples() {
        v[k] = x;
    }
    let (mcp_off, mcp_on) = mcp_samples();
    v["mcp_status"] = json!(mcp_off);
    v["set_mcp_enabled"] = json!(mcp_on);
    v["obtain_status"] = json!(b.obtain_status().unwrap());
    let r = MockBackend::default();
    set_up(&r);
    v["read_game_data"] = json!(
        r.read_game_data(ReadGameDataRequest { target: live() })
            .unwrap()
    );
    let (token_off, token_on, fetched) = uex_samples();
    v["uex_token_status"] = json!(token_off);
    v["set_uex_token"] = json!(token_on);
    v["fetch_uex"] = json!(fetched);
    for (k, x) in shop_samples() {
        v[k] = x;
    }
    for (k, x) in commodity_samples() {
        v[k] = x;
    }
    let m = MockBackend::default();
    v["memory_status"] = json!(m.memory_status().unwrap());
    v["set_memory_limit"] = json!(
        m.set_memory_limit(MemoryLimitRequest { gb: Some(6.0) })
            .unwrap()
    );
    let (storage, cleared) = storage_samples();
    v["storage_status"] = json!(storage);
    v["storage_clear_assets"] = json!(cleared);
    v
}

/// Settings > Storage on the mock: with a played sound in the asset cache, then cleared.
fn storage_samples() -> (StorageStatus, StorageStatus) {
    let m = MockBackend::default();
    m.asset_state().cached.push(("sound:1".into(), 48_000_000));
    let before = m.storage_status().unwrap();
    let after = m.storage_clear_assets().unwrap();
    (before, after)
}

#[test]
fn storage_on_the_mock_lists_every_area_and_clears_assets() {
    let (s, c) = storage_samples();
    let ids: Vec<_> = s.areas.iter().map(|a| a.id).collect();
    assert_eq!(
        ids,
        [
            StorageAreaId::GameData,
            StorageAreaId::WhereToGet,
            StorageAreaId::Uex,
            StorageAreaId::AssetExports,
            StorageAreaId::Temporary,
            StorageAreaId::Backups,
        ]
    );
    let assets = |s: &StorageStatus| {
        s.areas
            .iter()
            .find(|a| a.id == StorageAreaId::AssetExports)
            .unwrap()
            .bytes
    };
    assert_eq!(assets(&s), 48_000_000);
    assert_eq!(assets(&c), 0);
    assert_eq!(s.cache_bytes - c.cache_bytes, 48_000_000);
    assert_eq!(s.cap_bytes, starorg_core::storage::DEFAULT_CAP);
    assert_eq!(s.backup_keep_newest, 5);
    let backups = s
        .areas
        .iter()
        .find(|a| a.id == StorageAreaId::Backups)
        .unwrap();
    assert!(!backups.in_cache);
    assert_eq!(s.total_bytes, s.cache_bytes + backups.bytes);
}

#[test]
fn memory_limit_round_trip_on_the_mock() {
    let b = MockBackend::default();
    let s = b.memory_status().unwrap();
    // 32 GB machine: 1.5 .. 30 GB in 0.5 GB steps, default 5 GB; fits = under the 90% stop.
    assert_eq!(
        (s.min_gb, s.max_gb, s.step_gb, s.default_gb),
        (1.5, 30.0, 0.5, 5.0)
    );
    assert_eq!(s.stop_percent, 90);
    assert!(s.note.is_none());
    assert!(s.is_default && s.limit_gb == 5.0);
    let fits = |s: &MemoryStatus, id: &str| s.estimates.iter().find(|e| e.id == id).unwrap().fits;
    assert!(fits(&s, "mcp-ships") && !fits(&s, "model-full"));
    let s = b
        .set_memory_limit(MemoryLimitRequest { gb: Some(7.8) })
        .unwrap();
    // Full (7 GB) fits at 8 GB (stop 7.2 GB), not at 7.5 GB (stop 6.75 GB).
    assert_eq!(s.limit_gb, 8.0);
    assert!(!s.is_default && fits(&s, "model-full") && !fits(&s, "model-full-capital"));
    let s = b
        .set_memory_limit(MemoryLimitRequest { gb: Some(7.5) })
        .unwrap();
    assert!(!fits(&s, "model-full"));
    assert_eq!(
        b.set_memory_limit(MemoryLimitRequest { gb: Some(0.2) })
            .unwrap()
            .limit_gb,
        1.5
    );
    assert_eq!(
        b.set_memory_limit(MemoryLimitRequest { gb: Some(99.0) })
            .unwrap()
            .limit_gb,
        30.0
    );
    let s = b.set_memory_limit(MemoryLimitRequest { gb: None }).unwrap();
    assert!(s.is_default && s.limit_gb == 5.0);
    assert!(
        b.activity(5)
            .unwrap()
            .iter()
            .any(|e| e.message.contains("Memory limit"))
    );
}

/// Off as every start leaves it, and a running status with fixed values (a real run has a
/// random port and token).
fn mcp_samples() -> (McpStatus, McpStatus) {
    let b = MockBackend::default();
    let off = b.mcp_status().unwrap();
    let url = "http://127.0.0.1:41237/mcp";
    let token = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let on = McpStatus {
        running: true,
        url: Some(url.into()),
        token: Some(token.into()),
        started: Some("2026-11-14T21:30:05.000Z".into()),
        error: None,
        channels: vec!["LIVE".into(), "PTU".into()],
        claude_code: Some(crate::mcp::claude_code_config(url, token)),
        claude_desktop: Some(crate::mcp::claude_desktop_config(url, token)),
        stdio: crate::mcp::STDIO_COMMAND.into(),
        requests: vec![
            McpRequest {
                time: "2026-11-14T21:31:02.114Z".into(),
                method: Some("tools/call".into()),
                tool: Some("how_to_obtain".into()),
                status: 200,
                ok: true,
                millis: 412,
            },
            McpRequest {
                time: "2026-11-14T21:30:40.020Z".into(),
                method: None,
                tool: None,
                status: 401,
                ok: false,
                millis: 0,
            },
        ],
    };
    (off, on)
}

#[test]
fn mcp_server_is_off_by_default_and_runs_on_loopback() {
    let b = MockBackend::default();
    let off = b.mcp_status().unwrap();
    assert!(!off.running && off.url.is_none() && off.token.is_none());
    assert_eq!(off.stdio, "starorg-codex mcp");
    let on = b.set_mcp_enabled(McpToggle { enabled: true }).unwrap();
    assert!(on.running, "{:?}", on.error);
    assert!(on.url.as_deref().unwrap().starts_with("http://127.0.0.1:"));
    assert!(
        b.activity(5)
            .unwrap()
            .iter()
            .any(|e| e.job == Some(JobKind::Mcp) && e.message.contains("started"))
    );
    let off = b.set_mcp_enabled(McpToggle { enabled: false }).unwrap();
    assert!(!off.running);
    assert!(
        b.activity(5)
            .unwrap()
            .iter()
            .any(|e| e.job == Some(JobKind::Mcp) && e.message.contains("stopped"))
    );
}

/// Import the mock fleet file (dry run, then for real), settle one row, turn ship names on
/// with a group and a rename, then clear the fleet.
fn fleet_samples(
    b: &MockBackend,
) -> (
    FleetImportReport,
    FleetImportReport,
    FleetView,
    FleetView,
    FleetView,
    FleetView,
) {
    let import = |dry_run| {
        b.fleet_import(FleetImportRequest {
            target: live(),
            source: FleetSource::File,
            dry_run,
            merge: false,
            prune: false,
        })
        .unwrap()
    };
    let dry = import(true);
    let real = import(false);
    let matched = b
        .fleet_match(FleetMatchRequest {
            target: live(),
            name: "Starlance".into(),
            hull: None,
        })
        .unwrap();
    let mut s = matched.settings.clone();
    s.enabled = true;
    s.groups.push(ShipGroup {
        id: "combat".into(),
        name: "Combat".into(),
    });
    s.ships[0].group = Some("combat".into());
    s.ships[1].name = Some("Rusty".into());
    let saved = b
        .save_ship_names(SaveShipNamesRequest {
            target: live(),
            scope: ShipNamesScope::Shared,
            settings: s,
        })
        .unwrap();
    let view = b
        .fleet_view(FleetViewRequest {
            target: live(),
            read_game_data: false,
        })
        .unwrap();
    let cleared = b.fleet_clear(hotfix()).unwrap();
    // Leave the fleet in place for the samples after this one.
    b.fleet_import(FleetImportRequest {
        target: live(),
        source: FleetSource::File,
        dry_run: false,
        merge: true,
        prune: false,
    })
    .unwrap();
    (dry, real, matched, saved, cleared, view)
}

/// Update samples on their own mock (the switch is on there): off and never checked, an
/// available release, a repo that cannot be seen.
fn update_samples() -> (UpdateStatus, UpdateStatus, UpdateStatus) {
    let b = MockBackend::default();
    let off = b.update_status().unwrap();
    let mut s = b.get_settings().unwrap();
    s.network.check_updates = true;
    b.save_settings(s).unwrap();
    let available = b
        .check_for_updates(UpdateCheckRequest { force: false })
        .unwrap();
    b.set_update_prefs(UpdatePrefs {
        repo: "someone/private-fork".into(),
        include_prereleases: false,
    })
    .unwrap();
    let failed = b
        .check_for_updates(UpdateCheckRequest { force: true })
        .unwrap();
    (off, available, failed)
}

/// `samples.gen.json` is what the TS types are checked against (`samples.check.ts`).
/// Regenerate with `UPDATE_CONTRACT_SAMPLES=1 cargo test -p starorg-codex-app`.
/// Objects with sorted keys, whatever serde_json's `preserve_order` feature says (starorg-data
/// turns it on for the whole build).
pub(super) fn sorted(v: Value) -> Value {
    match v {
        Value::Object(m) => {
            let b: std::collections::BTreeMap<String, Value> =
                m.into_iter().map(|(k, v)| (k, sorted(v))).collect();
            Value::Object(b.into_iter().collect())
        }
        Value::Array(a) => Value::Array(a.into_iter().map(sorted).collect()),
        other => other,
    }
}

#[test]
fn samples_match_the_committed_file() {
    let path = app_src().join("samples.gen.json");
    let text = serde_json::to_string_pretty(&sorted(samples())).unwrap() + "\n";
    if std::env::var_os("UPDATE_CONTRACT_SAMPLES").is_some() {
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        committed == text,
        "{} is out of date: run UPDATE_CONTRACT_SAMPLES=1 cargo test -p starorg-codex-app, then npm run check",
        path.display()
    );
}

pub(super) fn names<T: serde::Serialize>(all: &[T]) -> Vec<String> {
    all.iter()
        .map(|v| {
            serde_json::to_value(v)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect()
}

/// `export type X = 'a' | 'b';` from types.ts, as the list of members.
pub(super) fn ts_union(ts: &str, name: &str) -> Vec<String> {
    let start = format!("export type {name} =");
    let at = ts
        .find(&start)
        .unwrap_or_else(|| panic!("types.ts has no `{start}`"));
    let rest = &ts[at + start.len()..];
    let end = rest.find(';').unwrap();
    rest[..end]
        .split('|')
        .map(|s| s.trim().trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[test]
fn ts_unions_list_every_variant() {
    let ts = std::fs::read_to_string(app_src().join("types.ts")).unwrap();
    use ErrorCode as E;
    let cases: Vec<(&str, Vec<String>)> = vec![
        (
            "ErrorCode",
            names(&[
                E::Invalid,
                E::NotFound,
                E::NeedsDecision,
                E::Busy,
                E::NetworkOff,
                E::Cancelled,
                E::NotImplemented,
                E::Internal,
                E::DevDisabled,
            ]),
        ),
        (
            "Language",
            names(&[Language::English, Language::Other, Language::Unknown]),
        ),
        (
            "CacheState",
            names(&[CacheState::Fresh, CacheState::Missing, CacheState::Stale]),
        ),
        (
            "FoundBy",
            names(&[
                FoundBy::Config,
                FoundBy::GameDir,
                FoundBy::Prefix,
                FoundBy::LugConfig,
                FoundBy::DefaultPrefix,
                FoundBy::Lutris,
                FoundBy::LauncherLog,
                FoundBy::CommonPath,
                FoundBy::Scan,
                FoundBy::Manual,
            ]),
        ),
        (
            "JobKind",
            names(&[
                JobKind::Discover,
                JobKind::Extract,
                JobKind::Apply,
                JobKind::Remove,
                JobKind::Restore,
                JobKind::Import,
                JobKind::Sync,
                JobKind::Hook,
                JobKind::Dev,
                JobKind::Mcp,
                JobKind::Uex,
            ]),
        ),
        (
            "JobPhase",
            names(&[
                JobPhase::Running,
                JobPhase::Done,
                JobPhase::Failed,
                JobPhase::Cancelled,
            ]),
        ),
        (
            "Preset",
            names(&[Preset::Recommended, Preset::Minimal, Preset::Everything]),
        ),
        (
            "ApplyResult",
            names(&[
                ApplyResult::Applied,
                ApplyResult::UpToDate,
                ApplyResult::Skipped,
                ApplyResult::Failed,
                ApplyResult::DryRun,
                ApplyResult::NeedsSetup,
                ApplyResult::NeedsDecision,
                ApplyResult::Removed,
                ApplyResult::Restored,
                ApplyResult::NothingToDo,
            ]),
        ),
        (
            "BackupReason",
            names(&[
                BackupReason::PreExisting,
                BackupReason::BeforeApply,
                BackupReason::BeforeRestore,
                BackupReason::Manual,
            ]),
        ),
        (
            "FileOrigin",
            names(&[FileOrigin::Ours, FileOrigin::Foreign, FileOrigin::None]),
        ),
        (
            "CategoryId",
            names(&[
                CategoryId::Ships,
                CategoryId::Components,
                CategoryId::ShipWeapons,
                CategoryId::FpsWeapons,
                CategoryId::Missiles,
                CategoryId::Missions,
                CategoryId::Obtain,
            ]),
        ),
        (
            "TagKind",
            names(&[TagKind::Components, TagKind::ShipWeapons, TagKind::Missiles]),
        ),
        (
            "OwnedMode",
            names(&[OwnedMode::Auto, OwnedMode::Any, OwnedMode::Character]),
        ),
        ("OverrideRule", names(&OverrideRule::ALL)),
        (
            "LocOp",
            names(&[LocOp::Set, LocOp::Append, LocOp::Prepend, LocOp::Replace]),
        ),
        (
            "TagStyle",
            names(&[TagStyle::Short, TagStyle::Medium, TagStyle::Long]),
        ),
        (
            "TagSeparator",
            names(&[
                TagSeparator::Dash,
                TagSeparator::None,
                TagSeparator::Space,
                TagSeparator::Underscore,
                TagSeparator::Dot,
                TagSeparator::Slash,
                TagSeparator::Pipe,
            ]),
        ),
        (
            "TagEnclosing",
            names(&[
                TagEnclosing::Square,
                TagEnclosing::Round,
                TagEnclosing::Curly,
                TagEnclosing::Angle,
                TagEnclosing::None,
            ]),
        ),
        (
            "TagPlacement",
            names(&[TagPlacement::Prefix, TagPlacement::Suffix]),
        ),
        (
            "StringSource",
            names(&[
                StringSource::Unmodified,
                StringSource::Enhanced,
                StringSource::Patched,
                StringSource::Edited,
            ]),
        ),
        (
            "StringSort",
            names(&[
                StringSort::Key,
                StringSort::KeyDesc,
                StringSort::Category,
                StringSort::Source,
                StringSort::Group,
                StringSort::Owner,
            ]),
        ),
        (
            "OwnerField",
            names(&[
                OwnerField::Name,
                OwnerField::ShortName,
                OwnerField::Description,
                OwnerField::Title,
                OwnerField::Career,
                OwnerField::Role,
            ]),
        ),
        (
            "OwnerKind",
            names(&[
                OwnerKind::Ship,
                OwnerKind::GroundVehicle,
                OwnerKind::Component,
                OwnerKind::ShipWeapon,
                OwnerKind::FpsWeapon,
                OwnerKind::Missile,
                OwnerKind::Mission,
                OwnerKind::Reputation,
                OwnerKind::Manufacturer,
                OwnerKind::Named,
                OwnerKind::Key,
            ]),
        ),
        (
            "GroupBy",
            names(&[
                GroupBy::None,
                GroupBy::Type,
                GroupBy::Manufacturer,
                GroupBy::Size,
            ]),
        ),
        (
            "GroupState",
            names(&[GroupState::Exact, GroupState::Approximate]),
        ),
        (
            "Layer",
            names(&[
                Layer::Stock,
                Layer::Generated,
                Layer::Patch,
                Layer::Override,
            ]),
        ),
        (
            "PatchState",
            names(&[
                PatchState::Applies,
                PatchState::NotNeeded,
                PatchState::TargetChanged,
                PatchState::NothingToAdd,
                PatchState::Missing,
                PatchState::OutOfRange,
                PatchState::Conflict,
                PatchState::Disabled,
                PatchState::NoData,
                PatchState::Retired,
            ]),
        ),
        (
            "PatchOrigin",
            names(&[PatchOrigin::BuiltIn, PatchOrigin::User]),
        ),
        ("ShopCategory", names(&ShopCategory::ALL)),
        (
            "CommodityDataState",
            names(&[
                CommodityDataState::Ready,
                CommodityDataState::NotRead,
                CommodityDataState::NoChannel,
            ]),
        ),
        (
            "ShopDataState",
            names(&[ShopDataState::Ready, ShopDataState::Missing]),
        ),
        (
            "LogLevel",
            names(&[LogLevel::Info, LogLevel::Warn, LogLevel::Error]),
        ),
        ("Theme", names(&[Theme::System, Theme::Dark, Theme::Light])),
        (
            "Accent",
            names(&[
                Accent::Cyan,
                Accent::Green,
                Accent::Amber,
                Accent::Orange,
                Accent::Rose,
                Accent::Violet,
            ]),
        ),
        ("Palette", names(&[Palette::Void, Palette::Black])),
        (
            "BlueprintKind",
            names(&[
                BlueprintKind::ShipComponent,
                BlueprintKind::ShipWeapon,
                BlueprintKind::FpsWeapon,
                BlueprintKind::Armor,
                BlueprintKind::FpsAmmo,
                BlueprintKind::MissionItem,
                BlueprintKind::Other,
            ]),
        ),
        (
            "CharacterMode",
            names(&[
                CharacterMode::Auto,
                CharacterMode::Manual,
                CharacterMode::Union,
            ]),
        ),
        (
            "StarOrgEnv",
            names(&[StarOrgEnv::Production, StarOrgEnv::Dev]),
        ),
        (
            "SyncDirection",
            names(&[SyncDirection::Pull, SyncDirection::Push]),
        ),
        (
            "TokenStoreKind",
            names(&[TokenStoreKind::Keyring, TokenStoreKind::File]),
        ),
        (
            "HookKind",
            names(&[HookKind::Prelaunch, HookKind::Postexit]),
        ),
        (
            "NameStyle",
            names(&[
                NameStyle::None,
                NameStyle::Numeric,
                NameStyle::GroupLetter,
                NameStyle::Symbol,
            ]),
        ),
        ("RestMode", names(&[RestMode::Stock, RestMode::Numbered])),
        (
            "ShipNamesScope",
            names(&[
                ShipNamesScope::Shared,
                ShipNamesScope::Channel,
                ShipNamesScope::Off,
            ]),
        ),
        (
            "FleetFormat",
            names(&[
                FleetFormat::XplorJson,
                FleetFormat::XplorCsv,
                FleetFormat::Text,
            ]),
        ),
        (
            "CheckLevel",
            names(&[
                CheckLevel::Ok,
                CheckLevel::Info,
                CheckLevel::Warn,
                CheckLevel::Error,
            ]),
        ),
        (
            "UpdateInstall",
            names(&[
                UpdateInstall::InApp,
                UpdateInstall::DownloadOnly,
                UpdateInstall::Store,
            ]),
        ),
        (
            "UpdateProblem",
            names(&[
                UpdateProblem::NotVisible,
                UpdateProblem::RateLimited,
                UpdateProblem::Offline,
                UpdateProblem::BadResponse,
            ]),
        ),
        (
            "NoteKind",
            names(&[NoteKind::Heading, NoteKind::Item, NoteKind::Text]),
        ),
    ];
    for (name, want) in cases {
        assert_eq!(ts_union(&ts, name), want, "types.ts union {name}");
    }
    // Tagged unions: the `state` tags.
    let tags = |v: Vec<Value>| -> Vec<String> {
        v.iter()
            .map(|x| x["state"].as_str().unwrap().to_string())
            .collect()
    };
    let apply_states = tags(vec![
        serde_json::to_value(ApplyState::Applied).unwrap(),
        serde_json::to_value(ApplyState::Stale { reasons: vec![] }).unwrap(),
        serde_json::to_value(ApplyState::ChangedByAnotherTool { tool: None }).unwrap(),
        serde_json::to_value(ApplyState::NotApplied).unwrap(),
    ]);
    assert_eq!(ts_union(&ts, "ApplyStateName"), apply_states);
    let hook_states = tags(vec![
        serde_json::to_value(HookState::Absent).unwrap(),
        serde_json::to_value(HookState::Current).unwrap(),
        serde_json::to_value(HookState::Outdated).unwrap(),
        serde_json::to_value(HookState::Broken { why: String::new() }).unwrap(),
        serde_json::to_value(HookState::Missing).unwrap(),
    ]);
    assert_eq!(ts_union(&ts, "HookStateName"), hook_states);
    let kinds: Vec<String> = [
        FleetSource::File,
        FleetSource::Text {
            text: String::new(),
        },
        FleetSource::Last,
        FleetSource::StarOrg,
    ]
    .iter()
    .map(|v| {
        serde_json::to_value(v).unwrap()["kind"]
            .as_str()
            .unwrap()
            .to_string()
    })
    .collect();
    assert_eq!(ts_union(&ts, "FleetSourceKind"), kinds);
    let release = match update_samples().1.check {
        UpdateCheck::Available { release } => release,
        other => panic!("{other:?}"),
    };
    let update_states = tags(vec![
        serde_json::to_value(UpdateCheck::NotChecked).unwrap(),
        serde_json::to_value(UpdateCheck::UpToDate).unwrap(),
        serde_json::to_value(UpdateCheck::Available { release }).unwrap(),
        serde_json::to_value(UpdateCheck::Failed {
            problem: UpdateProblem::Offline,
            message: String::new(),
        })
        .unwrap(),
    ]);
    assert_eq!(ts_union(&ts, "UpdateCheckState"), update_states);
}

#[test]
fn ship_names_contract_matches_the_core() {
    use starorg_core::fleet::store;
    let Some(src) = core_src("fleet/store.rs") else {
        eprintln!("core fleet sources not in this checkout: drift check skipped");
        return;
    };
    let b = MockBackend::default();
    let v = b
        .fleet_view(FleetViewRequest {
            target: live(),
            read_game_data: false,
        })
        .unwrap();
    let mut want: Vec<String> = rust_fields(&src, "ShipNames")
        .into_iter()
        .filter(|f| f != "schema" && f != "extra")
        .collect();
    want.sort();
    assert_eq!(json_keys(&serde_json::to_value(&v.settings).unwrap()), want);
    let mut entry: Vec<String> = rust_fields(&src, "ShipEntry")
        .into_iter()
        .filter(|f| f != "extra")
        .collect();
    entry.sort();
    let e = ShipEntry {
        hull: "h".into(),
        group: None,
        name: None,
        short: None,
    };
    assert_eq!(json_keys(&serde_json::to_value(&e).unwrap()), entry);
    // Enum values are the core's serde names.
    let core_styles = [
        store::Style::None,
        store::Style::Numeric,
        store::Style::GroupLetter,
        store::Style::Symbol,
    ];
    let ours = [
        NameStyle::None,
        NameStyle::Numeric,
        NameStyle::GroupLetter,
        NameStyle::Symbol,
    ];
    assert_eq!(names(&core_styles), names(&ours));
    assert_eq!(
        names(&[
            store::Scope::Shared,
            store::Scope::Channel,
            store::Scope::Off
        ]),
        names(&[
            ShipNamesScope::Shared,
            ShipNamesScope::Channel,
            ShipNamesScope::Off
        ])
    );
    assert_eq!(
        names(&[store::Rest::Stock, store::Rest::Numbered]),
        names(&[RestMode::Stock, RestMode::Numbered])
    );
    // Hull fields are a subset of the core's catalogue fields.
    let cat = rust_fields(&core_src("fleet/catalog.rs").unwrap(), "Hull");
    let h = serde_json::to_value(&v.catalog[0]).unwrap();
    for k in json_keys(&h) {
        assert!(cat.contains(&k), "FleetHull.{k} not in fleet::Hull");
    }
}

#[test]
fn fleet_import_dry_run_writes_nothing_and_new_hulls_join_the_order() {
    let b = MockBackend::default();
    let view = || {
        b.fleet_view(FleetViewRequest {
            target: live(),
            read_game_data: false,
        })
        .unwrap()
    };
    let req = |source, dry_run, merge| FleetImportRequest {
        target: live(),
        source,
        dry_run,
        merge,
        prune: false,
    };
    let r = b.fleet_import(req(FleetSource::File, true, false)).unwrap();
    assert!(r.dry_run && r.hulls.len() == 4, "{r:?}");
    assert_eq!(r.ambiguous[0].name, "Talon", "Mk I and Mk II both fit");
    assert_eq!(r.unmatched[0].name, "Starlance");
    let kite = r
        .hulls
        .iter()
        .find(|h| h.hull == "vehicle_NameKSTL_Kite")
        .unwrap();
    assert_eq!(
        (kite.count, kite.names.clone()),
        (2, vec!["Needle".to_string()])
    );
    let v = view();
    assert!(v.owned.is_empty() && v.settings.ships.is_empty());

    let r = b
        .fleet_import(req(FleetSource::File, false, false))
        .unwrap();
    assert_eq!(r.added_to_order, 4);
    let v = view();
    assert_eq!(v.owned.len(), 4);
    assert_eq!(v.settings.ships.len(), 4);
    assert!(!v.settings.enabled, "import never turns ship names on");
    // Pasted text, merged: counts are the larger, the order list only grows.
    let r = b
        .fleet_import(req(
            FleetSource::Text {
                text: "3x Kestrel Kite\nVolmar Moth\n".into(),
            },
            false,
            true,
        ))
        .unwrap();
    assert_eq!(r.format, FleetFormat::Text);
    assert_eq!(r.added_to_order, 1);
    let v = view();
    assert_eq!(
        v.owned
            .iter()
            .find(|h| h.hull == "vehicle_NameKSTL_Kite")
            .unwrap()
            .count,
        3
    );
    assert_eq!(v.settings.ships.len(), 5);
    // Hand match of the ambiguous row.
    let v = b
        .fleet_match(FleetMatchRequest {
            target: live(),
            name: "Talon".into(),
            hull: Some("vehicle_NameKSTL_Talon_Mk2".into()),
        })
        .unwrap();
    assert!(v.pending.iter().all(|p| p.name != "Talon"));
    assert_eq!(v.settings.ships.len(), 6);
    // Bad renames are refused before anything is saved.
    let mut s = v.settings.clone();
    s.ships[0].name = Some("<EM4>x".into());
    let e = b
        .save_ship_names(SaveShipNamesRequest {
            target: live(),
            scope: ShipNamesScope::Shared,
            settings: s,
        })
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid);
    // Off for HOTFIX leaves LIVE's list alone.
    let off = b
        .save_ship_names(SaveShipNamesRequest {
            target: hotfix(),
            scope: ShipNamesScope::Off,
            settings: v.settings.clone(),
        })
        .unwrap();
    assert_eq!(off.scope, ShipNamesScope::Off);
    assert_eq!(view().scope, ShipNamesScope::Shared);
}

#[test]
fn fleet_import_prune_drops_ships_the_new_fleet_lacks() {
    let b = MockBackend::default();
    let view = || {
        b.fleet_view(FleetViewRequest {
            target: live(),
            read_game_data: false,
        })
        .unwrap()
    };
    let req = |text: &str, dry_run, merge, prune| FleetImportRequest {
        target: live(),
        source: FleetSource::Text { text: text.into() },
        dry_run,
        merge,
        prune,
    };
    b.fleet_import(FleetImportRequest {
        target: live(),
        source: FleetSource::File,
        dry_run: false,
        merge: false,
        prune: false,
    })
    .unwrap();
    let v = view();
    assert_eq!(v.settings.ships.len(), 4);
    let kite = "vehicle_NameKSTL_Kite";
    let mut s = v.settings.clone();
    s.ships.iter_mut().find(|e| e.hull == kite).unwrap().name = Some("Needle".into());
    b.save_ship_names(SaveShipNamesRequest {
        target: live(),
        scope: ShipNamesScope::Shared,
        settings: s,
    })
    .unwrap();

    let list = "Kestrel Kite\nVolmar Moth\n";
    // Dry run: lists what would go, writes nothing.
    let r = b.fleet_import(req(list, true, false, true)).unwrap();
    assert!(r.prune);
    assert_eq!(r.removed_from_order.len(), 3, "{r:?}");
    assert!(r.removed_from_order.iter().all(|x| x.hull != kite));
    assert_eq!(view().settings.ships.len(), 4);
    // Refused: with merge, or when nothing matched (it would empty My ships).
    let e = b.fleet_import(req(list, true, true, true)).unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid);
    let e = b
        .fleet_import(req("Nothing Like A Ship\n", false, false, true))
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid);
    assert_eq!(view().settings.ships.len(), 4);

    let r = b.fleet_import(req(list, false, false, true)).unwrap();
    assert_eq!((r.removed_from_order.len(), r.added_to_order), (3, 1));
    let v = view();
    let hulls: Vec<_> = v.settings.ships.iter().map(|e| e.hull.as_str()).collect();
    assert_eq!(hulls.len(), 2);
    assert_eq!(hulls[0], kite, "ships that stay keep their place");
    let k = &v.settings.ships[0];
    assert_eq!(k.name.as_deref(), Some("Needle"), "and their rename");
    assert_eq!(v.owned.len(), 2);
    // Without prune the report says so.
    let r = b.fleet_import(req(list, true, false, false)).unwrap();
    assert!(!r.prune && r.removed_from_order.is_empty());
}

#[test]
fn ts_client_lists_every_command() {
    let ts = std::fs::read_to_string(app_src().join("client.ts")).unwrap();
    let start = ts
        .find("export const COMMANDS = [")
        .expect("client.ts has COMMANDS");
    let rest = &ts[start..];
    let body = &rest[rest.find('[').unwrap() + 1..rest.find(']').unwrap()];
    let listed: Vec<&str> = body
        .split(',')
        .map(|s| s.trim().trim_matches('\''))
        .filter(|s| !s.is_empty())
        .collect();
    assert_eq!(listed, COMMAND_NAMES);
    let src = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/contract/mod.rs"),
    )
    .unwrap();
    for name in COMMAND_NAMES {
        assert!(
            src.contains(&format!("$crate::contract::{name},"))
                || src.contains(&format!("$crate::contract::dev::{name},"))
                || src.contains(&format!("$crate::contract::assets::{name},")),
            "{name} missing from contract_handler!"
        );
    }
}

// ---------------------------------------------------------------------------------------
// Over IPC
// ---------------------------------------------------------------------------------------

pub(super) fn ipc(cmd: &str, body: Value) -> Result<Value, Value> {
    use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder, mock_context, noop_assets};
    let app = crate::builder(
        mock_builder(),
        crate::commands::DiscoveryConfig::default(),
        BackendState::new(MockBackend::default()),
    )
    .build(mock_context(noop_assets()))
    .unwrap();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    get_ipc_response(
        &webview,
        tauri::webview::InvokeRequest {
            cmd: cmd.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: if cfg!(windows) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .unwrap(),
            body: tauri::ipc::InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|r| r.deserialize::<Value>().unwrap())
}

#[test]
fn overview_over_ipc_is_first_run_with_synthetic_installs() {
    let v = ipc("overview", json!({})).unwrap();
    assert_eq!(v["contract"], CONTRACT_VERSION);
    assert_eq!(v["first_run"], true);
    assert_eq!(
        v["installs"][0]["channels"][0]["status"]["state"],
        "changed-by-another-tool"
    );
}

#[test]
fn query_strings_over_ipc_takes_a_snake_case_req() {
    let v = ipc(
        "query_strings",
        json!({ "req": serde_json::to_value(query("")).unwrap() }),
    )
    .unwrap();
    assert_eq!(v["all"], MOCK_STRING_COUNT);
    assert_eq!(v["rows"].as_array().unwrap().len(), 3);
}

#[test]
fn errors_cross_ipc_as_app_error() {
    let e = ipc("apply", json!({ "req": { "target": { "install": "inst-7f3a", "channel": "LIVE" }, "dry_run": false } })).unwrap_err();
    assert_eq!(e["code"], "needs-decision");
    assert!(
        e["detail"]
            .as_str()
            .unwrap()
            .contains("starorg-codex setup")
    );
}

// ---------------------------------------------------------------------------------------
// Mock behaviour the UI relies on
// ---------------------------------------------------------------------------------------

#[test]
fn setup_asks_when_the_language_is_not_english() {
    let b = MockBackend::default();
    let e = b
        .setup(SetupRequest {
            install: "inst-7f3a".into(),
            channels: vec![ChannelConsent {
                channel: "PTU".into(),
                consent: true,
                switch_language: false,
            }],
            preset: Preset::Minimal,
            fix_known_bugs: true,
        })
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::NeedsDecision);
    assert!(b.overview().unwrap().first_run);
}

#[test]
fn first_apply_pins_the_foreign_file_and_restore_needs_a_mismatch_flag() {
    let b = MockBackend::default();
    set_up(&b);
    assert!(!b.overview().unwrap().first_run);
    let r = b
        .apply(
            ApplyRequest {
                target: live(),
                dry_run: false,
            },
            &noop,
        )
        .unwrap();
    assert_eq!(r.result, ApplyResult::Applied);
    let list = b.list_backups(live()).unwrap();
    let f = list
        .offers
        .restore_foreign
        .clone()
        .expect("foreign restore offered");
    assert!(f.build_mismatch);
    assert_eq!(f.tool.as_deref(), Some("Smart Citizen"));
    assert!(list.offers.undo_last_apply.is_none());
    let e = b
        .restore_backup(RestoreRequest {
            target: live(),
            backup: f.backup.clone(),
            allow_build_mismatch: false,
        })
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::NeedsDecision);
    b.restore_backup(RestoreRequest {
        target: live(),
        backup: f.backup,
        allow_build_mismatch: true,
    })
    .unwrap();
    // A second apply over our own output makes "undo last apply" available.
    b.apply(
        ApplyRequest {
            target: live(),
            dry_run: false,
        },
        &noop,
    )
    .unwrap();
    b.apply(
        ApplyRequest {
            target: live(),
            dry_run: false,
        },
        &noop,
    )
    .unwrap();
    assert!(
        b.list_backups(live())
            .unwrap()
            .offers
            .undo_last_apply
            .is_some()
    );
}

#[test]
fn apply_reports_progress_steps_in_order() {
    let b = MockBackend::default();
    set_up(&b);
    let seen = std::sync::Mutex::new(Vec::new());
    b.apply(
        ApplyRequest {
            target: live(),
            dry_run: true,
        },
        &|p| seen.lock().unwrap().push((p.step, p.phase)),
    )
    .unwrap();
    let seen = seen.into_inner().unwrap();
    assert_eq!(seen.first(), Some(&(1, JobPhase::Running)));
    assert_eq!(seen.last(), Some(&(5, JobPhase::Done)));
}

#[test]
fn rejected_override_names_the_rule_and_reset_restores() {
    let b = MockBackend::default();
    let key = "item_NameSYN_Cooler_S2_00001";
    let r = b
        .set_override(OverrideRequest {
            target: live(),
            key: key.into(),
            value: "Cooler (mine)  |  50%".into(),
        })
        .unwrap();
    assert!(r.accepted);
    let pct = b
        .set_override(OverrideRequest {
            target: live(),
            key: key.into(),
            value: "50% off".into(),
        })
        .unwrap();
    assert_eq!(pct.rule, Some(OverrideRule::PercentRule));
    assert_eq!(r.detail.unwrap().row.source, StringSource::Edited);
    let bad = b
        .set_override(OverrideRequest {
            target: live(),
            key: key.into(),
            value: "a\nb".into(),
        })
        .unwrap();
    assert!(!bad.accepted);
    assert_eq!(bad.rule, Some(OverrideRule::ControlCharacter));
    let d = b
        .reset_override(KeyRequest {
            target: live(),
            key: key.into(),
        })
        .unwrap();
    assert!(d.override_value.is_none());
}

#[test]
fn strings_query_filters_and_pages_the_whole_table() {
    let b = MockBackend::default();
    let mut q = query("");
    q.changed_only = true;
    q.limit = 50;
    let p = b.query_strings(q.clone()).unwrap();
    assert!(p.total > 0 && p.total < p.all);
    assert!(p.rows.iter().all(|r| r.source != StringSource::Unmodified));
    q.offset = p.total - 1;
    assert_eq!(b.query_strings(q).unwrap().rows.len(), 1);
}

/// Strings owners: every page row has an owner (a readable-key fallback when nothing owns
/// the key), search matches owner names, the owner sort puts owned keys first, and a shared
/// key lists every owner in its detail.
#[test]
fn strings_rows_carry_readable_owners() {
    let b = MockBackend::default();
    let p = b
        .query_strings(StringsQuery {
            limit: 1000,
            ..query("")
        })
        .unwrap();
    assert!(p.rows.iter().all(|r| r.owner.is_some()));
    let ui_page = b.query_strings(query("ui_SYN")).unwrap();
    let ui = &ui_page.rows[0];
    let o = ui.owner.as_deref().unwrap();
    assert_eq!((o.kind, o.field, ui.owners), (OwnerKind::Key, None, 0));
    assert_eq!(o.name, starorg_core::groups::owners::pretty_key(&ui.key));
    // Only the owner says "Sample Rifle": the key and text do not.
    let hits = b.query_strings(query("sample rifle 1")).unwrap();
    assert!(hits.total > 0);
    assert!(hits.rows.iter().all(|r| {
        let o = r.owner.as_deref().unwrap();
        o.kind == OwnerKind::FpsWeapon && o.field == Some(OwnerField::Description)
    }));
    // Owned keys first, A-Z by owner name.
    let sorted = b
        .query_strings(StringsQuery {
            sort: StringSort::Owner,
            limit: 50,
            ..query("")
        })
        .unwrap();
    let names: Vec<String> = sorted
        .rows
        .iter()
        .map(|r| r.owner.as_deref().unwrap().name.to_lowercase())
        .collect();
    assert!(names.windows(2).all(|w| w[0] <= w[1]), "{names:?}");
    assert!(sorted.rows.iter().all(|r| r.owners > 0));
    // A shared ship-weapon key lists both owners, primary first.
    let lasers = b
        .query_strings(StringsQuery {
            limit: 50,
            ..query("SYN_Laser")
        })
        .unwrap();
    let shared = lasers.rows.iter().find(|r| r.owners > 1).unwrap();
    let d = b
        .string_detail(KeyRequest {
            target: live(),
            key: shared.key.clone(),
        })
        .unwrap();
    assert_eq!(d.owners.len() as u32, shared.owners);
    assert_eq!(Some(&d.owners[0]), shared.owner.as_deref());
    assert!(d.owners[1].name.ends_with("Turret"));
    // Search finds a key by a secondary owner too.
    let turrets = b
        .query_strings(StringsQuery {
            limit: 50,
            ..query("repeater 10 turret")
        })
        .unwrap();
    assert!(turrets.total > 0);
    assert!(turrets.rows.iter().all(|r| r.owners > 1));
}

/// Addendum 8.4: one group per row, tree counts follow the facet rule, a parent selects its
/// children, the flat modes end with "none".
#[test]
fn strings_groups_follow_the_facet_rule() {
    let b = MockBackend::default();
    set_up(&b);
    let mut q = query("");
    q.limit = 1000;
    let p = b.query_strings(q.clone()).unwrap();
    assert_eq!(p.group_state, GroupState::Exact);
    let leaves: u32 = p
        .groups
        .iter()
        .filter(|g| {
            !p.groups
                .iter()
                .any(|c| c.parent.as_deref() == Some(g.id.as_str()))
        })
        .map(|g| g.count)
        .sum();
    assert_eq!(leaves, p.total, "leaf counts add up to the rows");
    let node = |p: &StringsPage, id: &str| p.groups.iter().find(|g| g.id == id).cloned().unwrap();
    let comps = node(&p, "components");
    assert!(comps.count > 0 && comps.enhanced > 0);
    assert_eq!(
        comps.count,
        p.groups
            .iter()
            .filter(|g| g.parent.as_deref() == Some("components"))
            .map(|g| g.count)
            .sum::<u32>()
    );
    // Zero-count groups stay in the tree (dimmed in the UI).
    assert_eq!(node(&p, "blueprints").count, 0);

    // Selecting a parent filters to its children; the tree counts do not change.
    q.groups = vec!["components".into()];
    let sel = b.query_strings(q.clone()).unwrap();
    assert_eq!(sel.total, comps.count);
    assert!(sel.rows.iter().all(|r| r.group.starts_with("components/")));
    assert_eq!(sel.groups, p.groups);
    // A leaf and a manufacturer together; the source facet reflects the group filter.
    q.groups = vec!["components/coolers".into()];
    q.manufacturers = vec!["SYNA".into()];
    let both = b.query_strings(q.clone()).unwrap();
    assert!(both.total > 0);
    assert!(
        both.rows
            .iter()
            .all(|r| r.group == "components/coolers" && r.manufacturer.as_deref() == Some("SYNA"))
    );
    assert_eq!(
        both.sources.iter().map(|s| s.count).sum::<u32>(),
        both.total
    );

    // Manufacturer mode: flat, "none" last; its counts ignore the manufacturer filter only.
    q.group_by = GroupBy::Manufacturer;
    let m = b.query_strings(q.clone()).unwrap();
    assert_eq!(m.groups.last().unwrap().id, "none");
    assert!(m.groups.iter().all(|g| g.parent.is_none()));
    let syna = m.groups.iter().find(|g| g.id == "SYNA").unwrap();
    assert_eq!(syna.count, both.total);
    assert!(m.groups.iter().any(|g| g.id == "SYNC" && g.count > 0));

    // Size mode and sort by group.
    q = query("");
    q.group_by = GroupBy::Size;
    q.sort = StringSort::Group;
    q.sizes = vec![0];
    q.limit = 5;
    let s = b.query_strings(q).unwrap();
    assert!(s.rows.iter().all(|r| r.size.is_none()));
    assert_eq!(s.groups.last().unwrap().id, "none");
    assert_eq!(s.groups.first().unwrap().id, "1");

    // None: no tree.
    let mut q = query("");
    q.group_by = GroupBy::None;
    assert!(b.query_strings(q).unwrap().groups.is_empty());

    // HOTFIX starts approximate until the pass runs.
    let mut q = query("");
    q.target = hotfix();
    assert_eq!(
        b.query_strings(q.clone()).unwrap().group_state,
        GroupState::Approximate
    );
    assert_eq!(b.read_string_groups(hotfix()).unwrap(), GroupState::Exact);
    assert_eq!(b.query_strings(q).unwrap().group_state, GroupState::Exact);
}

/// The TS mock assigns groups with the same table (`mockGroup` in mock.ts).
#[test]
fn mock_groups_match_the_ts_table() {
    let ts = std::fs::read_to_string(app_src().join("mock.ts")).unwrap();
    for (cat, i) in [
        ("components", 16),
        ("items", 40),
        ("ui", 24),
        ("ship-weapons", 0),
    ] {
        let (g, m, s) = super::mock::mock_group(cat, i);
        let line = format!(
            "// pin: {cat} {i} -> {g} {} {}",
            m.unwrap_or("-"),
            s.map(|s| s.to_string()).unwrap_or("-".into())
        );
        assert!(ts.contains(&line), "mock.ts lacks `{line}`");
    }
}

#[test]
fn starorg_push_is_the_union_of_ticked_characters() {
    let b = MockBackend::default();
    assert_eq!(
        b.starorg_login("LIVE".into()).unwrap_err().code,
        ErrorCode::NetworkOff
    );
    let mut s = b.get_settings().unwrap();
    s.network.starorg = true;
    b.save_settings(s).unwrap();
    let st = b.starorg_login("LIVE".into()).unwrap();
    assert!(st.connected);
    // Pilot_A owns 6, Alt_B owns 4 with one in common: union = 9.
    assert_eq!(st.push_count, 9);
    let st = b
        .set_sync_characters(SyncCharactersRequest {
            channel: "LIVE".into(),
            ticked: vec!["1000000000002".into()],
        })
        .unwrap();
    assert_eq!(st.push_count, 4);
    let st = b
        .set_sync_characters(SyncCharactersRequest {
            channel: "LIVE".into(),
            ticked: vec![],
        })
        .unwrap();
    assert_eq!(st.push_count, 0);
}

#[test]
fn preview_follows_profile_switches() {
    let b = MockBackend::default();
    let mut p = b.preset_profile(Preset::Recommended).unwrap();
    let with = b
        .build_preview(PreviewRequest {
            target: live(),
            profile: Some(p.clone()),
            preset: None,
        })
        .unwrap();
    assert!(with[0].result.contains("--- STATS ---"));
    p.ships = false;
    let without = b
        .build_preview(PreviewRequest {
            target: live(),
            profile: Some(p),
            preset: None,
        })
        .unwrap();
    assert_eq!(without[0].result, without[0].original);
}

#[test]
fn preview_has_where_to_get_samples_that_follow_mode_and_lines() {
    let b = MockBackend::default();
    let get = |p: &Profile| {
        b.build_preview(PreviewRequest {
            target: live(),
            profile: Some(p.clone()),
            preset: None,
        })
        .unwrap()
        .into_iter()
        .filter(|i| i.category == "obtain-hints")
        .collect::<Vec<_>>()
    };
    let mut p = b.preset_profile(Preset::Recommended).unwrap();
    let s = get(&p);
    assert_eq!(s.len(), 2, "{s:?}");
    let (gun, ore) = (&s[0].result, &s[1].result);
    // After the stats block; Normal: header, no odds, no shops (off by default).
    let stats = gun.find("--- STATS ---").unwrap();
    let block = gun.find("<EM3>WHERE TO GET</EM3>").unwrap();
    assert!(stats < block, "{gun}");
    assert!(
        gun.contains("<EM4>Loot:</EM4> Weapons rack (Rare) at Bunkers"),
        "{gun}"
    );
    assert!(!gun.contains('%') && !gun.contains("Center Mass"), "{gun}");
    assert!(gun.contains("<EM4>Craft:</EM4>") && gun.contains("Synthetic Recovery [BP]"));
    assert!(ore.contains("<EM4>Mined:</EM4> Lyria") && ore.contains("<EM4>Quality:</EM4>"));
    // Short: one Get: line.
    p.obtain_mode = ObtainMode::Short;
    let s = get(&p);
    assert!(
        s[0].result
            .contains("<EM4>Get:</EM4> Loot (Rare) · Craft [BP] · Reward")
    );
    assert!(!s[0].result.contains("WHERE TO GET"));
    // Detailed with shops: odds and shop names, never prices.
    p.obtain_mode = ObtainMode::Detailed;
    p.obtain_shops = true;
    let s = get(&p);
    assert!(
        s[0].result
            .contains("<EM4>Buy:</EM4> Center Mass (Area18, Stanton)")
    );
    assert!(s[0].result.contains("21%") && !s[0].result.contains("aUEC"));
    // Line toggles: loot off leaves the line out; every line off drops the sample.
    p.lines_off = vec!["obtain.loot".into()];
    assert!(!get(&p)[0].result.contains("Loot:"));
    p.lines_off = starorg_core::enhance::obtain::LINES
        .iter()
        .map(|l| format!("obtain.{l}"))
        .collect();
    assert!(get(&p).is_empty());
    p.lines_off.clear();
    p.obtain = false;
    assert!(get(&p).is_empty());
}

#[test]
fn obtain_schema_lines_are_the_core_lines() {
    let s = MockBackend::default().profile_schema().unwrap();
    let c = s
        .categories
        .iter()
        .find(|c| c.id == CategoryId::Obtain)
        .unwrap();
    let want: Vec<String> = starorg_core::enhance::obtain::LINES
        .iter()
        .map(|l| format!("{}.{l}", starorg_core::enhance::obtain::LINE_CATEGORY))
        .collect();
    let got: Vec<String> = c.lines.iter().map(|l| l.key.clone()).collect();
    assert_eq!(got, want);
}

/// Token not set, then saved; "Fetch now" with the switch on (the mock writes nothing).
fn uex_samples() -> (UexTokenStatus, UexTokenStatus, ObtainStatus) {
    let b = MockBackend::default();
    let off = b.uex_token_status().unwrap();
    let on = b
        .set_uex_token(UexTokenRequest {
            token: Some("sample-token".into()),
        })
        .unwrap();
    let mut s = b.get_settings().unwrap();
    s.network.uex = true;
    b.save_settings(s).unwrap();
    (off, on, b.fetch_uex().unwrap())
}

/// The Shops screen before "Fetch now" (`missing`), after it, one shop's items, and every
/// shop's (the TS mock serves `shop_items` from that list).
fn shop_samples() -> Vec<(&'static str, Value)> {
    let b = MockBackend::default();
    let req = || ShopInventoryRequest {
        target: Some(live()),
    };
    let missing = b.shop_inventory(req()).unwrap();
    let mut s = b.get_settings().unwrap();
    s.network.uex = true;
    b.save_settings(s).unwrap();
    b.fetch_uex().unwrap();
    let ready = b.shop_inventory(req()).unwrap();
    let first = ready.systems[0].locations[0].shops[0].id;
    let items = b
        .shop_items(ShopItemsRequest {
            target: Some(live()),
            terminal: first,
        })
        .unwrap();
    // After "Read game data" on LIVE: ship items link too.
    set_up(&b);
    b.read_game_data(ReadGameDataRequest { target: live() })
        .unwrap();
    let linked = b.shop_inventory(req()).unwrap();
    use super::mock_shops::mock_all_items;
    vec![
        ("shop_inventory_missing", json!(missing)),
        ("shop_inventory", json!(ready)),
        ("shop_inventory_linked", json!(linked)),
        ("shop_items", json!(items)),
        ("shop_items_all", json!(mock_all_items(false))),
        ("shop_items_all_linked", json!(mock_all_items(true))),
    ]
}

/// The Commodities screen: before "Read game data" (`not-read`), after it without and with
/// the UEX files, and every resource's detail both ways (the TS mock serves
/// `commodity_detail` from those lists).
fn commodity_samples() -> Vec<(&'static str, Value)> {
    let b = MockBackend::default();
    let req = || CommodityListRequest {
        target: Some(live()),
    };
    let not_read = b.commodity_list(req()).unwrap();
    set_up(&b);
    b.read_game_data(ReadGameDataRequest { target: live() })
        .unwrap();
    let ready = b.commodity_list(req()).unwrap();
    let mut s = b.get_settings().unwrap();
    s.network.uex = true;
    b.save_settings(s).unwrap();
    b.fetch_uex().unwrap();
    let priced = b.commodity_list(req()).unwrap();
    let detail = b
        .commodity_detail(CommodityDetailRequest {
            target: Some(live()),
            id: "ResourceType.Raw_Fixtanium".into(),
        })
        .unwrap();
    use super::mock_commodities::mock_all_details;
    vec![
        ("commodity_list_not_read", json!(not_read)),
        ("commodity_list", json!(ready)),
        ("commodity_list_priced", json!(priced)),
        ("commodity_detail", json!(detail)),
        ("commodity_details_all", json!(mock_all_details(false))),
        (
            "commodity_details_all_priced",
            json!(mock_all_details(true)),
        ),
    ]
}

#[test]
fn commodities_on_the_mock_list_summaries_and_detail_on_request() {
    let b = MockBackend::default();
    let none = b
        .commodity_list(CommodityListRequest { target: None })
        .unwrap();
    assert_eq!(none.state, CommodityDataState::NoChannel);
    let req = || CommodityListRequest {
        target: Some(live()),
    };
    let unread = b.commodity_list(req()).unwrap();
    assert_eq!(unread.state, CommodityDataState::NotRead);
    assert!(unread.commodities.is_empty());
    set_up(&b);
    b.read_game_data(ReadGameDataRequest { target: live() })
        .unwrap();
    let l = b.commodity_list(req()).unwrap();
    assert_eq!(l.state, CommodityDataState::Ready);
    assert!(!l.market.loaded);
    // Placeholders are left out; groups read as words.
    assert!(l.commodities.iter().all(|c| c.group != "Heat Placeholder"));
    let ore = l
        .commodities
        .iter()
        .find(|c| c.id == "ResourceType.Ore_Synthium")
        .unwrap();
    assert_eq!(
        (ore.group.as_str(), ore.subgroup.as_deref()),
        ("Metal", Some("Unrefined Ores"))
    );
    assert!(ore.raw && !ore.volatile);
    let d = |id: &str| {
        b.commodity_detail(CommodityDetailRequest {
            target: Some(live()),
            id: id.into(),
        })
        .unwrap()
        .commodity
        .unwrap()
    };
    let raw = d("ResourceType.Raw_Fixtanium");
    assert_eq!(raw.refined.as_ref().unwrap().name, "Fixtanium");
    assert!(raw.volatility.is_some() && raw.damage.len() == 1);
    let mined = raw.mined.as_ref().unwrap();
    assert_eq!(mined[0].belts.len(), 2);
    assert!(!raw.market.loaded);
    let stub = d("Stubbite");
    assert_eq!(stub.mined.as_ref().unwrap()[0].spots.len(), 2);
    let refined = d("ResourceType.Synthium");
    assert!(refined.mined.is_none());
    assert_eq!(refined.raw_sources[0].id, "ResourceType.Ore_Synthium");
    // After "Fetch now": prices on both sides.
    let mut s = b.get_settings().unwrap();
    s.network.uex = true;
    b.save_settings(s).unwrap();
    b.fetch_uex().unwrap();
    let h = d("ResourceType.Hydrogen");
    assert!(h.market.loaded);
    // Levski Admin (112) is not on LIVE: left out.
    assert_eq!(h.market.sell[0].price, 105.0);
    assert_eq!(h.market.buy[0].price, 92.0);
    assert!(
        b.commodity_detail(CommodityDetailRequest {
            target: Some(live()),
            id: "Nope".into()
        })
        .unwrap()
        .commodity
        .is_none()
    );
}

#[test]
fn shops_on_the_mock_group_categorise_and_link() {
    let b = MockBackend::default();
    let req = ShopInventoryRequest { target: None };
    let d = b.shop_inventory(req.clone()).unwrap();
    assert_eq!(d.state, ShopDataState::Missing);
    assert!(d.systems.is_empty() && d.folder.is_some());
    let mut s = b.get_settings().unwrap();
    s.network.uex = true;
    b.save_settings(s).unwrap();
    b.fetch_uex().unwrap();
    let d = b.shop_inventory(req).unwrap();
    assert_eq!(d.state, ShopDataState::Ready);
    assert_eq!(d.fetched.as_deref(), Some(super::mock::MOCK_UEX_FETCHED));
    let systems: Vec<&str> = d.systems.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(systems, ["Nyx", "Pyro", "Stanton"]);
    assert_eq!(d.shop_count, 7);
    let all: Vec<&ShopSummary> = d
        .systems
        .iter()
        .flat_map(|s| s.locations.iter().flat_map(|l| l.shops.iter()))
        .collect();
    let dumper = all.iter().find(|s| s.name == "Dumper's GrimHEX").unwrap();
    assert_eq!(dumper.location, "Grim HEX, Yela, Crusader");
    let cats: Vec<ShopCategory> = dumper.categories.iter().map(|c| c.category).collect();
    assert_eq!(
        cats,
        [
            ShopCategory::ShipWeapons,
            ShopCategory::Missiles,
            ShopCategory::ShipComponents,
            ShopCategory::Mining
        ]
    );
    assert!(all.iter().any(|s| s.not_on_live));
    let items = b
        .shop_items(ShopItemsRequest {
            target: None,
            terminal: dumper.id,
        })
        .unwrap();
    assert_eq!(items.items.len(), dumper.items);
    // Before the channel's game data is read only item-tags links: Dumper's sells ship items,
    // so its ship guns, missiles and components do not link yet; every row has its category.
    assert!(!d.ship_items);
    assert!(items.items.iter().all(|i| i.record.is_none()
        || !matches!(
            i.category,
            ShopCategory::ShipWeapons | ShopCategory::Missiles | ShopCategory::ShipComponents
        )));
    set_up(&b);
    b.read_game_data(ReadGameDataRequest { target: live() })
        .unwrap();
    let d = b
        .shop_inventory(ShopInventoryRequest {
            target: Some(live()),
        })
        .unwrap();
    assert!(d.ship_items);
    let items = b
        .shop_items(ShopItemsRequest {
            target: Some(live()),
            terminal: dumper.id,
        })
        .unwrap();
    // Catalogue rows link to their record; the extra repeater does not, and is a ship gun.
    let rep = items
        .items
        .iter()
        .find(|i| i.name == "SYN-9 Repeater")
        .unwrap();
    assert!(rep.record.is_none() && rep.category == ShopCategory::ShipWeapons);
    let linked = items.items.iter().filter(|i| i.record.is_some()).count();
    assert_eq!(linked, items.items.len() - 1);
    assert!(
        items
            .items
            .iter()
            .filter(|i| i.record.is_some())
            .all(|i| i.price_sell.is_some())
    );
    // A missile's UEX name lacks the kind word.
    assert!(items.items.iter().any(|i| i.uex_name.is_some()));
    assert!(
        b.shop_items(ShopItemsRequest {
            target: None,
            terminal: 1
        })
        .unwrap()
        .shop
        .is_none()
    );
}

#[test]
fn read_game_data_on_the_mock_needs_setup_and_freshens_the_cache() {
    let b = MockBackend::default();
    let e = b
        .read_game_data(ReadGameDataRequest { target: live() })
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::NeedsDecision);
    set_up(&b);
    let s = b
        .read_game_data(ReadGameDataRequest { target: live() })
        .unwrap();
    assert_eq!(s.cache, CacheState::Fresh);
    assert_eq!(
        ipc("read_game_data", json!({ "req": { "target": live() } })).unwrap_err()["code"],
        "needs-decision"
    );
}

#[test]
fn uex_on_the_mock_needs_its_switch_and_never_returns_the_token() {
    let b = MockBackend::default();
    let e = b.fetch_uex().unwrap_err();
    assert_eq!(e.code, ErrorCode::NetworkOff);
    assert!(!b.obtain_status().unwrap().shops_ready);
    let (off, on, fetched) = uex_samples();
    assert_eq!(
        off,
        UexTokenStatus {
            set: false,
            store: None
        }
    );
    assert!(on.set);
    assert!(!json!(on).to_string().contains("sample-token"));
    assert!(fetched.shops_ready);
    // An empty token removes it; spaces inside are refused.
    assert!(
        !b.set_uex_token(UexTokenRequest {
            token: Some("  ".into())
        })
        .unwrap()
        .set
    );
    assert_eq!(
        b.set_uex_token(UexTokenRequest {
            token: Some("a b".into())
        })
        .unwrap_err()
        .code,
        ErrorCode::Invalid
    );
    // Over IPC: the request takes `token`, the status never has one.
    assert_eq!(
        ipc("set_uex_token", json!({ "req": { "token": "t-1" } })).unwrap(),
        json!({ "set": true, "store": "keyring" })
    );
    assert_eq!(
        ipc("fetch_uex", json!({})).unwrap_err()["code"],
        "network-off"
    );
}

#[test]
fn obtain_status_lists_the_uex_files_without_reading_them() {
    let dir = std::env::temp_dir().join(format!("starorg-uex-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let st = super::real::obtain_status_in(&dir);
    assert!(!st.shops_ready && !st.prices_ready);
    assert_eq!(st.files.len(), 3);
    assert!(st.files.iter().all(|f| !f.present && f.modified.is_none()));
    // Terminals and item prices: the shops line can work, the sell line not yet.
    for f in ["uex-terminals.json", "uex-item-prices.json"] {
        std::fs::write(dir.join(f), "not json").unwrap();
    }
    let st = super::real::obtain_status_in(&dir);
    assert!(st.shops_ready && !st.prices_ready, "{st:?}");
    for f in super::real::UEX_FILES {
        std::fs::write(dir.join(f), "not json").unwrap();
    }
    let st = super::real::obtain_status_in(&dir);
    assert!(st.shops_ready && st.prices_ready, "{st:?}");
    assert!(st.files.iter().all(|f| f.modified.is_some()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn preview_shows_marker_colours_and_the_colour_test() {
    let b = MockBackend::default();
    let mut p = b.preset_profile(Preset::Recommended).unwrap();
    let get = |p: &Profile, key: &str| {
        b.build_preview(PreviewRequest {
            target: live(),
            profile: Some(p.clone()),
            preset: None,
        })
        .unwrap()
        .into_iter()
        .find(|i| i.key == key)
    };
    let desc = get(&p, "mission_DescSYN_Recovery").unwrap();
    assert!(
        desc.result
            .contains("\\n- Sample Rifle [Owned]\\n- Sample Pistol\\n")
    );
    assert!(get(&p, "mG_ContractsMgr_Select").is_none());
    p.mission.owned_em = 4;
    p.mission.missing_marker = true;
    p.mission.missing_em = 2;
    p.colour_test = true;
    let desc = get(&p, "mission_DescSYN_Recovery").unwrap();
    assert!(
        desc.result
            .contains("- Sample Rifle <EM4>[Owned]</EM4>\\n- Sample Pistol <EM2>[Missing]</EM2>"),
        "{}",
        desc.result
    );
    let t = get(&p, "mG_ContractsMgr_Select").unwrap();
    assert!(t.result.ends_with("<EM5>EM5</EM5>"));
}

#[test]
fn preview_title_shows_bp_owned_when_it_is_on() {
    let b = MockBackend::default();
    let mut p = b.preset_profile(Preset::Recommended).unwrap();
    let title = |p: &Profile| {
        b.build_preview(PreviewRequest {
            target: live(),
            profile: Some(p.clone()),
            preset: None,
        })
        .unwrap()
        .into_iter()
        .find(|i| i.key == "mission_TitleSYN_Recovery")
        .unwrap()
        .result
    };
    p.mission.title_blueprint = true;
    p.mission.title_owned = false;
    assert!(title(&p).contains("[BP]"));
    p.mission.title_owned = true;
    let t = title(&p);
    assert!(t.contains("[BP Owned]") && !t.contains("[BP]"), "{t}");
}

#[test]
fn starorg_pull_and_push_need_a_connection_and_respect_q22() {
    let b = MockBackend::default();
    let pull = || {
        b.starorg_pull(StarOrgPullRequest {
            channel: "LIVE".into(),
            character: "1000000000002".into(),
        })
    };
    assert_eq!(pull().unwrap_err().code, ErrorCode::NetworkOff);
    let mut s = b.get_settings().unwrap();
    s.network.starorg = true;
    b.save_settings(s).unwrap();
    assert_eq!(pull().unwrap_err().code, ErrorCode::NeedsDecision);
    b.starorg_login("LIVE".into()).unwrap();
    // Each kind needs its "What to sync" switch.
    assert_eq!(pull().unwrap_err().code, ErrorCode::Invalid);
    assert_eq!(
        b.starorg_push("LIVE".into()).unwrap_err().code,
        ErrorCode::Invalid
    );
    let mut s = b.get_settings().unwrap();
    s.starorg.pull_blueprints = true;
    s.starorg.push_blueprints = true;
    b.save_settings(s).unwrap();
    assert!(b.starorg_status("LIVE".into()).unwrap().connected);
    // Pull writes into the one chosen character (union, never delete).
    let r = pull().unwrap();
    assert_eq!(r.direction, SyncDirection::Pull);
    assert_eq!(r.added.len(), 2);
    assert_eq!(r.already, 1);
    let alt = b
        .owned_blueprints(CharacterRequest {
            target: live(),
            character: "1000000000002".into(),
        })
        .unwrap();
    assert_eq!(alt.len(), 6);
    let main = b
        .owned_blueprints(CharacterRequest {
            target: live(),
            character: "1000000000001".into(),
        })
        .unwrap();
    assert_eq!(main.len(), 6, "pull never spreads across alts");
    // Push sends the union of ticked characters: Pilot_A 6 + Alt_B now 6, 1 shared = 11.
    assert_eq!(b.starorg_push("LIVE".into()).unwrap().sent, 11);
    b.set_sync_characters(SyncCharactersRequest {
        channel: "LIVE".into(),
        ticked: vec!["1000000000001".into()],
    })
    .unwrap();
    assert_eq!(b.starorg_push("LIVE".into()).unwrap().sent, 6);
}

#[test]
fn starorg_environment_switch_signs_out_and_sync_follows_the_switches() {
    let b = MockBackend::default();
    let mut s = b.get_settings().unwrap();
    assert_eq!(s.starorg, StarOrgSettings::default());
    assert!(s.starorg.import_game_log && !s.starorg.push_blueprints);
    s.network.starorg = true;
    let s = b.save_settings(s).unwrap();
    let st = b.starorg_login("LIVE".into()).unwrap();
    assert_eq!(st.host, "starorg.tools");
    assert_eq!(st.guilds.len(), 2);
    assert_eq!(st.token_store, Some(TokenStoreKind::Keyring));
    // Nothing on: "Sync now" refuses, the hangar too.
    assert_eq!(
        b.starorg_sync("LIVE".into()).unwrap_err().code,
        ErrorCode::Invalid
    );
    let hangar = FleetImportRequest {
        target: live(),
        source: FleetSource::StarOrg,
        dry_run: true,
        merge: true,
        prune: false,
    };
    assert_eq!(
        b.fleet_import(hangar.clone()).unwrap_err().code,
        ErrorCode::Invalid
    );
    let mut on = s.clone();
    on.starorg.pull_fleet = true;
    on.starorg.push_blueprints = true;
    b.save_settings(on.clone()).unwrap();
    assert!(b.fleet_import(hangar).unwrap().total > 0);
    let run = b.starorg_sync("LIVE".into()).unwrap();
    assert_eq!(run.reports.len(), 1);
    assert_eq!(run.reports[0].direction, SyncDirection::Push);
    assert!(run.fleet.is_some() && run.skipped.is_empty());
    // Another org.
    let st = b
        .set_starorg_guild(StarOrgGuildRequest {
            channel: "LIVE".into(),
            guild: "100000000000000002".into(),
        })
        .unwrap();
    assert_eq!(st.guild.as_deref(), Some("Other Test Org"));
    assert!(
        b.set_starorg_guild(StarOrgGuildRequest {
            channel: "LIVE".into(),
            guild: "nope".into(),
        })
        .is_err()
    );
    // Dev: signed out, other host.
    on.starorg.environment = StarOrgEnv::Dev;
    b.save_settings(on).unwrap();
    let st = b.starorg_status("LIVE".into()).unwrap();
    assert!(!st.connected && st.user.is_none());
    assert_eq!(st.host, "dev.starorg.tools");
    assert_eq!(st.environment, StarOrgEnv::Dev);
}

#[test]
fn diagnostics_are_redacted() {
    let b = MockBackend::default();
    let d = b.export_diagnostics().unwrap();
    assert!(d.redacted > 0);
    for leak in ["/home/pilot", "1000000000001", "Pilot_A", "pilot_a_discord"] {
        assert!(!d.text.contains(leak), "diagnostics leak {leak}");
    }
    assert!(d.text.contains("<geid>"));
}

#[test]
fn apply_records_the_applied_build_and_last_apply() {
    let b = MockBackend::default();
    set_up(&b);
    assert!(b.channel_status(live()).unwrap().last_apply.is_none());
    b.apply(
        ApplyRequest {
            target: live(),
            dry_run: false,
        },
        &noop,
    )
    .unwrap();
    let c = b.channel_status(live()).unwrap();
    assert_eq!(c.applied_build.as_ref(), Some(&c.build));
    let last = c.last_apply.unwrap();
    assert_eq!(last.result, ApplyResult::Applied);
    assert!(last.changed > 0);
    let d = b
        .patch_detail(PatchRef {
            target: live(),
            id: "syn-moved-key".into(),
        })
        .unwrap();
    assert_eq!(d.patch.state, PatchState::TargetChanged);
    assert!(!d.examples.is_empty());
}

#[test]
fn apply_skips_while_the_game_runs_and_says_why() {
    let b = MockBackend::default();
    set_up(&b);
    b.set_game_running(&live(), true);
    let r = b
        .apply(
            ApplyRequest {
                target: live(),
                dry_run: false,
            },
            &noop,
        )
        .unwrap();
    assert_eq!(r.result, ApplyResult::Skipped);
    assert!(r.reason.as_deref().unwrap().contains("running"));
    assert!(b.channel_status(live()).unwrap().last_apply.is_none());
    let rm = b
        .remove_changes(RemoveRequest {
            target: live(),
            allow_foreign: true,
        })
        .unwrap();
    assert_eq!(rm.result, ApplyResult::Skipped);
}

#[test]
fn remove_asks_before_deleting_another_tools_file() {
    let b = MockBackend::default();
    let r = b
        .remove_changes(RemoveRequest {
            target: live(),
            allow_foreign: false,
        })
        .unwrap();
    assert_eq!(r.result, ApplyResult::NeedsDecision);
    assert!(r.reason.unwrap().contains("Smart Citizen"));
    assert!(b.channel_status(live()).unwrap().foreign_file);
    let r = b
        .remove_changes(RemoveRequest {
            target: live(),
            allow_foreign: true,
        })
        .unwrap();
    assert_eq!(r.result, ApplyResult::Removed);
    assert!(!b.channel_status(live()).unwrap().foreign_file);
}

#[test]
fn cancel_stops_an_apply_before_it_writes() {
    let b = std::sync::Arc::new(MockBackend::default());
    set_up(&b);
    let b2 = b.clone();
    let e = b
        .apply(
            ApplyRequest {
                target: live(),
                dry_run: false,
            },
            &move |p: JobProgress| {
                if p.step == 2 && p.phase == JobPhase::Running {
                    b2.cancel_job(p.job).unwrap();
                }
            },
        )
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Cancelled);
    assert!(b.channel_status(live()).unwrap().last_apply.is_none());
}

#[test]
fn user_patch_add_export_delete_import_round_trip() {
    let b = MockBackend::default();
    let p = b
        .add_user_patch(NewPatch {
            target: live(),
            title: "Fix sample label".into(),
            description: String::new(),
            key: "ui_SYN_Label_00007".into(),
            op: LocOp::Set,
            find: None,
            value: "Sign 7".into(),
        })
        .unwrap();
    assert_eq!(p.origin, PatchOrigin::User);
    assert_eq!(p.state, PatchState::Applies);
    let r = PatchRef {
        target: live(),
        id: p.id.clone(),
    };
    let x = b.export_user_patch(r.clone()).unwrap();
    assert!(x.text.contains("op = \"set\"") && x.file_name.ends_with(".patch.toml"));
    let left = b.delete_user_patch(r.clone()).unwrap();
    assert!(!left.iter().any(|v| v.id == p.id));
    let back = b.import_user_patch(live()).unwrap();
    assert_eq!(back.id, p.id);
    let builtin = PatchRef {
        target: live(),
        id: "syn-typo-cargo".into(),
    };
    assert_eq!(
        b.delete_user_patch(builtin).unwrap_err().code,
        ErrorCode::Invalid
    );
}

#[test]
fn fix_known_bugs_off_disables_every_built_in_patch() {
    let b = MockBackend::default();
    b.setup(SetupRequest {
        install: "inst-7f3a".into(),
        channels: vec![ChannelConsent {
            channel: "HOTFIX".into(),
            consent: true,
            switch_language: false,
        }],
        preset: Preset::Minimal,
        fix_known_bugs: false,
    })
    .unwrap();
    let p = b.get_profile(hotfix()).unwrap();
    let ids: Vec<String> = b
        .list_patches(hotfix())
        .unwrap()
        .into_iter()
        .filter(|p| p.origin == PatchOrigin::BuiltIn)
        .map(|p| p.id)
        .collect();
    assert_eq!(p.patches_disabled, ids);
}

#[test]
fn schema_lines_name_only_known_categories() {
    let s = MockBackend::default().profile_schema().unwrap();
    for c in &s.categories {
        let prefix = serde_json::to_value(c.id)
            .unwrap()
            .as_str()
            .unwrap()
            .replace('-', "_");
        for l in &c.lines {
            assert!(l.key.starts_with(&format!("{prefix}.")), "{}", l.key);
        }
    }
}

// ---------------------------------------------------------------------------------------
// Drift against the real core (M1/M2). Reads the core sources; runs once they are in the
// workspace (or point STARORG_CORE_SRC at a checkout's crates/starorg-core/src).
// ---------------------------------------------------------------------------------------

fn core_src(file: &str) -> Option<String> {
    let dir = std::env::var_os("STARORG_CORE_SRC")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/starorg-core/src")
        });
    std::fs::read_to_string(dir.join(file)).ok()
}

/// `pub <field>:` names inside `pub struct <name> {`.
fn rust_fields(src: &str, name: &str) -> Vec<String> {
    let at = src
        .find(&format!("pub struct {name} {{"))
        .unwrap_or_else(|| panic!("no struct {name}"));
    let body = &src[at..at + src[at..].find("\n}").unwrap()];
    body.lines()
        .filter_map(|l| l.trim().strip_prefix("pub "))
        .filter_map(|l| l.split_once(':').map(|(f, _)| f.trim().to_string()))
        .filter(|f| !f.contains(' '))
        .collect()
}

fn json_keys(v: &Value) -> Vec<String> {
    let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
    k.sort();
    k
}

#[test]
fn contract_matches_core_names() {
    let (Some(validate), Some(profile), Some(patch), Some(apply)) = (
        core_src("validate.rs"),
        core_src("enhance/profile.rs"),
        core_src("patch.rs"),
        core_src("apply.rs"),
    ) else {
        eprintln!("core M1/M2 sources not in this checkout: drift check skipped");
        return;
    };
    // Override rules: M1 `Rule::name()`.
    let core_rules: Vec<String> = validate
        .lines()
        .filter(|l| l.trim_start().starts_with("Rule::") && l.contains("=> \""))
        .map(|l| l.split('"').nth(1).unwrap().to_string())
        .collect();
    assert_eq!(names(&OverrideRule::ALL), core_rules);
    // Profile fields: M2 `Profile`, `Tags`, `Mission`, `TagStyle` (+ ours: preset, patches).
    let b = MockBackend::default();
    let p = serde_json::to_value(b.preset_profile(Preset::Recommended).unwrap()).unwrap();
    let mut want = rust_fields(&profile, "Profile");
    want.extend(["preset".to_string(), "patches_disabled".to_string()]);
    want.sort();
    assert_eq!(json_keys(&p), want, "Profile");
    let mut tags = rust_fields(&profile, "Tags");
    tags.sort();
    assert_eq!(json_keys(&p["tags"]), tags, "Tags");
    let mut mission = rust_fields(&profile, "Mission");
    mission.sort();
    assert_eq!(json_keys(&p["mission"]), mission, "Mission");
    let mut style = rust_fields(&profile, "TagStyle");
    style.sort();
    assert_eq!(json_keys(&p["tags"]["components"]), style, "TagStyle");
    // Patch states: every M1 EntryState is a PatchState.
    let at = patch.find("pub enum EntryState {").unwrap();
    let body = &patch[at..at + patch[at..].find('}').unwrap()];
    for v in body
        .lines()
        .map(str::trim)
        .filter(|l| l.ends_with(',') && !l.starts_with("//"))
    {
        let kebab: String = v
            .trim_end_matches(',')
            .chars()
            .enumerate()
            .flat_map(|(i, c)| {
                if c.is_uppercase() && i > 0 {
                    vec!['-', c.to_ascii_lowercase()]
                } else {
                    vec![c.to_ascii_lowercase()]
                }
            })
            .collect();
        assert!(
            serde_json::from_value::<PatchState>(json!(kebab)).is_ok(),
            "PatchState lacks {kebab}"
        );
    }
    // Apply outcomes: M1 `Outcome` = our ApplyResult.
    let at = apply.find("pub enum Outcome {").unwrap();
    let body = &apply[at..at + apply[at..].find('}').unwrap()];
    let n = body.lines().filter(|l| l.trim().ends_with(',')).count();
    assert_eq!(n, 10, "M1 Outcome has {n} variants; ApplyResult has 10");
    // Report fields the UI needs are in M1's Report.
    for f in ["recovered", "owned", "reason", "warnings", "duration_ms"] {
        assert!(
            rust_fields(&apply, "Report").iter().any(|x| x == f),
            "Report.{f}"
        );
    }
}

// ---------------------------------------------------------------------------------------
// Updates (docs/updates.md)
// ---------------------------------------------------------------------------------------

#[test]
fn update_check_over_ipc_needs_the_switch_and_takes_a_req() {
    // Fresh mock: switch off. The start-up check is a quiet no-op; "Check now" says why not.
    let quiet = ipc("check_for_updates", json!({ "req": { "force": false } })).unwrap();
    assert_eq!(quiet["enabled"], false);
    assert_eq!(quiet["check"]["state"], "not-checked");
    let e = ipc("check_for_updates", json!({ "req": { "force": true } })).unwrap_err();
    assert_eq!(e["code"], "network-off");
    assert_eq!(
        ipc("install_update", json!({})).unwrap_err()["code"],
        "network-off"
    );
    let st = ipc("update_status", json!({})).unwrap();
    assert_eq!(st["prefs"]["repo"], "voidput/starorg-codex");
    assert_eq!(
        ipc("skip_update", json!({ "version": "0.2.0" })).unwrap()["skipped_version"],
        "0.2.0"
    );
}

#[test]
fn open_link_takes_only_allowlisted_links() {
    assert_eq!(ExternalLink::Support.url(), "https://discord.gg/GCkYhbFYNq");
    assert_eq!(
        ExternalLink::Donate.url(),
        "https://buymeacoffee.com/zedwil"
    );
    // links.ts shows the same URLs (display only; Rust opens them).
    let ts = std::fs::read_to_string(app_src().join("../links.ts")).unwrap();
    assert_eq!(
        ExternalLink::Patreon.url(),
        "https://www.patreon.com/cw/StarOrgTools"
    );
    assert_eq!(ExternalLink::Uex.url(), "https://uexcorp.space");
    for l in [
        ExternalLink::Support,
        ExternalLink::Donate,
        ExternalLink::Patreon,
        ExternalLink::Uex,
    ] {
        assert!(l.url().starts_with("https://"));
        assert!(
            ts.contains(&format!("url: '{}'", l.url())),
            "{l:?} not in links.ts"
        );
    }
    assert_eq!(
        ipc("open_link", json!({ "link": "support" })).unwrap(),
        Value::Null
    );
    assert_eq!(
        ipc("open_link", json!({ "link": "donate" })).unwrap(),
        Value::Null
    );
    assert_eq!(
        ipc("open_link", json!({ "link": "patreon" })).unwrap(),
        Value::Null
    );
    assert_eq!(
        ipc("open_link", json!({ "link": "uex" })).unwrap(),
        Value::Null
    );
    // Never a URL, never an unknown name.
    for bad in [
        json!({ "link": "https://example.com" }),
        json!({ "link": "discord" }),
        json!({ "url": "https://example.com" }),
        json!({}),
    ] {
        assert!(ipc("open_link", bad.clone()).is_err(), "{bad}");
    }
    let b = MockBackend::default();
    b.open_link(ExternalLink::Donate).unwrap();
    let log = b.activity(1).unwrap();
    assert!(
        log[0].message.contains("buymeacoffee.com/zedwil"),
        "{log:?}"
    );
}

#[test]
fn mock_update_flow_finds_skips_and_refuses_to_install() {
    let (_, available, failed) = update_samples();
    let UpdateCheck::Available { release } = &available.check else {
        panic!("{:?}", available.check)
    };
    // Stable only by default: the beta and the draft are not offered.
    assert_eq!(release.version, "0.2.0");
    assert_eq!(available.current_version, crate::update::mock::MOCK_VERSION);
    assert!(
        release
            .notes
            .iter()
            .all(|n| !n.text.contains('<') && !n.text.contains("onerror")),
        "{:?}",
        release.notes
    );
    let UpdateCheck::Failed { problem, message } = &failed.check else {
        panic!("{:?}", failed.check)
    };
    assert_eq!(*problem, UpdateProblem::NotVisible);
    assert!(message.contains("private"));

    let b = MockBackend::default();
    let mut s = b.get_settings().unwrap();
    s.network.check_updates = true;
    b.save_settings(s).unwrap();
    b.set_update_prefs(UpdatePrefs {
        repo: crate::update::DEFAULT_REPO.into(),
        include_prereleases: true,
    })
    .unwrap();
    let pre = b
        .check_for_updates(UpdateCheckRequest { force: false })
        .unwrap();
    let UpdateCheck::Available { release } = &pre.check else {
        panic!()
    };
    assert_eq!(release.version, "0.2.1-beta.1");
    b.open_release_page().unwrap();
    assert_eq!(b.install_update().unwrap_err().code, ErrorCode::Internal);
    let skipped = b.skip_update(release.version.clone()).unwrap();
    assert_eq!(skipped.skipped_version.as_deref(), Some("0.2.1-beta.1"));
}

#[test]
fn blueprint_finder_merges_channels_with_their_own_owners() {
    let b = MockBackend::default();
    let req = |channels: &[&str]| BlueprintFinderRequest {
        install: live().install,
        channels: channels.iter().map(|c| c.to_string()).collect(),
    };
    let all = b.blueprint_finder(req(&[])).unwrap();
    let chans: Vec<(&str, Option<u32>)> = all
        .channels
        .iter()
        .map(|c| (c.channel.as_str(), c.catalog))
        .collect();
    assert_eq!(
        chans,
        vec![("LIVE", Some(39)), ("HOTFIX", None), ("PTU", Some(39))]
    );
    assert_eq!(
        all.characters.len(),
        4,
        "three characters and the unattributed set"
    );
    let row = |f: &BlueprintFinder, n: &str| f.rows.iter().find(|r| r.name == n).cloned().unwrap();
    let rifle = row(&all, "Synthetic Rifle Mk1");
    let mut owners: Vec<(String, String)> = rifle
        .owners
        .iter()
        .map(|o| (o.channel.clone(), o.character.clone()))
        .collect();
    owners.sort();
    assert_eq!(owners.len(), 4, "LIVE x2, HOTFIX, PTU: {owners:?}");
    assert_eq!(rifle.channels, vec!["LIVE", "PTU"]);
    assert_eq!(row(&all, "PTU Prototype Shield").channels, vec!["PTU"]);
    assert_eq!(row(&all, "Sample Crate").channels, vec!["LIVE"]);
    // A log name two blueprints have: an ambiguous name row; both are "maybe owned".
    let visor: Vec<&BlueprintRow> = all
        .rows
        .iter()
        .filter(|r| r.name == "Sample Visor")
        .collect();
    assert_eq!(visor.len(), 3);
    let name_row = visor.iter().find(|r| r.record.is_none()).unwrap();
    assert_eq!(
        (name_row.candidates.len(), name_row.owners[0].ambiguous),
        (2, true)
    );
    assert_eq!(name_row.kind, BlueprintKind::Armor);
    assert!(
        visor
            .iter()
            .filter(|r| r.record.is_some())
            .all(|r| r.owners.is_empty() && r.maybe.len() == 1)
    );
    // The unattributed set shows as its own owner.
    assert_eq!(all.characters.last().unwrap().id, "unattributed");
    assert_eq!(
        row(&all, "Test Scattergun S1").owners[0].character,
        "unattributed"
    );
    // One channel: only its owners.
    let live_only = b.blueprint_finder(req(&["live"])).unwrap();
    assert_eq!(live_only.channels.len(), 1);
    assert!(
        live_only
            .rows
            .iter()
            .flat_map(|r| &r.owners)
            .all(|o| o.channel == "LIVE")
    );
    assert!(
        live_only
            .rows
            .iter()
            .all(|r| r.name != "PTU Prototype Shield")
    );
    // An import lands in the finder for that channel and character only.
    b.import_smart_citizen(ImportRequest {
        target: live(),
        character: "1000000000003".into(),
        dry_run: false,
        reuse_last: false,
        replace: false,
    })
    .unwrap();
    let after = b.blueprint_finder(req(&["LIVE"])).unwrap();
    let legs = row(&after, "Sample Armor Legs");
    assert_eq!(legs.owners.len(), 1);
    assert_eq!(
        (
            legs.owners[0].character.as_str(),
            legs.owners[0].sources[0].as_str()
        ),
        ("1000000000003", "import:smart-citizen")
    );
    assert_eq!(
        b.blueprint_finder(req(&["NOPE"])).unwrap_err().code,
        ErrorCode::NotFound
    );
}
