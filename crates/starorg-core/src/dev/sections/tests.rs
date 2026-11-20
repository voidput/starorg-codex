use super::rules::{self, SECTIONS};
use super::*;
use crate::dev::explorer::{Entry, ExplorerIndex};

fn entry(i: usize, name: &str, ty: &str, path: &str) -> Entry {
    Entry {
        guid: format!("00000000-0000-0000-0000-{i:012x}"),
        name: format!("{ty}.{name}"),
        struct_type: ty.into(),
        path: format!("libs/foundry/records/{path}"),
        main: true,
        tag: None,
    }
}

fn ent(item: &str, sub: &str, size: i64, maker: &str, display: &str) -> Option<Facts> {
    Some(Facts {
        item_type: Some(item.into()),
        sub_type: Some(sub.into()),
        size: Some(size),
        manufacturer: (!maker.is_empty()).then(|| maker.into()),
        display: (!display.is_empty()).then(|| display.into()),
        grade: None,
        class: None,
        model: true,
        sounds: false,
    })
}

const E: &str = "EntityClassDefinition";

fn fixture() -> (ExplorerIndex, SectionIndex) {
    let rows: Vec<(Entry, Option<Facts>)> = vec![
        (
            entry(1, "AEGS_Gladius", E, "entities/spaceships/aegs_gladius.xml"),
            ent("NOITEM_Vehicle", "Vehicle_Spaceship", 3, "AEGS", "Gladius"),
        ),
        (
            entry(
                2,
                "AEGS_Gladius_PU_AI_CIV",
                E,
                "entities/spaceships/aegs_gladius_pu_ai_civ.xml",
            ),
            ent("NOITEM_Vehicle", "Vehicle_Spaceship", 3, "AEGS", "Gladius"),
        ),
        (
            entry(
                3,
                "COOL_AEGS_S01_Bracer",
                E,
                "entities/scitem/ships/cooler/cool_aegs_s01_bracer.xml",
            ),
            ent("Cooler", "UNDEFINED", 1, "AEGS", "Bracer"),
        ),
        (
            entry(
                4,
                "COOL_JUST_S02_Glacier",
                E,
                "entities/scitem/ships/cooler/cool_just_s02.xml",
            ),
            ent("Cooler", "UNDEFINED", 2, "JUST", "Glacier"),
        ),
        (
            entry(
                5,
                "POWR_S01_Template",
                E,
                "entities/scitem/ships/powerplant/powr_template.xml",
            ),
            ent("PowerPlant", "Power", 1, "", ""),
        ),
        (
            entry(
                6,
                "behr_rifle_ballistic_01",
                E,
                "entities/scitem/weapons/fps_weapons/behr_rifle.xml",
            ),
            ent("WeaponPersonal", "Medium", 2, "BEHR", "P4-AR Rifle"),
        ),
        (
            entry(
                7,
                "rrs_helmet_01",
                E,
                "entities/scitem/characters/human/armor/rrs_helmet.xml",
            ),
            ent("Char_Armor_Helmet", "UNDEFINED", 1, "RRS", "Helmet"),
        ),
        (
            entry(
                8,
                "copper_ore",
                E,
                "entities/commodities/minerals/copper.xml",
            ),
            None,
        ),
        (
            entry(9, "Pyro1", "StarMapObject", "starmap/pyro/pyro1.xml"),
            None,
        ),
        (
            entry(
                10,
                "BP_CRAFT_x",
                "CraftingBlueprintRecord",
                "crafting/blueprints/x.xml",
            ),
            None,
        ),
        (
            entry(
                11,
                "hud_main",
                "BuildingBlocks_Canvas",
                "ui/hud/hud_main.xml",
            ),
            None,
        ),
        (
            entry(12, "mystery", "SomethingNew", "whatever/mystery.xml"),
            None,
        ),
        (
            entry(13, "box_01", E, "entities/scitem/prop/box_01.xml"),
            None,
        ),
    ];
    let (entries, facts): (Vec<_>, Vec<_>) = rows.into_iter().unzip();
    let ix = ExplorerIndex::new(entries);
    // The index keeps the input order.
    let facts: Vec<(u32, Facts)> = facts
        .into_iter()
        .enumerate()
        .filter_map(|(i, f)| f.map(|f| (i as u32, f)))
        .collect();
    let si = SectionIndex::new(&ix, &facts, &|c| {
        (c == "AEGS").then(|| "Aegis Dynamics".to_string())
    });
    (ix, si)
}

#[test]
fn every_record_lands_in_exactly_one_section() {
    let (ix, si) = fixture();
    let total: u32 = si.counts().iter().map(|c| c.1).sum();
    assert_eq!(total as usize, ix.len());
    let of = |n: &str| si.section_id(ix.find_name_or_short(n).unwrap());
    assert_eq!(of("AEGS_Gladius"), "ships");
    assert_eq!(of("COOL_AEGS_S01_Bracer"), "components");
    assert_eq!(of("behr_rifle_ballistic_01"), "fps-weapons");
    assert_eq!(of("rrs_helmet_01"), "armor");
    assert_eq!(of("copper_ore"), "commodities");
    assert_eq!(of("Pyro1"), "locations");
    assert_eq!(of("BP_CRAFT_x"), "blueprints");
    assert_eq!(of("hud_main"), "audio-ui");
    assert_eq!(of("mystery"), "other");
    assert_eq!(of("box_01"), "items");
}

#[test]
fn rules_name_known_sections_only_and_other_is_last() {
    for (_, id) in rules::RULES {
        assert!(rules::section_index(id).is_some(), "{id}");
    }
    assert_eq!(SECTIONS.last().unwrap().id, rules::OTHER);
    let mut ids: Vec<_> = SECTIONS.iter().map(|s| s.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), SECTIONS.len());
}

#[test]
fn section_rules_table_examples() {
    let e = |t: &str| SECTIONS[rules::section_of(E, "entities/x.xml", Some(t), true)].id;
    assert_eq!(e("WeaponGun"), "ship-weapons");
    assert_eq!(e("Missile"), "missiles");
    assert_eq!(e("WeaponAttachment"), "attachments");
    assert_eq!(e("Food"), "consumables");
    assert_eq!(e("SalvageHead"), "mining");
    assert_eq!(e("Char_Clothing_Feet"), "armor");
    assert_eq!(e("Char_Head_Hair"), "characters");
    assert_eq!(e("ShopDisplay"), "shops");
    assert_eq!(e("SomeNewType"), "items");
    let r = |t: &str, p: &str| SECTIONS[rules::section_of(t, p, None, false)].id;
    assert_eq!(r("ContractGenerator", "contracts/x.xml"), "missions");
    assert_eq!(
        r("SReputationStandingParams", "reputation/x.xml"),
        "factions"
    );
    assert_eq!(r("LoadoutKit", "loadoutkits/x.xml"), "loadouts");
    assert_eq!(r("AmmoParams", "ammoparams/vehicle/x.xml"), "ship-weapons");
    assert_eq!(r("AmmoParams", "ammoparams/fps/x.xml"), "fps-weapons");
    assert_eq!(r("ShopFranchise", "franchises/x.xml"), "shops");
    assert_eq!(r("Tag", "tagdatabase/x.xml"), "other");
    // Animation and recoil records are not FPS weapons.
    assert_eq!(
        r("WeaponProceduralClip", "weaponproceduralclip/x.xml"),
        "other"
    );
    // Human-made props without a type are items, not characters.
    assert_eq!(
        SECTIONS[rules::section_of(E, "entities/scitem/human/food/x.xml", None, true)].id,
        "items"
    );
    // Named families and ship weapon parts.
    let x = |short: &str, item: &str, sub: &str| {
        SECTIONS[rules::section_of_subject(&rules::Subject {
            struct_type: E,
            rel: "entities/x.xml",
            short,
            item_type: Some(item),
            sub_type: Some(sub),
            entity: true,
        })]
        .id
    };
    assert_eq!(x("GPI_Destructible_Barrel", "WeaponGun", "Gun"), "items");
    assert_eq!(
        x(
            "BEHR_Gun_FiringMechanism",
            "WeaponAttachment",
            "FiringMechanism"
        ),
        "ship-weapons"
    );
    assert_eq!(
        x("Scope_4x", "WeaponAttachment", "IronSight"),
        "attachments"
    );
    // A struct rule never applies to an entity (EntityClassDefinition is no "Entity*" rule).
    assert_eq!(
        SECTIONS[rules::section_of(E, "mission/x.xml", None, true)].id,
        "items"
    );
}

#[test]
fn variant_heuristic() {
    assert!(rules::is_variant(
        "EntityClassDefinition.AEGS_Gladius_PU_AI_CIV",
        "entities/x.xml"
    ));
    assert!(rules::is_variant(
        "EntityClassDefinition.QDRV_S02_Template",
        "entities/x.xml"
    ));
    assert!(rules::is_variant(
        "EntityClassDefinition.Guy",
        "actor/actors/npc_archetypes/guy.xml"
    ));
    assert!(rules::is_variant(
        "EntityClassDefinition.thing_test",
        "entities/x.xml"
    ));
    assert!(!rules::is_variant(
        "EntityClassDefinition.AEGS_Gladius",
        "entities/x.xml"
    ));
    // `ai` must be a whole token: "Aires", "Raider" are not AI.
    assert!(!rules::is_variant(
        "EntityClassDefinition.Raider_Aires",
        "entities/x.xml"
    ));
    assert!(!rules::is_variant(
        "EntityClassDefinition.cds_legacy_armor_heavy_arms",
        "entities/x.xml"
    ));
    // Real-like names: the Pirate Gladius is a player ship; wrecks, sentries, probes,
    // hijacked and display copies are not.
    let v = |n: &str| rules::is_variant(&format!("EntityClassDefinition.{n}"), "entities/x.xml");
    assert!(!v("AEGS_Gladius_PIR"));
    assert!(!v("DRAK_Cutlass_Black"));
    assert!(!v("MISC_Prospector"));
    // Teach's Ship Shop sells these; shop racks, HUD props and hangar flair are real items.
    for n in [
        "DRAK_Golem_Teach",
        "AEGS_Reclaimer_Teach",
        "Flair_viVid_Display",
        "counter_display_shelf_1",
        "vehicle_display_ship_01",
        "Radar_Display_Gladius",
    ] {
        assert!(!v(n), "{n}");
    }
    for n in [
        "AEGS_Gladius_PU_Hijacked",
        "ARMR_Sentinel_Display",
        "RSI_Aurora_Display",
        "SalvageableDebris_AresInferno",
        "Orbital_Sentry_PU_NineTails_Size2",
        "probe_comms_01",
        "EAObjectiveDestructable_MiningLaser",
        "Door_Indestructible",
        "MISC_Freelancer_NoInterior",
        "AEGS_Idris_GameMaster",
    ] {
        assert!(v(n), "{n}");
    }
}

#[test]
fn tree_root_media_sections_and_groups() {
    let (ix, si) = fixture();
    let root = si.tree(&ix, "", 0, 500, true, false);
    let labels: Vec<_> = root.nodes.iter().map(|n| n.label.as_str()).collect();
    assert_eq!(labels[0], "Models");
    assert!(!labels.contains(&"Sounds"), "no record has sounds");
    assert!(labels.contains(&"Ships & vehicles"));
    let ships = root
        .nodes
        .iter()
        .find(|n| n.label == "Ships & vehicles")
        .unwrap();
    assert_eq!(ships.count, 2);
    assert_eq!(ships.kind, NodeKind::Section);
    // Player-only hides the AI Gladius.
    let root = si.tree(&ix, "", 0, 500, true, true);
    assert_eq!(
        root.nodes
            .iter()
            .find(|n| n.label == "Ships & vehicles")
            .unwrap()
            .count,
        1
    );
    // Ships > Vehicles > Spaceships > Aegis Dynamics (AEGS) > records.
    let l1 = si.tree(&ix, &ships.id, 0, 500, true, false);
    assert_eq!(l1.nodes[0].label, "Vehicles");
    let l2 = si.tree(&ix, &l1.nodes[0].id, 0, 500, true, false);
    assert_eq!(l2.nodes[0].label, "Spaceships");
    let l3 = si.tree(&ix, &l2.nodes[0].id, 0, 500, true, false);
    assert_eq!(l3.nodes[0].label, "Aegis Dynamics (AEGS)");
    let recs = si.tree(&ix, &l3.nodes[0].id, 0, 500, true, false);
    assert_eq!(recs.total, 2);
    assert!(
        recs.nodes
            .iter()
            .all(|n| n.kind == NodeKind::Record && n.label == "Gladius")
    );
    // Components > Coolers > Size 1, Size 2.
    let comp = root_id(&si, &ix, "Ship components");
    let k = si.tree(&ix, &comp, 0, 500, true, false);
    let coolers = k.nodes.iter().find(|n| n.label == "Coolers").unwrap();
    let sizes: Vec<_> = si
        .tree(&ix, &coolers.id, 0, 500, true, false)
        .nodes
        .into_iter()
        .map(|n| n.label)
        .collect();
    assert_eq!(sizes, ["Size 1", "Size 2"]);
    // A level nobody has (commodities without facts: Kind is the struct type, Folder set).
    assert!(
        si.tree(&ix, "s:[\"nope\"]", 0, 500, true, false)
            .nodes
            .is_empty()
    );
    assert!(
        si.tree(&ix, "garbage", 0, 500, true, false)
            .nodes
            .is_empty()
    );
}

fn root_id(si: &SectionIndex, ix: &ExplorerIndex, label: &str) -> String {
    si.tree(ix, "", 0, 500, true, false)
        .nodes
        .into_iter()
        .find(|n| n.label == label)
        .unwrap()
        .id
}

#[test]
fn models_view_groups_by_section() {
    let (ix, si) = fixture();
    let models = si.tree(&ix, "m:models:[]", 0, 500, true, false);
    let labels: Vec<_> = models.nodes.iter().map(|n| n.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Ships & vehicles",
            "Ship components",
            "FPS weapons",
            "Armor & clothing"
        ]
    );
    assert!(models.nodes[0].id.starts_with("m:models:"));
    let page = si.list(
        &ix,
        &ListQuery {
            node: "m:models:[]".into(),
            limit: 500,
            ..Default::default()
        },
    );
    assert_eq!(page.total, 7);
    assert_eq!(page.title, "Models");
}

#[test]
fn list_filters_facets_and_sort() {
    let (ix, si) = fixture();
    let comp = root_id(&si, &ix, "Ship components");
    let q = |f: Filters, sort: SortKey| ListQuery {
        node: comp.clone(),
        filters: f,
        sort,
        limit: 500,
        ..Default::default()
    };
    let all = si.list(&ix, &q(Filters::default(), SortKey::Name));
    assert_eq!(all.total, 3);
    let labels: Vec<_> = all.items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(labels, ["Bracer", "Glacier", "POWR_S01_Template"]);
    assert_eq!(all.items[0].detail.as_deref(), Some("COOL_AEGS_S01_Bracer"));
    assert_eq!(all.items[2].detail, None);
    assert_eq!(all.facets.player, 2);
    assert_eq!(all.facets.model, 3);
    let sizes: Vec<_> = all
        .facets
        .sizes
        .iter()
        .map(|v| (v.label.as_str(), v.count))
        .collect();
    assert_eq!(sizes, [("S1", 2), ("S2", 1)]);
    // Size 1 only: the Size facet still counts both sizes (facet rule), makers follow.
    let s1 = si.list(
        &ix,
        &q(
            Filters {
                sizes: vec![1],
                ..Default::default()
            },
            SortKey::Name,
        ),
    );
    assert_eq!(s1.total, 2);
    assert_eq!(s1.facets.sizes.len(), 2);
    assert_eq!(s1.facets.manufacturers.len(), 1);
    assert_eq!(s1.facets.manufacturers[0].label, "Aegis Dynamics (AEGS)");
    // Player only drops the template.
    let p = si.list(
        &ix,
        &q(
            Filters {
                player_only: true,
                ..Default::default()
            },
            SortKey::Name,
        ),
    );
    assert_eq!(p.total, 2);
    // Two makers OR'd, kinds filter, case-insensitive.
    let m = si.list(
        &ix,
        &q(
            Filters {
                manufacturers: vec!["aegs".into(), "JUST".into()],
                kinds: vec!["cooler".into()],
                ..Default::default()
            },
            SortKey::Name,
        ),
    );
    assert_eq!(m.total, 2);
    // Sort by size (no size last) and by manufacturer (none last).
    let by_size = si.list(&ix, &q(Filters::default(), SortKey::Size));
    assert_eq!(by_size.items.last().unwrap().size, Some(2));
    let by_maker = si.list(&ix, &q(Filters::default(), SortKey::Manufacturer));
    assert_eq!(by_maker.items.last().unwrap().manufacturer, None);
    // Search inside the node by readable or record name; ranks the exact label first.
    let s = si.list(
        &ix,
        &ListQuery {
            node: comp.clone(),
            query: "glacier".into(),
            limit: 500,
            ..Default::default()
        },
    );
    assert_eq!(s.total, 1);
    let s = si.list(
        &ix,
        &ListQuery {
            node: String::new(),
            query: "gladius".into(),
            limit: 500,
            ..Default::default()
        },
    );
    assert_eq!(s.total, 2);
    assert_eq!(s.title, "");
    // Paging.
    let pg = si.list(
        &ix,
        &ListQuery {
            node: comp,
            offset: 1,
            limit: 1,
            ..Default::default()
        },
    );
    assert_eq!((pg.items.len(), pg.total, pg.offset), (1, 3, 1));
}

#[test]
fn compact_rows() {
    let (_, si) = fixture();
    assert!(std::mem::size_of::<Rec>() <= 32);
    assert!(si.bytes() > 0);
}

#[test]
fn query_prefixes() {
    let p = parse_query(
        "Type:Cooler maker:aegs size:1-2 s:3 tag:Sys section:ships grade:a class:mil has:model has:sounds foo bar: x:y",
    );
    assert_eq!(p.types, ["cooler"]);
    assert_eq!(p.makers, ["aegs"]);
    assert_eq!(p.sizes, [(1, 2), (3, 3)]);
    assert_eq!(p.tags, ["sys"]);
    assert_eq!(p.sections, ["ships"]);
    assert_eq!(p.grades, ["a"]);
    assert_eq!(p.classes, ["mil"]);
    assert!(p.model && p.sounds);
    assert_eq!(p.words, ["foo", "bar:", "x:y"]);
    assert_eq!(p.understood.len(), 10);
    assert_eq!(parse_query("size:big").words, ["size:big"]);
    assert_eq!(parse_query("size:5-1").sizes, [(1, 5)]);
}

#[test]
fn description_grade_and_class() {
    let t = r"Item Type: Cooler\nManufacturer: Aegis Dynamics \nSize: 1\nGrade: C\nClass: Military\n\nFor those";
    assert_eq!(desc_facts(t), (Some("C".into()), Some("Military".into())));
    assert_eq!(desc_facts("Just prose."), (None, None));
}

#[test]
fn search_is_fuzzy_ranked_and_prefixed() {
    let (ix, si) = fixture();
    let all = |q: &str| {
        si.list(
            &ix,
            &ListQuery {
                query: q.into(),
                limit: 500,
                ..Default::default()
            },
        )
    };
    // Both name prefixes match; the ship section comes before components.
    let r = all("gla");
    assert_eq!(r.matching, Matching::Exact);
    assert_eq!(r.items[0].label, "Gladius");
    assert!(r.items.iter().any(|i| i.label == "Glacier"));
    // Nothing exact: close spellings, then abbreviations, and say so.
    let r = all("glaceir");
    assert_eq!(
        (r.matching, r.items[0].label.as_str()),
        (Matching::Typo, "Glacier")
    );
    let r = all("glcr");
    assert_eq!(
        (r.matching, r.items[0].label.as_str()),
        (Matching::Abbreviation, "Glacier")
    );
    assert_eq!(all("zzzz").total, 0);
    // Manufacturer name and kind label are searchable words.
    assert!(
        all("aegis")
            .items
            .iter()
            .all(|i| i.manufacturer.as_deref() == Some("AEGS"))
    );
    assert_eq!(all("coolers").total, 2);
    // Prefixes.
    assert_eq!(all("type:cooler").total, 2);
    assert_eq!(all("maker:aegis size:1").total, 1);
    assert_eq!(all("size:2-3").total, 4);
    assert_eq!(all("section:fps").total, 1);
    assert_eq!(all("has:model").total, 7);
    assert_eq!(all("has:sounds").total, 0);
    let r = all("type:cooler glacier");
    assert_eq!((r.total, r.understood.len(), r.words.len()), (1, 1, 1));
    // Counts per section follow the search.
    let r = all("aegs");
    let secs: Vec<_> = r
        .facets
        .sections
        .iter()
        .map(|v| (v.value.as_str(), v.count))
        .collect();
    assert_eq!(secs, [("ships", 2), ("components", 1)]);
}

#[test]
fn grouped_list_with_collapsed_headers() {
    let (ix, si) = fixture();
    let q = |group: ListGroup, collapsed: Vec<String>| ListQuery {
        group,
        collapsed,
        limit: 500,
        ..Default::default()
    };
    let r = si.list(&ix, &q(ListGroup::Section, vec![]));
    assert_eq!(r.groups[0].key, "ships");
    assert_eq!(r.groups.iter().map(|g| g.count).sum::<u32>(), 13);
    assert_eq!(r.items[0].group.as_deref(), Some("ships"));
    // Rows follow header order.
    let order: Vec<_> = r.items.iter().map(|i| i.group.clone().unwrap()).collect();
    let mut seen: Vec<String> = vec![];
    for g in order {
        if seen.last() != Some(&g) {
            assert!(!seen.contains(&g), "groups are contiguous");
            seen.push(g);
        }
    }
    let c = si.list(&ix, &q(ListGroup::Section, vec!["ships".into()]));
    assert_eq!((c.total, c.matched), (11, 13));
    assert!(c.groups[0].collapsed);
    let m = si.list(&ix, &q(ListGroup::Manufacturer, vec![]));
    assert_eq!(m.groups.last().unwrap().label, "No manufacturer");
    let s = si.list(&ix, &q(ListGroup::Size, vec![]));
    assert_eq!(s.groups[0].label, "Size 1");
    let k = si.list(&ix, &q(ListGroup::Kind, vec![]));
    assert!(
        k.groups
            .iter()
            .any(|g| g.label == "Coolers" && g.count == 2)
    );
}

#[test]
fn facet_rule_on_every_dimension() {
    let (ix, si) = fixture();
    let f = Filters {
        sections: vec!["components".into()],
        ..Default::default()
    };
    let r = si.list(
        &ix,
        &ListQuery {
            filters: f,
            limit: 500,
            ..Default::default()
        },
    );
    assert_eq!(r.total, 3);
    // The section facet ignores its own filter: every section still counts.
    assert!(r.facets.sections.len() > 5);
    // Kinds follow the section filter.
    assert!(
        r.facets
            .kinds
            .iter()
            .all(|k| k.value == "Cooler" || k.value == "PowerPlant")
    );
    let f = Filters {
        tags: vec!["none".into()],
        ..Default::default()
    };
    assert_eq!(
        si.list(
            &ix,
            &ListQuery {
                filters: f,
                limit: 500,
                ..Default::default()
            }
        )
        .total,
        0
    );
}

#[test]
fn changes_facet_and_badges() {
    let (ix, mut si) = fixture();
    let q = |f: Filters| ListQuery {
        filters: f,
        limit: 500,
        ..Default::default()
    };
    assert!(
        si.list(&ix, &q(Filters::default()))
            .facets
            .changes
            .is_empty()
    );
    let g = ix.find_name_or_short("AEGS_Gladius").unwrap();
    let c = ix.find_name_or_short("COOL_AEGS_S01_Bracer").unwrap();
    si.set_changes([
        (g, Change::Changed),
        (c, Change::Added),
        (9999, Change::Added),
    ]);
    let all = si.list(&ix, &q(Filters::default()));
    let ch: Vec<_> = all
        .facets
        .changes
        .iter()
        .map(|v| (v.value.as_str(), v.count))
        .collect();
    assert_eq!(ch, [("added", 1), ("changed", 1)]);
    let only = si.list(
        &ix,
        &q(Filters {
            changes: vec!["changed".into()],
            ..Default::default()
        }),
    );
    assert_eq!(only.total, 1);
    assert_eq!(only.items[0].change.as_deref(), Some("changed"));
    // The facet rule: the change facet still counts both states.
    assert_eq!(only.facets.changes.len(), 2);
}

#[test]
fn an_unbuilt_index_answers_nothing() {
    let (ix, _) = fixture();
    let si = SectionIndex::default();
    assert!(!si.covers(&ix));
    assert!(si.tree(&ix, "", 0, 500, true, false).nodes.is_empty());
    assert_eq!(si.list(&ix, &ListQuery::default()).total, 0);
}

#[test]
fn compare_marks_come_from_the_dataset_diff_by_guid() {
    use crate::dev::dataset::infer;
    use crate::dev::diff::diff_sources;
    let g = |n: u32| format!("00000000-0000-0000-0000-{n:012x}");
    let a = serde_json::json!({ "components": [
        { "id": g(1), "name": "a", "health": 1 },
        { "id": g(2), "name": "b", "health": 2 },
        { "id": g(3), "name": "gone", "health": 3 },
    ]});
    let b = serde_json::json!({ "components": [
        { "id": g(1), "name": "a", "health": 1 },
        { "id": g(2), "name": "b", "health": 20 },
        { "id": g(4), "name": "new", "health": 4 },
        { "id": "not-a-guid", "name": "x", "health": 5 },
    ]});
    let (da, db) = (infer("gamedata.json", 0, a), infer("gamedata.json", 0, b));
    let tables = diff_sources(
        &[("gamedata.json".into(), &da)],
        &[("gamedata.json".into(), &db)],
        &|_, _| {},
    );
    let (mut marks, removed) = compare_marks(&tables, &[("gamedata.json".into(), &db)]);
    marks.sort_by(|x, y| x.0.cmp(&y.0));
    assert_eq!(marks, [(g(2), Change::Changed), (g(4), Change::Added)]);
    assert_eq!(removed, 1);
}

#[test]
fn search_ranks_base_items_and_tolerates_typos() {
    let rows: Vec<(Entry, Option<Facts>)> = vec![
        (
            entry(1, "Plush_Cutlass", E, "entities/scitem/prop/plush.xml"),
            ent("Misc", "UNDEFINED", 1, "", "Cutlass Black Plushie"),
        ),
        (
            entry(
                2,
                "DRAK_Cutlass_Black_PYAM",
                E,
                "entities/spaceships/c2.xml",
            ),
            ent(
                "NOITEM_Vehicle",
                "Vehicle_Spaceship",
                2,
                "DRAK",
                "Cutlass Black PYAM Exec",
            ),
        ),
        (
            entry(3, "DRAK_Cutlass_Black", E, "entities/spaceships/c1.xml"),
            ent(
                "NOITEM_Vehicle",
                "Vehicle_Spaceship",
                2,
                "DRAK",
                "Drake Cutlass Black",
            ),
        ),
        (
            entry(4, "QDRV_A", E, "entities/scitem/ships/qd/a.xml"),
            ent("QuantumDrive", "UNDEFINED", 1, "", "Atlas Quantum Drive"),
        ),
        (
            entry(5, "QuantumDriveSpooling01", "AudioFoo", "audio/qd.xml"),
            None,
        ),
        (
            entry(6, "Molecule_Stuff", E, "entities/scitem/prop/m.xml"),
            ent("Misc", "UNDEFINED", 1, "", "Molecule Lamp"),
        ),
    ];
    let (entries, facts): (Vec<_>, Vec<_>) = rows.into_iter().unzip();
    let ix = ExplorerIndex::new(entries);
    let facts: Vec<(u32, Facts)> = facts
        .into_iter()
        .enumerate()
        .filter_map(|(i, f)| f.map(|f| (i as u32, f)))
        .collect();
    let si = SectionIndex::new(&ix, &facts, &|_| None);
    let all = |q: &str| {
        si.list(
            &ix,
            &ListQuery {
                query: q.into(),
                limit: 500,
                ..Default::default()
            },
        )
    };
    let labels = |p: &ListPage| p.items.iter().map(|i| i.label.clone()).collect::<Vec<_>>();
    // The base ship first: whole words, ships before props, fewer words left over.
    let r = all("cutlass black");
    assert_eq!(
        labels(&r),
        [
            "Drake Cutlass Black",
            "Cutlass Black PYAM Exec",
            "Cutlass Black Plushie"
        ]
    );
    // Real drives before the audio record that only shares the name.
    let r = all("quantum drive");
    assert_eq!(r.items[0].label, "Atlas Quantum Drive");
    // Swapped letters: one edit; the typo hits are the only hits, counted alone.
    let r = all("qauntum drive");
    assert_eq!(r.matching, Matching::Typo);
    assert_eq!(r.items[0].label, "Atlas Quantum Drive");
    assert_eq!(
        r.facets.sections.iter().map(|v| v.count).sum::<u32>(),
        r.total
    );
    // An exact hit keeps abbreviations out: `mole` matches Molecule only, nothing fuzzy.
    let r = all("mole");
    assert_eq!((r.matching, r.total), (Matching::Exact, 1));
    // Short words get no typo tolerance.
    assert_eq!(all("atl").matching, Matching::Exact);
    assert_eq!(all("xtl").total, 0);
    assert_eq!(all("lmap").total, 0);
}

#[test]
fn edit_distance_counts_a_swap_as_one() {
    assert_eq!(edit_distance("galdius", "gladius", 2), Some(1));
    assert_eq!(edit_distance("qauntum", "quantum", 1), Some(1));
    assert_eq!(edit_distance("kitten", "sitting", 2), None);
    assert_eq!(edit_distance("kitten", "sitting", 3), Some(3));
    assert_eq!(edit_distance("same", "same", 0), Some(0));
}

#[test]
fn a_picked_facet_value_stays_listed() {
    let (ix, si) = fixture();
    let f = Filters {
        manufacturers: vec!["NOPE".into()],
        ..Default::default()
    };
    let r = si.list(
        &ix,
        &ListQuery {
            filters: f,
            limit: 500,
            ..Default::default()
        },
    );
    let v = r
        .facets
        .manufacturers
        .iter()
        .find(|v| v.value == "NOPE")
        .expect("listed");
    assert_eq!(v.count, 0);
}

/// Avengers that read the same: the record names differ, the display names do not.
fn avengers() -> (ExplorerIndex, SectionIndex) {
    let ship = |i: usize, name: &str, display: &str, main: bool| {
        let mut e = entry(
            i,
            name,
            E,
            &format!("entities/spaceships/{}.xml", name.to_lowercase()),
        );
        e.main = main;
        (
            e,
            ent("NOITEM_Vehicle", "Vehicle_Spaceship", 2, "AEGS", display),
        )
    };
    let rows = vec![
        ship(1, "AEGS_Avenger_Titan_PU_AI", "Avenger Titan", true),
        ship(2, "AEGS_Avenger_Titan_Template", "Avenger Titan", true),
        ship(3, "AEGS_Avenger_Titan_Ext", "Avenger Titan", true),
        ship(4, "AEGS_Avenger_Titan", "Avenger Titan", true),
        ship(5, "AEGS_Avenger_Stalker_B", "Avenger Stalker", false),
        ship(6, "AEGS_Avenger_Stalker_Long", "AVENGER stalker", true),
        ship(7, "AEGS_Gladius", "Gladius", true),
    ];
    let (entries, facts): (Vec<_>, Vec<_>) = rows.into_iter().unzip();
    let ix = ExplorerIndex::new(entries);
    let facts: Vec<(u32, Facts)> = facts
        .into_iter()
        .enumerate()
        .filter_map(|(i, f)| f.map(|f| (i as u32, f)))
        .collect();
    let si = SectionIndex::new(&ix, &facts, &|_| None);
    (ix, si)
}

fn short(it: &ListItem) -> &str {
    it.name.split_once('.').unwrap().1
}

#[test]
fn folding_off_lists_every_record() {
    let (ix, si) = avengers();
    let page = si.list(
        &ix,
        &ListQuery {
            limit: 500,
            ..Default::default()
        },
    );
    assert_eq!((page.total, page.matched), (7, 7));
    assert!(page.items.iter().all(|i| i.variants.is_empty()));
    // No `variants` key on the wire while empty.
    let v = serde_json::to_value(&page.items[0]).unwrap();
    assert!(v.get("variants").is_none());
}

#[test]
fn folding_picks_the_player_record_and_lists_the_rest() {
    let (ix, si) = avengers();
    let q = |offset: u32, limit: u32| ListQuery {
        fold_variants: true,
        offset,
        limit,
        ..Default::default()
    };
    let page = si.list(&ix, &q(0, 500));
    // One row per readable name, case-insensitively; totals count rows.
    assert_eq!((page.total, page.matched), (3, 3));
    let reps: Vec<_> = page.items.iter().map(short).collect();
    assert_eq!(
        reps,
        [
            "AEGS_Avenger_Stalker_Long",
            "AEGS_Avenger_Titan",
            "AEGS_Gladius"
        ]
    );
    // A main record beats a shorter sub-record; the rest follow in the same order.
    let stalker = &page.items[0];
    let others: Vec<_> = stalker.variants.iter().map(|v| v.short.as_str()).collect();
    assert_eq!(others, ["AEGS_Avenger_Stalker_B"]);
    assert!(!stalker.variants[0].main);
    let titan = &page.items[1];
    let others: Vec<_> = titan.variants.iter().map(|v| v.short.as_str()).collect();
    assert_eq!(
        others,
        [
            "AEGS_Avenger_Titan_Ext",
            "AEGS_Avenger_Titan_PU_AI",
            "AEGS_Avenger_Titan_Template"
        ]
    );
    assert_eq!(
        titan.variants.iter().map(|v| v.variant).collect::<Vec<_>>(),
        [false, true, true]
    );
    assert!(titan.variants[0].guid.starts_with("00000000-"));
    assert!(titan.variants[0].name.starts_with("EntityClassDefinition."));
    assert!(page.items[2].variants.is_empty());
    // Facet counts stay per record.
    assert_eq!(page.facets.sections[0].count, 7);
    assert_eq!(page.facets.player, 5);
    // Paging over folded rows.
    let p2 = si.list(&ix, &q(1, 1));
    assert_eq!((p2.total, p2.items.len()), (3, 1));
    assert_eq!(short(&p2.items[0]), "AEGS_Avenger_Titan");
    // Grouped headers count folded rows.
    let g = si.list(
        &ix,
        &ListQuery {
            group: ListGroup::Manufacturer,
            ..q(0, 500)
        },
    );
    assert_eq!(g.groups.iter().map(|h| h.count).sum::<u32>(), 3);
    // Filters apply before folding: player-only leaves Titan's two real records.
    let p = si.list(
        &ix,
        &ListQuery {
            filters: Filters {
                player_only: true,
                ..Default::default()
            },
            ..q(0, 500)
        },
    );
    let titan = p.items.iter().find(|i| i.label == "Avenger Titan").unwrap();
    assert_eq!(titan.variants.len(), 1);
    // A search that only one variant matches still finds the row, by its representative.
    let s = si.list(
        &ix,
        &ListQuery {
            query: "titan_template".into(),
            ..q(0, 500)
        },
    );
    assert_eq!(s.total, 1);
    assert_eq!(short(&s.items[0]), "AEGS_Avenger_Titan_Template");
    let s = si.list(
        &ix,
        &ListQuery {
            query: "avenger titan".into(),
            ..q(0, 500)
        },
    );
    assert_eq!(s.total, 1);
    assert_eq!(short(&s.items[0]), "AEGS_Avenger_Titan");
    assert_eq!(s.items[0].variants.len(), 3);
}

#[test]
fn folded_tree_leaves_count_their_records() {
    let (ix, si) = avengers();
    let mut node = root_id(&si, &ix, "Ships & vehicles");
    let opts = |fold_variants| TreeOpts {
        main_only: false,
        player_only: false,
        fold_variants,
    };
    let leaf = loop {
        let page = si.tree_with(&ix, &node, 0, 500, opts(false));
        match page.nodes.first() {
            Some(n) if n.kind == NodeKind::Folder => node = n.id.clone(),
            _ => break node,
        }
    };
    let flat = si.tree_with(&ix, &leaf, 0, 500, opts(false));
    assert_eq!(flat.total, 7);
    assert!(flat.nodes.iter().all(|n| n.count == 0));
    let folded = si.tree_with(&ix, &leaf, 0, 500, opts(true));
    let got: Vec<_> = folded
        .nodes
        .iter()
        .map(|n| (n.label.as_str(), n.count))
        .collect();
    assert_eq!(
        got,
        [("AVENGER stalker", 2), ("Avenger Titan", 4), ("Gladius", 0)]
    );
    assert_eq!(folded.total, 3);
    // The leaf opens the representative.
    let titan = ix
        .by_guid(folded.nodes[1].id.trim_start_matches("r:"))
        .unwrap();
    assert_eq!(
        ix.entries()[titan].name,
        "EntityClassDefinition.AEGS_Avenger_Titan"
    );
    // Paging over folded leaves.
    let p = si.tree_with(&ix, &leaf, 2, 1, opts(true));
    assert_eq!((p.total, p.nodes.len()), (3, 1));
}
