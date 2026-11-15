//! Blueprint finder in contract form (addendum §7.19), shared by the real and the mock
//! backend: both run the core merge (`blueprints::finder::build`); only where the catalogue
//! and the owned sets come from differs (cached game data and the channel data folders vs a
//! synthetic build in memory).

use super::types::*;
use starorg_core::blueprints::finder::{self, Kind};

pub fn kind(k: Kind) -> BlueprintKind {
    match k {
        Kind::ShipComponent => BlueprintKind::ShipComponent,
        Kind::ShipWeapon => BlueprintKind::ShipWeapon,
        Kind::FpsWeapon => BlueprintKind::FpsWeapon,
        Kind::Armor => BlueprintKind::Armor,
        Kind::FpsAmmo => BlueprintKind::FpsAmmo,
        Kind::MissionItem => BlueprintKind::MissionItem,
        Kind::Other => BlueprintKind::Other,
    }
}

pub fn contract(f: finder::Finder) -> BlueprintFinder {
    BlueprintFinder {
        channels: f
            .channels
            .into_iter()
            .map(|c| BlueprintFinderChannel {
                channel: c.channel,
                catalog: c.catalog,
                characters: c.characters,
                owned: c.owned,
            })
            .collect(),
        characters: f
            .characters
            .into_iter()
            .map(|c| BlueprintFinderCharacter {
                id: c.id,
                name: c.name,
                last_seen: c.last_seen,
                channels: c.channels,
            })
            .collect(),
        rows: f
            .rows
            .into_iter()
            .map(|r| BlueprintRow {
                key: r.key,
                name: r.name,
                record: r.record,
                kind: kind(r.kind),
                type_label: r.type_label,
                class: r.class,
                manufacturer: r.manufacturer,
                manufacturer_name: r.manufacturer_name,
                size: r.size.map(u32::from),
                grade: r.grade,
                channels: r.channels,
                owners: r
                    .owners
                    .into_iter()
                    .map(|o| BlueprintOwner {
                        channel: o.channel,
                        character: o.character,
                        sources: o.sources,
                        acquired: o.acquired,
                        favorite: o.favorite,
                        ambiguous: o.ambiguous,
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// The channels a request names, in the install's order (the current one first when the
/// app asks for it first). Empty = all.
pub fn pick<'a>(all: &'a [String], req: &[String]) -> Result<Vec<&'a String>, AppError> {
    if req.is_empty() {
        return Ok(all.iter().collect());
    }
    req.iter()
        .map(|c| {
            all.iter()
                .find(|a| a.eq_ignore_ascii_case(c))
                .ok_or_else(|| {
                    AppError::new(
                        ErrorCode::NotFound,
                        format!("no channel {c} in this install"),
                    )
                })
        })
        .collect()
}
