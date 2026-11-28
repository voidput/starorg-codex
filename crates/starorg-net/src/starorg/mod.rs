//! StarOrg Tools client for the desktop app (addendum §5; server: SC-ORG-BOT#1659,
//! `apps/org_mgmt/api/routes/strings_auth.py` and `docs/internal/reference/api.md`).
//!
//! Sign-in is the server's loopback PKCE flow for audience `strings`:
//!
//! 1. `GET /api/auth/strings/login?redirect_uri=&code_challenge=&code_challenge_method=S256&state=`
//!    opens in the system browser (consent page, then Discord);
//! 2. the server sends the browser to `http://127.0.0.1:<port>/callback?code=&state=`
//!    ([`loopback::Loopback`]);
//! 3. `POST /api/auth/strings/exchange {code, code_verifier, redirect_uri}` returns
//!    `{access_token, expires_in, token_type: "bearer", aud: "strings", user}`.
//!
//! The token (90 days, no refresh token) may call only the server's `STRINGS_ROUTES`:
//! `GET /api/auth/me`, `POST /api/auth/logout`, `GET|POST
//! /api/industrial/crafting/me/blueprints` (the account's owned list; SC-ORG-BOT #1737), its
//! older form `…/crafting/guilds/{guild_id}/my-blueprints` for servers without it (see
//! [`Client::my_blueprints`]), `POST …/my-blueprints/remove`,
//! `GET …/blueprint-library` and `GET /api/fleet/hangar`. The public
//! `GET /api/industrial/blueprint-lookup` (id, record key and names of every blueprint) is
//! read without the token. While the server's `STRINGS_AUTH_ENABLED` is off the sign-in
//! routes are a 404 and tokens a 403 ([`StarOrgError::Disabled`]).
//!
//! The app never pushes removals, so `my-blueprints/remove` is not wrapped here.

pub mod loopback;
#[cfg(feature = "mock-server")]
pub mod mock;
pub mod pkce;
pub mod wire;

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use std::io::Read;
use std::time::{Duration, Instant};
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

/// Token audience the app asks for and accepts.
pub const AUDIENCE: &str = "strings";
/// `source` of the blueprints the app adds (`OwnedSource` in the server schema).
pub const SOURCE: &str = "starorg_strings";
/// The server's `MAX_BULK_IDS` per add request.
pub const MAX_BULK_IDS: usize = 2000;
/// Largest response read (the blueprint lookup is a few hundred KB).
const MAX_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StarOrgError {
    /// No HTTP answer (offline, DNS, TLS, timeout).
    Network(String),
    /// Sign-in for the desktop app is switched off on that server (`STRINGS_AUTH_ENABLED`).
    Disabled,
    /// The token is missing, expired or revoked: sign in again.
    Unauthorized(String),
    /// 404 on a data route (no account-level blueprint route and no org with blueprint
    /// tracking, or no such guild).
    NotFound(String),
    RateLimited,
    /// Any other refusal, with the server's `detail`.
    Refused {
        status: u16,
        detail: String,
    },
    /// An answer the client does not understand.
    Protocol(String),
    /// The browser came back with a `state` this attempt did not send.
    StateMismatch,
    Cancelled(String),
    Timeout,
}

impl std::fmt::Display for StarOrgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(e) => write!(f, "could not reach StarOrg Tools: {e}"),
            Self::Disabled => f.write_str(
                "sign-in for StarOrg Codex is not enabled on this StarOrg Tools server yet",
            ),
            Self::Unauthorized(d) => write!(
                f,
                "StarOrg Tools refused the sign-in token ({d}); sign in again"
            ),
            Self::NotFound(d) => write!(f, "not found on StarOrg Tools: {d}"),
            Self::RateLimited => {
                f.write_str("StarOrg Tools asks to slow down; try again in a minute")
            }
            Self::Refused { status, detail } => {
                write!(f, "StarOrg Tools answered {status}: {detail}")
            }
            Self::Protocol(e) => write!(f, "unexpected answer from StarOrg Tools: {e}"),
            Self::StateMismatch => f.write_str(
                "the browser returned a sign-in this app did not start; nothing was linked",
            ),
            Self::Cancelled(e) => f.write_str(e),
            Self::Timeout => f.write_str("no answer from the browser in time; sign-in stopped"),
        }
    }
}

impl std::error::Error for StarOrgError {}

/// Discord ids arrive as strings, older rows may carry numbers.
fn id_text<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum T {
        S(String),
        N(i64),
    }
    Ok(match T::deserialize(d)? {
        T::S(s) => s,
        T::N(n) => n.to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantUser {
    #[serde(deserialize_with = "id_text")]
    pub discord_id: String,
    pub username: String,
    #[serde(default)]
    pub rsi_handle: Option<String>,
}

/// `/exchange`'s answer. `Debug` leaves the token out.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct TokenGrant {
    pub access_token: String,
    pub expires_in: u64,
    pub token_type: String,
    #[serde(default)]
    pub aud: Option<String>,
    pub user: GrantUser,
}

impl std::fmt::Debug for TokenGrant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenGrant")
            .field("expires_in", &self.expires_in)
            .field("aud", &self.aud)
            .field("user", &self.user)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeGuild {
    /// The Discord id: the `{guild_id}` of the blueprint routes.
    #[serde(deserialize_with = "id_text")]
    pub discord_id: String,
    pub name: String,
    #[serde(default)]
    pub features: Vec<String>,
}

/// The parts of `GET /api/auth/me` the app reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Me {
    #[serde(deserialize_with = "id_text")]
    pub discord_id: String,
    pub username: String,
    #[serde(default)]
    pub rsi_handle: Option<String>,
    #[serde(default)]
    pub guilds: Vec<MeGuild>,
}

/// One row of `GET …/my-blueprints` (`OwnedBlueprint`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedRow {
    pub blueprint_id: i64,
    #[serde(default)]
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub acquired_at: Option<String>,
}

/// `POST …/my-blueprints` answer (`OwnedBlueprintAddResult`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddResult {
    pub added: Vec<i64>,
    pub already_owned: Vec<i64>,
}

/// One row of the public `blueprint-lookup` (`LookupRow`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupRow {
    pub id: i64,
    /// Canonical internal key of the record name.
    pub k: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p: Option<String>,
    /// Display label.
    pub n: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub c: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lookup {
    #[serde(default)]
    pub build: Option<String>,
    pub blueprints: Vec<LookupRow>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vehicle {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub manufacturer: Option<String>,
}

/// One row of `GET /api/fleet/hangar` (`HangarVehicleResponse`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HangarVehicle {
    #[serde(default)]
    pub custom_name: Option<String>,
    #[serde(default = "one")]
    pub amount: u32,
    #[serde(default)]
    pub is_hidden: bool,
    #[serde(default)]
    pub vehicle: Vehicle,
}

fn one() -> u32 {
    1
}

/// The `canonical_internal_key` of SC-ORG-BOT `core/services/owned_blueprints.py` (the BP
/// Dumper's rule): lower case, no `craftingblueprintrecord.` prefix, no `,p`, `bp_craft_`,
/// `_scitem.json` / `.json` / `_scitem`, `scitem_`.
pub fn canonical_key(raw: &str) -> String {
    let mut v = raw.replace('\\', "/").trim().to_lowercase();
    if let Some(r) = v.strip_prefix("craftingblueprintrecord.") {
        v = r.to_string();
    }
    if let Some(r) = v.strip_suffix(",p") {
        v = r.to_string();
    }
    if let Some(r) = v.strip_prefix("bp_craft_") {
        v = r.to_string();
    }
    if let Some(r) = v.strip_suffix("_scitem.json") {
        v = r.to_string();
    } else if let Some(r) = v.strip_suffix(".json") {
        v = r.to_string();
    } else if let Some(r) = v.strip_suffix("_scitem") {
        v = r.to_string();
    }
    if let Some(r) = v.strip_prefix("scitem_") {
        v = r.to_string();
    }
    v
}

/// The two real hosts over https, or a loopback test server over http.
pub fn allowed_base(base: &str) -> bool {
    let base = base.trim_end_matches('/');
    if base == "https://starorg.tools" || base == "https://dev.starorg.tools" {
        return true;
    }
    let Some(rest) = base
        .strip_prefix("http://127.0.0.1:")
        .or_else(|| base.strip_prefix("http://[::1]:"))
    else {
        return false;
    };
    !rest.is_empty() && rest.len() <= 5 && rest.bytes().all(|b| b.is_ascii_digit())
}

/// Guild ids go into a path: Discord snowflakes are digits; refuse anything else.
fn path_id(id: &str) -> Result<&str, StarOrgError> {
    if !id.is_empty() && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        Ok(id)
    } else {
        Err(StarOrgError::Protocol(format!("refusing guild id {id:?}")))
    }
}

/// HTTP client for one StarOrg Tools environment. `Debug` leaves the token out.
pub struct Client {
    agent: ureq::Agent,
    base: String,
    token: Option<String>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("base", &self.base)
            .field("token", &self.token.as_ref().map(|_| "<set>"))
            .finish()
    }
}

struct Answer {
    status: u16,
    body: Vec<u8>,
}

impl Client {
    /// `base`: `https://starorg.tools`, `https://dev.starorg.tools`, or a loopback test server.
    pub fn new(base: &str, user_agent: &str) -> Result<Self, StarOrgError> {
        if !allowed_base(base) {
            return Err(StarOrgError::Protocol(format!(
                "refusing StarOrg base {base:?}"
            )));
        }
        let tls = TlsConfig::builder()
            .provider(TlsProvider::NativeTls)
            .root_certs(RootCerts::PlatformVerifier)
            .build();
        let agent = ureq::Agent::config_builder()
            .tls_config(tls)
            .https_only(base.starts_with("https://"))
            .timeout_global(Some(Duration::from_secs(30)))
            .user_agent(user_agent)
            .http_status_as_error(false)
            // Never follow a redirect with the bearer token on it.
            .max_redirects(0)
            .build()
            .into();
        Ok(Self {
            agent,
            base: base.trim_end_matches('/').to_string(),
            token: None,
        })
    }

    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    fn read(resp: ureq::http::Response<ureq::Body>) -> Result<Answer, StarOrgError> {
        let status = resp.status().as_u16();
        let mut body = Vec::new();
        resp.into_body()
            .into_reader()
            .take(MAX_BYTES + 1)
            .read_to_end(&mut body)
            .map_err(|e| StarOrgError::Network(e.to_string()))?;
        if body.len() as u64 > MAX_BYTES {
            return Err(StarOrgError::Protocol("answer too large".into()));
        }
        Ok(Answer { status, body })
    }

    fn net(e: ureq::Error) -> StarOrgError {
        StarOrgError::Network(e.to_string())
    }

    fn auth(&self) -> Result<String, StarOrgError> {
        self.token
            .as_ref()
            .map(|t| format!("Bearer {t}"))
            .ok_or_else(|| StarOrgError::Unauthorized("not signed in".into()))
    }

    fn get(&self, path: &str, bearer: bool) -> Result<Answer, StarOrgError> {
        let mut r = self
            .agent
            .get(self.url(path))
            .header("Accept", "application/json");
        if bearer {
            r = r.header("Authorization", self.auth()?);
        }
        Self::read(r.call().map_err(Self::net)?)
    }

    fn post(
        &self,
        path: &str,
        body: &serde_json::Value,
        bearer: bool,
    ) -> Result<Answer, StarOrgError> {
        let bytes = serde_json::to_vec(body).map_err(|e| StarOrgError::Protocol(e.to_string()))?;
        let mut r = self
            .agent
            .post(self.url(path))
            .header("Accept", "application/json")
            .header("Content-Type", "application/json");
        if bearer {
            r = r.header("Authorization", self.auth()?);
        }
        Self::read(r.send(&bytes[..]).map_err(Self::net)?)
    }

    fn detail(body: &[u8]) -> String {
        serde_json::from_slice::<serde_json::Value>(body)
            .ok()
            .and_then(|v| v.get("detail").and_then(|d| d.as_str()).map(str::to_string))
            .unwrap_or_else(|| {
                String::from_utf8_lossy(&body[..body.len().min(200)])
                    .trim()
                    .to_string()
            })
    }

    fn ok(a: Answer, what: &str) -> Result<Vec<u8>, StarOrgError> {
        match a.status {
            200..=299 => Ok(a.body),
            401 => Err(StarOrgError::Unauthorized(Self::detail(&a.body))),
            403 => {
                let d = Self::detail(&a.body);
                if d.contains("sign-in is disabled") {
                    Err(StarOrgError::Disabled)
                } else {
                    Err(StarOrgError::Refused {
                        status: 403,
                        detail: d,
                    })
                }
            }
            404 => Err(StarOrgError::NotFound(what.to_string())),
            429 => Err(StarOrgError::RateLimited),
            s => Err(StarOrgError::Refused {
                status: s,
                detail: Self::detail(&a.body),
            }),
        }
    }

    fn json<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T, StarOrgError> {
        serde_json::from_slice(body).map_err(|e| StarOrgError::Protocol(e.to_string()))
    }

    /// The sign-in page with this attempt's challenge, state and loopback redirect.
    pub fn login_url(&self, redirect_uri: &str, p: &pkce::Pkce) -> String {
        format!(
            "{}/api/auth/strings/login?redirect_uri={}&code_challenge={}&code_challenge_method=S256&state={}",
            self.base,
            wire::encode(redirect_uri),
            wire::encode(&p.challenge),
            wire::encode(&p.state)
        )
    }

    /// Whether the server takes desktop sign-ins at all, before a browser opens: with
    /// `STRINGS_AUTH_ENABLED` off `/login` is a 404 whatever is asked; on, a request without
    /// a challenge is a 400.
    pub fn probe_sign_in(&self) -> Result<(), StarOrgError> {
        let a = self.get("/api/auth/strings/login", false)?;
        match a.status {
            404 => Err(StarOrgError::Disabled),
            400 | 200 => Ok(()),
            429 => Err(StarOrgError::RateLimited),
            s => Err(StarOrgError::Refused {
                status: s,
                detail: Self::detail(&a.body),
            }),
        }
    }

    /// The whole sign-in: probe, loopback listener, browser, callback, exchange.
    pub fn sign_in(
        &self,
        open_browser: &dyn Fn(&str) -> Result<(), String>,
        timeout: Duration,
        cancel: &dyn Fn() -> bool,
    ) -> Result<TokenGrant, StarOrgError> {
        self.probe_sign_in()?;
        let p = pkce::Pkce::new().map_err(StarOrgError::Protocol)?;
        let lb = loopback::Loopback::bind()?;
        let redirect = lb.redirect_uri();
        debug_assert!(pkce::loopback_redirect_port(&redirect).is_some());
        open_browser(&self.login_url(&redirect, &p))
            .map_err(|e| StarOrgError::Cancelled(format!("could not open the browser: {e}")))?;
        let code = lb.wait(&p.state, Instant::now() + timeout, cancel)?;
        self.exchange(&code, &p.verifier, &redirect)
    }

    pub fn exchange(
        &self,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
    ) -> Result<TokenGrant, StarOrgError> {
        let body = serde_json::json!({
            "code": code, "code_verifier": verifier, "redirect_uri": redirect_uri,
        });
        let a = self.post("/api/auth/strings/exchange", &body, false)?;
        if a.status == 404 {
            return Err(StarOrgError::Disabled);
        }
        let g: TokenGrant = Self::json(&Self::ok(a, "sign-in")?)?;
        if !g.token_type.eq_ignore_ascii_case("bearer")
            || g.aud.as_deref() != Some(AUDIENCE)
            || g.access_token.is_empty()
        {
            return Err(StarOrgError::Protocol("not a StarOrg Codex token".into()));
        }
        Ok(g)
    }

    pub fn me(&self) -> Result<Me, StarOrgError> {
        Self::json(&Self::ok(self.get("/api/auth/me", true)?, "account")?)
    }

    /// Revoke the token on the server (denylists its `jti`).
    pub fn logout(&self) -> Result<(), StarOrgError> {
        Self::ok(
            self.post("/api/auth/logout", &serde_json::json!({}), true)?,
            "sign-out",
        )
        .map(|_| ())
    }

    /// The account's owned blueprints. The list is one per account: the account-level
    /// route is asked first, and a server that does not have it yet (404, 405, or the 403
    /// of a token not allowed on that path) is asked through `guild`, the id of an org
    /// with blueprint tracking, when there is one.
    pub fn my_blueprints(&self, guild: Option<&str>) -> Result<Vec<OwnedRow>, StarOrgError> {
        let mut a = self.get(ME_BLUEPRINTS, true)?;
        if let Some(g) = guild.filter(|_| no_such_route(&a)) {
            a = self.get(&guild_blueprints(g)?, true)?;
        }
        Self::json(&Self::ok(a, BLUEPRINTS_WHAT)?)
    }

    /// Add ids in requests of at most [`MAX_BULK_IDS`], with `source: starorg_strings` and
    /// the earliest local receipt time per id when known. Routes as in
    /// [`Self::my_blueprints`]; once the account-level route is refused, the rest of the
    /// call goes through `guild`.
    pub fn add_blueprints(
        &self,
        guild: Option<&str>,
        ids: &[i64],
        acquired_at: &BTreeMap<i64, String>,
    ) -> Result<AddResult, StarOrgError> {
        let mut path = ME_BLUEPRINTS.to_string();
        let mut out = AddResult::default();
        for chunk in ids.chunks(MAX_BULK_IDS) {
            let times: BTreeMap<String, &String> = chunk
                .iter()
                .filter_map(|id| acquired_at.get(id).map(|t| (id.to_string(), t)))
                // The server parses these as datetimes and refuses the whole chunk on one bad
                // value: send only well-formed ones (the rest go without a time).
                .filter(|(_, t)| is_iso_datetime(t))
                .collect();
            let mut body = serde_json::json!({ "blueprint_ids": chunk, "source": SOURCE });
            if !times.is_empty() {
                body["acquired_at"] = serde_json::json!(times);
            }
            let mut a = self.post(&path, &body, true)?;
            if path == ME_BLUEPRINTS
                && let Some(g) = guild.filter(|_| no_such_route(&a))
            {
                path = guild_blueprints(g)?;
                a = self.post(&path, &body, true)?;
            }
            let r: AddResult = Self::json(&Self::ok(a, BLUEPRINTS_WHAT)?)?;
            out.added.extend(r.added);
            out.already_owned.extend(r.already_owned);
        }
        Ok(out)
    }

    /// Public catalogue (no token sent).
    pub fn blueprint_lookup(&self) -> Result<Lookup, StarOrgError> {
        Self::json(&Self::ok(
            self.get("/api/industrial/blueprint-lookup", false)?,
            "blueprint lookup",
        )?)
    }

    pub fn hangar(&self) -> Result<Vec<HangarVehicle>, StarOrgError> {
        Self::json(&Self::ok(self.get("/api/fleet/hangar", true)?, "hangar")?)
    }
}

/// The account-level owned-blueprints route (no org in the path).
const ME_BLUEPRINTS: &str = "/api/industrial/crafting/me/blueprints";
const BLUEPRINTS_WHAT: &str = "owned blueprint list";

/// The same list through an org's route, for servers without [`ME_BLUEPRINTS`].
fn guild_blueprints(guild: &str) -> Result<String, StarOrgError> {
    Ok(format!(
        "/api/industrial/crafting/guilds/{}/my-blueprints",
        path_id(guild)?
    ))
}

/// The answer of a server that does not serve this path to the app: no such route (404,
/// 405), or a path its token scope does not list (403, other than sign-in switched off).
fn no_such_route(a: &Answer) -> bool {
    match a.status {
        404 | 405 => true,
        403 => !Client::detail(&a.body).contains("sign-in is disabled"),
        _ => false,
    }
}

/// `YYYY-MM-DD[T ]HH:MM[:SS[.frac]][Z|±HH[:]MM]`, the shapes the server's datetime parser
/// takes from a string.
pub fn is_iso_datetime(s: &str) -> bool {
    let b = s.as_bytes();
    let digits =
        |r: std::ops::Range<usize>| b.get(r).is_some_and(|x| x.iter().all(u8::is_ascii_digit));
    if b.len() < 16
        || !digits(0..4)
        || b[4] != b'-'
        || !digits(5..7)
        || b[7] != b'-'
        || !digits(8..10)
        || !matches!(b[10], b'T' | b't' | b' ')
        || !digits(11..13)
        || b[13] != b':'
        || !digits(14..16)
    {
        return false;
    }
    let num = |r: std::ops::Range<usize>| s[r].parse::<u32>().unwrap_or(99);
    if !(1..=12).contains(&num(5..7))
        || !(1..=31).contains(&num(8..10))
        || num(11..13) > 23
        || num(14..16) > 59
    {
        return false;
    }
    let mut i = 16;
    if b.get(i) == Some(&b':') {
        if !digits(i + 1..i + 3) || num(i + 1..i + 3) > 59 {
            return false;
        }
        i += 3;
        if b.get(i) == Some(&b'.') {
            let n = b[i + 1..].iter().take_while(|c| c.is_ascii_digit()).count();
            if n == 0 {
                return false;
            }
            i += 1 + n;
        }
    }
    match &b[i..] {
        [] | [b'Z' | b'z'] => true,
        [b'+' | b'-', rest @ ..] => {
            let r: Vec<u8> = rest.iter().copied().filter(|c| *c != b':').collect();
            r.len() == 4
                && r.iter().all(u8::is_ascii_digit)
                && rest.len() <= 5
                && (rest.len() == 4 || rest.get(2) == Some(&b':'))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_key_mirrors_the_server() {
        assert_eq!(
            canonical_key("CraftingBlueprintRecord.BP_CRAFT_KSAR_Rifle_01_SCItem"),
            "ksar_rifle_01"
        );
        assert_eq!(canonical_key("bp_craft_scitem_helmet.json"), "helmet");
        assert_eq!(canonical_key("Item_Name,P"), "item_name");
        assert_eq!(canonical_key(""), "");
    }

    #[test]
    fn only_the_two_hosts_or_loopback() {
        assert!(allowed_base("https://starorg.tools"));
        assert!(allowed_base("https://dev.starorg.tools/"));
        assert!(allowed_base("http://127.0.0.1:8080"));
        for bad in [
            "http://starorg.tools",
            "https://evil.example",
            "https://starorg.tools.evil.example",
            "http://localhost:8080",
            "http://127.0.0.1:8080/x",
        ] {
            assert!(!allowed_base(bad), "{bad}");
            assert!(Client::new(bad, "t").is_err());
        }
    }

    #[test]
    fn login_url_carries_s256_and_the_loopback_redirect() {
        let c = Client::new("https://dev.starorg.tools", "t").unwrap();
        let p = pkce::Pkce::new().unwrap();
        let u = c.login_url("http://127.0.0.1:50123/callback", &p);
        assert!(u.starts_with("https://dev.starorg.tools/api/auth/strings/login?"));
        assert!(u.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A50123%2Fcallback"));
        assert!(u.contains("code_challenge_method=S256"));
        assert!(u.contains(&format!("code_challenge={}", p.challenge)));
        assert!(u.contains(&format!("state={}", p.state)));
        assert!(!u.contains(&p.verifier));
        let c = c.with_token("x".repeat(40));
        assert!(!format!("{c:?}").contains("xxxx"));
    }

    #[test]
    fn iso_datetimes_the_server_parses() {
        for ok in [
            "2026-03-02T10:05:00.000Z",
            "2026-03-02T10:05:00Z",
            "2026-03-02 10:05:00",
            "2026-03-02T10:05",
            "2026-03-02T10:05:00.123456+02:00",
            "2026-03-02T10:05:00-0500",
        ] {
            assert!(is_iso_datetime(ok), "{ok}");
        }
        for bad in [
            "",
            "yesterday",
            "2026-03-02",
            "2026-13-02T10:05:00Z",
            "2026-03-02T25:05:00Z",
            "2026-03-02T10:05:00.Z",
            "2026-03-02T10:05:00 UTC",
            "2026-03-02T10:05:00+2",
            "03/02/2026 10:05",
        ] {
            assert!(!is_iso_datetime(bad), "{bad}");
        }
    }

    #[test]
    fn guild_ids_are_checked_before_they_reach_a_path() {
        assert!(path_id("123456789012345678").is_ok());
        assert!(path_id("../auth/me").is_err());
        assert!(path_id("").is_err());
    }
}
