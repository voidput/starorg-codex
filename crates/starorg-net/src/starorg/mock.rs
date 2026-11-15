//! A local stand-in for StarOrg Tools (feature `mock-server`, tests only): the routes and
//! checks of SC-ORG-BOT#1659 on `127.0.0.1`, with the consent page and Discord left out —
//! `/login` validates like the server and answers with the 302 to the loopback redirect at
//! once. Tokens are random per run. Every request is recorded so tests can check what was
//! sent (and that nothing was).

use super::pkce;
use super::wire::{self, Request};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub bearer: bool,
    pub body: String,
}

/// What the browser leg does at `/login`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoginMode {
    #[default]
    Approve,
    /// Cancel on Discord: `?error=access_denied&state=`.
    Deny,
    /// Send someone else's `state` back.
    WrongState,
}

#[derive(Debug, Clone)]
pub struct MockState {
    /// `STRINGS_AUTH_ENABLED`.
    pub enabled: bool,
    pub login: LoginMode,
    pub expires_in: u64,
    pub requests: Vec<Recorded>,
    /// code -> (challenge, redirect_uri)
    codes: HashMap<String, (String, String)>,
    tokens: BTreeSet<String>,
    pub revoked: u32,
    /// (discord id, name, features)
    pub guilds: Vec<(String, String, Vec<String>)>,
    /// Guilds whose `industry_v2` is on (others 404 on the blueprint routes).
    pub industry: BTreeSet<String>,
    /// The public lookup rows.
    pub lookup: Vec<Value>,
    /// Owned ids -> (source, acquired_at)
    pub owned: BTreeMap<i64, (String, Option<String>)>,
    pub hangar: Vec<Value>,
}

impl Default for MockState {
    fn default() -> Self {
        Self {
            enabled: true,
            login: LoginMode::Approve,
            expires_in: 90 * 86400,
            requests: Vec::new(),
            codes: HashMap::new(),
            tokens: BTreeSet::new(),
            revoked: 0,
            guilds: vec![
                (
                    "100000000000000001".into(),
                    "Synthetic Org".into(),
                    vec!["industrial".into()],
                ),
                ("100000000000000002".into(), "Other Test Org".into(), vec![]),
            ],
            industry: ["100000000000000001".to_string()].into(),
            lookup: vec![
                json!({"id": 1, "k": "fake_rifle_kestrel", "n": "Fake Rifle 'Kestrel'", "c": "WeaponPersonal"}),
                json!({"id": 2, "k": "test_armor_arms", "n": "Test Armor Arms", "a": ["Test Arms"]}),
                json!({"id": 3, "k": "synthetic_medical_gun", "n": "Sample Medical Gun"}),
                json!({"id": 4, "k": "twin_a", "n": "Twin Name"}),
                json!({"id": 5, "k": "twin_b", "n": "Twin Name"}),
            ],
            owned: BTreeMap::from([(3, ("manual".to_string(), None))]),
            hangar: vec![
                json!({"id": 1, "vehicle_id": 11, "custom_name": "Rusty", "amount": 1, "is_hidden": false,
                       "vehicle": {"name": "Synthetic Alpha", "manufacturer": "Test Works"}}),
                json!({"id": 2, "vehicle_id": 12, "custom_name": null, "amount": 2, "is_hidden": false,
                       "vehicle": {"name": "Synthetic Bravo", "manufacturer": "Test Works"}}),
            ],
        }
    }
}

pub struct MockServer {
    pub base: String,
    state: Arc<Mutex<MockState>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl MockServer {
    pub fn start() -> Self {
        Self::with(MockState::default())
    }

    pub fn with(state: MockState) -> Self {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let state = Arc::new(Mutex::new(state));
        let stop = Arc::new(AtomicBool::new(false));
        let (st, sp) = (state.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            while !sp.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((s, _)) => {
                        let st = st.clone();
                        std::thread::spawn(move || serve(s, &st));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(10)),
                }
            }
        });
        Self {
            base: format!("http://127.0.0.1:{port}"),
            state,
            stop,
            thread: Some(thread),
        }
    }

    pub fn state(&self) -> MutexGuard<'_, MockState> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.state().requests.clone()
    }

    /// Issue a valid token directly (tests that start signed in).
    pub fn issue_token(&self) -> String {
        let t = random_text("mock");
        self.state().tokens.insert(t.clone());
        t
    }

    /// "Sign out everywhere": every issued token stops working.
    pub fn revoke_all(&self) {
        self.state().tokens.clear();
    }

    pub fn valid_tokens(&self) -> usize {
        self.state().tokens.len()
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The system browser in tests: GET the login URL and follow its redirect to the app's
/// loopback listener, on a thread (the app waits on the listener meanwhile).
pub fn fake_browser(url: &str) -> Result<(), String> {
    let url = url.to_string();
    std::thread::spawn(move || {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(20)))
            .build()
            .into();
        let _ = agent.get(&url).call();
    });
    Ok(())
}

fn random_text(prefix: &str) -> String {
    let mut b = [0u8; 24];
    getrandom::fill(&mut b).expect("randomness");
    format!("{prefix}-{}", pkce::base64url(&b))
}

fn serve(mut s: TcpStream, st: &Mutex<MockState>) {
    let _ = s.set_nonblocking(false);
    let Some(req) = wire::read_request(&mut s) else {
        return;
    };
    let mut g = st.lock().unwrap_or_else(|p| p.into_inner());
    let bearer = req
        .header("authorization")
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(str::to_string);
    g.requests.push(Recorded {
        method: req.method.clone(),
        path: req.path.clone(),
        bearer: bearer.is_some(),
        body: String::from_utf8_lossy(&req.body).into_owned(),
    });
    let (status, body, location) = route(&mut g, &req, bearer.as_deref());
    drop(g);
    match location {
        Some(l) => wire::respond(&mut s, 302, "text/plain", &[("Location", &l)], b""),
        None => wire::respond(
            &mut s,
            status,
            "application/json",
            &[],
            body.to_string().as_bytes(),
        ),
    }
}

fn detail(d: &str) -> Value {
    json!({ "detail": d })
}

type Reply = (u16, Value, Option<String>);

fn route(g: &mut MockState, req: &Request, bearer: Option<&str>) -> Reply {
    let m = req.method.as_str();
    let p = req.path.as_str();
    if p.starts_with("/api/auth/strings/") && !g.enabled {
        return (404, detail("Not Found"), None);
    }
    match (m, p) {
        ("GET", "/api/auth/strings/login") => return login(g, req),
        ("POST", "/api/auth/strings/exchange") => return exchange(g, req),
        ("GET", "/api/industrial/blueprint-lookup") => {
            return (
                200,
                json!({ "build": "4.10.200.58746", "blueprints": g.lookup }),
                None,
            );
        }
        _ => {}
    }
    // Everything else needs a valid strings token.
    let Some(t) = bearer.filter(|t| g.tokens.contains(*t)) else {
        return (401, detail("Not authenticated"), None);
    };
    if !g.enabled {
        return (403, detail("StarOrg Strings sign-in is disabled"), None);
    }
    let t = t.to_string();
    let guild_route = p
        .strip_prefix("/api/industrial/crafting/guilds/")
        .and_then(|r| r.split_once('/'));
    match (m, p, guild_route) {
        ("GET", "/api/auth/me", _) => (
            200,
            json!({
                "id": 7, "discord_id": "200000000000000007", "username": "test_pilot_discord",
                "rsi_handle": null,
                "guilds": g.guilds.iter().map(|(id, name, f)| json!({
                    "id": 1, "discord_id": id, "name": name, "features": f,
                })).collect::<Vec<_>>(),
            }),
            None,
        ),
        ("POST", "/api/auth/logout", _) => {
            g.tokens.remove(&t);
            g.revoked += 1;
            (200, json!({ "status": "ok" }), None)
        }
        ("GET", "/api/fleet/hangar", _) => (200, json!(g.hangar), None),
        (_, _, Some((guild, rest))) => {
            if !g.industry.contains(guild) {
                return (404, detail("Not Found"), None);
            }
            match (m, rest) {
                ("GET", "my-blueprints") => {
                    let rows: Vec<Value> = g
                        .owned
                        .iter()
                        .map(|(id, (src, at))| {
                            let row = g.lookup.iter().find(|r| r["id"] == *id);
                            json!({
                                "blueprint_id": id,
                                "slug": row.and_then(|r| r["k"].as_str()).unwrap_or_default(),
                                "name": row.and_then(|r| r["n"].as_str()).unwrap_or("Unknown"),
                                "category": row.and_then(|r| r["c"].as_str()),
                                "source": src, "acquired_at": at, "is_active": true,
                            })
                        })
                        .collect();
                    (200, json!(rows), None)
                }
                ("POST", "my-blueprints") => add(g, req),
                _ => (
                    403,
                    detail(
                        "This token is only valid for the StarOrg Strings blueprint and hangar sync",
                    ),
                    None,
                ),
            }
        }
        _ => (
            403,
            detail("This token is only valid for the StarOrg Strings blueprint and hangar sync"),
            None,
        ),
    }
}

fn login(g: &mut MockState, req: &Request) -> Reply {
    let bad = |d: &str| (400, detail(d), None);
    let Some(challenge) = req.param("code_challenge") else {
        return bad("code_challenge required");
    };
    if req.param("code_challenge_method") != Some("S256") || !pkce::valid_challenge(challenge) {
        return bad("Invalid code_challenge");
    }
    let redirect = req.param("redirect_uri").unwrap_or_default();
    if pkce::loopback_redirect_port(redirect).is_none() {
        return bad("redirect_uri is not a loopback callback");
    }
    let state = req.param("state").unwrap_or_default();
    if !pkce::valid_state(state) {
        return bad("Invalid state");
    }
    let q = match g.login {
        LoginMode::Deny => format!("error=access_denied&state={}", wire::encode(state)),
        LoginMode::WrongState | LoginMode::Approve => {
            let code = random_text("code");
            g.codes
                .insert(code.clone(), (challenge.to_string(), redirect.to_string()));
            let st = if g.login == LoginMode::WrongState {
                random_text("other")
            } else {
                state.to_string()
            };
            format!("code={}&state={}", wire::encode(&code), wire::encode(&st))
        }
    };
    (302, Value::Null, Some(format!("{redirect}?{q}")))
}

fn exchange(g: &mut MockState, req: &Request) -> Reply {
    let Ok(b) = serde_json::from_slice::<Value>(&req.body) else {
        return (422, detail("body"), None);
    };
    let (Some(code), Some(verifier), Some(redirect)) = (
        b["code"].as_str(),
        b["code_verifier"].as_str(),
        b["redirect_uri"].as_str(),
    ) else {
        return (422, detail("body"), None);
    };
    if !pkce::valid_verifier(verifier) {
        return (422, detail("code_verifier"), None);
    }
    let invalid = (400, detail("Invalid or expired code"), None);
    // Single use: a wrong verifier or redirect spends the code too.
    let Some((challenge, bound)) = g.codes.remove(code) else {
        return invalid;
    };
    if pkce::challenge(verifier) != challenge || redirect != bound {
        return invalid;
    }
    let t = random_text("mock");
    g.tokens.insert(t.clone());
    (
        200,
        json!({
            "access_token": t, "expires_in": g.expires_in, "token_type": "bearer", "aud": "strings",
            "user": { "discord_id": "200000000000000007", "username": "test_pilot_discord", "rsi_handle": null },
        }),
        None,
    )
}

fn add(g: &mut MockState, req: &Request) -> Reply {
    let Ok(b) = serde_json::from_slice::<Value>(&req.body) else {
        return (422, detail("body"), None);
    };
    if b["source"] != "starorg_strings" {
        return (422, detail("source"), None);
    }
    let Some(ids) = b["blueprint_ids"].as_array() else {
        return (422, detail("blueprint_ids"), None);
    };
    if ids.is_empty() || ids.len() > super::MAX_BULK_IDS {
        return (422, detail("blueprint_ids"), None);
    }
    let (mut added, mut already) = (Vec::new(), Vec::new());
    for id in ids.iter().filter_map(Value::as_i64) {
        if !g.lookup.iter().any(|r| r["id"] == id) {
            return (400, detail(&format!("Unknown blueprint id {id}")), None);
        }
        match g.owned.entry(id) {
            std::collections::btree_map::Entry::Occupied(_) => already.push(id),
            std::collections::btree_map::Entry::Vacant(v) => {
                let at = b["acquired_at"][id.to_string()]
                    .as_str()
                    .map(str::to_string);
                v.insert(("starorg_strings".into(), at));
                added.push(id);
            }
        }
    }
    (
        200,
        json!({ "added": added, "already_owned": already }),
        None,
    )
}
