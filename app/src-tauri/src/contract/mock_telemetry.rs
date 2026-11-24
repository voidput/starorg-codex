//! Live telemetry on the mock backend: the switches and `USER.cfg` lines in memory, and the
//! synthetic generator (`starorg_telemetry::mock`) in place of a game, so the screen and its
//! trigger buttons work without one. The mock's PTU runs 5.0 (offered); LIVE and HOTFIX run
//! 4.10 (not offered). Nothing is read from or written to disk, and no socket is opened.
//!
//! "Record sessions" keeps its recordings in memory ([`TelemetryHost::default`] has no
//! folder), starting with two synthetic ones ([`seed_recordings`]); a replay plays the
//! synthetic generator over a recording's length.
//! "Read Game.log events" feeds synthetic Game.log lines (`starorg_telemetry::gamelog::mock`,
//! fake handles) instead of a file.

use super::Backend;
use super::mock::MockBackend;
use super::telemetry::*;
use super::types::*;
use crate::telemetry::GameLogSource;
use crate::telemetry::TelemetryHost;
use std::collections::BTreeMap;
use std::sync::MutexGuard;

/// The mock's `USER.cfg` path per channel (shown only).
const CFG_ROOT: &str =
    "/home/pilot/Games/star-citizen/drive_c/Program Files/Roberts Space Industries/StarCitizen";

/// The mock's recordings folder (shown only; nothing is written there).
pub const RECORDINGS_DIR: &str = "/synthetic/data/telemetry/recordings";

/// Two synthetic recordings of the mock's PTU, oldest first (fixed times, for the samples).
pub fn seed_recordings() -> Vec<TelemetryRecording> {
    let rec = |start_ms: u64, duration_ms: u64| TelemetryRecording {
        name: format!("{}-PTU.jsonl", starorg_telemetry::record::stamp(start_ms)),
        bytes: 200 + (duration_ms / 250 + 1) * 1_800,
        started_ms: Some(start_ms),
        duration_ms: Some(duration_ms),
        channel: Some("PTU".into()),
        schema: Some("mock".into()),
    };
    vec![
        rec(1_794_900_000_000, 95_000),
        rec(1_795_100_000_000, 240_000),
    ]
}

/// The mock's join history file (shown only; the rows stay in memory).
pub const SESSIONS_DIR: &str = "/synthetic/data/telemetry/sessions";

/// Synthetic join history rows, oldest first (fixed times and fake shard ids, for the samples):
/// a stint that dropped, a rejoin of the same shard, a shard change and a quit.
pub fn seed_sessions() -> Vec<crate::contract::telemetry::TelemetryStint> {
    use starorg_telemetry::gamelog::{LeftReason, Stint, session::parse_shard};
    let row = |channel: &str,
               shard: &str,
               joined_ms: i64,
               mins: i64,
               reason: LeftReason,
               systems: &[&str]| {
        let (code, region, short) = parse_shard(shard);
        Stint {
            channel: Some(channel.into()),
            build: Some("0.0.0.0".into()),
            shard: shard.into(),
            shard_short: short,
            region_code: code,
            region,
            log_started: Some("Mon Nov 23 08:00:00 2026".into()),
            joined_ms,
            left_ms: Some(joined_ms + mins * 60_000),
            left_reason: Some(reason),
            uptime_s: Some((mins * 60) as f64),
            systems: systems.iter().map(|x| (*x).to_owned()).collect(),
            disconnects: u32::from(matches!(
                reason,
                LeftReason::Dropped | LeftReason::Disconnect
            )),
            dropped: reason == LeftReason::Dropped,
        }
    };
    vec![
        row(
            "LIVE",
            "mock_euw1a_0_004",
            1_794_990_000_000,
            42,
            LeftReason::Leave,
            &["Stanton"],
        ),
        row(
            "PTU",
            "mock_use1b_0_001",
            1_795_000_000_000,
            75,
            LeftReason::Dropped,
            &["Stanton", "Pyro"],
        ),
        row(
            "PTU",
            "mock_use1b_0_001",
            1_795_004_800_000,
            30,
            LeftReason::ShardChange,
            &["Pyro"],
        ),
        row(
            "PTU",
            "mock_use1b_0_002",
            1_795_006_600_000,
            55,
            LeftReason::Disconnect,
            &["Pyro", "Stanton"],
        ),
    ]
}

/// The mock backend's host: recordings and the join history in memory, seeded.
pub fn mock_host() -> TelemetryHost {
    let h = TelemetryHost::default();
    h.seed_mock_recordings(seed_recordings());
    h.seed_mock_sessions(seed_sessions());
    h
}

/// Which of the two lines the mock's `USER.cfg` of a channel holds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MockLines {
    pub telemetry: bool,
    pub triggers: bool,
}

#[derive(Debug, Clone)]
pub struct TelemetryMock {
    pub settings: TelemetrySettings,
    /// By `install/CHANNEL`.
    pub lines: BTreeMap<String, MockLines>,
    /// Notifications, speech and rule overrides (in memory).
    pub alerts: TelemetryAlertSettings,
}

impl Default for TelemetryMock {
    fn default() -> Self {
        Self {
            settings: starorg_core::telemetry::Settings::default().into(),
            lines: BTreeMap::new(),
            alerts: TelemetryAlertSettings::default(),
        }
    }
}

fn key(r: &ChannelRef) -> String {
    format!("{}/{}", r.install, r.channel.to_ascii_uppercase())
}

fn cfg_line(on: bool, m: &starorg_core::usercfg::ManagedLine) -> TelemetryCfgLine {
    TelemetryCfgLine {
        line: m.line.into(),
        state: if on {
            CvarState::Set
        } else {
            CvarState::Missing
        },
        value: None,
    }
}

fn version_of(b: &Build) -> Option<starorg_core::build::GameVersion> {
    b.game_version
        .as_deref()
        .or(b.branch.as_deref())
        .and_then(starorg_core::build::GameVersion::from_branch)
}

impl MockBackend {
    fn tele(&self) -> MutexGuard<'_, TelemetryMock> {
        self.telemetry_cfg.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn tele_channels(&self) -> Vec<(ChannelRef, Build)> {
        let Ok(o) = self.overview() else {
            return Vec::new();
        };
        o.installs
            .iter()
            .flat_map(|i| {
                i.channels.iter().map(|c| {
                    (
                        ChannelRef {
                            install: i.id.clone(),
                            channel: c.channel.clone(),
                        },
                        c.build.clone(),
                    )
                })
            })
            .collect()
    }

    fn tele_status(&self) -> TelemetryStatus {
        use starorg_core::usercfg::{TELEMETRY, TELEMETRY_TRIGGERS};
        let t = self.tele().clone();
        let channels = self
            .tele_channels()
            .into_iter()
            .map(|(r, b)| {
                let offered = offer(version_of(&b));
                let lines = t.lines.get(&key(&r)).copied().unwrap_or_default();
                TelemetryChannel {
                    user_cfg: format!("{CFG_ROOT}/{}/USER.cfg", r.channel),
                    telemetry_line: cfg_line(lines.telemetry, &TELEMETRY),
                    triggers_line: cfg_line(lines.triggers, &TELEMETRY_TRIGGERS),
                    install: r.install,
                    channel: r.channel,
                    game_version: b.game_version,
                    offered: offered.is_ok(),
                    reason: offered.err(),
                    exe_found: true,
                }
            })
            .collect();
        // The samples show the Linux help on every machine.
        let os = HelpOs::Linux;
        TelemetryStatus {
            settings: t.settings,
            channels,
            help_os: os,
            env_help: starorg_core::telemetry::env_help(os, t.settings.port),
            endpoint: starorg_core::telemetry::endpoint(t.settings.port).to_string(),
            link: self.telemetry.link(),
            streaming: self.telemetry.target(),
            session: self.telemetry.session(),
            schema: self.telemetry.schema(),
            ui_hz: starorg_telemetry::frames::UI_HZ,
            dangerous: crate::telemetry::dangerous(),
            mock: true,
            replay: self.telemetry.replay_state(),
            recording: self.telemetry.recording(),
            record_error: self.telemetry.record_error(),
        }
    }

    fn tele_offered(&self, r: &ChannelRef) -> CmdResult<()> {
        let b = self
            .tele_channels()
            .into_iter()
            .find(|(c, _)| c == r)
            .map(|(_, b)| b)
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::NotFound,
                    format!("No channel {} in install {}", r.channel, r.install),
                )
            })?;
        offer(version_of(&b)).map_err(|m| AppError::new(ErrorCode::Invalid, m))
    }
}

impl TelemetryBackend for MockBackend {
    fn telemetry_host(&self) -> &crate::telemetry::TelemetryHost {
        &self.telemetry
    }

    fn telemetry_status(&self) -> CmdResult<TelemetryStatus> {
        Ok(self.tele_status())
    }

    fn telemetry_configure(
        &self,
        req: TelemetryConfigureRequest,
    ) -> CmdResult<TelemetryConfigureResult> {
        use starorg_core::usercfg::{TELEMETRY, TELEMETRY_TRIGGERS};
        let s = req.settings;
        if s.port < starorg_core::telemetry::MIN_PORT {
            return Err(AppError::new(
                ErrorCode::Invalid,
                format!(
                    "port {} is below {}; the game accepts 1024 to 65535",
                    s.port,
                    starorg_core::telemetry::MIN_PORT
                ),
            ));
        }
        if s.enabled {
            let Some(t) = &req.target else {
                return Err(AppError::new(
                    ErrorCode::Invalid,
                    "Pick the channel whose USER.cfg gets the telemetry line.",
                ));
            };
            self.tele_offered(t)?;
        }
        let change = |on: bool, was: bool| match (on, was) {
            (true, true) => LineOutcome::AlreadySet,
            (true, false) => LineOutcome::Written,
            (false, true) => LineOutcome::Reverted,
            (false, false) => LineOutcome::NotOurs,
        };
        let (tl, gl) = {
            let mut m = self.tele();
            // "Record sessions" has its own command: a configure keeps it.
            let s = TelemetrySettings {
                record: m.settings.record,
                ..s
            };
            m.settings = s;
            let was = req
                .target
                .as_ref()
                .and_then(|t| m.lines.get(&key(t)).copied())
                .unwrap_or_default();
            if !s.enabled {
                // Off: every line goes.
                m.lines.clear();
            }
            match &req.target {
                Some(t) => {
                    let now = MockLines {
                        telemetry: s.enabled,
                        triggers: s.triggers_active(),
                    };
                    m.lines.insert(key(t), now);
                    let line = |l: &str, o| TelemetryLineChange {
                        line: l.into(),
                        outcome: o,
                        value: None,
                    };
                    (
                        Some(line(TELEMETRY.line, change(now.telemetry, was.telemetry))),
                        Some(line(
                            TELEMETRY_TRIGGERS.line,
                            change(now.triggers, was.triggers),
                        )),
                    )
                }
                None => (None, None),
            }
        };
        if s.enabled {
            let src = s.gamelog_active().then_some(GameLogSource::Mock);
            self.telemetry
                .set_gamelog(src, s.people_active(), s.sessions_active());
        } else {
            self.telemetry.stop();
            self.telemetry.set_record(false);
        }
        self.log(
            LogLevel::Info,
            None,
            req.target.as_ref().map(|t| t.channel.as_str()),
            &if s.enabled {
                format!(
                    "Live telemetry on (port {}, game actions {})",
                    s.port,
                    if s.triggers { "allowed" } else { "off" }
                )
            } else {
                "Live telemetry off; its USER.cfg lines were released".into()
            },
        );
        Ok(TelemetryConfigureResult {
            status: self.tele_status(),
            telemetry_line: tl,
            triggers_line: gl,
            warnings: Vec::new(),
        })
    }

    fn telemetry_start(&self, req: ChannelRef, sink: Sink) -> CmdResult<TelemetryStatus> {
        if !self.tele().settings.enabled {
            return Err(AppError::new(
                ErrorCode::Invalid,
                "Turn on Live telemetry under Settings -> Experimental first.",
            ));
        }
        self.tele_offered(&req)?;
        let rec = self.tele().settings.record_active();
        self.telemetry.set_record(rec);
        let alerts = self.tele().alerts.clone();
        self.telemetry.set_alerts(&alerts);
        self.telemetry.start_mock(req, sink);
        let s = self.tele().settings;
        if s.gamelog_active() {
            self.telemetry.set_gamelog(
                Some(GameLogSource::Mock),
                s.people_active(),
                s.sessions_active(),
            );
        }
        Ok(self.tele_status())
    }

    fn telemetry_stop(&self) -> CmdResult<TelemetryStatus> {
        self.telemetry.stop();
        Ok(self.tele_status())
    }

    fn telemetry_actions(&self) -> CmdResult<TelemetryActions> {
        let on = self.tele().settings.triggers_active();
        Ok(self.telemetry.actions(on))
    }

    fn telemetry_trigger(&self, req: TelemetryTriggerRequest) -> CmdResult<TelemetryTriggerResult> {
        let on = self.tele().settings.triggers_active();
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
        let active = {
            let mut m = self.tele();
            m.settings.record = req.on;
            m.settings.record_active()
        };
        self.telemetry.set_record(active);
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
        Ok(self.tele_status())
    }

    fn telemetry_recordings(&self) -> CmdResult<TelemetryRecordings> {
        Ok(crate::telemetry::recordings_view(
            &self.telemetry,
            RECORDINGS_DIR.into(),
        ))
    }

    fn telemetry_recording_delete(
        &self,
        req: TelemetryRecordingRef,
    ) -> CmdResult<TelemetryRecordings> {
        self.telemetry
            .delete_recording(&req.name)
            .map_err(|m| AppError::new(ErrorCode::Busy, m))?;
        self.telemetry_recordings()
    }

    fn telemetry_replay_start(
        &self,
        req: TelemetryRecordingRef,
        sink: Sink,
    ) -> CmdResult<TelemetryStatus> {
        if !self.tele().settings.enabled {
            return Err(AppError::new(
                ErrorCode::Invalid,
                "Turn on Live telemetry under Settings -> Experimental first.",
            ));
        }
        self.telemetry
            .start_replay(&req.name, sink)
            .map_err(|m| AppError::new(ErrorCode::Invalid, m))?;
        Ok(self.tele_status())
    }

    fn telemetry_replay_control(&self, req: TelemetryReplayControl) -> CmdResult<TelemetryStatus> {
        self.telemetry
            .replay_control(req.playing, req.seek_ms, req.speed)
            .map_err(|m| AppError::new(ErrorCode::Invalid, m))?;
        Ok(self.tele_status())
    }

    fn telemetry_alerts(&self) -> CmdResult<TelemetryAlerts> {
        let a = self.tele().alerts.clone();
        Ok(crate::telemetry::alerts_view(&self.telemetry, &a))
    }

    fn telemetry_alerts_configure(
        &self,
        req: TelemetryAlertSettings,
    ) -> CmdResult<TelemetryAlerts> {
        let a = crate::telemetry::alerts_normalize(&req)
            .map_err(|m| AppError::new(ErrorCode::Invalid, m))?;
        self.tele().alerts = a.clone();
        self.telemetry.set_alerts(&a);
        Ok(crate::telemetry::alerts_view(&self.telemetry, &a))
    }

    fn telemetry_alert_test(&self, req: TelemetryAlertTestRequest) -> CmdResult<TelemetryAlert> {
        let a = self.tele().alerts.clone();
        crate::telemetry::alert_test(&a, &req.rule).map_err(|(c, m)| AppError::new(c, m))
    }
}
