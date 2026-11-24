//! Live telemetry on the real backend (experimental; docs/plans/specs/live-telemetry.md):
//! the `[experimental]` switches in `config.toml`, the channel's `USER.cfg` lines through
//! `starorg_core::telemetry`, and the stream over [`crate::telemetry::TelemetryHost`].
//!
//! The schema comes from the channel's own `Bin64/StarCitizen.exe` the first time a build is
//! streamed (a scan of a few seconds) and is cached in `<cache>/telemetry/` (Settings >
//! Storage counts it; three builds are kept). Only `127.0.0.1` is ever contacted.

use super::convert::{err, path_text};
use super::{Found, RealBackend, io};
use crate::contract::telemetry::*;
use crate::contract::types::*;
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
        }
    }

    /// The channel, refused (`invalid`) unless its build is offered.
    fn telemetry_target(&self, r: &ChannelRef) -> CmdResult<(Found, Channel)> {
        let f = self.resolve(r)?;
        let ch = f.channel(&r.channel)?.clone();
        gate::eligible(&ch).map_err(|e| err(ErrorCode::Invalid, e.to_string()))?;
        Ok((f, ch))
    }

    /// Release both lines of every channel (switching off). Failures become warnings.
    fn telemetry_release_all(&self) -> Vec<String> {
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
                if let Err(e) = gate::configure_user_cfg(self.dirs(), &ch.dir, &me, &off, false) {
                    warnings.push(format!("{}: USER.cfg not changed: {e}", ch.name));
                }
            }
        }
        warnings
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
            (Some(t), false) => {
                let f = self.resolve(t)?;
                let ch = f.channel(&t.channel)?.clone();
                Some((f, ch))
            }
            (None, true) => {
                return Err(err(
                    ErrorCode::Invalid,
                    "Pick the channel whose USER.cfg gets the telemetry line.",
                ));
            }
            (None, false) => None,
        };
        let mut cfg = Config::load(self.dirs()).map_err(io)?;
        gate::set_settings(&mut cfg, &s).map_err(|m| err(ErrorCode::Invalid, m))?;
        cfg.save(self.dirs()).map_err(io)?;
        let mut warnings = Vec::new();
        let (mut tl, mut gl) = (None, None);
        if let Some((mut f, ch)) = target {
            self.register(&mut f)?;
            let me = Target::new(&f.install, f.id.clone(), &ch).channel_ref();
            let c = gate::configure_user_cfg(self.dirs(), &ch.dir, &me, &s, req.replace_other)
                .map_err(|e| err(ErrorCode::Busy, format!("USER.cfg not changed: {e}")))?;
            tl = Some(TelemetryLineChange::of(TELEMETRY.line, &c.telemetry));
            gl = Some(TelemetryLineChange::of(
                TELEMETRY_TRIGGERS.line,
                &c.triggers,
            ));
        }
        if !s.enabled {
            self.telemetry.stop();
            warnings = self.telemetry_release_all();
        }
        let channel = req.target.as_ref().map(|t| t.channel.as_str());
        self.log(
            LogLevel::Info,
            None,
            channel,
            &if s.enabled {
                format!(
                    "Live telemetry on (127.0.0.1:{}, game actions {})",
                    s.port,
                    if s.triggers { "allowed" } else { "off" }
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
        Ok(self.telemetry_view())
    }

    fn telemetry_stop(&self) -> CmdResult<TelemetryStatus> {
        self.telemetry.stop();
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
}
