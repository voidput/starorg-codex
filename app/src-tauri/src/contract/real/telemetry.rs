//! Live telemetry on the real backend (experimental; docs/plans/specs/live-telemetry.md):
//! the `[experimental]` switches in `config.toml`, the channel's `USER.cfg` lines through
//! `starorg_core::telemetry`, and the stream over [`crate::telemetry::TelemetryHost`].
//!
//! The schema comes from the channel's own `Bin64/StarCitizen.exe` the first time a build is
//! streamed (a scan of a few seconds) and is cached in `<cache>/telemetry/` (Settings >
//! Storage counts it; three builds are kept). Only `127.0.0.1` is ever contacted.
//!
//! With "Record sessions" on, streams are recorded to `<data>/telemetry/recordings/`
//! (`starorg_core::storage::telemetry_recordings_dir`, Settings > Storage counts it, capped by
//! `starorg_telemetry::record::Caps`) and can be replayed into the screen.
//!
//! "Read Game.log events" follows the streamed channel's own `Game.log` (read only, the
//! bytes added since the last poll); "People" keeps the handles it names in memory. Neither
//! writes a file, and no handle goes into the Activity log.

use super::convert::{err, path_text};
use super::{Found, RealBackend, io};
use crate::contract::telemetry::*;
use crate::contract::types::*;
use crate::telemetry::GameLogSource;
use starorg_core::apply::Target;
use starorg_core::config::Config;
use starorg_core::discovery::Channel;
use starorg_core::telemetry::{self as gate, Settings};
use starorg_core::usercfg::{self, TELEMETRY, TELEMETRY_TRIGGERS};
use starorg_telemetry::client::LoopbackAddr;
use starorg_telemetry::schema::{self, CacheKey};
use std::sync::Arc;

impl RealBackend {
    fn telemetry_settings(&self) -> Settings {
        gate::settings(&self.config())
    }

    fn telemetry_alert_settings(&self) -> gate::AlertSettings {
        gate::alert_settings(&self.config())
    }

    fn telemetry_channel(f: &Found, ch: &Channel) -> TelemetryChannel {
        let cfg = usercfg::locate(&ch.dir).ok();
        let text = cfg.as_ref().and_then(|t| usercfg::read(t).ok()).flatten();
        let offered = gate::eligible(ch).map_err(|e| e.to_string());
        TelemetryChannel {
            install: f.ui_id(),
            channel: ch.name.clone(),
            game_version: ch
                .build
                .as_ref()
                .and_then(|b| b.game_version)
                .map(|v| v.to_string()),
            offered: offered.is_ok(),
            reason: offered.err(),
            user_cfg: cfg
                .map(|t| path_text(&t.path))
                .unwrap_or_else(|| path_text(&ch.dir.join("USER.cfg"))),
            telemetry_line: TelemetryCfgLine::of(text.as_deref(), &TELEMETRY),
            triggers_line: TelemetryCfgLine::of(text.as_deref(), &TELEMETRY_TRIGGERS),
            exe_found: gate::game_exe(ch).ok().flatten().is_some(),
        }
    }

    fn telemetry_view(&self) -> TelemetryStatus {
        let s: TelemetrySettings = self.telemetry_settings().into();
        let channels = self
            .installs()
            .iter()
            .flat_map(|f| {
                f.install
                    .channels
                    .iter()
                    .map(|ch| Self::telemetry_channel(f, ch))
            })
            .collect();
        let os = gate::HelpOs::current();
        TelemetryStatus {
            settings: s,
            channels,
            help_os: os,
            env_help: gate::env_help(os, s.port),
            endpoint: gate::endpoint(s.port).to_string(),
            link: self.telemetry.link(),
            streaming: self.telemetry.target(),
            session: self.telemetry.session(),
            schema: self.telemetry.schema(),
            ui_hz: starorg_telemetry::frames::UI_HZ,
            dangerous: crate::telemetry::dangerous(),
            mock: false,
            replay: self.telemetry.replay_state(),
            recording: self.telemetry.recording(),
            record_error: self.telemetry.record_error(),
        }
    }

    fn telemetry_recordings_view(&self) -> TelemetryRecordings {
        crate::telemetry::recordings_view(
            &self.telemetry,
            path_text(&starorg_core::storage::telemetry_recordings_dir(
                self.dirs(),
            )),
        )
    }

    /// The channel, refused (`invalid`) unless its build is offered.
    fn telemetry_target(&self, r: &ChannelRef) -> CmdResult<(Found, Channel)> {
        let f = self.resolve(r)?;
        let ch = f.channel(&r.channel)?.clone();
        gate::eligible(&ch).map_err(|e| err(ErrorCode::Invalid, e.to_string()))?;
        Ok((f, ch))
    }

    /// Release both lines of every registered channel but `keep` (`<install-id>/<CHANNEL>`):
    /// all of them when switching off, the others when the lines move to `keep`. Only one
    /// channel carries the lines, so neither line stays in a USER.cfg the app no longer
    /// streams from. Failures become warnings.
    fn telemetry_release_others(&self, keep: Option<&str>) -> Vec<String> {
        let off = Settings {
            enabled: false,
            ..Settings::default()
        };
        let mut warnings = Vec::new();
        for f in self.installs() {
            // A ledger names registered installs only (`<install-id>/<CHANNEL>`).
            if f.id.is_none() {
                continue;
            }
            for ch in &f.install.channels {
                let me = Target::new(&f.install, f.id.clone(), ch).channel_ref();
                if keep == Some(me.as_str()) {
                    continue;
                }
                if let Err(e) = gate::configure_user_cfg(self.dirs(), &ch.dir, &me, &off, false) {
                    warnings.push(format!("{}: USER.cfg not changed: {e}", ch.name));
                }
            }
        }
        warnings
    }

    /// Undo what a failed configure may have done to the target's USER.cfg before the error:
    /// a line `before` did not want is released again (best effort; the error is reported).
    fn telemetry_cfg_rollback(&self, ch: &Channel, me: &str, before: &Settings, now: &Settings) {
        let wanted_now =
            now.enabled && !before.enabled || now.triggers_active() && !before.triggers_active();
        if wanted_now {
            let _ = gate::configure_user_cfg(self.dirs(), &ch.dir, me, before, false);
        }
    }

    /// Start, keep or stop the Game.log reader of the running stream to match `s`.
    fn telemetry_gamelog_sync(&self, s: &Settings) {
        let source = s
            .gamelog_active()
            .then(|| self.telemetry.target())
            .flatten()
            .and_then(|t| {
                let f = self.resolve(&t).ok()?;
                Some(GameLogSource::Channel(
                    f.channel(&t.channel).ok()?.dir.clone(),
                ))
            });
        self.telemetry.set_gamelog(source, s.people_active());
    }

    /// The build's schema: from `<cache>/telemetry/` when there, else scanned from the exe.
    fn telemetry_schema(&self, ch: &Channel) -> Result<schema::Schema, String> {
        let dir = starorg_core::storage::telemetry_dir(self.dirs());
        let exe = gate::game_exe(ch).map_err(|e| e.to_string())?;
        let key = ch
            .build
            .as_ref()
            .and_then(CacheKey::for_build)
            .or_else(|| exe.as_deref().and_then(|e| CacheKey::for_exe(e).ok()));
        let Some(key) = key else {
            return Err(format!(
                "{} has no build_manifest.id and no Bin64/StarCitizen.exe to read the telemetry schema from",
                ch.name
            ));
        };
        if let Some(s) = schema::load_cached(&dir, &key) {
            return Ok(s);
        }
        let Some(exe) = exe else {
            return Err(format!(
                "{} has no Bin64/StarCitizen.exe to read the telemetry schema from",
                ch.name
            ));
        };
        let s = schema::load_or_extract(&exe, &dir, &key).map_err(|e| e.to_string())?;
        self.sizes_changed();
        Ok(s)
    }
}

impl TelemetryBackend for RealBackend {
    fn telemetry_host(&self) -> &crate::telemetry::TelemetryHost {
        &self.telemetry
    }

    fn telemetry_status(&self) -> CmdResult<TelemetryStatus> {
        Ok(self.telemetry_view())
    }

    fn telemetry_configure(
        &self,
        req: TelemetryConfigureRequest,
    ) -> CmdResult<TelemetryConfigureResult> {
        let s: Settings = req.settings.into();
        // The channel first: nothing is saved for one that is not offered.
        let target = match (&req.target, s.enabled) {
            (Some(t), true) => Some(self.telemetry_target(t)?),
            // Off never fails on the channel: a channel gone since is released with the rest.
            (Some(t), false) => self.resolve(t).ok().and_then(|f| {
                let ch = f.channel(&t.channel).ok()?.clone();
                Some((f, ch))
            }),
            (None, true) => {
                return Err(err(
                    ErrorCode::Invalid,
                    "Pick the channel whose USER.cfg gets the telemetry line.",
                ));
            }
            (None, false) => None,
        };
        let mut cfg = Config::load(self.dirs()).map_err(io)?;
        let before = gate::settings(&cfg);
        // "Record sessions" has its own command: a configure keeps what is saved.
        let s = Settings {
            record: before.record,
            ..s
        };
        gate::set_settings(&mut cfg, &s).map_err(|m| err(ErrorCode::Invalid, m))?;
        let (mut tl, mut gl) = (None, None);
        let warnings = if s.enabled {
            let Some((mut f, ch)) = target else {
                unreachable!("refused above without a target");
            };
            // USER.cfg first, `config.toml` only once it took: a failed write (lock busy,
            // file not ours, too big) leaves the switches as they were, so the switch and
            // the file never disagree.
            if f.id.is_none() {
                // Registering saves `config.toml` itself: load it again, or the save below
                // would drop the install it just added.
                self.register(&mut f)?;
                cfg = Config::load(self.dirs()).map_err(io)?;
                gate::set_settings(&mut cfg, &s).map_err(|m| err(ErrorCode::Invalid, m))?;
            }
            let me = Target::new(&f.install, f.id.clone(), &ch).channel_ref();
            let c = gate::configure_user_cfg(self.dirs(), &ch.dir, &me, &s, req.replace_other)
                .map_err(|e| {
                    self.telemetry_cfg_rollback(&ch, &me, &before, &s);
                    err(ErrorCode::Busy, format!("USER.cfg not changed: {e}"))
                })?;
            if let Err(e) = cfg.save(self.dirs()) {
                self.telemetry_cfg_rollback(&ch, &me, &before, &s);
                return Err(io(e));
            }
            tl = Some(TelemetryLineChange::of(TELEMETRY.line, &c.telemetry));
            gl = Some(TelemetryLineChange::of(
                TELEMETRY_TRIGGERS.line,
                &c.triggers,
            ));
            // The lines live in the target's USER.cfg only: a changed channel must not
            // leave them (the triggers line above all) in another one.
            self.telemetry_release_others(Some(&me))
        } else {
            // Off is saved first and always wins: the stream and the trigger gate read
            // `config.toml`, and a USER.cfg that cannot be changed is only a warning.
            cfg.save(self.dirs()).map_err(io)?;
            self.telemetry.stop();
            self.telemetry.set_record(false);
            self.sizes_changed();
            let mut keep = None;
            let mut warnings = Vec::new();
            if let Some((f, ch)) = &target
                && f.id.is_some()
            {
                let me = Target::new(&f.install, f.id.clone(), ch).channel_ref();
                match gate::configure_user_cfg(self.dirs(), &ch.dir, &me, &s, false) {
                    Ok(c) => {
                        tl = Some(TelemetryLineChange::of(TELEMETRY.line, &c.telemetry));
                        gl = Some(TelemetryLineChange::of(
                            TELEMETRY_TRIGGERS.line,
                            &c.triggers,
                        ));
                    }
                    Err(e) => warnings.push(format!("{}: USER.cfg not changed: {e}", ch.name)),
                }
                keep = Some(me);
            }
            warnings.extend(self.telemetry_release_others(keep.as_deref()));
            warnings
        };
        if s.enabled {
            self.telemetry_gamelog_sync(&s);
        }
        let channel = req.target.as_ref().map(|t| t.channel.as_str());
        self.log(
            LogLevel::Info,
            None,
            channel,
            &if s.enabled {
                format!(
                    "Live telemetry on (127.0.0.1:{}, game actions {}, Game.log events {}{})",
                    s.port,
                    if s.triggers { "allowed" } else { "off" },
                    if s.gamelog { "on" } else { "off" },
                    if s.people_active() { ", people on" } else { "" }
                )
            } else {
                "Live telemetry off; its USER.cfg lines were released".into()
            },
        );
        for w in &warnings {
            self.log(LogLevel::Warn, None, None, w);
        }
        Ok(TelemetryConfigureResult {
            status: self.telemetry_view(),
            telemetry_line: tl,
            triggers_line: gl,
            warnings,
        })
    }

    fn telemetry_start(&self, req: ChannelRef, sink: Sink) -> CmdResult<TelemetryStatus> {
        let s = self.telemetry_settings();
        if !s.enabled {
            return Err(err(
                ErrorCode::Invalid,
                "Turn on Live telemetry under Settings -> Experimental first.",
            ));
        }
        let (_, ch) = self.telemetry_target(&req)?;
        let addr = LoopbackAddr::from_socket_addr(gate::endpoint(s.port))
            .map_err(|e| err(ErrorCode::Invalid, e.to_string()))?;
        self.telemetry.set_record(s.record_active());
        for w in self.telemetry.set_alerts(&self.telemetry_alert_settings()) {
            self.log(
                LogLevel::Warn,
                None,
                None,
                &format!("Alert rule override skipped: {w}"),
            );
        }
        match self.telemetry_schema(&ch) {
            Ok(schema) => {
                self.log(
                    LogLevel::Info,
                    None,
                    Some(&ch.name),
                    &format!(
                        "Live telemetry: reading 127.0.0.1:{} (schema {})",
                        s.port,
                        schema.key()
                    ),
                );
                self.telemetry.start_game(req, Arc::new(schema), addr, sink);
            }
            Err(reason) => {
                self.log(
                    LogLevel::Warn,
                    None,
                    Some(&ch.name),
                    &format!("Live telemetry: no schema: {reason}"),
                );
                self.telemetry.no_schema(req, reason, &sink);
            }
        }
        // Game.log needs no schema: it runs with or without the stream.
        if s.gamelog_active() {
            self.telemetry.set_gamelog(
                Some(GameLogSource::Channel(ch.dir.clone())),
                s.people_active(),
            );
            self.log(
                LogLevel::Info,
                None,
                Some(&ch.name),
                "Live telemetry: reading Game.log events",
            );
        }
        Ok(self.telemetry_view())
    }

    fn telemetry_stop(&self) -> CmdResult<TelemetryStatus> {
        if let Some(t) = self.telemetry.target() {
            self.log(
                LogLevel::Info,
                None,
                Some(&t.channel),
                "Live telemetry: stopped",
            );
        }
        self.telemetry.stop();
        self.sizes_changed();
        Ok(self.telemetry_view())
    }

    fn telemetry_actions(&self) -> CmdResult<TelemetryActions> {
        let on = self.telemetry_settings().triggers_active();
        Ok(self.telemetry.actions(on))
    }

    fn telemetry_trigger(&self, req: TelemetryTriggerRequest) -> CmdResult<TelemetryTriggerResult> {
        let on = self.telemetry_settings().triggers_active();
        let r = self
            .telemetry
            .trigger(&req.action, req.hold_ms, req.confirmed, on);
        if let Some(o) = &r.outcome {
            self.log(
                LogLevel::Info,
                None,
                None,
                &format!("Game action {}: {}", req.action, o.status.as_str()),
            );
        }
        Ok(r)
    }

    fn telemetry_history(&self, req: TelemetryHistoryRequest) -> CmdResult<TelemetryHistory> {
        Ok(self.telemetry.history(req.epoch, req.since_ms))
    }

    fn telemetry_record(&self, req: TelemetryRecordRequest) -> CmdResult<TelemetryStatus> {
        let mut cfg = Config::load(self.dirs()).map_err(io)?;
        let s = Settings {
            record: req.on,
            ..gate::settings(&cfg)
        };
        gate::set_settings(&mut cfg, &s).map_err(|m| err(ErrorCode::Invalid, m))?;
        cfg.save(self.dirs()).map_err(io)?;
        let was = self.telemetry.recording();
        self.telemetry.set_record(s.record_active());
        if was.is_some() && !req.on {
            self.sizes_changed();
        }
        self.log(
            LogLevel::Info,
            None,
            None,
            if req.on {
                "Live telemetry: Record sessions on"
            } else {
                "Live telemetry: Record sessions off"
            },
        );
        Ok(self.telemetry_view())
    }

    fn telemetry_recordings(&self) -> CmdResult<TelemetryRecordings> {
        Ok(self.telemetry_recordings_view())
    }

    fn telemetry_recording_delete(
        &self,
        req: TelemetryRecordingRef,
    ) -> CmdResult<TelemetryRecordings> {
        self.telemetry
            .delete_recording(&req.name)
            .map_err(|m| err(ErrorCode::Busy, m))?;
        self.sizes_changed();
        self.log(
            LogLevel::Info,
            None,
            None,
            &format!("Live telemetry: deleted recording {}", req.name),
        );
        Ok(self.telemetry_recordings_view())
    }

    fn telemetry_replay_start(
        &self,
        req: TelemetryRecordingRef,
        sink: Sink,
    ) -> CmdResult<TelemetryStatus> {
        self.telemetry
            .start_replay(&req.name, sink)
            .map_err(|m| err(ErrorCode::Invalid, m))?;
        self.sizes_changed();
        self.log(
            LogLevel::Info,
            None,
            None,
            &format!("Live telemetry: replaying {}", req.name),
        );
        Ok(self.telemetry_view())
    }

    fn telemetry_replay_control(&self, req: TelemetryReplayControl) -> CmdResult<TelemetryStatus> {
        self.telemetry
            .replay_control(req.playing, req.seek_ms, req.speed)
            .map_err(|m| err(ErrorCode::Invalid, m))?;
        Ok(self.telemetry_view())
    }

    fn telemetry_alerts(&self) -> CmdResult<TelemetryAlerts> {
        Ok(crate::telemetry::alerts_view(
            &self.telemetry,
            &self.telemetry_alert_settings(),
        ))
    }

    fn telemetry_alerts_configure(
        &self,
        req: TelemetryAlertSettings,
    ) -> CmdResult<TelemetryAlerts> {
        let a = crate::telemetry::alerts_normalize(&req).map_err(|m| err(ErrorCode::Invalid, m))?;
        let mut cfg = Config::load(self.dirs()).map_err(io)?;
        let before = gate::alert_settings(&cfg);
        gate::set_alert_settings(&mut cfg, &a);
        cfg.save(self.dirs()).map_err(io)?;
        self.telemetry.set_alerts(&a);
        if before.notifications != a.notifications || before.speech != a.speech {
            self.log(
                LogLevel::Info,
                None,
                None,
                &format!(
                    "Live telemetry notifications: {} (speech {})",
                    a.notifications.as_str(),
                    if a.speech { "on" } else { "off" }
                ),
            );
        }
        Ok(crate::telemetry::alerts_view(&self.telemetry, &a))
    }

    fn telemetry_alert_test(&self, req: TelemetryAlertTestRequest) -> CmdResult<TelemetryAlert> {
        crate::telemetry::alert_test(&self.telemetry_alert_settings(), &req.rule)
            .map_err(|(code, m)| err(code, m))
    }
}
