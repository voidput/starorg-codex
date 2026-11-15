//! Merge: `stock < generated < loc patches < user overrides`, with per-value validation
//! (spec 4.3, 7.3).
//!
//! A value that fails a per-value rule is dropped, the next lower layer is tried
//! (override -> loc patch -> generated -> stock), and a warning names the key and the rule.
//! Provenance is kept per key so reports can say why a string changed.

use crate::build::BuildInfo;
use crate::ini::{Edits, StockIni};
use crate::model::GameData;
use crate::patch::{self, EntryOutcome, PatchSet, Snapshot};
use crate::source::Warning;
use crate::state::Override;
use crate::validate::{self, Op, WARN_LEN};
use serde::Serialize;
use std::collections::BTreeMap;

/// An edit from a generator, kept in parts until render time (spec 4.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenEdit {
    pub key: String,
    pub generator: String,
    pub prefix: String,
    pub suffix: String,
    /// Replaces the whole value instead of prefix/suffix.
    pub whole: Option<String>,
    /// The stock part loses its trailing `\n` escapes and spaces (spec 5.1, 6.2).
    pub trim_stock: bool,
}

impl From<&crate::enhance::GenEdit> for GenEdit {
    fn from(e: &crate::enhance::GenEdit) -> Self {
        Self {
            key: e.key.clone(),
            generator: e.category.name().to_string(),
            // A rename is a whole value here: merge needs the final value and the generated
            // part, which are the same for it.
            prefix: if e.replace.is_some() {
                String::new()
            } else {
                e.prefix.clone()
            },
            suffix: if e.replace.is_some() {
                String::new()
            } else {
                e.suffix.clone()
            },
            whole: e.replace.as_ref().map(|_| e.value("")),
            trim_stock: e.trim_stock,
        }
    }
}

/// Where a key's final value came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "layer", rename_all = "kebab-case")]
pub enum Source {
    Generated { generator: String },
    LocPatch { patch: String },
    Override,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Counts {
    pub changed: usize,
    pub generated: usize,
    pub loc_patches: usize,
    pub overrides: usize,
    pub dropped: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Merged {
    pub edits: Edits,
    pub provenance: BTreeMap<usize, Source>,
    pub counts: Counts,
    pub warnings: Vec<Warning>,
    pub patch_outcomes: Vec<EntryOutcome>,
}

struct Snap<'a> {
    stock: &'a StockIni,
    generated: &'a BTreeMap<usize, (String, String)>,
}

impl Snapshot for Snap<'_> {
    fn stock(&self) -> &StockIni {
        self.stock
    }
    fn generated_part(&self, line: usize) -> String {
        self.generated
            .get(&line)
            .map(|(_, part)| part.clone())
            .unwrap_or_default()
    }
    fn final_value(&self, line: usize) -> String {
        match self.generated.get(&line) {
            Some((v, _)) => v.clone(),
            None => self.stock.value_at(line).unwrap_or_default().to_string(),
        }
    }
}

fn drop_warning(key: &str, layer: &str, rule: &validate::Rule) -> Warning {
    Warning::new(
        "dropped-edit",
        format!("{key}: {layer} value dropped ({})", rule.name()),
    )
}

/// Build the merged edit set.
pub fn merge(
    stock: &StockIni,
    game_data: &GameData,
    build: Option<&BuildInfo>,
    generated: &[GenEdit],
    patches: &PatchSet,
    overrides: &[Override],
) -> Merged {
    let mut m = Merged::default();

    // 1. Generated, validated first so loc patches read only values that survive.
    let mut gen_values: BTreeMap<usize, (String, String)> = BTreeMap::new();
    let mut gen_src: BTreeMap<usize, String> = BTreeMap::new();
    for g in generated {
        let Ok(line) = stock.find(&g.key) else {
            continue;
        };
        let sv = stock.value_at(line).unwrap_or_default();
        // Parts are checked with the generators' own rules (enhance::check, spec 7.3), so
        // `apply` keeps exactly what `build` keeps (including CIG's open-tail values);
        // a whole replacement goes through the general value rules.
        let checked: Result<(String, String), String> = match &g.whole {
            Some(w) => validate::check_value(sv, w, Op::GeneratedWhole, &[w.as_str()])
                .map(|()| (w.clone(), w.clone()))
                .map_err(|r| r.name().to_string()),
            None => {
                let stock_part = if g.trim_stock {
                    crate::enhance::trim_tail(sv)
                } else {
                    sv
                };
                crate::enhance::check::check_generated(sv, stock_part, &g.prefix, &g.suffix)
                    .map(|()| {
                        (
                            format!("{}{stock_part}{}", g.prefix, g.suffix),
                            format!("{}{}", g.prefix, g.suffix),
                        )
                    })
                    .map_err(|r| r.name().to_string())
            }
        };
        match checked {
            Ok((value, part)) => {
                gen_values.insert(line, (value, part));
                gen_src.insert(line, g.generator.clone());
            }
            Err(rule) => {
                m.counts.dropped += 1;
                m.warnings.push(Warning::new(
                    "dropped-edit",
                    format!("{}: generated value dropped ({rule})", g.key),
                ));
            }
        }
    }

    // 2. Loc patches from one snapshot.
    let snap = Snap {
        stock,
        generated: &gen_values,
    };
    let (loc_edits, outcomes, notes) = patch::run(patches, game_data, &snap, build);
    for n in notes {
        m.warnings.push(Warning::new("patch-range", n));
    }
    for e in &patches.errors {
        m.warnings.push(Warning::new(
            "patch-file",
            format!("{}: {}", e.name, e.error),
        ));
    }
    for o in &outcomes {
        if o.state.warns() {
            m.warnings.push(Warning::new(
                "patch",
                format!("{} {}: {:?} {}", o.patch, o.entry, o.state, o.detail),
            ));
        }
    }
    m.patch_outcomes = outcomes;
    let mut loc_values: BTreeMap<usize, (String, String)> = BTreeMap::new();
    for e in loc_edits {
        let key = stock.key_at(e.line).unwrap_or_default();
        let sv = stock.value_at(e.line).unwrap_or_default();
        let added: Vec<&str> = e.added.iter().map(String::as_str).collect();
        match validate::check_value(sv, &e.value, e.op, &added) {
            Ok(()) => {
                loc_values.insert(e.line, (e.value, e.patch));
            }
            Err(rule) => {
                m.counts.dropped += 1;
                m.warnings
                    .push(drop_warning(key, &format!("patch {}", e.patch), &rule));
            }
        }
    }

    // 3. Overrides replace the whole value.
    let mut ovr_values: BTreeMap<usize, String> = BTreeMap::new();
    for o in overrides {
        let line = match stock.find(&o.key) {
            Ok(l) => l,
            Err(e) => {
                m.warnings.push(Warning::new(
                    "override-ignored",
                    format!("overrides.ini line {}: {e}", o.line),
                ));
                continue;
            }
        };
        let sv = stock.value_at(line).unwrap_or_default();
        match validate::check_value(sv, &o.value, Op::Override, &[]) {
            Ok(()) => {
                ovr_values.insert(line, o.value.clone());
            }
            Err(rule) => {
                m.counts.dropped += 1;
                m.warnings.push(drop_warning(&o.key, "override", &rule));
            }
        }
    }

    // 4. Highest surviving layer wins.
    let lines: std::collections::BTreeSet<usize> = gen_values
        .keys()
        .chain(loc_values.keys())
        .chain(ovr_values.keys())
        .copied()
        .collect();
    for line in lines {
        let key = stock.key_at(line).unwrap_or_default().to_string();
        let sv = stock.value_at(line).unwrap_or_default();
        let (value, source) = if let Some(v) = ovr_values.remove(&line) {
            m.counts.overrides += 1;
            (v, Source::Override)
        } else if let Some((v, patch)) = loc_values.remove(&line) {
            m.counts.loc_patches += 1;
            (v, Source::LocPatch { patch })
        } else if let Some((v, _)) = gen_values.remove(&line) {
            m.counts.generated += 1;
            (
                v,
                Source::Generated {
                    generator: gen_src.remove(&line).unwrap_or_default(),
                },
            )
        } else {
            continue;
        };
        if value == sv {
            continue;
        }
        if value.len() > WARN_LEN {
            m.warnings.push(Warning::new(
                "long-value",
                format!("{key}: {} bytes (game limit unknown, Q9)", value.len()),
            ));
        }
        // Validation refused raw CR/LF already; this cannot fail.
        let _ = m.edits.set_line(line, &key, value);
        m.provenance.insert(line, source);
    }
    m.counts.changed = m.edits.len();
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::{LoadedPatch, PatchOrigin, parse_patch};

    fn stock() -> StockIni {
        StockIni::parse(
            b"\xEF\xBB\xBFa=Alpha\r\nb,P=Beta ~mission(Location)\r\nc=Gamma\r\nf=%i dmg\r\n"
                .to_vec(),
        )
    }

    #[test]
    fn layers_and_fallback() {
        let s = stock();
        let generated = vec![
            GenEdit {
                key: "a".into(),
                generator: "names".into(),
                prefix: "[S] ".into(),
                suffix: String::new(),
                whole: None,
                trim_stock: false,
            },
            GenEdit {
                key: "f".into(),
                generator: "stats".into(),
                prefix: String::new(),
                suffix: " x".into(),
                whole: None,
                trim_stock: false,
            },
        ];
        let text = "schema = 1\nid = \"p\"\ntitle = \"t\"\n[[loc]]\nkey = \"a\"\nop = \"append\"\nvalue = \" <EM4>bad\"\n";
        let set = PatchSet {
            patches: vec![LoadedPatch {
                file: parse_patch("p", text).unwrap(),
                origin: PatchOrigin::Builtin,
                sha256: String::new(),
                enabled: true,
            }],
            errors: vec![],
        };
        let overrides = vec![
            Override {
                line: 1,
                key: "B".into(),
                value: "Mine ~mission(Location)".into(),
            },
            Override {
                line: 2,
                key: "c".into(),
                value: "lost ~var(x)".into(),
            },
            Override {
                line: 3,
                key: "zzz".into(),
                value: "x".into(),
            },
        ];
        let m = merge(&s, &GameData::default(), None, &generated, &set, &overrides);
        let out = s.render(&m.edits);
        let text = String::from_utf8(out).unwrap();
        // Generated survived; the loc patch was dropped (unbalanced added tag) -> generated.
        assert!(
            text.contains("\r\na=[S] Alpha\r\n") || text.contains("\u{feff}a=[S] Alpha\r\n"),
            "{text}"
        );
        // Override kept the flag suffix spelling of stock.
        assert!(text.contains("b,P=Mine ~mission(Location)\r\n"));
        // Bad override dropped -> stock.
        assert!(text.contains("c=Gamma\r\n"));
        // Format key: no generated text.
        assert!(text.contains("f=%i dmg\r\n"));
        assert_eq!(m.counts.changed, 2);
        assert_eq!(m.counts.dropped, 3);
        assert!(m.warnings.iter().any(|w| w.kind == "override-ignored"));
        assert_eq!(m.provenance[&1], Source::Override);
    }

    #[test]
    fn trim_stock_drops_the_stock_tail() {
        let s = StockIni::parse(b"\xEF\xBB\xBFd=Prose\\n\\n  \r\n".to_vec());
        let g = [GenEdit {
            key: "d".into(),
            generator: "ships".into(),
            prefix: String::new(),
            suffix: "\\n\\nStats".into(),
            whole: None,
            trim_stock: true,
        }];
        let m = merge(
            &s,
            &GameData::default(),
            None,
            &g,
            &PatchSet::default(),
            &[],
        );
        let out = String::from_utf8(s.render(&m.edits)).unwrap();
        assert!(out.ends_with("d=Prose\\n\\nStats\r\n"), "{out:?}");
    }
}
