//! Enhancement settings: the `[enhance]` part of `profile.toml` (spec 4.7) and the presets.
//!
//! A profile names a preset and may override any field below it in an `[enhance]` table.
//! Unknown keys are ignored here (M1's profile store keeps them on write).

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Presets shipped (spec 4.7).
pub const PRESETS: &[&str] = &["recommended", "minimal", "everything"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Profile {
    pub ships: bool,
    pub components: bool,
    pub ship_weapons: bool,
    pub fps_weapons: bool,
    pub missiles: bool,
    pub missions: bool,
    /// Stat lines switched off, as `<category>.<line>` (e.g. `ships.boost`, `components.signatures`).
    pub lines_off: BTreeSet<String>,
    /// "Stats above prose" (spec 5.1).
    pub stats_above: bool,
    /// Stat block header. Plain ASCII: EM tags are confirmed only in mobiGlas text.
    pub header: String,
    pub tags: Tags,
    pub mission: Mission,
    /// Spec 5.7, off by default.
    pub branding: bool,
    /// Colour test (developer aid, off by default): one sample line `EM1 … EM5`, each in its
    /// own tag, after [`COLOUR_TEST_KEY`] so the in-game look of every tag can be checked.
    /// Not in any preset; the next apply without it puts the stock text back.
    pub colour_test: bool,
}

/// The one string the colour test writes to: the Contracts Manager's "Please select a
/// contract." prompt. It shows every time the mobiGlas Contracts Manager opens with nothing
/// selected, sits in the pane where contract text (and its EM markup) is drawn, and is a
/// prompt nobody needs to read, so a test line there harms nothing.
pub const COLOUR_TEST_KEY: &str = "mG_ContractsMgr_Select";

/// EM tag numbers a colour setting may name; 0 means no tag. EM3 (underlined) and EM4 (the
/// blue highlight) are what stock uses in mobiGlas text; 1, 2 and 5 exist in stock but their
/// look is not verified in game yet.
pub const EM_MAX: u8 = 5;

/// `text` in tag `n`, or plain when `n` is 0.
pub fn em(n: u8, text: &str) -> String {
    if n == 0 {
        text.to_string()
    } else {
        format!("<EM{n}>{text}</EM{n}>")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tags {
    pub components: TagStyle,
    pub ship_weapons: TagStyle,
    pub missiles: TagStyle,
    /// Also write the tag to the short-name key when the record has one (Q10; off until
    /// the in-game check of M2 decides).
    pub short_names: bool,
    /// Tractor beams, salvage heads, mining lasers and EMPs get a size-only tag (spec 5.4).
    pub tool_size_tag: bool,
}

/// Tag builder settings for one tag kind (spec 5.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TagStyle {
    pub enabled: bool,
    /// Element names in order. Components: `class`, `size`, `grade`. Ship weapons:
    /// `damage`, `size`. Missiles: `tracking`, `size`.
    pub elements: Vec<String>,
    /// Style per element: `short`, `medium` or `long`; elements not listed are `medium`.
    /// Read profiles hold no `medium` entries (an explicit one, needed to undo a preset's
    /// `short`, is dropped on read), so equal settings compare equal.
    pub styles: BTreeMap<String, String>,
    /// `-` (default), `` (none), ` `, `_`, `.`, `/`, `|`.
    pub separator: String,
    /// `[]` (default), `()`, `{}`, `<>`, `` (none).
    pub enclosing: String,
    /// `prefix` (default) or `suffix`.
    pub placement: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Mission {
    /// `[BP]` / `[BP?]` title tags.
    pub title_blueprint: bool,
    /// `[N Rep]` title tags.
    pub title_reputation: bool,
    /// Wrap title tags in `<EM4>` (the M2 in-game check decides the default).
    pub title_em: bool,
    pub blueprints: bool,
    pub item_rewards: bool,
    pub details: bool,
    /// Name tags on blueprint bullets.
    pub bullet_tags: bool,
    /// `[Owned]` on bullets of blueprints the active character owns.
    pub owned_marker: bool,
    /// Lists longer than this are cut with `- and N more`.
    pub max_list: usize,
    /// EM tag for section headers (0 = none, 1-5; stock uses 3).
    pub header_em: u8,
    /// EM tag for detail labels (0 = none, 1-5).
    pub label_em: u8,
    /// EM tag of the blueprint title tag (`[BP]`, `[BP?]`, `[BP Owned]`) when `title_em`
    /// is on (0 = none, 1-5).
    pub title_bp_em: u8,
    /// EM tag of the `[N Rep]` title tag when `title_em` is on (0 = none, 1-5).
    pub title_rep_em: u8,
    /// `[BP Owned]` in place of `[BP]`/`[BP?]` when the active character owns every
    /// blueprint the contract can award.
    pub title_owned: bool,
    /// EM tag around `[Owned]` on owned bullets (0 = none, 1-5).
    pub owned_em: u8,
    /// Colour the whole owned bullet (tag, name and `[Owned]`) instead of only the marker.
    pub owned_line: bool,
    /// `[Missing]` on bullets the active character does not own; only when an owned set
    /// is known (an empty set would mark every line).
    pub missing_marker: bool,
    /// EM tag around `[Missing]` (0 = none, 1-5).
    pub missing_em: u8,
    /// EM tag of the variant sub-headers in a blueprint list (`[Easy]`, `[Variant 2]`).
    pub variant_em: u8,
    /// EM tag around item names in the ITEM REWARDS list (0 = none, 1-5).
    pub reward_em: u8,
}

impl Default for TagStyle {
    fn default() -> Self {
        Self {
            enabled: true,
            elements: Vec::new(),
            styles: BTreeMap::new(),
            separator: "-".into(),
            enclosing: "[]".into(),
            placement: "prefix".into(),
        }
    }
}

impl TagStyle {
    fn with(elements: &[&str]) -> Self {
        Self {
            elements: elements.iter().map(|s| s.to_string()).collect(),
            ..Self::default()
        }
    }

    pub fn style_of(&self, element: &str) -> &str {
        self.styles
            .get(element)
            .map(String::as_str)
            .unwrap_or("medium")
    }
}

impl Default for Tags {
    fn default() -> Self {
        Self {
            components: TagStyle::with(&["class", "size", "grade"]),
            ship_weapons: TagStyle {
                styles: BTreeMap::from([("damage".to_string(), "short".to_string())]),
                ..TagStyle::with(&["damage", "size"])
            },
            missiles: TagStyle::with(&["tracking", "size"]),
            short_names: false,
            tool_size_tag: false,
        }
    }
}

impl Default for Mission {
    fn default() -> Self {
        Self {
            title_blueprint: true,
            title_reputation: true,
            title_em: true,
            blueprints: true,
            item_rewards: true,
            details: true,
            bullet_tags: true,
            owned_marker: true,
            max_list: 25,
            header_em: 3,
            label_em: 4,
            title_bp_em: 4,
            title_rep_em: 4,
            title_owned: false,
            owned_em: 4,
            owned_line: false,
            missing_marker: false,
            missing_em: 0,
            variant_em: 4,
            reward_em: 0,
        }
    }
}

impl Default for Profile {
    /// The Recommended preset.
    fn default() -> Self {
        Self {
            ships: true,
            components: true,
            ship_weapons: true,
            fps_weapons: true,
            missiles: true,
            missions: true,
            lines_off: BTreeSet::new(),
            stats_above: false,
            header: "--- STATS ---".into(),
            tags: Tags::default(),
            mission: Mission::default(),
            branding: false,
            colour_test: false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("unknown preset {0:?} (have: recommended, minimal, everything)")]
    UnknownPreset(String),
    #[error("profile: {0}")]
    Toml(String),
    #[error("profile: {0}")]
    Invalid(String),
}

impl Profile {
    /// Nothing generated (the stock file).
    pub fn off() -> Self {
        let mut p = Self::minimal();
        for t in [
            &mut p.tags.components,
            &mut p.tags.ship_weapons,
            &mut p.tags.missiles,
        ] {
            t.enabled = false;
        }
        p
    }

    /// Name tags (and, in M1, patches) only.
    pub fn minimal() -> Self {
        Self {
            ships: false,
            components: false,
            ship_weapons: false,
            fps_weapons: false,
            missiles: false,
            missions: false,
            ..Self::default()
        }
    }

    pub fn everything() -> Self {
        let mut p = Self::default();
        p.tags.short_names = true;
        p.tags.tool_size_tag = true;
        p.branding = true;
        p
    }

    pub fn preset(name: &str) -> Result<Self, ProfileError> {
        match name {
            "recommended" => Ok(Self::default()),
            "minimal" => Ok(Self::minimal()),
            "everything" => Ok(Self::everything()),
            other => Err(ProfileError::UnknownPreset(other.into())),
        }
    }

    /// Read a `profile.toml`: `preset = "..."` (default recommended) plus an optional
    /// `[enhance]` table that overrides fields of the preset. Keys this build does not know
    /// are ignored (see [`Self::from_toml_checked`]).
    pub fn from_toml(text: &str) -> Result<Self, ProfileError> {
        Self::from_toml_checked(text).map(|(p, _)| p)
    }

    /// [`Self::from_toml`], plus the `[enhance]` keys this build does not know (as
    /// `enhance.mission.some_key`). They are ignored, not refused, so a profile saved by a
    /// newer build still reads here; callers report them as warnings. Known keys are
    /// still checked: a wrong type or value is an error.
    pub fn from_toml_checked(text: &str) -> Result<(Self, Vec<String>), ProfileError> {
        let table: toml::Table =
            toml::from_str(text).map_err(|e| ProfileError::Toml(e.to_string()))?;
        let preset = table
            .get("preset")
            .and_then(|v| v.as_str())
            .unwrap_or("recommended");
        let base = Self::preset(preset)?;
        let Some(over) = table.get("enhance") else {
            return Ok((base, Vec::new()));
        };
        let mut merged =
            toml::Value::try_from(&base).map_err(|e| ProfileError::Toml(e.to_string()))?;
        let mut unknown = Vec::new();
        let over = known_only(&merged, over, "enhance", &mut unknown);
        merge(&mut merged, &over);
        let p: Self = merged
            .try_into()
            .map_err(|e: toml::de::Error| ProfileError::Toml(e.to_string()))?;
        p.validate()?;
        let mut p = p;
        p.normalize();
        Ok((p, unknown))
    }

    /// Drop `medium` tag styles: an element not listed is `medium` already.
    fn normalize(&mut self) {
        for t in [
            &mut self.tags.components,
            &mut self.tags.ship_weapons,
            &mut self.tags.missiles,
        ] {
            t.styles.retain(|_, s| s != "medium");
        }
    }

    fn validate(&self) -> Result<(), ProfileError> {
        for t in [
            &self.tags.components,
            &self.tags.ship_weapons,
            &self.tags.missiles,
        ] {
            if let Some(bad) = t
                .styles
                .values()
                .find(|s| !["short", "medium", "long"].contains(&s.as_str()))
            {
                return Err(ProfileError::Invalid(format!("tag style {bad:?}")));
            }
            if !["", "-", " ", "_", ".", "/", "|"].contains(&t.separator.as_str()) {
                return Err(ProfileError::Invalid(format!(
                    "tag separator {:?}",
                    t.separator
                )));
            }
            if !["", "[]", "()", "{}", "<>"].contains(&t.enclosing.as_str()) {
                return Err(ProfileError::Invalid(format!(
                    "tag enclosing {:?}",
                    t.enclosing
                )));
            }
            if !["prefix", "suffix"].contains(&t.placement.as_str()) {
                return Err(ProfileError::Invalid(format!(
                    "tag placement {:?}",
                    t.placement
                )));
            }
        }
        let m = &self.mission;
        for n in [
            m.header_em,
            m.label_em,
            m.title_bp_em,
            m.title_rep_em,
            m.owned_em,
            m.missing_em,
            m.variant_em,
            m.reward_em,
        ] {
            if n > EM_MAX {
                return Err(ProfileError::Invalid(format!(
                    "EM tag {n}: use 0 (none) or 1 to {EM_MAX}"
                )));
            }
        }
        if self.header.contains(['%', '<', '~']) || self.header.contains("\\n") {
            return Err(ProfileError::Invalid("header must be plain text".into()));
        }
        Ok(())
    }

    /// Is a stat line on?
    pub fn line_on(&self, category: &str, line: &str) -> bool {
        !self.lines_off.contains(&format!("{category}.{line}"))
    }

    /// Hash for reports and the applied manifest, stable across builds: only the settings
    /// that differ from the Recommended defaults count ([`crate::canon::settings_hash`]), so
    /// a field a later build adds with its default leaves it unchanged. A changed default
    /// changes the output for the same profile: bump [`crate::OUTPUT_VERSION`] for that.
    pub fn hash(&self) -> String {
        crate::canon::settings_hash(self)
    }
}

impl Profile {
    /// The `[enhance]` table that turns `base` (the preset) into `self`: only the fields
    /// that differ, so a default a later build changes reaches every setting left alone.
    /// Lists are written whole when they differ; a `styles` map holds each element whose
    /// style differs, with an explicit `medium` where `base` has another style and `self`
    /// none (reading cannot remove a key the preset has). Reading it back over `base`
    /// ([`Self::from_toml`]) gives `self` (callers still check the round trip).
    pub fn sparse_over(&self, base: &Profile) -> Result<toml::Table, ProfileError> {
        let cur = toml::Value::try_from(self).map_err(|e| ProfileError::Toml(e.to_string()))?;
        let def = toml::Value::try_from(base).map_err(|e| ProfileError::Toml(e.to_string()))?;
        Ok(match diff(&cur, &def, "") {
            Some(toml::Value::Table(t)) => t,
            _ => toml::Table::new(),
        })
    }
}

/// A `styles` entry as read: an element not listed is `medium`.
fn style_at(t: &toml::Table, element: &str) -> toml::Value {
    t.get(element)
        .cloned()
        .unwrap_or_else(|| toml::Value::String("medium".into()))
}

/// `cur` minus what equals `def`; `None` when nothing differs.
fn diff(cur: &toml::Value, def: &toml::Value, path: &str) -> Option<toml::Value> {
    match (cur, def) {
        (toml::Value::Table(c), toml::Value::Table(d)) if path.ends_with(".styles") => {
            let mut out = toml::Table::new();
            for k in c.keys().chain(d.keys()) {
                let v = style_at(c, k);
                if v != style_at(d, k) {
                    out.insert(k.clone(), v);
                }
            }
            (!out.is_empty()).then_some(toml::Value::Table(out))
        }
        (toml::Value::Table(c), toml::Value::Table(d)) if !path.ends_with(".styles") => {
            let mut out = toml::Table::new();
            for (k, v) in c {
                let p = format!("{path}.{k}");
                let changed = match d.get(k) {
                    Some(dv) => diff(v, dv, &p),
                    None => Some(v.clone()),
                };
                if let Some(x) = changed {
                    out.insert(k.clone(), x);
                }
            }
            (!out.is_empty()).then_some(toml::Value::Table(out))
        }
        _ => (cur != def).then(|| cur.clone()),
    }
}

/// `over` without the keys `base` (a serialised profile) has no field for, which are
/// listed in `unknown`. Tables whose keys are data, not fields (`styles`), pass whole.
fn known_only(
    base: &toml::Value,
    over: &toml::Value,
    path: &str,
    unknown: &mut Vec<String>,
) -> toml::Value {
    match (base, over) {
        (toml::Value::Table(b), toml::Value::Table(o)) if !path.ends_with(".styles") => {
            let mut out = toml::Table::new();
            for (k, v) in o {
                let p = format!("{path}.{k}");
                match b.get(k) {
                    Some(bv) => {
                        out.insert(k.clone(), known_only(bv, v, &p, unknown));
                    }
                    None => unknown.push(p),
                }
            }
            toml::Value::Table(out)
        }
        _ => over.clone(),
    }
}

fn merge(base: &mut toml::Value, over: &toml::Value) {
    match (base, over) {
        (toml::Value::Table(b), toml::Value::Table(o)) => {
            for (k, v) in o {
                match b.get_mut(k) {
                    Some(slot) => merge(slot, v),
                    None => {
                        b.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (slot, v) => *slot = v.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets() {
        let r = Profile::preset("recommended").unwrap();
        assert!(r.ships && r.missions && r.tags.components.enabled && !r.branding);
        let m = Profile::preset("minimal").unwrap();
        assert!(!m.ships && !m.missions && m.tags.missiles.enabled);
        let e = Profile::preset("everything").unwrap();
        assert!(e.branding && e.tags.short_names);
        assert!(Profile::preset("nope").is_err());
        assert_ne!(r.hash(), m.hash());
    }

    #[test]
    fn hash_counts_only_settings_off_the_defaults() {
        let d = Profile::default();
        // Recommended = no differences = the hash of `{}`.
        assert_eq!(d.hash(), crate::fsutil::sha256_hex(b"{}"));
        // A later build adding a field with its default: same JSON plus that key, on both
        // sides of the diff, leaves the hash as it was.
        let mut cur = serde_json::to_value(Profile::minimal()).unwrap();
        let mut def = serde_json::to_value(&d).unwrap();
        let before = crate::canon::diff_hash(&cur, &def);
        assert_eq!(before, Profile::minimal().hash());
        for v in [&mut cur, &mut def] {
            v["mission"]["new_toggle"] = serde_json::json!(true);
            v["new_section"] = serde_json::json!({"on": false});
        }
        assert_eq!(crate::canon::diff_hash(&cur, &def), before);
        // Any user change shows.
        let mut p = Profile::default();
        p.mission.max_list = 10;
        assert_ne!(p.hash(), d.hash());
        let mut q = Profile::default();
        q.tags
            .components
            .styles
            .insert("size".into(), "long".into());
        assert_ne!(q.hash(), d.hash());
        assert_ne!(q.hash(), p.hash());
        // `[enhance]` setting a value equal to the default is no change.
        assert_eq!(
            Profile::from_toml("[enhance]\nships = true\n")
                .unwrap()
                .hash(),
            d.hash()
        );
    }

    #[test]
    fn toml_overrides_preset_fields() {
        let p = Profile::from_toml(
            "preset = \"minimal\"\nfuture = 1\n[patches]\ndisabled = []\n[enhance]\nmissions = true\nlines_off = [\"ships.boost\"]\n[enhance.tags.components]\nenclosing = \"()\"\n[enhance.mission]\nmax_list = 10\n",
        )
        .unwrap();
        assert!(p.missions && !p.ships);
        assert_eq!(p.tags.components.enclosing, "()");
        assert_eq!(p.tags.components.elements, vec!["class", "size", "grade"]);
        assert_eq!(p.mission.max_list, 10);
        assert!(!p.line_on("ships", "boost"));
        assert!(p.line_on("ships", "size"));
        assert_eq!(Profile::from_toml("").unwrap(), Profile::default());
    }

    #[test]
    fn colour_settings_read_and_default_to_known_styles() {
        let d = Profile::default();
        let m = &d.mission;
        // Defaults name only EM3/EM4 or no tag: the styles stock uses in mobiGlas.
        for n in [
            m.header_em,
            m.label_em,
            m.title_bp_em,
            m.title_rep_em,
            m.owned_em,
            m.missing_em,
            m.variant_em,
            m.reward_em,
        ] {
            assert!([0, 3, 4].contains(&n), "{n}");
        }
        assert!(!m.title_owned && !m.owned_line && !m.missing_marker && !d.colour_test);
        for preset in PRESETS {
            assert!(!Profile::preset(preset).unwrap().colour_test, "{preset}");
        }
        let p = Profile::from_toml(
            "[enhance]\ncolour_test = true\n[enhance.mission]\nowned_em = 2\nheader_em = 0\nmissing_marker = true\nmissing_em = 5\n",
        )
        .unwrap();
        assert!(p.colour_test && p.mission.missing_marker);
        assert_eq!(
            (
                p.mission.owned_em,
                p.mission.header_em,
                p.mission.missing_em
            ),
            (2, 0, 5)
        );
        assert_ne!(p.hash(), d.hash());
        assert_eq!(em(0, "x"), "x");
        assert_eq!(em(5, "x"), "<EM5>x</EM5>");
    }

    #[test]
    fn unknown_keys_are_ignored_and_listed() {
        // A profile saved by a newer build: its new keys are skipped, the rest applies.
        let (p, unknown) = Profile::from_toml_checked(
            "[enhance]\nno_such = 1\nships = false\n[enhance.mission]\nnew_em = 2\nowned_em = 4\n[enhance.tags.components]\nnew_flag = true\n[enhance.tags.components.styles]\nsize = \"long\"\n[enhance.future]\nx = 1\n",
        )
        .unwrap();
        assert!(!p.ships && p.mission.owned_em == 4);
        assert_eq!(p.tags.components.style_of("size"), "long");
        assert_eq!(
            unknown,
            vec![
                "enhance.future",
                "enhance.mission.new_em",
                "enhance.no_such",
                "enhance.tags.components.new_flag"
            ]
        );
        assert_eq!(
            Profile::from_toml("[enhance]\nno_such = 1\n").unwrap(),
            Profile::default()
        );
    }

    #[test]
    fn sparse_over_holds_only_differences() {
        let d = Profile::default();
        assert!(d.sparse_over(&d).unwrap().is_empty());
        let mut p = Profile {
            branding: true,
            ..Profile::default()
        };
        p.mission.owned_em = 0;
        p.mission.title_owned = true;
        p.tags
            .ship_weapons
            .styles
            .insert("size".into(), "long".into());
        p.lines_off.insert("ships.boost".into());
        let over = p.sparse_over(&d).unwrap();
        let text = toml::to_string(&toml::Table::from_iter([(
            "enhance".to_string(),
            toml::Value::Table(over.clone()),
        )]))
        .unwrap();
        assert_eq!(
            over.keys().collect::<Vec<_>>(),
            ["branding", "lines_off", "mission", "tags"]
        );
        let m = over["mission"].as_table().unwrap();
        assert_eq!(m.keys().collect::<Vec<_>>(), ["owned_em", "title_owned"]);
        // A styles map holds only the elements whose style differs.
        let styles = over["tags"]["ship_weapons"]["styles"].as_table().unwrap();
        assert_eq!(styles.keys().collect::<Vec<_>>(), ["size"]);
        assert_eq!(Profile::from_toml(&text).unwrap(), p);
        // Against another preset: only what differs from that preset.
        let min = Profile::minimal();
        let mut q = Profile::minimal();
        q.missions = true;
        let over = q.sparse_over(&min).unwrap();
        assert_eq!(over.keys().collect::<Vec<_>>(), ["missions"]);
    }

    /// Every style of every element of every tag kind, over every preset, reads back as
    /// set: including `medium` where the preset has another style (ship-weapon `damage`).
    #[test]
    fn every_tag_style_round_trips_over_every_preset() {
        type Get = fn(&mut Tags) -> &mut TagStyle;
        let groups: [(&str, Get); 3] = [
            ("components", |t| &mut t.components),
            ("ship_weapons", |t| &mut t.ship_weapons),
            ("missiles", |t| &mut t.missiles),
        ];
        for preset in PRESETS {
            let base = Profile::preset(preset).unwrap();
            for (name, get) in groups {
                let elements = get(&mut base.tags.clone()).elements.clone();
                for el in &elements {
                    for style in ["short", "medium", "long"] {
                        let mut p = base.clone();
                        let t = get(&mut p.tags);
                        t.styles.insert(el.clone(), style.into());
                        p.normalize();
                        let over = p.sparse_over(&base).unwrap();
                        let text = format!(
                            "preset = {preset:?}\n{}",
                            toml::to_string(&toml::Table::from_iter([(
                                "enhance".to_string(),
                                toml::Value::Table(over),
                            )]))
                            .unwrap()
                        );
                        let back = Profile::from_toml(&text).unwrap();
                        assert_eq!(back, p, "{preset} {name}.{el} = {style}\n{text}");
                        assert_eq!(get(&mut back.tags.clone()).style_of(el), style);
                    }
                }
            }
        }
    }

    #[test]
    fn bad_values_are_refused() {
        assert!(Profile::from_toml("[enhance.tags.missiles]\nseparator = \"%\"\n").is_err());
        assert!(Profile::from_toml("[enhance]\nheader = \"<EM4>x\"\n").is_err());
        assert!(Profile::from_toml("[enhance.mission]\nheader_em = 6\n").is_err());
        assert!(Profile::from_toml("[enhance.mission]\nowned_em = 9\n").is_err());
        assert!(Profile::from_toml("[enhance.mission]\nreward_em = -1\n").is_err());
        // Wrong types of known keys are still errors.
        assert!(Profile::from_toml("[enhance.mission]\nowned_em = \"blue\"\n").is_err());
        assert!(Profile::from_toml("[enhance]\nmission = 3\n").is_err());
        assert!(Profile::from_toml("preset = \"x\"").is_err());
    }
}
