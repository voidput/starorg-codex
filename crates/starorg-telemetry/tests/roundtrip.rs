//! End to end against an in-process gRPC server that speaks the synthetic schema
//! (`starorg_telemetry::mock`) on 127.0.0.1: the stream, the throttled live loop, the action
//! list and a trigger that changes what the stream shows.

use prost_reflect::{DynamicMessage, ReflectMessage, Value};
use starorg_telemetry::client::{LoopbackAddr, TelemetryClient};
use starorg_telemetry::codec::DynamicCodec;
use starorg_telemetry::live::{self, LinkStatus, LiveEvent, LiveOptions};
use starorg_telemetry::mock::{self, MockGame};
use starorg_telemetry::schema::Schema;
use starorg_telemetry::trigger::{TriggerGate, TriggerStatus};
use starorg_telemetry::{SERVICE, json};
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};
use tonic::body::Body;
use tonic::codegen::{BoxFuture, Context, Poll, Service, http};
use tonic::server::Grpc;
use tonic::{Request, Response, Status};

/// Frames the server streams per connection, 20 ms apart (the game sends ~50 ms apart).
const FRAMES: usize = 30;
const FRAME_GAP: Duration = Duration::from_millis(20);

#[derive(Clone)]
struct MockServer {
    schema: Arc<Schema>,
    game: Arc<Mutex<MockGame>>,
    /// How long the stream's response headers are held back (a game that sends its initial
    /// metadata with the first frame, after spawning).
    hold_headers: Duration,
}

#[derive(Clone)]
struct StreamCall(MockServer);

impl Service<Request<DynamicMessage>> for StreamCall {
    type Response = Response<ReceiverStream<Result<DynamicMessage, Status>>>;
    type Error = Status;
    type Future = BoxFuture<Self::Response, Status>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Status>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: Request<DynamicMessage>) -> Self::Future {
        let srv = self.0.clone();
        Box::pin(async move {
            tokio::time::sleep(srv.hold_headers).await;
            let (tx, rx) = tokio::sync::mpsc::channel(4);
            tokio::spawn(async move {
                for i in 0..FRAMES {
                    let state = srv.game.lock().unwrap().state;
                    let f = mock::frame(&srv.schema, i as f64 * 0.05, &state);
                    if tx.send(Ok(f)).await.is_err() {
                        return;
                    }
                    tokio::time::sleep(FRAME_GAP).await;
                }
                // The game ends the stream with UNAVAILABLE when it exits.
                let _ = tx.send(Err(Status::unavailable("game exited"))).await;
            });
            Ok(Response::new(ReceiverStream::new(rx)))
        })
    }
}

#[derive(Clone)]
struct UnaryCall(MockServer, &'static str);

impl Service<Request<DynamicMessage>> for UnaryCall {
    type Response = Response<DynamicMessage>;
    type Error = Status;
    type Future = BoxFuture<Self::Response, Status>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Status>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<DynamicMessage>) -> Self::Future {
        let UnaryCall(srv, name) = self.clone();
        Box::pin(async move {
            let m = srv.schema.method(name).unwrap();
            let mut resp = DynamicMessage::new(m.output());
            let req = req.into_inner();
            assert_eq!(req.descriptor().full_name(), m.input().full_name());
            if name == "ListTriggerableActions" {
                let actions = srv
                    .game
                    .lock()
                    .unwrap()
                    .actions()
                    .iter()
                    .map(|a| {
                        let mut e = DynamicMessage::new(
                            srv.schema
                                .pool()
                                .get_message_by_name("sc.game.telemetry.v1.TriggerableAction")
                                .unwrap(),
                        );
                        e.set_field_by_name("action_name", Value::String(a.action_name.clone()));
                        e.set_field_by_name("category", Value::String(a.category.clone()));
                        e.set_field_by_name("holdable", Value::Bool(a.holdable));
                        Value::Message(e)
                    })
                    .collect();
                resp.set_field_by_name("actions", Value::List(actions));
            } else {
                let action = req
                    .get_field_by_name("action_name")
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default();
                let hold_fd = req.descriptor().get_field_by_name("hold_ms").unwrap();
                let hold = req
                    .has_field(&hold_fd)
                    .then(|| req.get_field(&hold_fd).as_u32())
                    .flatten();
                let out = srv
                    .game
                    .lock()
                    .unwrap()
                    .trigger(&action, hold, Instant::now());
                mock::set_path(
                    &mut resp,
                    "status",
                    Value::String(format!(
                        "TRIGGER_GAME_ACTION_STATUS_{}",
                        out.status.as_str()
                    )),
                );
                resp.set_field_by_name("message", Value::String(out.message));
            }
            Ok(Response::new(resp))
        })
    }
}

impl Service<http::Request<Body>> for MockServer {
    type Response = http::Response<Body>;
    type Error = Infallible;
    type Future = BoxFuture<Self::Response, Infallible>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<Body>) -> Self::Future {
        let srv = self.clone();
        Box::pin(async move {
            let path = req.uri().path().to_owned();
            let Some(name) = path
                .strip_prefix(&format!("/{SERVICE}/"))
                .and_then(|n| srv.schema.method(n).map(|m| (n.to_owned(), m)))
            else {
                return Ok(Status::unimplemented(path).into_http());
            };
            let (name, m) = name;
            let mut grpc = Grpc::new(DynamicCodec::new(m.input()));
            Ok(match name.as_str() {
                "TelemetryStream" => grpc.server_streaming(StreamCall(srv), req).await,
                "ListTriggerableActions" => {
                    grpc.unary(UnaryCall(srv, "ListTriggerableActions"), req)
                        .await
                }
                _ => grpc.unary(UnaryCall(srv, "TriggerGameAction"), req).await,
            })
        })
    }
}

async fn serve() -> (LoopbackAddr, Arc<Schema>, Arc<Mutex<MockGame>>) {
    serve_holding_headers(Duration::ZERO).await
}

async fn serve_holding_headers(
    hold_headers: Duration,
) -> (LoopbackAddr, Arc<Schema>, Arc<Mutex<MockGame>>) {
    let schema = Arc::new(mock::schema());
    let game = Arc::new(Mutex::new(MockGame::default()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = LoopbackAddr::from_socket_addr(listener.local_addr().unwrap()).unwrap();
    let svc = MockServer {
        schema: schema.clone(),
        game: game.clone(),
        hold_headers,
    };
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .serve_with_incoming(svc, TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    (addr, schema, game)
}

fn lights(frame: &DynamicMessage) -> Option<bool> {
    json::to_json(frame)["vehicle"]["state"]["lights_enabled"].as_bool()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stream_and_trigger_round_trip() {
    let (addr, schema, game) = serve().await;
    let mut client = TelemetryClient::connect(schema, addr).await.unwrap();

    let mut stream = client.stream().await.unwrap();
    let first = stream.message().await.unwrap().unwrap();
    assert_eq!(lights(&first), Some(true));

    let actions = client.list_actions().await.unwrap();
    assert!(
        actions
            .iter()
            .any(|a| a.action_name == "v_invoke_ping" && a.holdable)
    );

    let mut gate = TriggerGate::new();
    gate.set_enabled(true);
    gate.set_actions(&actions);
    gate.check("v_lights", None, false, Instant::now()).unwrap();
    let out = client.trigger("v_lights", None).await.unwrap();
    assert_eq!(out.status, TriggerStatus::Ok, "{out:?}");
    assert!(!game.lock().unwrap().state.lights_enabled);

    // The stream now shows the lights off.
    let mut off = false;
    while let Some(f) = stream.message().await.unwrap() {
        if lights(&f) == Some(false) {
            off = true;
            break;
        }
    }
    assert!(off, "the trigger shows in the stream");

    // The game's own answers come back typed.
    let out = client.trigger("v_lights", Some(500)).await.unwrap();
    assert_eq!(out.status, TriggerStatus::HoldNotAllowed);
    let out = client.trigger("v_invoke_ping", Some(1500)).await.unwrap();
    assert_eq!(out.status, TriggerStatus::Ok);
    let out = client.trigger("not_listed", None).await.unwrap();
    assert_eq!(out.status, TriggerStatus::UnknownAction);
    // Invalid names never leave the client.
    assert!(client.trigger("Bad Name", None).await.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_loop_throttles_and_reconnects() {
    let (addr, schema, _game) = serve().await;
    let opts = LiveOptions {
        ui_interval: Duration::from_millis(100),
        backoff_min: Duration::from_millis(50),
        backoff_max: Duration::from_millis(50),
    };
    let (handle, mut rx) = live::spawn(schema, addr, opts, 256);
    let mut statuses = Vec::new();
    let mut frames = Vec::new();
    let started = Instant::now();
    // Two connections: the first stream ends with UNAVAILABLE, then the loop reconnects.
    while statuses
        .iter()
        .filter(|s| **s == LinkStatus::Connected)
        .count()
        < 2
    {
        let ev = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("an event within 10 s")
            .expect("the loop is running");
        match ev {
            LiveEvent::Status(s) => statuses.push(s),
            LiveEvent::Frame(f) => frames.push((Instant::now(), f)),
        }
    }
    handle.stop();
    assert_eq!(
        &statuses[..4],
        [
            LinkStatus::Connecting,
            LinkStatus::Connected,
            LinkStatus::GameNotRunning,
            LinkStatus::Connecting,
        ]
    );
    // 30 frames over ~600 ms in, at most 10 Hz out (plus the held-back last one).
    let elapsed = started.elapsed().as_secs_f64();
    assert!(!frames.is_empty());
    assert!(
        frames.len() <= (elapsed * 10.0).ceil() as usize + 1,
        "{} frames in {elapsed:.2} s",
        frames.len()
    );
    assert!(frames.len() < FRAMES, "frames were throttled");
    for w in frames.windows(2) {
        assert!(w[1].1.seq > w[0].1.seq);
    }
    let (_, last) = frames.last().unwrap();
    let ship = last.snapshot.ship.as_ref().expect("a ship in the mock");
    assert_eq!(ship.name.as_deref(), Some("Mock Gladius"));
    assert!(last.raw["actor"]["vitals"].is_object());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connected_shows_before_the_stream_headers_arrive() {
    // The headers come 3 s late; "Connected" must not wait for them.
    let (addr, schema, _game) = serve_holding_headers(Duration::from_secs(3)).await;
    let (handle, mut rx) = live::spawn(schema, addr, LiveOptions::default(), 16);
    let mut statuses = Vec::new();
    let started = Instant::now();
    while !statuses.contains(&LinkStatus::Connected) {
        let ev = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("Connected within 2 s, before the headers")
            .expect("the loop is running");
        if let LiveEvent::Status(s) = ev {
            statuses.push(s);
        }
    }
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_eq!(statuses, [LinkStatus::Connecting, LinkStatus::Connected]);
    handle.stop();
}
