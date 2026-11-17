//! Fleet and ship names in contract form (addendum §6), shared by the real and the mock
//! backend: both run the core importer, matcher and prefix rules; only where the files live
//! differs (app folders vs memory).

use super::types::*;
use starorg_core::fleet::import::{self, Outcome};
use starorg_core::fleet::store::{
    self, Effective, FleetFile, Group, Rest, Scope, ShipNames, Style, name_problem,
};
use starorg_core::fleet::{Catalog, Hull};
use starorg_core::ini::fold_key;
use starorg_core::state::Override;

fn err(code: ErrorCode, m: impl Into<String>) -> AppError {
    AppError::new(code, m)
}

pub fn style(s: Style) -> NameStyle {
    match s {
        Style::None => NameStyle::None,
        Style::Numeric => NameStyle::Numeric,
        Style::GroupLetter => NameStyle::GroupLetter,
        Style::Symbol => NameStyle::Symbol,
    }
}

fn core_style(s: NameStyle) -> Style {
    match s {
        NameStyle::None => Style::None,
        NameStyle::Numeric => Style::Numeric,
        NameStyle::GroupLetter => Style::GroupLetter,
        NameStyle::Symbol => Style::Symbol,
    }
}

pub fn scope(s: Scope) -> ShipNamesScope {
    match s {
        Scope::Shared => ShipNamesScope::Shared,
        Scope::Channel => ShipNamesScope::Channel,
        Scope::Off => ShipNamesScope::Off,
    }
}

pub fn core_scope(s: ShipNamesScope) -> Scope {
    match s {
        ShipNamesScope::Shared => Scope::Shared,
        ShipNamesScope::Channel => Scope::Channel,
        ShipNamesScope::Off => Scope::Off,
    }
}

pub fn format(f: import::Format) -> FleetFormat {
    match f {
        import::Format::XplorJson => FleetFormat::XplorJson,
        import::Format::XplorCsv => FleetFormat::XplorCsv,
        import::Format::Text => FleetFormat::Text,
    }
}

pub fn to_contract(s: &ShipNames) -> ShipNamesSettings {
    ShipNamesSettings {
        enabled: s.enabled,
        style: style(s.style),
        separator: s.separator.clone(),
        symbol: s.symbol.clone(),
        rest: match s.rest {
            Rest::Stock => RestMode::Stock,
            Rest::Numbered => RestMode::Numbered,
        },
        prefix_short: s.prefix_short,
        groups: s
            .groups
            .iter()
            .map(|g| ShipGroup {
                id: g.id.clone(),
                name: g.name.clone(),
            })
            .collect(),
        ships: s
            .ships
            .iter()
            .map(|e| ShipEntry {
                hull: e.hull.clone(),
                group: e.group.clone(),
                name: e.name.clone(),
                short: e.short.clone(),
            })
            .collect(),
    }
}

/// Refuse what the core would drop (the UI checks the same; this is the backstop), then
/// convert. Empty renames mean "reset to stock".
pub fn to_core(s: &ShipNamesSettings) -> CmdResult<ShipNames> {
    if !store::SEPARATORS.contains(&s.separator.as_str()) {
        return Err(err(
            ErrorCode::Invalid,
            "separator: one of \" \", \"-\", \". \"",
        ));
    }
    if !store::SYMBOLS.contains(&s.symbol.as_str()) {
        return Err(err(ErrorCode::Invalid, "symbol: one of ! # * + - ."));
    }
    let clean = |v: &Option<String>, what: &str| -> CmdResult<Option<String>> {
        match v.as_deref().map(str::trim) {
            None | Some("") => Ok(None),
            Some(x) => match name_problem(x) {
                Some(why) => Err(err(ErrorCode::Invalid, format!("{what} {x:?}: {why}"))),
                None => Ok(Some(x.to_string())),
            },
        }
    };
    let mut ids = std::collections::BTreeSet::new();
    let mut groups = Vec::new();
    for g in &s.groups {
        if g.id.trim().is_empty() || !ids.insert(g.id.clone()) {
            return Err(err(ErrorCode::Invalid, format!("group id {:?}", g.id)));
        }
        let name = g.name.trim();
        if let Some(why) = name_problem(name) {
            return Err(err(
                ErrorCode::Invalid,
                format!("group name {name:?}: {why}"),
            ));
        }
        groups.push(Group {
            id: g.id.clone(),
            name: name.into(),
            extra: Default::default(),
        });
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut ships = Vec::new();
    for e in &s.ships {
        if e.hull.trim().is_empty() || !seen.insert(fold_key(&e.hull)) {
            return Err(err(
                ErrorCode::Invalid,
                format!("ship {:?} listed twice", e.hull),
            ));
        }
        ships.push(store::ShipEntry {
            hull: e.hull.clone(),
            group: e.group.clone().filter(|g| ids.contains(g)),
            name: clean(&e.name, "name")?,
            short: clean(&e.short, "short name")?,
            extra: Default::default(),
        });
    }
    let out = ShipNames {
        enabled: s.enabled,
        style: core_style(s.style),
        separator: s.separator.clone(),
        symbol: s.symbol.clone(),
        rest: match s.rest {
            RestMode::Stock => Rest::Stock,
            RestMode::Numbered => Rest::Numbered,
        },
        prefix_short: s.prefix_short,
        groups,
        ships,
        ..ShipNames::default()
    };
    if out.style == Style::GroupLetter && out.block_count() > store::MAX_BLOCKS {
        return Err(err(
            ErrorCode::Invalid,
            "letters A-Z name at most 26 blocks; use fewer groups or another style",
        ));
    }
    Ok(out)
}

pub fn hull(h: &Hull) -> FleetHull {
    FleetHull {
        hull: h.hull.clone(),
        name: h.name.clone(),
        short: h.short.clone(),
        maker: h.maker.clone(),
        ground: h.ground,
        npc_shared: h.npc_shared,
    }
}

fn pending(p: &store::PendingRow) -> PendingShip {
    PendingShip {
        name: p.name.clone(),
        count: p.count,
        maker: p.maker.clone(),
        names: p.names.clone(),
        candidates: p.candidates.clone(),
    }
}

/// Overrides on the name keys of the catalogue's hulls (last line wins, as in merge).
fn hull_overrides(cat: &Catalog, overrides: &[Override]) -> Vec<HullOverride> {
    let find = |k: &str| {
        let f = fold_key(k);
        overrides
            .iter()
            .rev()
            .find(|o| fold_key(&o.key) == f)
            .map(|o| o.value.clone())
    };
    cat.flyable()
        .filter_map(|h| {
            let long = find(&h.hull);
            let short = h.short_key.as_deref().and_then(find);
            (long.is_some() || short.is_some()).then(|| HullOverride {
                hull: h.hull.clone(),
                long,
                short,
            })
        })
        .collect()
}

pub struct ViewParts<'a> {
    pub channel: &'a str,
    pub catalog: Option<&'a Catalog>,
    pub fleet: &'a FleetFile,
    pub effective: &'a Effective,
    /// The shared settings, shown for `off`.
    pub shared: &'a ShipNames,
    pub overrides: &'a [Override],
    pub fleet_file: String,
    pub settings_file: String,
}

pub fn view(p: ViewParts<'_>) -> FleetView {
    let e = p.effective;
    let shared_settings = to_contract(&p.shared.validated().0);
    let settings = if e.scope == Scope::Off {
        shared_settings.clone()
    } else {
        to_contract(&e.settings)
    };
    FleetView {
        channel: p.channel.to_string(),
        catalog_ready: p.catalog.is_some_and(|c| !c.is_empty()),
        catalog: p
            .catalog
            .map(|c| c.flyable().map(hull).collect())
            .unwrap_or_default(),
        owned: p
            .fleet
            .hulls
            .iter()
            .map(|h| OwnedHull {
                hull: h.hull.clone(),
                count: h.count,
                names: h.names.clone(),
            })
            .collect(),
        pending: p.fleet.pending.iter().map(pending).collect(),
        not_in_game: p
            .fleet
            .matches
            .iter()
            .filter(|(_, v)| v.is_none())
            .map(|(k, _)| k.clone())
            .collect(),
        imported_at: p.fleet.imported_at.clone(),
        format: p
            .fleet
            .format
            .as_deref()
            .and_then(import::Format::parse)
            .map(format),
        scope: scope(e.scope),
        settings,
        shared_settings,
        overrides: p
            .catalog
            .map(|c| hull_overrides(c, p.overrides))
            .unwrap_or_default(),
        warnings: e.warnings.iter().map(|w| w.detail.clone()).collect(),
        invalid: e.invalid,
        fleet_file: p.fleet_file,
        settings_file: p.settings_file,
    }
}

/// `merge` and `prune` together, or a real prune that matched no ship (it would empty My
/// ships): refused before anything is written.
pub fn check_prune(req: &FleetImportRequest, o: &Outcome) -> CmdResult<()> {
    if !req.prune {
        return Ok(());
    }
    if req.merge {
        return Err(err(
            ErrorCode::Invalid,
            "\"Remove all ships first\" replaces the fleet; it cannot also add to it.",
        ));
    }
    if !req.dry_run && o.report.hulls.is_empty() {
        return Err(err(
            ErrorCode::Invalid,
            "No ship matched, so My ships would be emptied: nothing removed or imported.",
        ));
    }
    Ok(())
}

/// "Remove all ships first": drop from `list` the ships the imported fleet does not have.
pub fn prune_order(list: &mut ShipNames, o: &Outcome, cat: &Catalog) -> Vec<RemovedShip> {
    let keep: Vec<String> = o.fleet.hulls.iter().map(|h| h.hull.clone()).collect();
    list.prune_to(&keep)
        .into_iter()
        .map(|e| RemovedShip {
            name: cat
                .get(&e.hull)
                .map(|h| h.name.trim().to_string())
                .unwrap_or_else(|| e.hull.clone()),
            rename: e.name,
            hull: e.hull,
        })
        .collect()
}

pub fn report(
    o: &Outcome,
    file: Option<String>,
    dry_run: bool,
    appended: usize,
    removed: Option<Vec<RemovedShip>>,
) -> FleetImportReport {
    let r = &o.report;
    let find = |name: &str| o.fleet.pending.iter().find(|p| p.name == name).map(pending);
    let pend = |list: &[import::PendingReport]| -> Vec<PendingShip> {
        list.iter()
            .map(|p| {
                find(&p.name).unwrap_or(PendingShip {
                    name: p.name.clone(),
                    count: p.count,
                    maker: None,
                    names: Vec::new(),
                    candidates: p.candidates.clone(),
                })
            })
            .collect()
    };
    FleetImportReport {
        file,
        format: format(r.format),
        dry_run,
        merge: r.merge,
        total: r.total as u32,
        skipped: r.skipped as u32,
        hulls: r
            .hulls
            .iter()
            .map(|h| ImportedHull {
                hull: h.hull.clone(),
                name: h.name.clone(),
                count: h.count,
                names: h.names.clone(),
                new: h.new,
            })
            .collect(),
        unmatched: pend(&r.unmatched),
        ambiguous: pend(&r.ambiguous),
        not_in_game: r.not_in_game as u32,
        empty_catalog: r.empty_catalog,
        added_to_order: appended as u32,
        prune: removed.is_some(),
        removed_from_order: removed.unwrap_or_default(),
    }
}

/// Bytes of the import: pasted text, or what the caller read from a file.
pub fn parse(bytes: &[u8]) -> CmdResult<import::Parsed> {
    import::parse(bytes, None).map_err(|e| err(ErrorCode::Invalid, e))
}

/// A pending row matched by hand: the hull must be a flyable hull of the build.
pub fn match_target<'c>(cat: &'c Catalog, hull: Option<&str>) -> CmdResult<Option<&'c Hull>> {
    match hull {
        None => Ok(None),
        Some(id) => cat.get(id).filter(|h| h.flyable).map(Some).ok_or_else(|| {
            err(
                ErrorCode::Invalid,
                format!("{id} is not a ship of this build"),
            )
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_refusals() {
        let s = ShipNamesSettings {
            enabled: true,
            style: NameStyle::GroupLetter,
            separator: ". ".into(),
            symbol: "#".into(),
            rest: RestMode::Numbered,
            prefix_short: true,
            groups: vec![ShipGroup {
                id: "combat".into(),
                name: " Combat ".into(),
            }],
            ships: vec![ShipEntry {
                hull: "vehicle_NameA".into(),
                group: Some("combat".into()),
                name: Some("  ".into()),
                short: Some("Kite".into()),
            }],
        };
        let c = to_core(&s).unwrap();
        assert_eq!(c.groups[0].name, "Combat");
        assert_eq!(c.ships[0].name, None, "blank rename = stock");
        let back = to_contract(&c);
        assert_eq!(back.style, NameStyle::GroupLetter);
        assert_eq!(back.ships[0].short.as_deref(), Some("Kite"));
        let mut bad = s.clone();
        bad.ships[0].name = Some("50% off".into());
        assert_eq!(to_core(&bad).unwrap_err().code, ErrorCode::Invalid);
        let mut bad = s.clone();
        bad.separator = "_".into();
        assert!(to_core(&bad).is_err());
        let mut bad = s;
        bad.ships.push(bad.ships[0].clone());
        assert!(to_core(&bad).is_err());
    }
}
