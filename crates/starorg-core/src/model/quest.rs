//! Quest items: items a mission or contract hands out or asks for, with the missions that
//! reference them (addendum §12).
//!
//! Only real record references count: a `MissionBrokerEntry` spawn description that puts
//! the item in an NPC's or a container's inventory (`autoSpawnSettings.inventoryItems`), and
//! a contract hauling order (`haulingOrderContent[].entityClass`). Loc keys that merely look
//! alike are never used to link an item to a mission. Local extraction only.

use super::LocKeys;
use serde::{Deserialize, Serialize};

/// How a mission references the item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QuestRef {
    /// Spawned into an inventory by a `MissionBrokerEntry`.
    Spawned,
    /// Asked for (or delivered) by a contract hauling order.
    Hauled,
}

/// One mission or contract that references a quest item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestUse {
    pub kind: QuestRef,
    /// Contract generator short name, or `MissionBrokerEntry`.
    pub source: String,
    /// Contract debug name, or the `MissionBrokerEntry` record short name.
    pub mission: String,
    /// Title loc key (with `@`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_key: Option<String>,
    /// Star systems the mission is offered in (`Stanton`, `Pyro`, `Nyx`), from its locality
    /// or location; empty when the record names none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub systems: Vec<String>,
    /// `notForRelease` or `workInProgress`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unreleased: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestItem {
    /// `EntityClassDefinition.<name>`.
    pub record_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(default, skip_serializing_if = "LocKeys::is_empty")]
    pub loc: LocKeys,
    /// `AttachDef.Type`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_type: Option<String>,
    /// `AttachDef.SubType` (`UNDEFINED` is left out).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_type: Option<String>,
    /// Other entity records in the same folder whose description key is this item's
    /// (short names). A hint in a shared key would show on every one of them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub desc_shared_with: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uses: Vec<QuestUse>,
}

impl QuestItem {
    pub fn short_name(&self) -> &str {
        super::short_name(&self.record_name)
    }
}

/// The star system a locality or starmap record path names: the file stem's last dotted
/// part, `starmapobject.stanton1` -> `Stanton`, `pu_missionlocality/pyro` -> `Pyro`.
/// `None` for anything else (a station, a planet without the system prefix).
pub fn system_of(path: &str) -> Option<&'static str> {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let stem = file.strip_suffix(".json").unwrap_or(file);
    let last = stem.rsplit('.').next().unwrap_or(stem).to_ascii_lowercase();
    SYSTEMS
        .iter()
        .find(|(prefix, _)| last.starts_with(prefix))
        .map(|(_, name)| *name)
}

/// Locality/starmap file-name prefixes and the system names the game shows.
const SYSTEMS: &[(&str, &str)] = &[("stanton", "Stanton"), ("pyro", "Pyro"), ("nyx", "Nyx")];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn systems_from_paths() {
        assert_eq!(
            system_of(
                "libs/foundry/records/starmap/pu/system/stanton/stanton1/starmapobject.stanton1.json"
            ),
            Some("Stanton")
        );
        assert_eq!(
            system_of("missiondata/pu_missionlocality/pyro.json"),
            Some("Pyro")
        );
        assert_eq!(
            system_of("pu_missionlocality/nyx_glaciem.json"),
            Some("Nyx")
        );
        assert_eq!(system_of("starmapobject.stantonstar.json"), Some("Stanton"));
        assert_eq!(system_of("locations/area18.json"), None);
        assert_eq!(system_of(""), None);
    }
}
