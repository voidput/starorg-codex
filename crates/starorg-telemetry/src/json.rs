//! Dynamic messages to JSON, in the shape sc-telemetry's bridge serves (`server.py`
//! `to_dict`: protobuf `MessageToDict` with `preserving_proto_field_name` and
//! `always_print_fields_with_no_presence`):
//!
//! - field names as in the proto (`heart_rate`, not `heartRate`);
//! - enums as value names (`VEHICLE_MASTER_MODE_SCM`; a number the schema does not name
//!   stays a number);
//! - 64-bit integers as strings (`uec_balance`), as proto3 JSON does;
//! - fields without presence are always written, with their default; a message field or a
//!   proto3 `optional` field that is not set is left out;
//! - `float` values as the shortest decimal that reads back as the same `f32` (`0.83`, not
//!   `0.8299999833106995`); NaN and infinities as the strings `"NaN"`, `"Infinity"`,
//!   `"-Infinity"`;
//! - bytes as standard base64; map keys as strings, sorted.
//!
//! Well-known types get no special form (the telemetry schema uses none that matter).

use prost_reflect::{DynamicMessage, FieldDescriptor, Kind, MapKey, ReflectMessage, Value};
use serde_json::{Map, Number, Value as Json};

/// A message as a JSON object.
pub fn to_json(msg: &DynamicMessage) -> Json {
    let mut out = Map::new();
    for fd in msg.descriptor().fields() {
        if fd.supports_presence() && !msg.has_field(&fd) {
            continue;
        }
        let v = msg.get_field(&fd);
        out.insert(fd.name().to_owned(), field_value(&fd, &v));
    }
    Json::Object(out)
}

fn field_value(fd: &FieldDescriptor, v: &Value) -> Json {
    match v {
        Value::List(items) => Json::Array(items.iter().map(|i| scalar(fd, i)).collect()),
        Value::Map(m) => {
            let value_fd = match fd.kind() {
                Kind::Message(entry) => entry.map_entry_value_field(),
                _ => fd.clone(),
            };
            let mut entries: Vec<(String, Json)> = m
                .iter()
                .map(|(k, v)| (map_key(k), scalar(&value_fd, v)))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Json::Object(entries.into_iter().collect())
        }
        v => scalar(fd, v),
    }
}

fn map_key(k: &MapKey) -> String {
    match k {
        MapKey::Bool(b) => b.to_string(),
        MapKey::I32(n) => n.to_string(),
        MapKey::I64(n) => n.to_string(),
        MapKey::U32(n) => n.to_string(),
        MapKey::U64(n) => n.to_string(),
        MapKey::String(s) => s.clone(),
    }
}

fn non_finite(f: f64) -> Json {
    Json::String(
        if f.is_nan() {
            "NaN"
        } else if f > 0.0 {
            "Infinity"
        } else {
            "-Infinity"
        }
        .into(),
    )
}

fn scalar(fd: &FieldDescriptor, v: &Value) -> Json {
    match v {
        Value::Bool(b) => Json::Bool(*b),
        Value::I32(n) => Json::from(*n),
        Value::U32(n) => Json::from(*n),
        Value::I64(n) => Json::String(n.to_string()),
        Value::U64(n) => Json::String(n.to_string()),
        Value::F32(f) if f.is_finite() => {
            // Shortest f32 text, read back as f64: 0.83f32 becomes 0.83, not 0.8299999...
            let short: f64 = f.to_string().parse().unwrap_or(f64::from(*f));
            Number::from_f64(short).map_or(Json::Null, Json::Number)
        }
        Value::F32(f) => non_finite(f64::from(*f)),
        Value::F64(f) if f.is_finite() => Number::from_f64(*f).map_or(Json::Null, Json::Number),
        Value::F64(f) => non_finite(*f),
        Value::String(s) => Json::String(s.clone()),
        Value::Bytes(b) => Json::String(base64(b)),
        Value::EnumNumber(n) => match fd.kind() {
            Kind::Enum(e) => e
                .get_value(*n)
                .map_or_else(|| Json::from(*n), |ev| Json::String(ev.name().to_owned())),
            _ => Json::from(*n),
        },
        Value::Message(m) => to_json(m),
        Value::List(_) | Value::Map(_) => field_value(fd, v),
    }
}

fn base64(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for (i, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
            if i <= c.len() {
                s.push(A[((n >> shift) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock;
    use serde_json::json;

    #[test]
    fn mock_frame_to_json() {
        let s = mock::schema();
        let f = mock::frame(&s, 20.0, &mock::MockState::default());
        let j = to_json(&f);
        let vs = &j["vehicle"]["state"];
        // Enum names, proto field names, int64 as a string.
        assert_eq!(vs["master_mode"]["mode"], json!("VEHICLE_MASTER_MODE_SCM"));
        assert_eq!(
            vs["flight"]["state"]["gear"],
            json!("VEHICLE_COMPONENT_DEPLOYMENT_STATE_RETRACTED")
        );
        assert_eq!(j["actor"]["state"]["uec_balance"], json!("1284550"));
        assert_eq!(
            j["actor"]["state"]["locomotion"],
            json!("ACTOR_LOCOMOTION_STATE_SEATED")
        );
        assert_eq!(vs["shields"]["faces"].as_array().unwrap().len(), 4);
        // Shortest f32 text.
        assert_eq!(vs["flight"]["instruments"]["quantum_fuel"], json!(0.82));
        // Defaults of fields without presence are written.
        assert_eq!(vs["flight"]["state"]["is_landed"], json!(false));
    }

    #[test]
    fn unset_messages_are_left_out_and_defaults_written() {
        let s = mock::schema();
        let desc = s.method("TelemetryStream").unwrap().output();
        let mut m = DynamicMessage::new(desc);
        mock::set_path(&mut m, "actor.vitals.heart_rate", Value::F32(f32::NAN));
        mock::set_path(
            &mut m,
            "actor.state.seat_name",
            Value::String(String::new()),
        );
        let j = to_json(&m);
        assert!(j.get("vehicle").is_none(), "{j}");
        assert_eq!(j["actor"]["vitals"]["heart_rate"], json!("NaN"));
        assert_eq!(j["actor"]["vitals"]["gravity"], json!(0.0));
        assert!(j["actor"]["vitals"].get("health").is_none());
        assert_eq!(j["actor"]["state"]["uec_balance"], json!("0"));
        assert_eq!(
            j["actor"]["state"]["locomotion"],
            json!("ACTOR_LOCOMOTION_STATE_UNSPECIFIED")
        );
    }

    #[test]
    fn unset_proto3_optional_is_left_out() {
        let s = mock::schema();
        let desc = s.method("TriggerGameAction").unwrap().input();
        let mut m = DynamicMessage::new(desc);
        m.set_field_by_name("action_name", Value::String("v_lights".into()));
        assert_eq!(to_json(&m), json!({"action_name": "v_lights"}));
        m.set_field_by_name("hold_ms", Value::U32(0));
        assert_eq!(
            to_json(&m),
            json!({"action_name": "v_lights", "hold_ms": 0})
        );
    }

    #[test]
    fn base64_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
