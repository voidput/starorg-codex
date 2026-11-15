//! Contract tests: the TS mirror matches the Rust shapes, commands work over IPC, and the mock
//! behaves like the spec says the real backend will.

use super::mock::{MOCK_STRING_COUNT, MockBackend};
use super::*;
use serde_json::{Value, json};
use std::path::PathBuf;

fn app_src() -> PathBuf {
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
        data_source: DataSource::Local,
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
    }
}

/// One sample of every response, in a fixed scripted state. Keys are command names.
fn samples() -> Value {
    let b = MockBackend::default();
    let first_overview = b.overview().unwrap();
    let discovered = b.discover_installs(false).unwrap();
    let picked = b.pick_install_folder().unwrap();
    set_up(&b);
    let setup_result = json!({ "install": "inst-7f3a", "set_up": ["LIVE", "HOTFIX"], "skipped": ["PTU"] });
    let mut events = std::sync::Mutex::new(Vec::new());
    let record = |p: JobProgress| events.lock().unwrap().push(p);
    let applied = b.apply(ApplyRequest { target: live(), dry_run: false }, &record).unwrap();
    let event = events.get_mut().unwrap().first().cloned().unwrap();
    b.apply(ApplyRequest { target: live(), dry_run: false }, &noop).unwrap();
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
    let mut settings = b.get_settings().unwrap();
    settings.network.starorg = true;
    let settings = b.save_settings(settings).unwrap();
    let err = b.apply(ApplyRequest { target: ChannelRef { channel: "PTU".into(), ..live() }, dry_run: false }, &noop).unwrap_err();
    json!({
        "contract": CONTRACT_VERSION,
        "job_event": JOB_EVENT,
        "error": err,
        "overview_first_run": first_overview,
        "overview": b.overview().unwrap(),
        "discover_installs": discovered,
        "pick_install_folder": picked,
        "setup": setup_result,
        "channel_status": b.channel_status(live()).unwrap(),
        "apply": applied,
        "job_progress": event,
        "list_backups": b.list_backups(live()).unwrap(),
        "backup_now": b.backup_now(hotfix()).unwrap(),
        "remove_changes": b.remove_changes(hotfix()).unwrap(),
        "build_preview": b.build_preview(PreviewRequest { target: live(), profile: None, preset: Some(Preset::Everything) }).unwrap(),
        "get_profile": b.get_profile(live()).unwrap(),
        "query_strings": b.query_strings(query("cooler")).unwrap(),
        "string_detail": b.string_detail(KeyRequest { target: live(), key: "vehicle_DescSYN_Ship_00000".into() }).unwrap(),
        "set_override": ov,
        "set_override_rejected": rejected,
        "list_patches": b.list_patches(live()).unwrap(),
        "activity": b.activity(5).unwrap(),
        "get_settings": settings,
        "list_characters": b.list_characters(live()).unwrap(),
        "owned_blueprints": b.owned_blueprints(CharacterRequest { target: live(), character: "1000000000001".into() }).unwrap(),
        "import_smart_citizen": b.import_smart_citizen(ImportRequest { target: live(), character: "1000000000002".into(), dry_run: true }).unwrap(),
        "starorg_status": b.starorg_login("LIVE".into()).unwrap(),
        "hooks_status": b.hooks_status().unwrap(),
        "doctor": b.doctor().unwrap(),
    })
}

/// `samples.gen.json` is what the TS types are checked against (`samples.check.ts`).
/// Regenerate with `UPDATE_CONTRACT_SAMPLES=1 cargo test -p starorg-strings-app`.
#[test]
fn samples_match_the_committed_file() {
    let path = app_src().join("samples.gen.json");
    let text = serde_json::to_string_pretty(&samples()).unwrap() + "\n";
    if std::env::var_os("UPDATE_CONTRACT_SAMPLES").is_some() {
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        committed == text,
        "{} is out of date: run UPDATE_CONTRACT_SAMPLES=1 cargo test -p starorg-strings-app, then npm run check",
        path.display()
    );
}

fn names<T: serde::Serialize>(all: &[T]) -> Vec<String> {
    all.iter()
        .map(|v| serde_json::to_value(v).unwrap().as_str().unwrap().to_string())
        .collect()
}

/// `export type X = 'a' | 'b';` from types.ts, as the list of members.
fn ts_union(ts: &str, name: &str) -> Vec<String> {
    let start = format!("export type {name} =");
    let at = ts.find(&start).unwrap_or_else(|| panic!("types.ts has no `{start}`"));
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
        ("ErrorCode", names(&[E::Invalid, E::NotFound, E::NeedsDecision, E::Busy, E::NetworkOff, E::Cancelled, E::Internal])),
        ("Language", names(&[Language::English, Language::Other, Language::Unknown])),
        ("CacheState", names(&[CacheState::Fresh, CacheState::Missing, CacheState::Stale])),
        ("FoundBy", names(&[
            FoundBy::Config, FoundBy::GameDir, FoundBy::Prefix, FoundBy::LugConfig, FoundBy::DefaultPrefix,
            FoundBy::Lutris, FoundBy::LauncherLog, FoundBy::CommonPath, FoundBy::Scan, FoundBy::Manual,
        ])),
        ("JobKind", names(&[JobKind::Discover, JobKind::Extract, JobKind::Apply, JobKind::Remove, JobKind::Restore, JobKind::Import, JobKind::Sync])),
        ("JobPhase", names(&[JobPhase::Running, JobPhase::Done, JobPhase::Failed, JobPhase::Cancelled])),
        ("Preset", names(&[Preset::Recommended, Preset::Minimal, Preset::Everything])),
        ("DataSource", names(&[DataSource::Local, DataSource::PublishedDumps])),
        ("ApplyResult", names(&[
            ApplyResult::Applied, ApplyResult::UpToDate, ApplyResult::Skipped, ApplyResult::Failed, ApplyResult::DryRun,
            ApplyResult::NeedsSetup, ApplyResult::NeedsDecision, ApplyResult::Removed, ApplyResult::Restored, ApplyResult::NothingToDo,
        ])),
        ("BackupReason", names(&[BackupReason::PreExisting, BackupReason::BeforeApply, BackupReason::BeforeRestore, BackupReason::Manual])),
        ("FileOrigin", names(&[FileOrigin::Ours, FileOrigin::Foreign, FileOrigin::None])),
        ("CategoryId", names(&[
            CategoryId::Ships, CategoryId::Components, CategoryId::ShipWeapons, CategoryId::Missiles,
            CategoryId::FpsWeapons, CategoryId::NameTags, CategoryId::Missions, CategoryId::Blueprints,
        ])),
        ("StatsPlacement", names(&[StatsPlacement::BelowProse, StatsPlacement::AboveProse])),
        ("TagStyle", names(&[TagStyle::Short, TagStyle::Medium, TagStyle::Long])),
        ("TagSeparator", names(&[
            TagSeparator::Dash, TagSeparator::None, TagSeparator::Space, TagSeparator::Underscore,
            TagSeparator::Dot, TagSeparator::Slash, TagSeparator::Pipe,
        ])),
        ("TagEnclosing", names(&[TagEnclosing::Square, TagEnclosing::Round, TagEnclosing::Curly, TagEnclosing::Angle, TagEnclosing::None])),
        ("TagPlacement", names(&[TagPlacement::Prefix, TagPlacement::Suffix])),
        ("StringSource", names(&[StringSource::Unmodified, StringSource::Enhanced, StringSource::Patched, StringSource::Edited])),
        ("StringSort", names(&[StringSort::Key, StringSort::KeyDesc, StringSort::Category, StringSort::Source])),
        ("Layer", names(&[Layer::Stock, Layer::Generated, Layer::Patch, Layer::Override])),
        ("PatchState", names(&[
            PatchState::Applies, PatchState::NotNeeded, PatchState::TargetChanged, PatchState::NothingToAdd, PatchState::Missing,
            PatchState::OutOfRange, PatchState::Conflict, PatchState::Disabled, PatchState::Retired,
        ])),
        ("PatchOrigin", names(&[PatchOrigin::BuiltIn, PatchOrigin::User])),
        ("LogLevel", names(&[LogLevel::Info, LogLevel::Warn, LogLevel::Error])),
        ("Theme", names(&[Theme::System, Theme::Dark, Theme::Light])),
        ("Accent", names(&[Accent::Cyan, Accent::Green, Accent::Amber, Accent::Orange, Accent::Rose, Accent::Violet])),
        ("Palette", names(&[Palette::Void, Palette::Black])),
        ("CharacterMode", names(&[CharacterMode::Auto, CharacterMode::Manual, CharacterMode::Union])),
        ("StarOrgEnv", names(&[StarOrgEnv::Production, StarOrgEnv::Dev])),
        ("HookKind", names(&[HookKind::Prelaunch, HookKind::Postexit])),
        ("CheckLevel", names(&[CheckLevel::Ok, CheckLevel::Info, CheckLevel::Warn, CheckLevel::Error])),
    ];
    for (name, want) in cases {
        assert_eq!(ts_union(&ts, name), want, "types.ts union {name}");
    }
    // Tagged unions: the `state` tags.
    let tags = |v: Vec<Value>| -> Vec<String> { v.iter().map(|x| x["state"].as_str().unwrap().to_string()).collect() };
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
}

#[test]
fn ts_client_lists_every_command() {
    let ts = std::fs::read_to_string(app_src().join("client.ts")).unwrap();
    let start = ts.find("export const COMMANDS = [").expect("client.ts has COMMANDS");
    let rest = &ts[start..];
    let body = &rest[rest.find('[').unwrap() + 1..rest.find(']').unwrap()];
    let listed: Vec<&str> = body
        .split(',')
        .map(|s| s.trim().trim_matches('\''))
        .filter(|s| !s.is_empty())
        .collect();
    assert_eq!(listed, COMMAND_NAMES);
    let src = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/contract/mod.rs")).unwrap();
    for name in COMMAND_NAMES {
        assert!(src.contains(&format!("$crate::contract::{name},")), "{name} missing from contract_handler!");
    }
}

// ---------------------------------------------------------------------------------------
// Over IPC
// ---------------------------------------------------------------------------------------

fn ipc(cmd: &str, body: Value) -> Result<Value, Value> {
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
            url: if cfg!(windows) { "http://tauri.localhost" } else { "tauri://localhost" }
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
    assert_eq!(v["installs"][0]["channels"][0]["status"]["state"], "changed-by-another-tool");
}

#[test]
fn query_strings_over_ipc_takes_a_snake_case_req() {
    let v = ipc("query_strings", json!({ "req": serde_json::to_value(query("")).unwrap() })).unwrap();
    assert_eq!(v["all"], MOCK_STRING_COUNT);
    assert_eq!(v["rows"].as_array().unwrap().len(), 3);
}

#[test]
fn errors_cross_ipc_as_app_error() {
    let e = ipc("apply", json!({ "req": { "target": { "install": "inst-7f3a", "channel": "LIVE" }, "dry_run": false } })).unwrap_err();
    assert_eq!(e["code"], "needs-decision");
    assert!(e["detail"].as_str().unwrap().contains("starorg-strings setup"));
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
            channels: vec![ChannelConsent { channel: "PTU".into(), consent: true, switch_language: false }],
            data_source: DataSource::Local,
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
    let r = b.apply(ApplyRequest { target: live(), dry_run: false }, &noop).unwrap();
    assert_eq!(r.result, ApplyResult::Applied);
    let list = b.list_backups(live()).unwrap();
    let f = list.offers.restore_foreign.clone().expect("foreign restore offered");
    assert!(f.build_mismatch);
    assert_eq!(f.tool.as_deref(), Some("Smart Citizen"));
    assert!(list.offers.undo_last_apply.is_none());
    let e = b
        .restore_backup(RestoreRequest { target: live(), backup: f.backup.clone(), allow_build_mismatch: false })
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::NeedsDecision);
    b.restore_backup(RestoreRequest { target: live(), backup: f.backup, allow_build_mismatch: true }).unwrap();
    // A second apply over our own output makes "undo last apply" available.
    b.apply(ApplyRequest { target: live(), dry_run: false }, &noop).unwrap();
    b.apply(ApplyRequest { target: live(), dry_run: false }, &noop).unwrap();
    assert!(b.list_backups(live()).unwrap().offers.undo_last_apply.is_some());
}

#[test]
fn apply_reports_progress_steps_in_order() {
    let b = MockBackend::default();
    set_up(&b);
    let seen = std::sync::Mutex::new(Vec::new());
    b.apply(ApplyRequest { target: live(), dry_run: true }, &|p| seen.lock().unwrap().push((p.step, p.phase))).unwrap();
    let seen = seen.into_inner().unwrap();
    assert_eq!(seen.first(), Some(&(1, JobPhase::Running)));
    assert_eq!(seen.last(), Some(&(5, JobPhase::Done)));
}

#[test]
fn rejected_override_names_the_rule_and_reset_restores() {
    let b = MockBackend::default();
    let key = "item_NameSYN_Cooler_S2_00001";
    let r = b.set_override(OverrideRequest { target: live(), key: key.into(), value: "Cooler (mine)  |  50%".into() }).unwrap();
    assert!(r.accepted);
    let pct = b.set_override(OverrideRequest { target: live(), key: key.into(), value: "50% off".into() }).unwrap();
    assert_eq!(pct.rule.as_deref(), Some("percent"));
    assert_eq!(r.detail.unwrap().row.source, StringSource::Edited);
    let bad = b.set_override(OverrideRequest { target: live(), key: key.into(), value: "a\nb".into() }).unwrap();
    assert!(!bad.accepted);
    assert_eq!(bad.rule.as_deref(), Some("newline"));
    let d = b.reset_override(KeyRequest { target: live(), key: key.into() }).unwrap();
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

#[test]
fn starorg_push_is_the_union_of_ticked_characters() {
    let b = MockBackend::default();
    assert_eq!(b.starorg_login("LIVE".into()).unwrap_err().code, ErrorCode::NetworkOff);
    let mut s = b.get_settings().unwrap();
    s.network.starorg = true;
    b.save_settings(s).unwrap();
    let st = b.starorg_login("LIVE".into()).unwrap();
    assert!(st.connected);
    // Pilot_A owns 4, Alt_B owns 2 with one in common: union = 5.
    assert_eq!(st.push_count, 5);
    let st = b
        .set_sync_characters(SyncCharactersRequest { channel: "LIVE".into(), ticked: vec!["1000000000002".into()] })
        .unwrap();
    assert_eq!(st.push_count, 2);
    let st = b.set_sync_characters(SyncCharactersRequest { channel: "LIVE".into(), ticked: vec![] }).unwrap();
    assert_eq!(st.push_count, 0);
}

#[test]
fn preview_follows_profile_switches() {
    let b = MockBackend::default();
    let mut p = b.preset_profile(Preset::Recommended).unwrap();
    let with = b.build_preview(PreviewRequest { target: live(), profile: Some(p.clone()), preset: None }).unwrap();
    assert!(with[0].result.contains("--- STATS ---"));
    p.categories.iter_mut().find(|c| c.id == CategoryId::Ships).unwrap().enabled = false;
    let without = b.build_preview(PreviewRequest { target: live(), profile: Some(p), preset: None }).unwrap();
    assert_eq!(without[0].result, without[0].original);
}
