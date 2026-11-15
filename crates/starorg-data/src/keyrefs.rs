//! The loc-reference pass (addendum 8.2): which main records hold which `@key` strings.
//!
//! [`RecordScan`] is fed by a StarBreaker export sink (in [`crate::starbreaker`]) while the
//! walker visits one record: it keeps only `@`-strings and the few `AttachDef` fields the
//! grouping needs, so no JSON is built. [`collect`] turns the per-record scans into the
//! core's [`KeyRefs`].

use starorg_core::groups::{KeyRefs, KeyRefsBuilder, RefSource};
use starorg_core::ini::StockIni;

/// Path segments kept in [`RefSource::folder`].
const FOLDER_SEGMENTS: usize = 3;

/// The state of one record walk.
#[derive(Debug, Default)]
pub struct RecordScan {
    /// Names of the open objects and arrays (`None` for array elements).
    stack: Vec<Option<String>>,
    pub keys: Vec<String>,
    pub item_type: Option<String>,
    pub sub_type: Option<String>,
    pub size: Option<i64>,
    pub manufacturer: Option<String>,
}

impl RecordScan {
    fn in_attach_def(&self) -> bool {
        self.stack
            .last()
            .is_some_and(|n| n.as_deref() == Some("AttachDef"))
    }

    pub fn begin(&mut self, name: Option<&str>) {
        self.stack.push(name.map(str::to_string));
    }

    pub fn end(&mut self) {
        self.stack.pop();
    }

    pub fn string(&mut self, name: Option<&str>, value: &str) {
        if value.len() > 1 && value.starts_with('@') {
            self.keys.push(value.to_string());
        }
        if self.in_attach_def() {
            match name {
                Some("Type") if self.item_type.is_none() => self.item_type = Some(value.into()),
                Some("SubType") if self.sub_type.is_none() => self.sub_type = Some(value.into()),
                _ => {}
            }
        }
    }

    pub fn int(&mut self, name: Option<&str>, value: i64) {
        if name == Some("Size") && self.in_attach_def() && self.size.is_none() {
            self.size = Some(value);
        }
    }

    /// A reference to another main record, by its file path.
    pub fn record_ref(&mut self, name: Option<&str>, path: &str) {
        // Only references into the manufacturer records (a few `Manufacturer` fields point at
        // logo decals and paints instead).
        if name == Some("Manufacturer")
            && self.in_attach_def()
            && self.manufacturer.is_none()
            && path.to_ascii_lowercase().contains("scitemmanufacturer")
        {
            let file = path.rsplit(['/', '\\']).next().unwrap_or_default();
            let stem = file
                .strip_suffix(".json")
                .or_else(|| file.strip_suffix(".xml"))
                .unwrap_or(file);
            let code = stem.rsplit('.').next().unwrap_or_default();
            if !code.is_empty() {
                self.manufacturer = Some(code.to_ascii_uppercase());
            }
        }
    }
}

/// One scanned main record.
#[derive(Debug, Clone)]
pub struct Scanned {
    pub struct_type: String,
    /// Normalised record path (see [`crate::gamedata::json::record_path`]).
    pub path: String,
    pub scan: ScanResult,
}

/// The parts of a [`RecordScan`] that survive the walk.
#[derive(Debug, Clone, Default)]
pub struct ScanResult {
    pub keys: Vec<String>,
    pub item_type: Option<String>,
    pub sub_type: Option<String>,
    pub size: Option<i64>,
    pub manufacturer: Option<String>,
}

impl From<RecordScan> for ScanResult {
    fn from(s: RecordScan) -> Self {
        Self {
            keys: s.keys,
            item_type: s.item_type,
            sub_type: s.sub_type,
            size: s.size,
            manufacturer: s.manufacturer,
        }
    }
}

fn folder(path: &str) -> String {
    let mut parts: Vec<&str> = path.split('/').collect();
    parts.pop(); // the file itself
    parts.truncate(FOLDER_SEGMENTS);
    parts.join("/")
}

/// Build [`KeyRefs`] from scanned records, keeping only keys of the stock file.
pub fn collect(records: impl IntoIterator<Item = Scanned>, stock: &StockIni) -> KeyRefs {
    let mut b = KeyRefsBuilder::new();
    for r in records {
        if r.scan.keys.is_empty() {
            continue;
        }
        let keys: Vec<&str> = r
            .scan
            .keys
            .iter()
            .map(String::as_str)
            .filter(|k| stock.find(k).is_ok())
            .collect();
        if keys.is_empty() {
            continue;
        }
        let is_entity = r.struct_type == "EntityClassDefinition";
        let source = RefSource {
            item_type: r.scan.item_type.filter(|_| is_entity),
            sub_type: r.scan.sub_type.filter(|_| is_entity),
            size: r.scan.size.filter(|_| is_entity),
            manufacturer: r.scan.manufacturer.filter(|_| is_entity),
            folder: folder(&r.path),
            struct_type: r.struct_type,
        };
        b.add(source, keys);
    }
    b.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use starorg_core::ini::BOM;

    fn scan_entity() -> RecordScan {
        let mut s = RecordScan::default();
        s.begin(None);
        s.string(Some("_RecordName_"), "EntityClassDefinition.X");
        s.begin(Some("Components"));
        s.begin(None);
        s.begin(Some("AttachDef"));
        s.string(Some("Type"), "Cooler");
        s.string(Some("SubType"), "UNDEFINED");
        s.int(Some("Size"), 2);
        s.record_ref(
            Some("Manufacturer"),
            "file://./../../decals/paint_logo.json",
        );
        s.record_ref(
            Some("Manufacturer"),
            "file://./../../scitemmanufacturer/scitemmanufacturer.aegs.json",
        );
        s.begin(Some("Localization"));
        s.string(Some("Name"), "@item_NameX");
        s.string(Some("Description"), "@item_DescX");
        s.record_ref(
            Some("Manufacturer"),
            "file://./../../decals/paint_logo.json",
        );
        // Not in AttachDef itself: ignored.
        s.string(Some("Type"), "Wrong");
        s.end();
        s.end();
        s.end();
        s.string(Some("Other"), "@item_NameX");
        s.string(Some("Plain"), "no key");
        s.string(Some("At"), "@");
        s.end();
        s.end();
        s
    }

    #[test]
    fn scan_keeps_keys_and_attach_def() {
        let r: ScanResult = scan_entity().into();
        assert_eq!(r.keys, ["@item_NameX", "@item_DescX", "@item_NameX"]);
        assert_eq!(r.item_type.as_deref(), Some("Cooler"));
        assert_eq!(r.sub_type.as_deref(), Some("UNDEFINED"));
        assert_eq!(r.size, Some(2));
        assert_eq!(r.manufacturer.as_deref(), Some("AEGS"));
    }

    #[test]
    fn collect_filters_to_stock_keys() {
        let mut b = BOM.to_vec();
        b.extend_from_slice(b"item_NameX=X\r\nitem_DescX,P=D\r\n");
        let stock = StockIni::parse(b);
        let refs = collect(
            [
                Scanned {
                    struct_type: "EntityClassDefinition".into(),
                    path: "entities/scitem/ships/cooler/x".into(),
                    scan: scan_entity().into(),
                },
                Scanned {
                    struct_type: "BuildingBlocks_Canvas".into(),
                    path: "ui/hud/canvas".into(),
                    scan: ScanResult {
                        keys: vec!["@item_NameX".into(), "@not_in_stock".into()],
                        item_type: Some("ignored for non-entities".into()),
                        ..Default::default()
                    },
                },
            ],
            &stock,
        );
        assert_eq!(refs.keys.len(), 2);
        assert_eq!(refs.keys["item_namex"], vec![[0, 2], [1, 1]]);
        assert_eq!(refs.keys["item_descx"], vec![[0, 1]]);
        assert_eq!(refs.sources[0].folder, "entities/scitem/ships");
        assert_eq!(refs.sources[0].manufacturer.as_deref(), Some("AEGS"));
        assert_eq!(refs.sources[1].folder, "ui/hud");
        assert_eq!(refs.sources[1].item_type, None);
    }
}
