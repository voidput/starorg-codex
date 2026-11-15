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
}
