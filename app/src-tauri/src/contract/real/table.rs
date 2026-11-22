//! The Strings table: apply's build step (data patches -> generators -> merge -> loc patches
//! -> overrides, spec 7.1 step 5) run in memory, kept per row with where each value came
//! from. Used by the Strings screen, the previews, patch outcomes and plan counts. Writes
//! nothing.

use crate::contract::types::*;
use starorg_core::blueprints::Selection;
use starorg_core::build::BuildInfo;
use starorg_core::enhance::{self, Category, profile::Profile as EnhanceProfile};
use starorg_core::groups::GroupIndex;
use starorg_core::groups::owners::{self, OwnerIndex};
use starorg_core::ini::StockIni;
use starorg_core::merge::{self, Source};
use starorg_core::model::GameData;
use starorg_core::patch::{self, PatchSet};
use starorg_core::state::Override;
use std::collections::HashMap;
use std::sync::Arc;

/// One row's history.
#[derive(Debug, Clone)]
pub struct RowInfo {
    /// Generator and the value it offered (before merge), when one offered.
    pub generated: Option<(String, String)>,
    pub source: Option<Source>,
}

pub struct Table {
    pub rows: Vec<StringRow>,
    pub info: Vec<RowInfo>,
    /// Lower-case key -> row.
    pub index: HashMap<String, usize>,
    pub summary: Vec<patch::PatchOutcome>,
    pub counts: ApplyCounts,
    /// "Also used by" per row (Q-G3), from the group index.
    pub also: Vec<Vec<String>>,
    /// Owners per row (same order as `rows`), for the detail's full list.
    pub owners: Arc<OwnerIndex>,
}

/// Names and makers of the secondary owners, lower case, for search (`None` when none).
pub fn other_owners<'a>(rest: impl Iterator<Item = owners::Owner<'a>>) -> Option<Box<str>> {
    let mut out = String::new();
    for o in rest {
        out.push_str(&o.name.to_lowercase());
        out.push('\n');
        if let Some(d) = o.detail {
            out.push_str(&d.to_lowercase());
            out.push('\n');
        }
    }
    (!out.is_empty()).then(|| out.into())
}

/// A core owner in contract terms.
pub fn key_owner(o: owners::Owner<'_>) -> KeyOwner {
    use owners::{Field as F, OwnerKind as K};
    KeyOwner {
        name: o.name.to_string(),
        field: Some(match o.field {
            F::Name => OwnerField::Name,
            F::ShortName => OwnerField::ShortName,
            F::Description => OwnerField::Description,
            F::Title => OwnerField::Title,
            F::Career => OwnerField::Career,
            F::Role => OwnerField::Role,
        }),
        kind: match o.kind {
            K::Ship => OwnerKind::Ship,
            K::GroundVehicle => OwnerKind::GroundVehicle,
            K::Component => OwnerKind::Component,
            K::ShipWeapon => OwnerKind::ShipWeapon,
            K::FpsWeapon => OwnerKind::FpsWeapon,
            K::Missile => OwnerKind::Missile,
            K::Mission => OwnerKind::Mission,
            K::Reputation => OwnerKind::Reputation,
            K::Manufacturer => OwnerKind::Manufacturer,
            K::Named => OwnerKind::Named,
        },
        detail: o.detail.map(str::to_string),
    }
}

/// The UI category of a generator (`StringRow::category`).
pub fn category_of(generator: &str) -> &'static str {
    match generator {
        "ships" => "ships",
        "components" | "component-tags" => "components",
        "ship-weapons" | "ship-weapon-tags" => "ship-weapons",
        "fps-weapons" => "fps-weapons",
        "missiles" | "missile-tags" => "missiles",
        "mission-titles" | "mission-descriptions" => "missions",
        "branding" | "colour-test" => "ui",
        "ship-names" => "ships",
        "quest-hints" => "items",
        "obtain-hints" => "items",
        "missing-names" => "items",
        _ => "other",
    }
}

/// A category for rows no generator touched, from the key.
fn guess_category(key: &str) -> &'static str {
    let k = key.to_ascii_lowercase();
    if k.starts_with("vehicle_") {
        "ships"
    } else if k.starts_with("item_") {
        "items"
    } else if k.contains("_desc") || k.contains("_title") || k.contains("mission") {
        "missions"
    } else if k.starts_with("ui_") || k.starts_with("frontend_") || k.starts_with("pu_") {
        "ui"
    } else {
        "other"
    }
}

pub struct Inputs<'a> {
    pub stock: &'a StockIni,
    pub game_data: &'a GameData,
    pub build: Option<&'a BuildInfo>,
    pub profile: &'a EnhanceProfile,
    pub patches: &'a PatchSet,
    pub overrides: &'a [Override],
    pub owned: &'a Selection,
    /// Groups of the stock keys, in `stock.entries()` order.
    pub groups: &'a GroupIndex,
    /// Owners of the stock keys, in `stock.entries()` order (built lazily per build).
    pub owners: &'a Arc<OwnerIndex>,
    /// The channel's active ship-name settings (addendum §6.5).
    pub ship_names: Option<&'a starorg_core::fleet::store::ShipNames>,
    /// Obtain hints with their description keys filled (spec obtain-hints), when the
    /// profile asks for them and the datasets could be read.
    pub obtain: Option<&'a starorg_core::model::ObtainHints>,
}

pub fn build(i: &Inputs<'_>) -> Table {
    let patched = patch::data_overlay(i.patches, i.game_data, i.build);
    // Curated names from the same patch set merge applies (as `apply` does).
    let curated = patch::curated_names(i.patches, i.game_data, i.build);
    let generated = enhance::generate_named(
        patched.as_ref().unwrap_or(i.game_data),
        i.stock,
        i.profile,
        i.owned,
        i.ship_names,
        i.obtain,
        &curated,
    );
    let gen_edits = merge::gen_edits(&generated);
    let merged = merge::merge(
        i.stock,
        i.game_data,
        i.build,
        &gen_edits,
        i.patches,
        i.overrides,
    );
    let finals: HashMap<usize, &str> = merged.edits.iter().collect();
    let n = |cs: &[Category]| cs.iter().map(|&c| generated.count(c)).sum::<usize>() as u32;
    let c = &merged.counts;
    let counts = ApplyCounts {
        changed: c.changed as u32,
        ships: n(&[Category::Ships]),
        components: n(&[Category::Components, Category::ComponentTags]),
        ship_weapons: n(&[Category::ShipWeapons, Category::ShipWeaponTags]),
        fps_weapons: n(&[Category::FpsWeapons]),
        missiles: n(&[Category::Missiles, Category::MissileTags]),
        missions: n(&[Category::MissionTitles, Category::MissionDescriptions]),
        // Counted with its extensions: most blocks go after another category's stats.
        obtain_hints: n(&[Category::ObtainHints]),
        missing_names: n(&[Category::MissingNames]),
        overrides: c.overrides as u32,
        dropped: c.dropped as u32,
    };
    let mut rows = Vec::new();
    let mut info = Vec::new();
    let mut index = HashMap::new();
    let mut also = Vec::new();
    for (row_no, (line, key, stock_value)) in i.stock.entries().enumerate() {
        let gen_edit = generated.edits.get(&line);
        let generated_v = gen_edit.map(|g| {
            (
                g.category.name().to_string(),
                g.value(stock_value).to_string(),
            )
        });
        let source = merged.provenance.get(&line).cloned();
        let result = finals
            .get(&line)
            .map(|v| v.to_string())
            .unwrap_or_else(|| stock_value.to_string());
        let (src, category) = match &source {
            _ if result == stock_value => (
                StringSource::Unmodified,
                generated_v
                    .as_ref()
                    .map(|(g, _)| category_of(g))
                    .unwrap_or_else(|| guess_category(key)),
            ),
            Some(Source::Generated { generator }) => {
                (StringSource::Enhanced, category_of(generator))
            }
            Some(Source::LocPatch { .. }) => (
                StringSource::Patched,
                generated_v
                    .as_ref()
                    .map(|(g, _)| category_of(g))
                    .unwrap_or_else(|| guess_category(key)),
            ),
            Some(Source::Override) => (
                StringSource::Edited,
                generated_v
                    .as_ref()
                    .map(|(g, _)| category_of(g))
                    .unwrap_or_else(|| guess_category(key)),
            ),
            None => (StringSource::Enhanced, guess_category(key)),
        };
        index.insert(key.to_ascii_lowercase(), rows.len());
        let g = i.groups.group(row_no);
        rows.push(StringRow {
            key: key.to_string(),
            original: stock_value.to_string(),
            result,
            source: src,
            category: category.to_string(),
            group: g.id.to_string(),
            group_label: g.label.to_string(),
            manufacturer: i.groups.manufacturer(row_no).map(|m| m.code.clone()),
            size: i.groups.size(row_no).map(u32::from),
            owner: i.owners.primary(row_no).map(|o| Box::new(key_owner(o))),
            owners: i.owners.count(row_no) as u32,
            other_owners: other_owners(i.owners.owners(row_no).skip(1)),
        });
        also.push(
            i.groups
                .also_used_by(row_no)
                .into_iter()
                .map(str::to_string)
                .collect(),
        );
        info.push(RowInfo {
            generated: generated_v,
            source,
        });
    }
    // Keys stock lacks (spec missing-names): rows after the stock ones, empty original.
    let finals_added: HashMap<String, &str> = merged
        .edits
        .added()
        .map(|(k, v)| (starorg_core::ini::fold_key(k), v))
        .collect();
    for (folded, a) in &generated.added {
        let Some(result) = finals_added.get(folded) else {
            continue;
        };
        let source = merged.added_provenance.get(folded).cloned();
        let src = match source {
            Some(Source::Override) => StringSource::Edited,
            Some(Source::LocPatch { .. }) => StringSource::Patched,
            _ => StringSource::Enhanced,
        };
        index.insert(a.key.to_ascii_lowercase(), rows.len());
        let g = i.groups.group(usize::MAX);
        rows.push(StringRow {
            key: a.key.clone(),
            original: String::new(),
            result: result.to_string(),
            source: src,
            category: category_of(a.category.name()).to_string(),
            group: g.id.to_string(),
            group_label: g.label.to_string(),
            manufacturer: None,
            size: None,
            owner: None,
            owners: 0,
            other_owners: None,
        });
        also.push(Vec::new());
        info.push(RowInfo {
            generated: Some((a.category.name().to_string(), a.value.clone())),
            source,
        });
    }
    Table {
        rows,
        info,
        index,
        summary: patch::summarise(i.patches, &merged.patch_outcomes),
        counts,
        also,
        owners: i.owners.clone(),
    }
}

impl Table {
    pub fn row(&self, key: &str) -> Option<(usize, &StringRow)> {
        let i = *self.index.get(&key.to_ascii_lowercase())?;
        Some((i, &self.rows[i]))
    }

    /// The layers of one key, the winner last (spec 9.4 Strings detail).
    pub fn detail(&self, i: usize, override_value: Option<String>) -> StringDetail {
        let row = crate::contract::strings_query::with_fallback(self.rows[i].clone());
        let info = &self.info[i];
        let unchanged = row.result == row.original;
        let mut p = vec![Provenance {
            layer: Layer::Stock,
            by: "Data.p4k".into(),
            text: row.original.clone(),
            winner: unchanged,
            note: None,
        }];
        if let Some((g, v)) = &info.generated {
            p.push(Provenance {
                layer: Layer::Generated,
                by: g.clone(),
                text: v.clone(),
                winner: !unchanged && matches!(info.source, Some(Source::Generated { .. }) | None),
                note: None,
            });
        }
        if let Some(Source::LocPatch { patch }) = &info.source {
            p.push(Provenance {
                layer: Layer::Patch,
                by: patch.clone(),
                text: row.result.clone(),
                winner: !unchanged,
                note: None,
            });
        }
        if let Some(v) = &override_value {
            p.push(Provenance {
                layer: Layer::Override,
                by: "overrides.ini".into(),
                text: v.clone(),
                winner: matches!(info.source, Some(Source::Override)),
                note: (!matches!(info.source, Some(Source::Override))).then(|| {
                    "This override was dropped: it breaks a rule (see Activity)".to_string()
                }),
            });
        }
        StringDetail {
            row,
            provenance: p,
            override_value,
            also_used_by: self.also.get(i).cloned().unwrap_or_default(),
            owners: self.owners.owners(i).map(key_owner).collect(),
        }
    }
}
