//! Contracts, blueprints, blueprint pools and reputation standings (spec 6.1).

use super::json::{array, at, num, ref_path, referenced_name, text, type_of};
use super::spawns::{ShipTags, TagDb, contract_spawns};
use serde_json::Value;
use starorg_core::model::quest::system_of;
use starorg_core::model::{
    Blueprint, BlueprintPool, BlueprintPoolEntry, BlueprintReward, BlueprintTier, Contract,
    ItemReward, KeySource, QuestRef, QuestUse, ReputationReward, ReputationStanding,
};

/// Lookups the mission parsers need, by normalised record path.
pub trait MissionLookup {
    /// Short name of the record at `path`.
    fn short_name(&self, path: &str) -> Option<String>;
    /// `AttachDef.Localization.Name` of the entity at `path`.
    fn entity_name_key(&self, path: &str) -> Option<String>;
    /// `reputationAmount` of the `SReputationRewardAmount` at `path`.
    fn reputation_amount(&self, path: &str) -> Option<f64>;
    /// `contractDisplayInfo.displayString` of the `ContractTemplate` at `path`.
    fn template_strings(&self, path: &str) -> Vec<String>;
    /// `displayName` loc reference of the faction reputation record at `path`, when it has
    /// a real one.
    fn faction_name_key(&self, _path: &str) -> Option<String> {
        None
    }
    /// `contractProperties` of the `ContractTemplate` at `path` (its default mission
    /// properties).
    fn template_properties(&self, _path: &str) -> Vec<Value> {
        Vec::new()
    }
    /// The tag database and the vehicles by tag, for contract spawns; none without a
    /// tag database (contracts then carry no spawns).
    fn spawn_tags(&self) -> Option<(&TagDb, &ShipTags)> {
        None
    }
    /// A data oddity worth a warning (extraction report).
    fn note(&self, _detail: String) {}
}

/// Index of the title and description in a template's `displayString`.
const TEMPLATE_TITLE: usize = 0;
const TEMPLATE_DESCRIPTION: usize = 2;

fn string_param<'v>(overrides: Option<&'v Value>, param: &str) -> Option<&'v str> {
    array(overrides.and_then(|o| o.get("stringParamOverrides")))
        .iter()
        .find(|p| text(p.get("param")) == Some(param))
        .and_then(|p| text(p.get("value")))
}

fn bool_param(overrides: Option<&Value>, param: &str) -> Option<bool> {
    array(overrides.and_then(|o| o.get("boolParamOverrides")))
        .iter()
        .find(|p| text(p.get("param")) == Some(param))
        .and_then(|p| p.get("value")?.as_bool())
}

/// `propertyOverrides` of a `ContractParamOverrides`.
fn overrides(params: Option<&Value>) -> &[Value] {
    array(params.and_then(|o| o.get("propertyOverrides")))
}

/// Visit every object below `v` (depth first, `v` included).
fn walk<'v>(v: &'v Value, f: &mut impl FnMut(&'v Value)) {
    match v {
        Value::Object(o) => {
            f(v);
            for x in o.values() {
                walk(x, f);
            }
        }
        Value::Array(a) => {
            for x in a {
                walk(x, f);
            }
        }
        _ => {}
    }
}

fn mission_results(v: &Value) -> (bool, bool) {
    let r = array(v.get("missionResults"));
    let at_i = |i: usize| r.get(i).and_then(Value::as_bool).unwrap_or(false);
    (at_i(0), at_i(1))
}

/// An item a mission references: the entity record path and the use.
pub type QuestLink = (String, QuestUse);

/// Entity paths of every `entityClass` below a `haulingOrderContent` array of `v`.
fn hauled_items(v: &Value) -> Vec<String> {
    let mut out = Vec::new();
    walk(v, &mut |node| {
        for order in array(node.get("haulingOrderContent")) {
            walk(order, &mut |o| {
                if let Some(p) = ref_path(o.get("entityClass"))
                    && !out.contains(&p)
                {
                    out.push(p);
                }
            });
        }
    });
    out
}

/// Systems of the `ContractPrerequisite_Locality` entries among `prereqs`, sorted.
fn locality_systems<'v>(prereqs: impl Iterator<Item = &'v Value>) -> Vec<String> {
    let mut out: Vec<String> = prereqs
        .filter(|p| type_of(p) == Some("ContractPrerequisite_Locality"))
        .filter_map(|p| ref_path(p.get("localityAvailable")))
        .filter_map(|path| system_of(&path).map(str::to_string))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// All contracts of one `ContractGenerator` record.
pub fn parse_contracts(generator: &str, v: &Value, lk: &dyn MissionLookup) -> Vec<Contract> {
    parse_contracts_with_items(generator, v, lk).0
}

/// [`parse_contracts`], plus the items each contract's hauling orders ask for (addendum §13).
pub fn parse_contracts_with_items(
    generator: &str,
    v: &Value,
    lk: &dyn MissionLookup,
) -> (Vec<Contract>, Vec<QuestLink>) {
    let gen_short = starorg_core::model::short_name(generator).to_string();
    let mut out = Vec::new();
    let mut links = Vec::new();
    for g in array(v.get("generators")) {
        let handler = text(g.get("debugName")).unwrap_or_default().to_string();
        let gen_params = g.get("contractParams");
        let availability = g.get("defaultAvailability");
        let default_once = availability
            .and_then(|a| a.get("onceOnly"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let gen_prereqs = array(availability.and_then(|a| a.get("prerequisites")));
        for c in array(g.get("contracts")) {
            let Some(debug_name) = text(c.get("debugName")) else {
                continue;
            };
            let params = c.get("paramOverrides");
            let template_path = ref_path(c.get("template"));
            let template = template_path
                .as_deref()
                .map(|p| lk.template_strings(p))
                .unwrap_or_default();
            let pick_key = |param: &str, slot: usize| match string_param(params, param) {
                Some(k) => (Some(k.to_string()), Some(KeySource::Contract)),
                None => match string_param(gen_params, param) {
                    Some(k) => (Some(k.to_string()), Some(KeySource::Generator)),
                    None => match template
                        .get(slot)
                        .filter(|k| starorg_core::model::loc_key(k).is_some())
                    {
                        Some(k) => (Some(k.clone()), Some(KeySource::Template)),
                        None => (None, None),
                    },
                },
            };
            let (title_key, title_from) = pick_key("Title", TEMPLATE_TITLE);
            let (description_key, description_from) = pick_key("Description", TEMPLATE_DESCRIPTION);
            let prereqs = gen_prereqs
                .iter()
                .chain(array(c.get("additionalPrerequisites")));
            let systems = locality_systems(prereqs.clone());
            let mut chain = false;
            let mut min_standing = None;
            for p in prereqs {
                match type_of(p) {
                    Some("ContractPrerequisite_CompletedContractTags") => chain = true,
                    Some("ContractPrerequisite_Reputation") if min_standing.is_none() => {
                        min_standing =
                            ref_path(p.get("minStanding")).and_then(|path| lk.short_name(&path));
                    }
                    _ => {}
                }
            }
            let mut contract = Contract {
                generator: gen_short.clone(),
                handler: handler.clone(),
                debug_name: debug_name.to_string(),
                id: text(c.get("id")).unwrap_or_default().to_string(),
                title_key,
                title_from,
                description_key,
                description_from,
                once_only: bool_param(params, "OnceOnly").unwrap_or(default_once),
                chain,
                not_for_release: c.get("notForRelease") == Some(&Value::Bool(true)),
                work_in_progress: c.get("workInProgress") == Some(&Value::Bool(true)),
                difficulty: referenced_name(at(c, "contractResults.difficulty.difficultyProfile"))
                    .map(str::to_string),
                min_standing,
                systems: systems.clone(),
                ..Contract::default()
            };
            if let Some((tags, ships)) = lk.spawn_tags() {
                let template_props = template_path
                    .as_deref()
                    .map(|p| lk.template_properties(p))
                    .unwrap_or_default();
                let (spawns, twice) = contract_spawns(
                    &[overrides(params), overrides(gen_params), &template_props],
                    tags,
                    ships,
                );
                contract.spawns = spawns;
                for var in twice {
                    lk.note(format!(
                        "{}: spawn variable {var} set twice in one layer; the first is kept",
                        contract.debug_name
                    ));
                }
            }
            if let Some(results) = c.get("contractResults") {
                walk(results, &mut |node| match type_of(node) {
                    Some("BlueprintRewards") => {
                        if let Some(pool) =
                            ref_path(node.get("blueprintPool")).and_then(|p| lk.short_name(&p))
                        {
                            contract.blueprint_rewards.push(BlueprintReward {
                                pool,
                                chance: num(node.get("chance")),
                            });
                        }
                    }
                    Some(t @ ("ContractResult_Item" | "ItemAwardEntityClass")) => {
                        let amount_field = if t == "ContractResult_Item" {
                            "amount"
                        } else {
                            "amountToAward"
                        };
                        if let Some(path) = ref_path(node.get("entityClass")) {
                            let item = lk
                                .short_name(&path)
                                .or_else(|| path.rsplit('/').next().map(str::to_string))
                                .unwrap_or_default();
                            contract.item_rewards.push(ItemReward {
                                item,
                                name_key: lk.entity_name_key(&path),
                                amount: num(node.get(amount_field)).unwrap_or(1.0),
                            });
                        }
                    }
                    Some("ContractResult_LegacyReputation") => {
                        let amounts = node.get("contractResultReputationAmounts");
                        let reward = amounts.and_then(|a| ref_path(a.get("reward")));
                        if let Some(amount) = reward.and_then(|p| lk.reputation_amount(&p)) {
                            let (on_success, on_failure) = mission_results(node);
                            let faction = amounts.and_then(|a| a.get("factionReputation"));
                            contract.reputation.push(ReputationReward {
                                faction: referenced_name(faction).map(str::to_string),
                                faction_name_key: ref_path(faction)
                                    .and_then(|p| lk.faction_name_key(&p)),
                                amount,
                                on_success,
                                on_failure,
                            });
                        }
                    }
                    _ => {}
                });
            }
            for path in hauled_items(c) {
                links.push((
                    path,
                    QuestUse {
                        kind: QuestRef::Hauled,
                        source: gen_short.clone(),
                        mission: contract.debug_name.clone(),
                        title_key: contract.title_key.clone(),
                        systems: systems.clone(),
                        unreleased: contract.not_for_release || contract.work_in_progress,
                    },
                ));
            }
            out.push(contract);
        }
    }
    (out, links)
}

/// Items a `MissionBrokerEntry` spawns into inventories (`autoSpawnSettings.inventoryItems`),
/// with the mission as their use (addendum §13).
pub fn parse_mission_entry(name: &str, v: &Value) -> Vec<QuestLink> {
    let mut paths: Vec<String> = Vec::new();
    for prop in array(v.get("properties")) {
        walk(prop, &mut |node| {
            if let Some(auto) = node.get("autoSpawnSettings") {
                for item in array(auto.get("inventoryItems")) {
                    if let Some(p) = ref_path(item.get("entityClass"))
                        && !paths.contains(&p)
                    {
                        paths.push(p);
                    }
                }
            }
        });
    }
    if paths.is_empty() {
        return Vec::new();
    }
    let mut systems: Vec<String> = ["locationMissionAvailable", "localityAvailable"]
        .iter()
        .filter_map(|f| ref_path(v.get(*f)))
        .filter_map(|p| system_of(&p).map(str::to_string))
        .collect();
    systems.sort();
    systems.dedup();
    let used = QuestUse {
        kind: QuestRef::Spawned,
        source: "MissionBrokerEntry".into(),
        mission: starorg_core::model::short_name(name).to_string(),
        title_key: text(v.get("title"))
            .filter(|k| starorg_core::model::loc_key(k).is_some())
            .map(str::to_string),
        systems,
        unreleased: v.get("notForRelease") == Some(&Value::Bool(true)),
    };
    paths.into_iter().map(|p| (p, used.clone())).collect()
}

/// A crafting blueprint; `None` for records without tiers (dismantle).
pub fn parse_blueprint(
    name: &str,
    id: &str,
    path: &str,
    v: &Value,
    lk: &dyn MissionLookup,
) -> Option<Blueprint> {
    let bp = v.get("blueprint")?;
    let tiers: Vec<BlueprintTier> = array(bp.get("tiers"))
        .iter()
        .filter_map(|t| {
            let costs = at(t, "recipe.costs")?;
            let time = costs.get("craftTime").map(|ct| {
                ["days", "hours", "minutes", "seconds"]
                    .iter()
                    .zip([86400.0, 3600.0, 60.0, 1.0])
                    .map(|(k, f)| num(ct.get(*k)).unwrap_or(0.0) * f)
                    .sum::<f64>()
            });
            Some(BlueprintTier {
                craft_time_seconds: time
                    .filter(|t| *t > 0.0)
                    .map(|t| (t * 100.0).round() / 100.0),
            })
        })
        .collect();
    if tiers.is_empty() {
        return None;
    }
    let product = ref_path(at(bp, "processSpecificData.entityClass"));
    Some(Blueprint {
        record_name: name.to_string(),
        id: id.to_string(),
        path: path.to_string(),
        category: bp
            .get("category")
            .and_then(|c| text(c.get("_RecordName_")))
            .map(|n| starorg_core::model::short_name(n).to_string())
            .or_else(|| referenced_name(bp.get("category")).map(str::to_string)),
        produces: product
            .as_deref()
            .and_then(|p| lk.short_name(p))
            .or_else(|| {
                referenced_name(at(bp, "processSpecificData.entityClass")).map(str::to_string)
            }),
        produces_name_key: product.as_deref().and_then(|p| lk.entity_name_key(p)),
        tiers,
    })
}

pub fn parse_pool(name: &str, v: &Value, lk: &dyn MissionLookup) -> BlueprintPool {
    BlueprintPool {
        name: starorg_core::model::short_name(name).to_string(),
        blueprints: array(v.get("blueprintRewards"))
            .iter()
            .filter_map(|e| {
                let path = ref_path(e.get("blueprintRecord"))?;
                Some(BlueprintPoolEntry {
                    blueprint: lk
                        .short_name(&path)
                        .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(&path).to_string()),
                    weight: num(e.get("weight")),
                })
            })
            .collect(),
    }
}

pub fn parse_standing(name: &str, id: &str, v: &Value) -> ReputationStanding {
    ReputationStanding {
        name: starorg_core::model::short_name(name).to_string(),
        record_name: name.to_string(),
        id: id.to_string(),
        display_name_key: text(v.get("displayName")).map(str::to_string),
        min_reputation: num(v.get("minReputation")),
        gated: v.get("gated") == Some(&Value::Bool(true)),
    }
}
