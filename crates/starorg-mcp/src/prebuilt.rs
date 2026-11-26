//! Prebuilt datasets from `voidput/sc-gamedata-dumps`, to skip a local extraction.
//!
//! An augment for speed, never a replacement: the channel's own `Data.p4k` stays the source
//! of truth, and every dataset that is not taken from a dump (none published for the build,
//! not one this app can use, a failed check, no network, too slow, the setting off) is
//! extracted locally as before. The CLI never downloads (it links no HTTP client); the app
//! passes a [`DumpPrebuilt`] over `starorg-net`'s GitHub transport while Settings -> Game
//! data -> "Download prebuilt game data" is on.
//!
//! For build `B` of channel `C`:
//!
//! 1. The release tagged exactly `B` (one release and tag per game build, `B` a build tag:
//!    [`dump::is_build_tag`], never a revision tag `B-r<n>`). None: no dump, and nothing else
//!    is tried (never another build).
//! 2. The tag's commit and its tree (git blob ids and sizes). When the tree lists
//!    `build.json` (`{"build","channel","revision",...,"extractor":{"commit","dirty"},
//!    "files":[{"name","sha256","bytes","schema"}]}`, unknown keys ignored) it must match its
//!    blob id and read, or nothing of the tag is used: its `build` must be `B`, its `channel`
//!    (else the release's) `C`, a dirty extractor checkout is refused, and its `files` give
//!    each dataset's SHA-256, size and schema. Otherwise the tree's blob id and size check
//!    each file.
//! 3. The revision: `B` moves on a republish, so `build.json`'s `revision` (1 on the first
//!    publish, one more per republish) names what it holds now, with the immutable tag
//!    `B-r<n>` each publish pushes: `r<n>-<commit12>` when that tag exists and is `B`'s
//!    commit, else (dumps published before revisions, or a tag that disagrees) the short
//!    commit id alone.
//! 4. Per dataset, only those in [`DUMP_USE`]: the file is downloaded at that commit, checked,
//!    and must pass [`DumpUse::accepts`]; [`Prebuilt::fetch`] hands its bytes over. The store
//!    fills redacted prose from the local string table ([`fill_prose`]) into a temp file and
//!    swaps it in with its stamp (`dump:<B>@<revision>` and the tag's ref, under
//!    [`rule_key`], so a changed rule or a tag added to [`KNOWN_BAD`] later takes the dataset
//!    out of service).
//! 5. A republish: reading a dataset taken from `B`, the store asks [`Prebuilt::head`], which
//!    never waits on the network. The first time per build it starts a background check
//!    (one request for the tag's ref, 5 s at most); while that runs, or offline, the cached
//!    copy is served. The same ref as the stamp's: current. Another one: a background
//!    resolve (steps 1-3) gives the revision; when it differs, the next read takes the
//!    dataset again (or extracts it when the new revision is refused, still serving the old
//!    copy should that fail too).
//!
//! Answers per tag are remembered for the process (one [`DumpPrebuilt`] for every channel):
//! a resolved tag and a checked ref for the run, a missing release for an hour, a network
//! failure (or a download budget of [`DOWNLOAD_BUDGET`] used up) for ten minutes, so an
//! offline or slow "Read game data" fails over to the local extraction once, not per
//! dataset. No lock is held across a request.

use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use starorg_core::source::dump::{self, DumpTransport, git_blob_sha1, is_build_tag};
use starorg_gamedata::helpers_world::js_trim;
use starorg_gamedata::localization::{
    Localization, is_placeholder_name, localize_str, lookup_string,
};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// A dataset [`Prebuilt::fetch`] downloaded, verified and accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    pub tag: String,
    /// What the tag held when it was read: `r<n>-<commit12>`, else the short commit id
    /// ([`DumpHead::revision`]).
    pub revision: String,
    /// The tag's ref then ([`DumpHead::tag_ref`]).
    pub tag_ref: String,
    /// The file as published.
    pub bytes: Vec<u8>,
}

/// What the dump tag of a build holds now, as far as [`Prebuilt::head`] knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DumpHead {
    /// The tag's ref ([`DumpTransport::tag_ref`]): the same ref, the same revision.
    pub tag_ref: String,
    /// Its revision, once resolved for that ref.
    pub revision: Option<String>,
}

/// A source of prebuilt datasets ([`crate::store::StoreConfig::prebuilt`]).
pub trait Prebuilt: Send + Sync {
    /// The user's switch now. Off: nothing is fetched, and datasets taken from a dump before
    /// are not served (they are extracted locally).
    fn enabled(&self) -> bool;
    /// `<dataset>` of `build` of `channel`, verified and in a shape this app reads.
    /// `Err(why not)` otherwise (the caller extracts).
    fn fetch(&self, channel: &str, build: &str, dataset: &str) -> Result<Fetched, String>;
    /// What the dump tag of `build` holds now, without waiting: `None` while that is being
    /// asked in the background (started by this call, once per build), offline, the switch
    /// off or no dump, and then a dataset taken from it before is served as it is. When the
    /// ref is not `known_ref` (a stamp's), the revision is resolved in the background too.
    fn head(&self, build: &str, known_ref: Option<&str>) -> Option<DumpHead> {
        let _ = (build, known_ref);
        None
    }
    /// Bumped whenever a background answer lands: the store then looks at the datasets it
    /// holds in memory again.
    fn generation(&self) -> u64 {
        0
    }
}

/// Bumped when [`DumpUse::accepts`] or [`fill_prose`] changes what they take for every
/// dataset: each one taken from a dump is then checked again (fetched again, or extracted).
/// A change for some datasets only bumps their [`ROW_RULES`] instead.
pub const RULES_VERSION: u32 = 1;

/// Rule versions of single [`DUMP_USE`] rows (1 when not listed), part of [`rule_key`]:
/// refining (its processes' and refinables' fill rules), manufacturers (kept as the table
/// says) and starmap (`not_before` against the files' `_extracted`) are 2.
pub const ROW_RULES: &[(&str, u32)] = &[
    ("game-refining", 2),
    ("game-manufacturers", 2),
    ("game-starmap", 2),
];

/// Whether a dataset without `_schema` may be taken at all (only for the tags its row lists
/// in [`DumpUse::verified`]). Set to `false` once sc-gamedata stamps every dataset with
/// `_schema` (planned): then only stamped files are taken.
pub const ACCEPT_UNSCHEMAED: bool = true;

/// Largest dataset taken (the biggest is about 6 MB): it is parsed in this process.
pub const MAX_BYTES: u64 = 32 * 1024 * 1024;

/// Time all downloads of one build may take per run; past it, everything is extracted.
pub const DOWNLOAD_BUDGET: Duration = Duration::from_secs(60);

/// Reads a dataset with one of this app's typed readers.
pub type TypedCheck = fn(&Value) -> Result<(), String>;

/// What this app takes from a dump, per dataset. One table: bump a schema, a needed path or a
/// cut-off here (and in `docs/mcp.md`, "Prebuilt datasets").
#[derive(Debug, Clone, Copy)]
pub struct DumpUse {
    pub dataset: &'static str,
    /// `_schema` versions read (`<dataset>@<n>`, in the file or `build.json`). When this
    /// app's code for the dataset changes in a way older dumps lack, drop the old version.
    pub schemas: &'static [u32],
    /// Tags whose file without `_schema` was checked against this app's output and may be
    /// taken by shape ([`ACCEPT_UNSCHEMAED`]).
    pub verified: &'static [&'static str],
    /// Dumps extracted before this date (RFC 3339 prefix, `2026-12-01`; the file's
    /// `_extracted`, else the release date) are not good enough: set when a fix to this
    /// app's extraction is newer than them.
    pub not_before: Option<&'static str>,
    /// Paths the dump must carry, not null (`summary.withAmmo`).
    pub needs: &'static [&'static str],
    /// Fields `_redacted` may name: prose this app fills from the local string table
    /// ([`fill_prose`]). Any other redaction (and a redacted string table) means the
    /// dataset is extracted locally.
    pub may_strip: &'static [&'static str],
    /// The typed reader this app reads the dataset with, when there is one: the dump must
    /// read with it.
    pub typed: Option<TypedCheck>,
}

fn as_type<T: serde::de::DeserializeOwned>(v: &Value, field: Option<&str>) -> Result<(), String> {
    let v = match field {
        Some(f) => v.get(f).cloned().unwrap_or(Value::Null),
        None => v.clone(),
    };
    serde_json::from_value::<T>(v)
        .map(drop)
        .map_err(|e| format!("does not read as this app reads it: {e}"))
}

fn typed_loot(v: &Value) -> Result<(), String> {
    as_type::<starorg_gamedata::query::obtain::LootData>(v, None)
}
fn typed_item_sources(v: &Value) -> Result<(), String> {
    use starorg_gamedata::query::types::{ContractAward, LootArea};
    as_type::<Vec<LootArea>>(v, Some("areas"))?;
    as_type::<Vec<ContractAward>>(v, Some("contractAwards"))
}
fn typed_item_tags(v: &Value) -> Result<(), String> {
    as_type::<Vec<starorg_gamedata::query::types::ItemTags>>(v, Some("items"))
}
fn typed_blueprints(v: &Value) -> Result<(), String> {
    as_type::<Vec<starorg_gamedata::query::obtain::BlueprintLike>>(v, Some("blueprints"))
}
fn typed_wikelo(v: &Value) -> Result<(), String> {
    as_type::<Vec<starorg_gamedata::query::obtain::WikeloTradeLike>>(v, Some("trades"))
}
fn typed_mining_spawns(v: &Value) -> Result<(), String> {
    as_type::<starorg_gamedata::query::mined::MiningSpawns>(v, None)
}
fn typed_commodities(v: &Value) -> Result<(), String> {
    as_type::<starorg_gamedata::query::quality::Commodities>(v, None)
}
fn typed_npc(v: &Value) -> Result<(), String> {
    as_type::<starorg_gamedata::domains::npc_loadouts::NpcLoadouts>(v, None)
}

/// The tag checked by hand (this app's 4.10.201.48085 output against the published
/// 4.10.200.58746 files, and the port byte-identical to sc-gamedata on 4.10.200.58746).
const V200: &[&str] = &["4.10.200.58746"];

/// The datasets this app may take from a dump. Everything else (`game-strings-english`:
/// published with name keys only; the derived search index) is always extracted locally.
#[rustfmt::skip]
pub const DUMP_USE: &[DumpUse] = &[
    DumpUse { dataset: "game-ammo", schemas: &[1], verified: V200, not_before: None, needs: &["ammo", "summary"], may_strip: &[], typed: None },
    DumpUse { dataset: "game-armor", schemas: &[1], verified: V200, not_before: None, needs: &["armor", "clothing", "bySlot", "summary"], may_strip: &[], typed: None },
    DumpUse { dataset: "game-blueprints", schemas: &[1], verified: V200, not_before: None, needs: &["blueprints", "rewardPools", "resourceUse", "summary"], may_strip: &[], typed: Some(typed_blueprints) },
    DumpUse { dataset: "game-cargo", schemas: &[1], verified: V200, not_before: None, needs: &["manifests", "loading", "summary"], may_strip: &[], typed: None },
    DumpUse { dataset: "game-commodities", schemas: &[1], verified: V200, not_before: None, needs: &["resources", "items", "damage", "byGroup", "summary"], may_strip: &["description"], typed: Some(typed_commodities) },
    DumpUse { dataset: "game-consumables", schemas: &[1], verified: V200, not_before: None, needs: &["consumables", "subtypes", "summary"], may_strip: &[], typed: None },
    DumpUse { dataset: "game-containers", schemas: &[1], verified: V200, not_before: None, needs: &["containers", "summary"], may_strip: &[], typed: None },
    DumpUse { dataset: "game-fps-weapons", schemas: &[1], verified: &[], not_before: None, needs: &["weapons", "summary"], may_strip: &[], typed: None },
    DumpUse { dataset: "game-item-sources", schemas: &[1], verified: V200, not_before: None, needs: &["areas", "contractAwards", "summary"], may_strip: &[], typed: Some(typed_item_sources) },
    // `ammoItems` on weapon rows (WHERE TO GET's ammo lines); `summary.withAmmo` says the
    // extractor wrote them.
    DumpUse { dataset: "game-item-tags", schemas: &[1], verified: &[], not_before: None, needs: &["items", "summary.withAmmo", "summary.tags"], may_strip: &[], typed: Some(typed_item_tags) },
    DumpUse { dataset: "game-law", schemas: &[1], verified: V200, not_before: None, needs: &["crimes", "jurisdictions", "summary"], may_strip: &[], typed: None },
    DumpUse { dataset: "game-loot", schemas: &[1], verified: V200, not_before: None, needs: &["tables", "archetypes", "secondaryChoices", "filters", "specialEvents", "summary"], may_strip: &[], typed: Some(typed_loot) },
    DumpUse { dataset: "game-manufacturers", schemas: &[1], verified: V200, not_before: None, needs: &["manufacturers", "byCode", "summary"], may_strip: &["description"], typed: None },
    DumpUse { dataset: "game-mining", schemas: &[1], verified: V200, not_before: None, needs: &["mineableElements", "miningModules", "elementSignatures", "summary"], may_strip: &[], typed: None },
    DumpUse { dataset: "game-mining-spawns", schemas: &[1], verified: V200, not_before: None, needs: &["locations", "rocks", "oreSpawns", "oreLocations", "summary"], may_strip: &[], typed: Some(typed_mining_spawns) },
    // A long title stripped (`title over 120 characters`) also changes `name`: refused.
    DumpUse { dataset: "game-missions", schemas: &[1], verified: V200, not_before: None, needs: &["missions", "byGiver", "summary"], may_strip: &["description"], typed: None },
    // This repository's own domain; sc-gamedata ports it to the same shape.
    DumpUse { dataset: "game-npc-loadouts", schemas: &[1], verified: &[], not_before: None, needs: &["actors", "byLootTable", "summary"], may_strip: &[], typed: Some(typed_npc) },
    DumpUse { dataset: "game-quantum", schemas: &[1], verified: V200, not_before: None, needs: &["rules", "summary"], may_strip: &[], typed: None },
    DumpUse { dataset: "game-refining", schemas: &[1], verified: V200, not_before: None, needs: &["processes", "refinables", "workloadTiers", "settings", "summary"], may_strip: &["description", "rawDescription", "refinedDescription"], typed: None },
    DumpUse { dataset: "game-reputation", schemas: &[1], verified: V200, not_before: None, needs: &["standings", "byOrganisation", "summary"], may_strip: &[], typed: None },
    DumpUse { dataset: "game-salvage", schemas: &[1], verified: V200, not_before: None, needs: &["structural", "repairBeam", "summary"], may_strip: &[], typed: None },
    DumpUse { dataset: "game-ship-components", schemas: &[1], verified: V200, not_before: None, needs: &["armor", "missiles", "mounts", "quantumDrives", "shields", "weapons", "summary"], may_strip: &[], typed: None },
    DumpUse { dataset: "game-ships", schemas: &[1], verified: &[], not_before: None, needs: &["vehicles", "byManufacturer", "byRole", "summary"], may_strip: &["description"], typed: None },
    DumpUse { dataset: "game-shops", schemas: &[1], verified: V200, not_before: None, needs: &["selling", "commodities", "buying", "summary"], may_strip: &[], typed: None },
    // Plain containers at system level (Nyx's glaciem ring: Levski, Delamar) since
    // sc-gamedata 350e7053b; older dumps lack their positions.
    DumpUse { dataset: "game-starmap", schemas: &[1], verified: &[], not_before: Some("2026-11-26"), needs: &["locations", "byType", "summary"], may_strip: &["description"], typed: None },
    DumpUse { dataset: "game-weapons", schemas: &[1], verified: V200, not_before: None, needs: &["shipWeapons", "fpsWeapons", "missiles", "summary"], may_strip: &[], typed: None },
    DumpUse { dataset: "game-wikelo-trades", schemas: &[1], verified: V200, not_before: None, needs: &["trades", "totalDemand", "summary"], may_strip: &[], typed: Some(typed_wikelo) },
];

/// Published datasets known to differ from what this app extracts for the same build, by
/// tag: never used, also when taken before the entry was added.
pub const KNOWN_BAD: &[(&str, &str, &str)] = &[
    // sc-gamedata parsed a records folder that still held 65 files of an older extraction
    // (`crates/starorg-gamedata/README.md`, Status): 54 renamed vehicles, a melee tint and a
    // renamed box that are not in this build.
    (
        "4.10.200.58746",
        "game-ships",
        "holds records of an older extraction",
    ),
    (
        "4.10.200.58746",
        "game-fps-weapons",
        "holds records of an older extraction",
    ),
    (
        "4.10.200.58746",
        "game-item-tags",
        "holds records of an older extraction",
    ),
];

/// The [`DUMP_USE`] row of a dataset.
pub fn dump_use(dataset: &str) -> Option<&'static DumpUse> {
    DUMP_USE.iter().find(|u| u.dataset == dataset)
}

/// Why a published dataset of `tag` is never used, when [`KNOWN_BAD`] says so.
pub fn known_bad(tag: &str, dataset: &str) -> Option<&'static str> {
    KNOWN_BAD
        .iter()
        .find(|(t, d, _)| *t == tag && *d == dataset)
        .map(|(_, _, why)| *why)
}

/// The rule a prebuilt dataset was taken under: [`RULES_VERSION`] and its [`DUMP_USE`] row.
/// Part of a dump stamp's key ([`crate::stamps::expected_code`]): a changed row takes it out
/// of service.
pub fn rule_key(dataset: &str) -> String {
    let row = dump_use(dataset).map(|u| {
        format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{}",
            u.schemas,
            u.verified,
            u.not_before,
            u.needs,
            u.may_strip,
            u.typed.is_some()
        )
    });
    let mut h = Sha256::new();
    h.update(format!(
        "{RULES_VERSION}:{ACCEPT_UNSCHEMAED}:{}",
        row.unwrap_or_default()
    ));
    // Version 1 adds nothing, so a row's key stays what it was before row versions.
    if let Some((_, n)) = ROW_RULES.iter().find(|(d, _)| *d == dataset)
        && *n > 1
    {
        h.update(format!("|rules {n}"));
    }
    h.finalize()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn has_path(v: &Value, path: &str) -> bool {
    let mut at = v;
    for seg in path.split('.') {
        match at.get(seg) {
            Some(x) => at = x,
            None => return false,
        }
    }
    !at.is_null()
}

/// The prose fields a dump lacks (its `_redacted` list), `None` when it is not redacted.
pub fn redacted(doc: &Value) -> Option<Vec<String>> {
    let r = doc.get("_redacted")?;
    Some(
        r.as_array()
            .map(|a| {
                a.iter()
                    .map(|x| x.as_str().unwrap_or("?").to_string())
                    .collect()
            })
            .unwrap_or_else(|| vec![r.to_string()]),
    )
}

impl DumpUse {
    /// `Ok` when this dataset, as published for `tag` (its release dated `published`, with `schema`
    /// from `build.json` when the file has none), is one this app reads.
    pub fn accepts(
        &self,
        doc: &Value,
        tag: &str,
        published: Option<&str>,
        schema: Option<&str>,
    ) -> Result<(), String> {
        let name = self.dataset;
        if !doc.is_object() {
            return Err(format!("{name}: not a dataset object"));
        }
        match doc.get("_build").and_then(Value::as_str) {
            Some(b) if b == tag => {}
            Some(b) => return Err(format!("{name} is stamped {b}, not {tag}")),
            None => return Err(format!("{name} carries no _build stamp")),
        }
        if let Some(why) = known_bad(tag, name) {
            return Err(format!("{name} of {tag} {why}"));
        }
        // When the file was made (a republish of an old extraction keeps its date), else
        // when its release was published.
        let made = doc.get("_extracted").and_then(Value::as_str).or(published);
        if let Some(cut) = self.not_before
            && made.is_none_or(|p| p < cut)
        {
            return Err(format!(
                "{name} of {tag} was extracted before this app's extraction fix of {cut}"
            ));
        }
        let schema = doc.get("_schema").and_then(Value::as_str).or(schema);
        match schema {
            Some(s) => {
                let n = s
                    .strip_prefix(name)
                    .and_then(|s| s.strip_prefix('@'))
                    .and_then(|n| n.parse::<u32>().ok());
                if !n.is_some_and(|n| self.schemas.contains(&n)) {
                    return Err(format!("{name}: schema {s} is not one this app reads"));
                }
            }
            None if ACCEPT_UNSCHEMAED && self.verified.contains(&tag) => {}
            None => {
                return Err(format!(
                    "{name} of {tag} has no _schema and its shape was not checked for that build"
                ));
            }
        }
        if let Some(fields) = redacted(doc) {
            if doc.get("_redacted").is_some_and(|r| !r.is_array()) {
                return Err(format!("{name}: published redacted"));
            }
            if let Some(x) = fields
                .iter()
                .find(|x| !self.may_strip.contains(&x.as_str()))
            {
                return Err(format!(
                    "{name}: published without {x}, which this app uses"
                ));
            }
        }
        if let Some(p) = self.needs.iter().find(|p| !has_path(doc, p)) {
            return Err(format!(
                "{name}: published without {p}, which this app needs"
            ));
        }
        if let Some(t) = self.typed {
            t(doc).map_err(|e| format!("{name} {e}"))?;
        }
        Ok(())
    }
}

/// How [`fill_prose`] turns a string-table key into `field`'s text, by the rule the dataset's
/// domain writes that field with, so a filled value is the one a local extraction writes:
///
/// - commodity descriptions, and refining's `rawDescription` / `refinedDescription` (the
///   resources' descriptions): `commodities::description_of` (markup stripped, the table's
///   literal `\n` as line breaks, placeholders and "X description." stubs dropped);
/// - refining's process `description`: the table's text, trimmed (an empty one stays empty);
/// - manufacturer descriptions: the table's text as it is;
/// - anything else: the text when it is not a placeholder or an unresolved key.
pub fn fill_text(loc: &Localization, dataset: &str, field: &str, key: &str) -> Option<String> {
    let at = if key.starts_with('@') {
        key.to_string()
    } else {
        format!("@{key}")
    };
    match (dataset, field) {
        ("game-commodities", _) | ("game-refining", "rawDescription" | "refinedDescription") => {
            starorg_gamedata::domains::commodities::description_of(loc, Some(&Value::String(at)))
        }
        ("game-refining", "description") => lookup_string(loc, key).map(|t| js_trim(t).to_string()),
        // The domain keeps whatever the table says (CIG's "placeholder info" included).
        ("game-manufacturers", _) => localize_str(loc, &at).map(str::to_string),
        _ => {
            let t = localize_str(loc, &at)?;
            (!is_placeholder_name(Some(t))).then(|| t.to_string())
        }
    }
}

/// Where a filled field goes in its object, so the file reads as the domains write it (and
/// as sc-gamedata does before it strips the field): refining's right before its key (each
/// key follows its field there); elsewhere right after the row's display name
/// (`displayName`, `title`, a manufacturer's `name`) or, without one, its `id` (the keys
/// follow later), else before its key.
fn fill_at(m: &serde_json::Map<String, Value>, dataset: &str, key_name: &str) -> usize {
    let key_at = m.keys().position(|k| k == key_name).unwrap_or(m.len());
    if dataset == "game-refining" {
        return key_at;
    }
    let name = if dataset == "game-manufacturers" {
        "name"
    } else {
        "displayName"
    };
    [name, "title", "id"]
        .iter()
        .find_map(|n| m.keys().position(|k| k == n))
        .map_or(key_at, |i| i + 1)
}

/// Fill the prose a redacted dump lacks (its `_redacted` fields, e.g. `description`) from the
/// local string table: every object with `<field>Key` and no `<field>` gets the text that
/// key names, by the rule the dataset's domain applies ([`fill_text`]), where the domain
/// writes it ([`fill_at`]). Marks the dataset
/// `_filled` (which fields, from where), so nothing mistakes it for a published file. Returns
/// how many values were filled.
pub fn fill_prose(doc: &mut Value, dataset: &str, loc: &Localization) -> usize {
    let Some(fields) = redacted(doc) else {
        return 0;
    };
    fn walk(v: &mut Value, fields: &[String], dataset: &str, loc: &Localization, n: &mut usize) {
        match v {
            Value::Array(a) => a.iter_mut().for_each(|x| walk(x, fields, dataset, loc, n)),
            Value::Object(m) => {
                for f in fields {
                    if m.contains_key(f.as_str()) {
                        continue;
                    }
                    let key_name = format!("{f}Key");
                    let key = m.get(&key_name).and_then(Value::as_str);
                    if let Some(t) = key.and_then(|k| fill_text(loc, dataset, f, k)) {
                        let at = fill_at(m, dataset, &key_name);
                        m.shift_insert(at, f.clone(), Value::String(t));
                        *n += 1;
                    }
                }
                for (_, x) in m.iter_mut() {
                    walk(x, fields, dataset, loc, n);
                }
            }
            _ => {}
        }
    }
    let mut n = 0;
    walk(doc, &fields, dataset, loc, &mut n);
    if let Some(m) = doc.as_object_mut() {
        m.insert(
            "_filled".into(),
            serde_json::json!({
                "fields": fields,
                "from": "this computer's Data.p4k string table (global.ini)",
                "values": n,
            }),
        );
    }
    n
}

/// `build.json` at the dumps root (unknown keys ignored; everything optional but `build`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct BuildJson {
    pub build: String,
    #[serde(default)]
    pub channel: Option<String>,
    /// 1 on a build's first publish, one more per republish (absent before the field).
    #[serde(default)]
    pub revision: Option<u64>,
    #[serde(default)]
    pub extractor: Option<Extractor>,
    #[serde(default)]
    pub files: Option<Vec<BuildFile>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Extractor {
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default)]
    pub dirty: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BuildFile {
    pub name: String,
    pub sha256: String,
    pub bytes: u64,
    #[serde(default)]
    pub schema: Option<String>,
}

/// How a dataset of the tag is checked.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Expect {
    Sha256 {
        sha256: String,
        bytes: u64,
        schema: Option<String>,
    },
    Blob {
        sha1: String,
        size: u64,
    },
}

#[derive(Debug)]
struct Resolved {
    commit: String,
    /// `r<n>-<commit12>` when `build.json` names revision `n` and its tag is this commit,
    /// else the short commit id.
    revision: String,
    /// The tag's ref it was resolved under.
    tag_ref: String,
    /// The dump's channel (`build.json`, else the release), when it says.
    channel: Option<String>,
    /// The release date (RFC 3339).
    published: Option<String>,
    /// `data/<dataset>.json` -> its check.
    files: BTreeMap<String, Expect>,
    /// Why every dataset of this tag is refused (a dirty extractor), when one is.
    refused: Option<String>,
}

const RETRY_MISSING: Duration = Duration::from_secs(60 * 60);
const RETRY_FAILED: Duration = Duration::from_secs(10 * 60);
const MAX_BUILD_JSON: u64 = 1024 * 1024;

/// What a tag resolved to, and when.
type TagAnswer = (Instant, Result<Arc<Resolved>, String>);

/// The background check of one build's tag.
#[derive(Default)]
struct HeadState {
    /// The tag's ref (or why not), and when.
    tag_ref: Option<(Instant, Result<String, String>)>,
    /// A check or resolve of it running now.
    busy: bool,
}

/// Background work for one build.
enum Job {
    /// Ask the tag's ref.
    Ref,
    /// Resolve the tag (steps 1-3) for this ref.
    Resolve(String),
}

struct Inner {
    transport: Box<dyn DumpTransport + Send + Sync>,
    /// For background checks (short timeouts); `transport` when `None`.
    check: Option<Box<dyn DumpTransport + Send + Sync>>,
    enabled: Arc<dyn Fn() -> bool + Send + Sync>,
    tags: Mutex<HashMap<String, TagAnswer>>,
    heads: Mutex<HashMap<String, HeadState>>,
    /// Time spent downloading per tag since it was resolved.
    spent: Mutex<HashMap<String, Duration>>,
    budget: Duration,
    generation: AtomicU64,
    running: Mutex<Vec<std::thread::JoinHandle<()>>>,
}

/// Transport plus the user's switch, read at every fetch. One per process, shared by every
/// channel's store, so a build's tag is asked once.
pub struct DumpPrebuilt {
    inner: Arc<Inner>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn sha256_hex(b: &[u8]) -> String {
    crate::stamps::sha256(b)
}

fn net(e: starorg_core::source::SourceError) -> String {
    format!("GitHub: {}", dump::error_text(&e))
}

/// The first 12 characters of a commit id.
fn short(commit: &str) -> &str {
    commit.get(..12).unwrap_or(commit)
}

impl DumpPrebuilt {
    pub fn new(
        transport: Box<dyn DumpTransport + Send + Sync>,
        enabled: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                transport,
                check: None,
                enabled,
                tags: Mutex::default(),
                heads: Mutex::default(),
                spent: Mutex::default(),
                budget: DOWNLOAD_BUDGET,
                generation: AtomicU64::new(0),
                running: Mutex::default(),
            }),
        }
    }

    fn inner_mut(&mut self) -> &mut Inner {
        Arc::get_mut(&mut self.inner).expect("configured before use")
    }

    /// Another download budget per tag and run (tests).
    pub fn with_budget(mut self, budget: Duration) -> Self {
        self.inner_mut().budget = budget;
        self
    }

    /// The transport of the background checks (`starorg_net::GitHubTransport::quick`: 5 s).
    pub fn with_check_transport(mut self, check: Box<dyn DumpTransport + Send + Sync>) -> Self {
        self.inner_mut().check = Some(check);
        self
    }

    /// Wait until no background check runs (tests, and a clean exit).
    pub fn settle(&self) {
        loop {
            let running = std::mem::take(&mut *lock(&self.inner.running));
            if running.is_empty() {
                return;
            }
            for h in running {
                let _ = h.join();
            }
        }
    }

    fn spawn(&self, build: &str, job: Job) {
        let inner = self.inner.clone();
        let b = build.to_string();
        let started = std::thread::Builder::new()
            .name("dump-check".into())
            .spawn(move || {
                let via: &dyn DumpTransport = match &inner.check {
                    Some(c) => &**c,
                    None => &*inner.transport,
                };
                match job {
                    Job::Ref => {
                        let r = via.tag_ref(&b).map_err(net).and_then(|r| {
                            r.ok_or_else(|| format!("no prebuilt data published for {b}"))
                        });
                        lock(&inner.heads).entry(b.clone()).or_default().tag_ref =
                            Some((Instant::now(), r));
                    }
                    Job::Resolve(want) => {
                        let _ = inner.resolve(&b, via, Some(&want));
                    }
                }
                lock(&inner.heads).entry(b).or_default().busy = false;
                inner.generation.fetch_add(1, Ordering::SeqCst);
            });
        let mut running = lock(&self.inner.running);
        running.retain(|h| !h.is_finished());
        match started {
            Ok(h) => running.push(h),
            Err(_) => {
                lock(&self.inner.heads)
                    .entry(build.into())
                    .or_default()
                    .busy = false
            }
        }
    }
}

impl Inner {
    /// Nothing more from `tag` for a while: later datasets go straight to the local
    /// extraction, and a cached copy is served as it is.
    fn fail(&self, tag: &str, why: String) -> String {
        let now = Instant::now();
        lock(&self.tags).insert(tag.to_string(), (now, Err(why.clone())));
        lock(&self.heads)
            .entry(tag.to_string())
            .or_default()
            .tag_ref = Some((now, Err(why.clone())));
        self.generation.fetch_add(1, Ordering::SeqCst);
        why
    }

    /// The remembered answer for `tag`, while it holds: a resolve is for the run unless the
    /// tag's ref is known to have moved since (`want`).
    fn cached(&self, tag: &str, want: Option<&str>) -> Option<Result<Arc<Resolved>, String>> {
        let g = lock(&self.tags);
        let (at, r) = g.get(tag)?;
        let keep = match r {
            Ok(x) => want.is_none_or(|w| w == x.tag_ref),
            Err(e) if e.starts_with("no prebuilt") => at.elapsed() < RETRY_MISSING,
            Err(_) => at.elapsed() < RETRY_FAILED,
        };
        keep.then(|| r.clone())
    }

    /// The tag's ref as last checked, when that worked.
    fn known_ref(&self, tag: &str) -> Option<String> {
        match &lock(&self.heads).get(tag)?.tag_ref {
            Some((_, Ok(r))) => Some(r.clone()),
            _ => None,
        }
    }

    /// Resolve `tag` over `via` (no lock is held while it asks).
    fn resolve(
        &self,
        tag: &str,
        via: &dyn DumpTransport,
        want: Option<&str>,
    ) -> Result<Arc<Resolved>, String> {
        if let Some(r) = self.cached(tag, want) {
            return r;
        }
        lock(&self.spent).remove(tag);
        let r = resolve_now(via, tag).map(Arc::new);
        let now = Instant::now();
        lock(&self.tags).insert(tag.to_string(), (now, r.clone()));
        if let Ok(x) = &r {
            lock(&self.heads)
                .entry(tag.to_string())
                .or_default()
                .tag_ref = Some((now, Ok(x.tag_ref.clone())));
        }
        r
    }
}

fn resolve_now(t: &dyn DumpTransport, tag: &str) -> Result<Resolved, String> {
    let release = t
        .release(tag)
        .map_err(net)?
        .ok_or_else(|| format!("no prebuilt data published for {tag}"))?;
    if release.tag != tag {
        return Err(format!("asked for {tag}, the release is {}", release.tag));
    }
    let tag_ref = t
        .tag_ref(tag)
        .map_err(net)?
        .ok_or_else(|| format!("no prebuilt data published for {tag}"))?;
    let commit = t.tag_commit(tag).map_err(net)?.to_ascii_lowercase();
    if commit.len() != 40 || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("bad commit id {commit:?}"));
    }
    let mut revision = None;
    let tree = t.tree(&commit).map_err(net)?;
    let mut channel = dump::release_channel(&release);
    let mut refused = None;
    let mut files = BTreeMap::new();
    for e in &tree {
        files.insert(
            e.path.clone(),
            Expect::Blob {
                sha1: e.sha.to_ascii_lowercase(),
                size: e.size,
            },
        );
    }
    // A `build.json` the tree lists must check and read, or the tag is refused.
    if let Some(entry) = tree.iter().find(|e| e.path == "build.json") {
        let bad = |why: String| format!("build.json of {tag} {why}; nothing of it is used");
        if entry.size > MAX_BUILD_JSON {
            return Err(bad("is too large".into()));
        }
        let b = t.file(&commit, "build.json", MAX_BUILD_JSON).map_err(net)?;
        if b.len() as u64 != entry.size || git_blob_sha1(&b) != entry.sha.to_ascii_lowercase() {
            return Err(bad("does not match its blob id".into()));
        }
        let bj: BuildJson =
            serde_json::from_slice(&b).map_err(|e| bad(format!("does not read: {e}")))?;
        if bj.build != tag {
            return Err(format!("build.json of {tag} says {}", bj.build));
        }
        revision = bj.revision;
        if let Some(c) = bj.channel.as_deref().filter(|c| !c.is_empty()) {
            channel = Some(c.to_ascii_uppercase());
        }
        if bj.extractor.as_ref().is_some_and(|e| e.dirty) {
            refused = Some(format!(
                "{tag} was made by an unpublished extractor version"
            ));
        }
        for f in bj.files.iter().flatten() {
            let name = if f.name.starts_with("data/") {
                f.name.clone()
            } else {
                format!("data/{}", f.name)
            };
            files.insert(
                name,
                Expect::Sha256 {
                    sha256: f.sha256.to_ascii_lowercase(),
                    bytes: f.bytes,
                    schema: f.schema.clone(),
                },
            );
        }
    }
    // A revision counts only with its own tag (pushed with it, never moved) at this commit;
    // else the commit alone names what is read.
    let checked = match revision {
        Some(n) => t
            .find_tag_commit(&dump::revision_tag(tag, n))
            .map_err(net)?
            .is_some_and(|c| c.eq_ignore_ascii_case(&commit))
            .then_some(n),
        None => None,
    };
    let revision = match checked {
        Some(n) => format!("r{n}-{}", short(&commit)),
        None => short(&commit).to_string(),
    };
    Ok(Resolved {
        commit,
        revision,
        tag_ref,
        channel,
        published: release.published,
        files,
        refused,
    })
}

impl Prebuilt for DumpPrebuilt {
    fn enabled(&self) -> bool {
        (self.inner.enabled)()
    }

    fn fetch(&self, channel: &str, build: &str, dataset: &str) -> Result<Fetched, String> {
        if !self.enabled() {
            return Err("prebuilt game data is turned off".into());
        }
        let rule =
            dump_use(dataset).ok_or_else(|| format!("{dataset} is never taken from a dump"))?;
        if !is_build_tag(build) {
            return Err(format!("build {build:?} cannot be a dump tag"));
        }
        let i = &*self.inner;
        let want = i.known_ref(build);
        let r = i.resolve(build, &*i.transport, want.as_deref())?;
        if let Some(why) = &r.refused {
            return Err(why.clone());
        }
        if let Some(c) = &r.channel
            && !c.eq_ignore_ascii_case(channel)
        {
            return Err(format!("the dump of {build} is for {c}, not {channel}"));
        }
        let path = format!("data/{dataset}.json");
        let expect = r
            .files
            .get(&path)
            .ok_or_else(|| format!("{dataset} is not in the dump of {build}"))?;
        let (size, schema) = match expect {
            Expect::Sha256 { bytes, schema, .. } => (*bytes, schema.as_deref()),
            Expect::Blob { size, .. } => (*size, None),
        };
        if size > MAX_BYTES {
            return Err(format!("{dataset} is {size} bytes, more than allowed"));
        }
        let spent = lock(&i.spent).get(build).copied().unwrap_or_default();
        if spent >= i.budget {
            return Err(i.fail(
                build,
                format!(
                    "downloads of {build} took over {} s; reading locally",
                    i.budget.as_secs()
                ),
            ));
        }
        let started = Instant::now();
        let got = i.transport.file(&r.commit, &path, MAX_BYTES);
        *lock(&i.spent).entry(build.to_string()).or_default() += started.elapsed();
        // A failed download (offline, slow, refused): nothing more from this tag for a while.
        let bytes = got.map_err(|e| i.fail(build, net(e)))?;
        let ok = match expect {
            Expect::Sha256 {
                sha256, bytes: n, ..
            } => bytes.len() as u64 == *n && sha256_hex(&bytes) == *sha256,
            Expect::Blob { sha1, size } => {
                bytes.len() as u64 == *size && git_blob_sha1(&bytes) == *sha1
            }
        };
        if !ok {
            return Err(format!("{dataset} does not match its checksum in {build}"));
        }
        let doc: Value =
            serde_json::from_slice(&bytes).map_err(|e| format!("{dataset}: not a dataset: {e}"))?;
        rule.accepts(&doc, build, r.published.as_deref(), schema)?;
        drop(doc);
        Ok(Fetched {
            tag: build.to_string(),
            revision: r.revision.clone(),
            tag_ref: r.tag_ref.clone(),
            bytes,
        })
    }

    fn head(&self, build: &str, known_ref: Option<&str>) -> Option<DumpHead> {
        if !self.enabled() || !is_build_tag(build) {
            return None;
        }
        let i = &*self.inner;
        let mut job = None;
        let out = {
            let mut g = lock(&i.heads);
            let st = g.entry(build.to_string()).or_default();
            let answer = match &st.tag_ref {
                Some((_, Ok(r))) => Some(Ok(r.clone())),
                Some((at, Err(_))) if at.elapsed() < RETRY_FAILED => Some(Err(())),
                _ => None,
            };
            match answer {
                None => {
                    if !st.busy {
                        st.busy = true;
                        job = Some(Job::Ref);
                    }
                    None
                }
                Some(Err(())) => None,
                Some(Ok(r)) => {
                    let revision = match i.cached(build, Some(&r)) {
                        Some(Ok(x)) => Some(x.revision.clone()),
                        Some(Err(_)) => None,
                        None => {
                            if known_ref != Some(r.as_str()) && !st.busy {
                                st.busy = true;
                                job = Some(Job::Resolve(r.clone()));
                            }
                            None
                        }
                    };
                    Some(DumpHead {
                        tag_ref: r,
                        revision,
                    })
                }
            }
        };
        if let Some(job) = job {
            self.spawn(build, job);
        }
        out
    }

    fn generation(&self) -> u64 {
        self.inner.generation.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TAG: &str = "4.10.201.48085";

    #[test]
    fn the_table_covers_extractable_datasets_only_once() {
        let all = crate::worker::extractable();
        let mut seen = std::collections::BTreeSet::new();
        for u in DUMP_USE {
            assert!(all.contains(&u.dataset), "{} is not a dataset", u.dataset);
            assert!(seen.insert(u.dataset), "{} twice", u.dataset);
            assert!(!u.schemas.is_empty() && !u.needs.is_empty());
        }
        assert!(dump_use(starorg_gamedata::strings::NAME).is_none());
        assert!(dump_use(starorg_gamedata::search_index::NAME).is_none());
        assert_ne!(rule_key("game-loot"), rule_key("game-mining"));
        assert_eq!(rule_key("game-loot"), rule_key("game-loot"));
    }

    fn tags(extra: Value) -> Value {
        let mut v = json!({"_source": "x", "_build": TAG, "_schema": "game-item-tags@1", "items": [], "summary": {"items": 0, "withAmmo": 0, "tags": 0}});
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        v
    }

    #[test]
    fn accepts_checks_build_schema_redaction_shape_and_type() {
        let u = dump_use("game-item-tags").unwrap();
        let ok = |v: Value| u.accepts(&v, TAG, None, None);
        assert!(ok(tags(json!({}))).is_ok());
        // Fields added later (sc-gamedata adds optional ones without a schema bump): ignored.
        assert!(
            ok(tags(
                json!({"byClass": {}, "summary": {"withAmmo": 0, "tags": 0, "new": 1}})
            ))
            .is_ok()
        );
        // game-tag-index (published since 4.10.201) is not read by this app: never taken.
        assert!(dump_use("game-tag-index").is_none());
        let e = |v: Value, tag: &str| u.accepts(&v, tag, None, None).unwrap_err();
        assert!(e(tags(json!({})), "4.10.201.1").contains("stamped"));
        assert!(e(tags(json!({"_build": null})), TAG).contains("no _build"));
        assert!(e(tags(json!({"_schema": "game-item-tags@2"})), TAG).contains("schema"));
        assert!(e(tags(json!({"_schema": "game-ships@1"})), TAG).contains("schema"));
        assert!(e(tags(json!({"_redacted": ["description"]})), TAG).contains("description"));
        assert!(e(tags(json!({"_redacted": "names only"})), TAG).contains("redacted"));
        assert!(e(tags(json!({"summary": {"items": 0}})), TAG).contains("summary.withAmmo"));
        // Null does not count as present.
        assert!(
            e(tags(json!({"summary": {"withAmmo": null, "tags": 1}})), TAG).contains("withAmmo")
        );
        // Read with the app's own type: a row of the wrong shape is refused.
        assert!(e(tags(json!({"items": [{"className": 5}]})), TAG).contains("does not read"));
        assert!(e(json!([1]), TAG).contains("object"));
        // No `_schema`: only for a tag whose shape was checked, else the build.json schema.
        let mut bare = tags(json!({}));
        bare.as_object_mut().unwrap().remove("_schema");
        assert!(e(bare.clone(), TAG).contains("no _schema"));
        assert!(
            u.accepts(&bare, TAG, None, Some("game-item-tags@1"))
                .is_ok()
        );
        let loot = dump_use("game-loot").unwrap();
        let v200 = json!({"_build": "4.10.200.58746", "tables": [], "archetypes": [], "secondaryChoices": [], "filters": [], "specialEvents": [], "summary": {}});
        assert!(
            loot.accepts(&v200, "4.10.200.58746", None, None).is_ok(),
            "{:?}",
            loot.accepts(&v200, "4.10.200.58746", None, None)
        );
        // Known bad, and published before this app's fix.
        bare["_build"] = json!("4.10.200.58746");
        bare["_schema"] = json!("game-item-tags@1");
        assert!(
            u.accepts(&bare, "4.10.200.58746", None, None)
                .unwrap_err()
                .contains("older extraction")
        );
        let sm = dump_use("game-starmap").unwrap();
        let star = json!({"_build": TAG, "_schema": "game-starmap@1", "locations": [], "byType": {}, "summary": {}});
        assert!(
            sm.accepts(&star, TAG, Some("2026-11-12T18:20:43Z"), None)
                .unwrap_err()
                .contains("before")
        );
        assert!(
            sm.accepts(&star, TAG, Some("2026-12-01T00:00:00Z"), None)
                .is_ok()
        );
        // The file's own extraction date counts, not a later (re)publish.
        let mut old = star.clone();
        old["_extracted"] = json!("2026-11-12T18:00:00.000Z");
        assert!(
            sm.accepts(&old, TAG, Some("2026-12-01T00:00:00Z"), None)
                .unwrap_err()
                .contains("extracted before")
        );
        old["_extracted"] = json!("2026-11-26T01:22:42.552Z");
        assert!(sm.accepts(&old, TAG, None, None).is_ok());
        // Row rule versions change only their rows' keys.
        assert!(ROW_RULES.iter().all(|(d, _)| dump_use(d).is_some()));
        // Prose this app fills may be stripped; anything else not.
        let mut red = star.clone();
        red["_redacted"] = json!(["description"]);
        assert!(sm.accepts(&red, TAG, Some("2026-12-01"), None).is_ok());
        let missions = dump_use("game-missions").unwrap();
        let m = json!({"_build": TAG, "_schema": "game-missions@1", "missions": [], "byGiver": {}, "summary": {}, "_redacted": ["description", "title over 120 characters"]});
        assert!(
            missions
                .accepts(&m, TAG, None, None)
                .unwrap_err()
                .contains("title over")
        );
    }

    #[test]
    fn redacted_prose_is_filled_from_the_local_string_table() {
        let mut loc = Localization::new();
        loc.set("ship_Desc", "A fine ship.");
        loc.set("stub_Desc", "<= PLACEHOLDER =>");
        let mut doc = json!({"_redacted": ["description"], "vehicles": [
            {"name": "A", "descriptionKey": "ship_Desc"},
            {"name": "B", "descriptionKey": "stub_Desc"},
            {"name": "C", "descriptionKey": "gone_Desc"},
            {"name": "D", "description": "kept"},
        ]});
        assert_eq!(fill_prose(&mut doc, "game-ships", &loc), 1, "{doc}");
        assert_eq!(doc["vehicles"][0]["description"], "A fine ship.");
        // A placeholder or a key the table lacks: left out, as the domains do.
        assert!(doc["vehicles"][1].get("description").is_none());
        assert!(doc["vehicles"][2].get("description").is_none());
        assert_eq!(doc["vehicles"][3]["description"], "kept");
        assert_eq!(doc["_filled"]["fields"], json!(["description"]));
        // Not redacted: untouched.
        let mut plain = json!({"vehicles": []});
        assert_eq!(fill_prose(&mut plain, "game-ships", &loc), 0);
        assert!(plain.get("_filled").is_none());
    }

    #[test]
    fn build_json_takes_unknown_and_missing_fields() {
        let b: BuildJson =
            serde_json::from_str(r#"{"build":"1","channel":null,"patch":"4.10","later":{"x":1}}"#)
                .unwrap();
        assert_eq!(b.build, "1");
        assert!(b.files.is_none() && b.extractor.is_none());
        let b: BuildJson = serde_json::from_str(r#"{"build":"1","extractor":{"commit":"abc","dirty":true},"files":[{"name":"data/game-ships.json","sha256":"AA","bytes":2,"schema":"game-ships@1","x":0}]}"#).unwrap();
        assert!(b.extractor.unwrap().dirty);
        assert_eq!(b.files.unwrap()[0].schema.as_deref(), Some("game-ships@1"));
    }

    use starorg_core::source::SourceError;
    use starorg_core::source::dump::{ReleaseInfo, TreeEntry};
    use std::sync::atomic::{AtomicUsize, Ordering};

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    /// The dumps repo in memory: no network.
    #[derive(Default)]
    struct Repo {
        /// Tag -> release channel word in the title.
        releases: BTreeMap<String, String>,
        files: BTreeMap<String, Vec<u8>>,
        /// Paths served with other bytes than the tree lists.
        tamper: Vec<String>,
        /// Paths whose download fails (a network error).
        broken: Vec<String>,
        offline: bool,
        calls: AtomicUsize,
        downloads: AtomicUsize,
        /// Revision tags (`<build>-r<n>`) -> commit.
        revision_tags: BTreeMap<String, String>,
        /// The build tag's ref is `ref<this>` (pushed again: another value).
        pushes: Mutex<String>,
    }

    impl Repo {
        fn with(tag: &str, channel: &str) -> Self {
            let mut r = Repo::default();
            r.releases.insert(tag.into(), channel.into());
            r.files.insert(
                "data/game-item-tags.json".into(),
                tags(json!({})).to_string().into_bytes(),
            );
            r.files.insert(
                "data/game-loot.json".into(),
                json!({"_build": tag, "_schema": "game-loot@1", "tables": []})
                    .to_string()
                    .into_bytes(),
            );
            r
        }
        fn net(&self) -> Result<(), SourceError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.offline {
                Err(SourceError::Unavailable("offline".into()))
            } else {
                Ok(())
            }
        }
    }

    impl DumpTransport for Repo {
        fn releases(&self) -> Result<Vec<ReleaseInfo>, SourceError> {
            unreachable!("never listed")
        }
        fn release(&self, tag: &str) -> Result<Option<ReleaseInfo>, SourceError> {
            self.net()?;
            Ok(self.releases.get(tag).map(|c| ReleaseInfo {
                tag: tag.into(),
                title: format!("x {c} ({tag})"),
                body: String::new(),
                published: Some("2026-12-01T00:00:00Z".into()),
            }))
        }
        fn tag_commit(&self, _: &str) -> Result<String, SourceError> {
            self.net()?;
            Ok(COMMIT.into())
        }
        fn find_tag_commit(&self, tag: &str) -> Result<Option<String>, SourceError> {
            self.net()?;
            Ok(self.revision_tags.get(tag).cloned())
        }
        fn tag_ref(&self, tag: &str) -> Result<Option<String>, SourceError> {
            self.net()?;
            Ok(if self.releases.contains_key(tag) {
                Some(format!("ref{}", self.pushes.lock().unwrap()))
            } else {
                self.revision_tags.get(tag).cloned()
            })
        }
        fn tree(&self, _: &str) -> Result<Vec<TreeEntry>, SourceError> {
            self.net()?;
            Ok(self
                .files
                .iter()
                .map(|(p, b)| TreeEntry {
                    path: p.clone(),
                    sha: git_blob_sha1(b),
                    size: b.len() as u64,
                })
                .collect())
        }
        fn file(&self, _: &str, path: &str, _: u64) -> Result<Vec<u8>, SourceError> {
            self.net()?;
            self.downloads.fetch_add(1, Ordering::SeqCst);
            if self.broken.iter().any(|b| b == path) {
                return Err(SourceError::Unavailable("connection reset".into()));
            }
            let mut b = self
                .files
                .get(path)
                .cloned()
                .ok_or_else(|| SourceError::Unavailable(format!("{path} not found")))?;
            if self.tamper.iter().any(|t| t == path) {
                b.push(b' ');
            }
            Ok(b)
        }
    }

    /// Shares the repo with the test, for its counters.
    struct Shared(Arc<Repo>);

    impl DumpTransport for Shared {
        fn releases(&self) -> Result<Vec<ReleaseInfo>, SourceError> {
            self.0.releases()
        }
        fn release(&self, tag: &str) -> Result<Option<ReleaseInfo>, SourceError> {
            self.0.release(tag)
        }
        fn tag_commit(&self, tag: &str) -> Result<String, SourceError> {
            self.0.tag_commit(tag)
        }
        fn find_tag_commit(&self, tag: &str) -> Result<Option<String>, SourceError> {
            self.0.find_tag_commit(tag)
        }
        fn tag_ref(&self, tag: &str) -> Result<Option<String>, SourceError> {
            self.0.tag_ref(tag)
        }
        fn tree(&self, commit: &str) -> Result<Vec<TreeEntry>, SourceError> {
            self.0.tree(commit)
        }
        fn file(&self, commit: &str, path: &str, max: u64) -> Result<Vec<u8>, SourceError> {
            self.0.file(commit, path, max)
        }
    }

    fn dumps(repo: Repo, on: bool) -> DumpPrebuilt {
        dumps_shared(Arc::new(repo), on)
    }

    fn dumps_shared(repo: Arc<Repo>, on: bool) -> DumpPrebuilt {
        DumpPrebuilt::new(Box::new(Shared(repo)), Arc::new(move || on))
    }

    #[test]
    fn a_matching_dump_is_verified_checked_and_written() {
        let p = dumps(Repo::with(TAG, "LIVE"), true);
        let f = p.fetch("LIVE", TAG, "game-item-tags").unwrap();
        assert_eq!(f.tag, TAG);
        assert_eq!(f.tag_ref, "ref");
        let v: Value = serde_json::from_slice(&f.bytes).unwrap();
        assert_eq!(v["_build"], TAG);
        // Never the string table, and only listed datasets.
        assert!(
            p.fetch("LIVE", TAG, "game-strings-english")
                .unwrap_err()
                .contains("never")
        );
        assert!(
            p.fetch("LIVE", TAG, "game-ships")
                .unwrap_err()
                .contains("not in the dump")
        );
        // A dataset without what this app needs: refused, nothing written.
        let e = p.fetch("LIVE", TAG, "game-loot").unwrap_err();
        assert!(e.contains("archetypes"), "{e}");
        // Another channel's dump of that build is not used; nor is a switched-off fetch.
        assert!(
            p.fetch("PTU", TAG, "game-item-tags")
                .unwrap_err()
                .contains("is for LIVE")
        );
        let off = dumps(Repo::with(TAG, "LIVE"), false);
        assert!(!off.enabled());
        assert!(
            off.fetch("LIVE", TAG, "game-item-tags")
                .unwrap_err()
                .contains("off")
        );
    }

    #[test]
    fn no_tag_a_bad_checksum_or_no_network_writes_nothing() {
        // No release for the build: never another build.
        let r = Arc::new(Repo::with("4.10.200.58746", "LIVE"));
        let p = dumps_shared(r.clone(), true);
        let e = p.fetch("LIVE", TAG, "game-item-tags").unwrap_err();
        assert!(e.contains("no prebuilt data published"), "{e}");
        // Remembered: GitHub is asked once per tag, not per dataset.
        let _ = p.fetch("LIVE", TAG, "game-loot");
        assert_eq!(r.calls.load(Ordering::SeqCst), 1);
        assert_eq!(r.downloads.load(Ordering::SeqCst), 0);
        // Checksum mismatch.
        let mut r = Repo::with(TAG, "LIVE");
        r.tamper.push("data/game-item-tags.json".into());
        let p = dumps(r, true);
        assert!(
            p.fetch("LIVE", TAG, "game-item-tags")
                .unwrap_err()
                .contains("checksum")
        );
        // Offline: a network error, remembered for the run.
        let mut r = Repo::with(TAG, "LIVE");
        r.offline = true;
        let r = Arc::new(r);
        let p = dumps_shared(r.clone(), true);
        assert!(
            p.fetch("LIVE", TAG, "game-item-tags")
                .unwrap_err()
                .contains("offline")
        );
        assert!(
            p.fetch("LIVE", TAG, "game-loot")
                .unwrap_err()
                .contains("offline")
        );
        assert_eq!(
            r.calls.load(Ordering::SeqCst),
            1,
            "one try per run, not per dataset"
        );
    }

    #[test]
    fn a_failed_or_slow_download_stops_the_tag_for_the_run() {
        // The first failing file: nothing more from that tag.
        let mut r = Repo::with(TAG, "LIVE");
        r.broken.push("data/game-loot.json".into());
        let r = Arc::new(r);
        let p = dumps_shared(r.clone(), true);
        assert!(
            p.fetch("LIVE", TAG, "game-loot")
                .unwrap_err()
                .contains("reset")
        );
        let before = r.downloads.load(Ordering::SeqCst);
        assert!(
            p.fetch("LIVE", TAG, "game-item-tags")
                .unwrap_err()
                .contains("reset")
        );
        assert_eq!(r.downloads.load(Ordering::SeqCst), before, "not tried");
        // Budget used up: the rest is read locally.
        let p = dumps(Repo::with(TAG, "LIVE"), true);
        let p = p.with_budget(Duration::ZERO);
        assert!(
            p.fetch("LIVE", TAG, "game-item-tags")
                .unwrap_err()
                .contains("took over")
        );
    }

    #[test]
    fn build_json_gives_sums_channel_and_a_dirty_or_broken_one_refuses_the_tag() {
        let mut r = Repo::with(TAG, "LIVE");
        let body = r.files["data/game-item-tags.json"].clone();
        // Shaped like sc-gamedata's publish-dump.mjs writes it (2-space JSON, null unknowns).
        let bj = |channel: &str, dirty: bool, sha: &str, name: &str| {
            format!(
                "{{\n  \"build\": \"{TAG}\",\n  \"channel\": \"{channel}\",\n  \"patch\": \"4.10.2\",\n  \"branch\": null,\n  \"extractor\": {{\n    \"commit\": \"350e7053b\",\n    \"dirty\": {dirty}\n  }},\n  \"files\": [\n    {{\n      \"name\": \"{name}\",\n      \"sha256\": \"{sha}\",\n      \"bytes\": {},\n      \"schema\": \"game-item-tags@1\"\n    }}\n  ]\n}}\n",
                body.len()
            )
            .into_bytes()
        };
        // build.json's channel wins over the release's; a bare file name is taken too.
        r.files.insert(
            "build.json".into(),
            bj("PTU", false, &sha256_hex(&body), "game-item-tags.json"),
        );
        let p = dumps(r, true);
        assert!(
            p.fetch("LIVE", TAG, "game-item-tags")
                .unwrap_err()
                .contains("is for PTU")
        );
        assert_eq!(p.fetch("PTU", TAG, "game-item-tags").unwrap().tag, TAG);
        // Its sha256 is the check: a wrong one refuses the right file.
        let mut r = Repo::with(TAG, "LIVE");
        r.files.insert(
            "build.json".into(),
            bj("LIVE", false, &"0".repeat(64), "data/game-item-tags.json"),
        );
        let p = dumps(r, true);
        assert!(
            p.fetch("LIVE", TAG, "game-item-tags")
                .unwrap_err()
                .contains("checksum")
        );
        // A dirty extractor checkout: nothing of that tag is used.
        let mut r = Repo::with(TAG, "LIVE");
        r.files.insert(
            "build.json".into(),
            bj("LIVE", true, &sha256_hex(&body), "data/game-item-tags.json"),
        );
        let p = dumps(r, true);
        assert!(
            p.fetch("LIVE", TAG, "game-item-tags")
                .unwrap_err()
                .contains("unpublished extractor")
        );
        // A build.json of another build refuses the tag.
        let mut r = Repo::with(TAG, "LIVE");
        r.files
            .insert("build.json".into(), br#"{"build":"4.10.201.1"}"#.to_vec());
        let p = dumps(r, true);
        assert!(
            p.fetch("LIVE", TAG, "game-item-tags")
                .unwrap_err()
                .contains("says 4.10.201.1")
        );
        // A listed build.json that does not read, or does not match its blob: refused, no
        // fallback to the tree.
        let mut r = Repo::with(TAG, "LIVE");
        r.files.insert("build.json".into(), b"{not json".to_vec());
        let p = dumps(r, true);
        assert!(
            p.fetch("LIVE", TAG, "game-item-tags")
                .unwrap_err()
                .contains("does not read")
        );
        let mut r = Repo::with(TAG, "LIVE");
        r.files.insert(
            "build.json".into(),
            bj(
                "LIVE",
                false,
                &sha256_hex(&body),
                "data/game-item-tags.json",
            ),
        );
        r.tamper.push("build.json".into());
        let p = dumps(r, true);
        assert!(
            p.fetch("LIVE", TAG, "game-item-tags")
                .unwrap_err()
                .contains("blob id")
        );
    }

    /// `repo` with a `build.json` of `revision` that lists the item-tags file.
    fn with_build_json(mut r: Repo, revision: Option<u64>) -> Repo {
        let body = r.files["data/game-item-tags.json"].clone();
        let mut bj = json!({"build": TAG, "channel": "LIVE", "extractor": {"commit": "abc"},
            "files": [{"name": "data/game-item-tags.json", "sha256": sha256_hex(&body),
                       "bytes": body.len(), "schema": "game-item-tags@1"}]});
        if let Some(n) = revision {
            bj["revision"] = json!(n);
        }
        r.files
            .insert("build.json".into(), bj.to_string().into_bytes());
        r
    }

    const SHORT: &str = "0123456789ab";

    #[test]
    fn the_revision_needs_its_own_tag_else_the_commit_names_it() {
        // r3 with `B-r3` at the build tag's commit: `r3-<commit12>`.
        let mut r = with_build_json(Repo::with(TAG, "LIVE"), Some(3));
        r.revision_tags
            .insert(dump::revision_tag(TAG, 3), COMMIT.into());
        let f = dumps(r, true).fetch("LIVE", TAG, "game-item-tags").unwrap();
        assert_eq!(f.revision, format!("r3-{SHORT}"));
        // r3 without its tag, or with it elsewhere: not a checked revision, the commit alone.
        let r = with_build_json(Repo::with(TAG, "LIVE"), Some(3));
        let f = dumps(r, true).fetch("LIVE", TAG, "game-item-tags").unwrap();
        assert_eq!(f.revision, SHORT);
        let mut r = with_build_json(Repo::with(TAG, "LIVE"), Some(3));
        r.revision_tags
            .insert(dump::revision_tag(TAG, 3), "f".repeat(40));
        let f = dumps(r, true).fetch("LIVE", TAG, "game-item-tags").unwrap();
        assert_eq!(f.revision, SHORT);
        // No revision (published before the field), or no build.json: the commit.
        let p = dumps(with_build_json(Repo::with(TAG, "LIVE"), None), true);
        assert_eq!(
            p.fetch("LIVE", TAG, "game-item-tags").unwrap().revision,
            SHORT
        );
        let p = dumps(Repo::with(TAG, "LIVE"), true);
        assert_eq!(
            p.fetch("LIVE", TAG, "game-item-tags").unwrap().revision,
            SHORT
        );
    }

    #[test]
    fn the_head_is_checked_in_the_background_with_one_request() {
        let mut r = with_build_json(Repo::with(TAG, "LIVE"), Some(3));
        r.revision_tags
            .insert(dump::revision_tag(TAG, 3), COMMIT.into());
        let r = Arc::new(r);
        let p = dumps_shared(r.clone(), true);
        // Not known yet: nothing waits; the check runs in the background.
        assert_eq!(p.head(TAG, Some("ref")), None);
        let g0 = p.generation();
        p.settle();
        assert!(p.generation() > g0, "the answer is announced");
        assert_eq!(r.calls.load(Ordering::SeqCst), 1, "one request: the ref");
        // The stamp's ref: current, and nothing else is asked.
        assert_eq!(
            p.head(TAG, Some("ref")),
            Some(DumpHead {
                tag_ref: "ref".into(),
                revision: None
            })
        );
        p.settle();
        assert_eq!(r.calls.load(Ordering::SeqCst), 1);
        // Pushed again: another ref, so the revision is resolved in the background.
        *r.pushes.lock().unwrap() = "2".into();
        let p = dumps_shared(r.clone(), true);
        assert_eq!(p.head(TAG, Some("ref")), None);
        p.settle();
        assert_eq!(
            p.head(TAG, Some("ref")),
            Some(DumpHead {
                tag_ref: "ref2".into(),
                revision: None
            })
        );
        p.settle();
        let head = p.head(TAG, Some("ref")).unwrap();
        assert_eq!(head.revision, Some(format!("r3-{SHORT}")));
        // The download that follows reuses that resolve: only the file is fetched.
        let before = r.calls.load(Ordering::SeqCst);
        let f = p.fetch("LIVE", TAG, "game-item-tags").unwrap();
        assert_eq!(
            (f.tag_ref.as_str(), f.revision),
            ("ref2", head.revision.unwrap())
        );
        assert_eq!(r.calls.load(Ordering::SeqCst), before + 1);
        // A failed download: the head is not known for a while (the cached copy is served).
        let mut broken = with_build_json(Repo::with(TAG, "LIVE"), None);
        broken.broken.push("data/game-item-tags.json".into());
        let p = dumps(broken, true);
        p.head(TAG, None);
        p.settle();
        assert!(p.head(TAG, None).is_some());
        p.settle();
        assert!(p.fetch("LIVE", TAG, "game-item-tags").is_err());
        assert_eq!(p.head(TAG, None), None);
        // Switched off, offline, no dump, or a revision tag: not known.
        let off = dumps(Repo::with(TAG, "LIVE"), false);
        assert_eq!(off.head(TAG, None), None);
        off.settle();
        assert_eq!(off.head(TAG, None), None);
        let mut r = Repo::with(TAG, "LIVE");
        r.offline = true;
        let p = dumps(r, true);
        p.head(TAG, None);
        p.settle();
        assert_eq!(p.head(TAG, None), None);
        let p = dumps(Repo::with("4.10.200.58746", "LIVE"), true);
        p.head(TAG, None);
        p.settle();
        assert_eq!(p.head(TAG, None), None);
    }

    #[test]
    fn a_revision_tag_is_never_looked_up_as_a_build() {
        let rt = dump::revision_tag(TAG, 3);
        let mut r = Repo::with(&rt, "LIVE");
        r.files.insert(
            "data/game-item-tags.json".into(),
            tags(json!({"_build": rt})).to_string().into_bytes(),
        );
        let r = Arc::new(r);
        let p = dumps_shared(r.clone(), true);
        assert!(
            p.fetch("LIVE", &rt, "game-item-tags")
                .unwrap_err()
                .contains("cannot be a dump tag")
        );
        assert_eq!(p.head(&rt, None), None);
        p.settle();
        assert_eq!(r.calls.load(Ordering::SeqCst), 0);
    }

    /// game-refining as this app's domain writes it, and as sc-gamedata publishes it:
    /// prose stripped, the keys its domain adds beside it (`nameKey`, `descriptionKey`,
    /// `rawDescriptionKey`, `refinedDescriptionKey`). Filled again from the same string
    /// table, it is this app's file byte for byte, once those keys are taken out.
    #[test]
    fn refining_prose_is_filled_as_its_domain_writes_it() {
        use starorg_gamedata::domain::{Domain, NoFiles, ParseContext};
        use starorg_gamedata::records::{RecordSet, normalize_record};
        let rec = |ty: &str, short: &str, mut v: Value| {
            v["_Type_"] = json!(ty);
            normalize_record(
                json!({"_RecordName_": format!("{ty}.{short}"), "_RecordId_": format!("id-{short}"),
                       "_RecordValue_": v}),
                &format!("libs/foundry/records/x/{}", short.to_lowercase()),
            )
            .unwrap()
        };
        let process = |name: &str, speed: &str, quality: &str| {
            rec(
                "RefiningProcess",
                &format!("ProcessingType-{name}"),
                json!({"refiningSpeed": speed, "refiningQuality": quality,
                       "processName": format!("@refinery_ui_ProcessingType_{name}")}),
            )
        };
        let resource = |name: &str, extra: Value| {
            let mut v = json!({
                "_RecordName_": format!("ResourceType.{name}"),
                "_RecordId_": format!("id-{name}"),
                "_Type_": "ResourceType",
                "displayName": format!("@items_commodities_{}", name.to_lowercase()),
                "description": format!("@items_commodities_{}_desc", name.to_lowercase()),
                "densityType": {"densityUnit": {"_Type_": "GramsPerCubicCentimeter", "gramsPerCubicCentimeter": 2.5}},
            });
            if let (Value::Object(m), Value::Object(e)) = (&mut v, extra) {
                m.extend(e);
            }
            v
        };
        let set = RecordSet::new(vec![
            process("SlowWasteful", "Slow", "Wasteful"),
            process("SlowNormal", "Slow", "Normal"),
            process("FastCareful", "Fast", "Careful"),
            process("NormalNormal", "Normal", "Normal"),
            rec(
                "ResourceTypeDatabase",
                "ResourceTypeDatabase",
                json!({"groups": [{"_RecordName_": "ResourceTypeGroup.Metal", "groups": [], "resources": [
                    resource("Gold", json!({})),
                    resource("Ore_Gold", json!({"refinedVersion": resource("Gold", json!({}))})),
                    resource("Tin", json!({})),
                    resource("Ore_Tin", json!({"refinedVersion": resource("Tin", json!({}))})),
                ]}]}),
            ),
        ]);
        let loc = starorg_gamedata::localization::parse_global_ini(concat!(
            "refinery_ui_ProcessingType_SlowWasteful=Kazen Winnowing\n",
            // Trimmed, markup and placeholders kept: the process rule.
            "refinery_ui_ProcessingType_SlowWasteful_Desc=  Slow and <EM4>cheap</EM4>.\\nStill used. \n",
            "refinery_ui_ProcessingType_SlowNormal_Desc=<= PLACEHOLDER =>\n",
            "refinery_ui_ProcessingType_FastCareful_Desc=\n",
            "refinery_ui_ProcessingType_FastCareful_Details,P=High Yield // High Cost // Fastest\n",
            "items_commodities_gold=Gold\n",
            "items_commodities_ore_gold=Gold (Ore)\n",
            // Markup stripped, `\\n` a line break: the commodities rule.
            "items_commodities_ore_gold_desc= A <EM4>dense</EM4> ore.\\nHeavy. \n",
            "items_commodities_gold_desc=Gold description.\n",
            "items_commodities_ore_tin_desc=<= PLACEHOLDER =>\n",
            "items_commodities_tin_desc=Soft metal.\n",
        ));
        let mut ours = serde_json::Map::new();
        ours.insert("_build".into(), json!(TAG));
        ours.extend(
            starorg_gamedata::domains::refining::DOMAIN
                .parse(&ParseContext::new(&set, &loc, &NoFiles))
                .unwrap(),
        );
        let ours = Value::Object(ours);
        // What the rules gave, so the fixture covers each case.
        let p = &ours["processes"];
        let by = |n: &str| {
            p.as_array()
                .unwrap()
                .iter()
                .find(|x| x["name"] == format!("ProcessingType-{n}"))
                .unwrap()
                .clone()
        };
        assert_eq!(
            by("SlowWasteful")["description"],
            "Slow and <EM4>cheap</EM4>.\\nStill used."
        );
        assert_eq!(by("SlowNormal")["description"], "<= PLACEHOLDER =>");
        assert_eq!(by("FastCareful")["description"], "");
        assert!(by("NormalNormal").get("description").is_none());
        let gold = &ours["refinables"][0];
        assert_eq!(gold["rawDescription"], "A dense ore.\nHeavy.");
        assert!(gold.get("refinedDescription").is_none(), "a stub");
        let tin = &ours["refinables"][1];
        assert!(tin.get("rawDescription").is_none(), "a placeholder");
        assert_eq!(tin["refinedDescription"], "Soft metal.");

        // Published: prose out, keys in (sc-gamedata's order: each key right after its field;
        // a process names its key only when it has a description).
        let mut published = ours.clone();
        let fields = ["description", "rawDescription", "refinedDescription"];
        for p in published["processes"].as_array_mut().unwrap() {
            let name = p["name"].as_str().unwrap().replace("ProcessingType-", "");
            let m = p.as_object_mut().unwrap();
            let mut out = serde_json::Map::new();
            for (k, v) in std::mem::take(m) {
                if k == "description" {
                    out.insert(
                        "descriptionKey".into(),
                        json!(format!("refinery_ui_ProcessingType_{name}_Desc")),
                    );
                } else {
                    out.insert(k, v);
                }
            }
            // A field this app does not read.
            out.insert(
                "nameKey".into(),
                json!(format!("refinery_ui_ProcessingType_{name}")),
            );
            *m = out;
        }
        for r in published["refinables"].as_array_mut().unwrap() {
            let m = r.as_object_mut().unwrap();
            let key = |name: &Value| {
                json!(format!(
                    "items_commodities_{}_desc",
                    name.as_str().unwrap().to_lowercase()
                ))
            };
            let (raw, refined) = (key(&m["raw"]), key(&m["refined"]));
            let mut out = serde_json::Map::new();
            for (k, v) in std::mem::take(m) {
                match k.as_str() {
                    "rawDescription" | "refinedDescription" => {}
                    "group" => {
                        out.insert("rawDescriptionKey".into(), raw.clone());
                        out.insert("refinedDescriptionKey".into(), refined.clone());
                        out.insert(k, v);
                    }
                    _ => {
                        out.insert(k, v);
                    }
                }
            }
            *m = out;
        }
        published["_redacted"] = json!(fields);
        assert!(published["processes"][0].get("description").is_none());

        let mut filled = published;
        assert_eq!(fill_prose(&mut filled, "game-refining", &loc), 5);
        let strip = |v: &mut Value| {
            fn walk(v: &mut Value) {
                match v {
                    Value::Array(a) => a.iter_mut().for_each(walk),
                    Value::Object(m) => {
                        for k in [
                            "nameKey",
                            "descriptionKey",
                            "rawDescriptionKey",
                            "refinedDescriptionKey",
                        ] {
                            m.shift_remove(k);
                        }
                        m.values_mut().for_each(walk);
                    }
                    _ => {}
                }
            }
            let m = v.as_object_mut().unwrap();
            m.shift_remove("_redacted");
            m.shift_remove("_filled");
            walk(v);
        };
        strip(&mut filled);
        assert_eq!(
            starorg_gamedata::output::to_js_json(&filled),
            starorg_gamedata::output::to_js_json(&ours)
        );
    }
}
