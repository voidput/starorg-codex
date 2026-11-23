//! StarOrg Tools in the real backend (addendum §5): sign-in, the org choice, pull, push,
//! the hangar and the automatic run after the game exits, over `starorg_net::starorg`.
//!
//! - Nothing is sent while Settings -> Network -> "Connect StarOrg Tools" is off
//!   (`network-off`), and each kind only while its "What to sync" switch is on.
//! - Environment and switches: `config.toml` `[sync]` (shared with the CLI and hooks). Per
//!   environment, the non-secret account card (user, orgs, expiry, last sync) is in
//!   `app.json`; the token is in [`crate::tokens`]. The keyring is only touched once an
//!   account card says there is a token.
//! - There is no refresh token (the server issues 90-day tokens); an expired, revoked or
//!   refused token is dropped and the status says to sign in again.

use super::convert::err;
use super::*;
use crate::tokens::{KeyringOrFile, TokenStore};
use starorg_core::blueprints::OwnedSet;
use starorg_core::starorg::{self as so, Environment, SyncPrefs};
use starorg_net::starorg::{self as net, Client, StarOrgError};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Opens a URL in the system browser (tests: [`net::mock::fake_browser`]).
pub type Browser = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// What the StarOrg part needs from outside; tests swap all three.
pub struct StarOrgDeps {
    pub tokens: Arc<dyn TokenStore>,
    /// Base URL per environment. The real ones are the two hosts; tests a loopback mock.
    pub base: Arc<dyn Fn(StarOrgEnv) -> String + Send + Sync>,
    pub browser: Browser,
    pub login_timeout: Duration,
}

impl StarOrgDeps {
    pub fn system(token_dir: PathBuf) -> Self {
        Self {
            tokens: Arc::new(KeyringOrFile::new(token_dir)),
            base: Arc::new(|e| core_env(e).base_url()),
            browser: Arc::new(|url| {
                // Our own sign-in URL only, never one from the server.
                if !url.starts_with("https://starorg.tools/api/auth/strings/login?")
                    && !url.starts_with("https://dev.starorg.tools/api/auth/strings/login?")
                {
                    return Err("refusing to open an unexpected URL".into());
                }
                open::that_detached(url).map_err(|e| e.to_string())
            }),
            login_timeout: Duration::from_secs(300),
        }
    }

    /// A test server for both environments, an in-memory token store and the fake browser.
    pub fn local(base: &str, browser: Browser) -> Self {
        let b = base.to_string();
        Self {
            tokens: Arc::new(crate::tokens::MemoryStore::default()),
            base: Arc::new(move |_| b.clone()),
            browser,
            login_timeout: Duration::from_secs(20),
        }
    }
}

/// Per-environment account card in `app.json` (no secrets).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct Account {
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub guild: Option<String>,
    #[serde(default)]
    pub guilds: Vec<StarOrgGuild>,
    /// Unix seconds.
    #[serde(default)]
    pub expires_at: Option<u64>,
    #[serde(default)]
    pub last_sync: Option<String>,
    #[serde(default)]
    pub token_store: Option<TokenStoreKind>,
    /// A token is (believed to be) stored.
    #[serde(default)]
    pub signed_in: bool,
    /// Dropped after expiry or a refusal.
    #[serde(default)]
    pub expired: bool,
}

#[derive(Default)]
pub(super) struct StarOrgMem {
    pub login_cancel: AtomicBool,
    /// Last seen "game running" per channel, for the run after exit.
    pub running: Mutex<HashMap<String, bool>>,
}

pub(super) fn core_env(e: StarOrgEnv) -> Environment {
    match e {
        StarOrgEnv::Production => Environment::Production,
        StarOrgEnv::Dev => Environment::Dev,
    }
}

pub(super) fn contract_env(e: Environment) -> StarOrgEnv {
    match e {
        Environment::Production => StarOrgEnv::Production,
        Environment::Dev => StarOrgEnv::Dev,
    }
}

pub(super) fn settings_of(p: &SyncPrefs) -> StarOrgSettings {
    StarOrgSettings {
        environment: contract_env(p.environment),
        import_game_log: p.import_game_log,
        push_blueprints: p.push_blueprints,
        pull_blueprints: p.pull_blueprints,
        pull_fleet: p.pull_fleet,
        auto_sync: p.auto_sync,
    }
}

pub(super) fn prefs_of(s: &StarOrgSettings) -> SyncPrefs {
    SyncPrefs {
        environment: core_env(s.environment),
        import_game_log: s.import_game_log,
        push_blueprints: s.push_blueprints,
        pull_blueprints: s.pull_blueprints,
        pull_fleet: s.pull_fleet,
        auto_sync: s.auto_sync,
    }
}

fn env_key(e: StarOrgEnv) -> &'static str {
    core_env(e).as_str()
}

fn user_agent() -> String {
    concat!("starorg-codex/", env!("CARGO_PKG_VERSION")).to_string()
}

fn off(what: &str) -> AppError {
    err(
        ErrorCode::Invalid,
        format!("{what} is off under Settings -> StarOrg Tools -> What to sync."),
    )
}

/// Index of the public blueprint lookup: names (label and other names) and record keys.
struct Lookup {
    by_name: HashMap<String, BTreeSet<i64>>,
    by_key: HashMap<String, BTreeSet<i64>>,
}

impl Lookup {
    fn new(l: &net::Lookup) -> Self {
        let mut by_name: HashMap<String, BTreeSet<i64>> = HashMap::new();
        let mut by_key: HashMap<String, BTreeSet<i64>> = HashMap::new();
        for r in &l.blueprints {
            for n in std::iter::once(&r.n).chain(r.a.iter().flatten()) {
                let k = blueprints::normalize(n);
                if !k.is_empty() {
                    by_name.entry(k).or_default().insert(r.id);
                }
            }
            for k in std::iter::once(&r.k).chain(r.p.iter()) {
                let k = net::canonical_key(k);
                if !k.is_empty() {
                    by_key.entry(k).or_default().insert(r.id);
                }
            }
        }
        Self { by_name, by_key }
    }

    /// One server id for a local entry: its record id first, then its name. `None` when
    /// unknown or ambiguous.
    fn find(&self, e: &blueprints::OwnedEntry) -> Option<i64> {
        let one =
            |s: Option<&BTreeSet<i64>>| s.filter(|s| s.len() == 1).and_then(|s| s.first().copied());
        let by_key: BTreeSet<i64> = e
            .ids
            .iter()
            .filter_map(|id| self.by_key.get(&net::canonical_key(id)))
            .flatten()
            .copied()
            .collect();
        if by_key.len() == 1 {
            return by_key.first().copied();
        }
        if !by_key.is_empty() {
            return None;
        }
        one(self.by_name.get(&blueprints::normalize(&e.name)))
    }
}

impl RealBackend {
    pub(super) fn prefs(&self) -> SyncPrefs {
        SyncPrefs::load(self.dirs())
    }

    fn env(&self) -> StarOrgEnv {
        contract_env(self.prefs().environment)
    }

    pub(super) fn account(&self, env: StarOrgEnv) -> Account {
        self.app_file()
            .starorg
            .get(env_key(env))
            .cloned()
            .unwrap_or_default()
    }

    fn save_account(&self, env: StarOrgEnv, acc: Account) -> CmdResult<()> {
        let mut a = self.app_file();
        a.starorg.insert(env_key(env).to_string(), acc);
        self.save_app_file(&a)
    }

    fn network_on(&self) -> CmdResult<()> {
        if self.settings().network.starorg {
            Ok(())
        } else {
            Err(err(
                ErrorCode::NetworkOff,
                "Turn on \"Connect StarOrg Tools\" under Settings -> Network first.",
            ))
        }
    }

    fn client(&self, env: StarOrgEnv) -> CmdResult<Client> {
        self.network_on()?;
        Client::new(&(self.starorg.base)(env), &user_agent()).map_err(|e| self.so_err(env, e))
    }

    /// A client with a live token, or why there is none (expired tokens are dropped here).
    fn authed(&self) -> CmdResult<(Client, Account, StarOrgEnv)> {
        let env = self.env();
        self.sign_out_other_envs(env)?;
        let c = self.client(env)?;
        let mut acc = self.account(env);
        let not_in = || {
            err(
                ErrorCode::Invalid,
                "Not signed in to StarOrg Tools. Sign in under Settings -> StarOrg Tools.",
            )
        };
        if !acc.signed_in {
            return Err(not_in());
        }
        if acc.expires_at.is_some_and(|t| t <= fsutil::unix_now()) {
            self.drop_token(env, &mut acc);
            return Err(self.so_err(env, StarOrgError::Unauthorized("expired".into())));
        }
        let token = match self.starorg.tokens.get(env.into()) {
            Ok(Some((t, _))) => t,
            Ok(None) => {
                self.drop_token(env, &mut acc);
                return Err(not_in());
            }
            Err(e) => {
                return Err(err(
                    ErrorCode::Internal,
                    format!("Cannot read the sign-in token: {e}"),
                ));
            }
        };
        Ok((c.with_token(token), acc, env))
    }

    fn drop_token(&self, env: StarOrgEnv, acc: &mut Account) {
        let _ = self.starorg.tokens.delete(env.into());
        acc.signed_in = false;
        acc.expired = true;
        acc.token_store = None;
        let _ = self.save_account(env, acc.clone());
    }

    /// Contract error for a client error. A refused token is dropped on the way.
    fn so_err(&self, env: StarOrgEnv, e: StarOrgError) -> AppError {
        let host = core_env(env).host();
        let (code, msg) = match &e {
            StarOrgError::Network(_) => (ErrorCode::Internal, format!("Could not reach {host}.")),
            StarOrgError::Disabled => (
                ErrorCode::NotImplemented,
                format!(
                    "Sign-in for StarOrg Codex is not enabled on {host} yet. Nothing was sent."
                ),
            ),
            StarOrgError::Unauthorized(_) => {
                let mut acc = self.account(env);
                if acc.signed_in {
                    self.drop_token(env, &mut acc);
                }
                (
                    ErrorCode::Invalid,
                    format!("The {host} sign-in expired or was revoked. Sign in again."),
                )
            }
            StarOrgError::NotFound(what) => (
                ErrorCode::NotFound,
                format!(
                    "{host} has no {what} for this org (its blueprint tracking may be off). Pick another org."
                ),
            ),
            StarOrgError::RateLimited => (
                ErrorCode::Busy,
                format!("{host} asks to slow down; try again in a minute."),
            ),
            StarOrgError::Refused { .. } => {
                (ErrorCode::Invalid, format!("{host} refused the request."))
            }
            StarOrgError::Protocol(_) => (
                ErrorCode::Internal,
                format!("Unexpected answer from {host}."),
            ),
            StarOrgError::StateMismatch => (
                ErrorCode::Invalid,
                "The browser returned a sign-in this app did not start. Nothing was linked.".into(),
            ),
            StarOrgError::Cancelled(_) => (
                ErrorCode::Cancelled,
                "Sign-in cancelled. Nothing was linked.".into(),
            ),
            StarOrgError::Timeout => (
                ErrorCode::Cancelled,
                "No answer from the browser in time. Sign-in stopped.".into(),
            ),
        };
        let mut a = err(code, msg);
        a.detail = Some(e.to_string());
        a
    }

    /// The first registered install's channel, for the sync commands (which name a
    /// channel only, like the status).
    fn sync_target(&self, channel: &str) -> CmdResult<(Found, PathBuf)> {
        let f = self
            .installs()
            .into_iter()
            .find(|f| f.id.is_some() && f.install.channel(channel).is_some())
            .ok_or_else(|| {
                err(
                    ErrorCode::NeedsDecision,
                    format!("{channel} is not set up yet"),
                )
            })?;
        let data = self.data_dir(&f, f.channel(channel)?);
        Ok((f, data))
    }

    pub(super) fn starorg_view(&self, channel: &str) -> CmdResult<StarOrgStatus> {
        let s = self.settings();
        let env = s.starorg.environment;
        self.sign_out_other_envs(env)?;
        let a = self.app_file();
        let ticked = a.sync.get(channel).cloned();
        let mut chars = Vec::new();
        let mut union: BTreeSet<String> = BTreeSet::new();
        if let Some(f) = self.installs().into_iter().find(|f| f.id.is_some())
            && let Some(ch) = f.install.channel(channel)
        {
            let data = self.data_dir(&f, ch);
            for (g, c) in blueprints::load_characters(&data)
                .unwrap_or_default()
                .characters
            {
                let set =
                    blueprints::load_set(&data, &g).unwrap_or_else(|_| OwnedSet::new(Some(&g)));
                let on = ticked.as_ref().is_none_or(|t| t.contains(&g));
                if on {
                    union.extend(set.entries.keys().cloned());
                }
                chars.push(SyncCharacter {
                    ticked: on,
                    owned: set.entries.len() as u32,
                    id: g,
                    name: c.name,
                });
            }
        }
        let mut acc = a.starorg.get(env_key(env)).cloned().unwrap_or_default();
        // Expired while the app was closed: drop it now (touches the store only then).
        if acc.signed_in && acc.expires_at.is_some_and(|t| t <= fsutil::unix_now()) {
            self.drop_token(env, &mut acc);
        }
        let guild = acc
            .guild
            .as_ref()
            .and_then(|id| acc.guilds.iter().find(|g| &g.id == id));
        Ok(StarOrgStatus {
            enabled: s.network.starorg,
            connected: acc.signed_in,
            environment: env,
            host: core_env(env).host().to_string(),
            user: acc.user.clone(),
            guild: guild.map(|g| g.name.clone()),
            guild_id: guild.map(|g| g.id.clone()),
            guilds: acc.guilds.clone(),
            expires: acc.expires_at.map(fsutil::rfc3339),
            expired: acc.expired && !acc.signed_in,
            token_store: acc.token_store.filter(|_| acc.signed_in),
            last_sync: acc.last_sync.clone(),
            sync_characters: chars,
            push_count: union.len() as u32,
        })
    }

    pub(super) fn starorg_login_impl(&self, channel: &str) -> CmdResult<StarOrgStatus> {
        let env = self.env();
        let c = self.client(env)?;
        self.starorg_mem
            .login_cancel
            .store(false, Ordering::Relaxed);
        let cancel = || self.starorg_mem.login_cancel.load(Ordering::Relaxed);
        let grant = c
            .sign_in(
                self.starorg.browser.as_ref(),
                self.starorg.login_timeout,
                &cancel,
            )
            .map_err(|e| self.so_err(env, e))?;
        let now = fsutil::unix_now();
        let store = self
            .starorg
            .tokens
            .set(env.into(), &grant.access_token)
            .map_err(|e| {
                err(
                    ErrorCode::Internal,
                    format!("Cannot keep the sign-in token: {e}"),
                )
            })?;
        let c = c.with_token(grant.access_token.clone());
        let me = match c.me() {
            // Refused on the first call: drop the token and save no signed-in card.
            Err(e @ StarOrgError::Unauthorized(_)) => {
                let _ = self.starorg.tokens.delete(env.into());
                return Err(self.so_err(env, e));
            }
            r => r,
        };
        let old = self.account(env);
        let mut acc = Account {
            user: Some(grant.user.username.clone()),
            expires_at: Some(now + grant.expires_in),
            token_store: Some(store),
            signed_in: true,
            last_sync: old.last_sync,
            ..Account::default()
        };
        if let Ok(me) = me {
            acc.guilds = guilds_of(&me);
            // Keep the earlier choice when it is still one of the orgs; else the first org
            // with blueprint tracking.
            acc.guild = old
                .guild
                .filter(|g| acc.guilds.iter().any(|x| &x.id == g))
                .or_else(|| {
                    acc.guilds
                        .iter()
                        .find(|g| g.blueprints)
                        .map(|g| g.id.clone())
                })
                .or_else(|| acc.guilds.first().map(|g| g.id.clone()));
        }
        self.save_account(env, acc)?;
        self.log(
            LogLevel::Info,
            Some(JobKind::Sync),
            None,
            &format!("Signed in to {}", core_env(env).host()),
        );
        self.starorg_view(channel)
    }

    pub(super) fn starorg_cancel_login_impl(&self) {
        self.starorg_mem.login_cancel.store(true, Ordering::Relaxed);
    }

    /// Revoke on the server when the network switch allows it (best effort), then forget the
    /// token and the account card here. Returns whether the server confirmed the revoke; when
    /// it did not (switch off, offline, refused) the token is gone from this computer but
    /// stays valid on the server until it expires.
    pub(super) fn sign_out(&self, env: StarOrgEnv) -> CmdResult<bool> {
        let acc = self.account(env);
        let revoked = acc.signed_in
            && self.settings().network.starorg
            && match (
                self.starorg.tokens.get(env.into()),
                Client::new(&(self.starorg.base)(env), &user_agent()),
            ) {
                (Ok(Some((t, _))), Ok(c)) => c.with_token(t).logout().is_ok(),
                _ => false,
            };
        self.starorg.tokens.delete(env.into()).map_err(|e| {
            err(
                ErrorCode::Internal,
                format!("Cannot remove the sign-in token: {e}"),
            )
        })?;
        let mut a = self.app_file();
        a.starorg.remove(env_key(env));
        self.save_app_file(&a)?;
        if acc.signed_in {
            let host = core_env(env).host();
            let msg = if revoked {
                format!("Signed out of {host}; the token was revoked there")
            } else {
                format!(
                    "Signed out of {host} on this computer only; the token was not revoked there and stays valid until it expires"
                )
            };
            self.log(
                if revoked {
                    LogLevel::Info
                } else {
                    LogLevel::Warn
                },
                Some(JobKind::Sync),
                None,
                &msg,
            );
        }
        Ok(revoked)
    }

    /// Sign out of every environment but the chosen one. The app's own switch does this
    /// itself; this catches `config.toml` changed by the CLI or by hand.
    pub(super) fn sign_out_other_envs(&self, env: StarOrgEnv) -> CmdResult<()> {
        for other in [StarOrgEnv::Production, StarOrgEnv::Dev] {
            if other != env && self.account(other).signed_in {
                self.sign_out(other)?;
            }
        }
        Ok(())
    }

    pub(super) fn starorg_logout_impl(&self, channel: &str) -> CmdResult<StarOrgStatus> {
        self.sign_out(self.env())?;
        self.starorg_view(channel)
    }

    pub(super) fn set_starorg_guild_impl(
        &self,
        req: &StarOrgGuildRequest,
    ) -> CmdResult<StarOrgStatus> {
        let env = self.env();
        let mut acc = self.account(env);
        if !acc.guilds.iter().any(|g| g.id == req.guild) {
            return Err(err(
                ErrorCode::Invalid,
                "Pick one of your orgs on StarOrg Tools.",
            ));
        }
        acc.guild = Some(req.guild.clone());
        self.save_account(env, acc)?;
        self.starorg_view(&req.channel)
    }

    fn guild_of(&self, acc: &Account) -> CmdResult<(String, String)> {
        acc.guild
            .as_ref()
            .and_then(|id| acc.guilds.iter().find(|g| &g.id == id))
            .map(|g| (g.id.clone(), g.name.clone()))
            .ok_or_else(|| {
                err(
                    ErrorCode::NeedsDecision,
                    "Pick the org to sync with under Settings -> StarOrg Tools.",
                )
            })
    }

    fn mark_synced(&self, env: StarOrgEnv) -> String {
        let t = now();
        let mut acc = self.account(env);
        acc.last_sync = Some(t.clone());
        let _ = self.save_account(env, acc);
        t
    }

    pub(super) fn starorg_pull_impl(&self, req: &StarOrgPullRequest) -> CmdResult<SyncReport> {
        self.network_on()?;
        if !self.prefs().pull_blueprints {
            return Err(off("Pull owned blueprints"));
        }
        let (c, acc, env) = self.authed()?;
        let (gid, gname) = self.guild_of(&acc)?;
        let (f, data) = self.sync_target(&req.channel)?;
        let ch = f.channel(&req.channel)?;
        let chars = blueprints::load_characters(&data).map_err(io)?;
        if !chars.characters.contains_key(&req.character) {
            return Err(err(
                ErrorCode::Invalid,
                "Pick one of the characters found in the logs.",
            ));
        }
        let rows = c.my_blueprints(&gid).map_err(|e| self.so_err(env, e))?;
        // Names this build's game data knows, when it has been read (else everything goes in).
        let known = self.loaded(&f, ch, false).ok().map(|d| {
            starorg_core::blueprints::catalog::index(&GameDataCatalog::new(&d.game_data, &d.stock))
        });
        let id = f.id.clone().unwrap_or_default();
        let _l = self.blueprint_lock(&id, ch)?;
        let mut set = blueprints::load_set(&data, &req.character).map_err(io)?;
        let (mut added, mut already, mut unmatched) = (Vec::new(), 0u32, Vec::new());
        for r in rows {
            let key = blueprints::normalize(&r.name);
            if key.is_empty() {
                continue;
            }
            if known.as_ref().is_some_and(|k| !k.contains_key(&key)) {
                unmatched.push(r.name);
                continue;
            }
            let src = blueprints::Source {
                kind: format!("starorg:{gname}"),
                time: r.acquired_at.clone(),
                file: None,
            };
            if set.add(&r.name, src) {
                added.push(blueprints::strip_tags(&r.name).to_string());
            } else {
                already += 1;
            }
        }
        blueprints::save_set(&data, &set).map_err(io)?;
        drop(_l);
        self.forget_tables(&f, ch);
        let time = self.mark_synced(env);
        self.log(
            LogLevel::Info,
            Some(JobKind::Sync),
            Some(&req.channel),
            &format!(
                "Pulled from StarOrg ({gname}): {} added, {already} already known",
                added.len()
            ),
        );
        Ok(SyncReport {
            direction: SyncDirection::Pull,
            channel: req.channel.clone(),
            guild: Some(gname),
            character: Some(req.character.clone()),
            added,
            already,
            unmatched,
            sent: 0,
            time,
        })
    }

    pub(super) fn starorg_push_impl(&self, channel: &str) -> CmdResult<SyncReport> {
        self.network_on()?;
        if !self.prefs().push_blueprints {
            return Err(off("Push owned blueprints"));
        }
        let (c, acc, env) = self.authed()?;
        let (gid, gname) = self.guild_of(&acc)?;
        let (_f, data) = self.sync_target(channel)?;
        let ticked = self.app_file().sync.get(channel).cloned();
        // Q22: the union of the ticked characters' sets, earliest time per name.
        let mut union: BTreeMap<String, blueprints::OwnedEntry> = BTreeMap::new();
        for g in blueprints::load_characters(&data)
            .unwrap_or_default()
            .characters
            .keys()
            .filter(|g| ticked.as_ref().is_none_or(|t| t.contains(g)))
        {
            let set = blueprints::load_set(&data, g).map_err(io)?;
            for (k, e) in set.entries {
                match union.get_mut(&k) {
                    Some(u) => {
                        for s in e.sources {
                            u.add_source(s);
                        }
                        for id in e.ids {
                            if !u.ids.contains(&id) {
                                u.ids.push(id);
                            }
                        }
                    }
                    None => {
                        union.insert(k, e);
                    }
                }
            }
        }
        let lookup = Lookup::new(&c.blueprint_lookup().map_err(|e| self.so_err(env, e))?);
        let mut ids: Vec<i64> = Vec::new();
        let mut names: HashMap<i64, String> = HashMap::new();
        let mut times: BTreeMap<i64, String> = BTreeMap::new();
        let mut unmatched = Vec::new();
        for e in union.values() {
            match lookup.find(e) {
                Some(id) => {
                    if let std::collections::hash_map::Entry::Vacant(v) = names.entry(id) {
                        ids.push(id);
                        v.insert(e.name.clone());
                    }
                    // Receipt times only: a log line's or the server's own. A Smart Citizen
                    // import's time is when the file was exported, not when it was received.
                    if let Some(t) = e
                        .sources
                        .iter()
                        .filter(|s| s.kind != blueprints::SOURCE_SMART_CITIZEN)
                        .filter_map(|s| s.time.clone())
                        .min()
                    {
                        let slot = times.entry(id).or_insert_with(|| t.clone());
                        if t < *slot {
                            *slot = t;
                        }
                    }
                }
                None => unmatched.push(e.name.clone()),
            }
        }
        let r = if ids.is_empty() {
            net::AddResult::default()
        } else {
            c.add_blueprints(&gid, &ids, &times)
                .map_err(|e| self.so_err(env, e))?
        };
        let time = self.mark_synced(env);
        let added: Vec<String> = r
            .added
            .iter()
            .filter_map(|id| names.get(id).cloned())
            .collect();
        self.log(
            LogLevel::Info,
            Some(JobKind::Sync),
            Some(channel),
            &format!(
                "Pushed {} blueprints to StarOrg ({gname}): {} new there",
                ids.len(),
                added.len()
            ),
        );
        Ok(SyncReport {
            direction: SyncDirection::Push,
            channel: channel.to_string(),
            guild: Some(gname),
            character: None,
            added,
            already: r.already_owned.len() as u32,
            unmatched,
            sent: ids.len() as u32,
            time,
        })
    }

    /// The hangar as a HangarXPLOR-shaped list for the fleet importer.
    pub(super) fn starorg_hangar_bytes(&self) -> CmdResult<Vec<u8>> {
        self.network_on()?;
        if !self.prefs().pull_fleet {
            return Err(off("Pull fleet/hangar"));
        }
        let (c, _, env) = self.authed()?;
        let h = c.hangar().map_err(|e| self.so_err(env, e))?;
        let mut rows = Vec::new();
        for v in &h {
            let Some(name) = v.vehicle.name.as_deref().filter(|n| !n.trim().is_empty()) else {
                continue;
            };
            for i in 0..v.amount.clamp(1, 100) {
                rows.push(serde_json::json!({
                    "name": name,
                    "ship_name": if i == 0 { v.custom_name.clone() } else { None },
                    "manufacturer_name": v.vehicle.manufacturer,
                    "entity_type": "ship",
                }));
            }
        }
        if rows.is_empty() {
            return Err(err(
                ErrorCode::NotFound,
                "The StarOrg Tools hangar has no ships.",
            ));
        }
        let _ = self.mark_synced(env);
        serde_json::to_vec(&rows).map_err(io)
    }

    pub(super) fn starorg_sync_impl(&self, channel: &str) -> CmdResult<SyncRun> {
        self.network_on()?;
        let p = self.prefs();
        if !p.any_network() {
            return Err(err(
                ErrorCode::Invalid,
                "Nothing to sync: turn on a StarOrg switch under Settings -> StarOrg Tools -> What to sync.",
            ));
        }
        // Signed in and an org picked, or nothing runs.
        let (_, acc, _) = self.authed()?;
        if p.pull_blueprints || p.push_blueprints {
            self.guild_of(&acc)?;
        }
        let (f, data) = self.sync_target(channel)?;
        let mut reports = Vec::new();
        let mut skipped = Vec::new();
        let mut fleet = None;
        if p.pull_blueprints {
            let ch = f.channel(channel)?;
            let who = self
                .characters_view(&f, ch)
                .ok()
                .and_then(|v| v.active.resolved.or(v.active.character));
            let who = who.or_else(|| {
                blueprints::load_characters(&data)
                    .ok()?
                    .newest()
                    .map(str::to_string)
            });
            match who {
                Some(c) => match self.starorg_pull_impl(&StarOrgPullRequest {
                    channel: channel.into(),
                    character: c,
                }) {
                    Ok(r) => reports.push(r),
                    Err(e) => skipped.push(format!("Pull: {}", e.message)),
                },
                None => skipped.push("Pull: no character found in the logs yet".into()),
            }
        }
        if p.push_blueprints {
            match self.starorg_push_impl(channel) {
                Ok(r) => reports.push(r),
                Err(e) => skipped.push(format!("Push: {}", e.message)),
            }
        }
        if p.pull_fleet {
            let req = FleetImportRequest {
                target: ChannelRef {
                    install: f.ui_id(),
                    channel: channel.into(),
                },
                source: FleetSource::StarOrg,
                dry_run: false,
                merge: true,
                prune: false,
            };
            match self.fleet_import(req) {
                Ok(r) => fleet = Some(r),
                Err(e) => skipped.push(format!("Fleet: {}", e.message)),
            }
        }
        Ok(SyncRun {
            channel: channel.into(),
            reports,
            fleet,
            skipped,
            time: now(),
        })
    }

    /// The run after the game exits (Settings -> StarOrg Tools -> "Sync automatically after
    /// game exit"): channels the post-exit hook noted, and channels this app saw go from
    /// running to stopped. Call it every few seconds; it does nothing unless the switch, the
    /// network switch and a sign-in are all there.
    pub fn starorg_auto_tick(&self) -> Vec<SyncRun> {
        self.auto_tick_with(&|prefix, channel, dir| {
            starorg_core::running::game_running(prefix, channel, dir).is_some()
        })
    }

    pub fn auto_tick_with(
        &self,
        running: &dyn Fn(Option<&Path>, &str, &Path) -> bool,
    ) -> Vec<SyncRun> {
        let p = self.prefs();
        let env = contract_env(p.environment);
        if !p.auto_sync
            || !p.any_network()
            || !self.settings().network.starorg
            || !self.account(env).signed_in
        {
            return Vec::new();
        }
        let mut due: BTreeSet<String> = so::take_pending(self.dirs());
        if let Some(f) = self.installs().into_iter().find(|f| f.id.is_some()) {
            let mut seen = self
                .starorg_mem
                .running
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            for ch in &f.install.channels {
                let t = Target::new(&f.install, f.id.clone(), ch);
                let now = running(t.prefix.as_deref(), &ch.name, &ch.dir);
                if seen.insert(ch.name.clone(), now) == Some(true) && !now {
                    due.insert(ch.name.clone());
                }
            }
        }
        let mut out = Vec::new();
        for ch in due {
            // Set-up channels only; read their logs first (when the Game.log import is on),
            // as the hook does.
            let Some(f) = self.installs().into_iter().find(|f| f.id.is_some()) else {
                continue;
            };
            let Some(c) = f.install.channel(&ch) else {
                continue;
            };
            if !self.is_set_up(&f, c) {
                continue;
            }
            self.scan(&f, c);
            match self.starorg_sync_impl(&ch) {
                Ok(r) => out.push(r),
                Err(e) => self.log(
                    LogLevel::Warn,
                    Some(JobKind::Sync),
                    Some(&ch),
                    &format!("Automatic StarOrg sync skipped: {}", e.message),
                ),
            }
        }
        out
    }
}

fn guilds_of(me: &net::Me) -> Vec<StarOrgGuild> {
    me.guilds
        .iter()
        .map(|g| StarOrgGuild {
            id: g.discord_id.clone(),
            name: g.name.clone(),
            blueprints: g.features.iter().any(|f| f == "industrial"),
        })
        .collect()
}
