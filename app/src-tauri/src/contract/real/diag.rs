//! Read-only views for the real backend: Strings queries, previews, patch views, doctor and
//! redacted diagnostics.

use super::RealBackend;
use super::convert::{kebab, path_text};
use super::table::Table;
use crate::contract::types::*;
use starorg_core::apply::{self, Target};
use starorg_core::backup;
use starorg_core::blueprints;
use starorg_core::fsutil;
use starorg_core::patch::{self, LoadedPatch};
use starorg_core::paths::resolve_ci;
use starorg_core::state::INTENT_JSON;
use starorg_core::usercfg;

// ---------------------------------------------------------------------------------------
// Strings
// ---------------------------------------------------------------------------------------

/// One changed row per category, as the in-game preview shows them.
pub fn preview(t: &Table) -> Vec<PreviewItem> {
    const WANT: &[(&str, &str)] = &[
        ("ships", "Ship description"),
        ("components", "Ship component"),
        ("ship-weapons", "Ship weapon"),
        ("fps-weapons", "Personal weapon"),
        ("missiles", "Missile"),
        ("missions", "Contract"),
    ];
    let item = |r: &StringRow, label: &str, cat: &str| PreviewItem {
        key: r.key.clone(),
        label: label.into(),
        category: cat.into(),
        original: r.original.clone(),
        result: r.result.clone(),
    };
    let changed = |r: &&StringRow| r.source != StringSource::Unmodified;
    let mut out = Vec::new();
    for (cat, label) in WANT {
        if let Some(r) = t.rows.iter().filter(changed).find(|r| r.category == *cat) {
            out.push(item(r, label, cat));
        }
    }
    // A contract with a blueprint list, so the marker colours show; one with an
    // `[Owned]` line first.
    let lists = || {
        t.rows
            .iter()
            .filter(changed)
            .filter(|r| r.category == "missions" && r.result.contains("BLUEPRINTS"))
    };
    if let Some(r) = lists()
        .find(|r| r.result.contains("[Owned]"))
        .or_else(|| lists().next())
        && !out.iter().any(|x| x.key == r.key)
    {
        out.push(item(r, "Contract description", "missions"));
    }
    let test = starorg_core::enhance::profile::COLOUR_TEST_KEY;
    if let Some(r) = t
        .rows
        .iter()
        .filter(changed)
        .find(|r| starorg_core::ini::strip_flag_suffix(&r.key).eq_ignore_ascii_case(test))
    {
        out.push(item(r, "Colour test", "ui"));
    }
    out
}

// ---------------------------------------------------------------------------------------
// Patches
// ---------------------------------------------------------------------------------------

fn state_of(p: &LoadedPatch, t: Option<&Table>) -> PatchState {
    if !p.enabled {
        return PatchState::Disabled;
    }
    t.and_then(|t| t.summary.iter().find(|s| s.id == p.file.id))
        .map(|s| super::convert::patch_state(&s.state))
        .unwrap_or(PatchState::NoData)
}

pub fn patch_view(p: &LoadedPatch, t: Option<&Table>) -> PatchView {
    PatchView {
        id: p.file.id.clone(),
        title: p.file.title.clone(),
        description: p.file.description.clone(),
        keys: (p.file.loc.len() + p.file.data.len()) as u32,
        origin: match p.origin {
            patch::PatchOrigin::Builtin => PatchOrigin::BuiltIn,
            patch::PatchOrigin::User { .. } => PatchOrigin::User,
        },
        state: state_of(p, t),
        enabled: p.enabled,
    }
}

fn range(b: &patch::Builds) -> Option<String> {
    let v = match (b.game_min.as_str(), b.game_max.as_str()) {
        ("", "") => None,
        (lo, "") => Some(format!("{lo} and later")),
        ("", hi) => Some(format!("up to {hi}")),
        (lo, hi) => Some(format!("{lo} - {hi}")),
    };
    let c = match (b.change_min, b.change_max) {
        (0, 0) => None,
        (lo, 0) => Some(format!("change {lo} and later")),
        (0, hi) => Some(format!("up to change {hi}")),
        (lo, hi) => Some(format!("changes {lo} - {hi}")),
    };
    match (v, c) {
        (None, None) => None,
        (Some(v), None) => Some(v),
        (None, Some(c)) => Some(c),
        (Some(v), Some(c)) => Some(format!("{v}, {c}")),
    }
}

fn reason(s: PatchState) -> &'static str {
    match s {
        PatchState::Applies => "The stock text of this build still needs it, so the patch applies.",
        PatchState::NotNeeded => "This build already has the fix; the patch changes nothing.",
        PatchState::Retired => "Every entry is already fixed in this build; the patch is retired.",
        PatchState::TargetChanged => {
            "The stock text changed since the patch was written; it was left out."
        }
        PatchState::NothingToAdd => "There is nothing to add for this build.",
        PatchState::Missing => "A key or record the patch needs is not in this build.",
        PatchState::OutOfRange => "This build is outside the range the patch is written for.",
        PatchState::Conflict => "Another layer writes the same key; the patch was left out.",
        PatchState::Disabled => "Switched off in this channel's profile.",
        PatchState::NoData => {
            "Game data of this build has not been read yet; apply (or a dry run) shows the outcome."
        }
    }
}

pub fn patch_detail(p: &LoadedPatch, t: Option<&Table>) -> PatchDetail {
    let view = patch_view(p, t);
    let examples = p
        .file
        .loc
        .iter()
        .filter_map(|l| {
            let t = t?;
            let (_, r) = t.row(&l.key)?;
            Some(PatchExample {
                key: r.key.clone(),
                before: r.original.clone(),
                after: r.result.clone(),
            })
        })
        .take(10)
        .collect();
    PatchDetail {
        kind: if p.file.loc.is_empty() { "data" } else { "loc" }.into(),
        range: range(&p.file.builds),
        reason: reason(view.state).into(),
        patch: view,
        examples,
    }
}

/// "Add patch": a one-entry loc patch (spec 4.5).
pub fn patch_toml(id: &str, req: &NewPatch) -> String {
    let op = match req.op {
        LocOp::Set => "set",
        LocOp::Append => "append",
        LocOp::Prepend => "prepend",
        LocOp::Replace => "replace",
    };
    let q = |s: &str| toml::Value::String(s.to_string()).to_string();
    let mut t = format!(
        "schema = 1\nid = {}\ntitle = {}\ndescription = {}\n\n[[loc]]\nkey = {}\nop = {}\n",
        q(id),
        q(req.title.trim()),
        q(req.description.trim()),
        q(req.key.trim()),
        q(op)
    );
    if let Some(f) = &req.find {
        t.push_str(&format!("find = {}\n", q(f)));
    }
    t.push_str(&format!("value = {}\n", q(&req.value)));
    t
}

// ---------------------------------------------------------------------------------------
// Doctor and diagnostics
// ---------------------------------------------------------------------------------------

fn chk(level: CheckLevel, label: impl Into<String>, detail: Option<String>) -> DoctorCheck {
    DoctorCheck {
        level,
        label: label.into(),
        detail,
    }
}

/// What `starorg-strings doctor` checks, as sections. Counts only for personal data.
pub fn doctor(b: &RealBackend) -> DoctorReport {
    let dirs = b.dirs();
    let mut app = vec![chk(
        CheckLevel::Info,
        "App folders",
        Some(format!(
            "config {}, data {}, cache {}",
            dirs.config.display(),
            dirs.data.display(),
            dirs.cache.display()
        )),
    )];
    if fsutil::is_elevated() {
        app.push(chk(
            CheckLevel::Error,
            "Running as root or elevated: apply refuses to run",
            None,
        ));
    }
    let found = b.installs();
    let mut installs = Vec::new();
    let mut paths = Vec::new();
    let mut tools = Vec::new();
    if found.is_empty() {
        installs.push(chk(
            CheckLevel::Warn,
            "No Star Citizen install found",
            Some("Choose the folder in the setup wizard.".into()),
        ));
    }
    for f in &found {
        installs.push(chk(
            CheckLevel::Ok,
            format!("Install found by {}", f.label),
            Some(path_text(&f.install.root)),
        ));
        for ch in &f.install.channels {
            let t = Target::new(&f.install, f.id.clone(), ch);
            let s = b.summary(f, ch);
            let state = match &s.status {
                ApplyState::Applied => "applied".to_string(),
                ApplyState::Stale { reasons } => format!("stale ({})", reasons.join(", ")),
                ApplyState::ChangedByAnotherTool { tool } => {
                    format!("changed by {}", tool.as_deref().unwrap_or("another tool"))
                }
                ApplyState::NotApplied => "not applied".into(),
            };
            installs.push(chk(
                if s.build_error.is_some() {
                    CheckLevel::Warn
                } else {
                    CheckLevel::Info
                },
                format!(
                    "{}: {}, {}",
                    ch.name,
                    if s.set_up { "set up" } else { "not set up" },
                    state
                ),
                s.build_error.clone().or_else(|| s.build.version.clone()),
            ));
            if let Ok(ini) = apply::locate_ini(&ch.dir) {
                let dir = ini.real.parent().map(|p| p.to_path_buf());
                let mut d = dir.clone();
                while let Some(x) = d.clone() {
                    if x.is_dir() {
                        break;
                    }
                    d = x.parent().map(|p| p.to_path_buf());
                }
                let writable = d
                    .as_deref()
                    .and_then(|d| std::fs::metadata(d).ok())
                    .is_some_and(|m| !m.permissions().readonly());
                paths.push(chk(
                    if writable {
                        CheckLevel::Ok
                    } else {
                        CheckLevel::Error
                    },
                    format!(
                        "{}/Data/Localization/english is {}",
                        ch.name,
                        if writable { "writable" } else { "not writable" }
                    ),
                    ini.is_link
                        .then(|| format!("global.ini -> {}", ini.real.display())),
                ));
            }
            if let Ok(c) = usercfg::locate(&ch.dir)
                && c.is_link
            {
                paths.push(chk(
                    CheckLevel::Info,
                    format!("{}/USER.cfg is a link", ch.name),
                    Some(format!("-> {}", c.real.display())),
                ));
            }
            if let Some(tool) = &s.foreign_tool {
                tools.push(chk(
                    CheckLevel::Warn,
                    format!("{}: loose global.ini written by {tool}", ch.name),
                    None,
                ));
            }
            if matches!(resolve_ci(&ch.dir, &[".starstrings_version"]), Ok(r) if r.exists()) {
                tools.push(chk(
                    CheckLevel::Warn,
                    format!("{}: .starstrings_version file", ch.name),
                    None,
                ));
            }
            if let Ok(r) = resolve_ci(&ch.dir, &["Data", "languages.ini"])
                && r.exists()
                && std::fs::metadata(&r.path).is_ok_and(|m| m.len() == 0)
            {
                tools.push(chk(
                    CheckLevel::Warn,
                    format!("{}: empty Data/languages.ini", ch.name),
                    Some("Use the launcher's Verify files to restore it.".into()),
                ));
            }
            if f.id.is_some() && t.data_dir(dirs).join(INTENT_JSON).exists() {
                installs.push(chk(
                    CheckLevel::Warn,
                    format!(
                        "{}: an interrupted apply is waiting to be finished",
                        ch.name
                    ),
                    None,
                ));
            }
            if f.id.is_some() {
                let data = t.data_dir(dirs);
                let n = backup::list(&data).len();
                let chars = blueprints::load_characters(&data)
                    .map(|c| c.characters.len())
                    .unwrap_or(0);
                installs.push(chk(
                    CheckLevel::Info,
                    format!("{}: {n} backups, {chars} characters", ch.name),
                    None,
                ));
            }
        }
    }
    let set = patch::load_patches(dirs, &[]);
    let mut patches = vec![chk(
        CheckLevel::Info,
        format!("{} patches loaded", set.patches.len()),
        None,
    )];
    for e in &set.errors {
        patches.push(chk(
            CheckLevel::Warn,
            format!("Patch {} skipped", e.name),
            Some(e.error.clone()),
        ));
    }
    let hooks = b.hooks_view();
    let mut hook_checks = vec![chk(
        CheckLevel::Info,
        if hooks.cli_on_path {
            "starorg-strings is on PATH"
        } else {
            "starorg-strings is not on PATH (the launch hooks call it)"
        },
        None,
    )];
    for s in &hooks.scripts {
        hook_checks.push(chk(
            match s.status {
                HookState::Current => CheckLevel::Ok,
                HookState::Broken { .. } => CheckLevel::Error,
                HookState::Outdated => CheckLevel::Warn,
                _ => CheckLevel::Info,
            },
            format!("{}: {}", kebab(&s.kind), kebab_state(&s.status)),
            s.path.clone(),
        ));
    }
    DoctorReport {
        sections: vec![
            DoctorSection {
                title: "App".into(),
                checks: app,
            },
            DoctorSection {
                title: "Installs".into(),
                checks: installs,
            },
            DoctorSection {
                title: "Paths".into(),
                checks: paths,
            },
            DoctorSection {
                title: "Other tools".into(),
                checks: if tools.is_empty() {
                    vec![chk(CheckLevel::Ok, "No traces of other tools", None)]
                } else {
                    tools
                },
            },
            DoctorSection {
                title: "Patches".into(),
                checks: patches,
            },
            DoctorSection {
                title: "Launch hooks".into(),
                checks: hook_checks,
            },
        ],
    }
}

fn kebab_state(s: &HookState) -> String {
    match s {
        HookState::Absent => "not installed".into(),
        HookState::Current => "installed".into(),
        HookState::Outdated => "outdated".into(),
        HookState::Broken { why } => format!("broken ({why})"),
        HookState::Missing => "no script".into(),
    }
}

/// Personal values to replace, longest first: home folder, user name, character names and
/// geids of every channel (addendum §3).
fn secrets(b: &RealBackend) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for var in ["HOME", "USERPROFILE"] {
        if let Some(h) = std::env::var_os(var).filter(|v| !v.is_empty()) {
            let h = h.to_string_lossy().into_owned();
            if h.len() > 1 {
                out.push((h, "~".into()));
            }
        }
    }
    for var in ["USER", "USERNAME", "LOGNAME"] {
        if let Ok(u) = std::env::var(var)
            && u.len() >= 3
        {
            out.push((u, "<user>".into()));
        }
    }
    for f in b.installs() {
        if f.id.is_none() {
            continue;
        }
        for ch in &f.install.channels {
            let data = b.data_dir(&f, ch);
            if let Ok(c) = blueprints::load_characters(&data) {
                for (geid, c) in c.characters {
                    out.push((geid, "<geid>".into()));
                    if c.name.len() >= 2 {
                        out.push((c.name, "<character>".into()));
                    }
                    if let Some(a) = c.account_id {
                        out.push((a, "<account>".into()));
                    }
                }
            }
        }
    }
    // StarOrg Tools account cards: the user name and the orgs (named in sync log lines).
    for acc in b.app_file().starorg.values() {
        if let Some(u) = acc.user.as_ref().filter(|u| u.len() >= 2) {
            out.push((u.clone(), "<starorg-user>".into()));
        }
        for g in &acc.guilds {
            out.push((g.id.clone(), "<org-id>".into()));
            if g.name.len() >= 2 {
                out.push((g.name.clone(), "<org>".into()));
            }
        }
    }
    out.sort_by(|a, b| (b.0.len(), &b.0).cmp(&(a.0.len(), &a.0)));
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

pub fn redact(b: &RealBackend, text: &str) -> (String, u32) {
    let mut out = text.to_string();
    let mut n = 0u32;
    for (from, to) in secrets(b) {
        let c = out.matches(from.as_str()).count() as u32;
        if c > 0 {
            out = out.replace(from.as_str(), &to);
            n += c;
        }
    }
    (out, n)
}

pub fn diagnostics(b: &RealBackend) -> Diagnostics {
    let mut text = format!(
        "StarOrg Codex {} diagnostics ({})\n\n",
        b.ctx().app_version,
        std::env::consts::OS
    );
    for sec in doctor(b).sections {
        text.push_str(&format!("## {}\n", sec.title));
        for c in sec.checks {
            text.push_str(&format!("[{}] {}", kebab(&c.level), c.label));
            if let Some(d) = c.detail {
                text.push_str(&format!(" ({d})"));
            }
            text.push('\n');
        }
        text.push('\n');
    }
    text.push_str("## Recent log\n");
    for e in b.mem().log.iter().rev().take(50) {
        text.push_str(&format!("{} {} {}\n", e.time, kebab(&e.level), e.message));
    }
    let (text, redacted) = redact(b, &text);
    let stamp = fsutil::stamp(fsutil::unix_now());
    Diagnostics {
        file_name: format!("starorg-strings-diagnostics-{stamp}.txt"),
        text,
        redacted,
    }
}
