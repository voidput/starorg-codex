//! Mock backend: synthetic installs, channels, strings and characters held in memory.
//!
//! Used by the contract tests and, with the non-default feature `mock-backend`, by the Tauri
//! app for UI work without a game install. Nothing here touches the disk or the network. All names are synthetic
//! (`Pilot_A`, `Alt_B`; geids `1000000000001`...), and all text is invented: no CIG prose.

pub use super::schema::{preset_profile, profile_schema};
use super::types::*;
use super::{Backend, Progress};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

/// Rows in the synthetic string table: about the size of LIVE 4.10 (spec 9.4).
pub const MOCK_STRING_COUNT: usize = 91_000;

const INSTALL_ID: &str = "inst-7f3a";
const ALT_INSTALL_ID: &str = "inst-c21e";
const ROOT: &str =
    "/home/pilot/Games/star-citizen/drive_c/Program Files/Roberts Space Industries/StarCitizen";
const ALT_ROOT: &str =
    "/mnt/library/sc-test/drive_c/Program Files/Roberts Space Industries/StarCitizen";
const NOW: &str = "2026-11-14T21:30:05Z";

fn live_build() -> Build {
    Build {
        branch: Some("sc-alpha-4.10.0".into()),
        game_version: Some("4.10.0".into()),
        change: Some(13_165_946),
        version: Some("4.10.200.58746".into()),
    }
}

fn hotfix_build() -> Build {
    Build {
        branch: Some("sc-alpha-4.10.0-hotfix".into()),
        game_version: Some("4.10.0".into()),
        change: Some(13_172_004),
        version: Some("1.0.191.55227".into()),
    }
}

fn ptu_build() -> Build {
    Build {
        branch: Some("sc-alpha-4.11.0-ptu".into()),
        game_version: Some("4.11.0".into()),
        change: Some(13_190_517),
        version: Some("4.11.0.60112".into()),
    }
}

fn old_live_build() -> Build {
    Build {
        branch: Some("sc-alpha-4.9.1".into()),
        game_version: Some("4.9.1".into()),
        change: Some(13_002_118),
        version: Some("4.9.141.51240".into()),
    }
}

#[derive(Debug, Clone)]
struct MockChannel {
    summary: ChannelSummary,
    backups: Vec<BackupView>,
    profile: Profile,
    overrides: BTreeMap<String, String>,
    patches: Vec<PatchView>,
    active: ActiveCharacter,
    owned: HashMap<String, Vec<OwnedBlueprint>>,
    /// Characters whose entries were deleted (`forget_character`).
    forgotten: BTreeSet<String>,
    /// Groups from the loc-reference pass (LIVE); other channels start approximate, as after
    /// a launch hook, until `read_string_groups`.
    exact_groups: bool,
    /// The `[ship_names]` part of the channel's profile (scope, own list).
    names_profile: starorg_core::state::Profile,
}

#[derive(Debug, Clone)]
struct MockState {
    installs: Vec<(InstallSummary, Vec<String>)>,
    channels: BTreeMap<(String, String), MockChannel>,
    settings: Settings,
    log: Vec<LogEntry>,
    starorg: StarOrgStatus,
    hooks: HooksStatus,
    next_log: u64,
    next_job: u64,
    picked: bool,
    /// Jobs asked to stop (`cancel_job`).
    cancel: BTreeSet<String>,
    /// User patch files exported or deleted, so "import" can bring one back.
    patch_files: Vec<(PatchView, String)>,
    /// `fleet.json` and the shared `ship-names.toml`, in memory.
    fleet: starorg_core::fleet::store::FleetFile,
    ship_names: starorg_core::fleet::store::ShipNames,
}

/// Synthetic, stateful backend. `delay` slows long jobs so progress can be seen in the UI;
/// tests use zero.
pub struct MockBackend {
    state: Mutex<MockState>,
    strings: OnceLock<Vec<StringRow>>,
    delay: Duration,
    update: crate::update::Updater,
    /// Developer mode state (mock_dev.rs).
    dev: Mutex<super::mock_dev::DevMock>,
    mcp: crate::mcp::McpHost,
}

impl Default for MockBackend {
    fn default() -> Self {
        Self::new(Duration::ZERO)
    }
}

fn channel(name: &str, build: Build, status: ApplyState) -> ChannelSummary {
    ChannelSummary {
        channel: name.into(),
        build,
        build_error: None,
        set_up: false,
        status,
        language: Language::English,
        language_value: Some("english".into()),
        foreign_tool: None,
        foreign_file: false,
        game_running: false,
        interrupted_apply: false,
        p4k_time: Some("2026-11-12T18:04:31Z".into()),
        cache: CacheState::Missing,
        applied_build: None,
        last_apply: None,
    }
}

/// The profile setup writes: the preset, with every built-in patch listed in
/// `patches_disabled` when "Fix known text bugs" is off.
fn setup_profile(req: &SetupRequest) -> Profile {
    let mut p = preset_profile(req.preset);
    if !req.fix_known_bugs {
        p.patches_disabled = builtin_patches().into_iter().map(|x| x.id).collect();
    }
    p
}

/// Synthetic per-category counts for a profile (plan_setup, apply).
fn counts_for(p: &Profile, overrides: u32) -> ApplyCounts {
    let n = |on: bool, v: u32| if on { v } else { 0 };
    let mut c = ApplyCounts {
        ships: n(p.ships, 231),
        components: n(p.components, 1840),
        ship_weapons: n(p.ship_weapons, 402),
        fps_weapons: n(p.fps_weapons, 318),
        missiles: n(p.missiles, 96),
        missions: n(p.missions, 2140),
        overrides,
        dropped: 0,
        changed: 0,
    };
    c.changed = (c.ships + c.components + c.ship_weapons + c.fps_weapons + c.missiles + c.missions)
        * 2
        + overrides
        + 1200;
    c
}

fn builtin_patches() -> Vec<PatchView> {
    let p = |id: &str, title: &str, d: &str, keys: u32, state: PatchState| PatchView {
        id: id.into(),
        title: title.into(),
        description: d.into(),
        keys,
        origin: PatchOrigin::BuiltIn,
        state,
        enabled: true,
    };
    vec![
        p(
            "syn-typo-cargo",
            "Cargo grid typo",
            "Fixes a misspelt unit in synthetic cargo descriptions.",
            3,
            PatchState::Applies,
        ),
        p(
            "syn-missing-rep",
            "Missing reputation line",
            "Adds the reputation reward a synthetic contract leaves out.",
            12,
            PatchState::Applies,
        ),
        p(
            "syn-old-name",
            "Renamed component",
            "The game text was fixed upstream.",
            1,
            PatchState::Retired,
        ),
        p(
            "syn-moved-key",
            "Moved description key",
            "The target text changed in this build; the patch no longer matches.",
            2,
            PatchState::TargetChanged,
        ),
    ]
}

fn owned(names: &[&str], source: &str) -> Vec<OwnedBlueprint> {
    names
        .iter()
        .map(|n| OwnedBlueprint {
            name: (*n).into(),
            ambiguous: false,
            favorite: false,
            sources: vec![source.into()],
            acquired: Some("2026-11-10T20:11:00Z".into()),
        })
        .collect()
}

/// Synthetic characters: geid, display name, last seen.
const CHARACTERS: &[(&str, &str, &str)] = &[
    ("1000000000001", "Pilot_A", "2026-11-14T19:02:00Z"),
    ("1000000000002", "Alt_B", "2026-11-11T22:40:00Z"),
    ("1000000000003", "Alt_C", "2026-10-30T17:15:00Z"),
];

fn mock_channel(summary: ChannelSummary) -> MockChannel {
    let mut owned_sets = HashMap::new();
    owned_sets.insert(
        CHARACTERS[0].0.to_string(),
        owned(
            &[
                "Synthetic Rifle Mk1",
                "Synthetic Armor Core",
                "Test Pistol Variant",
                "Sample Helmet Grey",
            ],
            "log",
        ),
    );
    owned_sets.insert(
        CHARACTERS[1].0.to_string(),
        owned(&["Synthetic Rifle Mk1", "Sample Undersuit"], "log"),
    );
    owned_sets.insert(CHARACTERS[2].0.to_string(), Vec::new());
    let exact_groups = summary.channel == "LIVE";
    MockChannel {
        summary,
        backups: Vec::new(),
        profile: preset_profile(Preset::Recommended),
        overrides: BTreeMap::new(),
        patches: builtin_patches(),
        active: ActiveCharacter {
            mode: CharacterMode::Auto,
            character: None,
            resolved: Some(CHARACTERS[0].0.into()),
        },
        exact_groups,
        owned: owned_sets,
        forgotten: BTreeSet::new(),
        names_profile: Default::default(),
    }
}

impl MockBackend {
    pub fn new(delay: Duration) -> Self {
        let mut live = channel(
            "LIVE",
            live_build(),
            ApplyState::ChangedByAnotherTool {
                tool: Some("Smart Citizen".into()),
            },
        );
        live.foreign_tool = Some("Smart Citizen".into());
        live.foreign_file = true;
        let hotfix = channel("HOTFIX", hotfix_build(), ApplyState::NotApplied);
        let mut ptu = channel("PTU", ptu_build(), ApplyState::NotApplied);
        ptu.language = Language::Other;
        ptu.language_value = Some("german_(germany)".into());
        let names = vec!["LIVE".to_string(), "HOTFIX".into(), "PTU".into()];
        let install = InstallSummary {
            id: INSTALL_ID.into(),
            root: ROOT.into(),
            prefix: Some("/home/pilot/Games/star-citizen".into()),
            found_by: FoundBy::LugConfig,
            found_by_label: "LUG Helper config".into(),
            saved: false,
            channels: vec![live.clone(), hotfix.clone(), ptu.clone()],
        };
        let alt_live = channel("LIVE", old_live_build(), ApplyState::NotApplied);
        let alt = InstallSummary {
            id: ALT_INSTALL_ID.into(),
            root: ALT_ROOT.into(),
            prefix: Some("/mnt/library/sc-test".into()),
            found_by: FoundBy::Lutris,
            found_by_label: "Lutris game entry".into(),
            saved: false,
            channels: vec![alt_live.clone()],
        };
        let mut channels = BTreeMap::new();
        for c in [live, hotfix, ptu] {
            channels.insert((INSTALL_ID.to_string(), c.channel.clone()), mock_channel(c));
        }
        channels.insert(
            (ALT_INSTALL_ID.to_string(), "LIVE".to_string()),
            mock_channel(alt_live),
        );
        let state = MockState {
            installs: vec![(install, names), (alt, vec!["LIVE".into()])],
            channels,
            settings: Settings {
                appearance: Appearance {
                    theme: Theme::System,
                    accent: Accent::Cyan,
                    palette: Palette::Void,
                    // Fixed, not per OS: the mock feeds samples.gen.json, which must match on every runner.
                    reduce_effects: true,
                },
                network: NetworkSettings {
                    check_updates: false,
                    published_dumps: false,
                    starorg: false,
                },
                default_channel: None,
                safe_gpu: false,
                advanced: AdvancedSettings::default(),
                starorg: StarOrgSettings::default(),
            },
            log: Vec::new(),
            starorg: StarOrgStatus {
                enabled: false,
                connected: false,
                environment: StarOrgEnv::Production,
                host: "starorg.tools".into(),
                user: None,
                guild: None,
                guild_id: None,
                guilds: Vec::new(),
                expires: None,
                expired: false,
                token_store: None,
                last_sync: None,
                sync_characters: Vec::new(),
                push_count: 0,
            },
            hooks: HooksStatus {
                supported: true,
                cli_on_path: true,
                scripts: vec![
                    HookScript {
                        kind: HookKind::Prelaunch,
                        path: Some("/home/pilot/Games/star-citizen/sc-prelaunch.sh".into()),
                        is_link: false,
                        status: HookState::Absent,
                    },
                    HookScript {
                        kind: HookKind::Postexit,
                        path: None,
                        is_link: false,
                        status: HookState::Missing,
                    },
                ],
            },
            next_log: 1,
            next_job: 1,
            picked: false,
            cancel: BTreeSet::new(),
            patch_files: Vec::new(),
            fleet: Default::default(),
            ship_names: Default::default(),
        };
        let me = Self {
            state: Mutex::new(state),
            strings: OnceLock::new(),
            delay,
            update: crate::update::mock::updater(),
            dev: Mutex::new(Default::default()),
            mcp: crate::mcp::McpHost::default(),
        };
        me.log(
            LogLevel::Info,
            None,
            None,
            "Found 2 installs: LUG Helper config, Lutris game entry",
        );
        me
    }

    /// Test and demo switch: the game is running on a channel (M1's running guard).
    pub fn set_game_running(&self, r: &ChannelRef, running: bool) {
        let _ = self.with_channel(r, |c, _| {
            c.summary.game_running = running;
            Ok(())
        });
    }

    // Accessors for mock_dev.rs (Developer mode).
    pub(crate) fn dev_mode_on(&self) -> bool {
        self.lock().settings.advanced.developer_mode
    }

    pub(crate) fn dumps_on(&self) -> bool {
        self.lock().settings.network.published_dumps
    }

    pub(crate) fn dev_state(&self) -> MutexGuard<'_, super::mock_dev::DevMock> {
        self.dev.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn job_delay(&self) -> Duration {
        self.delay
    }

    pub(crate) fn next_job_id(&self) -> String {
        let mut s = self.lock();
        s.next_job += 1;
        format!("job-{}", s.next_job)
    }

    pub(crate) fn has_channel(&self, r: &ChannelRef) -> bool {
        self.lock()
            .channels
            .contains_key(&(r.install.clone(), r.channel.clone()))
    }

    /// Stock text of a synthetic key (case-insensitive), for loc lookups.
    pub(crate) fn stock_text(&self, key: &str) -> Option<String> {
        self.strings()
            .iter()
            .find(|x| x.key.eq_ignore_ascii_case(key))
            .map(|x| x.original.clone())
    }

    /// The mock's channel names of an install, in order.
    pub(crate) fn channel_names(&self, install: &str) -> Vec<String> {
        self.lock()
            .installs
            .iter()
            .find(|i| i.0.id == install)
            .map(|i| i.1.clone())
            .unwrap_or_default()
    }

    fn lock(&self) -> MutexGuard<'_, MockState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn log(&self, level: LogLevel, job: Option<JobKind>, channel: Option<&str>, msg: &str) {
        let mut s = self.lock();
        let id = s.next_log;
        s.next_log += 1;
        s.log.push(LogEntry {
            id,
            time: NOW.into(),
            level,
            job,
            channel: channel.map(str::to_string),
            message: msg.into(),
        });
    }

    fn with_channel<T>(
        &self,
        r: &ChannelRef,
        f: impl FnOnce(&mut MockChannel, &mut MockState) -> CmdResult<T>,
    ) -> CmdResult<T> {
        let mut s = self.lock();
        let key = (r.install.clone(), r.channel.clone());
        let mut ch = s.channels.remove(&key).ok_or_else(|| {
            AppError::new(
                ErrorCode::NotFound,
                format!("No channel {} in install {}", r.channel, r.install),
            )
        })?;
        let out = f(&mut ch, &mut s);
        s.channels.insert(key, ch);
        sync_installs(&mut s);
        out
    }

    fn strings(&self) -> &[StringRow] {
        self.strings.get_or_init(synthetic_strings)
    }

    fn row_with_override(&self, r: &ChannelRef, key: &str) -> CmdResult<StringDetail> {
        let row = self
            .strings()
            .iter()
            .find(|x| x.key.eq_ignore_ascii_case(key))
            .cloned()
            .ok_or_else(|| AppError::new(ErrorCode::NotFound, format!("No key {key} in stock")))?;
        let ov = self.with_channel(r, |c, _| Ok(c.overrides.get(&row.key).cloned()))?;
        Ok(detail(row, ov))
    }
}

/// Keep `installs[].channels` in step with the per-channel state.
fn sync_installs(s: &mut MockState) {
    let chans = s.channels.clone();
    for (inst, names) in &mut s.installs {
        inst.channels = names
            .iter()
            .filter_map(|n| chans.get(&(inst.id.clone(), n.clone())))
            .map(|c| c.summary.clone())
            .collect();
    }
}

const CATEGORIES: &[&str] = &[
    "ships",
    "components",
    "ship-weapons",
    "missiles",
    "fps-weapons",
    "missions",
    "items",
    "ui",
];

/// Deterministic synthetic table. Every 9th row is enhanced, every 97th patched.
fn synthetic_strings() -> Vec<StringRow> {
    (0..MOCK_STRING_COUNT)
        .map(|i| {
            let cat = CATEGORIES[i % CATEGORIES.len()];
            let (key, original) = match cat {
                "ships" => (
                    format!("vehicle_DescSYN_Ship_{i:05}"),
                    format!("Synthetic hull {i}. A test ship description.\\nSecond line."),
                ),
                "components" => (
                    format!("item_NameSYN_Cooler_S{}_{i:05}", i % 4 + 1),
                    format!("Sample Cooler {i}"),
                ),
                "ship-weapons" => (
                    format!("item_NameSYN_Laser_S{}_{i:05}", i % 6 + 1),
                    format!("Test Repeater {i}"),
                ),
                "missiles" => (
                    format!("item_NameSYN_Missile_S{}_{i:05}", i % 3 + 1),
                    format!("Sample Missile {i}"),
                ),
                "fps-weapons" => (
                    format!("item_DescSYN_Rifle_{i:05}"),
                    format!("A synthetic rifle, number {i}."),
                ),
                "missions" => (
                    format!("mission_TitleSYN_Contract_{i:05}"),
                    format!("Synthetic Contract {i}"),
                ),
                "items" => (
                    format!("item_NameSYN_Item_{i:05}"),
                    format!("Sample item {i}"),
                ),
                _ => (format!("ui_SYN_Label_{i:05}"), format!("Label {i}")),
            };
            let (result, source) = if i % 97 == 0 {
                (format!("{original} (fixed)"), StringSource::Patched)
            } else if i % 9 == 0 {
                let r = match cat {
                    "ships" => format!(
                        "{original}\\n\\n--- STATS ---\\nRole: Test Fighter  |  Crew: 1\\nSCM: 226 m/s  |  Max: 1,193 m/s"
                    ),
                    "missions" => format!("{original} <EM4>[BP]</EM4> <EM4>[150 Rep]</EM4>"),
                    "components" | "ship-weapons" | "missiles" => {
                        format!("[S{}-A] {original}", i % 4 + 1)
                    }
                    _ => format!("{original}\\n\\n--- STATS ---\\nDamage: 12.5  |  RPM: 600"),
                };
                (r, StringSource::Enhanced)
            } else {
                (original.clone(), StringSource::Unmodified)
            };
            let (group, manufacturer, size) = mock_group(cat, i);
            StringRow {
                key,
                original,
                result,
                source,
                category: cat.into(),
                group: group.into(),
                group_label: group_label(group).into(),
                manufacturer: manufacturer.map(str::to_string),
                size,
            }
        })
        .collect()
}

/// Synthetic manufacturers (codes and names made up for the mock).
pub const MOCK_MANUFACTURERS: &[(&str, &str)] = &[
    ("SYNA", "Alpha Synthetics"),
    ("SYNB", "Beta Works"),
    ("SYNC", "Gamma Test Yards"),
    ("SYND", "Delta Sample Co"),
];

fn group_label(id: &str) -> &'static str {
    starorg_core::groups::rules::HIERARCHY
        .iter()
        .find(|g| g.id == id)
        .map(|g| g.label)
        .unwrap_or("Other")
}

/// The group, manufacturer and size of synthetic row `i` of category `cat`. The TS mock
/// (`mockGroup` in `mock.ts`) uses the same table; a test pins a few rows of it.
pub fn mock_group(cat: &str, i: usize) -> (&'static str, Option<&'static str>, Option<u32>) {
    let k = i / 8;
    let mfr = Some(MOCK_MANUFACTURERS[k % MOCK_MANUFACTURERS.len()].0);
    let pick = |v: &[&'static str]| v[k % v.len()];
    match cat {
        "ships" => (
            pick(&["ships/ships", "ships/ships", "ships/ground", "ships/paints"]),
            mfr,
            None,
        ),
        "components" => (
            pick(&[
                "components/coolers",
                "components/power-plants",
                "components/shields",
                "components/quantum-drives",
                "components/radars",
                "components/thrusters",
            ]),
            mfr,
            Some((i % 4 + 1) as u32),
        ),
        "ship-weapons" => (
            if k.is_multiple_of(5) {
                "ship-weapons/countermeasures"
            } else {
                "ship-weapons/guns"
            },
            mfr,
            Some((i % 6 + 1) as u32),
        ),
        "missiles" => ("missiles", mfr, Some((i % 3 + 1) as u32)),
        "fps-weapons" => (
            pick(&[
                "fps-weapons/weapons",
                "fps-weapons/weapons",
                "fps-weapons/attachments",
                "fps-weapons/gadgets",
            ]),
            mfr,
            None,
        ),
        "missions" => (
            pick(&["missions/contracts", "missions/text", "missions/givers"]),
            None,
            None,
        ),
        "items" => {
            let g = pick(&[
                "armor/helmets",
                "armor/torso",
                "clothing/torso",
                "consumables/food",
                "items/misc",
            ]);
            (g, if k % 5 < 3 { mfr } else { None }, None)
        }
        _ => (
            pick(&[
                "interface/hud",
                "interface/menus",
                "interface/mobiglas",
                "dialogue/voice",
                "other",
            ]),
            None,
            None,
        ),
    }
}

fn detail(mut row: StringRow, ov: Option<String>) -> StringDetail {
    let mut provenance = vec![Provenance {
        layer: Layer::Stock,
        by: "Data.p4k".into(),
        text: row.original.clone(),
        winner: row.source == StringSource::Unmodified && ov.is_none(),
        note: None,
    }];
    match row.source {
        StringSource::Enhanced => provenance.push(Provenance {
            layer: Layer::Generated,
            by: row.category.clone(),
            text: row.result.clone(),
            winner: ov.is_none(),
            note: Some("Owner by namesake rule; 2 paint variants shared the same text".into()),
        }),
        StringSource::Patched => provenance.push(Provenance {
            layer: Layer::Patch,
            by: "syn-typo-cargo".into(),
            text: row.result.clone(),
            winner: ov.is_none(),
            note: None,
        }),
        _ => {}
    }
    if let Some(v) = &ov {
        provenance.push(Provenance {
            layer: Layer::Override,
            by: "overrides.ini".into(),
            text: v.clone(),
            winner: true,
            note: None,
        });
        row.result = v.clone();
        row.source = StringSource::Edited;
    }
    let also_used_by = if row.group.starts_with("components/") && row.size == Some(1) {
        vec!["HUD".to_string()]
    } else {
        Vec::new()
    };
    StringDetail {
        row,
        provenance,
        override_value: ov,
        also_used_by,
    }
}

/// The subset of M1's validation (spec 7.3, `validate::Rule`) an override can break, for the
/// "rejected" state. Rule names are M1's.
fn check_override(original: &str, value: &str) -> Option<(OverrideRule, String)> {
    if value.chars().any(|c| c.is_control()) {
        return Some((
            OverrideRule::ControlCharacter,
            "A line break must be written as the two characters \\n.".into(),
        ));
    }
    let tags = |s: &str| s.matches("<EM").count() == s.matches("</EM").count();
    if !tags(value) {
        return Some((
            OverrideRule::AddedEmUnbalanced,
            "Every <EMn> tag needs its closing </EMn>.".into(),
        ));
    }
    let vars = |s: &str| s.matches('~').count();
    if vars(original) != vars(value) {
        return Some((
            OverrideRule::VariableTokens,
            "The value must keep the original's ~variable(...) tokens.".into(),
        ));
    }
    let spec = |s: &str| s.matches("%s").count() + s.matches("%d").count();
    if spec(original) != spec(value) {
        return Some((
            OverrideRule::FormatSpecifiers,
            "The value must keep the original's %s / %d placeholders.".into(),
        ));
    }
    if value.contains("% ")
        && value
            .split("% ")
            .skip(1)
            .any(|r| r.starts_with(char::is_alphabetic))
    {
        return Some((
            OverrideRule::PercentRule,
            "A % must be followed by the end, \\n or two spaces.".into(),
        ));
    }
    if value.len() > 4000 {
        return Some((
            OverrideRule::TooLong,
            "The value is longer than the game shows (4,000 characters).".into(),
        ));
    }
    None
}

fn report(ch: &ChannelSummary, result: ApplyResult) -> ApplyReport {
    ApplyReport {
        channel: ch.channel.clone(),
        result,
        reason: None,
        build: ch.build.clone(),
        counts: ApplyCounts {
            changed: 0,
            ships: 0,
            components: 0,
            ship_weapons: 0,
            fps_weapons: 0,
            missiles: 0,
            missions: 0,
            overrides: 0,
            dropped: 0,
        },
        patches: Vec::new(),
        warnings: Vec::new(),
        user_cfg: UserCfgReport {
            changed: false,
            path: None,
            link_target: None,
            shared_with: Vec::new(),
            line: None,
        },
        backup: None,
        backup_dir: None,
        target: None,
        target_is_link: false,
        recovered: None,
        owned: None,
        duration_ms: 0,
    }
}

fn target_path(install: &str, ch: &str) -> String {
    let root = if install == ALT_INSTALL_ID {
        ALT_ROOT
    } else {
        ROOT
    };
    format!("{root}/{ch}/Data/Localization/english/global.ini")
}

fn backup(
    id_seq: usize,
    reason: BackupReason,
    build: Build,
    origin: FileOrigin,
    tool: Option<String>,
) -> BackupView {
    BackupView {
        id: format!("20261114-2130{id_seq:02}-{}", serde_plain(&reason)),
        created: NOW.into(),
        reason,
        pinned: reason == BackupReason::PreExisting,
        build,
        existed: origin != FileOrigin::None,
        size: (origin != FileOrigin::None).then_some(11_204_331),
        origin,
        foreign_tool: tool,
    }
}

fn serde_plain<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn offers(c: &MockChannel) -> RestoreOffers {
    let s = &c.summary;
    let loose = !matches!(s.status, ApplyState::NotApplied);
    let undo = c
        .backups
        .iter()
        .rev()
        .find(|b| {
            b.reason == BackupReason::BeforeApply
                && b.origin == FileOrigin::Ours
                && b.build.change == s.build.change
        })
        .map(|b| b.id.clone());
    let foreign = c
        .backups
        .iter()
        .find(|b| b.pinned && b.origin == FileOrigin::Foreign)
        .map(|b| ForeignRestore {
            backup: b.id.clone(),
            tool: b.foreign_tool.clone(),
            made_for: b.build.clone(),
            current: s.build.clone(),
            build_mismatch: b.build.change != s.build.change,
        });
    RestoreOffers {
        remove: loose,
        remove_foreign: matches!(s.status, ApplyState::ChangedByAnotherTool { .. }),
        undo_last_apply: undo,
        restore_foreign: foreign,
    }
}

fn starorg_view(s: &mut MockState, channel: &str) {
    let key = (INSTALL_ID.to_string(), channel.to_string());
    let owned = s
        .channels
        .get(&key)
        .map(|c| c.owned.clone())
        .unwrap_or_default();
    let ticked: BTreeSet<String> = if s.starorg.sync_characters.is_empty() {
        CHARACTERS.iter().map(|c| c.0.to_string()).collect()
    } else {
        s.starorg
            .sync_characters
            .iter()
            .filter(|c| c.ticked)
            .map(|c| c.id.clone())
            .collect()
    };
    s.starorg.sync_characters = CHARACTERS
        .iter()
        .map(|(id, name, _)| SyncCharacter {
            id: (*id).into(),
            name: (*name).into(),
            ticked: ticked.contains(*id),
            owned: owned.get(*id).map_or(0, |v| v.len() as u32),
        })
        .collect();
    // Q22: push sends the union of the ticked characters' owned sets.
    let union: BTreeSet<&str> = ticked
        .iter()
        .filter_map(|id| owned.get(id))
        .flatten()
        .map(|b| b.name.as_str())
        .collect();
    s.starorg.push_count = union.len() as u32;
    s.starorg.enabled = s.settings.network.starorg;
    s.starorg.environment = s.settings.starorg.environment;
    s.starorg.host = match s.starorg.environment {
        StarOrgEnv::Production => "starorg.tools",
        StarOrgEnv::Dev => "dev.starorg.tools",
    }
    .into();
}

fn mock_sign_out(s: &mut MockState) {
    s.starorg.connected = false;
    s.starorg.user = None;
    s.starorg.guild = None;
    s.starorg.guild_id = None;
    s.starorg.guilds.clear();
    s.starorg.expires = None;
    s.starorg.token_store = None;
}

/// Synthetic orgs of the signed-in mock account.
fn mock_guilds() -> Vec<StarOrgGuild> {
    vec![
        StarOrgGuild {
            id: "100000000000000001".into(),
            name: "Synthetic Org".into(),
            blueprints: true,
        },
        StarOrgGuild {
            id: "100000000000000002".into(),
            name: "Other Test Org".into(),
            blueprints: false,
        },
    ]
}

fn sync_off(what: &str) -> AppError {
    AppError::new(
        ErrorCode::Invalid,
        format!("{what} is off under Settings -> StarOrg Tools -> What to sync."),
    )
}

/// Synthetic hulls for the Ships screen: invented makers and names, no CIG text.
pub(crate) const MOCK_HULLS: &[(&str, &str, Option<&str>, &str, bool)] = &[
    (
        "vehicle_NameKSTL_Kite",
        "Kestrel Kite",
        Some("Kite"),
        "kstl",
        false,
    ),
    (
        "vehicle_NameKSTL_Talon",
        "Kestrel Talon Mk I",
        Some("Talon Mk I"),
        "kstl",
        false,
    ),
    (
        "vehicle_NameKSTL_Talon_Mk2",
        "Kestrel Talon Mk II",
        Some("Talon Mk II"),
        "kstl",
        false,
    ),
    (
        "vehicle_NameKSTL_Harrier",
        "Kestrel Harrier",
        Some("Harrier"),
        "kstl",
        false,
    ),
    (
        "vehicle_NameORBW_Lantern",
        "Orbit Works Lantern",
        Some("Lantern"),
        "orbw",
        false,
    ),
    (
        "vehicle_NameORBW_Lantern_Pro",
        "Orbit Works Lantern Pro",
        Some("Lantern Pro"),
        "orbw",
        false,
    ),
    (
        "vehicle_NameORBW_Beacon",
        "Orbit Works Beacon",
        Some("Beacon"),
        "orbw",
        false,
    ),
    (
        "vehicle_NameRDGE_Hauler",
        "Ridge Freight Hauler",
        Some("Hauler"),
        "rdge",
        false,
    ),
    (
        "vehicle_NameRDGE_Hauler_XL",
        "Ridge Freight Hauler XL",
        Some("Hauler XL"),
        "rdge",
        false,
    ),
    (
        "vehicle_NameRDGE_Barge",
        "Ridge Freight Barge",
        None,
        "rdge",
        false,
    ),
    (
        "vehicle_NameVLMR_Moth",
        "Volmar Moth",
        Some("Moth"),
        "vlmr",
        false,
    ),
    (
        "vehicle_NameVLMR_Hornbill",
        "Volmar Hornbill",
        Some("Hornbill"),
        "vlmr",
        false,
    ),
    (
        "vehicle_NameVLMR_Pike",
        "Volmar Pike",
        Some("Pike"),
        "vlmr",
        false,
    ),
    (
        "vehicle_NameSOLN_Drifter",
        "Solen Drifter",
        Some("Drifter"),
        "soln",
        false,
    ),
    (
        "vehicle_NameSOLN_Prospect",
        "Solen Prospect",
        Some("Prospect"),
        "soln",
        false,
    ),
    (
        "vehicle_NameSOLN_Rover",
        "Solen Rover",
        Some("Rover"),
        "soln",
        true,
    ),
    (
        "vehicle_NameTRNE_Wren",
        "Tarne Wren",
        Some("Wren"),
        "trne",
        false,
    ),
    (
        "vehicle_NameTRNE_Heron",
        "Tarne Heron",
        Some("Heron"),
        "trne",
        false,
    ),
    (
        "vehicle_NameAXIS_Bastion",
        "Axis Bastion",
        Some("Bastion"),
        "axis",
        false,
    ),
    (
        "vehicle_NameAXIS_Vigil",
        "Axis Vigil",
        Some("Vigil"),
        "axis",
        false,
    ),
];

/// The "file" the mock import reads: HangarXPLOR JSON with made-up pledges and names.
pub(crate) const MOCK_FLEET_FILE: &str = r#"[
 {"ship_code":"KSTL_Kite","ship_name":"Needle","manufacturer_code":"KSTL","manufacturer_name":"Kestrel","name":"Kite","lti":true,"warbond":false,"entity_type":"ship","pledge_id":"90000001","pledge_name":"Synthetic pledge","pledge_date":"January 01, 2950","pledge_cost":"$10.00 USD"},
 {"ship_code":"KSTL_Kite","ship_name":"Kite","manufacturer_code":"KSTL","manufacturer_name":"Kestrel","name":"Kite","entity_type":"ship"},
 {"ship_name":"Talon","manufacturer_code":"KSTL","manufacturer_name":"Kestrel","name":"Talon","entity_type":"ship"},
 {"ship_code":"ORBW_Lantern","ship_name":"Rusty","manufacturer_code":"ORBW","manufacturer_name":"Orbit Works","name":"Lantern","entity_type":"ship"},
 {"ship_code":"RDGE_Hauler","ship_name":"Hauler","manufacturer_code":"RDGE","manufacturer_name":"Ridge Freight","name":"Hauler","entity_type":"ship"},
 {"ship_code":"RDGE_Hauler","ship_name":"Hauler","manufacturer_code":"RDGE","manufacturer_name":"Ridge Freight","name":"Hauler","entity_type":"ship"},
 {"ship_code":"VLMR_Pike","ship_name":"Pike","manufacturer_code":"VLMR","manufacturer_name":"Volmar","name":"Pike","entity_type":"ship"},
 {"ship_code":"TRNE_Starlance","ship_name":"Starlance","manufacturer_code":"TRNE","manufacturer_name":"Tarne","name":"Starlance","entity_type":"ship"},
 {"name":"Synthetic paint pack","entity_type":"skin"}
]"#;

fn mock_catalog() -> starorg_core::fleet::Catalog {
    use starorg_core::fleet::Hull;
    starorg_core::fleet::Catalog::new(
        MOCK_HULLS
            .iter()
            .map(|&(id, name, short, maker, ground)| Hull {
                hull: id.into(),
                name: name.into(),
                short: short.map(str::to_string),
                short_key: short.map(|_| format!("{id}_short")),
                maker: Some(maker.into()),
                flyable: true,
                ground,
                npc_shared: if ground { 0 } else { 2 },
                namesake: true,
            })
            .collect(),
    )
}

fn mock_fleet_view(c: &MockChannel, s: &MockState) -> FleetView {
    use starorg_core::fleet::store::{Effective, Scope, channel_settings, scope_of};
    let scope = scope_of(&c.names_profile);
    let raw = match scope {
        Scope::Channel => channel_settings(&c.names_profile).unwrap_or_default(),
        _ => s.ship_names.clone(),
    };
    let (settings, warnings, invalid) = raw.validated();
    let eff = Effective {
        scope,
        settings,
        warnings,
        invalid,
    };
    let cat = mock_catalog();
    let overrides: Vec<starorg_core::state::Override> = c
        .overrides
        .iter()
        .enumerate()
        .map(|(i, (k, v))| starorg_core::state::Override {
            line: i + 1,
            key: k.clone(),
            value: v.clone(),
        })
        .collect();
    super::fleet::view(super::fleet::ViewParts {
        channel: &c.summary.channel,
        catalog: Some(&cat),
        fleet: &s.fleet,
        effective: &eff,
        shared: &s.ship_names,
        overrides: &overrides,
        fleet_file: "~/.local/share/starorg-strings/fleet.json".into(),
        settings_file: if scope == Scope::Channel {
            format!(
                "~/.local/share/starorg-strings/channels/{INSTALL_ID}/{}/profile.toml",
                c.summary.channel
            )
        } else {
            "~/.local/share/starorg-strings/ship-names.toml".into()
        },
    })
}

impl Backend for MockBackend {
    fn overview(&self) -> CmdResult<Overview> {
        let s = self.lock();
        let installs: Vec<InstallSummary> = s.installs.iter().map(|i| i.0.clone()).collect();
        let first_run = !installs.iter().any(|i| i.channels.iter().any(|c| c.set_up));
        let active = installs.iter().find(|i| i.saved).map(|i| i.id.clone());
        Ok(Overview {
            contract: CONTRACT_VERSION,
            // Fixed, not the crate version: samples.gen.json must not change on every release bump.
            app_version: "0.1.0".into(),
            first_run,
            active_install: active.clone(),
            default_channel: s.settings.default_channel.clone().or_else(|| {
                installs
                    .iter()
                    .find(|i| Some(&i.id) == active.as_ref())
                    .and_then(|i| i.channels.iter().find(|c| c.set_up))
                    .map(|c| c.channel.clone())
            }),
            installs,
            job: None,
        })
    }

    fn discover_installs(&self, _deep: bool) -> CmdResult<Vec<InstallSummary>> {
        let s = self.lock();
        Ok(s.installs
            .iter()
            .map(|i| i.0.clone())
            .filter(|i| s.picked || i.found_by != FoundBy::Manual)
            .collect())
    }

    fn pick_install_folder(&self) -> CmdResult<Option<InstallSummary>> {
        let mut s = self.lock();
        s.picked = true;
        Ok(s.installs.get(1).map(|i| {
            let mut i = i.0.clone();
            i.found_by = FoundBy::Manual;
            i.found_by_label = "Chosen folder".into();
            i
        }))
    }

    fn plan_setup(&self, req: SetupRequest) -> CmdResult<SetupPlan> {
        let mut channels = Vec::new();
        let mut warnings = Vec::new();
        for c in req.channels.iter().filter(|c| c.consent) {
            let r = ChannelRef {
                install: req.install.clone(),
                channel: c.channel.clone(),
            };
            let summary = self.channel_status(r)?;
            if summary.language == Language::Other && !c.switch_language {
                warnings.push(Warning {
                    kind: "language".into(),
                    detail: format!(
                        "{}: the game language is not English; it will be skipped",
                        c.channel
                    ),
                });
                continue;
            }
            let p = setup_profile(&req);
            let mut counts = counts_for(&p, 0);
            // Channels differ a little (another build has other items).
            counts.changed -= 37 * c.channel.len() as u32;
            let root = if req.install == ALT_INSTALL_ID {
                ALT_ROOT
            } else {
                ROOT
            };
            channels.push(PlannedChannel {
                channel: c.channel.clone(),
                target: target_path(&req.install, &c.channel),
                target_is_link: false,
                link_target: None,
                backup_dir: format!(
                    "~/.local/share/starorg-strings/channels/{}/{}/backups",
                    req.install, c.channel
                ),
                user_cfg_line: (summary.language == Language::Other)
                    .then(|| "g_language = english".into()),
                user_cfg_path: Some(format!("{root}/{}/USER.cfg", c.channel)),
                replaces_tool: summary.foreign_tool.clone(),
                counts,
            });
        }
        if req.data_source == DataSource::PublishedDumps {
            warnings.push(Warning {
                kind: "network".into(),
                detail: "Published game data is downloaded only when a dump exists for the exact build; otherwise it is read locally.".into(),
            });
        }
        Ok(SetupPlan {
            install: req.install,
            channels,
            warnings,
        })
    }

    fn setup(&self, req: SetupRequest) -> CmdResult<SetupResult> {
        let mut set_up = Vec::new();
        let mut skipped = Vec::new();
        for c in &req.channels {
            let r = ChannelRef {
                install: req.install.clone(),
                channel: c.channel.clone(),
            };
            let res = self.with_channel(&r, |ch, _| {
                if !c.consent {
                    return Ok(false);
                }
                if ch.summary.language == Language::Other && !c.switch_language {
                    return Err(AppError {
                        code: ErrorCode::NeedsDecision,
                        message: format!(
                            "{}: the game language is not English. Choose whether to switch it.",
                            c.channel
                        ),
                        detail: ch.summary.language_value.clone(),
                    });
                }
                // Setup again keeps a set-up channel's own settings.
                if !ch.summary.set_up {
                    ch.profile = setup_profile(&req);
                }
                ch.summary.set_up = true;
                Ok(true)
            })?;
            if res {
                set_up.push(c.channel.clone());
            } else {
                skipped.push(c.channel.clone());
            }
        }
        {
            let mut s = self.lock();
            for (inst, _) in &mut s.installs {
                inst.saved = inst.id == req.install;
            }
            if s.settings.default_channel.is_none() {
                s.settings.default_channel = set_up.first().cloned();
            }
            sync_installs(&mut s);
        }
        self.log(
            LogLevel::Info,
            None,
            None,
            &format!(
                "Set up {} (skipped: {})",
                set_up.join(", "),
                skipped.join(", ")
            ),
        );
        Ok(SetupResult {
            install: req.install,
            set_up,
            skipped,
        })
    }

    fn channel_status(&self, req: ChannelRef) -> CmdResult<ChannelSummary> {
        self.with_channel(&req, |c, _| Ok(c.summary.clone()))
    }

    fn apply(&self, req: ApplyRequest, progress: Progress<'_>) -> CmdResult<ApplyReport> {
        let job = {
            let mut s = self.lock();
            s.next_job += 1;
            format!("job-{}", s.next_job)
        };
        let ch_name = req.target.channel.clone();
        let steps = [
            "Checking the game is not running",
            "Reading game data",
            "Building text",
            "Validating",
            "Backing up and writing",
        ];
        let n = steps.len() as u32;
        let set_up = self.with_channel(&req.target, |c, _| Ok(c.summary.set_up))?;
        if !set_up {
            return Err(AppError {
                code: ErrorCode::NeedsDecision,
                message: format!("{ch_name} is not set up. Run setup for this channel first."),
                detail: Some(format!(
                    "starorg-strings setup --channel {ch_name} --preset recommended --yes"
                )),
            });
        }
        let running = self.with_channel(&req.target, |c, _| Ok(c.summary.clone()))?;
        if running.game_running {
            let mut r = report(&running, ApplyResult::Skipped);
            r.reason = Some(format!(
                "Star Citizen is running on {ch_name}. Close the game, then apply."
            ));
            self.log(
                LogLevel::Warn,
                Some(JobKind::Apply),
                Some(&ch_name),
                "Apply skipped: the game is running",
            );
            return Ok(r);
        }
        for (i, label) in steps.iter().enumerate() {
            if self.lock().cancel.remove(&job) {
                progress(JobProgress {
                    job: job.clone(),
                    kind: JobKind::Apply,
                    channel: Some(ch_name.clone()),
                    phase: JobPhase::Cancelled,
                    step: i as u32,
                    steps: n,
                    label: "Stopped".into(),
                    cancellable: false,
                });
                self.log(
                    LogLevel::Info,
                    Some(JobKind::Apply),
                    Some(&ch_name),
                    "Apply stopped before writing",
                );
                return Err(AppError::new(
                    ErrorCode::Cancelled,
                    "Stopped. Nothing was written to the game.",
                ));
            }
            let last = req.dry_run && i + 1 == steps.len();
            progress(JobProgress {
                job: job.clone(),
                kind: JobKind::Apply,
                channel: Some(ch_name.clone()),
                phase: JobPhase::Running,
                step: i as u32 + 1,
                steps: n,
                label: if last {
                    "Dry run: nothing written".into()
                } else {
                    (*label).into()
                },
                cancellable: i < 3,
            });
            if !self.delay.is_zero() {
                std::thread::sleep(self.delay);
            }
        }
        let rep = self.with_channel(&req.target, |c, _| {
            let mut r = report(
                &c.summary,
                if req.dry_run {
                    ApplyResult::DryRun
                } else {
                    ApplyResult::Applied
                },
            );
            let p = &c.profile;
            r.counts = counts_for(p, c.overrides.len() as u32);
            if p.missions && p.mission.owned_marker {
                let who = c.active.resolved.clone().unwrap_or_default();
                r.owned = Some(OwnedSelection {
                    mode: OwnedMode::Auto,
                    count: c.owned.get(&who).map_or(0, |v| v.len() as u32),
                    characters: vec![who],
                    ambiguous: 0,
                });
            }
            if c.summary.interrupted_apply {
                r.recovered = Some(
                    "An earlier apply was interrupted; the file from before it was put back first."
                        .into(),
                );
                if !req.dry_run {
                    c.summary.interrupted_apply = false;
                }
            }
            r.patches = c
                .patches
                .iter()
                .map(|x| PatchOutcome {
                    id: x.id.clone(),
                    state: if x.enabled && !p.patches_disabled.contains(&x.id) {
                        x.state
                    } else {
                        PatchState::Disabled
                    },
                })
                .collect();
            if c.patches
                .iter()
                .any(|x| x.enabled && x.state == PatchState::TargetChanged)
            {
                r.warnings.push(Warning {
                    kind: "patch-target-changed".into(),
                    detail: "syn-moved-key: the target text changed in this build".into(),
                });
            }
            r.target = Some(target_path(&req.target.install, &c.summary.channel));
            r.user_cfg = UserCfgReport {
                changed: !req.dry_run && c.summary.language != Language::English,
                path: Some(format!("{}/{}/USER.cfg", ROOT, c.summary.channel)),
                link_target: Some("/home/pilot/Games/star-citizen/shared/USER.cfg".into()),
                shared_with: Vec::new(),
                line: (c.summary.language != Language::English)
                    .then(|| "g_language = english".into()),
            };
            r.backup_dir = Some(format!(
                "~/.local/share/starorg-strings/channels/{}/{}/backups",
                req.target.install, c.summary.channel
            ));
            r.duration_ms = 1830;
            if req.dry_run {
                return Ok(r);
            }
            let seq = c.backups.len() + 1;
            if c.backups.is_empty() {
                let (origin, tool) = if c.summary.foreign_file {
                    (FileOrigin::Foreign, c.summary.foreign_tool.clone())
                } else {
                    (FileOrigin::None, None)
                };
                let pre_build = if c.summary.foreign_file {
                    old_live_build()
                } else {
                    c.summary.build.clone()
                };
                c.backups.push(backup(
                    seq,
                    BackupReason::PreExisting,
                    pre_build,
                    origin,
                    tool,
                ));
            } else if matches!(
                c.summary.status,
                ApplyState::Applied | ApplyState::Stale { .. }
            ) {
                c.backups.push(backup(
                    seq,
                    BackupReason::BeforeApply,
                    c.summary.build.clone(),
                    FileOrigin::Ours,
                    None,
                ));
            }
            r.backup = c.backups.last().map(|b| b.id.clone());
            c.summary.status = ApplyState::Applied;
            c.summary.foreign_file = false;
            c.summary.cache = CacheState::Fresh;
            c.summary.applied_build = Some(c.summary.build.clone());
            c.summary.last_apply = Some(LastApply {
                time: NOW.into(),
                result: r.result,
                changed: r.counts.changed,
                warnings: r.warnings.len() as u32,
                dropped: r.counts.dropped,
                duration_ms: r.duration_ms,
            });
            Ok(r)
        })?;
        progress(JobProgress {
            job,
            kind: JobKind::Apply,
            channel: Some(ch_name.clone()),
            phase: JobPhase::Done,
            step: n,
            steps: n,
            label: if req.dry_run {
                "Dry run finished".into()
            } else {
                format!("Applied to {ch_name}")
            },
            cancellable: false,
        });
        self.log(
            if rep.warnings.is_empty() {
                LogLevel::Info
            } else {
                LogLevel::Warn
            },
            Some(JobKind::Apply),
            Some(&ch_name),
            &format!(
                "{}: {} strings changed, {} warning(s)",
                if req.dry_run { "Dry run" } else { "Applied" },
                rep.counts.changed,
                rep.warnings.len()
            ),
        );
        Ok(rep)
    }

    fn remove_changes(&self, req: RemoveRequest) -> CmdResult<ApplyReport> {
        let RemoveRequest {
            target: req,
            allow_foreign,
        } = req;
        let r = self.with_channel(&req, |c, _| {
            if matches!(c.summary.status, ApplyState::NotApplied) {
                return Ok(report(&c.summary, ApplyResult::NothingToDo));
            }
            if c.summary.game_running {
                let mut r = report(&c.summary, ApplyResult::Skipped);
                r.reason = Some(format!(
                    "Star Citizen is running on {}. Close the game, then remove.",
                    c.summary.channel
                ));
                return Ok(r);
            }
            if c.summary.foreign_file && !allow_foreign {
                let mut r = report(&c.summary, ApplyResult::NeedsDecision);
                r.reason = Some(format!(
                    "The file in place belongs to {}. Confirm to remove it (a backup is kept).",
                    c.summary.foreign_tool.as_deref().unwrap_or("another tool")
                ));
                return Ok(r);
            }
            if c.summary.foreign_file {
                let seq = c.backups.len() + 1;
                c.backups.push(backup(
                    seq,
                    BackupReason::BeforeRestore,
                    c.summary.build.clone(),
                    FileOrigin::Foreign,
                    c.summary.foreign_tool.clone(),
                ));
            }
            c.summary.status = ApplyState::NotApplied;
            c.summary.foreign_file = false;
            c.summary.applied_build = None;
            let mut r = report(&c.summary, ApplyResult::Removed);
            r.target = Some(target_path(&req.install, &c.summary.channel));
            Ok(r)
        })?;
        if r.result != ApplyResult::Removed {
            return Ok(r);
        }
        self.log(
            LogLevel::Info,
            Some(JobKind::Remove),
            Some(&req.channel),
            "Removed StarOrg changes (back to stock)",
        );
        Ok(r)
    }

    fn undo_last_apply(&self, req: ChannelRef) -> CmdResult<ApplyReport> {
        self.with_channel(&req, |c, _| {
            let o = offers(c);
            let Some(id) = o.undo_last_apply else {
                return Err(AppError::new(
                    ErrorCode::NeedsDecision,
                    "No backup of an earlier apply for this build. Re-apply instead.",
                ));
            };
            c.backups.retain(|b| b.id != id);
            c.summary.status = ApplyState::Stale {
                reasons: vec!["profile".into()],
            };
            let mut r = report(&c.summary, ApplyResult::Restored);
            r.backup = Some(id);
            Ok(r)
        })
    }

    fn restore_backup(&self, req: RestoreRequest) -> CmdResult<ApplyReport> {
        self.with_channel(&req.target, |c, _| {
            let b = c
                .backups
                .iter()
                .find(|b| b.id == req.backup)
                .cloned()
                .ok_or_else(|| {
                    AppError::new(ErrorCode::NotFound, format!("No backup {}", req.backup))
                })?;
            if b.build.change != c.summary.build.change && !req.allow_build_mismatch {
                return Err(AppError {
                    code: ErrorCode::NeedsDecision,
                    message: "This file was made for another game build.".into(),
                    detail: Some(format!(
                        "Made for {}, the game is now {}. New text would show as raw @keys.",
                        b.build.version.as_deref().unwrap_or("unknown"),
                        c.summary.build.version.as_deref().unwrap_or("unknown")
                    )),
                });
            }
            c.summary.status = ApplyState::ChangedByAnotherTool {
                tool: b.foreign_tool.clone(),
            };
            c.summary.foreign_file = b.origin == FileOrigin::Foreign;
            c.summary.foreign_tool = b.foreign_tool.clone();
            let mut r = report(&c.summary, ApplyResult::Restored);
            r.backup = Some(b.id);
            Ok(r)
        })
    }

    fn list_backups(&self, req: ChannelRef) -> CmdResult<BackupList> {
        self.with_channel(&req, |c, _| {
            Ok(BackupList {
                channel: c.summary.channel.clone(),
                folder: format!(
                    "~/.local/share/starorg-strings/channels/{}/{}/backups",
                    req.install, c.summary.channel
                ),
                backups: c.backups.iter().rev().cloned().collect(),
                offers: offers(c),
                keep_newest: 10,
                keep_builds: 3,
            })
        })
    }

    fn backup_now(&self, req: ChannelRef) -> CmdResult<BackupView> {
        self.with_channel(&req, |c, _| {
            let origin = match c.summary.status {
                ApplyState::NotApplied => FileOrigin::None,
                ApplyState::ChangedByAnotherTool { .. } => FileOrigin::Foreign,
                _ => FileOrigin::Ours,
            };
            let b = backup(
                c.backups.len() + 1,
                BackupReason::Manual,
                c.summary.build.clone(),
                origin,
                c.summary.foreign_tool.clone(),
            );
            c.backups.push(b.clone());
            Ok(b)
        })
    }

    fn delete_backup(&self, req: BackupRef) -> CmdResult<BackupList> {
        self.with_channel(&req.target, |c, _| {
            if c.backups.iter().any(|b| b.id == req.backup && b.pinned) {
                return Err(AppError::new(
                    ErrorCode::Invalid,
                    "The pre-existing backup is pinned and cannot be deleted.",
                ));
            }
            c.backups.retain(|b| b.id != req.backup);
            Ok(())
        })?;
        self.list_backups(req.target)
    }

    fn cancel_job(&self, job: String) -> CmdResult<()> {
        self.lock().cancel.insert(job);
        Ok(())
    }

    fn build_preview(&self, req: PreviewRequest) -> CmdResult<Vec<PreviewItem>> {
        let profile = match (req.profile, req.preset) {
            (Some(p), _) => p,
            (None, Some(p)) => preset_profile(p),
            (None, None) => self.get_profile(req.target)?,
        };
        Ok(preview(&profile))
    }

    fn profile_schema(&self) -> CmdResult<ProfileSchema> {
        Ok(profile_schema())
    }

    fn get_profile(&self, req: ChannelRef) -> CmdResult<Profile> {
        self.with_channel(&req, |c, _| Ok(c.profile.clone()))
    }

    fn preset_profile(&self, preset: Preset) -> CmdResult<Profile> {
        Ok(preset_profile(preset))
    }

    fn save_profile(&self, req: SaveProfileRequest) -> CmdResult<Profile> {
        self.with_channel(&req.target, |c, _| {
            c.profile = req.profile.clone();
            if matches!(c.summary.status, ApplyState::Applied) {
                c.summary.status = ApplyState::Stale {
                    reasons: vec!["profile".into()],
                };
            }
            Ok(c.profile.clone())
        })
    }

    fn copy_profile(&self, req: CopyProfileRequest) -> CmdResult<Profile> {
        let p = self.get_profile(req.from)?;
        self.save_profile(SaveProfileRequest {
            target: req.to,
            profile: p,
        })
    }

    fn query_strings(&self, req: StringsQuery) -> CmdResult<StringsPage> {
        let (overrides, exact) = self.with_channel(&req.target, |c, _| {
            Ok((c.overrides.clone(), c.exact_groups))
        })?;
        let all = self.strings();
        let rows = all.iter().map(|r| match overrides.get(&r.key) {
            Some(v) => std::borrow::Cow::Owned(StringRow {
                result: v.clone(),
                source: StringSource::Edited,
                ..r.clone()
            }),
            None => std::borrow::Cow::Borrowed(r),
        });
        let meta = super::strings_query::GroupMeta {
            state: if exact {
                GroupState::Exact
            } else {
                GroupState::Approximate
            },
            manufacturers: MOCK_MANUFACTURERS
                .iter()
                .map(|(c, l)| (c.to_string(), l.to_string()))
                .collect(),
        };
        Ok(super::strings_query::run(
            rows,
            all.len() as u32,
            &req,
            &meta,
        ))
    }

    fn read_string_groups(&self, req: ChannelRef) -> CmdResult<GroupState> {
        self.with_channel(&req, |c, _| {
            c.exact_groups = true;
            Ok(GroupState::Exact)
        })
    }

    fn string_detail(&self, req: KeyRequest) -> CmdResult<StringDetail> {
        self.row_with_override(&req.target, &req.key)
    }

    fn set_override(&self, req: OverrideRequest) -> CmdResult<OverrideResult> {
        let d = self.row_with_override(&req.target, &req.key)?;
        if let Some((rule, msg)) = check_override(&d.row.original, &req.value) {
            return Ok(OverrideResult {
                accepted: false,
                rule: Some(rule),
                message: Some(msg),
                detail: Some(d),
            });
        }
        let key = d.row.key.clone();
        self.with_channel(&req.target, |c, _| {
            c.overrides.insert(key.clone(), req.value.clone());
            if matches!(c.summary.status, ApplyState::Applied) {
                c.summary.status = ApplyState::Stale {
                    reasons: vec!["overrides".into()],
                };
            }
            Ok(())
        })?;
        Ok(OverrideResult {
            accepted: true,
            rule: None,
            message: None,
            detail: Some(self.row_with_override(&req.target, &key)?),
        })
    }

    fn reset_override(&self, req: KeyRequest) -> CmdResult<StringDetail> {
        let d = self.row_with_override(&req.target, &req.key)?;
        self.with_channel(&req.target, |c, _| {
            c.overrides.remove(&d.row.key);
            Ok(())
        })?;
        self.row_with_override(&req.target, &req.key)
    }

    fn list_overrides(&self, req: ChannelRef) -> CmdResult<Vec<OverrideEntry>> {
        let ov = self.with_channel(&req, |c, _| Ok(c.overrides.clone()))?;
        Ok(ov
            .into_iter()
            .map(|(key, value)| OverrideEntry {
                original: self
                    .strings()
                    .iter()
                    .find(|r| r.key == key)
                    .map(|r| r.original.clone())
                    .unwrap_or_default(),
                key,
                value,
            })
            .collect())
    }

    fn list_patches(&self, req: ChannelRef) -> CmdResult<Vec<PatchView>> {
        self.with_channel(&req, |c, _| Ok(c.patches.clone()))
    }

    fn set_patch_enabled(&self, req: PatchToggle) -> CmdResult<PatchView> {
        self.with_channel(&req.target, |c, _| {
            let p = c
                .patches
                .iter_mut()
                .find(|p| p.id == req.id)
                .ok_or_else(|| {
                    AppError::new(ErrorCode::NotFound, format!("No patch {}", req.id))
                })?;
            p.enabled = req.enabled;
            let out = p.clone();
            c.profile.patches_disabled = c
                .patches
                .iter()
                .filter(|p| !p.enabled)
                .map(|p| p.id.clone())
                .collect();
            Ok(out)
        })
    }

    fn patch_detail(&self, req: PatchRef) -> CmdResult<PatchDetail> {
        self.with_channel(&req.target, |c, _| {
            let p = c
                .patches
                .iter()
                .find(|p| p.id == req.id)
                .cloned()
                .ok_or_else(|| {
                    AppError::new(ErrorCode::NotFound, format!("No patch {}", req.id))
                })?;
            Ok(patch_detail(p))
        })
    }

    fn add_user_patch(&self, req: NewPatch) -> CmdResult<PatchView> {
        if req.title.trim().is_empty() || req.key.trim().is_empty() {
            return Err(AppError::new(
                ErrorCode::Invalid,
                "A patch needs a title and a key.",
            ));
        }
        if req.op == LocOp::Replace && req.find.as_deref().unwrap_or("").is_empty() {
            return Err(AppError::new(
                ErrorCode::Invalid,
                "Replace needs the text to find.",
            ));
        }
        let stock = self
            .strings()
            .iter()
            .find(|r| r.key.eq_ignore_ascii_case(req.key.trim()))
            .cloned();
        let state = match (&stock, req.op, req.find.as_deref()) {
            (None, _, _) => PatchState::Missing,
            (Some(r), LocOp::Replace, Some(f)) if r.original.matches(f).count() != 1 => {
                PatchState::TargetChanged
            }
            (Some(r), LocOp::Set, _) if r.original == req.value => PatchState::NotNeeded,
            _ => PatchState::Applies,
        };
        let slug: String = req
            .title
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        let view = PatchView {
            id: format!("user-{slug}"),
            title: req.title.trim().into(),
            description: req.description.trim().into(),
            keys: 1,
            origin: PatchOrigin::User,
            state,
            enabled: true,
        };
        let text = patch_toml(&view, &req);
        self.with_channel(&req.target, |c, s| {
            if c.patches.iter().any(|p| p.id == view.id) {
                return Err(AppError::new(
                    ErrorCode::Invalid,
                    format!(
                        "A patch with the id {} exists. Pick another title.",
                        view.id
                    ),
                ));
            }
            c.patches.push(view.clone());
            s.patch_files.retain(|(p, _)| p.id != view.id);
            s.patch_files.push((view.clone(), text));
            Ok(())
        })?;
        self.log(
            LogLevel::Info,
            None,
            Some(&req.target.channel),
            &format!("Added patch {}", view.title),
        );
        Ok(view)
    }

    fn import_user_patch(&self, req: ChannelRef) -> CmdResult<PatchView> {
        self.with_channel(&req, |c, s| {
            let have: BTreeSet<String> = c.patches.iter().map(|p| p.id.clone()).collect();
            let pick = s
                .patch_files
                .iter()
                .rev()
                .find(|(p, _)| !have.contains(&p.id))
                .map(|(p, _)| p.clone())
                .unwrap_or_else(|| PatchView {
                    id: "user-synthetic-sample".into(),
                    title: "Synthetic sample patch".into(),
                    description: "Imported from a .patch.toml file.".into(),
                    keys: 1,
                    origin: PatchOrigin::User,
                    state: PatchState::Applies,
                    enabled: true,
                });
            if have.contains(&pick.id) {
                return Err(AppError::new(
                    ErrorCode::Invalid,
                    format!("{} is already in the list.", pick.title),
                ));
            }
            c.patches.push(pick.clone());
            Ok(pick)
        })
    }

    fn export_user_patch(&self, req: PatchRef) -> CmdResult<PatchExport> {
        self.with_channel(&req.target, |c, s| {
            let p = c
                .patches
                .iter()
                .find(|p| p.id == req.id)
                .cloned()
                .ok_or_else(|| {
                    AppError::new(ErrorCode::NotFound, format!("No patch {}", req.id))
                })?;
            let text = s
                .patch_files
                .iter()
                .find(|(x, _)| x.id == p.id)
                .map(|(_, t)| t.clone())
                .unwrap_or_else(|| {
                    format!(
                        "schema = 1\nid = {:?}\ntitle = {:?}\ndescription = {:?}\n",
                        p.id, p.title, p.description
                    )
                });
            if !s.patch_files.iter().any(|(x, _)| x.id == p.id) {
                s.patch_files.push((p.clone(), text.clone()));
            }
            Ok(PatchExport {
                file_name: format!("{}.patch.toml", p.id),
                text,
            })
        })
    }

    fn delete_user_patch(&self, req: PatchRef) -> CmdResult<Vec<PatchView>> {
        self.with_channel(&req.target, |c, s| {
            let p = c
                .patches
                .iter()
                .find(|p| p.id == req.id)
                .cloned()
                .ok_or_else(|| {
                    AppError::new(ErrorCode::NotFound, format!("No patch {}", req.id))
                })?;
            if p.origin == PatchOrigin::BuiltIn {
                return Err(AppError::new(
                    ErrorCode::Invalid,
                    "Built-in patches can be switched off, not deleted.",
                ));
            }
            c.patches.retain(|x| x.id != req.id);
            c.profile.patches_disabled.retain(|x| x != &req.id);
            if !s.patch_files.iter().any(|(x, _)| x.id == p.id) {
                s.patch_files.push((p, String::new()));
            }
            Ok(c.patches.clone())
        })
    }

    fn activity(&self, limit: u32) -> CmdResult<Vec<LogEntry>> {
        self.mcp_to_log();
        let s = self.lock();
        Ok(s.log.iter().rev().take(limit as usize).cloned().collect())
    }

    fn get_settings(&self) -> CmdResult<Settings> {
        Ok(self.lock().settings.clone())
    }

    fn save_settings(&self, req: Settings) -> CmdResult<Settings> {
        let mut s = self.lock();
        // Tokens are per environment: switching signs out (the UI asks first).
        let env_changed = s.settings.starorg.environment != req.starorg.environment;
        s.settings = req;
        if !s.settings.network.starorg || env_changed {
            mock_sign_out(&mut s);
        }
        Ok(s.settings.clone())
    }

    fn list_characters(&self, req: ChannelRef) -> CmdResult<CharactersView> {
        self.with_channel(&req, |c, _| Ok(characters_view(c)))
    }

    fn set_active_character(&self, req: SetActiveCharacter) -> CmdResult<CharactersView> {
        self.with_channel(&req.target, |c, _| {
            if req.mode == CharacterMode::Manual
                && !req
                    .character
                    .as_deref()
                    .is_some_and(|id| CHARACTERS.iter().any(|x| x.0 == id))
            {
                return Err(AppError::new(
                    ErrorCode::Invalid,
                    "Pick one of the characters found in the logs.",
                ));
            }
            c.active.mode = req.mode;
            c.active.character = if req.mode == CharacterMode::Manual {
                req.character.clone()
            } else {
                None
            };
            Ok(characters_view(c))
        })
    }

    fn owned_blueprints(&self, req: CharacterRequest) -> CmdResult<Vec<OwnedBlueprint>> {
        self.with_channel(&req.target, |c, _| {
            Ok(c.owned.get(&req.character).cloned().unwrap_or_default())
        })
    }

    fn import_smart_citizen(&self, req: ImportRequest) -> CmdResult<ImportReport> {
        let candidates = [
            "Synthetic Rifle Mk1",
            "Sample Armor Legs",
            "Sample Armor Arms",
            "Test Knife",
        ];
        self.with_channel(&req.target, |c, _| {
            let set = c.owned.entry(req.character.clone()).or_default();
            let known: Vec<String> = candidates
                .iter()
                .filter(|n| set.iter().any(|b| b.name == **n))
                .map(|s| s.to_string())
                .collect();
            let imported: Vec<String> = candidates
                .iter()
                .filter(|n| !known.iter().any(|k| k == *n))
                .map(|s| s.to_string())
                .collect();
            if !req.dry_run {
                for n in &imported {
                    set.push(OwnedBlueprint {
                        name: n.clone(),
                        ambiguous: false,
                        favorite: false,
                        sources: vec!["import:smart-citizen".into()],
                        acquired: Some("2026-11-01T10:00:00Z".into()),
                    });
                }
            }
            Ok(ImportReport {
                file: "~/Games/star-citizen/drive_c/users/pilot/SmartCitizen/owned-blueprints.json"
                    .into(),
                exported_at: Some("2026-11-01T10:00:00Z".into()),
                dry_run: req.dry_run,
                total: 9,
                not_completed: 2,
                duplicates: 0,
                imported,
                known,
                unmatched: vec!["[H] Retired Sample Helmet".into()],
                empty_catalog: false,
                ambiguous: vec![AmbiguousName {
                    name: "Sample Undersuit".into(),
                    candidates: vec![
                        "Sample Undersuit (Grey)".into(),
                        "Sample Undersuit (Red)".into(),
                    ],
                }],
            })
        })
    }

    fn forget_character(&self, req: CharacterRequest) -> CmdResult<CharactersView> {
        self.with_channel(&req.target, |c, _| {
            if !CHARACTERS.iter().any(|(id, _, _)| *id == req.character) {
                return Err(AppError::new(
                    ErrorCode::NotFound,
                    "No such character in the logs.",
                ));
            }
            c.owned.remove(&req.character);
            c.forgotten.insert(req.character.clone());
            if c.active.character.as_deref() == Some(req.character.as_str())
                || c.active.resolved.as_deref() == Some(req.character.as_str())
            {
                c.active = ActiveCharacter {
                    mode: CharacterMode::Auto,
                    character: None,
                    resolved: CHARACTERS
                        .iter()
                        .map(|(id, _, _)| id.to_string())
                        .find(|id| !c.forgotten.contains(id)),
                };
            }
            Ok(characters_view(c))
        })
    }

    fn fleet_view(&self, req: FleetViewRequest) -> CmdResult<FleetView> {
        self.with_channel(&req.target, |c, s| Ok(mock_fleet_view(c, s)))
    }

    fn fleet_import(&self, req: FleetImportRequest) -> CmdResult<FleetImportReport> {
        use starorg_core::fleet::import;
        let (file, text) = match &req.source {
            FleetSource::Text { text } => (None, text.clone()),
            FleetSource::File | FleetSource::Last => (
                Some("synthetic-fleet.json".to_string()),
                MOCK_FLEET_FILE.to_string(),
            ),
            FleetSource::StarOrg => {
                self.starorg_guild()?;
                if !self.lock().settings.starorg.pull_fleet {
                    return Err(sync_off("Pull fleet/hangar"));
                }
                (
                    Some("StarOrg Tools hangar".to_string()),
                    MOCK_FLEET_FILE.to_string(),
                )
            }
        };
        let parsed = super::fleet::parse(text.as_bytes())?;
        let out = self.with_channel(&req.target, |_, s| {
            let cat = mock_catalog();
            let mut out = import::run(&parsed, &cat, &s.fleet, req.merge);
            let mut appended = 0;
            if !req.dry_run {
                out.fleet.imported_at = Some(NOW.into());
                s.fleet = out.fleet.clone();
                appended = s.ship_names.append(&out.added);
            }
            Ok(super::fleet::report(
                &out,
                file.clone(),
                req.dry_run,
                appended,
            ))
        })?;
        self.log(
            LogLevel::Info,
            Some(JobKind::Import),
            Some(&req.target.channel),
            &format!(
                "Fleet import{}: {} hulls, {} unmatched, {} ambiguous",
                if req.dry_run { " (dry run)" } else { "" },
                out.hulls.len(),
                out.unmatched.len(),
                out.ambiguous.len()
            ),
        );
        Ok(out)
    }

    fn fleet_match(&self, req: FleetMatchRequest) -> CmdResult<FleetView> {
        self.with_channel(&req.target, |c, s| {
            let cat = mock_catalog();
            let target = super::fleet::match_target(&cat, req.hull.as_deref())?;
            let added = starorg_core::fleet::import::hand_match(&mut s.fleet, &req.name, target);
            s.ship_names.append(added.as_slice());
            Ok(mock_fleet_view(c, s))
        })
    }

    fn save_ship_names(&self, req: SaveShipNamesRequest) -> CmdResult<FleetView> {
        use starorg_core::fleet::store::{Scope, set_channel};
        let new = super::fleet::to_core(&req.settings)?;
        self.with_channel(&req.target, |c, s| {
            let scope = super::fleet::core_scope(req.scope);
            if scope == Scope::Shared {
                s.ship_names.merge_from(&new);
            }
            set_channel(&mut c.names_profile, scope, Some(&new));
            Ok(mock_fleet_view(c, s))
        })
    }

    fn fleet_clear(&self, req: ChannelRef) -> CmdResult<FleetView> {
        self.with_channel(&req, |c, s| {
            s.fleet.hulls.clear();
            s.fleet.pending.clear();
            s.fleet.imported_at = None;
            s.fleet.format = None;
            Ok(mock_fleet_view(c, s))
        })
    }

    fn starorg_status(&self, channel: String) -> CmdResult<StarOrgStatus> {
        let mut s = self.lock();
        starorg_view(&mut s, &channel);
        Ok(s.starorg.clone())
    }

    fn starorg_login(&self, channel: String) -> CmdResult<StarOrgStatus> {
        let mut s = self.lock();
        if !s.settings.network.starorg {
            return Err(AppError::new(
                ErrorCode::NetworkOff,
                "Turn on \"Connect StarOrg Tools\" under Settings -> Network first.",
            ));
        }
        s.starorg.connected = true;
        s.starorg.expired = false;
        s.starorg.user = Some("pilot_a_discord".into());
        s.starorg.guilds = mock_guilds();
        s.starorg.guild = Some("Synthetic Org".into());
        s.starorg.guild_id = Some("100000000000000001".into());
        s.starorg.expires = Some("2027-02-12T21:30:05Z".into());
        s.starorg.token_store = Some(TokenStoreKind::Keyring);
        starorg_view(&mut s, &channel);
        Ok(s.starorg.clone())
    }

    fn starorg_cancel_login(&self) -> CmdResult<()> {
        Ok(())
    }

    fn starorg_logout(&self, channel: String) -> CmdResult<StarOrgStatus> {
        let mut s = self.lock();
        mock_sign_out(&mut s);
        starorg_view(&mut s, &channel);
        Ok(s.starorg.clone())
    }

    fn set_starorg_guild(&self, req: StarOrgGuildRequest) -> CmdResult<StarOrgStatus> {
        let mut s = self.lock();
        let Some(g) = s.starorg.guilds.iter().find(|g| g.id == req.guild).cloned() else {
            return Err(AppError::new(
                ErrorCode::Invalid,
                "Pick one of your orgs on StarOrg Tools.",
            ));
        };
        s.starorg.guild = Some(g.name);
        s.starorg.guild_id = Some(g.id);
        starorg_view(&mut s, &req.channel);
        Ok(s.starorg.clone())
    }

    fn set_sync_characters(&self, req: SyncCharactersRequest) -> CmdResult<StarOrgStatus> {
        let mut s = self.lock();
        starorg_view(&mut s, &req.channel);
        for c in &mut s.starorg.sync_characters {
            c.ticked = req.ticked.contains(&c.id);
        }
        // An empty tick list is a real choice; keep it from being read as "default: all".
        if req.ticked.is_empty() {
            s.starorg.push_count = 0;
            return Ok(s.starorg.clone());
        }
        starorg_view(&mut s, &req.channel);
        Ok(s.starorg.clone())
    }

    fn starorg_pull(&self, req: StarOrgPullRequest) -> CmdResult<SyncReport> {
        let guild = self.starorg_guild()?;
        if !self.lock().settings.starorg.pull_blueprints {
            return Err(sync_off("Pull owned blueprints"));
        }
        let ch = ChannelRef {
            install: INSTALL_ID.into(),
            channel: req.channel.clone(),
        };
        if !CHARACTERS.iter().any(|c| c.0 == req.character) {
            return Err(AppError::new(
                ErrorCode::Invalid,
                "Pick one of the characters found in the logs.",
            ));
        }
        let remote = [
            "Synthetic Rifle Mk1",
            "Sample Medical Gun",
            "Test Multitool",
        ];
        let (added, already) = self.with_channel(&ch, |c, _| {
            let set = c.owned.entry(req.character.clone()).or_default();
            let mut added = Vec::new();
            let mut already = 0;
            for n in remote {
                if set.iter().any(|b| b.name == n) {
                    already += 1;
                } else {
                    added.push(n.to_string());
                    set.push(OwnedBlueprint {
                        name: n.into(),
                        ambiguous: false,
                        favorite: false,
                        sources: vec![format!("starorg:{guild}")],
                        acquired: Some(NOW.into()),
                    });
                }
            }
            Ok((added, already))
        })?;
        let mut s = self.lock();
        s.starorg.last_sync = Some(NOW.into());
        drop(s);
        self.log(
            LogLevel::Info,
            Some(JobKind::Sync),
            Some(&req.channel),
            &format!(
                "Pulled from StarOrg ({guild}): {} added, {already} already known",
                added.len()
            ),
        );
        Ok(SyncReport {
            direction: SyncDirection::Pull,
            channel: req.channel,
            guild: Some(guild),
            character: Some(req.character),
            added,
            already,
            unmatched: vec!["Retired Sample Visor".into()],
            sent: 0,
            time: NOW.into(),
        })
    }

    fn starorg_push(&self, channel: String) -> CmdResult<SyncReport> {
        let guild = self.starorg_guild()?;
        if !self.lock().settings.starorg.push_blueprints {
            return Err(sync_off("Push owned blueprints"));
        }
        let mut s = self.lock();
        starorg_view(&mut s, &channel);
        let sent = s.starorg.push_count;
        s.starorg.last_sync = Some(NOW.into());
        drop(s);
        self.log(
            LogLevel::Info,
            Some(JobKind::Sync),
            Some(&channel),
            &format!("Pushed {sent} owned blueprints to StarOrg ({guild})"),
        );
        Ok(SyncReport {
            direction: SyncDirection::Push,
            channel,
            guild: Some(guild),
            character: None,
            added: Vec::new(),
            already: 0,
            unmatched: Vec::new(),
            sent,
            time: NOW.into(),
        })
    }

    fn starorg_sync(&self, channel: String) -> CmdResult<SyncRun> {
        self.starorg_guild()?;
        let p = self.lock().settings.starorg.clone();
        if !(p.push_blueprints || p.pull_blueprints || p.pull_fleet) {
            return Err(AppError::new(
                ErrorCode::Invalid,
                "Nothing to sync: turn on a StarOrg switch under Settings -> StarOrg Tools -> What to sync.",
            ));
        }
        let mut reports = Vec::new();
        let mut skipped = Vec::new();
        if p.pull_blueprints {
            let who = self
                .list_characters(ChannelRef {
                    install: INSTALL_ID.into(),
                    channel: channel.clone(),
                })
                .ok()
                .and_then(|v| v.active.resolved);
            match who {
                Some(c) => match self.starorg_pull(StarOrgPullRequest {
                    channel: channel.clone(),
                    character: c,
                }) {
                    Ok(r) => reports.push(r),
                    Err(e) => skipped.push(format!("Pull: {}", e.message)),
                },
                None => skipped.push("Pull: no character found in the logs yet".into()),
            }
        }
        if p.push_blueprints {
            match self.starorg_push(channel.clone()) {
                Ok(r) => reports.push(r),
                Err(e) => skipped.push(format!("Push: {}", e.message)),
            }
        }
        let fleet = if p.pull_fleet {
            match self.fleet_import(FleetImportRequest {
                target: ChannelRef {
                    install: INSTALL_ID.into(),
                    channel: channel.clone(),
                },
                source: FleetSource::StarOrg,
                dry_run: false,
                merge: true,
            }) {
                Ok(r) => Some(r),
                Err(e) => {
                    skipped.push(format!("Fleet: {}", e.message));
                    None
                }
            }
        } else {
            None
        };
        Ok(SyncRun {
            channel,
            reports,
            fleet,
            skipped,
            time: NOW.into(),
        })
    }

    fn hooks_status(&self) -> CmdResult<HooksStatus> {
        Ok(self.lock().hooks.clone())
    }

    fn hooks_install(&self) -> CmdResult<HooksStatus> {
        let mut s = self.lock();
        if !s.hooks.supported {
            return Err(AppError::new(
                ErrorCode::Invalid,
                "Launch hooks are for the LUG Helper on Linux.",
            ));
        }
        for h in &mut s.hooks.scripts {
            if h.path.is_none() {
                h.path = Some(format!(
                    "/home/pilot/Games/star-citizen/sc-{}.sh",
                    serde_plain(&h.kind)
                ));
            }
            h.status = HookState::Current;
        }
        let out = s.hooks.clone();
        drop(s);
        self.log(
            LogLevel::Info,
            Some(JobKind::Hook),
            None,
            "Installed the launch hook block in sc-prelaunch.sh and sc-postexit.sh",
        );
        Ok(out)
    }

    fn hooks_uninstall(&self) -> CmdResult<HooksStatus> {
        let mut s = self.lock();
        for h in &mut s.hooks.scripts {
            h.status = HookState::Absent;
        }
        let out = s.hooks.clone();
        drop(s);
        self.log(
            LogLevel::Info,
            Some(JobKind::Hook),
            None,
            "Removed the launch hook block; the rest of each script is unchanged",
        );
        Ok(out)
    }

    fn doctor(&self) -> CmdResult<DoctorReport> {
        let chk = |level, label: &str, detail: Option<&str>| DoctorCheck {
            level,
            label: label.into(),
            detail: detail.map(str::to_string),
        };
        Ok(DoctorReport {
            sections: vec![
                DoctorSection {
                    title: "Installs".into(),
                    checks: vec![
                        chk(
                            CheckLevel::Ok,
                            "Install found by LUG Helper config",
                            Some(ROOT),
                        ),
                        chk(
                            CheckLevel::Info,
                            "Second install found by Lutris game entry",
                            Some(ALT_ROOT),
                        ),
                    ],
                },
                DoctorSection {
                    title: "Paths".into(),
                    checks: vec![
                        chk(
                            CheckLevel::Ok,
                            "LIVE/Data/Localization/english is writable",
                            None,
                        ),
                        chk(
                            CheckLevel::Info,
                            "LIVE/USER.cfg is a symlink",
                            Some("-> /home/pilot/Games/star-citizen/shared/USER.cfg"),
                        ),
                    ],
                },
                DoctorSection {
                    title: "Other tools".into(),
                    checks: vec![
                        chk(
                            CheckLevel::Warn,
                            "Smart Citizen stamp in LIVE global.ini",
                            Some("Frontend_PU_Version"),
                        ),
                        chk(
                            CheckLevel::Warn,
                            "Empty Data/languages.ini (0 bytes)",
                            Some("Use the launcher's Verify files to restore it."),
                        ),
                    ],
                },
                DoctorSection {
                    title: "Graphics".into(),
                    checks: vec![chk(
                        CheckLevel::Info,
                        "NVIDIA on Wayland (KDE): explicit sync disabled for the window",
                        None,
                    )],
                },
            ],
        })
    }

    fn export_diagnostics(&self) -> CmdResult<Diagnostics> {
        let mut text = String::from("StarOrg Codex diagnostics\n\n");
        for sec in self.doctor()?.sections {
            text.push_str(&format!("## {}\n", sec.title));
            for c in sec.checks {
                text.push_str(&format!("[{}] {}", serde_plain(&c.level), c.label));
                if let Some(d) = c.detail {
                    text.push_str(&format!(" ({d})"));
                }
                text.push('\n');
            }
            text.push('\n');
        }
        text.push_str("## Recent log\n");
        let s = self.lock();
        for e in s.log.iter().rev().take(50) {
            text.push_str(&format!(
                "{} {} {}\n",
                e.time,
                serde_plain(&e.level),
                e.message
            ));
        }
        text.push_str(&format!(
            "\n## Characters\n{} (geid {}) active on LIVE\nStarOrg user: {}\n",
            CHARACTERS[0].1,
            CHARACTERS[0].0,
            s.starorg.user.as_deref().unwrap_or("not signed in")
        ));
        drop(s);
        let (text, redacted) = redact(&text);
        Ok(Diagnostics {
            file_name: "starorg-strings-diagnostics-20261114-213005.txt".into(),
            text,
            redacted,
        })
    }

    fn save_diagnostics(&self) -> CmdResult<Option<String>> {
        let d = self.export_diagnostics()?;
        self.log(LogLevel::Info, None, None, "Saved diagnostics");
        Ok(Some(format!("~/{}", d.file_name)))
    }

    fn update_status(&self) -> CmdResult<UpdateStatus> {
        self.update.status(self.updates_on())
    }

    fn check_for_updates(&self, req: UpdateCheckRequest) -> CmdResult<UpdateStatus> {
        self.update.check(self.updates_on(), req.force)
    }

    fn set_update_prefs(&self, req: UpdatePrefs) -> CmdResult<UpdateStatus> {
        self.update.set_prefs(self.updates_on(), req)
    }

    fn skip_update(&self, version: String) -> CmdResult<UpdateStatus> {
        self.update.skip(self.updates_on(), version)
    }

    fn open_release_page(&self) -> CmdResult<()> {
        self.update.open_release_page()
    }

    fn install_update(&self) -> CmdResult<UpdateStatus> {
        self.update.install(self.updates_on())
    }

    fn mcp_status(&self) -> CmdResult<McpStatus> {
        self.mcp_to_log();
        Ok(self.mcp.status())
    }

    /// A real loopback server with no game data: every data tool answers with an error.
    fn set_mcp_enabled(&self, req: McpToggle) -> CmdResult<McpStatus> {
        let st = if req.enabled {
            self.mcp.enable(|| {
                Ok(starorg_mcp::store::StoreConfig {
                    channels: vec![],
                    default_channel: None,
                    cache_dir: std::env::temp_dir().join("starorg-strings-mock-mcp"),
                    uex_dir: None,
                    our_outputs: vec![],
                    extractor: std::sync::Arc::new(NoGameData),
                })
            })
        } else {
            self.mcp.disable()
        };
        self.mcp_to_log();
        Ok(st)
    }
}

/// The mock has no archive to extract from.
struct NoGameData;

impl starorg_mcp::worker::Extractor for NoGameData {
    fn extract(
        &self,
        _: &std::path::Path,
        dataset: &str,
        _: &std::path::Path,
    ) -> Result<starorg_mcp::worker::Measurement, String> {
        Err(format!("{dataset}: the mock backend has no game data"))
    }
}

impl MockBackend {
    fn mcp_to_log(&self) {
        for (level, msg) in self.mcp.take_log() {
            self.log(level, Some(JobKind::Mcp), None, &msg);
        }
    }

    fn updates_on(&self) -> bool {
        self.lock().settings.network.check_updates
    }
}

impl MockBackend {
    /// The signed-in guild, or the error the UI shows when StarOrg is off or not connected.
    fn starorg_guild(&self) -> CmdResult<String> {
        let s = self.lock();
        if !s.settings.network.starorg {
            return Err(AppError::new(
                ErrorCode::NetworkOff,
                "Turn on \"Connect StarOrg Tools\" under Settings -> Network first.",
            ));
        }
        match (&s.starorg.connected, &s.starorg.guild) {
            (true, Some(g)) => Ok(g.clone()),
            _ => Err(AppError::new(
                ErrorCode::NeedsDecision,
                "Sign in to StarOrg Tools first (Settings -> StarOrg Tools).",
            )),
        }
    }
}

/// Replace home paths, user names and character ids (addendum §3 privacy).
pub fn redact(text: &str) -> (String, u32) {
    let mut out = text.to_string();
    let mut n = 0u32;
    let mut swap = |out: &mut String, from: &str, to: &str| {
        let c = out.matches(from).count() as u32;
        if c > 0 {
            *out = out.replace(from, to);
            n += c;
        }
    };
    swap(&mut out, "/home/pilot", "~");
    swap(&mut out, "users/pilot", "users/<user>");
    for (id, name, _) in CHARACTERS {
        swap(&mut out, id, "<geid>");
        swap(&mut out, name, "<character>");
    }
    swap(&mut out, "pilot_a_discord", "<starorg-user>");
    (out, n)
}

/// A user patch as the TOML of spec 4.5 (one loc entry).
fn patch_toml(p: &PatchView, req: &NewPatch) -> String {
    let op = match req.op {
        LocOp::Set => "set",
        LocOp::Append => "append",
        LocOp::Prepend => "prepend",
        LocOp::Replace => "replace",
    };
    let mut t = format!(
        "schema = 1\nid = {:?}\ntitle = {:?}\ndescription = {:?}\n\n[[loc]]\nkey = {:?}\nop = {op:?}\n",
        p.id, p.title, p.description, req.key
    );
    if let Some(f) = &req.find {
        t.push_str(&format!("find = {f:?}\n"));
    }
    t.push_str(&format!("value = {:?}\n", req.value));
    t
}

fn patch_detail(p: PatchView) -> PatchDetail {
    let ex = |key: &str, before: &str, after: &str| PatchExample {
        key: key.into(),
        before: before.into(),
        after: after.into(),
    };
    let (range, reason, examples) = match p.id.as_str() {
        "syn-typo-cargo" => (
            Some("4.9.0 - 4.10.x".into()),
            "The stock text still has the typo in this build, so the patch applies.".to_string(),
            vec![
                ex(
                    "vehicle_DescSYN_Ship_00194",
                    "Cargo grid: 12 SUC.",
                    "Cargo grid: 12 SCU.",
                ),
                ex(
                    "vehicle_DescSYN_Ship_00388",
                    "Holds 8 SUC of cargo.",
                    "Holds 8 SCU of cargo.",
                ),
            ],
        ),
        "syn-missing-rep" => (
            None,
            "The reputation line is still missing, so the patch adds it.".to_string(),
            vec![ex(
                "mission_DescSYN_Contract_00045",
                "Reward: 12,500 aUEC",
                "Reward: 12,500 aUEC\\nReputation: +150",
            )],
        ),
        "syn-old-name" => (
            Some("up to 4.9.x".into()),
            "Not needed: the game text already reads the way this patch would make it.".to_string(),
            vec![ex(
                "item_NameSYN_Cooler_S1_00001",
                "Sample Cooler 1",
                "Sample Cooler 1",
            )],
        ),
        _ => (
            Some("4.10.0 - 4.10.x".into()),
            "Target changed: the stock text is no longer what the patch expects, so it is skipped and reported."
                .to_string(),
            vec![ex(
                "item_DescSYN_Rifle_00004",
                "A synthetic rifle, number 4 (new wording).",
                "A synthetic rifle, number 4.",
            )],
        ),
    };
    PatchDetail {
        kind: if p.id == "syn-missing-rep" {
            "data".into()
        } else {
            "loc".into()
        },
        patch: p,
        range,
        reason,
        examples,
    }
}

fn characters_view(c: &MockChannel) -> CharactersView {
    CharactersView {
        channel: c.summary.channel.clone(),
        characters: CHARACTERS
            .iter()
            .filter(|(id, _, _)| !c.forgotten.contains(*id))
            .map(|(id, name, seen)| CharacterView {
                id: (*id).into(),
                name: (*name).into(),
                last_seen: Some((*seen).into()),
                owned: c.owned.get(*id).map_or(0, |v| v.len() as u32),
            })
            .collect(),
        active: c.active.clone(),
        unattributed: 1,
        last_scan: Some("2026-11-14T19:40:00Z".into()),
    }
}

/// Render the preview samples from a profile, so switches visibly change the text.
pub fn preview(p: &Profile) -> Vec<PreviewItem> {
    let line_on = |key: &str| !p.lines_off.iter().any(|l| l == key);
    let block = |prose: &str, lines: Vec<String>| {
        if lines.is_empty() {
            return prose.to_string();
        }
        let stats = format!("{}\\n{}", p.header, lines.join("\\n"));
        if p.stats_above {
            format!("{stats}\\n\\n------\\n\\n{prose}")
        } else {
            format!("{prose}\\n\\n{stats}")
        }
    };
    let mut out = Vec::new();

    let ship_prose = "A synthetic light fighter used for testing.";
    let mut ship_lines = Vec::new();
    if p.ships {
        for (key, l) in [
            (
                "ships.role",
                "Role: Light Fighter  |  Career: Combat  |  Crew: 1",
            ),
            ("ships.size", "Size: 21 × 16.5 × 5.5 m"),
            ("ships.speed", "SCM: 226 m/s  |  Max: 1,193 m/s"),
            ("ships.boost", "Boost: 520 m/s fwd  |  268 m/s back"),
            ("ships.cargo", "Cargo: 0 SCU"),
        ] {
            if line_on(key) {
                ship_lines.push(l.to_string());
            }
        }
    }
    out.push(PreviewItem {
        key: "vehicle_DescSYN_Fighter".into(),
        label: "Ship description".into(),
        category: "ships".into(),
        original: ship_prose.into(),
        result: block(ship_prose, ship_lines),
    });

    let name = "Sample Cooler";
    let t = &p.tags.components;
    let word = |id: &str| -> &str {
        let st = t.styles.get(id).copied().unwrap_or(TagStyle::Medium);
        match (id, st) {
            ("class", TagStyle::Short) => "IND",
            ("class", _) => "Industrial",
            ("size", TagStyle::Long) => "Size 2",
            ("size", _) => "S2",
            ("grade", TagStyle::Long) => "Grade A",
            ("grade", _) => "A",
            _ => "",
        }
    };
    let parts: Vec<&str> = if t.enabled {
        t.elements
            .iter()
            .map(|e| word(e))
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        Vec::new()
    };
    let tagged = if parts.is_empty() {
        name.to_string()
    } else {
        let sep = match t.separator {
            TagSeparator::Dash => "-",
            TagSeparator::None => "",
            TagSeparator::Space => " ",
            TagSeparator::Underscore => "_",
            TagSeparator::Dot => ".",
            TagSeparator::Slash => "/",
            TagSeparator::Pipe => "|",
        };
        let (l, r) = match t.enclosing {
            TagEnclosing::Square => ("[", "]"),
            TagEnclosing::Round => ("(", ")"),
            TagEnclosing::Curly => ("{", "}"),
            TagEnclosing::Angle => ("<", ">"),
            TagEnclosing::None => ("", ""),
        };
        let tag = format!("{l}{}{r}", parts.join(sep));
        match t.placement {
            TagPlacement::Prefix => format!("{tag} {name}"),
            TagPlacement::Suffix => format!("{name} {tag}"),
        }
    };
    out.push(PreviewItem {
        key: "item_NameSYN_Cooler_S2".into(),
        label: "Component name".into(),
        category: "components".into(),
        original: name.into(),
        result: tagged,
    });

    let title = "Synthetic Recovery Contract";
    let mut tt = title.to_string();
    if p.missions {
        let wrap = |s: &str| {
            if p.mission.title_em {
                format!(" <EM4>{s}</EM4>")
            } else {
                format!(" {s}")
            }
        };
        if p.mission.title_blueprint {
            tt.push_str(&wrap("[BP]"));
        }
        if p.mission.title_reputation {
            tt.push_str(&wrap("[150 Rep]"));
        }
    }
    out.push(PreviewItem {
        key: "mission_TitleSYN_Recovery".into(),
        label: "Mission title".into(),
        category: "missions".into(),
        original: title.into(),
        result: tt,
    });
    out
}
