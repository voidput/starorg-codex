//! gRPC client for [`SERVICE`](crate::SERVICE) over plaintext HTTP/2 on loopback.
//!
//! The game's service has no TLS and no authentication (api-spec §1, §6), so the client only
//! ever connects to a loopback address: [`LoopbackAddr`] refuses anything else, `localhost`
//! included only as the literal name (it maps to `127.0.0.1` without a DNS lookup).
//! Messages are [`DynamicMessage`]s of the run-time [`Schema`]; request types come from the
//! service's methods, so the client follows the build's own schema.

use crate::codec::DynamicCodec;
use crate::json;
use crate::schema::Schema;
use crate::trigger::{self, Refusal, TriggerOutcome, TriggerStatus, TriggerableAction};
use http::uri::PathAndQuery;
use prost_reflect::{DynamicMessage, Kind, MethodDescriptor, ReflectMessage, Value};
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tonic::transport::{Channel, Endpoint};
use tonic::{Code, Request, Streaming};

/// Connect timeout.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// Deadline of a unary call (server.py `TRIGGER_TIMEOUT_S`).
pub const UNARY_TIMEOUT: Duration = Duration::from_secs(5);

/// A loopback socket address: the only kind the client connects to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LoopbackAddr(SocketAddr);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AddrError {
    #[error("{0} is not a loopback address; live telemetry connects to this PC only")]
    NotLoopback(String),
    #[error("port 0 is not a port")]
    PortZero,
}

impl LoopbackAddr {
    /// `host` is an IP literal (`127.0.0.1`, `::1`, any of `127.0.0.0/8`) or `localhost`.
    /// Host names other than `localhost` are refused, not resolved.
    pub fn new(host: &str, port: u16) -> Result<Self, AddrError> {
        let host = host.trim();
        let h = host
            .strip_prefix('[')
            .and_then(|h| h.strip_suffix(']'))
            .unwrap_or(host);
        let ip: IpAddr = if h.eq_ignore_ascii_case("localhost") {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        } else {
            h.parse()
                .map_err(|_| AddrError::NotLoopback(host.to_owned()))?
        };
        Self::from_socket_addr(SocketAddr::new(ip, port))
    }

    /// An IPv4-mapped IPv6 address (`::ffff:127.0.0.1`) is stored as its IPv4 form.
    pub fn from_socket_addr(addr: SocketAddr) -> Result<Self, AddrError> {
        let ip = addr.ip().to_canonical();
        if !ip.is_loopback() {
            return Err(AddrError::NotLoopback(addr.ip().to_string()));
        }
        if addr.port() == 0 {
            return Err(AddrError::PortZero);
        }
        Ok(Self(SocketAddr::new(ip, addr.port())))
    }

    /// `127.0.0.1:<port>`.
    pub fn localhost(port: u16) -> Result<Self, AddrError> {
        Self::from_socket_addr(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))
    }

    pub fn addr(&self) -> SocketAddr {
        self.0
    }

    pub fn port(&self) -> u16 {
        self.0.port()
    }
}

impl fmt::Display for LoopbackAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// Nothing listens on the port: the game is not running, or its server is off.
    #[error("nothing is listening on {0} (is the game running with its telemetry server on?)")]
    NotListening(LoopbackAddr),
    #[error("cannot connect to {addr}: {reason}")]
    Connect { addr: LoopbackAddr, reason: String },
    #[error("the game answered {code:?}: {message}")]
    Rpc { code: Code, message: String },
    #[error("this build's schema lacks {0}")]
    Schema(String),
    #[error(transparent)]
    Refused(#[from] Refusal),
}

impl ClientError {
    /// The call failed because the game is gone (refused connection, or `UNAVAILABLE`).
    pub fn game_gone(&self) -> bool {
        matches!(self, Self::NotListening(_))
            || matches!(self, Self::Rpc { code, .. } if *code == Code::Unavailable)
    }
}

impl From<tonic::Status> for ClientError {
    fn from(s: tonic::Status) -> Self {
        Self::Rpc {
            code: s.code(),
            message: s.message().to_owned(),
        }
    }
}

fn refused(e: &(dyn std::error::Error + 'static)) -> bool {
    let mut cur = Some(e);
    while let Some(e) = cur {
        if let Some(io) = e.downcast_ref::<std::io::Error>()
            && io.kind() == std::io::ErrorKind::ConnectionRefused
        {
            return true;
        }
        cur = e.source();
    }
    false
}

/// A connection to the game's telemetry service.
#[derive(Debug, Clone)]
pub struct TelemetryClient {
    schema: Arc<Schema>,
    addr: LoopbackAddr,
    grpc: tonic::client::Grpc<Channel>,
}

impl TelemetryClient {
    /// Connect (HTTP/2 without TLS) to `addr`.
    pub async fn connect(schema: Arc<Schema>, addr: LoopbackAddr) -> Result<Self, ClientError> {
        let ep = Endpoint::from_shared(format!("http://{addr}"))
            .map_err(|e| ClientError::Connect {
                addr,
                reason: e.to_string(),
            })?
            .connect_timeout(CONNECT_TIMEOUT)
            .tcp_nodelay(true);
        let channel = ep.connect().await.map_err(|e| {
            if refused(&e) {
                ClientError::NotListening(addr)
            } else {
                let mut reason = e.to_string();
                let mut src = std::error::Error::source(&e);
                while let Some(s) = src {
                    reason = format!("{reason}: {s}");
                    src = s.source();
                }
                ClientError::Connect { addr, reason }
            }
        })?;
        Ok(Self {
            schema,
            addr,
            grpc: tonic::client::Grpc::new(channel),
        })
    }

    pub fn addr(&self) -> LoopbackAddr {
        self.addr
    }

    pub fn schema(&self) -> &Arc<Schema> {
        &self.schema
    }

    fn method(&self, name: &str) -> Result<MethodDescriptor, ClientError> {
        self.schema
            .method(name)
            .ok_or_else(|| ClientError::Schema(format!("the {name} method")))
    }

    fn path(m: &MethodDescriptor) -> PathAndQuery {
        let p = format!("/{}/{}", m.parent_service().full_name(), m.name());
        PathAndQuery::try_from(p).expect("service and method names form a valid path")
    }

    async fn ready(&mut self) -> Result<(), ClientError> {
        let addr = self.addr;
        self.grpc.ready().await.map_err(|e| ClientError::Connect {
            addr,
            reason: e.to_string(),
        })
    }

    async fn unary(
        &mut self,
        m: &MethodDescriptor,
        req: DynamicMessage,
    ) -> Result<DynamicMessage, ClientError> {
        self.ready().await?;
        let mut r = Request::new(req);
        r.set_timeout(UNARY_TIMEOUT);
        let resp = self
            .grpc
            .unary(r, Self::path(m), DynamicCodec::new(m.output()))
            .await?;
        Ok(resp.into_inner())
    }

    /// `TelemetryStream`: one frame per message until the game ends the stream. No frames come
    /// until the player has spawned, so there is no deadline.
    pub async fn stream(&mut self) -> Result<Streaming<DynamicMessage>, ClientError> {
        let m = self.method("TelemetryStream")?;
        self.ready().await?;
        let req = Request::new(DynamicMessage::new(m.input()));
        let resp = self
            .grpc
            .server_streaming(req, Self::path(&m), DynamicCodec::new(m.output()))
            .await?;
        Ok(resp.into_inner())
    }

    /// `ListTriggerableActions`, normalised ([`trigger::normalise_actions`]). Empty while the
    /// game has triggers off or the player has not spawned.
    pub async fn list_actions(&mut self) -> Result<Vec<TriggerableAction>, ClientError> {
        let m = self.method("ListTriggerableActions")?;
        let resp = self.unary(&m, DynamicMessage::new(m.input())).await?;
        let j = json::to_json(&resp);
        Ok(trigger::normalise_actions(&j["actions"]))
    }

    /// `TriggerGameAction`. Checks only the name and hold ([`trigger::validate`]); the app
    /// puts a [`trigger::TriggerGate`] in front of it.
    pub async fn trigger(
        &mut self,
        action_name: &str,
        hold_ms: Option<u32>,
    ) -> Result<TriggerOutcome, ClientError> {
        trigger::validate(action_name, hold_ms)?;
        let m = self.method("TriggerGameAction")?;
        let mut req = DynamicMessage::new(m.input());
        req.try_set_field_by_name("action_name", Value::String(action_name.to_owned()))
            .map_err(|_| ClientError::Schema("TriggerGameActionRequest.action_name".into()))?;
        if let Some(h) = hold_ms {
            req.try_set_field_by_name("hold_ms", Value::U32(h))
                .map_err(|_| ClientError::Schema("TriggerGameActionRequest.hold_ms".into()))?;
        }
        let resp = self.unary(&m, req).await?;
        Ok(outcome(&resp))
    }
}

/// A `TriggerGameActionResponse` to a typed outcome. The status goes by its value name, so the
/// enum numbers of the build do not matter.
pub fn outcome(resp: &DynamicMessage) -> TriggerOutcome {
    let desc = resp.descriptor();
    let status = desc
        .get_field_by_name("status")
        .map_or(TriggerStatus::Unspecified, |fd| {
            match (fd.kind(), resp.get_field(&fd).as_enum_number()) {
                (Kind::Enum(e), Some(n)) => e.get_value(n).map_or_else(
                    || TriggerStatus::Other(n.to_string()),
                    |v| TriggerStatus::from_name(v.name()),
                ),
                _ => TriggerStatus::Unspecified,
            }
        });
    let message = resp
        .get_field_by_name("message")
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    TriggerOutcome { status, message }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_only() {
        for ok in [
            "127.0.0.1",
            "127.3.4.5",
            "localhost",
            "LOCALHOST",
            "::1",
            "[::1]",
            "::ffff:127.0.0.1",
        ] {
            let a = LoopbackAddr::new(ok, 50051).unwrap_or_else(|e| panic!("{ok}: {e}"));
            assert!(a.addr().ip().to_canonical().is_loopback());
        }
        for bad in [
            "0.0.0.0",
            "192.168.1.20",
            "10.0.0.1",
            "::",
            "example.com",
            "localhost.example.com",
            "127.0.0.1.nip.io",
            "",
        ] {
            assert!(
                matches!(
                    LoopbackAddr::new(bad, 50051),
                    Err(AddrError::NotLoopback(_))
                ),
                "{bad} must be refused"
            );
        }
        assert_eq!(LoopbackAddr::new("127.0.0.1", 0), Err(AddrError::PortZero));
        assert!(LoopbackAddr::from_socket_addr("8.8.8.8:53".parse().unwrap()).is_err());
        assert_eq!(
            LoopbackAddr::localhost(1234).unwrap().to_string(),
            "127.0.0.1:1234"
        );
    }

    #[test]
    fn outcome_by_name() {
        let s = crate::mock::schema();
        let out = s.method("TriggerGameAction").unwrap().output();
        let mut m = DynamicMessage::new(out.clone());
        crate::mock::set_path(
            &mut m,
            "status",
            Value::String("TRIGGER_GAME_ACTION_STATUS_MAP_INACTIVE".into()),
        );
        m.set_field_by_name("message", Value::String("not seated".into()));
        let o = outcome(&m);
        assert_eq!(o.status, TriggerStatus::MapInactive);
        assert_eq!(o.message, "not seated");
        let mut m = DynamicMessage::new(out);
        m.set_field_by_name("status", Value::EnumNumber(99));
        assert_eq!(outcome(&m).status, TriggerStatus::Other("99".into()));
    }

    #[tokio::test]
    async fn refused_when_nothing_listens() {
        // Bind and drop to find a free port.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let addr = LoopbackAddr::localhost(port).unwrap();
        let err = TelemetryClient::connect(Arc::new(crate::mock::schema()), addr)
            .await
            .unwrap_err();
        assert!(err.game_gone(), "{err}");
    }
}
