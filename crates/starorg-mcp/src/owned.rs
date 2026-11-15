//! Owned blueprints from the app's store (addendum §3): which blueprints a character owns.
//!
//! Read only, from the channel's data folder (`blueprints/<geid>.json`). Character names are
//! personal data: they are shown to the local client that asked, never sent anywhere; geids
//! are not shown at all.

use crate::store::{ChannelSource, ToolError, ToolResult};
use starorg_core::blueprints::{self, normalize};
use starorg_core::state::ChannelState;
use starorg_gamedata::query::obtain::BlueprintLike;
use std::collections::BTreeSet;

/// The owned set of one character, `any` character, or the channel's active one.
#[derive(Debug, Clone, Default)]
pub struct OwnedView {
    /// Display names of the characters whose sets were used.
    pub characters: Vec<String>,
    names: BTreeSet<String>,
    ids: BTreeSet<String>,
    pub count: usize,
}

impl OwnedView {
    /// Owned when the blueprint's produced item name, or its record id, is in the set.
    pub fn owns(&self, b: &BlueprintLike) -> bool {
        let name = b.display_name.as_deref().unwrap_or(&b.name);
        self.names.contains(&normalize(name))
            || b.id
                .as_deref()
                .is_some_and(|i| self.ids.contains(&i.to_ascii_lowercase()))
    }
}

/// `who`: a character name (or geid), `active` (the channel's setting, default the newest
/// session's character) or `any`.
pub fn select(ch: &ChannelSource, who: &str) -> ToolResult<OwnedView> {
    let Some(dir) = &ch.data_dir else {
        return Err(ToolError(format!(
            "no owned blueprints for {}: the channel is not set up in StarOrg Strings",
            ch.name
        )));
    };
    let choice = match who.trim() {
        "" | "active" => ChannelState::load(dir)
            .ok()
            .and_then(|s| s.active_character),
        other => Some(other.to_string()),
    };
    let sel = blueprints::select(dir, choice.as_deref()).map_err(ToolError)?;
    let chars = blueprints::load_characters(dir).map_err(ToolError)?;
    let characters = sel
        .characters
        .iter()
        .map(|g| {
            chars
                .characters
                .get(g)
                .map_or_else(|| "unknown character".to_string(), |c| c.name.clone())
        })
        .collect();
    Ok(OwnedView {
        characters,
        count: sel.count,
        names: sel.names,
        ids: sel.ids,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use starorg_core::blueprints::{Characters, OwnedSet, Source};

    #[test]
    fn marks_blueprints_a_character_owns() {
        let dir = tempfile::tempdir().unwrap();
        let mut chars = Characters::default();
        chars.seen("111", None, "Pilot_A", Some("2026-11-01T00:00:00Z"));
        chars.seen("222", None, "Pilot_B", Some("2026-11-10T00:00:00Z"));
        blueprints::save_characters(dir.path(), &chars).unwrap();
        let mut set = OwnedSet::new(Some("111"));
        set.add(
            "FS-9 LMG",
            Source {
                kind: "log".into(),
                time: None,
                file: None,
            },
        );
        blueprints::save_set(dir.path(), &set).unwrap();
        let ch = ChannelSource {
            name: "LIVE".into(),
            p4k: dir.path().join("Data.p4k"),
            build: None,
            dir: dir.path().into(),
            data_dir: Some(dir.path().into()),
        };
        let bp = |name: &str| BlueprintLike {
            name: format!("BP_{name}"),
            display_name: Some(name.into()),
            ..Default::default()
        };
        let a = select(&ch, "Pilot_A").unwrap();
        assert_eq!(a.characters, ["Pilot_A"]);
        assert!(a.owns(&bp("FS-9 LMG")));
        assert!(!a.owns(&bp("Arrowhead Sniper Rifle")));
        // `active` with no setting = the newest session's character (Pilot_B, owns nothing).
        let active = select(&ch, "active").unwrap();
        assert_eq!(active.characters, ["Pilot_B"]);
        assert!(!active.owns(&bp("FS-9 LMG")));
        assert!(select(&ch, "any").unwrap().owns(&bp("FS-9 LMG")));
        assert!(select(&ch, "Nobody").is_err());
        let unset = ChannelSource {
            data_dir: None,
            ..ch
        };
        assert!(select(&unset, "any").is_err());
    }
}
