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
    /// EM tag number for section headers (3 or 4).
    pub header_em: u8,
    /// EM tag number for detail labels (3 or 4).
    pub label_em: u8,
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
    /// `[enhance]` table that overrides fields of the preset.
    pub fn from_toml(text: &str) -> Result<Self, ProfileError> {
        let table: toml::Table =
            toml::from_str(text).map_err(|e| ProfileError::Toml(e.to_string()))?;
        let preset = table
            .get("preset")
            .and_then(|v| v.as_str())
            .unwrap_or("recommended");
        let base = Self::preset(preset)?;
        let Some(over) = table.get("enhance") else {
            return Ok(base);
        };
        let mut merged =
            toml::Value::try_from(&base).map_err(|e| ProfileError::Toml(e.to_string()))?;
        merge(&mut merged, over);
        let p: Self = merged
            .try_into()
            .map_err(|e: toml::de::Error| ProfileError::Toml(e.to_string()))?;
        p.validate()?;
        Ok(p)
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
        for n in [self.mission.header_em, self.mission.label_em] {
            if !(3..=4).contains(&n) {
                return Err(ProfileError::Invalid(format!(
                    "EM tag {n}: only 3 and 4 are emitted"
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
    fn bad_values_are_refused() {
        assert!(Profile::from_toml("[enhance.tags.missiles]\nseparator = \"%\"\n").is_err());
        assert!(Profile::from_toml("[enhance]\nheader = \"<EM4>x\"\n").is_err());
        assert!(Profile::from_toml("[enhance.mission]\nheader_em = 1\n").is_err());
        assert!(Profile::from_toml("[enhance]\nno_such = 1\n").is_err());
        assert!(Profile::from_toml("preset = \"x\"").is_err());
    }
}
