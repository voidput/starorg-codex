//! Contracts, blueprints, blueprint pools and reputation standings (spec 6.1).
//!
//! Contracts come from contract-generator records and are filled by local extraction only:
//! the published dumps carry legacy `MissionBrokerEntry` missions and resolved text, not
//! the generator contracts with their loc keys.

use serde::{Deserialize, Serialize};

/// Where a contract's title or description key came from (spec 6.1, Q14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeySource {
    /// `contract.paramOverrides.stringParamOverrides`.
    Contract,
    /// `generator.contractParams.stringParamOverrides` (shared by all its contracts).
    Generator,
    /// The contract template's `contractDisplayInfo.displayString`: `[0]` title,
    /// `[2]` description (found in M2a on LIVE 4.10: the Hockrow facility-delve phase
    /// contracts carry no override and take their keys from here).
    Template,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintReward {
    /// Blueprint pool record short name.
    pub pool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chance: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemReward {
    /// Entity record short name.
    pub item: String,
    /// Name key of the awarded item, when the record has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_key: Option<String>,
    pub amount: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReputationReward {
    /// Faction reputation record short name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faction: Option<String>,
    /// That record's `displayName` loc reference (schema 6), when it has a real one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faction_name_key: Option<String>,
    pub amount: f64,
    /// Granted on success (`missionResults[0]`) or on failure (`missionResults[1]`).
    #[serde(default)]
    pub on_success: bool,
    #[serde(default)]
    pub on_failure: bool,
}

/// What a spawn group is to the player, from its mission variable and group name (and the
/// `AI/Spawning/Identifier/...` tag its options apply).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpawnRole {
    /// The ship the contract is about: a salvage wreck, a bounty, a ship to board or steal.
    Target,
    /// Ships that fight the player: waves, defenders, reinforcements, interdictors.
    Hostile,
    /// Ships the player protects, refuels or flies with.
    Escort,
    /// A ship that carries the cargo to recover.
    Cargo,
    /// Anything the names do not tell.
    #[default]
    Other,
}

/// One ship option of a spawn slot (`SpawnDescription_Ship`).
///
/// An option's `tags` are a tag query mixed with state the game applies to the spawned ship.
/// Filter: the tag, or a tag below it, must be on the ship entity. Applied: spawn flags, cargo and crew
/// manifests, pilot skill and spawn identifiers, which no ship entity carries (Sept 2026
/// data: none of 1,101 vehicles carries a `CargoManifest`, `CrewManifest`,
/// `SkillDefinitions` or `Spawning/Identifier` tag, and the only `Ship/SpawnFlags` tags on an
/// entity are three on one boarding Cutlass, while `PoweredOff`, which salvage options ask
/// for, is on none).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpawnOption {
    /// Filter tag paths (`Missions/MissionType/Salvage/AvailableToSalvage/Smallest`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selector: Vec<String>,
    /// `negativeTags`: ships with any of these (or a tag below one) are left out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
    /// Tag ids (selector or negative) the tag database does not know: they match nothing
    /// and are never shown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown: Vec<String>,
    /// Applied tag paths (`AI/CargoManifest/General/Value/HighValue`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applied: Vec<String>,
    /// `concurrentAmount`.
    pub amount: f64,
    #[serde(default)]
    pub weight: f64,
    /// `initialDamageSettings` record short name (`salvagecontractor_lightdamage`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage: Option<String>,
    /// Vehicle entity classes (lower case) that carry every selector tag (or a tag below
    /// it) and no excluded one, sorted; only the AI/unmanned/salvage versions when those
    /// match alongside player hulls; never test or template records. Empty when none does
    /// (the game picks from data this build lacks).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ships: Vec<String>,
}

/// A slot of a spawn group (`SpawnDescription_ShipOptions`): the game spawns one of its
/// options, picked by weight.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpawnSlot {
    #[serde(default)]
    pub options: Vec<SpawnOption>,
}

/// A ship group a contract spawns (`SpawnDescription_ShipGroup`, schema 7).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpawnGroup {
    /// `MissionProperty.missionVariableName` (`SalvageSpawnDescription_BP`).
    pub variable: String,
    /// The group's `Name` (`SalvageableShip`, `Wave 1`), trimmed; may be empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default)]
    pub role: SpawnRole,
    #[serde(default)]
    pub slots: Vec<SpawnSlot>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    /// Contract generator record short name.
    pub generator: String,
    /// The handler (`generators[]`) debug name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub handler: String,
    pub debug_name: String,
    #[serde(default)]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_from: Option<KeySource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_from: Option<KeySource>,
    /// `OnceOnly` bool override, or the handler's `defaultAvailability.onceOnly`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub once_only: bool,
    /// Has a completed-contract prerequisite (spec 6.4 "Chain").
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub chain: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub not_for_release: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub work_in_progress: bool,
    /// Difficulty profile record short name (variant label source, spec 6.4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blueprint_rewards: Vec<BlueprintReward>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub item_rewards: Vec<ItemReward>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reputation: Vec<ReputationReward>,
    /// Lowest required standing (`ContractPrerequisite_Reputation.minStanding`), record
    /// short name of an `SReputationStandingParams`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_standing: Option<String>,
    /// Star systems of its `ContractPrerequisite_Locality` entries (`Stanton`, `Pyro`),
    /// sorted (schema 6); empty = no locality prerequisite (offered anywhere, or not known).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub systems: Vec<String>,
    /// Ship groups the contract spawns (schema 7): its `propertyOverrides`, else its
    /// template's `contractProperties`, of type `MissionPropertyValue_ShipSpawnDescriptions`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spawns: Vec<SpawnGroup>,
}

impl Contract {
    /// First positive reputation reward granted on success (spec 6.1).
    pub fn reputation_gain(&self) -> Option<f64> {
        self.reputation
            .iter()
            .find(|r| r.on_success && r.amount > 0.0)
            .map(|r| r.amount)
    }

    /// Largest reputation loss on failure, as a negative number.
    pub fn failure_penalty(&self) -> Option<f64> {
        self.reputation
            .iter()
            .filter(|r| r.on_failure && r.amount < 0.0)
            .map(|r| r.amount)
            .reduce(f64::min)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintTier {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub craft_time_seconds: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Blueprint {
    pub record_name: String,
    #[serde(default)]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    /// Blueprint category record short name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Produced entity record short name (`processSpecificData.entityClass`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub produces: Option<String>,
    /// Name key of the produced item (local only). The record's own `blueprintName` is a
    /// placeholder on almost every record and is not used (spec 6.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub produces_name_key: Option<String>,
    /// Records without tiers (dismantle) are not kept.
    #[serde(default)]
    pub tiers: Vec<BlueprintTier>,
}

impl Blueprint {
    pub fn short_name(&self) -> &str {
        super::short_name(&self.record_name)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintPoolEntry {
    /// Blueprint record short name.
    pub blueprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintPool {
    /// Pool record short name.
    pub name: String,
    #[serde(default)]
    pub blueprints: Vec<BlueprintPoolEntry>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReputationStanding {
    /// Record short name.
    pub name: String,
    pub record_name: String,
    #[serde(default)]
    pub id: String,
    /// `displayName` loc reference (local only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_reputation: Option<f64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reputation_helpers() {
        let c = Contract {
            reputation: vec![
                ReputationReward {
                    faction: None,
                    faction_name_key: None,
                    amount: -1000.0,
                    on_success: false,
                    on_failure: true,
                },
                ReputationReward {
                    faction: None,
                    faction_name_key: None,
                    amount: 150.0,
                    on_success: true,
                    on_failure: false,
                },
            ],
            ..Contract::default()
        };
        assert_eq!(c.reputation_gain(), Some(150.0));
        assert_eq!(c.failure_penalty(), Some(-1000.0));
    }

    #[test]
    fn dump_blueprint_and_pool_deserialise() {
        let b: Blueprint = serde_json::from_str(
            r#"{"name":"X","recordName":"CraftingBlueprintRecord.BP_CRAFT_x","id":"i","category":"FPSArmours",
               "produces":"x_item","displayName":"X","tiers":[{"craftTimeSeconds":90,"costs":[]}],"rewardedBy":[]}"#,
        )
        .unwrap();
        assert_eq!(b.short_name(), "BP_CRAFT_x");
        assert_eq!(b.tiers.len(), 1);
        let p: BlueprintPool = serde_json::from_str(
            r#"{"name":"BP_MISSIONREWARD_A","blueprints":[{"blueprint":"BP_CRAFT_x","weight":1}]}"#,
        )
        .unwrap();
        assert_eq!(p.blueprints[0].weight, Some(1.0));
    }
}
