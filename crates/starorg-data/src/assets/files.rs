//! The archive as a file tree: listing (paged), search, previews and extraction.

use super::{AssetArchive, AssetError, AssetResult, norm_path};
use serde::{Deserialize, Serialize};
use starbreaker_p4k::{DirEntry, P4kEntry};
use std::path::{Component, Path, PathBuf};

/// At most this many rows per listing or search reply.
pub const MAX_PAGE: usize = 500;
/// Text previews are cut at 1 MiB.
pub const MAX_TEXT: usize = 1024 * 1024;
/// Files larger than this are not read for a preview (a texture's base file is small; its
/// mips are split into siblings).
pub const MAX_PREVIEW_READ: u64 = 256 * 1024 * 1024;
/// Largest side of a texture preview; a smaller mip is picked.
pub const PREVIEW_SIDE: u32 = 1024;
/// Largest single file extracted (the biggest LIVE entries are a few GB of streamed audio).
pub const MAX_EXTRACT_FILE: u64 = 8 * 1024 * 1024 * 1024;

/// One row of a listing or search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    /// Full archive path, backslash separated (`Data\Objects\...`).
    pub path: String,
    pub dir: bool,
    /// Uncompressed size (0 for folders).
    pub size: u64,
    /// Size in the archive.
    pub packed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirPage {
    /// `""` = the root.
    pub dir: String,
    pub parent: Option<String>,
    pub entries: Vec<FileEntry>,
    pub offset: u32,
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSearch {
    pub query: String,
    pub hits: Vec<FileEntry>,
    pub total: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PreviewKind {
    /// A PNG written to the preview path (DDS decoded, or a PNG/JPEG copied).
    Image,
    /// `text` holds it (XML, CryXML converted to XML, JSON, INI ...).
    Text,
    /// A WEM: play it through the sounds call with `media_path`.
    Audio,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preview {
    pub path: String,
    pub size: u64,
    pub kind: PreviewKind,
    /// `dds`, `cryxml`, `xml`, `png`, `wem`, ... (what it was read as).
    pub format: String,
    pub text: Option<String>,
    pub truncated: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Image previews: the mime type of the file written (`image/png`, `image/jpeg`).
    pub mime: Option<String>,
    pub note: Option<String>,
}

fn entry_row(e: &P4kEntry) -> FileEntry {
    FileEntry {
        name: e.name.rsplit('\\').next().unwrap_or(&e.name).to_string(),
        path: e.name.clone(),
        dir: false,
        size: e.uncompressed_size,
        packed: e.compressed_size,
    }
}

fn parent_of(dir: &str) -> Option<String> {
    if dir.is_empty() {
        return None;
    }
    Some(dir.rsplit_once('\\').map(|x| x.0).unwrap_or("").to_string())
}

/// One folder's children: folders first, then files, each sorted by name (case-insensitive).
pub fn list(a: &AssetArchive, dir: &str, offset: usize, limit: usize) -> AssetResult<DirPage> {
    let dir = norm_path(dir);
    let mut rows: Vec<FileEntry> = a
        .p4k()
        .list_dir(&dir)
        .into_iter()
        .map(|d| match d {
            DirEntry::Directory(name) => FileEntry {
                path: if dir.is_empty() {
                    name.clone()
                } else {
                    format!("{dir}\\{name}")
                },
                name,
                dir: true,
                size: 0,
                packed: 0,
            },
            DirEntry::File(e) => entry_row(e),
        })
        .collect();
    if rows.is_empty() && !dir.is_empty() {
        return Err(AssetError::not_found(format!("no folder {dir}")));
    }
    rows.sort_by(|x, y| {
        y.dir.cmp(&x.dir).then_with(|| {
            x.name
                .to_ascii_lowercase()
                .cmp(&y.name.to_ascii_lowercase())
        })
    });
    let total = rows.len();
    let entries = rows
        .into_iter()
        .skip(offset)
        .take(limit.min(MAX_PAGE))
        .collect();
    Ok(DirPage {
        parent: parent_of(&dir),
        dir,
        entries,
        offset: offset as u32,
        total: total as u32,
    })
}

/// Files whose path contains every word of `query` (case-insensitive), sorted by path.
pub fn search(a: &AssetArchive, query: &str, limit: usize) -> FileSearch {
    let q = query.replace('/', "\\");
    let mut hits = a.p4k().search(&q);
    let total = hits.len();
    let entries = a.p4k().entries();
    hits.sort_by(|x, y| entries[*x as usize].name.cmp(&entries[*y as usize].name));
    FileSearch {
        query: query.to_string(),
        hits: hits
            .into_iter()
            .take(limit.min(MAX_PAGE))
            .map(|i| entry_row(&entries[i as usize]))
            .collect(),
        total: total as u32,
    }
}

/// The lowercase extension of an archive path (`x.dds` -> `dds`, `x.dds.1` -> `1`).
pub fn ext(path: &str) -> String {
    let name = path.rsplit('\\').next().unwrap_or(path);
    name.rsplit_once('.')
        .map(|x| x.1.to_ascii_lowercase())
        .unwrap_or_default()
}

/// A split mip or alpha sibling of a texture (`x.dds.1`, `x.dds.3a`, `x.dds.a`): merged into
/// the PNG of `x.dds` when converting.
pub fn is_dds_sibling(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let Some(at) = lower.rfind(".dds.") else {
        return false;
    };
    let tail = &lower[at + 5..];
    !tail.is_empty()
        && tail
            .trim_end_matches('a')
            .chars()
            .all(|c| c.is_ascii_digit())
}

const TEXT_EXTS: &[&str] = &[
    "xml",
    "json",
    "ini",
    "cfg",
    "txt",
    "mtl",
    "cdf",
    "chrparams",
    "adb",
    "lua",
    "entxml",
    "csv",
    "log",
    "bspace",
    "comb",
    "animevents",
    "vehicle",
    "rmp",
    "dpl",
    "sbsar_meta",
    "socpak_meta",
    "dfw",
    "ent",
    "prefab",
    "lyr",
    "xls",
    "id",
    "uix",
    "def",
];

struct P4kSibling<'a> {
    a: &'a AssetArchive,
    base: String,
}

impl starbreaker_dds::ReadSibling for P4kSibling<'_> {
    fn read_sibling(&self, suffix: &str) -> Option<Vec<u8>> {
        let e = self.a.entry(&format!("{}{suffix}", self.base))?;
        self.a.read(e, MAX_PREVIEW_READ).ok()
    }
}

/// A texture with its split mips merged.
pub fn load_dds(a: &AssetArchive, path: &str) -> AssetResult<starbreaker_dds::DdsFile> {
    let e = a
        .entry(path)
        .ok_or_else(|| AssetError::not_found(format!("no file {path}")))?;
    let bytes = a.read(e, MAX_PREVIEW_READ)?;
    let reader = P4kSibling {
        a,
        base: e.name.clone(),
    };
    crate::extract::guarded("texture decode", || {
        starbreaker_dds::DdsFile::from_split(&bytes, &reader)
            .map_err(|x| crate::extract::ExtractError::Output(x.to_string()))
    })
    .map_err(|x| AssetError::invalid(format!("{path}: {x}")))
}

fn write_via_part(out: &Path, write: impl FnOnce(&Path) -> AssetResult<()>) -> AssetResult<()> {
    let mut s = out.as_os_str().to_owned();
    s.push(".part");
    let part = PathBuf::from(s);
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).map_err(|e| AssetError::internal(e.to_string()))?;
    }
    let r = write(&part).and_then(|()| {
        std::fs::rename(&part, out).map_err(|e| AssetError::internal(e.to_string()))
    });
    if r.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    r
}

/// Save a texture as PNG at the largest mip no wider or taller than `side` (0 = full size).
pub fn dds_to_png(
    dds: &starbreaker_dds::DdsFile,
    out: &Path,
    side: u32,
) -> AssetResult<(u32, u32)> {
    let mips = dds.mip_count().max(1);
    let mip = (0..mips)
        .find(|&m| {
            let (w, h) = dds.dimensions(m);
            side == 0 || (w <= side && h <= side)
        })
        .unwrap_or(mips - 1);
    let (w, h) = dds.dimensions(mip);
    // The image crate picks the format by extension: `<name>.part.png`, then renamed.
    let name = out
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let part = out.with_file_name(format!("{name}.part.png"));
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).map_err(|e| AssetError::internal(e.to_string()))?;
    }
    let r = crate::extract::guarded("texture decode", || {
        dds.save_png(&part, mip)
            .map_err(|x| crate::extract::ExtractError::Output(x.to_string()))
    })
    .map_err(|x| AssetError::invalid(x.to_string()))
    .and_then(|()| std::fs::rename(&part, out).map_err(|e| AssetError::internal(e.to_string())));
    if r.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    r?;
    Ok((w, h))
}

/// CryXmlB bytes as XML text; `None` when the bytes are not CryXmlB.
pub fn cryxml_text(bytes: &[u8]) -> AssetResult<Option<String>> {
    if !starbreaker_cryxml::is_cryxmlb(bytes) {
        return Ok(None);
    }
    crate::extract::guarded("CryXML", || {
        starbreaker_cryxml::from_bytes(bytes)
            .map(|x| format!("{x}"))
            .map_err(|e| crate::extract::ExtractError::Output(e.to_string()))
    })
    .map(Some)
    .map_err(|e| AssetError::invalid(e.to_string()))
}

fn plain_text(bytes: &[u8]) -> Option<String> {
    let head = &bytes[..bytes.len().min(8192)];
    if head.contains(&0) {
        return None;
    }
    let s = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_TEXT)]);
    Some(s.into_owned())
}

/// What a file looks like. Images are written to `image_out` (PNG, or the original JPEG);
/// text comes back inline (at most 1 MiB).
pub fn preview(a: &AssetArchive, path: &str, image_out: &Path) -> AssetResult<Preview> {
    let e = a
        .entry(path)
        .ok_or_else(|| AssetError::not_found(format!("no file {path}")))?;
    let x = ext(&e.name);
    let mut p = Preview {
        path: e.name.clone(),
        size: e.uncompressed_size,
        kind: PreviewKind::None,
        format: x.clone(),
        text: None,
        truncated: false,
        width: None,
        height: None,
        mime: None,
        note: None,
    };
    if x == "dds" {
        a.check_out(image_out)?;
        let dds = load_dds(a, &e.name)?;
        let (fw, fh) = dds.dimensions(0);
        let (w, h) = dds_to_png(&dds, image_out, PREVIEW_SIDE)?;
        p.kind = PreviewKind::Image;
        p.width = Some(w);
        p.height = Some(h);
        p.mime = Some("image/png".into());
        if (w, h) != (fw, fh) {
            p.note = Some(format!("{fw} x {fh}, shown at {w} x {h}"));
        }
        return Ok(p);
    }
    if is_dds_sibling(&e.name) {
        p.note = Some("A mip level of a texture; open the .dds file".into());
        return Ok(p);
    }
    if x == "wem" {
        p.kind = PreviewKind::Audio;
        return Ok(p);
    }
    if e.uncompressed_size > MAX_PREVIEW_READ {
        p.note = Some("Too large to preview; extract it".into());
        return Ok(p);
    }
    let bytes = a.read(e, MAX_PREVIEW_READ)?;
    if matches!(x.as_str(), "png" | "jpg" | "jpeg") {
        a.check_out(image_out)?;
        write_via_part(image_out, |part| {
            std::fs::write(part, &bytes).map_err(|e| AssetError::internal(e.to_string()))
        })?;
        p.kind = PreviewKind::Image;
        p.mime = Some(
            if x == "png" {
                "image/png"
            } else {
                "image/jpeg"
            }
            .into(),
        );
        return Ok(p);
    }
    if let Some(xml) = cryxml_text(&bytes)? {
        p.kind = PreviewKind::Text;
        p.format = "cryxml".into();
        p.truncated = xml.len() > MAX_TEXT;
        let mut xml = xml;
        if p.truncated {
            let mut cut = MAX_TEXT;
            while !xml.is_char_boundary(cut) {
                cut -= 1;
            }
            xml.truncate(cut);
        }
        p.text = Some(xml);
        return Ok(p);
    }
    if TEXT_EXTS.contains(&x.as_str())
        && let Some(t) = plain_text(&bytes)
    {
        p.kind = PreviewKind::Text;
        p.truncated = bytes.len() > MAX_TEXT;
        p.text = Some(t);
        return Ok(p);
    }
    p.note = Some("No preview for this file type; extract it".into());
    Ok(p)
}

/// One file of an extraction: the entry and where it goes below the output folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractItem {
    pub entry: usize,
    pub rel: PathBuf,
}

/// An archive path as a relative output path; `None` for anything that could escape the
/// output folder (`..`, a drive, an absolute path, an empty part).
pub fn safe_rel(rel: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for part in rel.split(['\\', '/']) {
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.contains(':')
            || part.chars().any(|c| c.is_control())
        {
            return None;
        }
        out.push(part);
    }
    let ok = out.components().all(|c| matches!(c, Component::Normal(_)));
    (ok && !out.as_os_str().is_empty()).then_some(out)
}

/// The files to write for the selected paths: a file as itself; a folder with everything below
/// it, kept under the folder's own name. Duplicates (a file inside a selected folder) once.
pub fn plan_extract(a: &AssetArchive, paths: &[String]) -> AssetResult<Vec<ExtractItem>> {
    let entries = a.p4k().entries();
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for raw in paths {
        let p = norm_path(raw);
        let parent_len = p.rfind('\\').map(|i| i + 1).unwrap_or(0);
        if let Some(e) = a.entry(&p) {
            let i = entries
                .iter()
                .position(|x| std::ptr::eq(x, e))
                .unwrap_or(usize::MAX);
            if i != usize::MAX && seen.insert(i) {
                let rel = safe_rel(&e.name[parent_len..])
                    .ok_or_else(|| AssetError::invalid(format!("unsafe path {}", e.name)))?;
                out.push(ExtractItem { entry: i, rel });
            }
            continue;
        }
        let prefix = if p.is_empty() {
            String::new()
        } else {
            format!("{}\\", p.to_ascii_lowercase())
        };
        let before = out.len();
        for (i, e) in entries.iter().enumerate() {
            if e.name.to_ascii_lowercase().starts_with(&prefix) && seen.insert(i) {
                let Some(rel) = safe_rel(&e.name[parent_len..]) else {
                    continue;
                };
                out.push(ExtractItem { entry: i, rel });
            }
        }
        if out.len() == before {
            return Err(AssetError::not_found(format!("no file or folder {p}")));
        }
    }
    out.sort_by(|x, y| x.rel.cmp(&y.rel));
    Ok(out)
}

/// What [`extract_item`] wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Written {
    pub bytes: u64,
    pub converted: bool,
    /// A texture sibling merged into its `.png` (nothing written).
    pub merged: bool,
}

/// Write one item below `out_dir`; with `convert`, textures become PNG (siblings merged) and
/// CryXmlB files XML.
pub fn extract_item(
    a: &AssetArchive,
    item: &ExtractItem,
    out_dir: &Path,
    convert: bool,
) -> AssetResult<Written> {
    let e = &a.p4k().entries()[item.entry];
    let mut target = out_dir.join(&item.rel);
    if convert && is_dds_sibling(&e.name) {
        return Ok(Written {
            merged: true,
            ..Default::default()
        });
    }
    a.check_out(&target)?;
    if convert && ext(&e.name) == "dds" {
        target.set_extension("png");
        let dds = load_dds(a, &e.name)?;
        dds_to_png(&dds, &target, 0)?;
        let bytes = std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
        return Ok(Written {
            bytes,
            converted: true,
            merged: false,
        });
    }
    let bytes = a.read(e, MAX_EXTRACT_FILE)?;
    let (data, converted) = match convert.then(|| cryxml_text(&bytes)).transpose()?.flatten() {
        Some(xml) => (xml.into_bytes(), true),
        None => (bytes, false),
    };
    write_via_part(&target, |part| {
        std::fs::write(part, &data).map_err(|e| AssetError::internal(e.to_string()))
    })?;
    Ok(Written {
        bytes: data.len() as u64,
        converted,
        merged: false,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractDone {
    pub out_dir: String,
    pub files: u32,
    pub bytes: u64,
    /// Textures written as PNG and CryXML as XML.
    pub converted: u32,
    /// Texture mip siblings merged into their PNG.
    pub merged: u32,
    /// The first 20 failures, `path: reason`.
    pub failed: Vec<String>,
    pub failures: u32,
}

/// Extract the planned items one by one; `step(done, total)` is called after each and stops
/// the run when it returns false (the files already written stay).
pub fn extract_all(
    a: &AssetArchive,
    items: &[ExtractItem],
    out_dir: &Path,
    convert: bool,
    step: &mut dyn FnMut(usize, usize) -> bool,
) -> AssetResult<ExtractDone> {
    a.check_out(&out_dir.join("x"))?;
    let mut d = ExtractDone {
        out_dir: out_dir.display().to_string(),
        files: 0,
        bytes: 0,
        converted: 0,
        merged: 0,
        failed: Vec::new(),
        failures: 0,
    };
    for (k, item) in items.iter().enumerate() {
        match extract_item(a, item, out_dir, convert) {
            Ok(w) if w.merged => d.merged += 1,
            Ok(w) => {
                d.files += 1;
                d.bytes += w.bytes;
                d.converted += w.converted as u32;
            }
            Err(e) if e.code == "invalid" && e.message.contains("exports never go there") => {
                return Err(e);
            }
            Err(e) => {
                d.failures += 1;
                if d.failed.len() < 20 {
                    d.failed.push(format!(
                        "{}: {}",
                        a.p4k().entries()[item.entry].name,
                        e.message
                    ));
                }
            }
        }
        if !step(k + 1, items.len()) {
            return Err(AssetError::cancelled());
        }
    }
    Ok(d)
}

/// Whether `pattern` has glob characters (`*`, `?`).
pub fn is_glob(pattern: &str) -> bool {
    pattern.contains(['*', '?'])
}

/// Case-insensitive glob over backslash paths: `**` any run of characters, `*` any run
/// within one folder, `?` one character (not `\`). `/` in the pattern means `\`.
pub fn glob_match(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern
        .replace('/', "\\")
        .to_ascii_lowercase()
        .chars()
        .collect();
    let n: Vec<char> = name.to_ascii_lowercase().chars().collect();
    // dp[i][j]: p[..i] matches n[..j]
    let mut dp = vec![vec![false; n.len() + 1]; p.len() + 1];
    dp[0][0] = true;
    let mut i = 0;
    while i < p.len() {
        let star2 = p[i] == '*' && p.get(i + 1) == Some(&'*');
        let w = if star2 { 2 } else { 1 };
        for j in 0..=n.len() {
            if !dp[i][j] {
                continue;
            }
            match p[i] {
                '*' => {
                    dp[i + w][j] = true;
                    let mut k = j;
                    while k < n.len() && (star2 || n[k] != '\\') {
                        k += 1;
                        dp[i + w][k] = true;
                    }
                }
                '?' => {
                    if j < n.len() && n[j] != '\\' {
                        dp[i + 1][j + 1] = true;
                    }
                }
                c => {
                    if j < n.len() && n[j] == c {
                        dp[i + 1][j + 1] = true;
                    }
                }
            }
        }
        i += w;
    }
    dp[p.len()][n.len()]
}

/// The files matching a glob, each kept at its full archive path below the output folder.
pub fn plan_glob(a: &AssetArchive, pattern: &str) -> AssetResult<Vec<ExtractItem>> {
    let mut out: Vec<ExtractItem> = a
        .p4k()
        .entries()
        .iter()
        .enumerate()
        .filter(|(_, e)| glob_match(pattern, &e.name))
        .filter_map(|(i, e)| safe_rel(&e.name).map(|rel| ExtractItem { entry: i, rel }))
        .collect();
    if out.is_empty() {
        return Err(AssetError::not_found(format!("nothing matches {pattern}")));
    }
    out.sort_by(|x, y| x.rel.cmp(&y.rel));
    Ok(out)
}
