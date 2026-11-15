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
            data_source: DataSource::PublishedDumps,
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
    let settings = b.save_settings(settings).unwrap();
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
    let mut all = json!({
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
        "starorg_pull": b.starorg_pull(StarOrgPullRequest { channel: "LIVE".into(), character: "1000000000001".into() }).unwrap(),
        "starorg_push": b.starorg_push("LIVE".into()).unwrap(),
        "patch_detail": b.patch_detail(PatchRef { target: live(), id: "syn-moved-key".into() }).unwrap(),
        "export_diagnostics": b.export_diagnostics().unwrap(),
        "save_diagnostics": b.save_diagnostics().unwrap(),
        "hooks_status": b.hooks_status().unwrap(),
        "doctor": b.doctor().unwrap(),
    });
    // Outside json! (its recursion limit).
    let (off, available, failed) = update_samples();
    let m = all.as_object_mut().unwrap();
    m.insert("update_status".into(), json!(off));
    m.insert("check_for_updates".into(), json!(available));
    m.insert("check_for_updates_failed".into(), json!(failed));
    all
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
/// Regenerate with `UPDATE_CONTRACT_SAMPLES=1 cargo test -p starorg-strings-app`.
/// Objects with sorted keys, whatever serde_json's `preserve_order` feature says (starorg-data
/// turns it on for the whole build).
fn sorted(v: Value) -> Value {
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
        "{} is out of date: run UPDATE_CONTRACT_SAMPLES=1 cargo test -p starorg-strings-app, then npm run check",
        path.display()
    );
}

fn names<T: serde::Serialize>(all: &[T]) -> Vec<String> {
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
fn ts_union(ts: &str, name: &str) -> Vec<String> {
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
            "DataSource",
            names(&[DataSource::Local, DataSource::PublishedDumps]),
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
            ]),
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
            "HookKind",
            names(&[HookKind::Prelaunch, HookKind::Postexit]),
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
            src.contains(&format!("$crate::contract::{name},")),
            "{name} missing from contract_handler!"
        );
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
            .contains("starorg-strings setup")
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
    // Pilot_A owns 4, Alt_B owns 2 with one in common: union = 5.
    assert_eq!(st.push_count, 5);
    let st = b
        .set_sync_characters(SyncCharactersRequest {
            channel: "LIVE".into(),
            ticked: vec!["1000000000002".into()],
        })
        .unwrap();
    assert_eq!(st.push_count, 2);
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
    assert_eq!(alt.len(), 4);
    let main = b
        .owned_blueprints(CharacterRequest {
            target: live(),
            character: "1000000000001".into(),
        })
        .unwrap();
    assert_eq!(main.len(), 4, "pull never spreads across alts");
    // Push sends the union of ticked characters: Pilot_A 4 + Alt_B now 4, 1 shared = 7.
    assert_eq!(b.starorg_push("LIVE".into()).unwrap().sent, 7);
    b.set_sync_characters(SyncCharactersRequest {
        channel: "LIVE".into(),
        ticked: vec!["1000000000001".into()],
    })
    .unwrap();
    assert_eq!(b.starorg_push("LIVE".into()).unwrap().sent, 4);
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
        data_source: DataSource::Local,
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
    assert_eq!(st["prefs"]["repo"], "voidput/starorg-strings");
    assert_eq!(
        ipc("skip_update", json!({ "version": "0.2.0" })).unwrap()["skipped_version"],
        "0.2.0"
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
