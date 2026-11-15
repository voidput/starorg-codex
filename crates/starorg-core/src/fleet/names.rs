//! The `ship-names` generator (addendum §6.5): sort prefixes and renames on the hull name
//! keys. Pure: settings + catalogue -> edits in parts (prefix + rename or stock).
//!
//! The same rules exist in TypeScript for the live preview (`app/src/lib/shipnames.ts`); both
//! are tested against `fixtures/ship-names-vectors.json`.

use super::catalog::{Catalog, sort_key};
use super::store::{Rest, ShipNames, Style};
use crate::ini::fold_key;
use std::collections::BTreeSet;

/// One edit on a name key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameEdit {
    /// Stock key spelling (hull id, or the short key).
    pub key: String,
    pub hull: String,
    pub short: bool,
    pub prefix: String,
    /// Whole-value rename; `None` keeps the stock value after the prefix.
    pub replace: Option<String>,
}

impl NameEdit {
    pub fn value(&self, stock: &str) -> String {
        format!(
            "{}{}",
            self.prefix,
            self.replace.as_deref().unwrap_or(stock)
        )
    }
}

/// A listed ship of this build, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    pub hull: String,
    /// 1-based position among the listed ships of this build.
    pub position: usize,
    pub group: Option<String>,
    /// The prefix without the separator (`01`, `A1`, `*`, empty).
    pub label: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub edits: Vec<NameEdit>,
    pub placed: Vec<Placed>,
    /// Listed hulls this build does not have (skipped, counted, no warning).
    pub not_in_build: Vec<String>,
}

fn digits(n: usize) -> usize {
    n.max(1).to_string().len()
}

/// Labels per style for ships at `positions` (block index, position in block, overall).
pub fn plan(s: &ShipNames, cat: &Catalog) -> Plan {
    let mut out = Plan::default();
    let mut ships = Vec::new();
    for (g, e) in s.ordered() {
        match cat.get(&e.hull) {
            Some(h) => ships.push((g.map(|g| g.id.clone()), e, h)),
            None => out.not_in_build.push(e.hull.clone()),
        }
    }
    // Blocks of this build: consecutive ships with the same group (ungrouped first).
    let mut block_of = Vec::with_capacity(ships.len());
    let mut in_block = Vec::with_capacity(ships.len());
    let mut sizes: Vec<usize> = Vec::new();
    for (i, (g, _, _)) in ships.iter().enumerate() {
        if i == 0 || ships[i - 1].0 != *g {
            sizes.push(0);
        }
        let b = sizes.len() - 1;
        sizes[b] += 1;
        block_of.push(b);
        in_block.push(sizes[b]);
    }
    let listed: BTreeSet<String> = ships.iter().map(|(_, _, h)| fold_key(&h.hull)).collect();
    let rest: Vec<&super::Hull> = if s.style == Style::Numeric && s.rest == Rest::Numbered {
        cat.flyable()
            .filter(|h| !listed.contains(&fold_key(&h.hull)))
            .collect()
    } else {
        Vec::new()
    };
    let total = ships.len() + rest.len();
    let num_w = digits(total).max(2);
    let blk_w = digits(sizes.iter().copied().max().unwrap_or(1));
    let sep = &s.separator;
    let with_sep = |label: &str| {
        if label.is_empty() {
            String::new()
        } else {
            format!("{label}{sep}")
        }
    };
    for (i, (g, e, h)) in ships.iter().enumerate() {
        let label = match s.style {
            Style::None => String::new(),
            Style::Numeric => format!("{:0w$}", i + 1, w = num_w),
            Style::GroupLetter => {
                let letter = (b'A' + (block_of[i] as u8).min(25)) as char;
                format!("{letter}{:0w$}", in_block[i], w = blk_w)
            }
            Style::Symbol => s.symbol.clone(),
        };
        let prefix = with_sep(&label);
        if !prefix.is_empty() || e.name.is_some() {
            out.edits.push(NameEdit {
                key: h.hull.clone(),
                hull: h.hull.clone(),
                short: false,
                prefix: prefix.clone(),
                replace: e.name.clone(),
            });
        }
        if let Some(sk) = &h.short_key {
            let renamed = e.short.clone().or_else(|| e.name.clone());
            let sp = if s.prefix_short {
                prefix.clone()
            } else {
                String::new()
            };
            if renamed.is_some() || !sp.is_empty() {
                out.edits.push(NameEdit {
                    key: sk.clone(),
                    hull: h.hull.clone(),
                    short: true,
                    prefix: sp,
                    replace: renamed,
                });
            }
        }
        out.placed.push(Placed {
            hull: h.hull.clone(),
            position: i + 1,
            group: g.clone(),
            label,
        });
    }
    for (j, h) in rest.iter().enumerate() {
        out.edits.push(NameEdit {
            key: h.hull.clone(),
            hull: h.hull.clone(),
            short: false,
            prefix: with_sep(&format!("{:0w$}", ships.len() + j + 1, w = num_w)),
            replace: None,
        });
    }
    out
}

/// The long names of every flyable hull after the plan (and `final_of`, which gives the
/// value an override or loc patch puts there instead), sorted the way the preview assumes the
/// game sorts: case-insensitive, character by character.
pub fn preview(
    plan: &Plan,
    cat: &Catalog,
    final_of: &dyn Fn(&str) -> Option<String>,
) -> Vec<String> {
    let mut v: Vec<String> = cat
        .flyable()
        .map(|h| {
            final_of(&h.hull).unwrap_or_else(|| {
                plan.edits
                    .iter()
                    .find(|e| !e.short && e.hull == h.hull)
                    .map(|e| e.value(&h.name))
                    .unwrap_or_else(|| h.name.clone())
            })
        })
        .collect();
    v.sort_by(|a, b| sort_key(a).cmp(&sort_key(b)).then_with(|| a.cmp(b)));
    v
}

/// One listed ship as `fleet list` and the Ships screen show it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Listed {
    /// 1-based among the listed ships this build has; `None` when it lacks the hull.
    pub position: Option<usize>,
    pub hull: String,
    pub group: Option<String>,
    /// Prefix with its separator (`01 `), empty for none.
    pub prefix: String,
    /// Stock long name (empty when the build lacks the hull).
    pub stock: String,
    /// Final long name after the plan and overrides.
    pub value: String,
    pub renamed: bool,
    /// An override replaces the long (or short) key: it wins with its whole value.
    pub overridden: bool,
    pub overridden_short: bool,
    pub not_in_build: bool,
}

/// The listed ships in order with their final names. `overrides` are the channel's
/// `overrides.ini` lines (a later line for the same key wins, as in merge).
pub fn listing(s: &ShipNames, cat: &Catalog, overrides: &[crate::state::Override]) -> Vec<Listed> {
    let p = plan(s, cat);
    let ov = |key: &str| {
        let f = fold_key(key);
        overrides
            .iter()
            .rev()
            .find(|o| fold_key(&o.key) == f)
            .map(|o| o.value.clone())
    };
    s.ordered()
        .into_iter()
        .map(|(g, e)| {
            let group = g.map(|g| g.id.clone());
            match (
                cat.get(&e.hull),
                p.placed
                    .iter()
                    .find(|x| fold_key(&x.hull) == fold_key(&e.hull)),
            ) {
                (Some(h), Some(placed)) => {
                    let edit = p.edits.iter().find(|x| !x.short && x.hull == h.hull);
                    let prefix = edit.map(|x| x.prefix.clone()).unwrap_or_default();
                    let long_ov = ov(&h.hull);
                    let short_ov = h.short_key.as_deref().and_then(ov);
                    Listed {
                        position: Some(placed.position),
                        hull: h.hull.clone(),
                        group,
                        prefix,
                        stock: h.name.clone(),
                        value: long_ov.clone().unwrap_or_else(|| {
                            edit.map(|x| x.value(&h.name))
                                .unwrap_or_else(|| h.name.clone())
                        }),
                        renamed: e.name.is_some(),
                        overridden: long_ov.is_some(),
                        overridden_short: short_ov.is_some(),
                        not_in_build: false,
                    }
                }
                _ => Listed {
                    position: None,
                    hull: e.hull.clone(),
                    group,
                    prefix: String::new(),
                    stock: String::new(),
                    value: e.name.clone().unwrap_or_default(),
                    renamed: e.name.is_some(),
                    overridden: false,
                    overridden_short: false,
                    not_in_build: true,
                },
            }
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::super::catalog::Hull;
    use super::super::store::{Group, ShipEntry};
    use super::*;
    use serde_json::{Value, json};

    fn hull(id: &str, name: &str, short: Option<&str>) -> Hull {
        Hull {
            hull: id.into(),
            name: name.into(),
            short: short.map(str::to_string),
            short_key: short.map(|_| format!("{id}_short")),
            maker: None,
            flyable: true,
            ground: false,
            npc_shared: 0,
            namesake: true,
        }
    }

    /// The shared vector catalogue: 12 synthetic hulls with real-looking names.
    fn vector_catalog() -> Vec<Hull> {
        vec![
            hull("vehicle_NameAEGS_Gladius", "Aegis Gladius", Some("Gladius")),
            hull(
                "vehicle_NameAEGS_Avenger_Titan",
                "Aegis Avenger Titan",
                Some("Avenger Titan"),
            ),
            hull("vehicle_NameANVL_Arrow", "Anvil Arrow", Some("Arrow")),
            hull(
                "vehicle_NameDRAK_Cutlass_Black",
                "Drake Cutlass Black",
                Some("Cutlass Black"),
            ),
            hull("vehicle_NameDRAK_Vulture", "Drake Vulture", None),
            hull(
                "vehicle_NameMISC_Prospector",
                "MISC Prospector",
                Some("Prospector"),
            ),
            hull(
                "vehicle_NameRSI_Aurora_MR",
                "RSI Aurora MR",
                Some("Aurora MR"),
            ),
            hull(
                "vehicle_NameORIG_890Jump",
                "Origin 890 Jump",
                Some("890 Jump"),
            ),
            hull(
                "vehicle_NameCRUS_Starlifter_C2",
                "Crusader C2 Hercules Starlifter",
                Some("C2 Hercules"),
            ),
            hull("vehicle_NameARGO_Mole", "Argo MOLE", Some("MOLE")),
            hull(
                "vehicle_NameKRIG_P52_Merlin",
                "Kruger P-52 Merlin",
                Some("P-52 Merlin"),
            ),
            hull(
                "vehicle_NameTMBL_Cyclone",
                "Tumbril Cyclone",
                Some("Cyclone"),
            ),
        ]
    }

    fn settings_from(v: &Value) -> ShipNames {
        let mut s = ShipNames {
            enabled: v["enabled"].as_bool().unwrap(),
            style: serde_json::from_value(v["style"].clone()).unwrap(),
            separator: v["separator"].as_str().unwrap().into(),
            symbol: v["symbol"].as_str().unwrap().into(),
            rest: serde_json::from_value(v["rest"].clone()).unwrap(),
            prefix_short: v["prefix_short"].as_bool().unwrap(),
            ..ShipNames::default()
        };
        for g in v["groups"].as_array().unwrap() {
            s.groups.push(Group {
                id: g["id"].as_str().unwrap().into(),
                name: g["name"].as_str().unwrap().into(),
                extra: Default::default(),
            });
        }
        for e in v["ships"].as_array().unwrap() {
            let opt = |k: &str| e[k].as_str().map(str::to_string);
            s.ships.push(ShipEntry {
                hull: e["hull"].as_str().unwrap().into(),
                group: opt("group"),
                name: opt("name"),
                short: opt("short"),
                extra: Default::default(),
            });
        }
        s
    }

    fn ship(hull: &str, group: Option<&str>, name: Option<&str>, short: Option<&str>) -> Value {
        json!({ "hull": hull, "group": group, "name": name, "short": short })
    }

    fn settings(
        style: &str,
        sep: &str,
        rest: &str,
        prefix_short: bool,
        groups: Value,
        ships: Vec<Value>,
    ) -> Value {
        json!({ "enabled": true, "style": style, "separator": sep, "symbol": "*", "rest": rest,
                "prefix_short": prefix_short, "groups": groups, "ships": ships })
    }

    /// Settings of every vector case (the expected output is computed, then compared with
    /// the committed file).
    fn cases() -> Vec<(&'static str, Value)> {
        let g = |id: &str| format!("vehicle_Name{id}");
        let basic = vec![
            ship(&g("DRAK_Vulture"), None, None, None),
            ship(&g("AEGS_Gladius"), None, Some("Needle"), None),
            ship(&g("MISC_Prospector"), None, None, None),
        ];
        let groups =
            json!([{ "id": "combat", "name": "Combat" }, { "id": "cargo", "name": "Cargo" }]);
        let grouped = vec![
            ship(&g("AEGS_Gladius"), Some("combat"), None, None),
            ship(
                &g("DRAK_Cutlass_Black"),
                Some("cargo"),
                Some("Bucket"),
                Some("Bkt"),
            ),
            ship(&g("RSI_Aurora_MR"), None, None, None),
            ship(&g("ANVL_Arrow"), Some("combat"), None, None),
            ship(&g("CRUS_Starlifter_C2"), Some("cargo"), None, None),
            ship(&g("NOPE_Missing"), None, None, None),
            ship(&g("tmbl_cyclone"), Some("nogroup"), None, None),
        ];
        vec![
            (
                "numeric, space",
                settings("numeric", " ", "stock", false, json!([]), basic.clone()),
            ),
            (
                "numeric, dash, prefix short",
                settings("numeric", "-", "stock", true, json!([]), basic.clone()),
            ),
            (
                "numeric, rest numbered",
                settings("numeric", ". ", "numbered", false, json!([]), basic.clone()),
            ),
            (
                "none = rename only",
                settings("none", " ", "numbered", true, json!([]), basic.clone()),
            ),
            (
                "symbol",
                settings("symbol", " ", "stock", false, json!([]), basic),
            ),
            (
                "group letters",
                settings(
                    "group-letter",
                    " ",
                    "stock",
                    false,
                    groups.clone(),
                    grouped.clone(),
                ),
            ),
            (
                "numeric over groups",
                settings("numeric", " ", "stock", true, groups, grouped),
            ),
        ]
    }

    fn expected(cat: &Catalog, v: &Value) -> Value {
        let s = settings_from(v);
        let (s, _, invalid) = s.validated();
        assert!(!invalid);
        let p = plan(&s, cat);
        let edits: Vec<Value> = p
            .edits
            .iter()
            .map(|e| {
                let h = cat.get(&e.hull).unwrap();
                let stock = if e.short {
                    h.short.clone().unwrap()
                } else {
                    h.name.clone()
                };
                json!({ "key": e.key, "value": e.value(&stock) })
            })
            .collect();
        json!({
            "edits": edits,
            "not_in_build": p.not_in_build,
            "preview": preview(&p, cat, &|_| None),
        })
    }

    /// `fixtures/ship-names-vectors.json`: written with `UPDATE_SHIP_NAME_VECTORS=1`, read
    /// here and by `app/src/lib/shipnames.test.ts`.
    #[test]
    fn shared_vectors_match() {
        let cat = Catalog::new(vector_catalog());
        let catalog: Vec<Value> = vector_catalog()
            .iter()
            .map(|h| json!({ "hull": h.hull, "name": h.name, "short": h.short, "short_key": h.short_key }))
            .collect();
        let cases: Vec<Value> = cases()
            .into_iter()
            .map(|(name, s)| {
                let exp = expected(&cat, &s);
                json!({ "name": name, "settings": s, "expected": exp })
            })
            .collect();
        let doc = json!({
            "schema": 1,
            "about": "Shared test vectors for the ship-names prefix and sort rules (addendum 6.9). Written by `UPDATE_SHIP_NAME_VECTORS=1 cargo test -p starorg-core shared_vectors`; read by the Rust and the TypeScript tests.",
            "sort_rule": "case-insensitive, character by character (unconfirmed, Q23)",
            "catalog": catalog,
            "cases": cases,
        });
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/ship-names-vectors.json");
        let text = serde_json::to_string_pretty(&doc).unwrap() + "\n";
        if std::env::var_os("UPDATE_SHIP_NAME_VECTORS").is_some() {
            std::fs::write(&path, &text).unwrap();
            return;
        }
        let committed = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        let a: Value = serde_json::from_str(&committed).expect("vector file is JSON");
        assert_eq!(
            a,
            doc,
            "{} is out of date: UPDATE_SHIP_NAME_VECTORS=1",
            path.display()
        );
    }

    fn many(n: usize) -> (Catalog, ShipNames) {
        let hulls: Vec<Hull> = (0..n)
            .map(|i| {
                hull(
                    &format!("vehicle_NameX_{i:03}"),
                    &format!("Ship {i:03}"),
                    None,
                )
            })
            .collect();
        let mut s = ShipNames::default();
        s.enabled = true;
        for h in &hulls {
            s.ships.push(ShipEntry {
                hull: h.hull.clone(),
                group: None,
                name: None,
                short: None,
                extra: Default::default(),
            });
        }
        (Catalog::new(hulls), s)
    }

    #[test]
    fn numeric_padding_at_9_10_99_100() {
        for (n, first, last) in [
            (9, "01 ", "09 "),
            (10, "01 ", "10 "),
            (99, "01 ", "99 "),
            (100, "001 ", "100 "),
        ] {
            let (c, s) = many(n);
            let p = plan(&s, &c);
            assert_eq!(p.edits.first().unwrap().prefix, first, "{n}");
            assert_eq!(p.edits.last().unwrap().prefix, last, "{n}");
        }
        let (c, mut s) = many(10);
        s.style = Style::GroupLetter;
        let p = plan(&s, &c);
        assert_eq!(p.edits[0].prefix, "A01 ");
        let (c, mut s) = many(9);
        s.style = Style::GroupLetter;
        assert_eq!(plan(&s, &c).edits[8].prefix, "A9 ");
    }

    #[test]
    fn ids_match_folded_and_short_needs_a_reason() {
        let c = Catalog::new(vector_catalog());
        let mut s = ShipNames::default();
        s.ships.push(ShipEntry {
            hull: "VEHICLE_NAMEtmbl_cyclone".into(),
            group: None,
            name: None,
            short: None,
            extra: Default::default(),
        });
        let p = plan(&s, &c);
        assert_eq!(
            p.edits.len(),
            1,
            "no short edit without a rename or prefix_short"
        );
        assert_eq!(p.edits[0].key, "vehicle_NameTMBL_Cyclone", "stock spelling");
        assert_eq!(p.edits[0].value("Tumbril Cyclone"), "01 Tumbril Cyclone");
        s.style = Style::None;
        assert!(plan(&s, &c).edits.is_empty(), "none + no rename = nothing");
    }
}
