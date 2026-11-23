//! `DATASET_CODE`: a hash of the code that writes the datasets (starorg-gamedata and
//! starorg-data sources, StarBreaker pin included). The dataset cache keys on it instead of
//! the app version, so a release that leaves extraction alone keeps the cached datasets,
//! and any change to that code extracts again.

use sha2::{Digest, Sha256};
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

fn main() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut h = Sha256::new();
    for name in ["starorg-gamedata", "starorg-data"] {
        let root = crates.join(name);
        for sub in ["src", "Cargo.toml"] {
            println!("cargo:rerun-if-changed={}", root.join(sub).display());
        }
        let mut all = vec![root.join("Cargo.toml")];
        files(&root.join("src"), &mut all);
        all.sort();
        for f in all {
            let rel = f
                .strip_prefix(&crates)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            // Same hash on every checkout: line endings do not count.
            let text = std::fs::read(&f).unwrap_or_default();
            let text: Vec<u8> = text.into_iter().filter(|&b| b != b'\r').collect();
            h.update(rel.as_bytes());
            h.update([0]);
            h.update((text.len() as u64).to_le_bytes());
            h.update(&text);
        }
    }
    let hex: String = h.finalize()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    println!("cargo:rustc-env=DATASET_CODE={hex}");
}
