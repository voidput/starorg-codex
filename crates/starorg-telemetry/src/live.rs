//! The read loop: connect, stream, reconnect with backoff (sc-telemetry `server.py`
//! `run_grpc`), with a [`LinkStatus`] for the UI and frames throttled to
//! [`crate::frames::UI_HZ`].
//!
//! The game accepts the stream before login but sends no frames until the player has
//! spawned, and ends it with `UNAVAILABLE` when it exits (api-spec §3.1). So a connected
//! stream without frames is normal, and an ended one is retried: after [`BACKOFF_MIN`],
//! doubling to [`BACKOFF_MAX`], back to the minimum once a frame arrives.

use crate::client::{ClientError, LoopbackAddr, TelemetryClient};
use crate::frames::{Frame, Snapshot, Throttle, UI_INTERVAL};
use crate::json;
use crate::schema::Schema;
use prost_reflect::DynamicMessage;
use serde::Serialize;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub const BACKOFF_MIN: Duration = Duration::from_secs(1);
pub const BACKOFF_MAX: Duration = Duration::from_secs(10);

/// The state of live telemetry, for the UI. Serialized as `{"state": "...", ...}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LinkStatus {
    /// Off (the default), or not offered for this build.
    Disabled,
    /// No schema: the exe could not be read, or holds no telemetry service.
    NoSchema {
        reason: String,
    },
    Connecting,
    /// The stream is open; frames arrive once the player has spawned.
    Connected,
    /// Nothing listens on the port, or the game closed the stream (it exited).
    GameNotRunning,
    /// Any other failure; retried.
    Unreachable {
        reason: String,
    },
}

/// What [`run`] reports.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum LiveEvent {
    Status(LinkStatus),
    Frame(Box<Frame>),
}

#[derive(Debug, Clone)]
pub struct LiveOptions {
    /// Least time between two frames to the UI.
    pub ui_interval: Duration,
    pub backoff_min: Duration,
    pub backoff_max: Duration,
}

impl Default for LiveOptions {
    fn default() -> Self {
        Self {
            ui_interval: UI_INTERVAL,
            backoff_min: BACKOFF_MIN,
            backoff_max: BACKOFF_MAX,
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn to_frame(seq: u64, msg: &DynamicMessage) -> Frame {
    let raw = json::to_json(msg);
    Frame {
        seq,
        received_ms: now_ms(),
        snapshot: Snapshot::from_json(&raw),
        raw,
    }
}

fn status_of(e: &ClientError) -> LinkStatus {
    if e.game_gone() {
        LinkStatus::GameNotRunning
    } else {
        LinkStatus::Unreachable {
            reason: e.to_string(),
        }
    }
}

/// Stream from `addr` until `sink` returns `false` (or the future is dropped). Statuses are
/// reported on change only; frames at most once per `opts.ui_interval`, the latest one
/// winning, so the last state before a pause still arrives.
pub async fn run(
    schema: Arc<Schema>,
    addr: LoopbackAddr,
    opts: LiveOptions,
    mut sink: impl FnMut(LiveEvent) -> bool + Send,
) {
    let mut last_status: Option<LinkStatus> = None;
    let mut status = |s: LinkStatus, sink: &mut dyn FnMut(LiveEvent) -> bool| -> bool {
        if last_status.as_ref() == Some(&s) {
            return true;
        }
        last_status = Some(s.clone());
        sink(LiveEvent::Status(s))
    };
    let mut backoff = opts.backoff_min;
    loop {
        if !status(LinkStatus::Connecting, &mut sink) {
            return;
        }
        let failed = match TelemetryClient::connect(schema.clone(), addr).await {
            Err(e) => status_of(&e),
            Ok(mut client) => match client.stream().await {
                Err(e) => status_of(&e),
                Ok(mut stream) => {
                    if !status(LinkStatus::Connected, &mut sink) {
                        return;
                    }
                    let mut throttle = Throttle::new(opts.ui_interval);
                    let mut seq = 0u64;
                    let mut pending: Option<(u64, DynamicMessage)> = None;
                    loop {
                        let wake = tokio::time::Instant::from_std(throttle.next_at(Instant::now()));
                        tokio::select! {
                            msg = stream.message() => match msg {
                                Ok(Some(m)) => {
                                    seq += 1;
                                    backoff = opts.backoff_min;
                                    if throttle.admit(Instant::now()) {
                                        pending = None;
                                        if !sink(LiveEvent::Frame(Box::new(to_frame(seq, &m)))) {
                                            return;
                                        }
                                    } else {
                                        pending = Some((seq, m));
                                    }
                                }
                                Ok(None) => break LinkStatus::GameNotRunning,
                                Err(s) => break status_of(&ClientError::from(s)),
                            },
                            () = tokio::time::sleep_until(wake), if pending.is_some() => {
                                if let Some((n, m)) = pending.take() {
                                    throttle.admit(Instant::now());
                                    if !sink(LiveEvent::Frame(Box::new(to_frame(n, &m)))) {
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }
            },
        };
        if !status(failed, &mut sink) {
            return;
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(opts.backoff_max);
    }
}

/// A [`run`] on the current Tokio runtime, sending into a channel. Stops when the receiver
/// is dropped, on [`LiveHandle::stop`], or when the handle is dropped.
pub fn spawn(
    schema: Arc<Schema>,
    addr: LoopbackAddr,
    opts: LiveOptions,
    capacity: usize,
) -> (LiveHandle, mpsc::Receiver<LiveEvent>) {
    let (tx, rx) = mpsc::channel(capacity.max(1));
    let task = tokio::spawn(run(schema, addr, opts, move |ev| match tx.try_send(ev) {
        Ok(()) => true,
        // A slow UI loses frames, never the loop; a closed receiver ends it.
        Err(mpsc::error::TrySendError::Full(_)) => true,
        Err(mpsc::error::TrySendError::Closed(_)) => false,
    }));
    (LiveHandle { task }, rx)
}

/// Stops the loop when dropped.
#[derive(Debug)]
pub struct LiveHandle {
    task: JoinHandle<()>,
}

impl LiveHandle {
    pub fn stop(self) {
        self.task.abort();
    }

    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }
}

impl Drop for LiveHandle {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_json_shape() {
        assert_eq!(
            serde_json::to_value(LinkStatus::Unreachable { reason: "x".into() }).unwrap(),
            serde_json::json!({"state": "unreachable", "reason": "x"})
        );
        assert_eq!(
            serde_json::to_value(LinkStatus::GameNotRunning).unwrap(),
            serde_json::json!({"state": "game_not_running"})
        );
    }

    #[tokio::test]
    async fn retries_with_backoff_while_nothing_listens() {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let addr = LoopbackAddr::localhost(port).unwrap();
        let mut seen = Vec::new();
        let mut connecting = 0;
        run(
            Arc::new(crate::mock::schema()),
            addr,
            LiveOptions {
                backoff_min: Duration::from_millis(10),
                backoff_max: Duration::from_millis(20),
                ..Default::default()
            },
            |ev| {
                if ev == LiveEvent::Status(LinkStatus::Connecting) {
                    connecting += 1;
                }
                seen.push(ev);
                seen.len() < 4
            },
        )
        .await;
        assert_eq!(
            seen,
            [
                LiveEvent::Status(LinkStatus::Connecting),
                LiveEvent::Status(LinkStatus::GameNotRunning),
                LiveEvent::Status(LinkStatus::Connecting),
                LiveEvent::Status(LinkStatus::GameNotRunning),
            ]
        );
        assert_eq!(connecting, 2);
    }
}
