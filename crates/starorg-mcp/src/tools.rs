//! The read-only tools: definitions (name, description, input schema) and their answers.
//!
//! Every tool takes an optional `channel` (default: the app's default channel, else LIVE)
//! and answers compact JSON. Lists are paginated: `limit` (default 10, at most 50) and an
//! opaque `cursor` from the previous page's `nextCursor`.
//!
//! | Tool | Datasets it loads |
//! |---|---|
//! | `search` | `game-search-index` (from item-tags + ships) |
//! | `item` | item-tags; ships, or the one detail dataset of the item's type |
//! | `where_to_buy` | item-tags + the user's UEX files, if any |
//! | `where_found` | item-tags, loot, item-sources |
//! | `how_to_obtain` | item-tags, loot, item-sources, blueprints, wikelo-trades, UEX; the owned-blueprint store |
//! | `mission` | missions, blueprints, item-sources |
//! | `compare` | as `item`, per compared key |
//! | `string` | strings-english; the channel's applied `global.ini` |
//! | `diff` | one dataset, in two channels or two cached builds |
//! | `status` | none (what is cached, memory) |

use crate::owned;
use crate::store::{ChannelSource, Store, ToolError, ToolResult, tool_err};
use serde::Serialize;
use serde_json::{Map, Value, json};
use starorg_core::ini::StockIni;
use starorg_gamedata::query::fuzzy::{self, Entry, EntryKind};
use starorg_gamedata::query::obtain::{
    BlueprintLike, Datasets, LootData, LootIndex, ObtainOptions, ObtainReport, WikeloTradeLike,
    build_loot_index, find_items, obtain,
};
use starorg_gamedata::query::shops::{
    AsOf, SHOP_ITEM_LIMIT, ShopInputs, ShopPrice, UexTerminal, limit_shop, shop_inventory,
};
use starorg_gamedata::query::types::{ContractAward, ItemTags, LootArea};
use starorg_gamedata::query::where_::where_summary;
use starorg_gamedata::{search_index, strings};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const DEFAULT_LIMIT: usize = 10;
pub const MAX_LIMIT: usize = 50;

/// A tool as `tools/list` describes it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDef {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    pub annotations: Value,
}

fn schema(channels: &[String], props: Value, required: &[&str]) -> Value {
    let mut p = props.as_object().cloned().unwrap_or_default();
    let mut ch = json!({"type": "string", "description": "Game channel (default: the app's default channel, else LIVE)."});
    if !channels.is_empty() {
        ch["enum"] = json!(channels);
    }
    p.insert("channel".into(), ch);
    json!({"type": "object", "properties": p, "required": required, "additionalProperties": false})
}

fn paging() -> Value {
    json!({
        "limit": {"type": "integer", "minimum": 1, "maximum": MAX_LIMIT, "description": "Rows per page (default 10)."},
        "cursor": {"type": "string", "description": "nextCursor of the previous page."}
    })
}

fn merge(a: Value, b: Value) -> Value {
    let mut m = a.as_object().cloned().unwrap_or_default();
    m.extend(b.as_object().cloned().unwrap_or_default());
    Value::Object(m)
}

/// Every tool, with schemas listing these channels.
pub fn definitions(channels: &[String]) -> Vec<ToolDef> {
    let ro = json!({"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false});
    let item_arg = json!({"type": "string", "description": "Item display name or class name (e.g. \"FS-9 LMG\", \"behr_lmg_ballistic_01\"); typos are matched to the closest name."});
    let d = |name, title, description, input_schema| ToolDef {
        name,
        title,
        description,
        input_schema,
        annotations: ro.clone(),
    };
    vec![
        d(
            "search",
            "Search items and ships",
            "Fuzzy search over every item and ship of the build by name, class name, maker or type. Returns keys for the other tools.",
            schema(
                channels,
                merge(
                    json!({
                        "query": {"type": "string", "description": "Words to match, e.g. \"kastak lmg\" or \"gladius\"."},
                        "kind": {"type": "string", "enum": ["any", "item", "ship"], "description": "Default any."},
                        "all": {"type": "boolean", "description": "Include world furniture, templates and unnamed records."}
                    }),
                    paging(),
                ),
                &["query"],
            ),
        ),
        d(
            "item",
            "Item or ship details",
            "One item or ship: identity, tags, loot eligibility, and its stats from the matching dataset (weapons, armor, ship components, consumables, ships).",
            schema(
                channels,
                json!({"key": {"type": "string", "description": "Class name, ship key or name (from search)."}}),
                &["key"],
            ),
        ),
        d(
            "where_to_buy",
            "Where to buy",
            "Shops selling an item, cheapest first, or what the shops at a place sell. Prices are UEX data (player-reported) and only available when the user put UEX files in the app cache; nothing is downloaded.",
            schema(
                channels,
                merge(
                    json!({
                        "item": item_arg.clone(),
                        "place": {"type": "string", "description": "Station, city, outpost, moon, planet, system or terminal (3+ letters)."}
                    }),
                    paging(),
                ),
                &[],
            ),
        ),
        d(
            "where_found",
            "Where it is found",
            "Loot spawns of an item: best areas with the container, spawn share and rough odds per box, fixed racks, and loot tables no area uses.",
            schema(
                channels,
                merge(json!({"item": item_arg.clone()}), paging()),
                &["item"],
            ),
        ),
        d(
            "how_to_obtain",
            "How to obtain",
            "Every way to get an item, easiest first: buy, contract or Wikelo reward, crafting blueprint (and whether a character owns it), loot.",
            schema(
                channels,
                json!({
                    "item": item_arg.clone(),
                    "at": {"type": "string", "description": "Only shops at this place."},
                    "owned_by": {"type": "string", "description": "Character name, \"active\" (the channel's character setting) or \"any\": mark blueprints that character owns (local owned-blueprint store)."}
                }),
                &["item"],
            ),
        ),
        d(
            "mission",
            "Missions and contracts",
            "Find missions (mission broker entries) and contracts by name, giver or type, with the blueprints and items contracts award.",
            schema(
                channels,
                merge(
                    json!({
                        "query": {"type": "string", "description": "Words to match in names, titles, givers, contracts."},
                        "type": {"type": "string", "description": "Mission type filter, e.g. mercenary."}
                    }),
                    paging(),
                ),
                &["query"],
            ),
        ),
        d(
            "compare",
            "Compare items or ships",
            "Side by side numbers of 2 to 4 items or ships of the same kind: every numeric stat they share, differing ones first.",
            schema(
                channels,
                merge(
                    json!({
                        "keys": {"type": "array", "items": {"type": "string"}, "minItems": 2, "maxItems": 4, "description": "Class names, ship keys or names."},
                        "all": {"type": "boolean", "description": "Also rows where every value is equal."}
                    }),
                    paging(),
                ),
                &["keys"],
            ),
        ),
        d(
            "string",
            "Game text",
            "A localization string by key, or a search of keys and text: the stock text and the text StarOrg Strings applied to the channel (its enhanced global.ini), if applied.",
            schema(
                channels,
                merge(
                    json!({
                        "key": {"type": "string", "description": "Exact localization key, e.g. item_NameBEHR_LMG_FS9."},
                        "search": {"type": "string", "description": "Text to find in keys and stock text (case-insensitive)."}
                    }),
                    paging(),
                ),
                &[],
            ),
        ),
        d(
            "diff",
            "Diff channels or builds",
            "What changed in one dataset between two channels (e.g. LIVE and PTU), or between a channel and an older build cached on this machine: added, removed and changed records.",
            schema(
                channels,
                merge(
                    json!({
                        "dataset": {"type": "string", "description": "Dataset: items (item-tags), ships, weapons, fps-weapons, armor, ship-components, consumables, missions, blueprints, loot, shops, strings, ... (any game-* name)."},
                        "to": {"type": "string", "description": "Channel to compare with (default: the `channel` again, with from_build)."},
                        "from_build": {"type": "string", "description": "A cached build folder of `channel` (see status) instead of its current build."},
                        "match": {"type": "string", "description": "Only records whose key contains this."}
                    }),
                    paging(),
                ),
                &["dataset"],
            ),
        ),
        d(
            "status",
            "Server status",
            "Channels, builds, which datasets are cached and what extracting them cost, and the server's memory.",
            schema(channels, json!({}), &[]),
        ),
    ]
}

// ---------------------------------------------------------------- arguments

pub struct Args<'a>(pub &'a Map<String, Value>);

impl Args<'_> {
    pub fn str(&self, k: &str) -> Option<&str> {
        self.0
            .get(k)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }
    fn need(&self, k: &str) -> ToolResult<&str> {
        self.str(k)
            .ok_or_else(|| ToolError(format!("missing argument {k}")))
    }
    fn bool(&self, k: &str) -> bool {
        self.0.get(k).and_then(Value::as_bool).unwrap_or(false)
    }
    fn limit(&self) -> usize {
        self.0
            .get("limit")
            .and_then(Value::as_u64)
            .map_or(DEFAULT_LIMIT, |n| (n as usize).clamp(1, MAX_LIMIT))
    }
    fn offset(&self) -> ToolResult<usize> {
        match self.str("cursor") {
            None => Ok(0),
            Some(c) => c
                .parse()
                .map_err(|_| ToolError(format!("bad cursor {c:?}"))),
        }
    }
}

/// `{ total, rows, nextCursor? }` for one page of `all`.
fn page<T: Serialize>(all: &[T], args: &Args<'_>) -> ToolResult<Value> {
    let (off, lim) = (args.offset()?, args.limit());
    let end = (off + lim).min(all.len());
    let rows: Vec<&T> = all
        .get(off..end)
        .map(|s| s.iter().collect())
        .unwrap_or_default();
    let mut v = json!({"total": all.len(), "rows": rows});
    if end < all.len() {
        v["nextCursor"] = json!(end.to_string());
    }
    Ok(v)
}

/// Long arrays cut to a few elements and a count, long strings cut, nulls dropped.
pub fn compact(v: &Value, depth: usize) -> Value {
    match v {
        Value::Array(a) => {
            if depth > 5 {
                return json!(format!("[{} entries]", a.len()));
            }
            let mut out: Vec<Value> = a.iter().take(12).map(|x| compact(x, depth + 1)).collect();
            if a.len() > 12 {
                out.push(json!({"_more": a.len() - 12}));
            }
            Value::Array(out)
        }
        Value::Object(o) => {
            if depth > 5 {
                return json!(format!("{{{} fields}}", o.len()));
            }
            Value::Object(
                o.iter()
                    .filter(|(_, x)| !x.is_null())
                    .map(|(k, x)| (k.clone(), compact(x, depth + 1)))
                    .collect(),
            )
        }
        Value::String(s) if s.chars().count() > 400 => {
            json!(format!("{}…", s.chars().take(400).collect::<String>()))
        }
        other => other.clone(),
    }
}

// ---------------------------------------------------------------- shared data

fn field<T: serde::de::DeserializeOwned>(v: &Value, k: &str) -> ToolResult<T> {
    serde_json::from_value(v.get(k).cloned().unwrap_or(Value::Null))
        .map_err(|e| ToolError(format!("reading {k}: {e}")))
}

struct Items {
    list: Vec<ItemTags>,
    entries: Vec<Entry>,
}

/// The parts of one build a request may need, each loaded on first use.
pub struct Ctx<'a> {
    pub store: &'a Store,
    pub ch: &'a ChannelSource,
    pub dir: PathBuf,
}

impl<'a> Ctx<'a> {
    pub fn new(store: &'a Store, args: &Args<'_>) -> ToolResult<Self> {
        let ch = store.channel(args.str("channel"))?;
        Ok(Self {
            store,
            ch,
            dir: store.build_dir(ch),
        })
    }

    fn json(&self, name: &str) -> ToolResult<Arc<Value>> {
        self.store.json(self.ch, name)
    }

    fn items(&self) -> ToolResult<Arc<Items>> {
        self.store.typed(&self.dir, "items", || {
            let list: Vec<ItemTags> = field(&*self.json("game-item-tags")?, "items")?;
            let entries = fuzzy::item_entries(&list, false);
            Ok(Items { list, entries })
        })
    }

    fn search_entries(&self, all: bool) -> ToolResult<Arc<Vec<Entry>>> {
        let build = || {
            fuzzy::from_index(&*self.json(search_index::NAME)?, all)
                .map_err(|e| ToolError(e.to_string()))
        };
        if all {
            self.store.typed(&self.dir, "search-all", build)
        } else {
            self.store.typed(&self.dir, "search", build)
        }
    }

    fn uex(
        &self,
    ) -> (
        Option<Vec<ShopPrice>>,
        Option<Vec<UexTerminal>>,
        Option<AsOf>,
    ) {
        let prices = self.store.uex("uex-item-prices.json");
        let terms = self.store.uex("uex-terminals.json");
        let rows = prices
            .as_ref()
            .and_then(|p| serde_json::from_value(p.get("prices")?.clone()).ok());
        let as_of = prices
            .as_ref()
            .and_then(|p| p.get("_fetched")?.as_str().map(|s| AsOf::Iso(s.into())));
        let terminals = terms
            .as_ref()
            .and_then(|t| serde_json::from_value(t.get("terminals")?.clone()).ok());
        (rows, terminals, as_of)
    }

    /// Datasets for the obtain report; `full` adds the reward datasets.
    fn corpus(&self, loot: bool, rewards: bool) -> ToolResult<Arc<Corpus>> {
        let name: &'static str = match (loot, rewards) {
            (true, true) => "corpus-all",
            (true, false) => "corpus-loot",
            (false, _) => "corpus-shops",
        };
        self.store.typed(&self.dir, name, || {
            let items = self.items()?;
            let mut d = Datasets {
                items: items.list.clone(),
                ..Default::default()
            };
            if loot {
                let l: LootData = serde_json::from_value((*self.json("game-loot")?).clone())
                    .map_err(|e| ToolError(format!("game-loot: {e}")))?;
                d.loot = Some(l);
                let src = self.json("game-item-sources")?;
                d.areas = Some(field::<Vec<LootArea>>(&src, "areas")?);
                d.contract_awards = Some(field::<Vec<ContractAward>>(&src, "contractAwards")?);
            }
            if rewards {
                d.blueprints = Some(field::<Vec<BlueprintLike>>(
                    &*self.json("game-blueprints")?,
                    "blueprints",
                )?);
                d.wikelo = Some(field::<Vec<WikeloTradeLike>>(
                    &*self.json("game-wikelo-trades")?,
                    "trades",
                )?);
            }
            let (shops, terminals, as_of) = self.uex();
            let shops_loaded = shops.is_some();
            d.shops = shops;
            d.terminals = terminals;
            d.shops_as_of = as_of;
            let index = d.loot.as_ref().map(build_loot_index);
            Ok(Corpus {
                d,
                index,
                shops_loaded,
            })
        })
    }
}

pub struct Corpus {
    d: Datasets,
    index: Option<LootIndex>,
    shops_loaded: bool,
}

const NO_UEX: &str = "no shop data: StarOrg Strings downloads nothing; put uex-item-prices.json and uex-terminals.json (sc-gamedata `fetch` output) in the UEX folder named by the status tool to answer shop questions";

/// `where`/`obtain`'s item resolution: names and words, else the closest fuzzy match (all
/// records with that name).
fn resolve<'a>(items: &'a Items, q: &str) -> ToolResult<Vec<&'a ItemTags>> {
    let found = find_items(&items.list, q);
    if !found.is_empty() {
        return Ok(found);
    }
    let best = fuzzy::rank_entries(&items.entries, q)
        .into_iter()
        .find(|e| e.kind == EntryKind::Item)
        .and_then(|e| items.list.iter().find(|i| i.class_name == e.key));
    match best {
        Some(b) => Ok(items.list.iter().filter(|i| i.name == b.name).collect()),
        None => tool_err(format!("no item matches {q:?}; try the search tool")),
    }
}

fn header(ctx: &Ctx<'_>) -> Value {
    json!({"channel": ctx.ch.name, "build": ctx.ch.build})
}

fn with_header(ctx: &Ctx<'_>, body: Value) -> Value {
    let mut h = header(ctx);
    if let (Some(h), Value::Object(b)) = (h.as_object_mut(), body) {
        h.extend(b);
    }
    h
}

// ---------------------------------------------------------------- tools

fn search(ctx: &Ctx<'_>, a: &Args<'_>) -> ToolResult<Value> {
    let q = a.need("query")?;
    let entries = ctx.search_entries(a.bool("all"))?;
    let kind = match a.str("kind") {
        None | Some("any") => None,
        Some("item") => Some(EntryKind::Item),
        Some("ship") => Some(EntryKind::Ship),
        Some(k) => return tool_err(format!("kind must be any, item or ship, not {k}")),
    };
    let ranked: Vec<&Entry> = fuzzy::rank_entries(&entries, q)
        .into_iter()
        .filter(|e| kind.is_none_or(|k| e.kind == k))
        .collect();
    Ok(with_header(ctx, page(&ranked, a)?))
}

/// The detail datasets for an item type.
fn detail_datasets(ty: Option<&str>) -> &'static [&'static str] {
    let ty = ty.unwrap_or("");
    if ty == "WeaponPersonal" {
        &["game-fps-weapons", "game-weapons"]
    } else if ty.starts_with("Char_Armor") || ty.starts_with("Char_Clothing") {
        &["game-armor"]
    } else if matches!(
        ty,
        "WeaponGun" | "Missile" | "MissileLauncher" | "WeaponMining" | "Bomb"
    ) {
        &["game-weapons", "game-ship-components"]
    } else if matches!(
        ty,
        "Shield"
            | "PowerPlant"
            | "Cooler"
            | "QuantumDrive"
            | "Radar"
            | "FuelTank"
            | "QuantumFuelTank"
            | "FuelIntake"
            | "LifeSupportGenerator"
            | "JumpDrive"
            | "Armor"
            | "CargoGrid"
            | "WeaponDefensive"
            | "Battery"
            | "QuantumInterdictionGenerator"
            | "EMP"
            | "SelfDestruct"
            | "TractorBeam"
            | "SalvageModifier"
            | "MiningModifier"
    ) {
        &["game-ship-components"]
    } else if matches!(ty, "FPS_Consumable" | "Food" | "Drink" | "Bottle") {
        &["game-consumables"]
    } else {
        &[]
    }
}

fn matches_record(v: &Value, id: &str, class: &str) -> bool {
    let s = |k: &str| v.get(k).and_then(Value::as_str);
    (!id.is_empty() && s("id") == Some(id))
        || s("className").is_some_and(|c| c.eq_ignore_ascii_case(class))
        || s("recordName").is_some_and(|r| {
            r.rsplit('.')
                .next()
                .is_some_and(|tail| tail.eq_ignore_ascii_case(class))
        })
}

/// The first record of `dataset` that is this item, with the collection it is in.
fn find_detail(v: &Value, id: &str, class: &str) -> Option<(String, Value)> {
    v.as_object()?.iter().find_map(|(k, list)| {
        if k.starts_with('_') {
            return None;
        }
        list.as_array()?
            .iter()
            .find(|x| matches_record(x, id, class))
            .map(|x| (k.clone(), x.clone()))
    })
}

enum Thing {
    Item(ItemTags),
    Ship(Value),
}

fn ships(ctx: &Ctx<'_>) -> ToolResult<Arc<Value>> {
    ctx.json("game-ships")
}

/// A key or name: an item class name, a ship key, else the best search hit.
fn find_thing(ctx: &Ctx<'_>, key: &str) -> ToolResult<Thing> {
    let items = ctx.items()?;
    if let Some(i) = items
        .list
        .iter()
        .find(|i| i.class_name.eq_ignore_ascii_case(key))
    {
        return Ok(Thing::Item(i.clone()));
    }
    let ship_by_key = |k: &str| -> ToolResult<Option<Value>> {
        let s = ships(ctx)?;
        Ok(s.get("vehicles").and_then(Value::as_array).and_then(|v| {
            v.iter()
                .find(|x| {
                    x.get("recordName")
                        .and_then(Value::as_str)
                        .is_some_and(|r| search_index::ship_key(r).eq_ignore_ascii_case(k))
                })
                .cloned()
        }))
    };
    let entries = ctx.search_entries(true)?;
    let best = fuzzy::rank_entries(&entries, key)
        .into_iter()
        .next()
        .cloned();
    let exact_ship = entries
        .iter()
        .any(|e| e.kind == EntryKind::Ship && e.key.eq_ignore_ascii_case(key));
    if exact_ship && let Some(s) = ship_by_key(key)? {
        return Ok(Thing::Ship(s));
    }
    match best {
        Some(e) if e.kind == EntryKind::Ship => ship_by_key(&e.key)?
            .map(Thing::Ship)
            .ok_or_else(|| ToolError(format!("ship {} not in game-ships", e.key))),
        Some(e) => items
            .list
            .iter()
            .find(|i| i.class_name == e.key)
            .cloned()
            .map(Thing::Item)
            .ok_or_else(|| ToolError(format!("item {} not in game-item-tags", e.key))),
        None => tool_err(format!("nothing matches {key:?}")),
    }
}

/// `(identity, stats)` of a thing, the stats from its detail dataset.
fn describe(ctx: &Ctx<'_>, t: &Thing) -> ToolResult<(Value, Option<Value>)> {
    match t {
        Thing::Ship(s) => {
            let mut stats = s.clone();
            if let Some(o) = stats.as_object_mut() {
                for k in ["loadout", "boundingBox", "crossSection", "description"] {
                    if let Some(v) = o.remove(k)
                        && k == "loadout"
                    {
                        let n = v.as_array().map_or(0, Vec::len);
                        o.insert("loadoutPorts".into(), json!(n));
                    }
                }
            }
            let ident = json!({
                "kind": "ship",
                "key": s.get("recordName").and_then(Value::as_str).map(search_index::ship_key),
                "name": s.get("displayName").or_else(|| s.get("name")),
                "manufacturer": s.get("manufacturer"),
                "role": s.get("role"),
                "career": s.get("career"),
            });
            Ok((ident, Some(stats)))
        }
        Thing::Item(i) => {
            let el = starorg_gamedata::query::obtain::loot_eligibility(&i.tags);
            let ident = json!({
                "kind": "item",
                "key": i.class_name,
                "name": i.name,
                "id": i.id,
                "type": i.ty,
                "subType": i.sub_type,
                "manufacturer": i.manufacturer,
                "unnamedTemplate": i.template(),
                "loot": el,
                "tags": i.tags,
                "ammoItems": i.ammo_items,
            });
            let mut stats = None;
            for ds in detail_datasets(i.ty.as_deref()) {
                if let Some((coll, rec)) = find_detail(&*ctx.json(ds)?, &i.id, &i.class_name) {
                    stats = Some(json!({"dataset": ds, "collection": coll, "record": rec}));
                    break;
                }
            }
            Ok((ident, stats))
        }
    }
}

fn item(ctx: &Ctx<'_>, a: &Args<'_>) -> ToolResult<Value> {
    let t = find_thing(ctx, a.need("key")?)?;
    let (ident, stats) = describe(ctx, &t)?;
    let mut out = with_header(ctx, json!({"item": ident}));
    if let Some(s) = stats {
        out["stats"] = compact(&s, 0);
    }
    Ok(out)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShopRow {
    terminal: String,
    location: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    price_buy: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    price_sell: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated: Option<String>,
    stale: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    not_on_live: Option<bool>,
}

fn where_to_buy(ctx: &Ctx<'_>, a: &Args<'_>) -> ToolResult<Value> {
    let place = a.str("place");
    let Some(q) = a.str("item") else {
        let Some(place) = place else {
            return tool_err("give an item, a place, or both");
        };
        let corpus = ctx.corpus(false, false)?;
        if !corpus.shops_loaded {
            return Ok(with_header(ctx, json!({"place": place, "notes": [NO_UEX]})));
        }
        let report = shop_inventory(
            &ShopInputs {
                shops: corpus.d.shops.as_deref().unwrap_or(&[]),
                terminals: corpus.d.terminals.as_deref().unwrap_or(&[]),
                items: &corpus.d.items,
                as_of: corpus.d.shops_as_of.clone(),
                stale_days: None,
            },
            place,
        );
        let cut = limit_shop(&report, Some(SHOP_ITEM_LIMIT), Some(MAX_LIMIT));
        let mut v = with_header(ctx, json!({"place": place, "rows": cut.rows}));
        if cut.list_only == Some(true) {
            v["places"] = json!(cut.places);
            v["placeCount"] = json!(cut.place_count);
            v["notes"] = json!(["too many terminals match; name one of these places"]);
        } else {
            v["terminals"] = page(&cut.terminals, a)?;
        }
        v["source"] = json!("UEX (player-reported), not game files");
        return Ok(v);
    };
    let corpus = ctx.corpus(false, false)?;
    let items = ctx.items()?;
    let found = resolve(&items, q)?;
    let mut out = Vec::new();
    for i in found.iter().take(3) {
        let r = obtain(
            &corpus.d,
            i,
            None,
            &ObtainOptions {
                at: place,
                ..Default::default()
            },
        );
        let mut rows: Vec<ShopRow> = r
            .shops
            .iter()
            .map(|s| ShopRow {
                terminal: s.terminal.clone(),
                location: s.location.clone(),
                price_buy: s.row.price_buy.filter(|p| *p > 0.0),
                price_sell: s.row.price_sell.filter(|p| *p > 0.0),
                updated: s.updated.clone(),
                stale: s.stale,
                not_on_live: (s.live == Some(false)).then_some(true),
            })
            .collect();
        rows.sort_by(|x, y| {
            let p = |r: &ShopRow| r.price_buy.unwrap_or(f64::INFINITY);
            p(x).partial_cmp(&p(y)).unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut v =
            json!({"item": {"key": i.class_name, "name": i.name}, "shops": page(&rows, a)?});
        if let Some(n) = r.shops_elsewhere.filter(|n| *n > 0) {
            v["shopsElsewhere"] = json!(n);
        }
        if let Some(c) = &r.cheapest_elsewhere {
            v["cheapestElsewhere"] = json!({"terminal": c.terminal, "location": c.location, "priceBuy": c.row.price_buy, "updated": c.updated});
        }
        out.push(v);
    }
    let mut v = with_header(
        ctx,
        json!({"matches": found.len(), "results": out, "source": "UEX (player-reported), not game files"}),
    );
    if !corpus.shops_loaded {
        v["notes"] = json!([NO_UEX]);
    }
    Ok(v)
}

fn first_report(
    ctx: &Ctx<'_>,
    corpus: &Corpus,
    q: &str,
    at: Option<&str>,
) -> ToolResult<(usize, Vec<ObtainReport>)> {
    let items = ctx.items()?;
    let found = resolve(&items, q)?;
    let reports = found
        .iter()
        .take(3)
        .map(|i| {
            obtain(
                &corpus.d,
                i,
                corpus.index.as_ref(),
                &ObtainOptions {
                    at,
                    ..Default::default()
                },
            )
        })
        .collect();
    Ok((found.len(), reports))
}

fn where_found(ctx: &Ctx<'_>, a: &Args<'_>) -> ToolResult<Value> {
    let corpus = ctx.corpus(true, false)?;
    let (n, reports) = first_report(ctx, &corpus, a.need("item")?, None)?;
    let mut out = Vec::new();
    for r in &reports {
        let areas: Vec<Value> = r
            .areas
            .iter()
            .map(|x| {
                let h = &x.best;
                json!({
                    "area": starorg_gamedata::query::obtain::area_label(&x.area),
                    "spawnPoint": h.config,
                    "kind": h.kind,
                    "container": h.entity_name.as_deref().unwrap_or(&h.entity),
                    "slotShare": h.slot_share,
                    "perRoll": h.per_roll,
                    "oddsPerBox": h.score,
                    "table": h.table,
                    "configs": x.configs,
                })
            })
            .collect();
        let tables: Vec<Value> = r
            .other_tables
            .iter()
            .take(10)
            .map(|t| json!({"table": t.table, "perRoll": t.per_roll}))
            .collect();
        out.push(json!({
            "item": {"key": r.item.class_name, "name": r.item.name},
            "loot": r.eligibility,
            "areas": page(&areas, a)?,
            "otherTables": tables,
            "otherTableCount": r.other_tables.len(),
        }));
    }
    Ok(with_header(
        ctx,
        json!({"matches": n, "results": out, "note": "odds are rough: an even pick inside a loot pool is assumed, not stated by the game data"}),
    ))
}

fn how_to_obtain(ctx: &Ctx<'_>, a: &Args<'_>) -> ToolResult<Value> {
    let corpus = ctx.corpus(true, true)?;
    let (n, reports) = first_report(ctx, &corpus, a.need("item")?, a.str("at"))?;
    let owner = match a.str("owned_by") {
        Some(who) => Some(owned::select(ctx.ch, who)?),
        None => None,
    };
    let mut out = Vec::new();
    for r in &reports {
        let s = where_summary(r, Some(corpus.shops_loaded));
        let mut v = serde_json::to_value(&s).map_err(|e| ToolError(e.to_string()))?;
        if !r.blueprints.is_empty() {
            let bps: Vec<Value> = r
                .blueprints
                .iter()
                .map(|b| {
                    let mut x = json!({"name": b.display_name.as_deref().unwrap_or(&b.name), "record": b.record_name.as_deref().unwrap_or(&b.name)});
                    if let Some(o) = &owner {
                        x["owned"] = json!(o.owns(b));
                    }
                    x
                })
                .collect();
            v["blueprints"] = json!(bps);
        }
        out.push(v);
    }
    let mut v = with_header(ctx, json!({"matches": n, "results": out}));
    if let Some(o) = &owner {
        v["ownedBy"] = json!({"characters": o.characters, "owned": o.count});
    }
    if !corpus.shops_loaded {
        v["notes"] = json!([NO_UEX]);
    }
    Ok(v)
}

fn words(q: &str) -> Vec<String> {
    q.to_lowercase()
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn all_words(hay: &str, w: &[String]) -> bool {
    let h = hay.to_lowercase();
    w.iter().all(|x| h.contains(x.as_str()))
}

fn mission(ctx: &Ctx<'_>, a: &Args<'_>) -> ToolResult<Value> {
    let w = words(a.need("query")?);
    let ty = a.str("type").map(str::to_lowercase);
    let missions = ctx.json("game-missions")?;
    let strings = |k: &Value| -> String {
        ["name", "title", "giver", "recordName", "type"]
            .iter()
            .filter_map(|f| k.get(*f).and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let rows: Vec<Value> = missions
        .get("missions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|m| all_words(&strings(m), &w))
        .filter(|m| {
            ty.as_ref().is_none_or(|t| {
                m.get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|x| x.eq_ignore_ascii_case(t))
            })
        })
        .map(|m| compact(m, 2))
        .collect();
    // Contracts: from blueprint rewards and item awards.
    let bps: Vec<BlueprintLike> = field(&*ctx.json("game-blueprints")?, "blueprints")?;
    let awards: Vec<ContractAward> = field(&*ctx.json("game-item-sources")?, "contractAwards")?;
    let mut contracts: Vec<(String, Value)> = Vec::new();
    fn slot(contracts: &mut Vec<(String, Value)>, name: &str, mission: Option<&str>) -> usize {
        if let Some(i) = contracts.iter().position(|(n, _)| n == name) {
            return i;
        }
        let missions: Vec<&str> = mission.into_iter().collect();
        contracts.push((
            name.to_string(),
            json!({"contract": name, "missions": missions, "blueprints": [], "items": []}),
        ));
        contracts.len() - 1
    }
    for b in &bps {
        for r in b.rewarded_by.iter().flatten() {
            let Some(c) = r.contract.as_deref() else {
                continue;
            };
            let hay = format!(
                "{c} {} {}",
                r.mission.as_deref().unwrap_or(""),
                r.pool.as_deref().unwrap_or("")
            );
            if !all_words(&hay, &w) {
                continue;
            }
            let i = slot(&mut contracts, c, r.mission.as_deref());
            let e = &mut contracts[i].1;
            if let Some(m) = r.mission.as_deref()
                && let Some(list) = e["missions"].as_array_mut()
                && !list.iter().any(|x| x == m)
            {
                list.push(json!(m));
            }
            let name = b.display_name.as_deref().unwrap_or(&b.name);
            if let Some(list) = e["blueprints"].as_array_mut()
                && !list.iter().any(|x| x == name)
            {
                list.push(json!(name));
            }
        }
    }
    for aw in &awards {
        if !all_words(&format!("{} {}", aw.contract, aw.generator), &w) {
            continue;
        }
        let i = slot(&mut contracts, &aw.contract, None);
        if let Some(list) = contracts[i].1["items"].as_array_mut() {
            list.push(json!({"item": aw.item, "amount": aw.amount}));
        }
    }
    let contracts: Vec<Value> = contracts.into_iter().map(|(_, v)| compact(&v, 0)).collect();
    Ok(with_header(
        ctx,
        json!({"missions": page(&rows, a)?, "contracts": page(&contracts, a)?}),
    ))
}

/// Numeric leaves as dotted paths.
fn numbers(v: &Value, prefix: &str, out: &mut Vec<(String, f64)>) {
    match v {
        Value::Number(n) => {
            if let Some(x) = n.as_f64() {
                out.push((prefix.to_string(), x));
            }
        }
        Value::Object(o) => {
            for (k, x) in o {
                if k == "id" {
                    continue;
                }
                let p = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                numbers(x, &p, out);
            }
        }
        Value::Array(a) if a.len() <= 16 => {
            for (i, x) in a.iter().enumerate() {
                numbers(x, &format!("{prefix}[{i}]"), out);
            }
        }
        _ => {}
    }
}

fn compare(ctx: &Ctx<'_>, a: &Args<'_>) -> ToolResult<Value> {
    let keys: Vec<String> =
        a.0.get("keys")
            .and_then(Value::as_array)
            .map(|v| {
                v.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
    if !(2..=4).contains(&keys.len()) {
        return tool_err("keys: give 2 to 4 class names, ship keys or names");
    }
    let mut idents = Vec::new();
    let mut nums: Vec<Vec<(String, f64)>> = Vec::new();
    for k in &keys {
        let t = find_thing(ctx, k)?;
        let (ident, stats) = describe(ctx, &t)?;
        let mut n = Vec::new();
        if let Some(s) = &stats {
            let rec = s.get("record").unwrap_or(s);
            numbers(rec, "", &mut n);
        }
        idents.push(ident);
        nums.push(n);
    }
    // Rows in the first-seen field order, fields at least two share.
    let mut fields: Vec<String> = Vec::new();
    for n in &nums {
        for (f, _) in n {
            if !fields.contains(f) {
                fields.push(f.clone());
            }
        }
    }
    let mut rows: Vec<(bool, Value)> = Vec::new();
    for f in fields {
        let vals: Vec<Option<f64>> = nums
            .iter()
            .map(|n| n.iter().find(|(k, _)| *k == f).map(|(_, v)| *v))
            .collect();
        if vals.iter().filter(|v| v.is_some()).count() < 2 {
            continue;
        }
        let first = vals.iter().flatten().next().copied();
        let differ = vals.iter().any(|v| *v != first);
        if !differ && !a.bool("all") {
            continue;
        }
        rows.push((differ, json!({"field": f, "values": vals})));
    }
    rows.sort_by_key(|(d, _)| !*d);
    let rows: Vec<Value> = rows.into_iter().map(|(_, v)| v).collect();
    let mut v = with_header(
        ctx,
        json!({"items": idents.iter().map(|i| json!({"key": i["key"], "name": i["name"], "kind": i["kind"]})).collect::<Vec<_>>()}),
    );
    v["rows"] = page(&rows, a)?;
    if rows.is_empty() {
        v["notes"] = json!([
            "no numeric stats in common (different kinds of item, or no stats dataset for their type)"
        ]);
    }
    Ok(v)
}

/// The channel's loose `global.ini`, parsed, and whether StarOrg Strings wrote it.
pub struct Applied {
    pub ini: StockIni,
    pub ours: bool,
}

type AppliedCache = Mutex<Option<(PathBuf, u64, std::time::SystemTime, Arc<Applied>)>>;

fn applied(cache: &AppliedCache, store: &Store, ch: &ChannelSource) -> Option<Arc<Applied>> {
    let path = starorg_core::paths::find_existing_ci(&ch.dir, starorg_core::config::LOOSE_INI)
        .ok()
        .flatten()?;
    let meta = std::fs::metadata(&path).ok()?;
    let (len, mtime) = (meta.len(), meta.modified().ok()?);
    let mut c = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((p, l, t, a)) = c.as_ref()
        && *p == path
        && *l == len
        && *t == mtime
    {
        return Some(a.clone());
    }
    let bytes = std::fs::read(&path).ok()?;
    let sha: String = {
        use sha2::Digest;
        sha2::Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    };
    let ours = store
        .cfg
        .our_outputs
        .iter()
        .any(|s| s.eq_ignore_ascii_case(&sha));
    let a = Arc::new(Applied {
        ini: StockIni::parse(bytes),
        ours,
    });
    *c = Some((path, len, mtime, a.clone()));
    Some(a)
}

fn string(ctx: &Ctx<'_>, a: &Args<'_>, cache: &AppliedCache) -> ToolResult<Value> {
    let data = ctx.json(strings::NAME)?;
    let table = data
        .get("strings")
        .and_then(Value::as_object)
        .ok_or_else(|| ToolError("the string table has no strings".into()))?;
    let applied = applied(cache, ctx.store, ctx.ch);
    let row = |k: &str, stock: Option<&Value>| -> Value {
        let mut v = json!({"key": k, "stock": stock});
        if let Some(ap) = &applied {
            let now = ap.ini.value(k);
            if now != stock.and_then(Value::as_str) {
                v[if ap.ours { "enhanced" } else { "installed" }] = json!(now);
            }
        }
        v
    };
    let applied_note = match &applied {
        Some(a) if a.ours => {
            "enhanced = the text StarOrg Strings applied to this channel (only where it differs from stock)"
        }
        Some(_) => {
            "installed = text of a global.ini in the channel that StarOrg Strings did not write (another tool's)"
        }
        None => "nothing applied to this channel: the game shows the stock text",
    };
    let mut v = if let Some(k) = a.str("key") {
        let stock = table.get(k).or_else(|| {
            table
                .iter()
                .find(|(x, _)| x.eq_ignore_ascii_case(k))
                .map(|(_, v)| v)
        });
        if stock.is_none() && applied.as_ref().and_then(|ap| ap.ini.value(k)).is_none() {
            return tool_err(format!("no string key {k} in this build"));
        }
        with_header(ctx, json!({"string": row(k, stock)}))
    } else if let Some(q) = a.str("search") {
        let q = q.to_lowercase();
        let hits: Vec<Value> = table
            .iter()
            .filter(|(k, v)| {
                k.to_lowercase().contains(&q)
                    || v.as_str().is_some_and(|s| s.to_lowercase().contains(&q))
            })
            .map(|(k, v)| row(k, Some(v)))
            .collect();
        with_header(ctx, json!({"strings": page(&hits, a)?}))
    } else {
        return tool_err("give a key or a search text");
    };
    v["note"] = json!(applied_note);
    Ok(v)
}

/// `items` -> `game-item-tags`, `ships` -> `game-ships`, `strings` -> the string table.
fn dataset_name(s: &str) -> ToolResult<String> {
    let s = s.trim().to_ascii_lowercase();
    let s = s.strip_suffix(".json").unwrap_or(&s).to_string();
    let base = match s.as_str() {
        "items" | "item-tags" => "game-item-tags".to_string(),
        "strings" | "string" | "text" => strings::NAME.to_string(),
        "components" => "game-ship-components".to_string(),
        "wikelo" => "game-wikelo-trades".to_string(),
        other if other.starts_with("game-") => other.to_string(),
        other => format!("game-{other}"),
    };
    if crate::worker::extractable().contains(&base.as_str()) {
        Ok(base)
    } else {
        tool_err(format!(
            "unknown dataset {s}; one of: {}",
            crate::worker::extractable()
                .iter()
                .map(|d| d.trim_start_matches("game-"))
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

const KEY_FIELDS: &[&str] = &[
    "id",
    "recordName",
    "className",
    "guid",
    "key",
    "code",
    "name",
];

fn key_field(a: &[Value], b: &[Value]) -> Option<&'static str> {
    KEY_FIELDS.iter().copied().find(|k| {
        a.iter()
            .chain(b)
            .all(|x| x.get(*k).is_some_and(|v| v.is_string() || v.is_number()))
    })
}

fn key_of(v: &Value, k: Option<&str>, i: usize) -> String {
    match k.and_then(|k| v.get(k)) {
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => format!("#{i}"),
    }
}

fn label(v: &Value) -> Option<String> {
    ["displayName", "name", "className", "recordName"]
        .iter()
        .find_map(|k| v.get(*k).and_then(Value::as_str).map(str::to_string))
}

fn leaf_diff(a: &Value, b: &Value, path: &str, out: &mut Vec<Value>) {
    if out.len() >= 8 || a == b {
        return;
    }
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys().filter(|k| !x.contains_key(*k))) {
                let p = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                leaf_diff(
                    x.get(k).unwrap_or(&Value::Null),
                    y.get(k).unwrap_or(&Value::Null),
                    &p,
                    out,
                );
            }
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                leaf_diff(p, q, &format!("{path}[{i}]"), out);
            }
        }
        _ => out.push(json!({"path": path, "from": compact(a, 4), "to": compact(b, 4)})),
    }
}

fn diff(ctx: &Ctx<'_>, a: &Args<'_>) -> ToolResult<Value> {
    let ds = dataset_name(a.need("dataset")?)?;
    let from_ch = ctx.ch;
    let to_ch = ctx.store.channel(a.str("to").or(Some(&from_ch.name)))?;
    let (from_label, from_val) = match a.str("from_build") {
        Some(b) => {
            let builds = ctx.store.cached_builds(from_ch);
            let Some((name, dir)) = builds.iter().find(|(n, _)| n == b || n.starts_with(b)) else {
                return tool_err(format!(
                    "no cached build {b} for {}; cached: {}",
                    from_ch.name,
                    builds
                        .iter()
                        .map(|(n, _)| n.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            };
            (
                format!("{} {name}", from_ch.name),
                ctx.store.json_in(from_ch, dir, &ds, false)?,
            )
        }
        None => {
            if to_ch.name == from_ch.name {
                return tool_err(
                    "give `to` (another channel) or `from_build` (an older cached build)",
                );
            }
            (
                format!(
                    "{} {}",
                    from_ch.name,
                    from_ch.build.as_deref().unwrap_or("?")
                ),
                ctx.store.json(from_ch, &ds)?,
            )
        }
    };
    let to_val = ctx.store.json(to_ch, &ds)?;
    let filter = a.str("match").map(str::to_lowercase);
    let keep = |k: &str| filter.as_ref().is_none_or(|f| k.to_lowercase().contains(f));
    let mut changes: Vec<Value> = Vec::new();
    let mut summary = Map::new();
    let empty = Map::new();
    let (fa, fb) = (
        from_val.as_object().unwrap_or(&empty),
        to_val.as_object().unwrap_or(&empty),
    );
    let collections: Vec<&String> = fb
        .keys()
        .chain(fa.keys().filter(|k| !fb.contains_key(*k)))
        .filter(|k| !k.starts_with('_') && k.as_str() != "summary")
        .collect();
    for coll in collections {
        let (x, y) = (fa.get(coll), fb.get(coll));
        let (mut added, mut removed, mut changed) = (0, 0, 0);
        match (x, y) {
            (Some(Value::Array(p)), Some(Value::Array(q))) => {
                let k = key_field(p, q);
                let pm: Vec<(String, &Value)> = p
                    .iter()
                    .enumerate()
                    .map(|(i, v)| (key_of(v, k, i), v))
                    .collect();
                let qm: Vec<(String, &Value)> = q
                    .iter()
                    .enumerate()
                    .map(|(i, v)| (key_of(v, k, i), v))
                    .collect();
                let pi: std::collections::HashMap<&str, &Value> =
                    pm.iter().map(|(k, v)| (k.as_str(), *v)).collect();
                let qi: std::collections::HashMap<&str, &Value> =
                    qm.iter().map(|(k, v)| (k.as_str(), *v)).collect();
                for (key, v) in &qm {
                    match pi.get(key.as_str()) {
                        None => {
                            added += 1;
                            if keep(key) {
                                changes.push(json!({"collection": coll, "key": key, "name": label(v), "change": "added"}));
                            }
                        }
                        Some(old) if *old != *v => {
                            changed += 1;
                            if keep(key) {
                                let mut f = Vec::new();
                                leaf_diff(old, v, "", &mut f);
                                changes.push(json!({"collection": coll, "key": key, "name": label(v), "change": "changed", "fields": f}));
                            }
                        }
                        _ => {}
                    }
                }
                for (key, v) in &pm {
                    if !qi.contains_key(key.as_str()) {
                        removed += 1;
                        if keep(key) {
                            changes.push(json!({"collection": coll, "key": key, "name": label(v), "change": "removed"}));
                        }
                    }
                }
            }
            (Some(Value::Object(p)), Some(Value::Object(q))) => {
                for (key, v) in q {
                    match p.get(key) {
                        None => {
                            added += 1;
                            if keep(key) {
                                changes.push(json!({"collection": coll, "key": key, "change": "added", "to": compact(v, 4)}));
                            }
                        }
                        Some(old) if old != v => {
                            changed += 1;
                            if keep(key) {
                                changes.push(json!({"collection": coll, "key": key, "change": "changed", "from": compact(old, 4), "to": compact(v, 4)}));
                            }
                        }
                        _ => {}
                    }
                }
                for (key, v) in p {
                    if !q.contains_key(key) {
                        removed += 1;
                        if keep(key) {
                            changes.push(json!({"collection": coll, "key": key, "change": "removed", "from": compact(v, 4)}));
                        }
                    }
                }
            }
            _ => {}
        }
        if added + removed + changed > 0 {
            summary.insert(
                coll.clone(),
                json!({"added": added, "removed": removed, "changed": changed}),
            );
        }
    }
    let to_label = format!("{} {}", to_ch.name, to_ch.build.as_deref().unwrap_or("?"));
    Ok(json!({
        "dataset": ds,
        "from": from_label,
        "to": to_label,
        "summary": summary,
        "changes": page(&changes, a)?,
    }))
}

fn status(store: &Store) -> ToolResult<Value> {
    let channels: Vec<Value> = store
        .cfg
        .channels
        .iter()
        .map(|ch| {
            let current = store.build_dir(ch);
            let builds: Vec<Value> = store
                .cached_builds(ch)
                .into_iter()
                .map(|(name, dir)| {
                    let mut ds: Vec<String> = std::fs::read_dir(&dir)
                        .into_iter()
                        .flatten()
                        .flatten()
                        .filter_map(|e| e.file_name().into_string().ok())
                        .filter_map(|n| n.strip_suffix(".json").map(str::to_string))
                        .filter(|n| n.starts_with("game-"))
                        .collect();
                    ds.sort();
                    json!({"build": name, "current": dir == current, "datasets": ds,
                           "extraction": Store::measurements(&dir)})
                })
                .collect();
            json!({"channel": ch.name, "build": ch.build,
                   "default": store.default_channel().is_some_and(|d| d.name == ch.name),
                   "cached": builds})
        })
        .collect();
    Ok(json!({
        "channels": channels,
        "server": {"rss": crate::rss::now(), "loaded": store.loaded()},
        "uexFolder": store.cfg.uex_dir.as_ref().map(|d| d.display().to_string()),
        "uexPresent": store.uex("uex-item-prices.json").is_some(),
        "network": "none: this server makes no network request and listens on this computer only",
    }))
}

/// The tool layer: dispatch by name.
pub struct Tools {
    pub store: Store,
    applied: AppliedCache,
}

impl Tools {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            applied: Mutex::new(None),
        }
    }

    pub fn definitions(&self) -> Vec<ToolDef> {
        definitions(&self.store.channel_names())
    }

    pub fn call(&self, name: &str, args: &Map<String, Value>) -> ToolResult<Value> {
        let a = Args(args);
        if name == "status" {
            return status(&self.store);
        }
        let ctx = Ctx::new(&self.store, &a)?;
        match name {
            "search" => search(&ctx, &a),
            "item" => item(&ctx, &a),
            "where_to_buy" => where_to_buy(&ctx, &a),
            "where_found" => where_found(&ctx, &a),
            "how_to_obtain" => how_to_obtain(&ctx, &a),
            "mission" => mission(&ctx, &a),
            "compare" => compare(&ctx, &a),
            "string" => string(&ctx, &a, &self.applied),
            "diff" => diff(&ctx, &a),
            other => tool_err(format!("unknown tool {other}")),
        }
    }
}

/// Paths a test or caller may want: the UEX folder under a cache dir.
pub fn uex_dir(cache: &Path) -> PathBuf {
    cache.join("uex")
}
