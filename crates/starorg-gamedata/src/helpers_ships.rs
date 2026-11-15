//! Small JavaScript-semantics helpers shared by the ships batch of ports (`quantum`,
//! `containers`, `components`, `cargo`, `loot`, `ships`).
//!
//! Kept in one batch file so parallel ports never edit the same lines; a later cleanup
//! can fold these into the shared helper slots once every batch has landed.

use serde_json::Value;

/// `Math.round`: nearest integer, ties toward +infinity (`Math.round(-2.5) == -2`).
pub fn js_round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let f = x.floor();
    // `x - floor(x)` is exact for every double.
    if x - f >= 0.5 { f + 1.0 } else { f }
}

/// `Math.round(x * factor) / factor`, the TypeScript's fixed-decimals idiom.
pub fn round_to(x: f64, factor: f64) -> f64 {
    js_round(x * factor) / factor
}

/// JavaScript truthiness of a number: non-zero and not NaN.
pub fn truthy(x: Option<f64>) -> bool {
    x.is_some_and(|v| v != 0.0 && !v.is_nan())
}

/// A finite number as a JSON value, integral values as integers (as `JSON.stringify`
/// prints them, and so the comparison harness sees the same shape).
pub fn number(x: f64) -> Value {
    serde_json::Number::from_f64(x).map_or(Value::Null, Value::Number)
}

/// `Object.values(obj)` of a JSON object as numbers (non-numbers skipped).
pub fn numbers_of(map: &serde_json::Map<String, Value>) -> impl Iterator<Item = f64> + '_ {
    map.values().filter_map(Value::as_f64)
}

/// `entityName` from `wikelo.ts` (used by `cargo.ts`): the file stem of a `file://` entity
/// path, `.json` removed case-insensitively; `None` for a non-string or empty path.
pub fn entity_name(value: Option<&Value>) -> Option<&str> {
    let path = crate::records::str_(value)?;
    let file = path.rsplit('/').next().filter(|f| !f.is_empty())?;
    let cut = file.len().saturating_sub(5);
    Some(match file.get(cut..) {
        Some(ext) if file.len() >= 5 && ext.eq_ignore_ascii_case(".json") => &file[..cut],
        _ => file,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_like_javascript() {
        assert_eq!(js_round(2.5), 3.0);
        assert_eq!(js_round(-2.5), -2.0);
        assert_eq!(js_round(-2.6), -3.0);
        assert_eq!(js_round(0.49999999999999994), 0.0);
        assert_eq!(round_to(1.23456, 100.0), 1.23);
        assert_eq!(
            round_to(1.005, 100.0),
            1.0,
            "1.005*100 = 100.49999999999999"
        );
        assert!(truthy(Some(1.0)) && !truthy(Some(0.0)) && !truthy(None));
    }

    #[test]
    fn entity_names() {
        use serde_json::json;
        let v = json!("file://./../../entities/scitem/x/Some_Item.JSON");
        assert_eq!(entity_name(Some(&v)), Some("Some_Item"));
        assert_eq!(entity_name(Some(&json!("a/"))), None);
        assert_eq!(entity_name(Some(&json!(""))), None);
        assert_eq!(entity_name(Some(&json!("x.json"))), Some("x"));
    }
}
