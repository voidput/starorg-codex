//! Sounds of a record: DataCore audio triggers -> ATL triggers -> Wwise bank events -> media,
//! then WEM -> Ogg Vorbis. Ported from StarBreaker's Tauri app (`app/src-tauri/src/
//! audio_commands.rs` at the pinned rev, MIT, Copyright (c) 2026 Diogo Trindade): the source
//! classification (embedded in a bank, loose media file, external source, unavailable), the
//! bank path index, the companion-bank-less event resolution and the decode.

use super::{AssetArchive, AssetError, AssetResult, find_record, record_short_name};
use serde::{Deserialize, Serialize};
use starbreaker_datacore::database::Database;
use starbreaker_datacore::loadout::{EntityIndex, LoadoutNode, resolve_loadout_indexed};
use starbreaker_p4k::MappedP4k;
use starbreaker_wwise::{AtlIndex, BnkFile, ExternalSourceIndex, Hierarchy, SoundSource};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// Banks kept parsed (each holds its event hierarchy); the cache starts over when full.
pub const BANK_CACHE: usize = 64;
/// Loadout children whose triggers are added to a ship's or vehicle's own.
pub const MAX_LOADOUT: usize = 200;
/// Largest media file read (streamed music runs to tens of MB).
pub const MAX_MEDIA: u64 = 512 * 1024 * 1024;
/// Largest bank read.
const MAX_BANK: u64 = 1024 * 1024 * 1024;

/// Where a sound's bytes are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    /// Inside its bank (DIDX/DATA).
    Embedded,
    /// A loose `Data\Sounds\wwise\...\<id>.wem`.
    MediaFile,
    /// An ATL external source with its own path.
    External,
    /// Referenced, but no bytes were found.
    Unavailable,
}

/// Enough to read one sound's bytes again.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SoundRef {
    pub media_id: u32,
    pub source: SourceKind,
    pub bank: String,
    #[serde(default)]
    pub media_path: Option<String>,
}

impl SoundRef {
    /// A cache key: the media id, or the external path's hash.
    pub fn key(&self) -> String {
        match &self.media_path {
            Some(p) => format!(
                "x{}",
                &starorg_core::fsutil::sha256_hex(p.to_ascii_lowercase().as_bytes())[..24]
            ),
            None => format!("{}", self.media_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoundInfo {
    #[serde(flatten)]
    pub sound: SoundRef,
    pub label: String,
    pub playable: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerInfo {
    /// The ATL trigger name the record names.
    pub name: String,
    /// The DataCore property it came from (`Components[...].audioTrigger`).
    pub property: String,
    /// The record it came from (the record itself, or a loadout item).
    pub from: String,
    pub bank: Option<String>,
    pub event: Option<String>,
    pub duration_type: Option<String>,
    pub sounds: Vec<SoundInfo>,
}

/// One ATL trigger found by name (7.17).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerHit {
    pub name: String,
    pub event: String,
    pub bank: String,
    pub duration_type: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerSearch {
    pub hits: Vec<TriggerHit>,
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordSounds {
    pub record: String,
    pub name: String,
    pub triggers: Vec<TriggerInfo>,
    pub sounds: u32,
    pub playable: u32,
    /// Loadout items whose triggers are included.
    pub loadout: u32,
}

struct LoadedBank {
    hierarchy: Hierarchy,
    embedded: HashSet<u32>,
}

/// The ATL triggers, external sources, and every `.bnk`/`.wem` file name under
/// `Data\Sounds\wwise\`.
pub struct AudioIndex {
    atl: AtlIndex,
    external: ExternalSourceIndex,
    paths: HashMap<String, String>,
    banks: Mutex<HashMap<String, Option<Arc<LoadedBank>>>>,
}

impl AudioIndex {
    /// One scan of the GameAudio documents and of the archive's file names.
    pub fn build(p4k: &MappedP4k) -> AssetResult<Self> {
        let (atl, external) = crate::extract::guarded("audio index", || {
            starbreaker_wwise::load_game_audio(p4k)
                .map_err(|e| crate::extract::ExtractError::Output(e.to_string()))
        })
        .map_err(|e| AssetError::internal(e.to_string()))?;
        let mut paths = HashMap::new();
        for e in p4k.entries() {
            let n = e.name.as_str();
            if n.starts_with("Data\\Sounds\\wwise\\")
                && (n.ends_with(".bnk") || n.ends_with(".wem"))
                && let Some(f) = n.rsplit('\\').next()
            {
                paths.insert(f.to_string(), n.to_string());
            }
        }
        Ok(Self {
            atl,
            external,
            paths,
            banks: Mutex::new(HashMap::new()),
        })
    }

    pub fn triggers(&self) -> usize {
        self.atl.len()
    }

    /// Triggers whose name, Wwise event or bank holds every word (case-insensitive), ranked:
    /// name starts with the words run together, then name holds them, then event/bank.
    /// Feeds the Explorer's "Sounds" search mode (addendum 7.17).
    pub fn search_triggers(&self, query: &str, limit: usize) -> TriggerSearch {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if words.is_empty() {
            return TriggerSearch::default();
        }
        let joined = words.join("_");
        let mut hits: Vec<(u8, &starbreaker_wwise::AtlTrigger)> = self
            .atl
            .iter()
            .filter_map(|(name, t)| {
                let n = name.to_lowercase();
                let other = format!("{} {}", t.wwise_event_name, t.bank_name).to_lowercase();
                if !words
                    .iter()
                    .all(|w| n.contains(w.as_str()) || other.contains(w.as_str()))
                {
                    return None;
                }
                let rank = if n.starts_with(&joined) || n.contains(&format!("_{joined}")) {
                    0
                } else if words.iter().all(|w| n.contains(w.as_str())) {
                    1
                } else {
                    2
                };
                Some((rank, t))
            })
            .collect();
        let total = hits.len() as u32;
        hits.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then(a.1.trigger_name.len().cmp(&b.1.trigger_name.len()))
                .then(a.1.trigger_name.cmp(&b.1.trigger_name))
        });
        TriggerSearch {
            hits: hits
                .into_iter()
                .take(limit.min(500))
                .map(|(_, t)| TriggerHit {
                    name: t.trigger_name.clone(),
                    event: t.wwise_event_name.clone(),
                    bank: t.bank_name.clone(),
                    duration_type: Some(t.duration_type.clone()).filter(|d| !d.is_empty()),
                })
                .collect(),
            total,
        }
    }

    /// One trigger with the sounds it plays.
    pub fn trigger(&self, a: &AssetArchive, name: &str) -> TriggerInfo {
        let (atl, sounds) = self.resolve(a, name);
        TriggerInfo {
            name: name.to_string(),
            property: String::new(),
            from: String::new(),
            bank: atl.map(|x| x.bank_name.clone()),
            event: atl.map(|x| x.wwise_event_name.clone()),
            duration_type: atl
                .map(|x| x.duration_type.clone())
                .filter(|d| !d.is_empty()),
            sounds,
        }
    }

    fn bank_path(&self, bank: &str) -> String {
        self.paths
            .get(bank)
            .cloned()
            .unwrap_or_else(|| format!("Data\\Sounds\\wwise\\{bank}"))
    }

    fn bank(&self, a: &AssetArchive, name: &str) -> Option<Arc<LoadedBank>> {
        let mut cache = self.banks.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(b) = cache.get(name) {
            return b.clone();
        }
        let loaded = (|| {
            let e = a.entry(&self.bank_path(name))?;
            let data = a.read(e, MAX_BANK).ok()?;
            let bnk = BnkFile::parse(&data).ok()?;
            let hirc = bnk.hirc.as_ref()?;
            Some(Arc::new(LoadedBank {
                hierarchy: Hierarchy::from_section(hirc),
                embedded: bnk.data_index.iter().map(|d| d.id).collect(),
            }))
        })();
        if cache.len() >= BANK_CACHE {
            cache.clear();
        }
        cache.insert(name.to_string(), loaded.clone());
        loaded
    }

    fn classify(
        &self,
        media_id: u32,
        hirc: Option<SoundSource>,
        embedded: &HashSet<u32>,
        bank: &str,
    ) -> SoundInfo {
        let (source, playable, detail) = if embedded.contains(&media_id) {
            (
                SourceKind::Embedded,
                true,
                "embedded in the bank".to_string(),
            )
        } else if let Some(p) = self.paths.get(&format!("{media_id}.wem")) {
            (SourceKind::MediaFile, true, p.clone())
        } else {
            let why = match hirc {
                Some(SoundSource::PrefetchStream | SoundSource::Stream) => {
                    "streamed media, but no loose media file was found"
                }
                _ => "the bank holds no data for it and no loose media file was found",
            };
            (SourceKind::Unavailable, false, why.to_string())
        };
        SoundInfo {
            sound: SoundRef {
                media_id,
                source,
                bank: bank.to_string(),
                media_path: None,
            },
            label: media_id.to_string(),
            playable,
            detail,
        }
    }

    /// The sounds an ATL trigger plays, or the external source of that name.
    fn resolve(
        &self,
        a: &AssetArchive,
        trigger: &str,
    ) -> (Option<&starbreaker_wwise::AtlTrigger>, Vec<SoundInfo>) {
        let Some(t) = self.atl.get_trigger(trigger) else {
            let ext: Vec<SoundInfo> = self
                .external
                .search(trigger)
                .into_iter()
                .filter(|s| s.name.eq_ignore_ascii_case(trigger))
                .take(8)
                .map(|s| SoundInfo {
                    sound: SoundRef {
                        media_id: 0,
                        source: SourceKind::External,
                        bank: s.language.clone(),
                        media_path: Some(s.p4k_path.clone()),
                    },
                    label: s.name.clone(),
                    playable: a.entry(&s.p4k_path).is_some(),
                    detail: s.p4k_path.clone(),
                })
                .collect();
            return (None, ext);
        };
        let Some(bank) = self.bank(a, &t.bank_name) else {
            return (Some(t), Vec::new());
        };
        let mut seen = HashSet::new();
        let sounds = bank
            .hierarchy
            .resolve_event_by_name(&t.wwise_event_name)
            .into_iter()
            .filter(|s| seen.insert(s.media_id))
            .map(|s| self.classify(s.media_id, Some(s.source), &bank.embedded, &t.bank_name))
            .collect();
        (Some(t), sounds)
    }

    /// The bytes of one sound (WEM).
    pub fn media_bytes(&self, a: &AssetArchive, s: &SoundRef) -> AssetResult<Vec<u8>> {
        let read = |path: &str| -> AssetResult<Vec<u8>> {
            let e = a
                .entry(path)
                .ok_or_else(|| AssetError::not_found(format!("no file {path}")))?;
            a.read(e, MAX_MEDIA)
        };
        let loose = |id: u32| -> AssetResult<Vec<u8>> {
            match self.paths.get(&format!("{id}.wem")) {
                Some(p) => read(p),
                None => read(&format!("Data\\Sounds\\wwise\\Media\\{id}.wem")),
            }
        };
        if let Some(p) = &s.media_path {
            return read(p);
        }
        match s.source {
            SourceKind::MediaFile => loose(s.media_id),
            SourceKind::Embedded => {
                let e = a
                    .entry(&self.bank_path(&s.bank))
                    .ok_or_else(|| AssetError::not_found(format!("no bank {}", s.bank)))?;
                let data = a.read(e, MAX_BANK)?;
                let from_bank = BnkFile::parse(&data)
                    .ok()
                    .and_then(|b| b.wem_data_by_id(s.media_id).ok().map(<[u8]>::to_vec));
                match from_bank {
                    Some(b) => Ok(b),
                    None => loose(s.media_id),
                }
            }
            SourceKind::External => Err(AssetError::invalid("an external source needs its path")),
            SourceKind::Unavailable => Err(AssetError::not_found(format!(
                "media {} has no data in the game files",
                s.media_id
            ))),
        }
    }
}

/// Vorbis WEM -> Ogg (`ogg`); any other codec stays WEM (`wem`).
pub fn decode(wem: &[u8]) -> AssetResult<(Vec<u8>, &'static str)> {
    let parsed = starbreaker_wem::WemFile::parse(wem)
        .map_err(|e| AssetError::invalid(format!("not a WEM file: {e}")))?;
    match parsed.codec_type() {
        starbreaker_wem::WemCodec::Vorbis => crate::extract::guarded("WEM decode", || {
            starbreaker_wem::decode::vorbis_to_ogg(wem)
                .map_err(|e| crate::extract::ExtractError::Output(e.to_string()))
        })
        .map(|b| (b, "ogg"))
        .map_err(|e| AssetError::invalid(e.to_string())),
        _ => Ok((wem.to_vec(), "wem")),
    }
}

/// Estimated length in seconds, when the WEM says.
pub fn duration(wem: &[u8]) -> Option<f64> {
    starbreaker_wem::WemFile::parse(wem)
        .ok()?
        .estimated_duration_secs()
}

fn walk_loadout<'n>(n: &'n LoadoutNode, out: &mut Vec<&'n LoadoutNode>) {
    for c in &n.children {
        if out.len() >= MAX_LOADOUT {
            return;
        }
        out.push(c);
        walk_loadout(c, out);
    }
}

/// The record's audio triggers (and its default loadout's, for ships and vehicles), each
/// with the sounds it plays.
pub fn record_sounds(a: &AssetArchive, what: &str) -> AssetResult<RecordSounds> {
    let db: &Database<'static> = a.db()?;
    let r = find_record(db, what)?;
    let full = db.resolve_string2(r.name_offset).to_string();
    let mut sources: Vec<(String, String)> = vec![(full.clone(), record_short_name(db, r))];
    let entity = db.struct_id("EntityClassDefinition");
    let mut loadout = 0;
    if Some(r.struct_id()) == entity {
        let idx = EntityIndex::new(db);
        let tree = resolve_loadout_indexed(&idx, r);
        let mut nodes = Vec::new();
        walk_loadout(&tree.root, &mut nodes);
        let mut seen = HashSet::new();
        for n in nodes {
            let name = db.resolve_string2(n.record.name_offset).to_string();
            if seen.insert(name.clone()) && name != full {
                sources.push((name, n.entity_name.clone()));
                loadout += 1;
            }
        }
    }
    let idx = a.audio()?;
    let mut triggers = Vec::new();
    let mut seen = HashSet::new();
    for (name, label) in &sources {
        let Some(info) = starbreaker_wwise::datacore_audio::find_audio_triggers(db, name) else {
            continue;
        };
        for t in info.triggers {
            if t.trigger_name.is_empty() || !seen.insert(t.trigger_name.clone()) {
                continue;
            }
            let (atl, sounds) = idx.resolve(a, &t.trigger_name);
            triggers.push(TriggerInfo {
                name: t.trigger_name,
                property: t.property_path,
                from: label.clone(),
                bank: atl.map(|x| x.bank_name.clone()),
                event: atl.map(|x| x.wwise_event_name.clone()),
                duration_type: atl
                    .map(|x| x.duration_type.clone())
                    .filter(|d| !d.is_empty()),
                sounds,
            });
        }
    }
    let sounds = triggers.iter().map(|t| t.sounds.len() as u32).sum();
    let playable = triggers
        .iter()
        .flat_map(|t| &t.sounds)
        .filter(|s| s.playable)
        .count() as u32;
    Ok(RecordSounds {
        record: r.id.to_string(),
        name: record_short_name(db, r),
        triggers,
        sounds,
        playable,
        loadout,
    })
}

/// A file name for an exported sound: `<trigger>_<media id>.<ext>`, filesystem-safe.
pub fn file_name(trigger: &str, s: &SoundRef, ext: &str) -> String {
    let base: String = trigger
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    let id = match &s.media_path {
        Some(p) => p
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or("external")
            .trim_end_matches(".wem")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
            .collect::<String>(),
        None => s.media_id.to_string(),
    };
    if base.is_empty() {
        format!("{id}.{ext}")
    } else {
        format!("{base}_{id}.{ext}")
    }
}
