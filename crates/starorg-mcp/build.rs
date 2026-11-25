//! Code keys of the dataset cache.
//!
//! - `dataset_codes.rs` (in `OUT_DIR`): one key per dataset, a hash of the code that writes
//!   it and nothing else: its domain module, the modules that module uses (followed through
//!   `crate::`/`super::` paths, so `ships` takes `components` and `helpers_ships`), the
//!   shared extraction core (`domain`, `pipeline`, `source`, `output` and what they use,
//!   starorg-data's `extract` and `starbreaker` adapter and what they use, the starorg-core
//!   modules any of that names (`starorg_core::x`) and what they use, the manifests, the
//!   StarBreaker pin lines of the workspace manifest, and this crate's `extract.rs`, the
//!   worker body). The search index takes its own module only (its stamp adds its input
//!   files). `DATASET_MODULES` lists what each key covers. Test modules at the end of a file do not count; the query layer
//!   (`query/`), the golden compare, the fixtures and the overlays are never used while
//!   extracting and do not count either. The store keeps each dataset's key in a stamp
//!   beside it (`store.rs`), so an app update re-extracts only the datasets whose code
//!   changed.
//! - `HINTS_CODE`: starorg-gamedata, starorg-data, this crate and starorg-core in full: the
//!   obtain hints built from the datasets are kept on disk under it (the query layer counts
//!   here).

use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            files(&p, out);
        } else if p
            .extension()
            .is_some_and(|x| x == "rs" || x == "toml" || x == "json")
        {
            out.push(p);
        }
    }
}

/// Same bytes on every checkout: line endings do not count.
fn read(f: &Path) -> String {
    let b = std::fs::read(f).unwrap_or_default();
    String::from_utf8_lossy(&b).replace('\r', "")
}

fn rel(crates: &Path, f: &Path) -> String {
    f.strip_prefix(crates)
        .unwrap_or(f)
        .to_string_lossy()
        .replace('\\', "/")
}

fn feed(h: &mut Sha256, name: &str, text: &str) {
    h.update(name.as_bytes());
    h.update([0]);
    h.update((text.len() as u64).to_le_bytes());
    h.update(text.as_bytes());
}

fn hex(h: Sha256, n: usize) -> String {
    h.finalize()[..n]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Hash of the sources and manifests of `names` (crates beside this one), whole.
fn hash_crates(crates: &Path, names: &[&str]) -> String {
    let mut h = Sha256::new();
    for name in names {
        let root = crates.join(name);
        let mut all = vec![root.join("Cargo.toml")];
        files(&root.join("src"), &mut all);
        all.sort();
        for f in all {
            feed(&mut h, &rel(crates, &f), &read(&f));
        }
    }
    hex(h, 8)
}

/// The file without a test module that ends it (`#[cfg(test)]` then `mod tests {` up to
/// the file's last line). Only cut when nothing but that module follows at column 0, so
/// code after a test module is never dropped.
fn without_tests(text: &str) -> &str {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut offset = 0usize;
    for (i, l) in lines.iter().enumerate() {
        let next = lines.get(i + 1).map(|s| s.trim_end());
        if l.trim_end() == "#[cfg(test)]"
            && next.is_some_and(|n| {
                (n.starts_with("mod ") || n.starts_with("pub(crate) mod ")) && n.ends_with('{')
            })
        {
            let rest = &lines[i + 2..];
            let col0: Vec<&str> = rest
                .iter()
                .map(|s| s.trim_end())
                .filter(|s| !s.is_empty() && !s.starts_with(' ') && !s.starts_with('\t'))
                .collect();
            if col0 == ["}"] {
                return &text[..offset];
            }
        }
        offset += l.len();
    }
    text
}

fn ident_at(s: &str) -> Option<&str> {
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(s.len());
    (end > 0).then(|| &s[..end])
}

/// The index of the `}` closing the `{` at `open`, braces counted (end of text if none).
fn closing(text: &str, open: usize) -> usize {
    let mut depth = 0usize;
    for (k, c) in text[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return open + k;
                }
            }
            _ => {}
        }
    }
    text.len()
}

/// The items of a `{...}` use group (the text between its braces), split at top-level
/// commas, each expanded under `prefix`, nested groups included (`{a::{b, c}, d}`).
fn group(inner: &str, prefix: &[String], out: &mut Vec<Vec<String>>) {
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut items = Vec::new();
    for (k, c) in inner.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                items.push(&inner[start..k]);
                start = k + 1;
            }
            _ => {}
        }
    }
    items.push(&inner[start..]);
    for item in items {
        let mut p = prefix.to_vec();
        let mut rest = item.trim();
        while let Some(id) = ident_at(rest) {
            p.push(id.to_string());
            rest = rest[id.len()..].trim_start();
            match rest.strip_prefix("::") {
                Some(r) => rest = r.trim_start(),
                None => break,
            }
        }
        if rest.starts_with('{') {
            let close = closing(rest, 0).min(rest.len());
            group(&rest[1..close], &p, out);
        } else {
            out.push(p);
        }
    }
}

/// The paths a file names through `crate::` or `super::`, as absolute module paths, `{...}`
/// use groups expanded (nested ones too).
fn paths_named(text: &str, module: &[String]) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if !text.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let rest = &text[i..];
        let boundary = i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
        let (base, mut j) = if boundary && rest.starts_with("crate::") {
            (Vec::new(), i + "crate::".len())
        } else if boundary && rest.starts_with("super::") {
            let mut base: Vec<String> = module.to_vec();
            let mut j = i;
            while text[j..].starts_with("super::") {
                base.pop();
                j += "super::".len();
            }
            (base, j)
        } else {
            i += 1;
            continue;
        };
        // Segments until a `{` group or the end of the path.
        let mut segs = base;
        loop {
            if text[j..].starts_with('{') {
                let close = closing(text, j);
                group(&text[j + 1..close], &segs, &mut out);
                j = close;
                break;
            }
            if let Some(id) = ident_at(&text[j..]) {
                segs.push(id.to_string());
                j += id.len();
                if text[j..].starts_with("::") {
                    j += 2;
                    continue;
                }
            }
            out.push(segs);
            break;
        }
        i = j.max(i + 1);
    }
    out
}

/// Top-level `starorg_core` modules a text names (`starorg_core::fsutil::...`).
fn core_modules(text: &str, out: &mut BTreeSet<String>) {
    let mut at = 0;
    while let Some(k) = text[at..].find("starorg_core::") {
        let s = at + k + "starorg_core::".len();
        if text[s..].starts_with('{') {
            let close = closing(text, s);
            let mut p = Vec::new();
            group(&text[s + 1..close], &[], &mut p);
            out.extend(p.into_iter().filter_map(|x| x.into_iter().next()));
        } else if let Some(id) = ident_at(&text[s..]) {
            out.insert(id.to_string());
        }
        at = s;
    }
}

/// One crate's module graph: a node per top-level module (a file `src/<m>.rs` and its
/// folder `src/<m>/`), and for `domains` one node per domain file (`domains/<d>`).
struct Graph {
    /// node -> files (relative to `crates`) and their text without trailing tests.
    files: BTreeMap<String, Vec<(String, String)>>,
    edges: BTreeMap<String, BTreeSet<String>>,
}

fn graph(crates: &Path, krate: &str, skip: &[&str]) -> Graph {
    let src = crates.join(krate).join("src");
    let mut all = Vec::new();
    files(&src, &mut all);
    all.sort();
    let mut by_node: BTreeMap<String, Vec<(PathBuf, Vec<String>)>> = BTreeMap::new();
    for f in all {
        let r = f.strip_prefix(&src).unwrap_or(&f).to_path_buf();
        let parts: Vec<String> = r
            .iter()
            .map(|p| p.to_string_lossy().trim_end_matches(".rs").to_string())
            .collect();
        if parts.len() == 1 && (parts[0] == "lib" || parts[0] == "main") {
            continue;
        }
        if skip.contains(&parts[0].as_str()) {
            continue;
        }
        let (node, module) = if parts[0] == "domains" && parts.len() == 2 {
            if parts[1] == "mod" {
                // The registry: a lookup by name only.
                continue;
            }
            (format!("domains/{}", parts[1]), parts.clone())
        } else {
            let mut m = parts.clone();
            if m.last().is_some_and(|l| l == "mod") {
                m.pop();
            }
            (parts[0].clone(), m)
        };
        by_node.entry(node).or_default().push((f, module));
    }
    let tops: BTreeSet<String> = by_node
        .keys()
        .map(|n| n.split('/').next().unwrap_or(n).to_string())
        .collect();
    let mut g = Graph {
        files: BTreeMap::new(),
        edges: BTreeMap::new(),
    };
    for (node, list) in &by_node {
        let mut deps = BTreeSet::new();
        let mut texts = Vec::new();
        for (f, module) in list {
            let whole = read(f);
            let text = without_tests(&whole).to_string();
            for p in paths_named(&text, module) {
                let Some(first) = p.first() else { continue };
                if first == "domains" {
                    if let Some(d) = p.get(1)
                        && by_node.contains_key(&format!("domains/{d}"))
                    {
                        deps.insert(format!("domains/{d}"));
                    }
                } else if tops.contains(first) {
                    deps.insert(first.clone());
                }
            }
            texts.push((rel(crates, f), text));
        }
        deps.remove(node);
        g.files.insert(node.clone(), texts);
        g.edges.insert(node.clone(), deps);
    }
    g
}

fn closure(g: &Graph, roots: &[String]) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut todo: Vec<String> = roots.to_vec();
    while let Some(n) = todo.pop() {
        if !g.files.contains_key(&n) || !seen.insert(n.clone()) {
            continue;
        }
        todo.extend(g.edges.get(&n).into_iter().flatten().cloned());
    }
    seen
}

fn feed_nodes(h: &mut Sha256, g: &Graph, nodes: &BTreeSet<String>) {
    for n in nodes {
        for (name, text) in &g.files[n] {
            feed(h, name, text);
        }
    }
}

/// `"game-x"` named by a domain file: its `NAME` constant or the string `fn name` returns.
fn dataset_name(text: &str) -> Option<String> {
    let at = text
        .find("pub const NAME: &str = \"game-")
        .map(|i| i + "pub const NAME: &str = ".len())
        .or_else(|| {
            let f = text.find("fn name(")?;
            text[f..].find("\"game-").map(|i| f + i)
        })?;
    let s = &text[at + 1..];
    s.find('"').map(|e| s[..e].to_string())
}

fn main() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for p in [
        crates.join("starorg-gamedata/src"),
        crates.join("starorg-gamedata/Cargo.toml"),
        crates.join("starorg-data/src"),
        crates.join("starorg-data/Cargo.toml"),
        crates.join("starorg-mcp/src"),
        crates.join("starorg-mcp/Cargo.toml"),
        crates.join("starorg-core/src"),
        crates.join("starorg-core/Cargo.toml"),
        crates.join("../Cargo.toml"),
    ] {
        println!("cargo:rerun-if-changed={}", p.display());
    }

    // Never used while extracting (they read datasets, compare them, or are test helpers).
    let gd = graph(
        &crates,
        "starorg-gamedata",
        &["query", "compare", "fixtures", "overlays", "bin"],
    );
    let data = graph(&crates, "starorg-data", &[]);
    let core_crate = graph(&crates, "starorg-core", &[]);

    // The shared core every dataset takes.
    let mut core = Sha256::new();
    for m in [
        "starorg-gamedata/Cargo.toml",
        "starorg-data/Cargo.toml",
        "starorg-core/Cargo.toml",
    ] {
        feed(&mut core, m, &read(&crates.join(m)));
    }
    let pins: String = read(&crates.join("../Cargo.toml"))
        .lines()
        .filter(|l| l.starts_with("starbreaker-"))
        .map(|l| format!("{l}\n"))
        .collect();
    feed(&mut core, "Cargo.toml#starbreaker", &pins);
    let worker_body = read(&crates.join("starorg-mcp/src/extract.rs"));
    let worker_body = without_tests(&worker_body);
    feed(&mut core, "starorg-mcp/src/extract.rs", worker_body);
    let data_nodes = closure(&data, &["extract".into(), "starbreaker".into()]);
    feed_nodes(&mut core, &data, &data_nodes);
    // The starorg-core modules the extraction path names (the worker body, starorg-data's
    // adapter, any gamedata module), and what they use in turn.
    let mut named = BTreeSet::new();
    core_modules(worker_body, &mut named);
    for n in &data_nodes {
        for (_, t) in &data.files[n] {
            core_modules(t, &mut named);
        }
    }
    for texts in gd.files.values() {
        for (_, t) in texts {
            core_modules(t, &mut named);
        }
    }
    let named: Vec<String> = named.into_iter().collect();
    let core_nodes_used = closure(&core_crate, &named);
    feed_nodes(&mut core, &core_crate, &core_nodes_used);
    let core_nodes: Vec<String> = ["domain", "pipeline", "source", "output"]
        .map(String::from)
        .to_vec();
    let core = hex(core, 16);

    let mut keys: BTreeMap<String, String> = BTreeMap::new();
    let mut modules: BTreeMap<String, String> = BTreeMap::new();
    let mut add = |dataset: String, roots: Vec<String>, with_core: bool| {
        let mut h = Sha256::new();
        if with_core {
            feed(&mut h, "core", &core);
        }
        let nodes = closure(&gd, &roots);
        feed_nodes(&mut h, &gd, &nodes);
        modules.insert(
            dataset.clone(),
            nodes.into_iter().collect::<Vec<_>>().join(" "),
        );
        keys.insert(dataset, hex(h, 8));
    };
    let with = |root: &str| {
        let mut r = core_nodes.clone();
        r.push(root.to_string());
        r
    };
    for (node, texts) in &gd.files {
        if node.starts_with("domains/")
            && let Some(name) = texts.first().and_then(|(_, t)| dataset_name(t))
        {
            add(name, with(node), true);
        }
    }
    add("game-strings-english".into(), with("strings"), true);
    // The derived search index: its own module only; its stamp adds the files it is built
    // from (`stamps::expected_code`).
    add(
        "game-search-index".into(),
        vec!["search_index".into()],
        false,
    );

    let mut out = String::from(
        "/// Code key per dataset (`build.rs`).\npub const DATASET_CODES: &[(&str, &str)] = &[\n",
    );
    for (k, v) in &keys {
        out.push_str(&format!("    ({k:?}, {v:?}),\n"));
    }
    out.push_str("];\n\n/// The starorg-gamedata modules each key covers (besides the shared core).\n#[allow(dead_code)]\npub const DATASET_MODULES: &[(&str, &str)] = &[\n");
    for (k, m) in &modules {
        out.push_str(&format!("    ({k:?}, {m:?}),\n"));
    }
    out.push_str(&format!(
        "];\n\n/// The starorg-core modules in the shared core.\n#[allow(dead_code)]\npub const CORE_MODULES: &str = {:?};\n",
        core_nodes_used.into_iter().collect::<Vec<_>>().join(" ")
    ));
    let dest = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("dataset_codes.rs");
    std::fs::write(dest, out).expect("write dataset_codes.rs");

    println!(
        "cargo:rustc-env=HINTS_CODE={}",
        hash_crates(
            &crates,
            &[
                "starorg-gamedata",
                "starorg-data",
                "starorg-mcp",
                "starorg-core"
            ]
        )
    );
}
