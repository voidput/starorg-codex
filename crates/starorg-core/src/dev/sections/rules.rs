//! The section table (addendum 7.17): which player-meaningful section a DataCore record
//! belongs to, and how a section is split below it. Everything here is data; the index only
//! walks these tables, so a new game type is a new row.
//!
//! Chosen from the LIVE 4.10.200 `Data.p4k` (118,499 records, 63,440 main, 30,083 entities
//! with 170 `AttachDef.Type` values); see the counts in the addendum.

/// One sub-level of a section's tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Level {
    /// The entity's `AttachDef.Type`, or the struct type of any other record.
    Kind,
    SubType,
    Size,
    Manufacturer,
    /// The DataCore struct type.
    Type,
    /// The record's folder below the records root (at most [`FOLDER_DEPTH`] segments).
    Folder,
}

impl Level {
    /// Stable name (the TS mock reads the table by these).
    pub fn id(self) -> &'static str {
        match self {
            Level::Kind => "kind",
            Level::SubType => "sub-type",
            Level::Size => "size",
            Level::Manufacturer => "manufacturer",
            Level::Type => "type",
            Level::Folder => "folder",
        }
    }
}

/// Folder segments a [`Level::Folder`] key keeps.
pub const FOLDER_DEPTH: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionDef {
    pub id: &'static str,
    pub label: &'static str,
    pub levels: &'static [Level],
}

use Level::*;

const fn s(id: &'static str, label: &'static str, levels: &'static [Level]) -> SectionDef {
    SectionDef { id, label, levels }
}

/// Every section, in display order. The last one ([`OTHER`]) catches the rest.
pub const SECTIONS: &[SectionDef] = &[
    s("ships", "Ships & vehicles", &[Kind, SubType, Manufacturer]),
    s("ship-weapons", "Ship weapons", &[Kind, Size, Manufacturer]),
    s("components", "Ship components", &[Kind, Size, Manufacturer]),
    s(
        "missiles",
        "Missiles & torpedoes",
        &[Kind, Size, Manufacturer],
    ),
    s("fps-weapons", "FPS weapons", &[Kind, SubType, Manufacturer]),
    s("armor", "Armor & clothing", &[Kind, Manufacturer]),
    s("attachments", "Attachments", &[SubType, Size]),
    s("consumables", "Consumables & medical", &[Kind, SubType]),
    s("mining", "Mining & salvage", &[Kind, Size]),
    s("commodities", "Commodities & resources", &[Kind, Folder]),
    s("missions", "Missions & contracts", &[Type, Folder]),
    s("locations", "Locations & star map", &[Type, Folder]),
    s("factions", "Factions & reputation", &[Type, Folder]),
    s("blueprints", "Blueprints & crafting", &[Type, Folder]),
    s("loadouts", "Loadouts & loot", &[Type, Folder]),
    s("shops", "Shops", &[Kind, Folder]),
    s("characters", "Characters & NPCs", &[Kind, Folder]),
    s("items", "Props & other items", &[Kind, SubType]),
    s("audio-ui", "Audio, UI & dialogue", &[Type, Folder]),
    s("other", "Other", &[Type, Folder]),
];

pub const OTHER: &str = "other";
/// Entities no rule names land here (they are still items of the game world).
pub const ENTITY_FALLBACK: &str = "items";

/// What a rule looks at. Item and struct names compare case-insensitively; paths are the
/// lower-case path below the records root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Match {
    /// Entity `AttachDef.Type`, exact.
    Item(&'static str),
    /// Entity `AttachDef.Type`, prefix.
    ItemPrefix(&'static str),
    /// Struct type, exact.
    Struct(&'static str),
    /// Struct type, prefix.
    StructPrefix(&'static str),
    /// Path prefix below the records root.
    Path(&'static str),
}

use Match::*;

/// The rules, first hit wins. Entity item types first, then entity folders (entities
/// without a type), then struct types, then folders.
pub const RULES: &[(Match, &str)] = &[
    // --- Entities by AttachDef.Type ---------------------------------------------------
    (Item("NOITEM_Vehicle"), "ships"),
    (Item("Paints"), "ships"),
    (Item("Decal"), "ships"),
    (Item("WeaponGun"), "ship-weapons"),
    (Item("WeaponDefensive"), "ship-weapons"),
    (Item("WeaponMount"), "ship-weapons"),
    (Item("Turret"), "ship-weapons"),
    (Item("TurretBase"), "ship-weapons"),
    (Item("UtilityTurret"), "ship-weapons"),
    (Item("WeaponController"), "ship-weapons"),
    (Item("Missile"), "missiles"),
    (Item("Bomb"), "missiles"),
    (Item("GroundVehicleMissile"), "missiles"),
    (Item("SpaceMine"), "missiles"),
    (Item("MissileLauncher"), "missiles"),
    (Item("BombLauncher"), "missiles"),
    (Item("GroundVehicleMissileLauncher"), "missiles"),
    (Item("MissileController"), "missiles"),
    (Item("WeaponMining"), "mining"),
    (Item("MiningModifier"), "mining"),
    (Item("MiningArm"), "mining"),
    (Item("MiningController"), "mining"),
    (Item("ToolArm"), "mining"),
    (Item("TractorBeam"), "mining"),
    (Item("TowingBeam"), "mining"),
    (ItemPrefix("Salvage"), "mining"),
    (ItemPrefix("Mining"), "mining"),
    (Item("Cooler"), "components"),
    (Item("PowerPlant"), "components"),
    (Item("Shield"), "components"),
    (Item("ShieldController"), "components"),
    (Item("QuantumDrive"), "components"),
    (Item("QuantumInterdictionGenerator"), "components"),
    (Item("JumpDrive"), "components"),
    (Item("Radar"), "components"),
    (Item("Scanner"), "components"),
    (Item("Sensor"), "components"),
    (Item("Ping"), "components"),
    (Item("MainThruster"), "components"),
    (Item("ManneuverThruster"), "components"),
    (Item("FuelTank"), "components"),
    (Item("QuantumFuelTank"), "components"),
    (Item("ExternalFuelTank"), "components"),
    (Item("FuelIntake"), "components"),
    (Item("Armor"), "components"),
    (Item("FlightController"), "components"),
    (Item("Avionics"), "components"),
    (Item("Computer"), "components"),
    (Item("SelfDestruct"), "components"),
    (Item("LifeSupportGenerator"), "components"),
    (Item("LifeSupportVent"), "components"),
    (Item("Battery"), "components"),
    (Item("EMP"), "components"),
    (Item("CargoGrid"), "components"),
    (Item("Transponder"), "components"),
    (Item("WeaponRegenPool"), "components"),
    (Item("Module"), "components"),
    (Item("AIModule"), "components"),
    (Item("DockingCollar"), "components"),
    (Item("DockingAnimator"), "components"),
    (Item("LandingSystem"), "components"),
    (Item("LandingGear"), "components"),
    (Item("GravityGenerator"), "components"),
    (Item("Relay"), "components"),
    (Item("AttachedPart"), "components"),
    (Item("TargetSelector"), "components"),
    (ItemPrefix("Thruster"), "components"),
    // Ship systems' controllers (energy, coolers, comms, wheels, …).
    (Item("SalvageController"), "mining"),
    (ItemPrefix("Capacitor"), "components"),
    (Item("WheeledController"), "components"),
    (Item("CoolerController"), "components"),
    (Item("EnergyController"), "components"),
    (Item("FuelController"), "components"),
    (Item("CommsController"), "components"),
    (Item("DoorController"), "components"),
    (Item("LightController"), "components"),
    (Item("WeaponPersonal"), "fps-weapons"),
    (Item("Gadget"), "fps-weapons"),
    (Item("Grenade"), "fps-weapons"),
    (Item("FPS_Deployable"), "fps-weapons"),
    (Item("WeaponAttachment"), "attachments"),
    (ItemPrefix("Char_Armor_"), "armor"),
    (ItemPrefix("Char_Clothing_"), "armor"),
    (ItemPrefix("Char_Accessory_"), "armor"),
    (Item("Suit"), "armor"),
    (Item("Visor"), "armor"),
    (Item("MobiGlas"), "armor"),
    (Item("Food"), "consumables"),
    (Item("Drink"), "consumables"),
    (Item("Bottle"), "consumables"),
    (Item("FPS_Consumable"), "consumables"),
    (Item("Cargo"), "commodities"),
    (Item("ShipCargo"), "commodities"),
    (Item("AmmoBox"), "commodities"),
    (Item("ShopDisplay"), "shops"),
    (Item("NOITEM_Player"), "characters"),
    (Item("Player"), "characters"),
    (ItemPrefix("Char_"), "characters"),
    // --- Entities without a type, by folder ------------------------------------------------
    (Path("entities/commodities/"), "commodities"),
    (Path("entities/scitem/shopdisplays/"), "shops"),
    (Path("entities/scitem/human/"), "characters"),
    (Path("actor/"), "characters"),
    // --- Other records, by struct type -----------------------------------------------------
    (Struct("SCItemManufacturer"), "factions"),
    (Struct("SpecialEventManufacturer"), "factions"),
    (StructPrefix("Faction"), "factions"),
    (StructPrefix("SReputation"), "factions"),
    (StructPrefix("Reputation"), "factions"),
    (Struct("InfractionDefinition"), "factions"),
    (Struct("BlueprintPoolRecord"), "blueprints"),
    (StructPrefix("Crafting"), "blueprints"),
    (StructPrefix("LegacyCrafting"), "blueprints"),
    (Struct("RefiningProcess"), "mining"),
    (StructPrefix("Mineable"), "mining"),
    (StructPrefix("Harvestable"), "mining"),
    (StructPrefix("SubHarvestable"), "mining"),
    (Struct("AsteroidFieldComposition"), "mining"),
    (StructPrefix("ResourceType"), "commodities"),
    (Struct("CargoManifest"), "commodities"),
    (StructPrefix("Contract"), "missions"),
    (StructPrefix("Mission"), "missions"),
    (StructPrefix("StarMap"), "locations"),
    (Struct("MegaMap"), "locations"),
    (Struct("InteriorMapSectionDefinition"), "locations"),
    (Struct("Level"), "locations"),
    (Struct("LocationEntityDeclaration"), "locations"),
    (Struct("SEntrancesDef"), "locations"),
    (StructPrefix("Atmosphere"), "locations"),
    (StructPrefix("Planet"), "locations"),
    (Struct("RadiationStateTemplate"), "locations"),
    (Struct("SLoadoutAssortment"), "loadouts"),
    (Struct("LoadoutKit"), "loadouts"),
    (Struct("DefaultPlayerLoadoutEntitlementRecord"), "loadouts"),
    (StructPrefix("SEAGlobal"), "loadouts"),
    (Struct("SEAPlayerLoadoutSnapshots"), "loadouts"),
    (StructPrefix("Loot"), "loadouts"),
    (Struct("PoolFilterRecord"), "loadouts"),
    (StructPrefix("Shop"), "shops"),
    (StructPrefix("GlobalShop"), "shops"),
    (Struct("ItemKioskBrand"), "shops"),
    (Struct("CameraShopConfig"), "shops"),
    (Struct("SLocalPlayerShoppingData"), "shops"),
    (Struct("Character"), "characters"),
    (StructPrefix("CharacterRandomName"), "characters"),
    (StructPrefix("Actor"), "characters"),
    (StructPrefix("AIWave"), "characters"),
    (StructPrefix("BuildingBlocks_"), "audio-ui"),
    (StructPrefix("UI"), "audio-ui"),
    (StructPrefix("Hint"), "audio-ui"),
    (StructPrefix("ControlHints"), "audio-ui"),
    (StructPrefix("MobiGlas"), "audio-ui"),
    (StructPrefix("mobiGlas"), "audio-ui"),
    (Struct("JournalEntry"), "audio-ui"),
    (StructPrefix("ChatEmote"), "audio-ui"),
    (StructPrefix("SCItemUIView"), "audio-ui"),
    (StructPrefix("ScanDisplay"), "audio-ui"),
    (StructPrefix("RadarDisplay"), "audio-ui"),
    (StructPrefix("StatusWidget"), "audio-ui"),
    (StructPrefix("VisorLens"), "audio-ui"),
    (StructPrefix("InnerThought"), "audio-ui"),
    (StructPrefix("FlashObject"), "audio-ui"),
    (Struct("PopupDef"), "audio-ui"),
    (Struct("Marker_Configuration"), "audio-ui"),
    (StructPrefix("Audio"), "audio-ui"),
    (StructPrefix("Music"), "audio-ui"),
    (StructPrefix("Foley"), "audio-ui"),
    (StructPrefix("Voice"), "audio-ui"),
    (StructPrefix("Dialogue"), "audio-ui"),
    (StructPrefix("Comms"), "audio-ui"),
    (StructPrefix("Communication"), "audio-ui"),
    (StructPrefix("Conversation"), "audio-ui"),
    (StructPrefix("VideoComms"), "audio-ui"),
    (Struct("Announcer"), "audio-ui"),
    (Struct("GPUParticleAudio"), "audio-ui"),
    (StructPrefix("VibrationAudio"), "audio-ui"),
    (StructPrefix("EntityAudio"), "audio-ui"),
    (StructPrefix("ProceduralPlanetAudio"), "audio-ui"),
    (StructPrefix("RaSTaR"), "audio-ui"),
    (StructPrefix("PlayerChoiceMenu"), "audio-ui"),
    (StructPrefix("WeaponProcedural"), "fps-weapons"),
    // --- Other records, by folder ---------------------------------------------------------
    (Path("ammoparams/vehicle/"), "ship-weapons"),
    (Path("ammoparams/fps/"), "fps-weapons"),
    (Path("loadoutkits/"), "loadouts"),
    (Path("lootgeneration/"), "loadouts"),
    (Path("franchises/"), "shops"),
    (Path("mining/"), "mining"),
    (Path("harvestable/"), "mining"),
    (Path("crafting/"), "blueprints"),
    (Path("reputation/"), "factions"),
    (Path("factions/"), "factions"),
    (Path("lawsystem/"), "factions"),
    (Path("contracts/"), "missions"),
    (Path("missiondata/"), "missions"),
    (Path("missionbroker/"), "missions"),
    (Path("starmap/"), "locations"),
    (Path("character/"), "characters"),
    (Path("aiprofile/"), "characters"),
    (Path("ui/"), "audio-ui"),
    (Path("hints/"), "audio-ui"),
    (Path("audio/"), "audio-ui"),
    (Path("foley/"), "audio-ui"),
    (Path("musiclogic/"), "audio-ui"),
];

/// Readable labels for common kind and sub-type values; others show as they are.
pub const VALUE_LABELS: &[(&str, &str)] = &[
    ("NOITEM_Vehicle", "Vehicles"),
    ("Vehicle_Spaceship", "Spaceships"),
    ("Vehicle_GroundVehicle", "Ground vehicles"),
    ("Vehicle_PowerSuit", "Power suits"),
    ("Paints", "Paints"),
    ("Decal", "Decals"),
    ("WeaponGun", "Guns"),
    ("WeaponDefensive", "Countermeasures"),
    ("WeaponMount", "Weapon mounts"),
    ("Turret", "Turrets"),
    ("TurretBase", "Turret bases"),
    ("Missile", "Missiles"),
    ("Bomb", "Bombs"),
    ("MissileLauncher", "Missile racks"),
    ("BombLauncher", "Bomb racks"),
    ("Cooler", "Coolers"),
    ("PowerPlant", "Power plants"),
    ("Shield", "Shield generators"),
    ("ShieldController", "Shield controllers"),
    ("QuantumDrive", "Quantum drives"),
    ("JumpDrive", "Jump drives"),
    ("Radar", "Radars"),
    ("MainThruster", "Main thrusters"),
    ("ManneuverThruster", "Maneuvering thrusters"),
    ("FuelTank", "Fuel tanks"),
    ("QuantumFuelTank", "Quantum fuel tanks"),
    ("FuelIntake", "Fuel intakes"),
    ("FlightController", "Flight controllers"),
    ("WeaponPersonal", "Weapons"),
    ("Gadget", "Gadgets"),
    ("Grenade", "Grenades"),
    ("WeaponMining", "Mining lasers"),
    ("MiningModifier", "Mining modules"),
    ("Char_Armor_Helmet", "Helmets"),
    ("Char_Armor_Torso", "Torso armor"),
    ("Char_Armor_Arms", "Arm armor"),
    ("Char_Armor_Legs", "Leg armor"),
    ("Char_Armor_Undersuit", "Undersuits"),
    ("Char_Armor_Backpack", "Backpacks"),
    ("Char_Clothing_Hat", "Hats"),
    ("Char_Clothing_Torso_0", "Shirts"),
    ("Char_Clothing_Torso_1", "Jackets"),
    ("Char_Clothing_Torso_2", "Outerwear"),
    ("Char_Clothing_Hands", "Gloves"),
    ("Char_Clothing_Legs", "Pants"),
    ("Char_Clothing_Feet", "Shoes"),
    ("Food", "Food"),
    ("Drink", "Drinks"),
    ("FPS_Consumable", "Medical & tools"),
    ("NOITEM_Player", "Player body parts"),
];

/// Name tokens (split on `_`, `-` and `.`, case-insensitive) of AI, NPC, test and template
/// variants. Checked on entity records only: `Template` and `Derelict` are also words in
/// mission and location names, and `Legacy` armor is player armor.
pub const VARIANT_TOKENS: &[&str] = &[
    "ai",
    "npc",
    "template",
    "test",
    "debug",
    "dummy",
    "unmanned",
    "placeholder",
    "temp",
    "tmp",
    "shipboarded",
    "boarded",
    "derelict",
    "wreck",
    "tutorial",
    "ea",
    "pir",
];

/// Folder names (any segment of the entity's path) that only hold such variants.
pub const VARIANT_FOLDERS: &[&str] = &[
    "npc_archetypes",
    "featuretests",
    "templates",
    "template",
    "test",
    "tests",
    "ai",
    "npc",
    "debug",
    "placeholder",
];

pub fn section_index(id: &str) -> Option<usize> {
    SECTIONS.iter().position(|s| s.id == id)
}

/// The section of a record. `item_type` is the entity's `AttachDef.Type` (entities only);
/// `rel` its lower-case path below the records root.
pub fn section_of(struct_type: &str, rel: &str, item_type: Option<&str>, entity: bool) -> usize {
    let hit = RULES.iter().find(|(m, _)| match *m {
        Item(t) => item_type.is_some_and(|x| x.eq_ignore_ascii_case(t)),
        ItemPrefix(p) => {
            item_type.is_some_and(|x| x.len() >= p.len() && x[..p.len()].eq_ignore_ascii_case(p))
        }
        Struct(t) => !entity && struct_type.eq_ignore_ascii_case(t),
        StructPrefix(p) => {
            !entity
                && struct_type.len() >= p.len()
                && struct_type[..p.len()].eq_ignore_ascii_case(p)
        }
        Path(p) => rel.starts_with(p),
    });
    let id = match hit {
        Some((_, id)) => id,
        None if entity => ENTITY_FALLBACK,
        None => OTHER,
    };
    section_index(id).unwrap_or(SECTIONS.len() - 1)
}

/// An AI, NPC, test or template variant, by name token or folder (entities only).
pub fn is_variant(name: &str, rel: &str) -> bool {
    let short = name.split_once('.').map_or(name, |(_, s)| s);
    let token = short
        .split(['_', '-', '.', ' '])
        .any(|t| VARIANT_TOKENS.iter().any(|v| t.eq_ignore_ascii_case(v)));
    token || {
        let mut segs: Vec<&str> = rel.split('/').collect();
        segs.pop();
        segs.iter().any(|s| VARIANT_FOLDERS.contains(s))
    }
}

pub fn value_label(v: &str) -> Option<&'static str> {
    VALUE_LABELS
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(v))
        .map(|(_, l)| *l)
}
