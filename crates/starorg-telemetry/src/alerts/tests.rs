//! Rule evaluation, baselines, cooldowns, hysteresis and overrides on synthetic frames (no
//! recording, no CIG data).

use super::*;
use serde_json::json;

fn ship(name: &str, engines: bool, boost: f64, hydrogen: f64) -> Value {
    json!({
        "actor": {"vitals": {"health": {"current": 100.0, "max": 100.0}, "oxygen": {"current": 1.0, "max": 1.0}}},
        "vehicle": {"state": {
            "name": name,
            "health": 1.0,
            "power": {"engines": engines, "shield_generators": true, "shield_emitters": true},
            "shields": {"level": 1.0, "faces": [0.25, 0.25, 0.25, 0.25]},
            "flight": {"instruments": {"boost": {"reserves": boost}, "hydrogen_fuel": hydrogen, "quantum_fuel": 0.8, "speed": 50.0}}
        }}
    })
}

fn boost(b: f64) -> Value {
    ship("Test Ship", true, b, 1.0)
}

fn only(ids: &[&str]) -> AlertEngine {
    AlertEngine::new(
        defaults()
            .into_iter()
            .filter(|r| ids.contains(&r.id.as_str()))
            .collect(),
    )
}

/// Feed `(t, frame)` pairs; the ids fired at each step, `t` in seconds.
fn run(e: &mut AlertEngine, frames: &[(f64, Value)]) -> Vec<(f64, String)> {
    let mut out = Vec::new();
    for (t, f) in frames {
        for a in e.evaluate(f, *t, (*t * 1000.0) as u64) {
            out.push((*t, a.rule));
        }
    }
    out
}

/// Frames every 0.1 s from `t0` for `n` steps, built by `f(t)`.
fn ticks(t0: f64, n: usize, f: impl Fn(f64) -> Value) -> Vec<(f64, Value)> {
    (0..n)
        .map(|i| {
            let t = t0 + i as f64 * 0.1;
            (t, f(t))
        })
        .collect()
}

#[test]
fn defaults_are_valid() {
    let rules = defaults();
    assert!(rules.len() >= 15);
    let ids: HashSet<&str> = rules.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids.len(), rules.len(), "unique ids");
    for r in &rules {
        assert!(starorg_core::telemetry::valid_rule_id(&r.id), "{}", r.id);
        assert!(
            !r.message.is_empty() && !r.description.is_empty(),
            "{}",
            r.id
        );
        for s in &r.supersedes {
            assert!(ids.contains(s.as_str()), "{} supersedes {s}", r.id);
        }
        assert!(
            matches!(
                r.sound.as_deref(),
                None | Some("info" | "warn" | "critical")
            ),
            "{}",
            r.id
        );
        // A default passes its own checks: an empty override and a round trip of its values.
        let o = match r.trigger {
            Trigger::Threshold { value, rearm, .. } => RuleOverride {
                value: Some(value),
                rearm,
                ..RuleOverride::default()
            },
            _ => RuleOverride::default(),
        };
        assert_eq!(apply_override(r, &o).as_ref(), Ok(r), "{}", r.id);
        if let Trigger::Composite { all, any } = &r.trigger {
            assert!(!all.is_empty() || !any.is_empty(), "{}", r.id);
        }
    }
    // Off by default only where sc-telemetry has it off; none here.
    assert!(rules.iter().all(|r| r.enabled));
}

#[test]
fn startup_is_a_baseline_and_a_crossing_fires_once() {
    let mut e = only(&["boost_low"]);
    // Already low on the first frame and while settling: latched, no alert.
    let mut f = ticks(0.0, 30, |_| boost(0.1));
    // Recover past the re-arm point, then drop again: one alert.
    f.extend(ticks(3.0, 5, |_| boost(0.5)));
    f.extend(ticks(3.5, 20, |_| boost(0.2)));
    let fired = run(&mut e, &f);
    assert_eq!(fired.len(), 1, "{fired:?}");
    assert!((fired[0].0 - 3.5).abs() < 1e-9);
}

#[test]
fn hysteresis_needs_the_rearm_point() {
    let mut e = only(&["boost_low"]);
    let mut f = ticks(0.0, 20, |_| boost(0.8));
    f.extend(ticks(2.0, 3, |_| boost(0.2))); // fires
    f.extend(ticks(2.3, 177, |_| boost(0.3))); // above 25 % but below the 35 % re-arm
    f.extend(ticks(20.0, 10, |_| boost(0.2))); // cooldown over, still not armed
    f.extend(ticks(21.0, 190, |_| boost(0.4))); // past the re-arm point
    f.extend(ticks(40.0, 3, |_| boost(0.2))); // fires again
    let fired: Vec<f64> = run(&mut e, &f).into_iter().map(|(t, _)| t).collect();
    assert_eq!(fired.len(), 2, "{fired:?}");
    assert!((fired[0] - 2.0).abs() < 1e-9 && (fired[1] - 40.0).abs() < 1e-9);
}

#[test]
fn a_cooldown_delays_but_never_drops() {
    let mut e = only(&["boost_low"]); // cooldown 15 s
    let mut f = ticks(0.0, 20, |_| boost(0.8));
    f.extend(ticks(2.0, 2, |_| boost(0.2))); // fires at 2.0
    f.extend(ticks(3.0, 2, |_| boost(0.5))); // re-armed
    // Low again inside the cooldown, and it stays low: fires when the cooldown ends.
    f.extend(ticks(4.0, 140, |_| boost(0.2)));
    let fired: Vec<f64> = run(&mut e, &f).into_iter().map(|(t, _)| t).collect();
    assert_eq!(fired.len(), 2, "{fired:?}");
    assert!((fired[1] - 17.0).abs() < 0.05, "{fired:?}");
}

#[test]
fn debounce_ignores_a_one_frame_glitch() {
    let mut e = only(&["hydrogen_low"]); // debounce 2 s
    let full = |_| ship("Test Ship", true, 1.0, 0.9);
    let low = |_| ship("Test Ship", true, 1.0, 0.1);
    let mut f = ticks(0.0, 20, full);
    f.push((2.0, low(0.0))); // a single-frame glitch to 0
    f.extend(ticks(2.1, 10, full));
    assert!(run(&mut e, &f).is_empty());
    let mut f = ticks(5.0, 15, low); // held for 1.5 s: not yet
    f.extend(ticks(6.5, 10, low)); // 2 s reached at 7.0
    let fired = run(&mut e, &f);
    assert_eq!(fired.len(), 1, "{fired:?}");
    assert!((fired[0].0 - 7.0).abs() < 0.05, "{fired:?}");
}

#[test]
fn a_guard_makes_the_current_state_the_baseline() {
    let mut e = only(&["hydrogen_low"]);
    // Engines off: hydrogen reads 0, no alert. Engines on with hydrogen still low: baseline.
    let mut f = ticks(0.0, 30, |_| ship("Test Ship", false, 1.0, 0.0));
    f.extend(ticks(3.0, 40, |_| ship("Test Ship", true, 1.0, 0.1)));
    assert!(run(&mut e, &f).is_empty());
}

#[test]
fn a_stronger_rule_supersedes_its_sibling() {
    let mut e = only(&["boost_low", "boost_empty"]);
    let mut f = ticks(0.0, 20, |_| boost(0.8));
    f.extend(ticks(2.0, 20, |_| boost(0.0))); // straight to empty
    let fired = run(&mut e, &f);
    assert_eq!(fired, vec![(2.0, "boost_empty".to_owned())]);
}

#[test]
fn a_vehicle_change_and_a_gap_rebaseline() {
    let mut e = only(&["boost_low"]);
    let mut f = ticks(0.0, 20, |_| boost(0.8));
    // Another ship that starts with low boost: settling, no alert.
    f.extend(ticks(2.0, 30, |_| ship("Other Ship", true, 0.1, 1.0)));
    assert!(run(&mut e, &f).is_empty());
    let mut e = only(&["boost_low"]);
    let mut f = ticks(0.0, 20, |_| boost(0.8));
    // The stream stalls for 10 s and resumes with low boost: a baseline, not a crossing.
    f.extend(ticks(12.0, 30, |_| boost(0.1)));
    assert!(run(&mut e, &f).is_empty());
    // A reset (a new stream) is a baseline too.
    let mut e = only(&["boost_low"]);
    run(&mut e, &ticks(0.0, 20, |_| boost(0.8)));
    e.reset();
    assert!(run(&mut e, &ticks(2.0, 30, |_| boost(0.1))).is_empty());
}

#[test]
fn missing_data_never_fires() {
    let mut e = AlertEngine::default();
    let empty = json!({});
    let nan = json!({"vehicle": {"state": {"name": "X", "health": "NaN",
        "flight": {"instruments": {"boost": {"reserves": 3.4e38}}}}}});
    let mut f = ticks(0.0, 30, |_| empty.clone());
    f.extend(ticks(3.0, 30, |_| nan.clone()));
    assert!(run(&mut e, &f).is_empty());
    assert!(e.evaluate(&json!(null), 10.0, 0).is_empty());
}

#[test]
fn transitions_edges_and_composites() {
    let ann = |s: &str| json!({"vehicle": {"state": {"name": "X", "annunciators": {"misl": s}}}});
    let mut e = only(&["missile_warning"]);
    let mut f = ticks(0.0, 20, |_| ann("ANNUNCIATOR_STATE_OFF"));
    f.extend(ticks(2.0, 5, |_| ann("ANNUNCIATOR_STATE_MAJOR_ON")));
    f.extend(ticks(2.5, 5, |_| ann("ANNUNCIATOR_STATE_MAJOR_FLASHING"))); // still in the set
    f.extend(ticks(3.0, 80, |_| ann("ANNUNCIATOR_STATE_OFF")));
    f.extend(ticks(11.0, 2, |_| ann("ANNUNCIATOR_STATE_MAJOR_ON")));
    let fired: Vec<f64> = run(&mut e, &f).into_iter().map(|(t, _)| t).collect();
    assert_eq!(fired, vec![2.0, 11.0]);

    // Stepping out into vacuum fires on entry (on_entry), after its 1.5 s debounce.
    let seated = json!({"actor": {"environment": {"breathable_atmosphere": false}},
        "vehicle": {"state": {"name": "X"}}});
    let foot = json!({"actor": {"environment": {"breathable_atmosphere": false}}});
    let mut e = only(&["unbreathable"]);
    let mut f = ticks(0.0, 30, |_| seated.clone());
    f.extend(ticks(3.0, 20, |_| foot.clone()));
    let fired = run(&mut e, &f);
    assert_eq!(fired.len(), 1, "{fired:?}");
    assert!((fired[0].0 - 4.5).abs() < 0.05, "{fired:?}");

    let g = |g: f64, safe: bool| {
        json!({"vehicle": {"state": {"name": "X", "flight": {
            "instruments": {"g_meter": {"current": g}},
            "settings": {"ifcs_g_safe": {"active": safe}}}}}})
    };
    let mut e = only(&["g_load"]);
    let mut f = ticks(0.0, 20, |_| g(1.0, false));
    f.extend(ticks(2.0, 10, |_| g(10.0, true))); // G-safe on: silent
    f.extend(ticks(3.0, 10, |_| g(10.0, false))); // fires after 0.3 s
    let fired = run(&mut e, &f);
    assert_eq!(fired.len(), 1, "{fired:?}");
    assert!((fired[0].0 - 3.3).abs() < 0.05, "{fired:?}");
}

#[test]
fn a_face_tuned_away_is_not_down() {
    let faces = |lvl: f64, f: [f64; 4]| {
        json!({"vehicle": {"state": {"name": "X", "power": {"shield_generators": true},
            "shields": {"level": lvl, "faces": f}}}})
    };
    let mut e = only(&["shield_face_down"]);
    // At full charge the front face is tuned to 0: learnt as its share.
    let mut f = ticks(0.0, 20, |_| faces(1.0, [0.0, 0.4, 0.3, 0.3]));
    f.extend(ticks(2.0, 10, |_| faces(0.5, [0.0, 0.2, 0.15, 0.15])));
    assert!(run(&mut e, &f).is_empty());
    // A face with a share drops to 0: down.
    let fired = run(
        &mut e,
        &ticks(3.0, 10, |_| faces(0.5, [0.0, 0.0, 0.25, 0.25])),
    );
    assert_eq!(fired.len(), 1, "{fired:?}");
}

#[test]
fn templates_and_test_alerts() {
    assert_eq!(render("Boost {pct}%", Some(0.234)), "Boost 23%");
    assert_eq!(
        render("Boost {pct_words} percent", Some(0.25)),
        "Boost twenty five percent"
    );
    assert_eq!(render("Health {value:.0f}", Some(29.6)), "Health 30");
    assert_eq!(render("Radiation {value:.2f}", Some(0.5)), "Radiation 0.50");
    assert_eq!(render("{value} / {nope} / {pct}", None), "? / ? / ?");
    assert_eq!(render("stray { brace", Some(1.0)), "stray { brace");
    assert_eq!(words(0), "zero");
    assert_eq!(words(40), "forty");
    assert_eq!(words(100), "one hundred");
    let e = AlertEngine::default();
    let a = e.test_alert("hull_half", 5).unwrap();
    assert_eq!(a.text, "TEST: Hull 50%");
    assert!(a.test && a.value == Some(0.5) && a.at_ms == 5);
    assert_eq!(a.severity, Severity::Warn);
    assert!(e.test_alert("nope", 0).is_none());
    // The engine's output is the contract shape: plain fields, severity in snake case.
    let v = serde_json::to_value(&a).unwrap();
    assert_eq!(v["severity"], json!("warn"));
    assert_eq!(v["sound"], json!("warn"));
}

#[test]
fn overrides_disable_retune_and_refuse() {
    let rules = defaults();
    let mut o = BTreeMap::new();
    o.insert(
        "boost_low".to_owned(),
        RuleOverride {
            enabled: Some(false),
            ..RuleOverride::default()
        },
    );
    o.insert(
        "hull_half".to_owned(),
        RuleOverride {
            value: Some(0.7),
            cooldown_s: Some(5.0),
            ..RuleOverride::default()
        },
    );
    let (r, w) = apply_overrides(&rules, &o);
    assert!(w.is_empty(), "{w:?}");
    let find = |id: &str| r.iter().find(|x| x.id == id).unwrap().clone();
    assert!(!find("boost_low").enabled);
    let hull = find("hull_half");
    assert_eq!(hull.cooldown_s, 5.0);
    // The default's 10 % hysteresis gap moves with the threshold.
    assert!(
        matches!(hull.trigger, Trigger::Threshold { value, rearm: Some(x), .. } if value == 0.7 && (x - 0.8).abs() < 1e-9)
    );

    // A disabled rule never fires.
    let mut e = AlertEngine::new(r.clone());
    let mut f = ticks(0.0, 20, |_| boost(0.8));
    f.extend(ticks(2.0, 20, |_| boost(0.2)));
    assert!(run(&mut e, &f).iter().all(|(_, id)| id != "boost_low"));

    let bad = |id: &str, o: RuleOverride| {
        let mut m = BTreeMap::new();
        m.insert(id.to_owned(), o);
        let (r, w) = apply_overrides(&rules, &m);
        assert_eq!(r, rules, "a bad override changes nothing");
        assert_eq!(w.len(), 1);
        w[0].clone()
    };
    let rearm_wrong_side = RuleOverride {
        rearm: Some(0.1),
        ..RuleOverride::default()
    };
    assert!(bad("boost_low", rearm_wrong_side).contains("past the threshold"));
    let composite_value = RuleOverride {
        value: Some(3.0),
        ..RuleOverride::default()
    };
    assert!(bad("g_load", composite_value).contains("only a threshold"));
    let over_100 = RuleOverride {
        value: Some(1.5),
        ..RuleOverride::default()
    };
    assert!(bad("hull_half", over_100).contains("fraction"));
    let long_cooldown = RuleOverride {
        cooldown_s: Some(1e9),
        ..RuleOverride::default()
    };
    assert!(bad("hull_half", long_cooldown).contains("cooldown"));
    assert!(bad("no_such_rule", RuleOverride::default()).contains("no such"));
}

#[test]
fn set_rules_rebaselines_and_keeps_cooldowns() {
    let mut e = only(&["boost_low"]);
    let mut f = ticks(0.0, 20, |_| boost(0.8));
    f.extend(ticks(2.0, 2, |_| boost(0.2)));
    assert_eq!(run(&mut e, &f).len(), 1);
    run(&mut e, &ticks(2.5, 3, |_| boost(0.5)));
    // Rules changed while armed and above: the next low frame is a baseline (needs a new
    // crossing), and the cooldown from 2.0 still counts.
    let rules = e.rules().to_vec();
    e.set_rules(rules);
    assert!(run(&mut e, &ticks(3.0, 5, |_| boost(0.2))).is_empty());
    run(&mut e, &ticks(4.0, 3, |_| boost(0.5)));
    let fired = run(&mut e, &ticks(5.0, 130, |_| boost(0.2)));
    assert_eq!(fired.len(), 1, "{fired:?}");
    assert!((fired[0].0 - 17.0).abs() < 0.05, "{fired:?}");
}
