//! `starorg-codex mcp`: the stdio MCP server against a fake install (no game data).
#![cfg(unix)]

mod common;

use common::Fake;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::Stdio;

fn session(f: &Fake, extra: &[&str], messages: &[Value]) -> Vec<Value> {
    let mut args = vec!["mcp"];
    args.extend_from_slice(extra);
    let mut child = f
        .cmd(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut answers = Vec::new();
    for m in messages {
        writeln!(stdin, "{m}").unwrap();
        stdin.flush().unwrap();
        if m.get("id").is_some() {
            let mut line = String::new();
            out.read_line(&mut line).unwrap();
            answers.push(serde_json::from_str(&line).unwrap());
        }
    }
    drop(stdin);
    assert!(child.wait().unwrap().success());
    answers
}

fn call(id: u64, name: &str, args: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": args}})
}

fn text(v: &Value) -> Value {
    serde_json::from_str(v["result"]["content"][0]["text"].as_str().unwrap())
        .unwrap_or_else(|_| v["result"]["content"][0]["text"].clone())
}

#[test]
fn stdio_server_lists_tools_and_answers_from_the_cache() {
    let f = Fake::new();
    // A cached string table for LIVE's build, where the server looks for it.
    let p4k = f.lib.join("LIVE/Data.p4k");
    let dir = f
        .base
        .join("home/cache/starorg-codex/mcp/LIVE")
        .join(format!(
            "4.10.200.58746-{}",
            starorg_mcp::store::Store::fingerprint(&p4k)
        ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("game-strings-english.json"),
        json!({"language": "english", "strings": {"item_NameA": "Filler Alpha"}}).to_string(),
    )
    .unwrap();
    // Stamped by this build's code, as an extraction would (a file without one is not served).
    starorg_mcp::stamps::write(
        &dir,
        "game-strings-english",
        starorg_mcp::stamps::LOCAL,
        None,
    )
    .unwrap();

    let answers = session(
        &f,
        &[],
        &[
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}}),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
            call(3, "string", json!({"key": "item_NameA"})),
            call(4, "status", json!({})),
            // The fake archive cannot be extracted: an error for this call, not a crash.
            call(5, "search", json!({"query": "alpha"})),
            call(6, "string", json!({"key": "item_NameA", "channel": "PTU"})),
        ],
    );
    assert_eq!(answers[0]["result"]["serverInfo"]["name"], "starorg-codex");
    let tools = answers[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 12);
    assert_eq!(
        tools[0]["inputSchema"]["properties"]["channel"]["enum"],
        json!(["LIVE", "HOTFIX", "PTU"])
    );
    let s = text(&answers[2]);
    assert_eq!(s["channel"], "LIVE");
    assert_eq!(s["string"]["stock"], "Filler Alpha");
    let st = text(&answers[3]);
    assert_eq!(st["channels"].as_array().unwrap().len(), 3);
    assert!(st["network"].as_str().unwrap().starts_with("none"));
    assert_eq!(answers[4]["result"]["isError"], true);
    assert_eq!(answers[5]["result"]["isError"], true);
}

#[test]
fn default_channel_flag() {
    let f = Fake::new();
    let a = session(&f, &["--channel", "ptu"], &[call(1, "status", json!({}))]);
    let st = text(&a[0]);
    let default: Vec<&str> = st["channels"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["default"] == true)
        .map(|c| c["channel"].as_str().unwrap())
        .collect();
    assert_eq!(default, ["PTU"]);
    let o = f.run(&["mcp", "--channel", "NOPE"]);
    assert_ne!(o.status.code(), Some(0));
}

#[test]
fn worker_refuses_to_write_into_the_game() {
    let f = Fake::new();
    let live = f.lib.join("LIVE");
    let o = std::process::Command::new(common::BIN)
        .arg("mcp-extract-worker")
        .arg("--p4k")
        .arg(live.join("Data.p4k"))
        .arg("--out")
        .arg(live.join("x"))
        .arg("--dataset")
        .arg("game-item-tags")
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("refusing"),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert_eq!(o.status.code(), Some(2));
    assert!(!live.join("x").exists());
}
