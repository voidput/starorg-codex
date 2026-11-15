//! Commodities: the resource types the economy trades in, the cargo items that hold them,
//! and what happens when volatile cargo is knocked about (port of
//! `sc-gamedata/src/domains/commodities.ts`).
//!
//! Every tradable substance is a `ResourceType` inside one record, the
//! `ResourceTypeDatabase`, nested in groups (`Metal > UnrefinedOres`). A raw ore names what
//! it refines into through `refinedVersion`, which carries the whole refined resource inline
//! rather than a reference.
//!
//! The cargo items under `entities/commodities` are thin wrappers: a display name, a type
//! label, and a `ResourceContainer` whose default composition is one resource type.
//!
//! `CommodityDamageConfiguration` covers mined cargo taking impact damage and volatile
//! cargo exploding; only gameplay numbers are kept.
//!
//! Quality runs 0-1000. A resource's `ResourceTypeCraftingData` points at three records:
//! the distribution its quality is rolled from, location overrides of that curve, and the
//! quantization bands that snap a rolled value to what the game shows.

use crate::collate::compare_names;
use crate::domain::{Domain, DomainError, ParseContext, field};
use crate::helpers_world::{js_trim, last_segment, literal_newlines, strip_tags};
use crate::localization::{Localization, is_placeholder_name, is_unresolved, localize};
use crate::records::{GameRecord, RecordSet, array, at, num, ref_path, ref_short_name, str_};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::HashSet;

pub static DOMAIN: Commodities = Commodities;

pub struct Commodities;

/// `localizeKey`: localize, retrying with the `,P` suffix some keys carry only in that form.
fn localize_key(localization: &Localization, value: Option<&Value>) -> Option<String> {
    let text = localize(localization, value).map(str::to_string);
    let Some(raw) = value.and_then(Value::as_str) else {
        return text;
    };
    if !is_unresolved(text.as_deref()) {
        return text;
    }
    let suffixed_key = Value::String(format!("{raw},P"));
    let suffixed = localize(localization, Some(&suffixed_key)).map(str::to_string);
    if is_unresolved(suffixed.as_deref()) {
        text
    } else {
        suffixed
    }
}

/// `displayNameOf`: a localized name, `None` for placeholders and unresolved keys, UI
/// markup stripped (carbon dioxide is `CO<font size="%d">2</font>` in the table).
pub fn display_name_of(localization: &Localization, value: Option<&Value>) -> Option<String> {
    let name = localize_key(localization, value).map(|t| js_trim(&strip_tags(&t)).to_string());
    if is_placeholder_name(name.as_deref()) {
        None
    } else {
        name
    }
}

/// `/^\S+ description\.?$/i`: the "Tritium description." stub.
fn is_description_stub(text: &str) -> bool {
    let body = text.strip_suffix('.').unwrap_or(text);
    let check = |b: &str| {
        const TAIL: &str = " description";
        if b.len() <= TAIL.len() || !b.is_char_boundary(b.len() - TAIL.len()) {
            return false;
        }
        let (head, tail) = b.split_at(b.len() - TAIL.len());
        tail.eq_ignore_ascii_case(TAIL)
            && !head.is_empty()
            && !head.chars().any(crate::helpers_world::is_js_space)
    };
    // `\.?` is optional, so the unstripped text may match too ("x description").
    check(body) || check(text)
}

/// `descriptionOf`: markup stripped, the table's literal `\n` turned into line breaks,
/// placeholders and "<Name> description." stubs dropped.
pub fn description_of(localization: &Localization, value: Option<&Value>) -> Option<String> {
    let text = localize_key(localization, value)
        .map(|t| js_trim(&literal_newlines(&strip_tags(&t))).to_string());
    if is_placeholder_name(text.as_deref()) || is_description_stub(text.as_deref().unwrap_or("")) {
        None
    } else {
        text
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Volatility {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volatility: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_decay_per_second: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceType {
    pub name: String,
    pub record_name: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Top-level group, e.g. "Metal".
    pub group: String,
    /// Nested group path below the top level, e.g. ["UnrefinedOres"].
    pub subgroups: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub density_grams_per_cc: Option<f64>,
    /// The resource this refines into, for raw ores.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refines_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volatility: Option<Volatility>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<ResourceQuality>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QualityDistribution {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mean: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stddev: Option<f64>,
}

/// `{ location, ...distributionOf(...) }`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LocationOverride {
    pub location: String,
    #[serde(flatten)]
    pub distribution: Option<QualityDistribution>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QualityBand {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceQuality {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distribution: Option<QualityDistribution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_overrides: Option<Vec<LocationOverride>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bands: Option<Vec<QualityBand>>,
}

fn distribution_of(value: Option<&Value>) -> Option<QualityDistribution> {
    let v = value.filter(|v| v.is_object() || v.is_array())?;
    let out = QualityDistribution {
        min: num(v.get("min")),
        max: num(v.get("max")),
        mean: num(v.get("mean")),
        stddev: num(v.get("stddev")),
    };
    [out.min, out.max, out.mean, out.stddev]
        .iter()
        .any(Option::is_some)
        .then_some(out)
}

/// `name.replace(/(ShipMineable)?_QualityDistribution.*$/, "")`.
fn quality_tier(name: &str) -> String {
    const LONG: &str = "ShipMineable_QualityDistribution";
    const SHORT: &str = "_QualityDistribution";
    for (i, _) in name.char_indices() {
        let rest = &name[i..];
        if rest.starts_with(LONG) || rest.starts_with(SHORT) {
            return name[..i].to_string();
        }
    }
    name.to_string()
}

fn is_object_like(v: &&Value) -> bool {
    v.is_object() || v.is_array()
}

/// `parseQuality`: resolve a resource's crafting quality references.
pub fn parse_quality(
    props: &[Value],
    records: &RecordSet,
    localization: &Localization,
) -> Option<ResourceQuality> {
    let crafting = props
        .iter()
        .find(|p| p.get("_Type_").and_then(Value::as_str) == Some("ResourceTypeCraftingData"))?;
    let lookup = |reference: Option<&Value>| -> Option<&GameRecord> {
        records.by_record_path(&ref_path(reference)?)
    };

    let distribution = lookup(at(crafting, "qualityDistribution.qualityDistributionRecord"));
    let overrides = lookup(at(crafting, "qualityLocationOverride.locationOverrideRecord"));
    let quantization = lookup(at(crafting, "qualityQuantization.qualityQuantizationRecord"));

    let override_list = array(overrides.and_then(|o| at(&o.value, "locationOverride.locationOverrideList")));
    // Sixty Nyx clusters share one display name and curve, so identical entries collapse.
    let mut seen = HashSet::new();
    let mut location_overrides = Vec::new();
    for e in override_list.iter().filter(is_object_like) {
        let place = lookup(e.get("location"));
        let location = display_name_of(localization, place.and_then(|p| p.value.get("name")))
            .or_else(|| place.map(|p| p.short_name.clone()))
            .or_else(|| ref_path(e.get("location")).map(|p| last_segment(&p).to_string()))
            .unwrap_or_else(|| "?".to_string());
        let entry = LocationOverride {
            location,
            distribution: distribution_of(e.get("qualityDistribution")),
        };
        let key = serde_json::to_string(&entry).unwrap_or_default();
        if seen.insert(key) {
            location_overrides.push(entry);
        }
    }

    let band_list = array(quantization.and_then(|q| at(&q.value, "qualityQuantization.bands")));
    let bands: Vec<QualityBand> = band_list
        .iter()
        .filter(is_object_like)
        .map(|b| QualityBand {
            start: num(b.get("start")),
            end: num(b.get("end")),
            value: num(b.get("mappedValue")),
        })
        .collect();

    let quality = ResourceQuality {
        profile: distribution.map(|d| d.short_name.clone()),
        tier: distribution.map(|d| quality_tier(&d.short_name)),
        distribution: distribution_of(distribution.and_then(|d| at(&d.value, "qualityDistribution"))),
        location_overrides: (!location_overrides.is_empty()).then_some(location_overrides),
        bands: (!bands.is_empty()).then_some(bands),
    };
    (quality.profile.as_deref().is_some_and(|p| !p.is_empty()) || quality.bands.is_some())
        .then_some(quality)
}

fn parse_resource(
    raw: &Value,
    groups: &[String],
    localization: &Localization,
    records: &RecordSet,
) -> Option<ResourceType> {
    let record_name = str_(raw.get("_RecordName_"))?.to_string();
    let props = array(raw.get("properties"));
    let volatile = props
        .iter()
        .find(|p| p.get("_Type_").and_then(Value::as_str) == Some("ResourceTypeVolatility"));
    Some(ResourceType {
        name: ref_short_name(Some(raw))
            .map(str::to_string)
            .unwrap_or_else(|| record_name.clone()),
        id: str_(raw.get("_RecordId_")).unwrap_or_default().to_string(),
        record_name,
        display_name: display_name_of(localization, raw.get("displayName")),
        description: description_of(localization, raw.get("description")),
        group: groups.first().cloned().unwrap_or_default(),
        subgroups: groups.iter().skip(1).cloned().collect(),
        density_grams_per_cc: num(at(raw, "densityType.densityUnit.gramsPerCubicCentimeter")),
        refines_to: ref_short_name(raw.get("refinedVersion")).map(str::to_string),
        volatility: volatile.map(|v| Volatility {
            volatility: num(v.get("volatility")),
            health_decay_per_second: num(v.get("healthDecayPerSecond")),
        }),
        quality: parse_quality(props, records, localization),
    })
}

/// `parseResourceDatabase`: walk the group tree. Groups and resources both contain nulls.
pub fn parse_resource_database(
    record: &GameRecord,
    localization: &Localization,
    records: &RecordSet,
) -> Vec<ResourceType> {
    fn walk(
        group: &Value,
        path: &[String],
        localization: &Localization,
        records: &RecordSet,
        found: &mut Vec<ResourceType>,
    ) {
        let mut here = path.to_vec();
        here.push(ref_short_name(Some(group)).unwrap_or("?").to_string());
        for raw in array(group.get("resources")) {
            if !crate::helpers_world::truthy(Some(raw)) {
                continue;
            }
            if let Some(r) = parse_resource(raw, &here, localization, records) {
                found.push(r);
            }
        }
        for child in array(group.get("groups")) {
            if crate::helpers_world::truthy(Some(child)) {
                walk(child, &here, localization, records, found);
            }
        }
    }
    let mut found = Vec::new();
    for group in array(record.value.get("groups")) {
        if crate::helpers_world::truthy(Some(group)) {
            walk(group, &[], localization, records, &mut found);
        }
    }
    found
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommodityItem {
    pub name: String,
    pub record_name: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// The game's type label, e.g. "Gas", "Metal".
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub ty: Option<String>,
    /// Folder under entities/commodities, e.g. "halogens".
    pub category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    /// Capacity of one item in SCU.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity_scu: Option<f64>,
    /// The resource's description, added after the fact (`{ ...item, description }`), so
    /// it comes last.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// `scuOf`: SCU from the game's unit wrappers, centi (1/100) and micro (1/1,000,000).
pub fn scu_of(capacity: Option<&Value>) -> Option<f64> {
    let c = capacity?;
    if let Some(centi) = num(at(c, "centiSCU")) {
        return Some(centi / 100.0);
    }
    if let Some(micro) = num(at(c, "microSCU")) {
        return Some(micro / 1_000_000.0);
    }
    num(at(c, "standardCargoUnits"))
}

fn component_of<'a>(components: &'a [Value], ty: &str) -> Option<&'a Value> {
    components
        .iter()
        .find(|c| c.get("_Type_").and_then(Value::as_str) == Some(ty))
}

pub fn parse_commodity_item(record: &GameRecord, localization: &Localization) -> CommodityItem {
    let components = array(record.value.get("Components"));
    let purchasable = component_of(components, "SCItemPurchasableParams");
    let container = component_of(components, "ResourceContainer");
    let composition = array(container.and_then(|c| c.get("defaultComposition")));
    let segments: Vec<&str> = record.record_path.split('/').collect();
    let folder = segments.iter().position(|s| *s == "commodities");
    CommodityItem {
        name: record.short_name.clone(),
        record_name: record.name.clone(),
        id: record.id.clone(),
        display_name: display_name_of(localization, purchasable.and_then(|p| p.get("displayName"))),
        ty: display_name_of(localization, purchasable.and_then(|p| p.get("displayType"))),
        category: match folder {
            Some(f) if segments.len() > f + 2 => segments[f + 1].to_string(),
            _ => String::new(),
        },
        resource: ref_short_name(composition.first().and_then(|c| c.get("entry")))
            .map(str::to_string),
        capacity_scu: scu_of(container.and_then(|c| c.get("capacity"))),
        description: None,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommodityDamage {
    pub name: String,
    pub record_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum_speed: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_squared_to_damage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grace_period: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explosion_damage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explosion_min_radius: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explosion_max_radius: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explosion_pressure: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volatility_radius_factor: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volatility_damage_factor: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volatility_force_factor: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volatility_commodity_damage_factor: Option<f64>,
}

pub fn parse_commodity_damage(record: &GameRecord) -> CommodityDamage {
    let v = &record.value;
    CommodityDamage {
        name: record.short_name.clone(),
        record_name: record.name.clone(),
        minimum_speed: num(v.get("minimumSpeed")),
        speed_squared_to_damage: num(v.get("speedSquaredToDamage")),
        grace_period: num(v.get("gracePeriod")),
        explosion_damage: num(at(v, "defaultExplosionParams.damage.damageTotal")),
        explosion_min_radius: num(at(v, "defaultExplosionParams.minRadius")),
        explosion_max_radius: num(at(v, "defaultExplosionParams.maxRadius")),
        explosion_pressure: num(at(v, "defaultExplosionParams.pressure")),
        volatility_radius_factor: num(v.get("volatilityRadiusFactor")),
        volatility_damage_factor: num(v.get("volatilityDamageFactor")),
        volatility_force_factor: num(v.get("volatilityForceFactor")),
        volatility_commodity_damage_factor: num(v.get("volatilityCommodityDamageFactor")),
    }
}

/// Every resource in the scoped `ResourceTypeDatabase` records, sorted by group then name
/// (the order `game-commodities` lists them in).
pub fn sorted_resources(records: &RecordSet, localization: &Localization) -> Vec<ResourceType> {
    let mut resources: Vec<ResourceType> = records
        .of_type("ResourceTypeDatabase")
        .flat_map(|r| parse_resource_database(r, localization, records))
        .collect();
    resources.sort_by(|a, b| {
        compare_names(&a.group, &b.group).then_with(|| {
            compare_names(
                a.display_name.as_deref().unwrap_or(&a.name),
                b.display_name.as_deref().unwrap_or(&b.name),
            )
        })
    });
    resources
}

impl Domain for Commodities {
    fn name(&self) -> &'static str {
        "game-commodities"
    }

    fn description(&self) -> &'static str {
        "Resource types with density and refined form, commodity cargo items, and cargo damage."
    }

    fn filters(&self) -> &'static [&'static str] {
        &[
            "**/resourcetypedatabase/**",
            "**/entities/commodities/**",
            "**/commodityconfiguration/**",
            "**/crafting/qualitydistribution/**",
            "**/crafting/qualityquantization/**",
            // Systems and clusters directly under pu/, which quality overrides name.
            "**/starmap/pu/*",
        ]
    }

    fn needs_localization(&self) -> bool {
        true
    }

    fn parse(&self, ctx: &ParseContext<'_>) -> Result<Map<String, Value>, DomainError> {
        let resources = sorted_resources(ctx.records, ctx.localization);
        let mut items: Vec<CommodityItem> = ctx
            .records
            .iter()
            .filter(|r| r.record_path.contains("/entities/commodities/"))
            .map(|r| {
                let mut item = parse_commodity_item(r, ctx.localization);
                let description = resources
                    .iter()
                    .find(|res| Some(&res.name) == item.resource.as_ref())
                    .and_then(|res| res.description.clone())
                    .filter(|d| !d.is_empty());
                item.description = description;
                item
            })
            .collect();
        items.sort_by(|a, b| {
            compare_names(
                a.display_name.as_deref().unwrap_or(&a.name),
                b.display_name.as_deref().unwrap_or(&b.name),
            )
        });
        let mut damage: Vec<CommodityDamage> = ctx
            .records
            .of_type("CommodityDamageConfiguration")
            .map(parse_commodity_damage)
            .collect();
        damage.sort_by(|a, b| compare_names(&a.name, &b.name));

        let mut by_group: Map<String, Value> = Map::new();
        for r in &resources {
            if let Value::Array(a) = by_group
                .entry(r.group.clone())
                .or_insert_with(|| Value::Array(Vec::new()))
            {
                a.push(Value::String(r.name.clone()));
            }
        }

        let mut body = Map::new();
        field(&mut body, "resources", &resources)?;
        field(&mut body, "items", &items)?;
        field(&mut body, "damage", &damage)?;
        body.insert("byGroup".into(), Value::Object(by_group));
        body.insert(
            "summary".into(),
            serde_json::json!({
                "resources": resources.len(),
                "items": items.len(),
                "damage": damage.len(),
            }),
        );
        Ok(body)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::domain::NoFiles;
    use crate::localization::parse_global_ini;
    use crate::records::normalize_record;
    use serde_json::json;

    pub(crate) fn rec(ty: &str, short: &str, value: Value, path: Option<&str>) -> GameRecord {
        let mut v = value;
        v["_Type_"] = json!(ty);
        let path = path
            .map(str::to_string)
            .unwrap_or_else(|| format!("libs/foundry/records/x/{}", short.to_lowercase()));
        normalize_record(
            json!({"_RecordName_": format!("{ty}.{short}"), "_RecordId_": format!("id-{short}"),
                   "_RecordValue_": v}),
            &path,
        )
        .unwrap()
    }

    pub(crate) fn ref_(name: &str) -> Value {
        json!({"_RecordName_": name, "_RecordId_": format!("id-{name}")})
    }

    fn resource(name: &str, density: f64, extra: Value) -> Value {
        let mut v = json!({
            "_RecordName_": format!("ResourceType.{name}"),
            "_RecordId_": format!("id-{name}"),
            "_Type_": "ResourceType",
            "displayName": format!("@items_commodities_{}", name.to_lowercase()),
            "densityType": {"densityUnit": {"_Type_": "GramsPerCubicCentimeter", "gramsPerCubicCentimeter": density}},
        });
        if let (Value::Object(m), Value::Object(e)) = (&mut v, extra) {
            m.extend(e);
        }
        v
    }

    /// The database nests groups and puts the refined resource inline under the raw one,
    /// with null entries scattered through both lists.
    pub(crate) fn database() -> GameRecord {
        rec(
            "ResourceTypeDatabase",
            "ResourceTypeDatabase",
            json!({"groups": [
                {"_RecordName_": "ResourceTypeGroup.Metal",
                 "resources": [resource("Gold", 1.0, json!({}))],
                 "groups": [null, {
                     "_RecordName_": "ResourceTypeGroup.UnrefinedOres",
                     "resources": [resource("Ore_Gold", 19.3, json!({"refinedVersion": resource("Gold", 1.0, json!({}))})), null],
                     "groups": []}]},
                {"_RecordName_": "ResourceTypeGroup.Mineral",
                 "resources": [resource("Raw_Quantainium", 20.45, json!({"properties": [null,
                     {"_Type_": "ResourceTypeVolatility", "volatility": 1, "healthDecayPerSecond": 0.0675}]}))],
                 "groups": []}
            ]}),
            None,
        )
    }

    pub(crate) fn strings() -> Localization {
        parse_global_ini(
            "items_commodities_gold=Gold\nitems_commodities_ore_gold=Gold (Ore)\nrefinery_ui_ProcessingType_SlowCareful=Dinyx Solventation",
        )
    }

    // Cases from sc-gamedata src/domains/economy.test.ts and links.test.ts.
    #[test]
    fn walks_nested_groups_and_skips_null_entries() {
        let set = RecordSet::new(vec![]);
        let resources = parse_resource_database(&database(), &strings(), &set);
        let names: Vec<_> = resources.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["Gold", "Ore_Gold", "Raw_Quantainium"]);
        let r = &resources[1];
        assert_eq!(r.group, "Metal");
        assert_eq!(r.subgroups, vec!["UnrefinedOres"]);
        assert_eq!(r.density_grams_per_cc, Some(19.3));
        assert_eq!(r.refines_to.as_deref(), Some("Gold"));
        assert_eq!(r.display_name.as_deref(), Some("Gold (Ore)"));
    }

    #[test]
    fn keeps_volatility_and_drops_unresolved_names() {
        let set = RecordSet::new(vec![]);
        let resources = parse_resource_database(&database(), &strings(), &set);
        let quant = resources.iter().find(|r| r.name == "Raw_Quantainium").unwrap();
        assert_eq!(
            quant.volatility,
            Some(Volatility {
                volatility: Some(1.0),
                health_decay_per_second: Some(0.0675)
            })
        );
        assert_eq!(quant.display_name, None);
    }

    #[test]
    fn strips_ui_markup_from_names() {
        let co2 = rec(
            "ResourceTypeDatabase",
            "Db",
            json!({"groups": [{"_RecordName_": "ResourceTypeGroup.Gas", "resources": [resource("CarbonDioxide", 1.0, json!({}))]}]}),
            None,
        );
        let loc = parse_global_ini(r#"items_commodities_carbondioxide=CO<font size="%d">2</font>"#);
        let r = parse_resource_database(&co2, &loc, &RecordSet::new(vec![]));
        assert_eq!(r[0].display_name.as_deref(), Some("CO2"));
    }

    #[test]
    fn descriptions() {
        let mut loc = Localization::new();
        loc.set("ore_des", "Found near <b>volcanic</b> outcroppings.\\nUsed as an insulator. ");
        loc.set("ph_des", "<= PLACEHOLDER =>");
        loc.set("t", "Tritium description.");
        loc.set("k,P", "Suffixed");
        let d = |k: &str| description_of(&loc, Some(&json!(k)));
        assert_eq!(
            d("@ore_des").as_deref(),
            Some("Found near volcanic outcroppings.\nUsed as an insulator.")
        );
        assert_eq!(d("@ph_des"), None);
        assert_eq!(d("@t"), None);
        assert_eq!(d("@k").as_deref(), Some("Suffixed"));
        assert!(is_description_stub("x description"));
        assert!(!is_description_stub("a b description"));
    }

    #[test]
    fn scu_units() {
        assert_eq!(scu_of(Some(&json!({"_Type_": "SCentiCargoUnit", "centiSCU": 1}))), Some(0.01));
        assert_eq!(scu_of(Some(&json!({"_Type_": "SMicroCargoUnit", "microSCU": 750}))), Some(0.00075));
    }

    #[test]
    fn links_an_item_to_its_resource() {
        let item = parse_commodity_item(
            &rec(
                "EntityClassDefinition",
                "gold",
                json!({"Components": [
                    {"_Type_": "SCItemPurchasableParams", "displayName": "@items_commodities_gold"},
                    {"_Type_": "ResourceContainer", "capacity": {"centiSCU": 1},
                     "defaultComposition": [{"entry": ref_("ResourceType.Gold"), "weight": 1}]}
                ]}),
                Some("libs/foundry/records/entities/commodities/metals/gold"),
            ),
            &strings(),
        );
        assert_eq!(item.display_name.as_deref(), Some("Gold"));
        assert_eq!(item.category, "metals");
        assert_eq!(item.resource.as_deref(), Some("Gold"));
        assert_eq!(item.capacity_scu, Some(0.01));
    }

    #[test]
    fn parses_the_whole_domain() {
        let set = RecordSet::new(vec![
            database(),
            rec(
                "CommodityDamageConfiguration",
                "Base",
                json!({"minimumSpeed": 15, "gracePeriod": 10}),
                None,
            ),
        ]);
        let loc = Localization::new();
        let out = DOMAIN.parse(&ParseContext::new(&set, &loc, &NoFiles)).unwrap();
        assert_eq!(out["summary"], json!({"resources": 3, "items": 0, "damage": 1}));
        assert_eq!(
            out["byGroup"],
            json!({"Metal": ["Gold", "Ore_Gold"], "Mineral": ["Raw_Quantainium"]})
        );
        let keys: Vec<_> = out.keys().cloned().collect();
        assert_eq!(keys, vec!["resources", "items", "damage", "byGroup", "summary"]);
    }

    fn file_ref(path: &str) -> Value {
        json!(format!("file://./../../../../libs/foundry/records/{path}.json"))
    }

    #[test]
    fn quality_joins_distribution_dedups_overrides_and_bands() {
        let p = |s: &str| Some(format!("libs/foundry/records/{s}"));
        let set = RecordSet::new(vec![
            rec(
                "CraftingQualityDistributionRecord",
                "LegendaryShipMineable_QualityDistribution_Default",
                json!({"qualityDistribution": {"min": 501, "max": 1000, "mean": 500, "stddev": 203}}),
                p("crafting/qualitydistribution/shipmineables/legendaryshipmineable_qualitydistribution_default").as_deref(),
            ),
            rec(
                "CraftingQualityLocationOverrideRecord",
                "legendary_override",
                json!({"locationOverride": {"locationOverrideList": [
                    {"location": file_ref("starmap/pu/pyrosolarsystem"), "qualityDistribution": {"min": 501, "max": 1000, "mean": 500, "stddev": 225}},
                    {"location": file_ref("starmap/pu/pyrosolarsystem"), "qualityDistribution": {"min": 501, "max": 1000, "mean": 500, "stddev": 225}}
                ]}}),
                p("crafting/qualitydistribution/shipmineables/legendary_override").as_deref(),
            ),
            rec("StarMapObject", "PyroSolarSystem", json!({"name": "@pyro"}), p("starmap/pu/pyrosolarsystem").as_deref()),
            rec(
                "CraftingQualityQuantizationRecord",
                "quantization_stileron",
                json!({"qualityQuantization": {"bands": [{"start": 950, "end": 998, "mappedValue": 972}]}}),
                p("crafting/qualityquantization/quantization_stileron").as_deref(),
            ),
        ]);
        let props = vec![json!({
            "_Type_": "ResourceTypeCraftingData",
            "qualityDistribution": {"qualityDistributionRecord": file_ref("crafting/qualitydistribution/shipmineables/legendaryshipmineable_qualitydistribution_default")},
            "qualityLocationOverride": {"locationOverrideRecord": file_ref("crafting/qualitydistribution/shipmineables/legendary_override")},
            "qualityQuantization": {"qualityQuantizationRecord": file_ref("crafting/qualityquantization/quantization_stileron")}
        })];
        let q = parse_quality(&props, &set, &parse_global_ini("pyro=Pyro System")).unwrap();
        assert_eq!(
            serde_json::to_value(&q).unwrap(),
            json!({
                "profile": "LegendaryShipMineable_QualityDistribution_Default",
                "tier": "Legendary",
                "distribution": {"min": 501.0, "max": 1000.0, "mean": 500.0, "stddev": 203.0},
                "locationOverrides": [{"location": "Pyro System", "min": 501.0, "max": 1000.0, "mean": 500.0, "stddev": 225.0}],
                "bands": [{"start": 950.0, "end": 998.0, "value": 972.0}]
            })
        );
        assert_eq!(quality_tier("FPSMineable_QualityDistribution_X"), "FPSMineable");
        assert_eq!(quality_tier("NoMatch"), "NoMatch");
    }
}
