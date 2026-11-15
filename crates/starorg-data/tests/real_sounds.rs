//! Sounds of a real LIVE install, by codec (addendum 7.16): every sound of a few ships and
//! weapons, two user-reported triggers and a sample of the loose media are converted; each
//! codec this build converts must convert every time, and each Ogg Vorbis must decode to its
//! end. Read only. Ignored by default:
//!
//! ```sh
//! cargo test --release -p starorg-data --test real_sounds -- --ignored --nocapture
//! ```
//!
//! `STARORG_GAME_DIR` as in `real_archives`; `STARORG_SOUND_SAMPLE` loose files (default
//! 2000, 0 to skip); `STARORG_SOUND_DUMP=<dir>` writes the two reported sounds' files there.

use starorg_data::assets::audio::{self, SoundRef};
use starorg_data::assets::wemcodec::{self, Codec};
use starorg_data::assets::{AssetArchive, AssetResult};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

fn live_p4k() -> PathBuf {
    std::env::var_os("STARORG_GAME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                "/mnt/games/star-citizen/drive_c/Program Files/Roberts Space Industries/StarCitizen",
            )
        })
        .join("LIVE")
        .join("Data.p4k")
}

const RECORDS: [&str; 6] = [
    "AEGS_Gladius",
    "ANVL_Arrow",
    "DRAK_Cutlass_Black",
    "MISC_Freelancer",
    "behr_rifle_ballistic_01",
    "klwe_pistol_energy_01",
];
/// User reports: "Not Vorbis audio" (1044913056) and "NotSupportedError" (1020390043).
const TRIGGERS: [(&str, u32); 2] = [
    ("Play_PHOB_Med_Cargo_Box_Metal_b_Impacts", 1_044_913_056),
    ("Play_SSDM_Shared_Explosion_Base_Light", 1_020_390_043),
];

#[derive(Default)]
struct Tally {
    /// codec -> (converted, kept)
    by: BTreeMap<String, (u32, u32)>,
    bad: Vec<String>,
}

impl Tally {
    fn add(&mut self, what: &str, wem: &[u8]) -> Option<wemcodec::Converted> {
        let codec = wemcodec::codec_of(wem).ok();
        let name = codec.map_or("not a WEM".into(), |c| c.to_string());
        let e = self.by.entry(name).or_default();
        match audio::decode(wem) {
            Ok(c) => {
                e.0 += 1;
                if c.format == wemcodec::Format::Ogg
                    && let Err(x) = wemcodec::check_ogg_vorbis(&c.bytes)
                {
                    self.bad
                        .push(format!("{what}: converted Ogg does not decode: {x}"));
                }
                Some(c)
            }
            Err(u) => {
                e.1 += 1;
                // A codec this build converts must convert.
                if codec.and_then(Codec::format).is_some() || codec.is_none() {
                    self.bad.push(format!("{what}: {}", u.message()));
                }
                None
            }
        }
    }

    fn print(&self, title: &str) {
        println!("{title}:");
        for (c, (ok, kept)) in &self.by {
            println!("  {c:<20} converted {ok:>6}  kept as .wem {kept:>4}");
        }
    }
}

#[test]
#[ignore = "needs a game install"]
fn every_supported_codec_converts_on_live() -> AssetResult<()> {
    let a = AssetArchive::open(&live_p4k())?;
    let idx = a.audio()?;
    let dump = std::env::var_os("STARORG_SOUND_DUMP").map(PathBuf::from);
    let mut t = Tally::default();
    // The reported triggers: every sound, and the two named ones become Ogg Vorbis.
    for (trigger, id) in TRIGGERS {
        let info = idx.trigger(&a, trigger)?;
        let mut found = false;
        for s in info.sounds.iter().filter(|s| s.playable) {
            let wem = idx.media_bytes(&a, &s.sound)?;
            let c = t.add(&format!("{trigger} {}", s.sound.media_id), &wem);
            if s.sound.media_id == id {
                found = true;
                let c = c.unwrap_or_else(|| panic!("{id} did not convert"));
                assert_eq!(c.format, wemcodec::Format::Ogg, "{id}");
                let n = wemcodec::check_ogg_vorbis(&c.bytes).unwrap();
                println!("{trigger} {id}: {} bytes Ogg, {n} samples", c.bytes.len());
                if let Some(d) = &dump {
                    std::fs::write(d.join(format!("{id}.ogg")), &c.bytes).unwrap();
                }
            }
        }
        assert!(found, "{trigger} plays {id}");
    }
    t.print("reported triggers");
    // Records: every unique sound.
    let mut r = Tally::default();
    for what in RECORDS {
        let rs = audio::record_sounds(&a, what)?;
        let mut seen = HashSet::new();
        let refs: Vec<SoundRef> = rs
            .triggers
            .iter()
            .flat_map(|t| &t.sounds)
            .filter(|s| s.playable && seen.insert(s.sound.key()))
            .map(|s| s.sound.clone())
            .collect();
        for s in &refs {
            match idx.media_bytes(&a, s) {
                Ok(w) => {
                    r.add(&format!("{what} {}", s.key()), &w);
                }
                Err(e) => r
                    .bad
                    .push(format!("{what} {}: read: {}", s.key(), e.message)),
            }
        }
        println!("{what}: {} unique sounds", refs.len());
    }
    r.print("records");
    // A sample of the loose media.
    let n: usize = std::env::var("STARORG_SOUND_SAMPLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2000);
    let mut l = Tally::default();
    if n > 0 {
        let es: Vec<_> = a
            .p4k()
            .entries()
            .iter()
            .filter(|e| e.name.starts_with("Data\\Sounds\\wwise\\") && e.name.ends_with(".wem"))
            .collect();
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        for _ in 0..n {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let e = es[(x % es.len() as u64) as usize];
            if e.uncompressed_size > 64 << 20 {
                continue;
            }
            let w = a.read(e, 64 << 20)?;
            l.add(&e.name, &w);
        }
        l.print(&format!("{n} loose media of {}", es.len()));
    }
    let bad: Vec<_> = t.bad.iter().chain(&r.bad).chain(&l.bad).collect();
    assert!(bad.is_empty(), "{} failures: {bad:#?}", bad.len());
    Ok(())
}
