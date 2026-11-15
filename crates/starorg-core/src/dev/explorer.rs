//! The DataCore explorer index (addendum 7.4): one [`Entry`] per record, the type and path
//! trees, search and lookups. Pure: the worker builds it from StarBreaker's database, the
//! mock from synthetic records; both answer the UI through these functions.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Records below this folder are shown relative to it (`entities/...`).
pub const RECORDS_ROOT: &str = "libs/foundry/records/";
/// Most tree rows, search hits or table rows in one reply (7.3).
pub const MAX_PAGE: usize = 500;
/// History entries kept by the UI (7.4).
pub const HISTORY: usize = 50;

/// One DataCore record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`, lower case.
    pub guid: String,
    pub name: String,
    #[serde(rename = "type")]
    pub struct_type: String,
    /// The record's file name as DataCore stores it (`libs/foundry/records/....xml`).
    pub path: String,
    /// The last record of its file (StarBreaker's "main" record).
    pub main: bool,
    /// DCB v8 record tag, when set.
    pub tag: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TreeRoot {
    Type,
    Path,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NodeKind {
    Type,
    Folder,
    Record,
}

/// One row of a tree level. Ids: `t:<type>`, `p:<folder>/`, `r:<guid>`; the root is `""`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeNode {
    pub id: String,
    pub label: String,
    pub kind: NodeKind,
    /// Records below (types, folders); 0 for a record.
    pub count: u32,
    /// Records: main record or sub-record.
    pub main: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreePage {
    pub nodes: Vec<TreeNode>,
    pub offset: u32,
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    pub guid: String,
    pub name: String,
    #[serde(rename = "type")]
    pub struct_type: String,
    pub path: String,
    pub main: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchPage {
    pub hits: Vec<SearchHit>,
    pub total: u32,
}

/// `dev types`: one row per struct type that has records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeCount {
    pub name: String,
    pub records: u32,
    pub main: u32,
}

/// The index over every record. Built once per worker start (about 70 ms on LIVE).
#[derive(Debug, Default)]
pub struct ExplorerIndex {
    entries: Vec<Entry>,
    lname: Vec<String>,
    /// Lower-case path below [`RECORDS_ROOT`] (or the whole path when outside it).
    lrel: Vec<String>,
    by_guid: HashMap<String, u32>,
    /// Type -> records sorted by lower-case name.
    by_type: BTreeMap<String, Vec<u32>>,
    /// Every record, sorted by (lower-case relative path, name).
    by_path: Vec<u32>,
    /// Every record by (lower-case name, main first, position): exact-name lookups.
    by_lname: Vec<u32>,
    /// The same by lower-case short name (after the `Type.` prefix).
    by_short: Vec<u32>,
}

/// `path` below [`RECORDS_ROOT`], without the root, original case.
pub fn relative_path(path: &str) -> &str {
    let p = path.trim_start_matches(['/', '\\']);
    match p.get(..RECORDS_ROOT.len()) {
        Some(head) if head.eq_ignore_ascii_case(RECORDS_ROOT) => &p[RECORDS_ROOT.len()..],
        _ => p,
    }
}

fn norm(s: &str) -> String {
    s.replace('\\', "/").to_lowercase()
}

/// A GUID or the start of one: hex digits and dashes, at least 4 characters.
pub fn guid_like(s: &str) -> bool {
    s.len() >= 4 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Full GUID shape (`8-4-4-4-12`).
pub fn is_guid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && parts
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(p, n)| p.len() == n && p.chars().all(|c| c.is_ascii_hexdigit()))
}

/// A record name without its `Type.` prefix.
fn short_of(name: &str) -> &str {
    name.split_once('.').map_or(name, |(_, s)| s)
}

/// `needle` (no spaces) appears in `hay` in order.
fn subsequence(needle: &str, hay: &str) -> bool {
    let mut it = hay.chars();
    needle.chars().all(|c| it.any(|h| h == c))
}

impl ExplorerIndex {
    pub fn new(entries: Vec<Entry>) -> Self {
        let lname: Vec<String> = entries.iter().map(|e| e.name.to_lowercase()).collect();
        let lrel: Vec<String> = entries
            .iter()
            .map(|e| norm(relative_path(&e.path)))
            .collect();
        let mut by_guid = HashMap::with_capacity(entries.len());
        let mut by_type: BTreeMap<String, Vec<u32>> = BTreeMap::new();
        for (i, e) in entries.iter().enumerate() {
            by_guid.insert(e.guid.to_lowercase(), i as u32);
            by_type
                .entry(e.struct_type.clone())
                .or_default()
                .push(i as u32);
        }
        for v in by_type.values_mut() {
            v.sort_by(|&a, &b| lname[a as usize].cmp(&lname[b as usize]).then(a.cmp(&b)));
        }
        let mut by_path: Vec<u32> = (0..entries.len() as u32).collect();
        by_path.sort_by(|&a, &b| {
            lrel[a as usize]
                .cmp(&lrel[b as usize])
                .then(lname[a as usize].cmp(&lname[b as usize]))
                .then(a.cmp(&b))
        });
        fn ordered<'k>(entries: &[Entry], key: impl Fn(usize) -> &'k str) -> Vec<u32> {
            let mut v: Vec<u32> = (0..entries.len() as u32).collect();
            v.sort_by(|&a, &b| {
                let (a, b) = (a as usize, b as usize);
                key(a)
                    .cmp(key(b))
                    .then(entries[b].main.cmp(&entries[a].main))
                    .then(a.cmp(&b))
            });
            v
        }
        let by_lname = ordered(&entries, |i| lname[i].as_str());
        let by_short = ordered(&entries, |i| short_of(&lname[i]));
        Self {
            entries,
            lname,
            lrel,
            by_guid,
            by_type,
            by_path,
            by_lname,
            by_short,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn entry(&self, i: usize) -> Option<&Entry> {
        self.entries.get(i)
    }

    pub fn main_count(&self) -> usize {
        self.entries.iter().filter(|e| e.main).count()
    }

    /// Struct types with records (sorted by name).
    pub fn types(&self) -> Vec<TypeCount> {
        self.by_type
            .iter()
            .map(|(name, v)| TypeCount {
                name: name.clone(),
                records: v.len() as u32,
                main: v.iter().filter(|&&i| self.entries[i as usize].main).count() as u32,
            })
            .collect()
    }

    /// Records of one type, in name order.
    pub fn of_type(&self, name: &str, main_only: bool) -> Vec<usize> {
        self.by_type
            .get(name)
            .map(|v| {
                v.iter()
                    .map(|&i| i as usize)
                    .filter(|&i| !main_only || self.entries[i].main)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A record of exactly this name (case-insensitive), main records first.
    pub fn find_name(&self, name: &str) -> Option<usize> {
        let ln = name.trim().to_lowercase();
        if ln.is_empty() {
            return None;
        }
        self.first_of(&self.by_lname, &ln, |i| self.lname[i].as_str())
    }

    /// A record of exactly this name or short name (`AEGS_Gladius` for
    /// `EntityClassDefinition.AEGS_Gladius`), case-insensitive; full names first, then
    /// main records.
    pub fn find_name_or_short(&self, name: &str) -> Option<usize> {
        let ln = name.trim().to_lowercase();
        if ln.is_empty() {
            return None;
        }
        self.first_of(&self.by_lname, &ln, |i| self.lname[i].as_str())
            .or_else(|| self.first_of(&self.by_short, &ln, |i| short_of(&self.lname[i])))
    }

    /// The first entry of `order` (sorted by `key`, main first) whose key is `ln`.
    fn first_of<'s>(
        &'s self,
        order: &[u32],
        ln: &str,
        key: impl Fn(usize) -> &'s str,
    ) -> Option<usize> {
        let at = order.partition_point(|&i| key(i as usize) < ln);
        order.get(at).map(|&i| i as usize).filter(|&i| key(i) == ln)
    }

    pub fn by_guid(&self, guid: &str) -> Option<usize> {
        self.by_guid
            .get(&guid.trim().to_lowercase())
            .map(|&i| i as usize)
    }

    /// A GUID, a record path (full or below the records root, with or without extension), or a
    /// record name. Main records win over sub-records of the same path or name.
    pub fn find(&self, what: &str) -> Option<usize> {
        let w = what.trim();
        if w.is_empty() {
            return None;
        }
        if let Some(i) = self.by_guid(w) {
            return Some(i);
        }
        let lw = norm(relative_path(w));
        let strip = |s: &str| -> String {
            s.strip_suffix(".xml")
                .or_else(|| s.strip_suffix(".json"))
                .unwrap_or(s)
                .to_string()
        };
        let lw_stem = strip(&lw);
        let best = |pred: &dyn Fn(usize) -> bool| -> Option<usize> {
            let mut hit = None;
            for i in 0..self.entries.len() {
                if pred(i) {
                    if self.entries[i].main {
                        return Some(i);
                    }
                    hit.get_or_insert(i);
                }
            }
            hit
        };
        if lw.contains('/')
            && let Some(i) = best(&|i| strip(&self.lrel[i]) == lw_stem)
        {
            return Some(i);
        }
        self.find_name(w)
    }

    fn record_node(&self, i: usize) -> TreeNode {
        let e = &self.entries[i];
        TreeNode {
            id: format!("r:{}", e.guid),
            label: e.name.clone(),
            kind: NodeKind::Record,
            count: 0,
            main: e.main,
        }
    }

    /// Children of `node` (`""` = root), `offset`/`limit` paged (limit capped at
    /// [`MAX_PAGE`]). Unknown ids give an empty page.
    pub fn tree(
        &self,
        root: TreeRoot,
        node: &str,
        offset: usize,
        limit: usize,
        main_only: bool,
    ) -> TreePage {
        let all = self.children(root, node, main_only);
        let limit = limit.min(MAX_PAGE);
        let total = all.len();
        let nodes = all.into_iter().skip(offset).take(limit).collect();
        TreePage {
            nodes,
            offset: offset as u32,
            total: total as u32,
        }
    }

    fn children(&self, root: TreeRoot, node: &str, main_only: bool) -> Vec<TreeNode> {
        match root {
            TreeRoot::Type if node.is_empty() => self
                .types()
                .into_iter()
                .map(|t| (t.name, if main_only { t.main } else { t.records }))
                .filter(|(_, n)| *n > 0)
                .map(|(name, count)| TreeNode {
                    id: format!("t:{name}"),
                    label: name,
                    kind: NodeKind::Type,
                    count,
                    main: false,
                })
                .collect(),
            TreeRoot::Type => match node.strip_prefix("t:") {
                Some(t) => self
                    .of_type(t, main_only)
                    .into_iter()
                    .map(|i| self.record_node(i))
                    .collect(),
                None => vec![],
            },
            TreeRoot::Path => {
                let prefix = if node.is_empty() {
                    String::new()
                } else if let Some(p) = node.strip_prefix("p:") {
                    p.to_lowercase()
                } else {
                    return vec![];
                };
                self.folder_children(&prefix, main_only)
            }
        }
    }

    /// Sub-folders (by name, with record counts) then records directly in the folder.
    fn folder_children(&self, prefix: &str, main_only: bool) -> Vec<TreeNode> {
        let start = self
            .by_path
            .partition_point(|&i| self.lrel[i as usize].as_str() < prefix);
        let mut folders: Vec<(String, String, u32)> = Vec::new(); // (lower, label, count)
        let mut records = Vec::new();
        for &i in &self.by_path[start..] {
            let i = i as usize;
            let l = &self.lrel[i];
            if !l.starts_with(prefix) {
                break;
            }
            if main_only && !self.entries[i].main {
                continue;
            }
            let rest = &l[prefix.len()..];
            match rest.find('/') {
                Some(cut) => {
                    let seg = &rest[..cut];
                    match folders.last_mut() {
                        Some(f) if f.0 == seg => f.2 += 1,
                        _ => {
                            // Original case from the record's own path.
                            let orig = relative_path(&self.entries[i].path).replace('\\', "/");
                            let label = orig
                                .get(prefix.len()..prefix.len() + cut)
                                .unwrap_or(seg)
                                .to_string();
                            folders.push((seg.to_string(), label, 1));
                        }
                    }
                }
                None => records.push(i),
            }
        }
        records.sort_by(|&a, &b| self.lname[a].cmp(&self.lname[b]).then(a.cmp(&b)));
        folders
            .into_iter()
            .map(|(lower, label, count)| TreeNode {
                id: format!("p:{prefix}{lower}/"),
                label,
                kind: NodeKind::Folder,
                count,
                main: false,
            })
            .chain(records.into_iter().map(|i| self.record_node(i)))
            .collect()
    }

    /// Server-side search (7.4). `type:<Name>` filters by struct type; a GUID or the start of
    /// one matches by prefix; otherwise every token must appear in the name or the path.
    /// Rank: exact name > name prefix > name substring > path substring > in-order
    /// subsequence of the name; then main records first, shorter names, name order.
    pub fn search(&self, query: &str, limit: usize) -> SearchPage {
        let mut type_filter: Option<String> = None;
        let mut tokens = Vec::new();
        for t in query.split_whitespace() {
            match t.get(..5) {
                Some(h) if h.eq_ignore_ascii_case("type:") => {
                    type_filter = Some(t[5..].to_lowercase())
                }
                _ => tokens.push(t.to_lowercase()),
            }
        }
        let joined = tokens.join(" ");
        let compact: String = tokens.concat();
        let guid_q = (tokens.len() == 1 && guid_like(&tokens[0])).then(|| tokens[0].clone());
        let type_ok = |e: &Entry| {
            type_filter
                .as_deref()
                .is_none_or(|t| t.is_empty() || e.struct_type.to_lowercase() == *t)
        };
        let mut ranked: Vec<(u8, usize)> = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            if !type_ok(e) {
                continue;
            }
            if tokens.is_empty() {
                if type_filter.is_some() {
                    ranked.push((5, i));
                }
                continue;
            }
            let name = &self.lname[i];
            if let Some(g) = &guid_q
                && e.guid.starts_with(g.as_str())
            {
                ranked.push((0, i));
                continue;
            }
            let rank = if *name == joined {
                1
            } else if name.starts_with(&joined) {
                2
            } else if name.contains(&joined) {
                3
            } else if tokens
                .iter()
                .all(|t| name.contains(t.as_str()) || self.lrel[i].contains(t.as_str()))
            {
                4
            } else if compact.len() >= 3 && subsequence(&compact, name) {
                5
            } else {
                continue;
            };
            ranked.push((rank, i));
        }
        let total = ranked.len();
        ranked.sort_by(|a, b| {
            let (ea, eb) = (&self.entries[a.1], &self.entries[b.1]);
            a.0.cmp(&b.0)
                .then(eb.main.cmp(&ea.main))
                .then(self.lname[a.1].len().cmp(&self.lname[b.1].len()))
                .then(self.lname[a.1].cmp(&self.lname[b.1]))
                .then(a.1.cmp(&b.1))
        });
        let hits = ranked
            .into_iter()
            .take(limit.min(MAX_PAGE))
            .map(|(_, i)| {
                let e = &self.entries[i];
                SearchHit {
                    guid: e.guid.clone(),
                    name: e.name.clone(),
                    struct_type: e.struct_type.clone(),
                    path: e.path.clone(),
                    main: e.main,
                }
            })
            .collect();
        SearchPage {
            hits,
            total: total as u32,
        }
    }
}
