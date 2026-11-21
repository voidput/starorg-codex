//! MCP over JSON-RPC 2.0: lifecycle, `tools/list`, `tools/call`, `ping`.
//!
//! A minimal implementation of the protocol's server side (revisions 2025-06-18,
//! 2025-03-26 and 2024-11-05), no SDK: the official Rust SDK needs an async runtime and,
//! for HTTP, a web framework; this server only ever answers one local client's requests in
//! order. Transports ([`crate::stdio`], `http`) hand each message to [`Server::handle`].

use crate::tools::Tools;
use serde_json::{Map, Value, json};
use std::time::Instant;

pub const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
pub const SERVER_NAME: &str = "starorg-codex";

/// JSON-RPC error codes.
pub mod code {
    pub const PARSE: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
}

/// One handled request, for the app's request log.
#[derive(Debug, Clone, PartialEq)]
pub struct Handled {
    pub method: String,
    /// `tools/call`: the tool's name.
    pub tool: Option<String>,
    pub ok: bool,
    pub millis: u64,
}

pub struct Server {
    pub tools: Tools,
    instructions: String,
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn result(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

impl Server {
    pub fn new(tools: Tools) -> Self {
        let default = tools
            .store
            .default_channel()
            .map(|c| c.name.clone())
            .unwrap_or_else(|| "none found".into());
        Self {
            instructions: format!(
                "Read-only Star Citizen game data from this computer's install (channels: {}; default {default}). \
                 Start with `search` to find keys, then `item`, `how_to_obtain`, `where_found`, `where_mined`, `where_to_buy`, \
                 `mission`, `compare`, `string` or `diff`. The first question about a build extracts the datasets \
                 it needs from Data.p4k (seconds to a minute, once per build). Shop prices are UEX data (player-reported) \
                 and only present when the user supplied them.",
                tools.store.channel_names().join(", ")
            ),
            tools,
        }
    }

    /// Answer one JSON-RPC message (or batch). `None` for notifications and responses.
    pub fn handle(&self, msg: &Value, log: &mut Vec<Handled>) -> Option<Value> {
        if let Value::Array(batch) = msg {
            if batch.is_empty() {
                return Some(error(Value::Null, code::INVALID_REQUEST, "empty batch"));
            }
            let out: Vec<Value> = batch.iter().filter_map(|m| self.one(m, log)).collect();
            return (!out.is_empty()).then_some(Value::Array(out));
        }
        self.one(msg, log)
    }

    /// Parse and answer one line of text (stdio framing).
    pub fn handle_text(&self, text: &str, log: &mut Vec<Handled>) -> Option<Value> {
        match serde_json::from_str::<Value>(text) {
            Ok(v) => self.handle(&v, log),
            Err(e) => Some(error(
                Value::Null,
                code::PARSE,
                &format!("parse error: {e}"),
            )),
        }
    }

    fn one(&self, msg: &Value, log: &mut Vec<Handled>) -> Option<Value> {
        let Some(obj) = msg.as_object() else {
            return Some(error(Value::Null, code::INVALID_REQUEST, "not an object"));
        };
        let method = obj.get("method").and_then(Value::as_str);
        let id = obj.get("id").cloned();
        let (Some(method), Some(id)) = (method, id) else {
            // A notification (no id) or a response to us (no method): nothing to answer.
            if method.is_none() && obj.get("id").is_none() {
                return Some(error(Value::Null, code::INVALID_REQUEST, "no method"));
            }
            return None;
        };
        if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Some(error(id, code::INVALID_REQUEST, "jsonrpc must be \"2.0\""));
        }
        let empty = Map::new();
        let params = obj
            .get("params")
            .and_then(Value::as_object)
            .unwrap_or(&empty);
        let started = Instant::now();
        let mut entry = Handled {
            method: method.to_string(),
            tool: None,
            ok: true,
            millis: 0,
        };
        let answer = match method {
            "initialize" => {
                let asked = params.get("protocolVersion").and_then(Value::as_str);
                let version = asked
                    .filter(|v| PROTOCOL_VERSIONS.contains(v))
                    .unwrap_or(PROTOCOL_VERSIONS[0]);
                result(
                    id,
                    json!({
                        "protocolVersion": version,
                        "capabilities": {"tools": {"listChanged": false}},
                        "serverInfo": {"name": SERVER_NAME, "title": "StarOrg Codex (experimental)", "version": env!("CARGO_PKG_VERSION")},
                        "instructions": self.instructions,
                    }),
                )
            }
            "ping" => result(id, json!({})),
            "tools/list" => result(id, json!({"tools": self.tools.definitions()})),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                entry.tool = Some(name.to_string());
                let args = params
                    .get("arguments")
                    .and_then(Value::as_object)
                    .unwrap_or(&empty);
                if !self.tools.definitions().iter().any(|d| d.name == name) {
                    entry.ok = false;
                    error(id, code::INVALID_PARAMS, &format!("unknown tool {name:?}"))
                } else {
                    match self.tools.call(name, args) {
                        Ok(v) => result(
                            id,
                            json!({"content": [{"type": "text", "text": starorg_gamedata::output::to_js_json_compact(&v)}],
                                   "isError": false}),
                        ),
                        Err(e) => {
                            entry.ok = false;
                            result(
                                id,
                                json!({"content": [{"type": "text", "text": e.0}], "isError": true}),
                            )
                        }
                    }
                }
            }
            "resources/list" => result(id, json!({"resources": []})),
            "prompts/list" => result(id, json!({"prompts": []})),
            other => {
                entry.ok = false;
                error(
                    id,
                    code::METHOD_NOT_FOUND,
                    &format!("method {other} not supported"),
                )
            }
        };
        entry.millis = started.elapsed().as_millis() as u64;
        log.push(entry);
        Some(answer)
    }
}
