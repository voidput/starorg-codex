//! UEX Corp market data (spec obtain-hints): the two files WHERE TO GET's shop line and the
//! MCP shop tools read, fetched from `api.uexcorp.space` while Settings -> Network ->
//! "Download UEX market data" is on.
//!
//! The output matches sibling `sc-gamedata`'s `fetch` exactly (`src/domains/uex-market.ts`):
//!
//! - `uex-terminals.json`: `{ _source, _fetched, terminals: [...], summary: { terminals } }`,
//!   rows cut to [`TERMINAL_KEYS`] ([`trim_terminal`]);
//! - `uex-item-prices.json`: `{ _source, _fetched, prices: [...], summary: { prices, items } }`,
//!   rows cut to [`ITEM_PRICE_KEYS`] ([`trim_item_price`]).
//!
//! Both endpoints are public. The user's token, when one is set, is sent as
//! `Authorization: Bearer` to this host and nowhere else; a wrong token is an error (UEX
//! denies it even on public endpoints). Error text never contains the token.

use serde_json::{Map, Value};
use starorg_core::fsutil;
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use std::time::Duration;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

/// The UEX API, version 2.
pub const BASE: &str = "https://api.uexcorp.space/2.0";
/// `_source` of both files.
pub const SOURCE: &str = "UEX Corp API (https://uexcorp.space)";
pub const TERMINALS_FILE: &str = "uex-terminals.json";
pub const PRICES_FILE: &str = "uex-item-prices.json";
/// Largest response read. `items_prices_all` is about 6.4 MB (Nov 2026), `terminals` 1 MB.
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// The columns a terminal row keeps: what places it.
pub const TERMINAL_KEYS: &[&str] = &[
    "id",
    "name",
    "fullname",
    "nickname",
    "displayname",
    "code",
    "type",
    "star_system_name",
    "planet_name",
    "orbit_name",
    "moon_name",
    "space_station_name",
    "city_name",
    "outpost_name",
    "is_available",
    "is_available_live",
];

/// The columns an item price row keeps; the rest is location ids and history.
pub const ITEM_PRICE_KEYS: &[&str] = &[
    "item_name",
    "item_uuid",
    "id_item",
    "id_category",
    "terminal_name",
    "id_terminal",
    "price_buy",
    "price_sell",
    "date_modified",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UexError {
    /// No answer (DNS, TLS, timeout, connection).
    Network(String),
    /// UEX refused the token (HTTP 401/403, or a token message in the body).
    Token(String),
    /// UEX answered with an error (non-200, or `status` other than `ok`).
    Refused(String),
    /// The answer is not what the API documents, or is too large.
    Bad(String),
    /// The files could not be written.
    Io(String),
}

impl std::fmt::Display for UexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(m) => write!(f, "UEX could not be reached: {m}"),
            Self::Token(m) => write!(
                f,
                "UEX refused the token ({m}). Check it in Settings -> Network, or remove it: both files are public."
            ),
            Self::Refused(m) => write!(f, "UEX answered with an error: {m}"),
            Self::Bad(m) => write!(f, "UEX sent something unexpected: {m}"),
            Self::Io(m) => write!(f, "could not write the UEX files: {m}"),
        }
    }
}

impl std::error::Error for UexError {}

/// One raw answer: HTTP status and body.
pub struct Answer {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Gets `<base>/<endpoint>`. The real one is [`HttpTransport`]; tests use a fake.
pub trait UexTransport: Send + Sync {
    fn get(&self, endpoint: &str, token: Option<&str>) -> Result<Answer, UexError>;
}

/// HTTPS to [`BASE`]: OS TLS, https only, a timeout and the app's user agent.
pub struct HttpTransport {
    agent: ureq::Agent,
    base: String,
}

impl HttpTransport {
    pub fn new() -> Self {
        let tls = TlsConfig::builder()
            .provider(TlsProvider::NativeTls)
            .root_certs(RootCerts::PlatformVerifier)
            .build();
        let agent = ureq::Agent::config_builder()
            .tls_config(tls)
            .https_only(true)
            .timeout_global(Some(Duration::from_secs(120)))
            .user_agent(concat!("starorg-codex/", env!("CARGO_PKG_VERSION")))
            .http_status_as_error(false)
            .build()
            .into();
        Self {
            agent,
            base: BASE.to_string(),
        }
    }
}

impl Default for HttpTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl UexTransport for HttpTransport {
    fn get(&self, endpoint: &str, token: Option<&str>) -> Result<Answer, UexError> {
        let url = format!("{}/{endpoint}", self.base);
        let mut req = self.agent.get(&url).header("Accept", "application/json");
        if let Some(t) = token {
            req = req.header("Authorization", &format!("Bearer {t}"));
        }
        // ureq's error text names the URL at most; the token is only in a header.
        let resp = req
            .call()
            .map_err(|e| UexError::Network(format!("{endpoint}: {e}")))?;
        let status = resp.status().as_u16();
        let mut body = Vec::new();
        resp.into_body()
            .into_reader()
            .take(MAX_BYTES + 1)
            .read_to_end(&mut body)
            .map_err(|e| UexError::Network(format!("{endpoint}: {e}")))?;
        if body.len() as u64 > MAX_BYTES {
            return Err(UexError::Bad(format!(
                "{endpoint}: more than {} MB",
                MAX_BYTES / 1024 / 1024
            )));
        }
        Ok(Answer { status, body })
    }
}

/// A token as it may go into a header: trimmed, printable ASCII without spaces, sane length.
/// `None` for an empty one. The error names the problem, never the value.
pub fn clean_token(raw: &str) -> Result<Option<String>, String> {
    let t = raw.trim();
    if t.is_empty() {
        return Ok(None);
    }
    if t.len() > 512 {
        return Err("The token is longer than 512 characters.".into());
    }
    if !t.chars().all(|c| c.is_ascii_graphic()) {
        return Err("The token has spaces or characters a UEX token does not have.".into());
    }
    Ok(Some(t.to_string()))
}

/// Error text from UEX with any copy of the token cut out.
fn scrub(msg: &str, token: Option<&str>) -> String {
    let mut m: String = msg.chars().take(300).collect();
    if let Some(t) = token.filter(|t| !t.is_empty()) {
        m = m.replace(t, "[token]");
    }
    m
}

/// The `data` rows of an answer: checks HTTP status and UEX's own `status` (UEX answers
/// HTTP 200 with `status` other than `ok` when a request fails).
pub fn rows_of(endpoint: &str, a: &Answer, token: Option<&str>) -> Result<Vec<Value>, UexError> {
    let body: Option<Value> = serde_json::from_slice(&a.body).ok();
    let message = body
        .as_ref()
        .and_then(|b| {
            b.get("message")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .or_else(|| b.get("status").and_then(Value::as_str))
        })
        .map(|m| scrub(m, token));
    let token_trouble = |m: &Option<String>| {
        m.as_deref()
            .is_some_and(|m| m.to_ascii_lowercase().contains("token"))
    };
    if a.status != 200 {
        let what = message
            .clone()
            .unwrap_or_else(|| format!("HTTP {}", a.status));
        if token.is_some() && (matches!(a.status, 401 | 403) || token_trouble(&message)) {
            return Err(UexError::Token(format!("{endpoint}: {what}")));
        }
        return Err(UexError::Refused(format!(
            "{endpoint} returned HTTP {}{}",
            a.status,
            message.map(|m| format!(": {m}")).unwrap_or_default()
        )));
    }
    let Some(body) = body else {
        return Err(UexError::Bad(format!("{endpoint}: not JSON")));
    };
    let status = body.get("status").and_then(Value::as_str).unwrap_or("");
    if status != "ok" {
        let what = message.clone().unwrap_or_else(|| "no status".into());
        if token.is_some() && token_trouble(&message) {
            return Err(UexError::Token(format!("{endpoint}: {what}")));
        }
        return Err(UexError::Refused(format!("{endpoint} failed: {what}")));
    }
    Ok(match body.get("data") {
        Some(Value::Array(v)) => v.clone(),
        Some(v @ Value::Object(_)) => vec![v.clone()],
        _ => Vec::new(),
    })
}

fn is_zero(v: &Value) -> bool {
    v.as_f64() == Some(0.0)
}

fn blank(v: &Value) -> bool {
    v.is_null() || v.as_str() == Some("")
}

/// `trimTerminalRow`: the keys of [`TERMINAL_KEYS`] in that order, without null and blank;
/// zero is dropped except on the `is_` flags (their zero is data).
pub fn trim_terminal(row: &Value) -> Value {
    let mut out = Map::new();
    for key in TERMINAL_KEYS {
        let Some(v) = row.get(*key) else { continue };
        if blank(v) || (is_zero(v) && !key.starts_with("is_")) {
            continue;
        }
        out.insert((*key).to_string(), v.clone());
    }
    Value::Object(out)
}

/// `trimItemPriceRow`: the keys of [`ITEM_PRICE_KEYS`] in that order, without null, blank
/// and zero.
pub fn trim_item_price(row: &Value) -> Value {
    let mut out = Map::new();
    for key in ITEM_PRICE_KEYS {
        let Some(v) = row.get(*key) else { continue };
        if blank(v) || is_zero(v) {
            continue;
        }
        out.insert((*key).to_string(), v.clone());
    }
    Value::Object(out)
}

/// `new Date().toISOString()` for Unix milliseconds: `2026-11-15T02:35:23.448Z`.
pub fn iso_millis(ms: u64) -> String {
    let s = fsutil::rfc3339(ms / 1000);
    format!("{}.{:03}Z", s.trim_end_matches('Z'), ms % 1000)
}

fn file_body(fetched: &str, key: &str, rows: Vec<Value>, summary: Value) -> Value {
    let mut m = Map::new();
    m.insert("_source".into(), Value::String(SOURCE.into()));
    m.insert("_fetched".into(), Value::String(fetched.into()));
    m.insert(key.into(), Value::Array(rows));
    m.insert("summary".into(), summary);
    Value::Object(m)
}

/// `uex-terminals.json` from the rows of `terminals`.
pub fn terminals_file(fetched: &str, rows: &[Value]) -> Value {
    let rows: Vec<Value> = rows.iter().map(trim_terminal).collect();
    let n = rows.len();
    file_body(
        fetched,
        "terminals",
        rows,
        serde_json::json!({ "terminals": n }),
    )
}

/// `uex-item-prices.json` from the rows of `items_prices_all`. `items` counts distinct
/// `item_uuid` values, a missing one counting once (as a JS `Set` does).
pub fn prices_file(fetched: &str, rows: &[Value]) -> Value {
    let rows: Vec<Value> = rows.iter().map(trim_item_price).collect();
    let items: HashSet<String> = rows
        .iter()
        .map(|r| r.get("item_uuid").map(Value::to_string).unwrap_or_default())
        .collect();
    let n = rows.len();
    file_body(
        fetched,
        "prices",
        rows,
        serde_json::json!({ "prices": n, "items": items.len() }),
    )
}

/// What [`fetch_market`] wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchReport {
    pub terminals: usize,
    pub prices: usize,
    pub items: usize,
    /// Bytes written, both files.
    pub bytes: u64,
    pub fetched: String,
}

/// Fetches both endpoints, then writes both files into `dir` (each atomically: temp file,
/// then rename). Nothing is written unless both answers are good, so a failure keeps the
/// files that were there.
pub fn fetch_market(
    t: &dyn UexTransport,
    dir: &Path,
    token: Option<&str>,
) -> Result<FetchReport, UexError> {
    let terminals = rows_of("terminals", &t.get("terminals", token)?, token)?;
    let prices = rows_of(
        "items_prices_all",
        &t.get("items_prices_all", token)?,
        token,
    )?;
    if terminals.is_empty() || prices.is_empty() {
        return Err(UexError::Bad(format!(
            "{} terminals and {} prices; keeping the files there were",
            terminals.len(),
            prices.len()
        )));
    }
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let fetched = iso_millis(ms);
    let tf = terminals_file(&fetched, &terminals);
    let pf = prices_file(&fetched, &prices);
    let count = |v: &Value, k: &str| v["summary"][k].as_u64().unwrap_or(0) as usize;
    let report = FetchReport {
        terminals: count(&tf, "terminals"),
        prices: count(&pf, "prices"),
        items: count(&pf, "items"),
        bytes: 0,
        fetched,
    };
    let mut bytes = 0;
    for (name, v) in [(TERMINALS_FILE, &tf), (PRICES_FILE, &pf)] {
        let b = serde_json::to_vec_pretty(v).map_err(|e| UexError::Io(e.to_string()))?;
        bytes += b.len() as u64;
        fsutil::write_atomic(&dir.join(name), &b)
            .map_err(|e| UexError::Io(format!("{name}: {e}")))?;
    }
    Ok(FetchReport { bytes, ..report })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    /// Answers per endpoint, and the tokens it was asked with.
    struct Fake {
        answers: Vec<(&'static str, u16, Value)>,
        seen: Mutex<Vec<Option<String>>>,
    }

    impl UexTransport for Fake {
        fn get(&self, endpoint: &str, token: Option<&str>) -> Result<Answer, UexError> {
            self.seen.lock().unwrap().push(token.map(str::to_string));
            let (_, status, body) = self
                .answers
                .iter()
                .find(|(e, _, _)| *e == endpoint)
                .expect("unexpected endpoint");
            Ok(Answer {
                status: *status,
                body: serde_json::to_vec(body).unwrap(),
            })
        }
    }

    fn ok(data: Value) -> Value {
        json!({ "status": "ok", "http_code": 200, "data": data, "message": "" })
    }

    fn terminal_row() -> Value {
        json!({
            "id": 1, "id_star_system": 68, "id_moon": 0, "name": "Admin - ARC-L1",
            "fullname": "Commodity Shop - Admin - ARC-L1", "nickname": "ARC-L1",
            "displayname": "ARC-L1 Wide Forest Station", "code": "ARCL1", "type": "commodity",
            "contact_url": "", "screenshot": "https://cdn/x.jpg", "star_system_name": "Stanton",
            "planet_name": "ArcCorp", "orbit_name": "ArcCorp Lagrange Point 1", "moon_name": null,
            "space_station_name": "ARC-L1 Wide Forest Station", "city_name": "", "outpost_name": 0,
            "is_available": 1, "is_available_live": 0, "is_visible": 1
        })
    }

    fn price_row(uuid: Option<&str>) -> Value {
        let mut v = json!({
            "id": 1, "id_item": 1, "id_category": 32, "id_terminal": 107, "price_buy": 15461,
            "price_sell": 0, "date_added": 1703516981, "date_modified": 1788031806,
            "item_name": "Omnisky III Cannon", "terminal_name": "CenterMass Area 18"
        });
        if let Some(u) = uuid {
            v["item_uuid"] = json!(u);
        }
        v
    }

    #[test]
    fn terminal_rows_keep_placing_keys_in_order() {
        let t = trim_terminal(&terminal_row());
        let keys: Vec<&str> = t.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "id",
                "name",
                "fullname",
                "nickname",
                "displayname",
                "code",
                "type",
                "star_system_name",
                "planet_name",
                "orbit_name",
                "space_station_name",
                "is_available",
                "is_available_live"
            ]
        );
        // The flag's zero is data.
        assert_eq!(t["is_available_live"], 0);
    }

    #[test]
    fn price_rows_drop_zero_blank_and_extra_keys() {
        let p = trim_item_price(&price_row(Some("u-1")));
        assert_eq!(
            p,
            json!({
                "item_name": "Omnisky III Cannon", "item_uuid": "u-1", "id_item": 1,
                "id_category": 32, "terminal_name": "CenterMass Area 18", "id_terminal": 107,
                "price_buy": 15461, "date_modified": 1788031806
            })
        );
        let keys: Vec<&str> = p.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys[0], "item_name");
        assert_eq!(keys[7], "date_modified");
    }

    #[test]
    fn files_carry_source_fetched_rows_and_summary() {
        let rows = [
            price_row(Some("a")),
            price_row(Some("a")),
            price_row(Some("b")),
            price_row(None),
        ];
        let f = prices_file("2026-11-15T02:35:23.448Z", &rows);
        let keys: Vec<&str> = f.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["_source", "_fetched", "prices", "summary"]);
        assert_eq!(f["_source"], SOURCE);
        assert_eq!(f["summary"], json!({ "prices": 4, "items": 3 }));
        let t = terminals_file("x", &[terminal_row()]);
        assert_eq!(t["summary"], json!({ "terminals": 1 }));
        assert_eq!(t["terminals"][0]["code"], "ARCL1");
    }

    #[test]
    fn iso_millis_matches_to_iso_string() {
        assert_eq!(iso_millis(1_794_710_123_448), "2026-11-15T02:35:23.448Z");
        assert_eq!(iso_millis(5), "1970-01-01T00:00:00.005Z");
    }

    #[test]
    fn status_other_than_ok_is_an_error_even_on_http_200() {
        let a = Answer {
            status: 200,
            body: serde_json::to_vec(&json!({ "status": "requests_limit_reached", "data": null }))
                .unwrap(),
        };
        let e = rows_of("terminals", &a, None).unwrap_err();
        assert!(matches!(e, UexError::Refused(_)), "{e:?}");
        assert!(e.to_string().contains("requests_limit_reached"));
        let a = Answer {
            status: 200,
            body: b"<html>".to_vec(),
        };
        assert!(matches!(
            rows_of("terminals", &a, None),
            Err(UexError::Bad(_))
        ));
        // One object is one row.
        let a = Answer {
            status: 200,
            body: serde_json::to_vec(&ok(json!({ "id": 1 }))).unwrap(),
        };
        assert_eq!(rows_of("terminals", &a, None).unwrap().len(), 1);
    }

    #[test]
    fn a_refused_token_says_so_without_the_token() {
        let secret = "sekrit-0123456789";
        for (status, body) in [
            (
                200,
                json!({ "status": "invalid_token", "message": format!("bad token {secret}") }),
            ),
            (401, json!({ "status": "error" })),
        ] {
            let a = Answer {
                status,
                body: serde_json::to_vec(&body).unwrap(),
            };
            let e = rows_of("terminals", &a, Some(secret)).unwrap_err();
            assert!(matches!(e, UexError::Token(_)), "{e:?}");
            assert!(!e.to_string().contains(secret), "{e}");
            assert!(!format!("{e:?}").contains(secret));
        }
    }

    #[test]
    fn tokens_are_cleaned_and_checked() {
        assert_eq!(clean_token("  ").unwrap(), None);
        assert_eq!(clean_token(" abc123 ").unwrap().as_deref(), Some("abc123"));
        assert!(clean_token("a b").is_err());
        assert!(clean_token("a\r\nX-Evil: 1").is_err());
        assert!(clean_token(&"x".repeat(513)).is_err());
        let e = clean_token("sekrit with space").unwrap_err();
        assert!(!e.contains("sekrit"));
    }

    #[test]
    fn fetch_writes_both_files_and_sends_the_token_given() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake {
            answers: vec![
                ("terminals", 200, ok(json!([terminal_row()]))),
                (
                    "items_prices_all",
                    200,
                    ok(json!([price_row(Some("a")), price_row(Some("b"))])),
                ),
            ],
            seen: Mutex::default(),
        };
        let r = fetch_market(&fake, dir.path(), Some("tok")).unwrap();
        assert_eq!((r.terminals, r.prices, r.items), (1, 2, 2));
        assert!(r.bytes > 0);
        assert_eq!(
            *fake.seen.lock().unwrap(),
            [Some("tok".to_string()), Some("tok".to_string())]
        );
        let read = |n: &str| -> Value {
            serde_json::from_slice(&std::fs::read(dir.path().join(n)).unwrap()).unwrap()
        };
        let p = read(PRICES_FILE);
        assert_eq!(p["_fetched"], r.fetched);
        assert_eq!(p["prices"].as_array().unwrap().len(), 2);
        assert_eq!(read(TERMINALS_FILE)["terminals"][0]["id"], 1);
        // No temp files left behind.
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names.len(), 2, "{names:?}");
    }

    #[test]
    fn a_failure_keeps_the_files_there_were() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(TERMINALS_FILE), "old").unwrap();
        let fake = Fake {
            answers: vec![
                ("terminals", 200, ok(json!([terminal_row()]))),
                ("items_prices_all", 500, json!({ "status": "error" })),
            ],
            seen: Mutex::default(),
        };
        let e = fetch_market(&fake, dir.path(), None).unwrap_err();
        assert!(matches!(e, UexError::Refused(_)), "{e:?}");
        assert_eq!(
            std::fs::read_to_string(dir.path().join(TERMINALS_FILE)).unwrap(),
            "old"
        );
        assert!(!dir.path().join(PRICES_FILE).exists());
        assert_eq!(*fake.seen.lock().unwrap(), [None, None]);
    }

    /// Real network, no token: both files fetch and parse.
    #[test]
    #[ignore = "network"]
    fn live_fetch() {
        let dir = std::env::var_os("UEX_LIVE_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("uex-live"));
        let t = std::time::Instant::now();
        let r = fetch_market(&HttpTransport::new(), &dir, None).unwrap();
        eprintln!("{r:?} in {:?}", t.elapsed());
        assert!(r.terminals > 100 && r.prices > 1000);
    }
}
