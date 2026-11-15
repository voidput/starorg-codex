//! The group hierarchy and the tables that map game data onto it (addendum 8.3).
//!
//! Everything here is data: the hierarchy, entity item types, DataCore struct types and key
//! prefixes. [`super::assign`] only walks these tables, so a new game type is a new row,
//! not a new `match` arm.

/// One node of the two-level hierarchy. Ids are stable strings (`components/coolers`); a
/// top-level group without children is its own leaf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupDef {
    pub id: &'static str,
    pub parent: Option<&'static str>,
    pub label: &'static str,
}

const fn top(id: &'static str, label: &'static str) -> GroupDef {
    GroupDef {
        id,
        parent: None,
        label,
    }
}

const fn sub(parent: &'static str, id: &'static str, label: &'static str) -> GroupDef {
    GroupDef {
        id,
        parent: Some(parent),
        label,
    }
}

/// The hierarchy in display order: each parent, then its children.
pub const HIERARCHY: &[GroupDef] = &[
    top("ships", "Ships & vehicles"),
    sub("ships", "ships/ships", "Ships"),
    sub("ships", "ships/ground", "Ground vehicles"),
    sub("ships", "ships/paints", "Paints"),
    top("components", "Ship components"),
    sub("components", "components/coolers", "Coolers"),
    sub("components", "components/power-plants", "Power plants"),
    sub("components", "components/shields", "Shields"),
    sub("components", "components/quantum-drives", "Quantum drives"),
    sub("components", "components/jump-drives", "Jump drives"),
    sub("components", "components/radars", "Radars"),
    sub("components", "components/thrusters", "Thrusters"),
    sub("components", "components/fuel", "Fuel tanks & intakes"),
    sub("components", "components/armor", "Armor"),
    sub("components", "components/turrets", "Turrets & mounts"),
    sub("components", "components/missile-racks", "Missile racks"),
    sub(
        "components",
        "components/mining-salvage",
        "Mining & salvage",
    ),
    sub(
        "components",
        "components/flight",
        "Flight controllers & computers",
    ),
    sub("components", "components/other", "Other components"),
    top("ship-weapons", "Ship weapons"),
    sub("ship-weapons", "ship-weapons/guns", "Guns"),
    sub(
        "ship-weapons",
        "ship-weapons/countermeasures",
        "Countermeasures",
    ),
    top("missiles", "Missiles & bombs"),
    top("fps-weapons", "FPS weapons"),
    sub("fps-weapons", "fps-weapons/weapons", "Weapons"),
    sub("fps-weapons", "fps-weapons/attachments", "Attachments"),
    sub("fps-weapons", "fps-weapons/gadgets", "Gadgets & grenades"),
    top("armor", "Armor"),
    sub("armor", "armor/helmets", "Helmets"),
    sub("armor", "armor/torso", "Torso"),
    sub("armor", "armor/arms", "Arms"),
    sub("armor", "armor/legs", "Legs"),
    sub("armor", "armor/undersuits", "Undersuits"),
    sub("armor", "armor/backpacks", "Backpacks"),
    sub("armor", "armor/other", "Other armor"),
    top("clothing", "Clothing"),
    sub("clothing", "clothing/hats", "Hats"),
    sub("clothing", "clothing/torso", "Shirts & jackets"),
    sub("clothing", "clothing/hands", "Gloves"),
    sub("clothing", "clothing/legs", "Pants"),
    sub("clothing", "clothing/feet", "Shoes"),
    sub("clothing", "clothing/backpacks", "Backpacks"),
    sub("clothing", "clothing/other", "Other clothing"),
    top("consumables", "Consumables"),
    sub("consumables", "consumables/food", "Food"),
    sub("consumables", "consumables/drink", "Drink"),
    sub("consumables", "consumables/medical", "Medical"),
    sub("consumables", "consumables/other", "Other consumables"),
    top("cargo", "Cargo & commodities"),
    sub("cargo", "cargo/cargo", "Cargo"),
    sub("cargo", "cargo/commodities", "Commodities & resources"),
    top("items", "Other items"),
    sub("items", "items/usables", "Usables"),
    sub("items", "items/displays", "Displays & props"),
    sub("items", "items/doors-seats", "Doors & seats"),
    sub("items", "items/misc", "Misc"),
    top("missions", "Missions & contracts"),
    sub("missions", "missions/contracts", "Contracts"),
    sub("missions", "missions/text", "Mission text"),
    sub("missions", "missions/locations", "Mission locations"),
    sub("missions", "missions/givers", "Organizations & givers"),
    top("blueprints", "Blueprints & crafting"),
    top("reputation", "Reputation"),
    top("locations", "Locations"),
    sub("locations", "locations/star-map", "Star map"),
    sub("locations", "locations/areas", "Areas"),
    top("manufacturers", "Manufacturers"),
    top("dialogue", "Dialogue"),
    sub("dialogue", "dialogue/voice", "Voice lines"),
    sub("dialogue", "dialogue/conversations", "Conversations"),
    sub("dialogue", "dialogue/comms", "Comms"),
    top("interface", "Interface"),
    sub("interface", "interface/hud", "HUD"),
    sub("interface", "interface/mobiglas", "mobiGlas"),
    sub("interface", "interface/menus", "Menus & settings"),
    sub("interface", "interface/input", "Input & hints"),
    sub("interface", "interface/chat", "Chat & emotes"),
    sub("interface", "interface/journal", "Journal"),
    sub("interface", "interface/other", "Other interface"),
    top("game-modes", "Game modes"),
    sub(
        "game-modes",
        "game-modes/arena-commander",
        "Arena Commander",
    ),
    sub("game-modes", "game-modes/star-marine", "Star Marine"),
    sub("game-modes", "game-modes/other", "Other modes"),
    top("other", "Other"),
];

/// Where an unknown entity item type goes (and is counted by the drift test).
pub const UNKNOWN_ITEM: &str = "items/misc";
pub const OTHER: &str = "other";

/// How specific a candidate is; lower wins (addendum 8.2 rule 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    Item = 0,
    Mission = 1,
    Blueprint = 2,
    Reputation = 3,
    Location = 4,
    Interface = 5,
    Dialogue = 6,
}

/// Entity `AttachDef.Type` (exact, case-insensitive) -> leaf. Checked before the prefix
/// table below.
pub const ITEM_TYPES: &[(&str, &str)] = &[
    ("NOITEM_Vehicle", "ships/ships"),
    ("Paints", "ships/paints"),
    ("Cooler", "components/coolers"),
    ("PowerPlant", "components/power-plants"),
    ("Shield", "components/shields"),
    ("ShieldController", "components/shields"),
    ("QuantumDrive", "components/quantum-drives"),
    ("QuantumInterdictionGenerator", "components/quantum-drives"),
    ("JumpDrive", "components/jump-drives"),
    ("Radar", "components/radars"),
    ("Scanner", "components/radars"),
    ("Ping", "components/radars"),
    ("MainThruster", "components/thrusters"),
    ("ManneuverThruster", "components/thrusters"),
    ("FuelTank", "components/fuel"),
    ("QuantumFuelTank", "components/fuel"),
    ("ExternalFuelTank", "components/fuel"),
    ("FuelIntake", "components/fuel"),
    ("Armor", "components/armor"),
    ("Turret", "components/turrets"),
    ("TurretBase", "components/turrets"),
    ("WeaponMount", "components/turrets"),
    ("UtilityTurret", "components/turrets"),
    ("MissileLauncher", "components/missile-racks"),
    ("BombLauncher", "components/missile-racks"),
    ("GroundVehicleMissileLauncher", "components/missile-racks"),
    ("WeaponMining", "components/mining-salvage"),
    ("MiningModifier", "components/mining-salvage"),
    ("MiningArm", "components/mining-salvage"),
    ("SalvageHead", "components/mining-salvage"),
    ("SalvageModifier", "components/mining-salvage"),
    ("SalvageFieldEmitter", "components/mining-salvage"),
    ("SalvageFieldSupporter", "components/mining-salvage"),
    ("SalvageFillerStation", "components/mining-salvage"),
    ("TractorBeam", "components/mining-salvage"),
    ("TowingBeam", "components/mining-salvage"),
    ("FlightController", "components/flight"),
    ("Avionics", "components/flight"),
    ("Computer", "components/flight"),
    ("SelfDestruct", "components/other"),
    ("LifeSupportGenerator", "components/other"),
    ("LifeSupportVent", "components/other"),
    ("Battery", "components/other"),
    ("EMP", "components/other"),
    ("CargoGrid", "components/other"),
    ("Transponder", "components/other"),
    ("Sensor", "components/other"),
    ("WeaponRegenPool", "components/other"),
    ("WeaponGun", "ship-weapons/guns"),
    ("WeaponDefensive", "ship-weapons/countermeasures"),
    ("Missile", "missiles"),
    ("Bomb", "missiles"),
    ("GroundVehicleMissile", "missiles"),
    ("WeaponPersonal", "fps-weapons/weapons"),
    ("WeaponAttachment", "fps-weapons/attachments"),
    ("Gadget", "fps-weapons/gadgets"),
    ("Grenade", "fps-weapons/gadgets"),
    ("Char_Armor_Helmet", "armor/helmets"),
    ("Char_Armor_Torso", "armor/torso"),
    ("Char_Armor_Arms", "armor/arms"),
    ("Char_Armor_Legs", "armor/legs"),
    ("Char_Armor_Undersuit", "armor/undersuits"),
    ("Char_Armor_Backpack", "armor/backpacks"),
    ("Char_Clothing_Hat", "clothing/hats"),
    ("Char_Clothing_Torso_0", "clothing/torso"),
    ("Char_Clothing_Torso_1", "clothing/torso"),
    ("Char_Clothing_Torso_2", "clothing/torso"),
    ("Char_Clothing_Hands", "clothing/hands"),
    ("Char_Clothing_Legs", "clothing/legs"),
    ("Char_Clothing_Feet", "clothing/feet"),
    ("Char_Clothing_Backpack", "clothing/backpacks"),
    ("Food", "consumables/food"),
    ("Drink", "consumables/drink"),
    ("Bottle", "consumables/drink"),
    ("Cargo", "cargo/cargo"),
    ("ShipCargo", "cargo/cargo"),
    ("Usable", "items/usables"),
    ("Display", "items/displays"),
    ("Door", "items/doors-seats"),
    ("Seat", "items/doors-seats"),
    ("SeatAccess", "items/doors-seats"),
    ("SeatDashboard", "items/doors-seats"),
    ("ControlPanel", "items/usables"),
    ("Button", "items/usables"),
    ("Elevator", "items/usables"),
    ("Crafter", "items/usables"),
    ("Container", "cargo/cargo"),
    ("InventoryContainer", "cargo/cargo"),
    ("Module", "components/other"),
    ("DockingCollar", "components/other"),
    ("DockingAnimator", "components/other"),
    ("LandingSystem", "components/other"),
    ("GravityGenerator", "components/other"),
    ("Relay", "components/other"),
    ("ToolArm", "components/mining-salvage"),
    ("SpaceMine", "missiles"),
    ("Suit", "armor/other"),
    ("Visor", "armor/other"),
    ("MobiGlas", "armor/other"),
    ("RemovableChip", "items/misc"),
    ("Currency", "items/misc"),
    ("Misc", "items/misc"),
    ("UNDEFINED", "items/misc"),
    ("NOITEM_Player", "items/misc"),
];

/// Entity item type prefixes (case-insensitive) for the families with many members.
pub const ITEM_TYPE_PREFIXES: &[(&str, &str)] = &[
    ("Char_Armor_", "armor/other"),
    ("Char_Clothing_", "clothing/other"),
    ("Char_", "clothing/other"),
    ("Seat", "items/doors-seats"),
    ("Door", "items/doors-seats"),
    ("Flair_", "items/displays"),
    ("Display", "items/displays"),
    ("Light", "items/displays"),
    ("Thruster", "components/thrusters"),
    ("Salvage", "components/mining-salvage"),
    ("Mining", "components/mining-salvage"),
    ("Weapon", "fps-weapons/weapons"),
];

/// `FPS_Consumable` and similar: medical sub-types (case-insensitive substring of SubType).
pub const MEDICAL_SUBTYPES: &[&str] = &["medical", "medpack", "medpen", "oxygen", "hacking"];

/// DataCore struct type (exact) -> leaf and rank, for records that are not entities.
pub const STRUCT_TYPES: &[(&str, &str, Rank)] = &[
    ("ContractGenerator", "missions/contracts", Rank::Mission),
    ("ContractTemplate", "missions/contracts", Rank::Mission),
    ("MissionBrokerEntry", "missions/text", Rank::Mission),
    (
        "MissionLocationTemplate",
        "missions/locations",
        Rank::Mission,
    ),
    ("MissionOrganization", "missions/givers", Rank::Mission),
    ("MissionGiver", "missions/givers", Rank::Mission),
    ("CraftingBlueprintRecord", "blueprints", Rank::Blueprint),
    (
        "CraftingBlueprintCategoryRecord",
        "blueprints",
        Rank::Blueprint,
    ),
    ("FactionReputation", "reputation", Rank::Reputation),
    ("SReputationStandingParams", "reputation", Rank::Reputation),
    ("SReputationScopeParams", "reputation", Rank::Reputation),
    ("Faction", "reputation", Rank::Reputation),
    ("StarMapObject", "locations/star-map", Rank::Location),
    ("StarMapObjectType", "locations/star-map", Rank::Location),
    ("SCItemManufacturer", "manufacturers", Rank::Location),
    ("ResourceType", "cargo/commodities", Rank::Location),
    ("ResourceTypeDatabase", "cargo/commodities", Rank::Location),
    ("ResourceTypeGroup", "cargo/commodities", Rank::Location),
    ("GameMode", "game-modes/other", Rank::Interface),
    ("HintUIData", "interface/input", Rank::Interface),
    ("ControlHints_Preset", "interface/input", Rank::Interface),
    ("MobiGlasAppData", "interface/mobiglas", Rank::Interface),
    ("ChatEmoteRecord", "interface/chat", Rank::Interface),
    ("JournalEntry", "interface/journal", Rank::Interface),
    ("BuildingBlocks_Canvas", "interface/other", Rank::Interface),
    ("DialogueContextBank", "dialogue/voice", Rank::Dialogue),
    (
        "DialogueContentBank",
        "dialogue/conversations",
        Rank::Dialogue,
    ),
    ("CommsNotification", "dialogue/comms", Rank::Dialogue),
];

/// Struct type prefixes (exact case) for the long families.
pub const STRUCT_PREFIXES: &[(&str, &str, Rank)] = &[
    ("Mission", "missions/text", Rank::Mission),
    ("Contract", "missions/contracts", Rank::Mission),
    ("Crafting", "blueprints", Rank::Blueprint),
    ("SReputation", "reputation", Rank::Reputation),
    ("Reputation", "reputation", Rank::Reputation),
    ("StarMap", "locations/star-map", Rank::Location),
    ("Comms", "dialogue/comms", Rank::Dialogue),
    ("Dialogue", "dialogue/conversations", Rank::Dialogue),
];

/// Canvas sub-groups by record path fragment (`BuildingBlocks_Canvas` is used by every UI).
pub const CANVAS_PATHS: &[(&str, &str)] = &[
    ("hud", "interface/hud"),
    ("visor", "interface/hud"),
    ("mobiglas", "interface/mobiglas"),
    ("menu", "interface/menus"),
    ("frontend", "interface/menus"),
    ("chat", "interface/chat"),
];

/// Key first token (lower case, before the first `_`) -> leaf, for keys no record uses.
pub const KEY_PREFIXES: &[(&str, &str)] = &[
    ("vehicle", "ships/ships"),
    ("paint", "ships/paints"),
    ("itemport", "components/other"),
    ("human", "dialogue/voice"),
    ("dlg", "dialogue/conversations"),
    ("dxsh", "dialogue/voice"),
    ("dxsm", "dialogue/voice"),
    ("pu", "dialogue/voice"),
    ("ph", "dialogue/voice"),
    // Inner Thought is the interaction prompt menu ("Force Open", "To Cargo Room").
    ("innerthought", "interface/input"),
    ("inner", "interface/input"),
    ("comm", "dialogue/comms"),
    ("commarray", "dialogue/comms"),
    ("mission", "missions/text"),
    ("missionmanager", "missions/text"),
    ("mg", "missions/text"),
    ("contract", "missions/contracts"),
    ("contracts", "missions/contracts"),
    ("bounty", "missions/text"),
    ("escort", "missions/text"),
    ("delivery", "missions/text"),
    ("assassin", "missions/text"),
    ("recoverstolen", "missions/text"),
    ("headhunters", "missions/text"),
    ("cfp", "missions/text"),
    ("dynamicevent", "missions/text"),
    ("event", "missions/text"),
    ("planetcollect", "missions/text"),
    ("thecollector", "missions/text"),
    ("dataheist", "missions/text"),
    ("crafting", "blueprints"),
    ("blueprint", "blueprints"),
    ("repstanding", "reputation"),
    ("reputation", "reputation"),
    ("manufacturer", "manufacturers"),
    ("area", "locations/areas"),
    // `port_Name*` are ship hardpoint names ("Power Plant 13"), like `itemPort_*`.
    ("port", "components/other"),
    ("crus", "locations/areas"),
    ("crusader", "locations/areas"),
    ("hurston", "locations/areas"),
    ("microtech", "locations/areas"),
    ("arccorp", "locations/areas"),
    ("hockrow", "locations/areas"),
    ("nyx", "locations/star-map"),
    ("pyro", "locations/star-map"),
    ("stanton", "locations/star-map"),
    ("asteroidcluster", "locations/star-map"),
    ("ui", "interface/menus"),
    ("frontend", "interface/menus"),
    ("pause", "interface/menus"),
    ("text", "interface/menus"),
    ("shipselector", "interface/menus"),
    ("notification", "interface/menus"),
    ("shop", "interface/menus"),
    ("kiosk", "interface/menus"),
    ("hud", "interface/hud"),
    ("mobiglas", "interface/mobiglas"),
    ("maps", "interface/mobiglas"),
    ("input", "interface/input"),
    ("hints", "interface/input"),
    ("hint", "interface/input"),
    ("interaction", "interface/input"),
    ("chat", "interface/chat"),
    ("journal", "interface/journal"),
    ("dfm", "game-modes/arena-commander"),
    ("ea", "game-modes/arena-commander"),
    ("ac", "game-modes/arena-commander"),
    ("sm", "game-modes/star-marine"),
    ("soo2", "game-modes/other"),
    ("tut", "game-modes/other"),
    ("fw", "missions/text"),
];

/// Key first tokens of shared hardpoint names: any record that uses one says nothing about
/// the key's manufacturer or size.
pub const NO_FACET_PREFIXES: &[&str] = &["port", "itemport"];

/// Top-level groups of screen and voice text: a key-prefix rule into one of these beats an
/// entity that merely displays the key (`hud_Label_On` on a usable, `ui_Close` on a crate).
/// Mission, location and crafting prefixes do not: `Hockrow_..._Sample04_Name` is the name of
/// a mission crate, and `crafting_machineName_*` of a fabricator.
pub const TEXT_TOPS: &[&str] = &["interface", "game-modes", "dialogue"];

/// Where the size facet means something: ships, ship parts, missiles, FPS weapons (S1
/// pistols to S5 launchers) and their attachments.
/// Everywhere else (`AttachDef.Size` defaults to 1 on paints, clothing, props) it is noise.
pub const SIZE_GROUPS: &[&str] = &[
    "ships/ships",
    "ships/ground",
    "components",
    "ship-weapons",
    "missiles",
    "fps-weapons/weapons",
    "fps-weapons/attachments",
];

/// First tokens with a trailing number (`stanton2b`, `pyro5`) match these stems.
pub const NUMBERED_STEMS: &[&str] = &["stanton", "pyro", "nyx", "tut", "fw"];

/// True when `leaf` (or its top-level group) is listed in [`SIZE_GROUPS`].
pub fn has_size(leaf: &str) -> bool {
    SIZE_GROUPS
        .iter()
        .any(|g| leaf == *g || leaf.strip_prefix(g).is_some_and(|r| r.starts_with('/')))
}

/// True when `leaf` sits under one of [`TEXT_TOPS`].
pub fn is_text(leaf: &str) -> bool {
    let top = leaf.split('/').next().unwrap_or_default();
    TEXT_TOPS.contains(&top)
}

pub fn find_item_type(item_type: &str) -> Option<&'static str> {
    ITEM_TYPES
        .iter()
        .find(|(t, _)| t.eq_ignore_ascii_case(item_type))
        .map(|(_, g)| *g)
        .or_else(|| {
            let lower = item_type.to_ascii_lowercase();
            ITEM_TYPE_PREFIXES
                .iter()
                .find(|(p, _)| lower.starts_with(&p.to_ascii_lowercase()))
                .map(|(_, g)| *g)
        })
}

pub fn find_struct(struct_type: &str) -> Option<(&'static str, Rank)> {
    STRUCT_TYPES
        .iter()
        .find(|(t, _, _)| *t == struct_type)
        .map(|(_, g, r)| (*g, *r))
        .or_else(|| {
            STRUCT_PREFIXES
                .iter()
                .find(|(p, _, _)| struct_type.starts_with(p))
                .map(|(_, g, r)| (*g, *r))
        })
}

/// The leaf for an unreferenced key from its first token. `stanton2b`, `pyro5`, `fw2022`:
/// a numbered first token also matches its stem when the stem is in [`NUMBERED_STEMS`].
pub fn find_prefix(folded_key: &str) -> Option<&'static str> {
    let token = folded_key
        .split(['_', '-', '.', ' ', ','])
        .next()
        .unwrap_or_default();
    let lookup = |t: &str| KEY_PREFIXES.iter().find(|(p, _)| *p == t).map(|(_, g)| *g);
    if let Some(g) = lookup(token) {
        return Some(g);
    }
    let no_letter = token
        .strip_suffix(|c: char| c.is_ascii_lowercase())
        .filter(|t| t.ends_with(|c: char| c.is_ascii_digit()))
        .unwrap_or(token);
    let stem = no_letter.trim_end_matches(|c: char| c.is_ascii_digit());
    if stem.len() < token.len() && NUMBERED_STEMS.contains(&stem) {
        return lookup(stem);
    }
    None
}
