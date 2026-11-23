//! Live telemetry (experimental): Star Citizen's own local gRPC service,
//! `sc.game.telemetry.v1.GameTelemetryService`, read on `127.0.0.1:<port>`.
//!
//! A Rust port of the read path and the guarded trigger path of sc-telemetry (same owner,
//! MIT): `sctelemetry/schema.py`, `server.py` and `mock.py`. The game hosts the service only
//! when the player turns it on (`GRPC_CLIENT_SERVER_ENABLED`, `GRPC_CLIENT_SERVER_PORT`, and
//! `server.game.telemetry.enabled` in `user.cfg`; sc-telemetry `docs/install.md`).
//!
//! - Opt-in only, off by default, and offered only for a channel whose build is 5.0 or newer
//!   ([`offered`]): PTU 5.0 today, LIVE on its own once LIVE reaches 5.0. An unknown version
//!   is not offered.
//! - Nothing leaves the computer. The service is plaintext and unauthenticated, so the
//!   client connects to loopback only and refuses any other host ([`client::LoopbackAddr`]).
//! - No CIG-derived file is in the repo: no `.proto`, no extracted descriptor, no
//!   recording. The schema is read at run time from the user's own `StarCitizen.exe`
//!   ([`schema`]) and cached in the app's cache folder; tests and the UI preview use the
//!   synthetic schema in [`mock`].
//!
//! Read: [`client::TelemetryClient::stream`] and [`live::run`] (reconnect with backoff, a
//! [`live::LinkStatus`], frames throttled for the UI as [`frames::Frame`]: a small stable
//! [`frames::Snapshot`] plus the raw frame as JSON). Write: `TriggerGameAction` for the
//! actions the game itself lists as triggerable, and only through a [`trigger::TriggerGate`]
//! the user has switched on separately.

pub mod client;
pub mod codec;
pub mod frames;
pub mod json;
pub mod live;
pub mod mock;
pub mod schema;
pub mod trigger;

use starorg_core::build::GameVersion;

/// Full name of the game's telemetry service.
pub const SERVICE: &str = "sc.game.telemetry.v1.GameTelemetryService";

/// The file that defines [`SERVICE`] (sc-telemetry `schema.py` `MARKER`).
pub const MARKER: &str = "sc/game/telemetry/v1/api.proto";

/// Port sc-telemetry's docs use for `GRPC_CLIENT_SERVER_PORT`. The game's own default is
/// unknown, so the player sets the port and the app asks for it.
pub const DEFAULT_PORT: u16 = 50051;

/// First game version with the telemetry service this crate supports (`TriggerGameAction`
/// and the vehicle fields arrived in PTU 5.0.0; sc-telemetry `docs/telemetry/api-spec.md` §7).
pub const MIN_GAME_VERSION: GameVersion = GameVersion {
    major: 5,
    minor: 0,
    patch: 0,
};

/// Whether live telemetry may be offered for a channel whose build has this version
/// (`BuildInfo::game_version`). `None` (version unknown) is not offered.
pub fn offered(game_version: Option<GameVersion>) -> bool {
    game_version.is_some_and(|v| v >= MIN_GAME_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(major: u32, minor: u32, patch: u32) -> Option<GameVersion> {
        Some(GameVersion {
            major,
            minor,
            patch,
        })
    }

    #[test]
    fn offered_from_5_0_only() {
        assert!(!offered(None));
        assert!(!offered(v(4, 10, 0)));
        assert!(!offered(v(4, 99, 9)));
        assert!(offered(v(5, 0, 0)));
        assert!(offered(v(5, 1, 2)));
        assert!(offered(v(6, 0, 0)));
    }
}
