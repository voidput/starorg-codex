//! Core structs -> contract shapes, and the profile both ways.
//!
//! The profile conversion has no defaults of its own: every contract field maps to one field
//! of M2's `enhance::Profile`, and back.

use crate::contract::schema::preset_profile;
use crate::contract::types::*;
use starorg_core::apply::{self, Outcome};
use starorg_core::backup;
use starorg_core::blueprints::Selection;
use starorg_core::build::BuildInfo;
use starorg_core::enhance::profile as ep;
use starorg_core::patch;
use starorg_core::source::Warning as CoreWarning;
use starorg_core::state;
use std::collections::BTreeMap;
use std::path::Path;

pub fn err(code: ErrorCode, msg: impl Into<String>) -> AppError {
    AppError::new(code, msg)
}

pub fn internal(msg: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorCode::Internal, msg.to_string())
}

pub fn path_text(p: &Path) -> String {
    p.display().to_string()
}

/// The kebab name serde gives a value (`"needs-setup"`).
pub fn kebab<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------------------
// Builds
// ---------------------------------------------------------------------------------------

pub fn build(b: Option<&BuildInfo>) -> Build {
    Build {
        branch: b.map(|b| b.branch.clone()),
        game_version: b.and_then(|b| b.game_version).map(|v| v.to_string()),
        change: b.and_then(|b| b.change),
        version: b.map(|b| b.version.clone()),
    }
}

pub fn build_json(b: &apply::BuildJson) -> Build {
    Build {
        branch: b.branch.clone(),
        game_version: b.game_version.clone(),
        change: b.change,
        version: b.version.clone(),
    }
}

pub fn build_record(b: &backup::BuildRecord) -> Build {
    Build {
        branch: b.branch.clone(),
        game_version: b.game_version.clone(),
        change: b.change,
        version: b.version.clone(),
    }
}

pub fn applied_build(a: &apply::Applied) -> Build {
    Build {
        branch: a.branch.clone(),
        game_version: a.game_version.clone(),
        change: a.change,
        version: a.version.clone(),
    }
}

pub fn state(s: &apply::State) -> ApplyState {
    match s {
        apply::State::Applied => ApplyState::Applied,
        apply::State::Stale { reasons } => ApplyState::Stale {
            reasons: reasons.clone(),
        },
        apply::State::ChangedByAnotherTool { tool } => {
            ApplyState::ChangedByAnotherTool { tool: tool.clone() }
        }
        apply::State::NotApplied => ApplyState::NotApplied,
    }
}

// ---------------------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------------------

pub fn result(o: Outcome) -> ApplyResult {
    match o {
        Outcome::Applied => ApplyResult::Applied,
        Outcome::UpToDate => ApplyResult::UpToDate,
        Outcome::Skipped => ApplyResult::Skipped,
        Outcome::Failed => ApplyResult::Failed,
        Outcome::DryRun => ApplyResult::DryRun,
        Outcome::NeedsSetup => ApplyResult::NeedsSetup,
        Outcome::NeedsDecision => ApplyResult::NeedsDecision,
        Outcome::Removed => ApplyResult::Removed,
        Outcome::Restored => ApplyResult::Restored,
        Outcome::NothingToDo => ApplyResult::NothingToDo,
    }
}

pub fn zero_counts() -> ApplyCounts {
    ApplyCounts {
        changed: 0,
        ships: 0,
        components: 0,
        ship_weapons: 0,
        fps_weapons: 0,
        missiles: 0,
        missions: 0,
        overrides: 0,
        dropped: 0,
    }
}

/// M1's `counts` object (spec 8).
pub fn counts(v: &serde_json::Value) -> ApplyCounts {
    let n = |k: &str| v.get(k).and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;
    ApplyCounts {
        changed: n("changed"),
        ships: n("ships"),
        components: n("components"),
        ship_weapons: n("ship_weapons"),
        fps_weapons: n("fps_weapons"),
        missiles: n("missiles"),
        missions: n("missions"),
        overrides: n("overrides"),
        dropped: n("dropped"),
    }
}

/// M1's per-patch summary (`applies`, `retired`, an entry state, or `empty`).
pub fn patch_state(s: &str) -> PatchState {
    serde_json::from_value(serde_json::Value::String(s.to_string())).unwrap_or(PatchState::NoData)
}

pub fn warnings(ws: &[CoreWarning]) -> Vec<Warning> {
    ws.iter()
        .map(|w| Warning {
            kind: w.kind.clone(),
            detail: w.detail.clone(),
        })
        .collect()
}

pub fn owned(s: &Selection) -> OwnedSelection {
    OwnedSelection {
        mode: match s.mode.as_str() {
            "any" => OwnedMode::Any,
            "character" => OwnedMode::Character,
            _ => OwnedMode::Auto,
        },
        characters: s.characters.clone(),
        count: s.count as u32,
        ambiguous: s.ambiguous as u32,
    }
}

/// The line M1's `user.cfg` step adds, from its action text.
fn cfg_line(action: Option<&str>) -> Option<String> {
    match action {
        Some(a) if a.starts_with("append") || a.starts_with("replace") => {
            Some(starorg_core::usercfg::OUR_LINE.to_string())
        }
        _ => None,
    }
}

pub fn report(r: &apply::Report, backup_dir: Option<&Path>) -> ApplyReport {
    ApplyReport {
        channel: r.channel.clone(),
        result: result(r.result),
        reason: r.reason.clone(),
        build: build_json(&r.build),
        counts: counts(&r.counts),
        patches: r
            .patches
            .iter()
            .map(|p| PatchOutcome {
                id: p.id.clone(),
                state: patch_state(&p.state),
            })
            .collect(),
        warnings: warnings(&r.warnings),
        user_cfg: UserCfgReport {
            changed: r.user_cfg.changed,
            path: r.user_cfg.path.as_deref().map(path_text),
            link_target: r.user_cfg.link_target.as_deref().map(path_text),
            shared_with: r.user_cfg.shared_with.clone(),
            line: cfg_line(r.user_cfg.action.as_deref()),
        },
        backup: r.backup.clone(),
        backup_dir: backup_dir.map(path_text),
        target: r.target.as_deref().map(path_text),
        target_is_link: r.target_is_link,
        recovered: r.recovered.clone(),
        owned: r.owned.as_ref().map(owned),
        duration_ms: r.duration_ms,
    }
}

/// A report for something the backend decided before reaching the core.
pub fn plain_report(channel: &str, b: Build, result: ApplyResult, reason: &str) -> ApplyReport {
    ApplyReport {
        channel: channel.to_string(),
        result,
        reason: Some(reason.to_string()),
        build: b,
        counts: zero_counts(),
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

// ---------------------------------------------------------------------------------------
// Backups
// ---------------------------------------------------------------------------------------

pub fn backup_view(b: &backup::Backup) -> BackupView {
    let m = &b.manifest;
    BackupView {
        id: b.id.clone(),
        created: m.created.clone(),
        reason: match m.reason {
            backup::Reason::PreExisting => BackupReason::PreExisting,
            backup::Reason::BeforeApply => BackupReason::BeforeApply,
            backup::Reason::BeforeRestore => BackupReason::BeforeRestore,
            backup::Reason::Manual => BackupReason::Manual,
        },
        pinned: b.pinned(),
        build: build_record(&m.build),
        existed: m.file.existed,
        size: m.file.size,
        origin: match m.file.origin {
            backup::FileOrigin::Ours => FileOrigin::Ours,
            backup::FileOrigin::Foreign => FileOrigin::Foreign,
            backup::FileOrigin::None => FileOrigin::None,
        },
        foreign_tool: m.file.foreign_tool.clone(),
    }
}

// ---------------------------------------------------------------------------------------
// Profile
// ---------------------------------------------------------------------------------------

const SEPARATORS: &[(TagSeparator, &str)] = &[
    (TagSeparator::Dash, "-"),
    (TagSeparator::None, ""),
    (TagSeparator::Space, " "),
    (TagSeparator::Underscore, "_"),
    (TagSeparator::Dot, "."),
    (TagSeparator::Slash, "/"),
    (TagSeparator::Pipe, "|"),
];

const ENCLOSINGS: &[(TagEnclosing, &str)] = &[
    (TagEnclosing::Square, "[]"),
    (TagEnclosing::Round, "()"),
    (TagEnclosing::Curly, "{}"),
    (TagEnclosing::Angle, "<>"),
    (TagEnclosing::None, ""),
];

fn style_name(s: TagStyle) -> &'static str {
    match s {
        TagStyle::Short => "short",
        TagStyle::Medium => "medium",
        TagStyle::Long => "long",
    }
}

fn tag_to(t: &ep::TagStyle) -> CmdResult<TagKindSettings> {
    let sep = SEPARATORS
        .iter()
        .find(|(_, s)| *s == t.separator)
        .map(|(v, _)| *v)
        .ok_or_else(|| internal(format!("tag separator {:?}", t.separator)))?;
    let enc = ENCLOSINGS
        .iter()
        .find(|(_, s)| *s == t.enclosing)
        .map(|(v, _)| *v)
        .ok_or_else(|| internal(format!("tag enclosing {:?}", t.enclosing)))?;
    let mut styles = BTreeMap::new();
    for (k, v) in &t.styles {
        let s = match v.as_str() {
            "short" => TagStyle::Short,
            "long" => TagStyle::Long,
            _ => continue,
        };
        styles.insert(k.clone(), s);
    }
    Ok(TagKindSettings {
        enabled: t.enabled,
        elements: t.elements.clone(),
        styles,
        separator: sep,
        enclosing: enc,
        placement: if t.placement == "suffix" {
            TagPlacement::Suffix
        } else {
            TagPlacement::Prefix
        },
    })
}

fn tag_from(t: &TagKindSettings) -> ep::TagStyle {
    let lookup = |v: TagSeparator| SEPARATORS.iter().find(|(x, _)| *x == v).map(|(_, s)| *s);
    let enc = |v: TagEnclosing| ENCLOSINGS.iter().find(|(x, _)| *x == v).map(|(_, s)| *s);
    ep::TagStyle {
        enabled: t.enabled,
        elements: t.elements.clone(),
        styles: t
            .styles
            .iter()
            .filter(|(_, s)| **s != TagStyle::Medium)
            .map(|(k, s)| (k.clone(), style_name(*s).to_string()))
            .collect(),
        separator: lookup(t.separator).unwrap_or("-").to_string(),
        enclosing: enc(t.enclosing).unwrap_or("[]").to_string(),
        placement: match t.placement {
            TagPlacement::Prefix => "prefix",
            TagPlacement::Suffix => "suffix",
        }
        .to_string(),
    }
}

const PRESETS: [(Preset, &str); 3] = [
    (Preset::Recommended, "recommended"),
    (Preset::Minimal, "minimal"),
    (Preset::Everything, "everything"),
];

pub fn preset_name(p: Preset) -> &'static str {
    PRESETS
        .iter()
        .find(|(x, _)| *x == p)
        .map(|(_, n)| *n)
        .unwrap_or("recommended")
}

/// Which preset `p` equals, if any.
fn detect_preset(p: &ep::Profile) -> Option<Preset> {
    PRESETS
        .iter()
        .find(|(_, n)| ep::Profile::preset(n).is_ok_and(|x| x == *p))
        .map(|(v, _)| *v)
}

pub fn profile_to(core: &ep::Profile, disabled: Vec<String>) -> CmdResult<Profile> {
    let tags = &core.tags;
    let m = &core.mission;
    Ok(Profile {
        preset: detect_preset(core),
        ships: core.ships,
        components: core.components,
        ship_weapons: core.ship_weapons,
        fps_weapons: core.fps_weapons,
        missiles: core.missiles,
        missions: core.missions,
        lines_off: core.lines_off.iter().cloned().collect(),
        stats_above: core.stats_above,
        header: core.header.clone(),
        tags: TagSettings {
            components: tag_to(&tags.components)?,
            ship_weapons: tag_to(&tags.ship_weapons)?,
            missiles: tag_to(&tags.missiles)?,
            short_names: tags.short_names,
            tool_size_tag: tags.tool_size_tag,
        },
        mission: MissionSettings {
            title_blueprint: m.title_blueprint,
            title_reputation: m.title_reputation,
            title_em: m.title_em,
            blueprints: m.blueprints,
            item_rewards: m.item_rewards,
            details: m.details,
            bullet_tags: m.bullet_tags,
            owned_marker: m.owned_marker,
            max_list: m.max_list as u32,
            header_em: m.header_em,
            label_em: m.label_em,
        },
        branding: core.branding,
        patches_disabled: disabled,
    })
}

pub fn profile_from(p: &Profile) -> ep::Profile {
    let m = &p.mission;
    ep::Profile {
        ships: p.ships,
        components: p.components,
        ship_weapons: p.ship_weapons,
        fps_weapons: p.fps_weapons,
        missiles: p.missiles,
        missions: p.missions,
        lines_off: p.lines_off.iter().cloned().collect(),
        stats_above: p.stats_above,
        header: p.header.clone(),
        tags: ep::Tags {
            components: tag_from(&p.tags.components),
            ship_weapons: tag_from(&p.tags.ship_weapons),
            missiles: tag_from(&p.tags.missiles),
            short_names: p.tags.short_names,
            tool_size_tag: p.tags.tool_size_tag,
        },
        mission: ep::Mission {
            title_blueprint: m.title_blueprint,
            title_reputation: m.title_reputation,
            title_em: m.title_em,
            blueprints: m.blueprints,
            item_rewards: m.item_rewards,
            details: m.details,
            bullet_tags: m.bullet_tags,
            owned_marker: m.owned_marker,
            max_list: m.max_list as usize,
            header_em: m.header_em,
            label_em: m.label_em,
        },
        branding: p.branding,
    }
}

/// A channel's `profile.toml` in contract form.
pub fn profile_of(file: &state::Profile) -> CmdResult<Profile> {
    let core = file
        .enhance()
        .map_err(|e| err(ErrorCode::Invalid, format!("profile.toml: {e}")))?;
    profile_to(&core, file.disabled_patches())
}

/// Write `p` into a `profile.toml` table, keeping keys we do not know. A profile equal to
/// a preset is stored as just `preset = "..."`; anything else as the preset plus a full
/// `[enhance]` table. Refused (and nothing changed) when the core would not read it back.
pub fn store_profile(file: &mut state::Profile, p: &Profile) -> CmdResult<()> {
    let core = profile_from(p);
    let t = &mut file.table;
    t.entry("schema").or_insert(toml::Value::Integer(1));
    match detect_preset(&core) {
        Some(pre) => {
            t.insert("preset".into(), preset_name(pre).into());
            t.remove("enhance");
        }
        None => {
            t.insert(
                "preset".into(),
                preset_name(p.preset.unwrap_or(Preset::Recommended)).into(),
            );
            let v = toml::Value::try_from(&core).map_err(internal)?;
            t.insert("enhance".into(), v);
        }
    }
    let mut patches = t
        .get("patches")
        .and_then(|v| v.as_table())
        .cloned()
        .unwrap_or_default();
    patches.insert(
        "disabled".into(),
        toml::Value::Array(
            p.patches_disabled
                .iter()
                .map(|s| toml::Value::String(s.clone()))
                .collect(),
        ),
    );
    t.insert("patches".into(), toml::Value::Table(patches));
    // The core must read back exactly what was asked for (header rules, EM numbers).
    let back = file
        .enhance()
        .map_err(|e| err(ErrorCode::Invalid, e.to_string()))?;
    if back != core {
        return Err(internal("profile did not round-trip through profile.toml"));
    }
    Ok(())
}

/// The contract preset profile, as M2 has it (kept in `schema.rs` for the mock too).
pub fn preset(p: Preset) -> Profile {
    preset_profile(p)
}

/// Built-in patch ids (what "Fix known text bugs" off switches off).
pub fn builtin_patch_ids(set: &patch::PatchSet) -> Vec<String> {
    set.patches
        .iter()
        .filter(|p| matches!(p.origin, patch::PatchOrigin::Builtin))
        .map(|p| p.file.id.clone())
        .collect()
}
