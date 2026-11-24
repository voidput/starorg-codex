//! Synthetic schema and frames for tests and the UI preview (port of sc-telemetry `mock.py`
//! and the `FakeTrigger` in `server.py`). No CIG data: the schema is written here by hand
//! and holds only the fields the [`crate::frames::Snapshot`] and the trigger path read,
//! with the field names of sc-telemetry's public docs (`docs/telemetry/api-spec.md` §3).
//! Field numbers and enum numbers are our own and need not match the game's: everything
//! here goes by name.

use crate::schema::{Schema, SchemaOrigin};
use crate::trigger::{TriggerOutcome, TriggerStatus, TriggerableAction};
use prost_reflect::prost_types::field_descriptor_proto::{Label, Type};
use prost_reflect::prost_types::{
    DescriptorProto, EnumDescriptorProto, EnumValueDescriptorProto, FieldDescriptorProto,
    FileDescriptorProto, MethodDescriptorProto, OneofDescriptorProto, ServiceDescriptorProto,
};
use prost_reflect::{DynamicMessage, Kind, ReflectMessage, Value};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const PKG: &str = "sc.game.telemetry.v1";

/// One field: name, number, type, and the type name for a message or enum field.
struct F(&'static str, i32, Type, &'static str);

fn field(f: &F, repeated: bool) -> FieldDescriptorProto {
    let F(name, number, ty, type_name) = *f;
    FieldDescriptorProto {
        name: Some(name.into()),
        number: Some(number),
        label: Some(if repeated {
            Label::Repeated
        } else {
            Label::Optional
        } as i32),
        r#type: Some(ty as i32),
        type_name: (!type_name.is_empty()).then(|| format!(".{PKG}.{type_name}")),
        ..Default::default()
    }
}

fn msg(name: &str, fields: &[F]) -> DescriptorProto {
    msg_rep(name, fields, &[])
}

/// A message whose fields named in `repeated` are lists.
fn msg_rep(name: &str, fields: &[F], repeated: &[&str]) -> DescriptorProto {
    DescriptorProto {
        name: Some(name.into()),
        field: fields
            .iter()
            .map(|f| field(f, repeated.contains(&f.0)))
            .collect(),
        ..Default::default()
    }
}

fn enumeration(name: &str, prefix: &str, values: &[&str]) -> EnumDescriptorProto {
    EnumDescriptorProto {
        name: Some(name.into()),
        value: values
            .iter()
            .enumerate()
            .map(|(i, v)| EnumValueDescriptorProto {
                name: Some(format!("{prefix}{v}")),
                number: Some(i as i32),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

fn file(name: &str, deps: &[&str]) -> FileDescriptorProto {
    FileDescriptorProto {
        name: Some(name.into()),
        package: Some(PKG.into()),
        dependency: deps.iter().map(|d| (*d).into()).collect(),
        syntax: Some("proto3".into()),
        ..Default::default()
    }
}

/// The synthetic `types.proto` and `api.proto` (the same split as the game's).
pub fn file_descriptor_protos() -> Vec<FileDescriptorProto> {
    use Type::{Bool, Enum, Float, Int64, Message, String as Str, Uint32};
    let mut types = file("sc/game/telemetry/v1/types.proto", &[]);
    types.enum_type = vec![
        enumeration(
            "ActorLocomotionState",
            "ACTOR_LOCOMOTION_STATE_",
            &["UNSPECIFIED", "ON_FOOT", "SEATED"],
        ),
        enumeration(
            "VehicleMasterModeKind",
            "VEHICLE_MASTER_MODE_",
            &["UNSPECIFIED", "NAVIGATION", "SCM"],
        ),
        enumeration(
            "VehicleComponentDeploymentState",
            "VEHICLE_COMPONENT_DEPLOYMENT_STATE_",
            &[
                "UNSPECIFIED",
                "RETRACTED",
                "DEPLOYING",
                "DEPLOYED",
                "RETRACTING",
            ],
        ),
        enumeration(
            "TriggerGameActionStatus",
            "TRIGGER_GAME_ACTION_STATUS_",
            &[
                "UNSPECIFIED",
                "OK",
                "UNKNOWN_ACTION",
                "MAP_INACTIVE",
                "RATE_LIMITED",
                "FAILED",
                "HOLD_NOT_ALLOWED",
                "FILTERED",
            ],
        ),
    ];
    types.message_type = vec![
        msg(
            "Gauge",
            &[F("current", 1, Float, ""), F("max", 2, Float, "")],
        ),
        msg(
            "ActorVitals",
            &[
                F("health", 1, Message, "Gauge"),
                F("oxygen", 2, Message, "Gauge"),
                F("body_temperature", 3, Message, "Gauge"),
                F("heart_rate", 4, Float, ""),
                F("gravity", 5, Float, ""),
            ],
        ),
        msg(
            "ActorState",
            &[
                F("locomotion", 1, Enum, "ActorLocomotionState"),
                F("seat_name", 2, Str, ""),
                F("uec_balance", 3, Int64, ""),
            ],
        ),
        msg(
            "Actor",
            &[
                F("vitals", 1, Message, "ActorVitals"),
                F("state", 2, Message, "ActorState"),
            ],
        ),
        msg_rep(
            "VehicleShields",
            &[F("level", 1, Float, ""), F("faces", 2, Float, "")],
            &["faces"],
        ),
        msg(
            "VehicleMasterMode",
            &[
                F("mode", 1, Enum, "VehicleMasterModeKind"),
                F("spooled", 2, Bool, ""),
            ],
        ),
        msg(
            "VehicleBoost",
            &[
                F("engaged", 1, Bool, ""),
                F("output", 2, Float, ""),
                F("reserves", 3, Float, ""),
            ],
        ),
        msg(
            "VehicleFlightInstruments",
            &[
                F("speed", 1, Float, ""),
                F("hydrogen_fuel", 2, Float, ""),
                F("quantum_fuel", 3, Float, ""),
                F("g_meter", 4, Message, "Gauge"),
                F("boost", 5, Message, "VehicleBoost"),
                F("altitude", 6, Float, ""),
                F("vertical_speed_indicator", 7, Float, ""),
                F("heading", 8, Float, ""),
            ],
        ),
        msg(
            "VehicleFlightState",
            &[
                F("is_landed", 1, Bool, ""),
                F("gear", 2, Enum, "VehicleComponentDeploymentState"),
            ],
        ),
        msg(
            "VehicleFlight",
            &[
                F("state", 1, Message, "VehicleFlightState"),
                F("instruments", 2, Message, "VehicleFlightInstruments"),
            ],
        ),
        msg(
            "VehiclePower",
            &[
                F("engines", 1, Bool, ""),
                F("shield_generators", 2, Bool, ""),
            ],
        ),
        msg(
            "VehicleState",
            &[
                F("name", 1, Str, ""),
                F("class_name", 2, Str, ""),
                F("health", 3, Float, ""),
                F("lights_enabled", 4, Bool, ""),
                F("shields", 5, Message, "VehicleShields"),
                F("master_mode", 6, Message, "VehicleMasterMode"),
                F("power", 7, Message, "VehiclePower"),
                F("flight", 8, Message, "VehicleFlight"),
            ],
        ),
        msg("Vehicle", &[F("state", 1, Message, "VehicleState")]),
        msg_rep(
            "TriggerableAction",
            &[
                F("action_name", 1, Str, ""),
                F("category", 2, Str, ""),
                F("holdable", 3, Bool, ""),
            ],
            &[],
        ),
    ];

    let mut api = file(crate::MARKER, &["sc/game/telemetry/v1/types.proto"]);
    let mut trigger_req = msg(
        "TriggerGameActionRequest",
        &[F("action_name", 1, Str, ""), F("hold_ms", 2, Uint32, "")],
    );
    // `optional uint32 hold_ms = 2;`: proto3 optional is a synthetic one-field oneof.
    trigger_req.field[1].proto3_optional = Some(true);
    trigger_req.field[1].oneof_index = Some(0);
    trigger_req.oneof_decl = vec![OneofDescriptorProto {
        name: Some("_hold_ms".into()),
        ..Default::default()
    }];
    api.message_type = vec![
        msg("TelemetryStreamRequest", &[]),
        msg(
            "TelemetryStreamResponse",
            &[
                F("actor", 1, Message, "Actor"),
                F("vehicle", 2, Message, "Vehicle"),
            ],
        ),
        msg("ListTriggerableActionsRequest", &[]),
        msg_rep(
            "ListTriggerableActionsResponse",
            &[F("actions", 1, Message, "TriggerableAction")],
            &["actions"],
        ),
        trigger_req,
        msg(
            "TriggerGameActionResponse",
            &[
                F("status", 1, Enum, "TriggerGameActionStatus"),
                F("message", 2, Str, ""),
            ],
        ),
    ];
    let method = |name: &str, input: &str, output: &str, stream: bool| MethodDescriptorProto {
        name: Some(name.into()),
        input_type: Some(format!(".{PKG}.{input}")),
        output_type: Some(format!(".{PKG}.{output}")),
        server_streaming: stream.then_some(true),
        ..Default::default()
    };
    api.service = vec![ServiceDescriptorProto {
        name: Some("GameTelemetryService".into()),
        method: vec![
            method(
                "TelemetryStream",
                "TelemetryStreamRequest",
                "TelemetryStreamResponse",
                true,
            ),
            method(
                "ListTriggerableActions",
                "ListTriggerableActionsRequest",
                "ListTriggerableActionsResponse",
                false,
            ),
            method(
                "TriggerGameAction",
                "TriggerGameActionRequest",
                "TriggerGameActionResponse",
                false,
            ),
        ],
        ..Default::default()
    }];
    vec![types, api]
}

/// The synthetic schema (key `mock`).
pub fn schema() -> Schema {
    Schema::from_files(file_descriptor_protos(), "mock", SchemaOrigin::Synthetic)
        .expect("the synthetic schema is valid")
}

/// Set a field by a dotted path, creating the messages on the way. Enum fields take the
/// value name as a [`Value::String`]. Unknown paths are ignored (a schema without the field).
pub fn set_path(msg: &mut DynamicMessage, path: &str, value: Value) {
    let mut cur = msg;
    let mut parts = path.split('.').peekable();
    while let Some(name) = parts.next() {
        let Some(fd) = cur.descriptor().get_field_by_name(name) else {
            return;
        };
        if parts.peek().is_none() {
            let value = match (fd.kind(), value) {
                (Kind::Enum(e), Value::String(s)) => match e.get_value_by_name(&s) {
                    Some(v) => Value::EnumNumber(v.number()),
                    None => return,
                },
                (_, v) => v,
            };
            let _ = cur.try_set_field(&fd, value);
            return;
        }
        let Some(next) = cur.get_field_mut(&fd).as_message_mut() else {
            return;
        };
        cur = next;
    }
}

/// Vehicle state a mock trigger can flip (mock.py `STATE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MockState {
    pub lights_enabled: bool,
    pub gear_deployed: bool,
    pub shield_generators: bool,
}

impl Default for MockState {
    fn default() -> Self {
        Self {
            lights_enabled: true,
            gear_deployed: false,
            shield_generators: true,
        }
    }
}

/// One plausible frame at `t` seconds (mock.py `frame`, the fields of the synthetic schema).
pub fn frame(schema: &Schema, t: f64, state: &MockState) -> DynamicMessage {
    let desc = schema
        .method("TelemetryStream")
        .expect("schema has TelemetryStream")
        .output();
    let mut m = DynamicMessage::new(desc);
    let s = f64::sin;
    let f = |v: f64| Value::F32(v as f32);
    let e = |v: &str| Value::String(v.into());
    let gravity = 1.0 + 3.5 * s(t / 4.0).max(0.0);
    let set = set_path;
    set(
        &mut m,
        "actor.vitals.health.current",
        f(80.0 + 20.0 * s(t / 9.0)),
    );
    set(&mut m, "actor.vitals.health.max", f(100.0));
    set(
        &mut m,
        "actor.vitals.oxygen.current",
        f(0.7 + 0.3 * s(t / 13.0)),
    );
    set(&mut m, "actor.vitals.oxygen.max", f(1.0));
    set(
        &mut m,
        "actor.vitals.body_temperature.current",
        f(37.0 + 0.6 * s(t / 7.0)),
    );
    set(&mut m, "actor.vitals.body_temperature.max", f(100.0));
    set(
        &mut m,
        "actor.vitals.heart_rate",
        f(78.0 + 25.0 * (0.5 + 0.5 * s(t / 5.0))),
    );
    set(&mut m, "actor.vitals.gravity", f(gravity));
    set(
        &mut m,
        "actor.state.locomotion",
        e("ACTOR_LOCOMOTION_STATE_SEATED"),
    );
    set(&mut m, "actor.state.seat_name", e("Pilot Seat"));
    set(&mut m, "actor.state.uec_balance", Value::I64(1_284_550));

    let vs = "vehicle.state";
    set(&mut m, &format!("{vs}.name"), e("Mock Gladius"));
    set(&mut m, &format!("{vs}.class_name"), e("MOCK_Gladius"));
    set(
        &mut m,
        &format!("{vs}.health"),
        f(0.75 + 0.25 * s(t / 17.0)),
    );
    set(
        &mut m,
        &format!("{vs}.lights_enabled"),
        Value::Bool(state.lights_enabled),
    );
    set(
        &mut m,
        &format!("{vs}.shields.level"),
        f(0.5 + 0.5 * s(t / 6.0)),
    );
    let faces = (0..4)
        .map(|k| f((0.6 + 0.4 * s(t / 3.0 + f64::from(k))).clamp(0.0, 1.0)))
        .collect();
    set(&mut m, &format!("{vs}.shields.faces"), Value::List(faces));
    let mode = if (t / 15.0) as i64 % 2 == 1 {
        "VEHICLE_MASTER_MODE_SCM"
    } else {
        "VEHICLE_MASTER_MODE_NAVIGATION"
    };
    set(&mut m, &format!("{vs}.master_mode.mode"), e(mode));
    set(
        &mut m,
        &format!("{vs}.master_mode.spooled"),
        Value::Bool(true),
    );
    set(&mut m, &format!("{vs}.power.engines"), Value::Bool(true));
    set(
        &mut m,
        &format!("{vs}.power.shield_generators"),
        Value::Bool(state.shield_generators),
    );
    set(
        &mut m,
        &format!("{vs}.flight.state.is_landed"),
        Value::Bool(false),
    );
    let gear = if state.gear_deployed {
        "VEHICLE_COMPONENT_DEPLOYMENT_STATE_DEPLOYED"
    } else {
        "VEHICLE_COMPONENT_DEPLOYMENT_STATE_RETRACTED"
    };
    set(&mut m, &format!("{vs}.flight.state.gear"), e(gear));
    let ins = format!("{vs}.flight.instruments");
    set(
        &mut m,
        &format!("{ins}.speed"),
        f(180.0 + 120.0 * s(t / 4.0)),
    );
    set(
        &mut m,
        &format!("{ins}.hydrogen_fuel"),
        f((1.0 - (t % 300.0) / 300.0).max(0.0)),
    );
    set(&mut m, &format!("{ins}.quantum_fuel"), f(0.82));
    set(&mut m, &format!("{ins}.g_meter.current"), f(gravity));
    set(&mut m, &format!("{ins}.g_meter.max"), f(9.0));
    set(
        &mut m,
        &format!("{ins}.boost.engaged"),
        Value::Bool(s(t / 4.0) > 0.6),
    );
    set(
        &mut m,
        &format!("{ins}.boost.output"),
        f(s(t / 4.0).max(0.0)),
    );
    set(
        &mut m,
        &format!("{ins}.boost.reserves"),
        f(0.5 + 0.5 * s(t / 8.0)),
    );
    set(
        &mut m,
        &format!("{ins}.altitude"),
        f(12_000.0 + 3_000.0 * s(t / 30.0)),
    );
    set(
        &mut m,
        &format!("{ins}.vertical_speed_indicator"),
        f(50.0 * s(t / 6.0)),
    );
    set(&mut m, &format!("{ins}.heading"), f((t * 12.0) % 360.0));
    m
}

/// The actions the mock lists (server.py `FAKE_FALLBACK`).
pub fn actions() -> Vec<TriggerableAction> {
    let a = |name: &str, category: &str, holdable: bool| TriggerableAction {
        action_name: name.into(),
        category: category.into(),
        holdable,
    };
    vec![
        a("v_lights", "lights_controller", false),
        a("v_lights_on", "lights_controller", false),
        a("v_lights_off", "lights_controller", false),
        a("v_toggle_landing_system", "spaceship_movement", false),
        a("v_deploy_landing_system", "spaceship_movement", false),
        a("v_retract_landing_system", "spaceship_movement", false),
        a("v_power_toggle_shields", "spaceship_power", false),
        a("v_power_set_shields_on", "spaceship_power", false),
        a("v_power_set_shields_off", "spaceship_power", false),
        a("v_power_set_off", "spaceship_power", false),
        a("v_self_destruct", "spaceship_general", false),
        a("v_invoke_ping", "spaceship_radar", true),
        a("toggle_flashlight", "player", false),
    ]
}

/// Stand-in for the game's `TriggerGameAction` (server.py `FakeTrigger`): answers like the
/// game and flips [`MockState`] for the light, gear and shield actions. Nothing reaches a game.
#[derive(Debug, Clone)]
pub struct MockGame {
    pub state: MockState,
    actions: Vec<TriggerableAction>,
    last: HashMap<String, Instant>,
    /// The game's own debounce (under 1 s; api-spec §3.3).
    pub debounce: Duration,
}

impl Default for MockGame {
    fn default() -> Self {
        Self {
            state: MockState::default(),
            actions: actions(),
            last: HashMap::new(),
            debounce: Duration::from_millis(500),
        }
    }
}

impl MockGame {
    pub fn actions(&self) -> &[TriggerableAction] {
        &self.actions
    }

    pub fn trigger(&mut self, name: &str, hold_ms: Option<u32>, now: Instant) -> TriggerOutcome {
        let out = |status, message: &str| TriggerOutcome {
            status,
            message: format!("mock: {message}"),
        };
        let Some(a) = self.actions.iter().find(|a| a.action_name == name) else {
            return out(TriggerStatus::UnknownAction, "not in allowlist");
        };
        if hold_ms.is_some() && !a.holdable {
            return out(TriggerStatus::HoldNotAllowed, "not holdable");
        }
        if self
            .last
            .get(name)
            .is_some_and(|t| now.saturating_duration_since(*t) < self.debounce)
        {
            return out(TriggerStatus::RateLimited, "too soon");
        }
        self.last.insert(name.to_owned(), now);
        let st = &mut self.state;
        let toggles: [(&mut bool, [&str; 3]); 3] = [
            (
                &mut st.lights_enabled,
                ["v_lights", "v_lights_on", "v_lights_off"],
            ),
            (
                &mut st.gear_deployed,
                [
                    "v_toggle_landing_system",
                    "v_deploy_landing_system",
                    "v_retract_landing_system",
                ],
            ),
            (
                &mut st.shield_generators,
                [
                    "v_power_toggle_shields",
                    "v_power_set_shields_on",
                    "v_power_set_shields_off",
                ],
            ),
        ];
        for (flag, [toggle, on, off]) in toggles {
            if name == toggle {
                *flag = !*flag;
            } else if name == on {
                *flag = true;
            } else if name == off {
                *flag = false;
            }
        }
        out(TriggerStatus::Ok, "accepted")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_schema_builds() {
        let s = schema();
        assert_eq!(s.origin(), SchemaOrigin::Synthetic);
        assert_eq!(s.service().methods().len(), 3);
        let f = frame(&s, 3.0, &MockState::default());
        assert!(f.get_field_by_name("vehicle").is_some());
    }

    #[test]
    fn mock_game_flips_state_and_debounces() {
        let mut g = MockGame::default();
        let t0 = Instant::now();
        assert_eq!(g.trigger("v_lights", None, t0).status, TriggerStatus::Ok);
        assert!(!g.state.lights_enabled);
        assert_eq!(
            g.trigger("v_lights", None, t0).status,
            TriggerStatus::RateLimited
        );
        assert_eq!(
            g.trigger("v_deploy_landing_system", None, t0).status,
            TriggerStatus::Ok
        );
        assert!(g.state.gear_deployed);
        assert_eq!(
            g.trigger("v_lights", Some(500), t0 + Duration::from_secs(2))
                .status,
            TriggerStatus::HoldNotAllowed
        );
        assert_eq!(
            g.trigger("nope", None, t0).status,
            TriggerStatus::UnknownAction
        );
        assert_eq!(
            g.trigger("v_invoke_ping", Some(1500), t0).status,
            TriggerStatus::Ok
        );
    }
}
