//! Canonical JSON and build-stable settings hashes for `applied.json`.
//!
//! A settings hash must change when the user changes a setting and must **not** change when a
//! new build only adds a field with a default. [`settings_hash`] therefore hashes the
//! *difference from the defaults*: only values that differ from `T::default()` are kept, with
//! object keys sorted. A new field the user never touched equals its default and drops out.
//!
//! Changing a default (or what a setting does) changes the output for unchanged inputs; that
//! is what [`crate::OUTPUT_VERSION`] is for, not this hash.

use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// JSON text with object keys sorted (whatever serde_json's `preserve_order` says).
pub fn canonical(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let b: BTreeMap<&String, String> = m.iter().map(|(k, v)| (k, canonical(v))).collect();
            let parts: Vec<String> = b
                .into_iter()
                .map(|(k, v)| format!("{}:{v}", Value::String(k.clone())))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        Value::Array(a) => {
            format!(
                "[{}]",
                a.iter().map(canonical).collect::<Vec<_>>().join(",")
            )
        }
        other => other.to_string(),
    }
}

/// The parts of `cur` that differ from `def`, recursing into objects. Arrays and scalars are
/// compared whole. A key the defaults have but `cur` lacks (a `skip_serializing_if` field)
/// is kept as `{"$absent": true}` so that difference still counts.
pub fn non_default(cur: &Value, def: &Value) -> Option<Value> {
    match (cur, def) {
        (Value::Object(c), Value::Object(d)) => {
            let mut out = serde_json::Map::new();
            for (k, v) in c {
                let diff = match d.get(k) {
                    Some(dv) => non_default(v, dv),
                    None => Some(v.clone()),
                };
                if let Some(x) = diff {
                    out.insert(k.clone(), x);
                }
            }
            for k in d.keys().filter(|k| !c.contains_key(*k)) {
                out.insert(k.clone(), serde_json::json!({ "$absent": true }));
            }
            (!out.is_empty()).then_some(Value::Object(out))
        }
        _ if cur == def => None,
        _ => Some(cur.clone()),
    }
}

/// SHA-256 of the canonical JSON of what differs between `cur` and `def` (`{}` when nothing).
pub fn diff_hash(cur: &Value, def: &Value) -> String {
    let d = non_default(cur, def).unwrap_or_else(|| Value::Object(Default::default()));
    crate::fsutil::sha256_hex(canonical(&d).as_bytes())
}

/// Build-stable hash of a settings struct: [`diff_hash`] against `T::default()`.
pub fn settings_hash<T: Serialize + Default>(v: &T) -> String {
    let cur = serde_json::to_value(v).unwrap_or_default();
    let def = serde_json::to_value(T::default()).unwrap_or_default();
    diff_hash(&cur, &def)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Serialize)]
    struct V1 {
        a: bool,
        n: u32,
    }
    impl Default for V1 {
        fn default() -> Self {
            Self { a: true, n: 3 }
        }
    }
    /// V1 plus a field a later build adds, with a default.
    #[derive(Serialize)]
    struct V2 {
        a: bool,
        added: String,
        n: u32,
    }
    impl Default for V2 {
        fn default() -> Self {
            Self {
                a: true,
                added: "x".into(),
                n: 3,
            }
        }
    }

    #[test]
    fn new_default_field_keeps_the_hash() {
        let old = V1 { a: false, n: 3 };
        let new = V2 {
            a: false,
            ..V2::default()
        };
        assert_eq!(settings_hash(&old), settings_hash(&new));
        assert_eq!(settings_hash(&V1::default()), settings_hash(&V2::default()));
        // A real change still shows, in the old and the new field.
        assert_ne!(settings_hash(&old), settings_hash(&V1::default()));
        let touched = V2 {
            added: "y".into(),
            ..new
        };
        assert_ne!(settings_hash(&old), settings_hash(&touched));
    }

    #[test]
    fn diff_is_key_order_free_and_nested() {
        let def = json!({"t": {"x": 1, "y": [1, 2]}, "z": true});
        let a = json!({"z": true, "t": {"y": [1, 2], "x": 2}});
        let b = json!({"t": {"x": 2, "y": [1, 2]}, "z": true});
        assert_eq!(diff_hash(&a, &def), diff_hash(&b, &def));
        assert_eq!(non_default(&a, &def), Some(json!({"t": {"x": 2}})));
        assert_eq!(non_default(&def, &def), None);
        // Arrays compare whole; a missing default key counts.
        assert_eq!(
            non_default(&json!({"t": {"x": 1, "y": [2, 1]}}), &def),
            Some(json!({"t": {"y": [2, 1]}, "z": {"$absent": true}}))
        );
    }
}
