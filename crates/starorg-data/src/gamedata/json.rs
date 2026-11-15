//! Small accessors over StarBreaker's JSON export of a record.
//!
//! The export has one shape for every record: `{"_Type_": ..., field: value, ...}`, entity
//! components in a `Components` array, file references as `file://./../..../x.json`
//! strings, and record references as `{"_RecordPath_", "_RecordName_", "_RecordId_"}`.

use serde_json::Value;

/// Read a dotted path (`"a.b.c"`). Array indices are not supported; use [`first`].
pub fn at<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = v;
    for seg in path.split('.') {
        cur = cur.as_object()?.get(seg)?;
    }
    Some(cur)
}

/// A finite number.
pub fn num(v: Option<&Value>) -> Option<f64> {
    v?.as_f64().filter(|x| x.is_finite())
}

pub fn num_at(v: &Value, path: &str) -> Option<f64> {
    num(at(v, path))
}

/// A non-empty string.
pub fn text(v: Option<&Value>) -> Option<&str> {
    v?.as_str().filter(|s| !s.is_empty())
}

pub fn text_at<'a>(v: &'a Value, path: &str) -> Option<&'a str> {
    text(at(v, path))
}

pub fn type_of(v: &Value) -> Option<&str> {
    text(v.get("_Type_"))
}

pub fn array(v: Option<&Value>) -> &[Value] {
    v.and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Entity components (`Components[]`).
pub fn components(entity: &Value) -> &[Value] {
    array(entity.get("Components"))
}

/// The first component of the given type.
pub fn component<'a>(entity: &'a Value, ty: &str) -> Option<&'a Value> {
    components(entity).iter().find(|c| type_of(c) == Some(ty))
}

pub fn has_component(entity: &Value, ty: &str) -> bool {
    component(entity, ty).is_some()
}

/// `SAttachableComponentParams.AttachDef`, or `Null`.
pub fn attach_def(entity: &Value) -> &Value {
    component(entity, "SAttachableComponentParams")
        .and_then(|c| c.get("AttachDef"))
        .unwrap_or(&Value::Null)
}

const RECORDS_ROOT: &str = "libs/foundry/records/";

/// Normalised record path: lower case, below `libs/foundry/records/`, no extension,
/// forward slashes. Works for record file names (`.xml`) and `file://` references
/// (`.json`).
pub fn record_path(s: &str) -> Option<String> {
    let lower = s.replace('\\', "/").to_lowercase();
    let start = lower.find(RECORDS_ROOT)? + RECORDS_ROOT.len();
    let rest = &lower[start..];
    let rest = rest
        .strip_suffix(".json")
        .or_else(|| rest.strip_suffix(".xml"))
        .unwrap_or(rest);
    (!rest.is_empty()).then(|| rest.to_string())
}

/// The record path a `file://` reference (string) points at.
pub fn ref_path(v: Option<&Value>) -> Option<String> {
    record_path(text(v)?)
}

/// File stem of a `file://.../x.json` reference; for `a.b.json` style double names the
/// last part (`b`).
pub fn referenced_name(v: Option<&Value>) -> Option<&str> {
    let s = text(v)?;
    let file = s.rsplit('/').next()?;
    let stem = file
        .strip_suffix(".json")
        .or_else(|| file.strip_suffix(".JSON"))
        .unwrap_or(file);
    let last = stem.rsplit('.').next()?;
    (!last.is_empty()).then_some(last)
}

/// File stem of a reference, keeping dots (`a.b.json` -> `a.b`).
pub fn referenced_stem(v: Option<&Value>) -> Option<&str> {
    let s = text(v)?;
    let file = s.rsplit('/').next()?;
    let stem = file.strip_suffix(".json").unwrap_or(file);
    (!stem.is_empty()).then_some(stem)
}

/// `_RecordName_` of a record-reference object.
pub fn ref_record_name(v: Option<&Value>) -> Option<&str> {
    text(v?.get("_RecordName_"))
}

/// Numeric fields of a `{_Type_: "Vec3", x, y, z}` struct.
pub fn vec3(v: Option<&Value>) -> Option<std::collections::BTreeMap<String, f64>> {
    let obj = v?.as_object()?;
    let out: std::collections::BTreeMap<String, f64> = obj
        .iter()
        .filter(|(k, _)| k.as_str() != "_Type_")
        .filter_map(|(k, v)| Some((k.clone(), v.as_f64()?)))
        .collect();
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn paths_and_names() {
        assert_eq!(
            record_path("file://./../../../libs/foundry/records/Crafting/BP/bp_x.json").as_deref(),
            Some("crafting/bp/bp_x")
        );
        assert_eq!(
            record_path("libs/foundry/records/entities/spaceships/aegs_gladius.xml").as_deref(),
            Some("entities/spaceships/aegs_gladius")
        );
        assert_eq!(record_path("elsewhere/x.json"), None);
        let r = json!("file://./../x/scitemmanufacturer.aegs.json");
        assert_eq!(referenced_name(Some(&r)), Some("aegs"));
        assert_eq!(referenced_stem(Some(&r)), Some("scitemmanufacturer.aegs"));
        assert_eq!(referenced_name(Some(&json!(""))), None);
    }

    #[test]
    fn accessors() {
        let v = json!({"Components":[{"_Type_":"A","x":{"y":2.5}},{"_Type_":"SAttachableComponentParams","AttachDef":{"Size":3}}]});
        assert_eq!(num_at(component(&v, "A").unwrap(), "x.y"), Some(2.5));
        assert_eq!(num(attach_def(&v).get("Size")), Some(3.0));
        assert!(attach_def(&json!({})).is_null());
        assert_eq!(text_at(&json!({"a":""}), "a"), None);
        let b = vec3(Some(&json!({"_Type_":"Vec3","x":1,"y":2,"z":3}))).unwrap();
        assert_eq!(b["z"], 3.0);
    }
}
