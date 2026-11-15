//! Per-channel state in the data directory (spec 4.2, 4.4, 4.7):
//! `channel.toml` (consent, language decision), `profile.toml`, `overrides.ini`.

use crate::fsutil;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Overrides larger than this are rejected (spec 11).
pub const OVERRIDES_LIMIT: u64 = 8 << 20;

pub const CHANNEL_TOML: &str = "channel.toml";
pub const PROFILE_TOML: &str = "profile.toml";
pub const OVERRIDES_INI: &str = "overrides.ini";
pub const APPLIED_JSON: &str = "applied.json";
pub const INTENT_JSON: &str = "intent.json";

/// `channel.toml`. A channel is **set up** when `consent = true` (spec 4.1).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChannelState {
    #[serde(default)]
    pub consent: bool,
    /// When `setup` ran (RFC 3339 UTC).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup_time: Option<String>,
    /// The user agreed to replace another `g_language` value with `english` (spec 7.4).
    #[serde(default)]
    pub switch_language: bool,
    #[serde(flatten)]
    pub extra: toml::Table,
}

fn read_toml<T: serde::de::DeserializeOwned + Default>(path: &Path) -> std::io::Result<T> {
    match fsutil::read_limited(path, 1 << 20)? {
        None => Ok(T::default()),
        Some(b) => {
            let s = String::from_utf8(b).map_err(std::io::Error::other)?;
            toml::from_str(&s)
                .map_err(|e| std::io::Error::other(format!("{}: {e}", path.display())))
        }
    }
}

impl ChannelState {
    pub fn load(channel_dir: &Path) -> std::io::Result<Self> {
        read_toml(&channel_dir.join(CHANNEL_TOML))
    }

    pub fn save(&self, channel_dir: &Path) -> std::io::Result<()> {
        let s = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        fsutil::write_atomic(&channel_dir.join(CHANNEL_TOML), s.as_bytes())
    }
}

/// Presets shipped (spec 4.7).
pub const PRESETS: &[&str] = &["recommended", "minimal", "everything"];

/// `profile.toml`, kept as a TOML table so unknown keys survive a write by an older version.
/// M1 reads only `preset` and `patches.disabled`; the category settings arrive with the
/// generators in M2.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Profile {
    pub table: toml::Table,
}

impl Profile {
    pub fn preset(name: &str) -> Option<Self> {
        if !PRESETS.contains(&name) {
            return None;
        }
        let mut table = toml::Table::new();
        table.insert("schema".into(), toml::Value::Integer(1));
        table.insert("preset".into(), toml::Value::String(name.into()));
        let mut patches = toml::Table::new();
        patches.insert("disabled".into(), toml::Value::Array(Vec::new()));
        table.insert("patches".into(), toml::Value::Table(patches));
        Some(Self { table })
    }

    pub fn path(channel_dir: &Path) -> PathBuf {
        channel_dir.join(PROFILE_TOML)
    }

    pub fn load_file(path: &Path) -> std::io::Result<Self> {
        Ok(Self {
            table: read_toml(path)?,
        })
    }

    pub fn save(&self, channel_dir: &Path) -> std::io::Result<()> {
        let s = toml::to_string_pretty(&self.table).map_err(std::io::Error::other)?;
        fsutil::write_atomic(&Self::path(channel_dir), s.as_bytes())
    }

    /// Ids of built-in patches the user switched off.
    pub fn disabled_patches(&self) -> Vec<String> {
        self.table
            .get("patches")
            .and_then(|p| p.get("disabled"))
            .and_then(|d| d.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Hash for the applied manifest: of the canonical TOML text.
    pub fn hash(&self) -> String {
        let s = toml::to_string(&self.table).unwrap_or_default();
        fsutil::sha256_hex(s.as_bytes())
    }
}

/// One line of `overrides.ini`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Override {
    pub line: usize,
    pub key: String,
    pub value: String,
}

/// Parse `overrides.ini` (spec 4.4): UTF-8, `;` comments, blank lines, split on the first
/// `=`, value taken raw (no trimming, no quotes). A BOM and CRLF endings are tolerated on
/// read. Lines without `=` are reported back as warnings.
pub fn parse_overrides(text: &str) -> (Vec<Override>, Vec<String>) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut out = Vec::new();
    let mut bad = Vec::new();
    for (i, raw) in text.split('\n').enumerate() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        match line.split_once('=') {
            Some((k, v)) if !k.is_empty() => out.push(Override {
                line: i + 1,
                key: k.to_string(),
                value: v.to_string(),
            }),
            _ => bad.push(format!("overrides.ini line {}: no key=value", i + 1)),
        }
    }
    (out, bad)
}

/// Read a channel's overrides. Missing file = none. Returns the raw bytes too (for hashing).
pub fn load_overrides(
    channel_dir: &Path,
) -> std::io::Result<(Vec<u8>, Vec<Override>, Vec<String>)> {
    let Some(bytes) = fsutil::read_limited(&channel_dir.join(OVERRIDES_INI), OVERRIDES_LIMIT)?
    else {
        return Ok((Vec::new(), Vec::new(), Vec::new()));
    };
    let text = String::from_utf8(bytes.clone())
        .map_err(|_| std::io::Error::other("overrides.ini is not valid UTF-8"))?;
    let (o, bad) = parse_overrides(&text);
    Ok((bytes, o, bad))
}

/// Write `overrides.ini` atomically, keeping the previous version as `overrides.ini.bak`.
pub fn save_overrides(channel_dir: &Path, text: &str) -> std::io::Result<()> {
    let path = channel_dir.join(OVERRIDES_INI);
    if path.exists() {
        std::fs::copy(&path, channel_dir.join("overrides.ini.bak"))?;
    }
    fsutil::write_atomic(&path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_parse_raw_values() {
        let (o, bad) = parse_overrides(
            "; comment\n\nvehicle_NameA=*05-Avenger Titan\nitem_x=  spaced = kept \nnokey\n=x\n",
        );
        assert_eq!(o.len(), 2);
        assert_eq!(o[0].key, "vehicle_NameA");
        assert_eq!(o[0].value, "*05-Avenger Titan");
        assert_eq!(o[1].value, "  spaced = kept ");
        assert_eq!(bad.len(), 2);
    }

    #[test]
    fn profile_keeps_unknown_keys_and_reads_disabled_patches() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(
            t.path().join(PROFILE_TOML),
            "preset = \"minimal\"\nfuture = 3\n[patches]\ndisabled = [\"x\"]\n",
        )
        .unwrap();
        let p = Profile::load_file(&Profile::path(t.path())).unwrap();
        assert_eq!(p.disabled_patches(), vec!["x".to_string()]);
        p.save(t.path()).unwrap();
        let s = std::fs::read_to_string(t.path().join(PROFILE_TOML)).unwrap();
        assert!(s.contains("future = 3"));
        assert!(Profile::preset("nope").is_none());
        assert_ne!(
            Profile::preset("minimal").unwrap().hash(),
            Profile::preset("everything").unwrap().hash()
        );
    }

    #[test]
    fn overrides_bak_is_kept() {
        let t = tempfile::tempdir().unwrap();
        save_overrides(t.path(), "a=1\n").unwrap();
        save_overrides(t.path(), "a=2\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(t.path().join("overrides.ini.bak")).unwrap(),
            "a=1\n"
        );
    }
}
