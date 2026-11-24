use super::*;
use crate::build::BuildInfo;

fn channel(dir: &Path, branch: Option<&str>) -> Channel {
    Channel {
        name: dir.file_name().unwrap().to_string_lossy().into_owned(),
        dir: dir.to_path_buf(),
        p4k: dir.join("Data.p4k"),
        build: branch.map(|b| BuildInfo {
            branch: b.into(),
            game_version: GameVersion::from_branch(b),
            change: Some(1),
            version: String::new(),
        }),
        build_error: None,
    }
}

#[test]
fn settings_default_off_and_round_trip() {
    let mut cfg = Config::default();
    assert_eq!(settings(&cfg), Settings::default());
    assert!(!settings(&cfg).enabled && !settings(&cfg).triggers);
    assert_eq!(settings(&cfg).port, DEFAULT_PORT);

    cfg.extra = toml::from_str("[experimental]\nother = \"kept\"\n").unwrap();
    let s = Settings {
        enabled: true,
        port: 50100,
        triggers: true,
    };
    set_settings(&mut cfg, &s).unwrap();
    let text = toml::to_string(&cfg).unwrap();
    let back: Config = toml::from_str(&text).unwrap();
    assert_eq!(settings(&back), s);
    assert!(text.contains("other = \"kept\""), "{text}");
    assert!(set_settings(&mut cfg, &Settings { port: 80, ..s }).is_err());
}

#[test]
fn bad_values_read_as_defaults() {
    let cfg: Config = toml::from_str(
        "[experimental]\ntelemetry = \"yes\"\ntelemetry_port = 80\ntelemetry_triggers = true\n",
    )
    .unwrap();
    let s = settings(&cfg);
    assert!(!s.enabled);
    assert_eq!(s.port, DEFAULT_PORT);
    assert!(
        s.triggers && !s.triggers_active(),
        "triggers need telemetry on"
    );
    let cfg: Config = toml::from_str("[experimental]\ntelemetry_port = 70000\n").unwrap();
    assert_eq!(settings(&cfg).port, DEFAULT_PORT);
    let cfg: Config = toml::from_str("experimental = 3\n").unwrap();
    assert_eq!(settings(&cfg), Settings::default());
}

#[test]
fn eligible_from_5_0() {
    let d = Path::new("/x/PTU");
    assert_eq!(eligible(&channel(d, Some("sc-alpha-5.0.0"))), Ok(()));
    assert_eq!(eligible(&channel(d, Some("sc-alpha-5.1.2-ptu"))), Ok(()));
    assert_eq!(eligible(&channel(d, Some("sc-alpha-6.0"))), Ok(()));
    let old = eligible(&channel(d, Some("sc-alpha-4.10.0"))).unwrap_err();
    assert_eq!(
        old,
        NotEligible::TooOld {
            version: GameVersion {
                major: 4,
                minor: 10,
                patch: 0
            }
        }
    );
    assert!(old.to_string().contains("4.10.0"), "{old}");
    assert_eq!(
        eligible(&channel(d, Some("sc-main"))),
        Err(NotEligible::UnknownVersion)
    );
    assert_eq!(
        eligible(&channel(d, None)),
        Err(NotEligible::UnknownVersion)
    );
}

#[test]
fn game_exe_is_found_case_insensitively() {
    let t = tempfile::tempdir().unwrap();
    let ch = t.path().join("PTU");
    let c = channel(&ch, Some("sc-alpha-5.0.0"));
    std::fs::create_dir_all(&ch).unwrap();
    assert_eq!(game_exe(&c).unwrap(), None);
    std::fs::create_dir_all(ch.join("bin64")).unwrap();
    std::fs::write(ch.join("bin64/starcitizen.EXE"), b"").unwrap();
    assert_eq!(
        game_exe(&c).unwrap(),
        Some(ch.join("bin64/starcitizen.EXE"))
    );
}

#[test]
fn endpoint_is_loopback() {
    assert_eq!(endpoint(50051).to_string(), "127.0.0.1:50051");
    assert!(endpoint(50051).ip().is_loopback());
}

#[test]
fn env_help_per_os() {
    let w = env_help(HelpOs::Windows, 50123);
    assert!(w.contains("    setx GRPC_CLIENT_SERVER_ENABLED 1\n"), "{w}");
    assert!(w.contains("    setx GRPC_CLIENT_SERVER_PORT 50123\n"));
    assert!(w.contains("system tray") && w.contains("Quit"));
    let l = env_help(HelpOs::Linux, 50123);
    assert!(
        l.contains("    export GRPC_CLIENT_SERVER_ENABLED=1\n"),
        "{l}"
    );
    assert!(l.contains("    export GRPC_CLIENT_SERVER_PORT=50123\n"));
    assert!(l.contains("Lutris") && l.contains("%command%"));
    for h in [&w, &l] {
        assert!(h.contains("Never set GRPC_CLIENT_SERVER_EXTERNAL"));
    }
}

fn cfg_text(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap()
}

#[test]
fn configure_user_cfg_on_triggers_off() {
    let t = tempfile::tempdir().unwrap();
    let dirs = AppDirs::under(&t.path().join("app"));
    let ch = t.path().join("PTU");
    std::fs::create_dir_all(&ch).unwrap();
    let before = "r_width = 1\r\ng_language = english\r\n";
    std::fs::write(ch.join("USER.cfg"), before).unwrap();
    let mut s = Settings {
        enabled: true,
        ..Settings::default()
    };

    let c = configure_user_cfg(&dirs, &ch, "i/PTU", &s, false).unwrap();
    assert_eq!(
        c.telemetry,
        LineChange::Adopted {
            outcome: AdoptOutcome::Written
        }
    );
    assert_eq!(
        cfg_text(&ch.join("USER.cfg")),
        format!("{before}{}\r\n", TELEMETRY.line)
    );

    s.triggers = true;
    configure_user_cfg(&dirs, &ch, "i/PTU", &s, false).unwrap();
    let on = cfg_text(&ch.join("USER.cfg"));
    assert!(on.ends_with(&format!(
        "{}\r\n{}\r\n",
        TELEMETRY.line, TELEMETRY_TRIGGERS.line
    )));

    s.triggers = false;
    let c = configure_user_cfg(&dirs, &ch, "i/PTU", &s, false).unwrap();
    assert_eq!(
        c.triggers,
        LineChange::Released {
            outcome: UndoOutcome::Reverted
        }
    );
    assert!(!cfg_text(&ch.join("USER.cfg")).contains("trigger"));

    s.enabled = false;
    s.triggers = true; // ignored while telemetry is off
    configure_user_cfg(&dirs, &ch, "i/PTU", &s, false).unwrap();
    assert_eq!(cfg_text(&ch.join("USER.cfg")), before);
}

#[test]
fn configure_user_cfg_off_removes_the_file_it_created() {
    // No USER.cfg before: the telemetry line creates it, the triggers line finds it. Off
    // takes both out and leaves no file, not an empty one.
    let t = tempfile::tempdir().unwrap();
    let dirs = AppDirs::under(&t.path().join("app"));
    let ch = t.path().join("PTU");
    std::fs::create_dir_all(&ch).unwrap();
    let on = Settings {
        enabled: true,
        triggers: true,
        ..Settings::default()
    };
    configure_user_cfg(&dirs, &ch, "i/PTU", &on, false).unwrap();
    assert!(cfg_text(&ch.join("user.cfg")).contains(TELEMETRY_TRIGGERS.line));
    let c = configure_user_cfg(&dirs, &ch, "i/PTU", &Settings::default(), false).unwrap();
    for l in [c.telemetry, c.triggers] {
        assert_eq!(
            l,
            LineChange::Released {
                outcome: UndoOutcome::Reverted
            }
        );
    }
    assert!(!ch.join("user.cfg").exists());
}

#[test]
fn configure_user_cfg_respects_other_value() {
    let t = tempfile::tempdir().unwrap();
    let dirs = AppDirs::under(&t.path().join("app"));
    let ch = t.path().join("PTU");
    std::fs::create_dir_all(&ch).unwrap();
    let before = "server.game.telemetry.enabled = 0 ; mine\n";
    std::fs::write(ch.join("USER.cfg"), before).unwrap();
    let s = Settings {
        enabled: true,
        triggers: true,
        ..Settings::default()
    };
    let c = configure_user_cfg(&dirs, &ch, "i/PTU", &s, false).unwrap();
    assert_eq!(
        c.telemetry,
        LineChange::Adopted {
            outcome: AdoptOutcome::NeedsDecision { value: "0".into() }
        }
    );
    // No triggers line without the telemetry line.
    assert_eq!(cfg_text(&ch.join("USER.cfg")), before);

    configure_user_cfg(&dirs, &ch, "i/PTU", &s, true).unwrap();
    assert_eq!(
        cfg_text(&ch.join("USER.cfg")),
        format!("{}\n{}\n", TELEMETRY.line, TELEMETRY_TRIGGERS.line)
    );
    let off = Settings::default();
    configure_user_cfg(&dirs, &ch, "i/PTU", &off, false).unwrap();
    assert_eq!(cfg_text(&ch.join("USER.cfg")), before, "previous line back");
}

#[cfg(unix)]
#[test]
fn configure_user_cfg_shared_symlink() {
    let t = tempfile::tempdir().unwrap();
    let dirs = AppDirs::under(&t.path().join("app"));
    let shared = t.path().join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    // No file yet: the first channel creates it through the link.
    for ch in ["PTU", "LIVE"] {
        std::fs::create_dir_all(t.path().join(ch)).unwrap();
        std::os::unix::fs::symlink(shared.join("USER.cfg"), t.path().join(ch).join("user.cfg"))
            .unwrap();
    }
    let on = Settings {
        enabled: true,
        triggers: true,
        ..Settings::default()
    };
    let ptu = t.path().join("PTU");
    let live = t.path().join("LIVE");
    configure_user_cfg(&dirs, &ptu, "i/PTU", &on, false).unwrap();
    let c = configure_user_cfg(&dirs, &live, "i/LIVE", &on, false).unwrap();
    assert_eq!(
        c.telemetry,
        LineChange::Adopted {
            outcome: AdoptOutcome::AlreadySet
        }
    );
    assert!(
        std::fs::symlink_metadata(live.join("user.cfg"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let off = Settings::default();
    let c = configure_user_cfg(&dirs, &ptu, "i/PTU", &off, false).unwrap();
    assert!(matches!(
        c.telemetry,
        LineChange::Released {
            outcome: UndoOutcome::StillUsed { .. }
        }
    ));
    assert!(cfg_text(&shared.join("USER.cfg")).contains(TELEMETRY_TRIGGERS.line));
    configure_user_cfg(&dirs, &live, "i/LIVE", &off, false).unwrap();
    // Created through a link: emptied, not removed (as for the language line).
    assert_eq!(cfg_text(&shared.join("USER.cfg")), "");
    assert!(dirs.cfg_dir().read_dir().unwrap().all(|e| {
        let n = e.unwrap().file_name().to_string_lossy().into_owned();
        !n.ends_with(".json")
    }));
    assert!(
        std::fs::symlink_metadata(live.join("user.cfg"))
            .unwrap()
            .file_type()
            .is_symlink(),
        "the link stays"
    );
}

#[test]
fn alert_settings_default_off_and_round_trip() {
    let mut cfg = Config::default();
    let a = alert_settings(&cfg);
    assert_eq!(a, AlertSettings::default());
    assert_eq!(a.notifications, Notifications::Off);
    assert!(!a.speech && a.overrides.is_empty());

    // The telemetry switches and the alert settings share the table without clobbering.
    set_settings(
        &mut cfg,
        &Settings {
            enabled: true,
            ..Settings::default()
        },
    )
    .unwrap();
    let mut a = AlertSettings {
        notifications: Notifications::InAppSound,
        speech: true,
        overrides: Default::default(),
    };
    a.overrides.insert(
        "boost_low".into(),
        RuleOverride {
            enabled: Some(false),
            cooldown_s: Some(30.0),
            ..RuleOverride::default()
        },
    );
    a.overrides.insert(
        "hull_half".into(),
        RuleOverride {
            value: Some(0.4),
            rearm: Some(0.5),
            ..RuleOverride::default()
        },
    );
    a.overrides
        .insert("empty_one".into(), RuleOverride::default());
    set_alert_settings(&mut cfg, &a);
    let text = toml::to_string(&cfg).unwrap();
    let back: Config = toml::from_str(&text).unwrap();
    a.overrides.remove("empty_one");
    assert_eq!(alert_settings(&back), a, "{text}");
    assert!(settings(&back).enabled, "{text}");
    // set_settings keeps the alert keys.
    let mut back = back;
    set_settings(&mut back, &Settings::default()).unwrap();
    assert_eq!(alert_settings(&back), a);
    // No overrides: the table goes.
    a.overrides.clear();
    set_alert_settings(&mut back, &a);
    assert!(!toml::to_string(&back).unwrap().contains("telemetry_alerts"));
}

#[test]
fn bad_alert_values_read_as_defaults() {
    let cfg: Config = toml::from_str(
        "[experimental]\ntelemetry_notifications = \"loud\"\ntelemetry_speech = 1\n\
         [experimental.telemetry_alerts]\n\"Bad-Id\" = { enabled = false }\nboost_low = 3\n\
         hull_half = { cooldown_s = 20, value = \"x\" }\n",
    )
    .unwrap();
    let a = alert_settings(&cfg);
    assert_eq!(a.notifications, Notifications::Off);
    assert!(!a.speech);
    assert_eq!(a.overrides.len(), 1);
    assert_eq!(
        a.overrides["hull_half"],
        RuleOverride {
            cooldown_s: Some(20.0),
            ..RuleOverride::default()
        }
    );
    assert!(valid_rule_id("g_load") && !valid_rule_id("") && !valid_rule_id("A"));
}
