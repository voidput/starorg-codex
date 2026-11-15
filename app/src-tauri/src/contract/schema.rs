//! Shared, backend-independent pieces of the contract: the preset profiles (M2's
//! `enhance::Profile::preset` values in contract form) and the labels for the core's stat line
//! and tag element ids. Both backends answer `preset_profile` and `profile_schema` from here.

use super::types::*;

pub(crate) fn tag_kind(elements: &[&str], styles: &[(&str, TagStyle)]) -> TagKindSettings {
    TagKindSettings {
        enabled: true,
        elements: elements.iter().map(|e| e.to_string()).collect(),
        styles: styles.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
        separator: TagSeparator::Dash,
        enclosing: TagEnclosing::Square,
        placement: TagPlacement::Prefix,
    }
}

/// The profile a preset starts from: the same values as M2's `Profile::preset`.
pub fn preset_profile(p: Preset) -> Profile {
    let cats = p != Preset::Minimal;
    let all = p == Preset::Everything;
    Profile {
        preset: Some(p),
        ships: cats,
        components: cats,
        ship_weapons: cats,
        fps_weapons: cats,
        missiles: cats,
        missions: cats,
        lines_off: Vec::new(),
        stats_above: false,
        header: "--- STATS ---".into(),
        tags: TagSettings {
            components: tag_kind(&["class", "size", "grade"], &[]),
            ship_weapons: tag_kind(&["damage", "size"], &[("damage", TagStyle::Short)]),
            missiles: tag_kind(&["tracking", "size"], &[]),
            short_names: all,
            tool_size_tag: all,
        },
        mission: MissionSettings {
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
        },
        branding: all,
        patches_disabled: Vec::new(),
    }
}

/// Labels for the stat line ids M2's generators read (`enhance::ships`, `enhance::items`).
pub fn profile_schema() -> ProfileSchema {
    type Lines = &'static [(&'static str, &'static str)];
    let cat =
        |id: CategoryId, label: &str, blurb: &str, prefix: &str, lines: Lines| CategorySchema {
            id,
            label: label.into(),
            blurb: blurb.into(),
            lines: lines
                .iter()
                .map(|(l, t)| LineSchema {
                    key: format!("{prefix}.{l}"),
                    label: (*t).into(),
                })
                .collect(),
        };
    let el = |els: &[(&str, &str)]| {
        els.iter()
            .map(|(id, label)| ElementSchema {
                id: (*id).into(),
                label: (*label).into(),
            })
            .collect()
    };
    ProfileSchema {
        categories: vec![
            cat(
                CategoryId::Ships,
                "Ships",
                "A stats block in ship descriptions.",
                "ships",
                &[
                    ("role", "Role, career and crew"),
                    ("size", "Size"),
                    ("speed", "SCM and max speed"),
                    ("boost", "Boost speeds"),
                    ("rotation", "Pitch, yaw and roll"),
                    ("weapons", "Weapons"),
                    ("components", "Components"),
                    ("armor", "Armor"),
                    ("damage_taken", "Damage taken"),
                    ("cargo", "Cargo"),
                ],
            ),
            cat(
                CategoryId::Components,
                "Components",
                "Stats in shields, coolers, power plants, quantum drives and radars.",
                "components",
                &[
                    ("health", "Health"),
                    ("power", "Power use"),
                    ("signatures", "EM and IR signatures"),
                    ("shield", "Shield HP and regen"),
                    ("delay", "Regen delay"),
                    ("resist", "Resistances"),
                    ("output", "Power output"),
                    ("cooling", "Cooling"),
                    ("speed", "Quantum speed"),
                    ("fuel", "Quantum fuel use"),
                    ("accel", "Quantum spool and acceleration"),
                    ("calibration", "Calibration"),
                    ("ping", "Radar range"),
                    ("slots", "Missile rack slots"),
                ],
            ),
            cat(
                CategoryId::ShipWeapons,
                "Ship weapons",
                "Damage, fire rate and range in ship weapon descriptions.",
                "ship_weapons",
                &[
                    ("damage", "Damage and DPS"),
                    ("rate", "Fire rate"),
                    ("velocity", "Projectile speed and range"),
                    ("ammo", "Ammo"),
                ],
            ),
            cat(
                CategoryId::FpsWeapons,
                "FPS weapons",
                "Damage, fire rate and magazine in personal weapon descriptions.",
                "fps_weapons",
                &[
                    ("damage", "Damage"),
                    ("rate", "Fire rate"),
                    ("velocity", "Projectile speed"),
                    ("falloff", "Damage falloff"),
                    ("magazine", "Magazine"),
                    ("modifiers", "Attachment modifiers"),
                ],
            ),
            cat(
                CategoryId::Missiles,
                "Missiles",
                "Damage, lock and speed in missile and bomb descriptions.",
                "missiles",
                &[
                    ("damage", "Damage"),
                    ("tracking", "Tracking and lock time"),
                    ("lock_range", "Lock range"),
                    ("speed", "Speed"),
                    ("health", "Health"),
                ],
            ),
            cat(
                CategoryId::Missions,
                "Missions",
                "Tags in contract titles and sections in their descriptions.",
                "missions",
                &[],
            ),
        ],
        tag_kinds: vec![
            TagKindSchema {
                kind: TagKind::Components,
                label: "Ship components".into(),
                elements: el(&[("class", "Class"), ("size", "Size"), ("grade", "Grade")]),
            },
            TagKindSchema {
                kind: TagKind::ShipWeapons,
                label: "Ship weapons".into(),
                elements: el(&[("damage", "Damage type"), ("size", "Size")]),
            },
            TagKindSchema {
                kind: TagKind::Missiles,
                label: "Missiles".into(),
                elements: el(&[("tracking", "Tracking"), ("size", "Size")]),
            },
        ],
    }
}
