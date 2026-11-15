//! Reader and writer for the game's `global.ini` (spec 4.3, 4.4, 7.2).
//!
//! The stock file is kept as bytes and indexed by line. Rendering streams the stock lines and
//! replaces only the bytes after the first `=` and before the line ending of changed lines.
//! Every other byte (BOM, key spelling with flag suffixes such as `,P`, comments, blank lines,
//! per-line CRLF/LF endings, a missing final newline) is copied as found, so rendering with no
//! edits is byte-identical to the input.

use std::collections::{BTreeMap, HashMap};

/// UTF-8 byte order mark. Without it the game shows every string as `@key`.
pub const BOM: &[u8] = b"\xEF\xBB\xBF";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IniError {
    #[error("key not in stock: {0}")]
    UnknownKey(String),
    #[error("key {key} is ambiguous after case folding (lines {lines:?})")]
    AmbiguousKey { key: String, lines: Vec<usize> },
    #[error("value for {0} contains a raw CR or LF")]
    LineBreakInValue(String),
}

/// One physical line of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Line {
    /// Offset of the first byte of the line (after the BOM for line 0).
    start: usize,
    /// Offset of the first `=`, if the line has one.
    eq: Option<usize>,
    /// Offset where the line ending starts (== `end` when there is none).
    content_end: usize,
    /// Offset just past the line ending.
    end: usize,
}

/// A parsed stock `global.ini`.
#[derive(Debug, Clone)]
pub struct StockIni {
    bytes: Vec<u8>,
    has_bom: bool,
    lines: Vec<Line>,
    /// Folded key (see [`fold_key`]) -> line indices with that key.
    index: HashMap<String, Vec<usize>>,
}

/// Fold a key for matching (spec 4.3): strip a leading `@`, drop a flag suffix such as `,P`,
/// and lowercase.
pub fn fold_key(key: &str) -> String {
    let key = key.strip_prefix('@').unwrap_or(key);
    strip_flag_suffix(key).to_lowercase()
}

/// Drop a trailing flag suffix: a `,` followed by one or more ASCII uppercase letters.
pub fn strip_flag_suffix(key: &str) -> &str {
    if let Some(pos) = key.rfind(',') {
        let flags = &key[pos + 1..];
        if !flags.is_empty() && flags.bytes().all(|b| b.is_ascii_uppercase()) {
            return &key[..pos];
        }
    }
    key
}

impl StockIni {
    /// Index the given bytes. Never fails: lines that are not `key=value` are kept verbatim.
    pub fn parse(bytes: Vec<u8>) -> Self {
        let has_bom = bytes.starts_with(BOM);
        let body_start = if has_bom { BOM.len() } else { 0 };
        let mut lines = Vec::new();
        let mut pos = body_start;
        while pos < bytes.len() {
            let nl = memchr(b'\n', &bytes[pos..]).map(|i| pos + i);
            let (content_end, end) = match nl {
                Some(n) if n > pos && bytes[n - 1] == b'\r' => (n - 1, n + 1),
                Some(n) => (n, n + 1),
                None => (bytes.len(), bytes.len()),
            };
            let eq = memchr(b'=', &bytes[pos..content_end]).map(|i| pos + i);
            lines.push(Line {
                start: pos,
                eq,
                content_end,
                end,
            });
            pos = end;
        }
        let mut ini = Self {
            bytes,
            has_bom,
            lines,
            index: HashMap::new(),
        };
        let mut index: HashMap<String, Vec<usize>> = HashMap::new();
        for i in 0..ini.lines.len() {
            if let Some(key) = ini.key_at(i) {
                index.entry(fold_key(key)).or_default().push(i);
            }
        }
        ini.index = index;
        ini
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn has_bom(&self) -> bool {
        self.has_bom
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Number of `key=value` lines.
    pub fn key_count(&self) -> usize {
        self.lines.iter().filter(|l| self.is_entry(l)).count()
    }

    fn is_entry(&self, l: &Line) -> bool {
        match l.eq {
            Some(eq) => {
                let first = self.bytes[l.start];
                eq > l.start && first != b';' && first != b'#'
            }
            None => false,
        }
    }

    /// Key text of line `i` exactly as in stock (including any flag suffix).
    pub fn key_at(&self, i: usize) -> Option<&str> {
        let l = self.lines.get(i)?;
        if !self.is_entry(l) {
            return None;
        }
        std::str::from_utf8(&self.bytes[l.start..l.eq?]).ok()
    }

    /// Value bytes of line `i` as in stock.
    pub fn value_at(&self, i: usize) -> Option<&str> {
        let l = self.lines.get(i)?;
        if !self.is_entry(l) {
            return None;
        }
        std::str::from_utf8(&self.bytes[l.eq? + 1..l.content_end]).ok()
    }

    /// Iterate `(line index, stock key, stock value)` over all entries.
    pub fn entries(&self) -> impl Iterator<Item = (usize, &str, &str)> {
        (0..self.lines.len()).filter_map(|i| Some((i, self.key_at(i)?, self.value_at(i)?)))
    }

    /// Folded keys shared by more than one line. Those keys are left out of generation and
    /// reported (spec 4.3).
    pub fn collisions(&self) -> Vec<(&str, &[usize])> {
        let mut out: Vec<_> = self
            .index
            .iter()
            .filter(|(_, v)| v.len() > 1)
            .map(|(k, v)| (k.as_str(), v.as_slice()))
            .collect();
        out.sort();
        out
    }

    /// Find the line for a key using the folding rule of spec 4.3.
    pub fn find(&self, key: &str) -> Result<usize, IniError> {
        match self.index.get(&fold_key(key)).map(Vec::as_slice) {
            Some([one]) => Ok(*one),
            Some(many) => Err(IniError::AmbiguousKey {
                key: key.to_string(),
                lines: many.to_vec(),
            }),
            None => Err(IniError::UnknownKey(key.to_string())),
        }
    }

    /// Stock value of a key (folding rule of spec 4.3); `None` when absent or ambiguous.
    pub fn value(&self, key: &str) -> Option<&str> {
        self.value_at(self.find(key).ok()?)
    }

    /// Render the file with the given new values. With no edits the output equals the input
    /// byte for byte. No key is added, removed or reordered.
    pub fn render(&self, edits: &Edits) -> Vec<u8> {
        let extra: usize = edits.values.values().map(String::len).sum();
        let mut out = Vec::with_capacity(self.bytes.len() + extra);
        let body_start = if self.has_bom { BOM.len() } else { 0 };
        out.extend_from_slice(&self.bytes[..body_start]);
        for (i, l) in self.lines.iter().enumerate() {
            match (edits.values.get(&i), l.eq) {
                (Some(value), Some(eq)) => {
                    out.extend_from_slice(&self.bytes[l.start..=eq]);
                    out.extend_from_slice(value.as_bytes());
                    out.extend_from_slice(&self.bytes[l.content_end..l.end]);
                }
                _ => out.extend_from_slice(&self.bytes[l.start..l.end]),
            }
        }
        out
    }
}

/// New values, by stock line. Build with [`Edits::set`] so keys are matched and checked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Edits {
    values: BTreeMap<usize, String>,
}

impl Edits {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Set the value of `key` (matched by [`fold_key`]). A newline in a value must be the
    /// two-character escape `\n`; raw CR or LF is refused.
    pub fn set(
        &mut self,
        stock: &StockIni,
        key: &str,
        value: impl Into<String>,
    ) -> Result<(), IniError> {
        let value = value.into();
        if value.contains(['\r', '\n']) {
            return Err(IniError::LineBreakInValue(key.to_string()));
        }
        let line = stock.find(key)?;
        self.values.insert(line, value);
        Ok(())
    }
}

fn memchr(needle: u8, hay: &[u8]) -> Option<usize> {
    hay.iter().position(|&b| b == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        let mut v = BOM.to_vec();
        v.extend_from_slice(
            b"; comment line\r\n\
              item_NameA=Alpha Gun\r\n\
              item_DescA,P=Desc with = sign and ~mission(Location) %s\r\n\
              \r\n\
              Vehicle_NameB=  spaced value  \r\n\
              lf_only=lf\n\
              empty_value=\r\n\
              last_no_newline=end",
        );
        v
    }

    #[test]
    fn round_trip_is_byte_identical() {
        let bytes = sample();
        let ini = StockIni::parse(bytes.clone());
        assert!(ini.has_bom());
        assert_eq!(ini.render(&Edits::new()), bytes);
        assert_eq!(ini.line_count(), 8);
        assert_eq!(ini.key_count(), 6);
    }

    #[test]
    fn round_trip_without_bom_and_with_trailing_newline() {
        let bytes = b"a=1\r\nb=2\r\n".to_vec();
        let ini = StockIni::parse(bytes.clone());
        assert!(!ini.has_bom());
        assert_eq!(ini.render(&Edits::new()), bytes);
        assert_eq!(ini.line_count(), 2);
    }

    #[test]
    fn empty_and_bom_only_files() {
        for bytes in [Vec::new(), BOM.to_vec()] {
            let ini = StockIni::parse(bytes.clone());
            assert_eq!(ini.render(&Edits::new()), bytes);
            assert_eq!(ini.key_count(), 0);
        }
    }

    #[test]
    fn keys_and_values_are_split_on_first_equals() {
        let ini = StockIni::parse(sample());
        let entries: Vec<_> = ini.entries().map(|(_, k, v)| (k, v)).collect();
        assert_eq!(
            entries[1],
            ("item_DescA,P", "Desc with = sign and ~mission(Location) %s")
        );
        assert_eq!(entries[2], ("Vehicle_NameB", "  spaced value  "));
        assert_eq!(entries[4], ("empty_value", ""));
        assert_eq!(entries[5], ("last_no_newline", "end"));
    }

    #[test]
    fn edit_replaces_only_value_and_keeps_flag_suffix_and_ending() {
        let ini = StockIni::parse(sample());
        let mut edits = Edits::new();
        // User writes the key without its flag suffix and in another case.
        edits.set(&ini, "ITEM_DESCA", "new text").unwrap();
        edits.set(&ini, "lf_only", "changed").unwrap();
        edits.set(&ini, "last_no_newline", "fin").unwrap();
        let out = ini.render(&edits);
        let text = std::str::from_utf8(&out[3..]).unwrap();
        assert!(text.contains("\r\nitem_DescA,P=new text\r\n"));
        assert!(text.contains("\r\nlf_only=changed\n"));
        assert!(text.ends_with("\r\nlast_no_newline=fin"));
        assert!(out.starts_with(BOM));
        // Same number of lines, same keys in the same order.
        let again = StockIni::parse(out);
        assert_eq!(again.line_count(), ini.line_count());
        let k1: Vec<_> = ini.entries().map(|(_, k, _)| k.to_string()).collect();
        let k2: Vec<_> = again.entries().map(|(_, k, _)| k.to_string()).collect();
        assert_eq!(k1, k2);
    }

    #[test]
    fn unknown_key_and_line_breaks_are_refused() {
        let ini = StockIni::parse(sample());
        let mut edits = Edits::new();
        assert_eq!(
            edits.set(&ini, "nope", "x"),
            Err(IniError::UnknownKey("nope".into()))
        );
        assert_eq!(
            edits.set(&ini, "item_NameA", "a\nb"),
            Err(IniError::LineBreakInValue("item_NameA".into()))
        );
        assert!(edits.is_empty());
        // Comments are not keys even when they contain '='.
        let ini = StockIni::parse(b";x=1\r\nx=2\r\n".to_vec());
        assert_eq!(ini.key_count(), 1);
    }

    #[test]
    fn folding_rules() {
        assert_eq!(fold_key("@Item_NameA"), "item_namea");
        assert_eq!(fold_key("key,P"), "key");
        assert_eq!(fold_key("key,PX"), "key");
        assert_eq!(fold_key("key,p"), "key,p");
        assert_eq!(fold_key("a,b,P"), "a,b");
        assert_eq!(strip_flag_suffix("key,"), "key,");
    }

    #[test]
    fn collisions_are_reported_and_ambiguous_lookup_fails() {
        let ini = StockIni::parse(b"Key=1\r\nkey,P=2\r\nother=3\r\n".to_vec());
        assert_eq!(ini.collisions(), vec![("key", &[0usize, 1][..])]);
        assert!(matches!(
            ini.find("KEY"),
            Err(IniError::AmbiguousKey { .. })
        ));
        assert_eq!(ini.find("OTHER"), Ok(2));
    }
}
