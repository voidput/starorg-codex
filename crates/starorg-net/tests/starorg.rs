//! The StarOrg Tools client against the local mock server (no real network).

use starorg_net::starorg::mock::{LoginMode, MockServer, fake_browser};
use starorg_net::starorg::{Client, StarOrgError};
use std::collections::BTreeMap;
use std::time::Duration;

fn client(m: &MockServer) -> Client {
    Client::new(&m.base, "starorg-strings-test").unwrap()
}

fn sign_in(m: &MockServer) -> Result<starorg_net::starorg::TokenGrant, StarOrgError> {
    client(m).sign_in(&fake_browser, Duration::from_secs(15), &|| false)
}

#[test]
fn sign_in_runs_the_loopback_pkce_flow() {
    let m = MockServer::start();
    let g = sign_in(&m).unwrap();
    assert_eq!(g.aud.as_deref(), Some("strings"));
    assert_eq!(g.user.username, "test_pilot_discord");
    assert_eq!(g.expires_in, 90 * 86400);
    assert!(!format!("{g:?}").contains(&g.access_token));
    let r = m.requests();
    let paths: Vec<&str> = r.iter().map(|x| x.path.as_str()).collect();
    // Probe, browser leg, exchange; the token is never sent before it exists.
    assert_eq!(
        paths,
        [
            "/api/auth/strings/login",
            "/api/auth/strings/login",
            "/api/auth/strings/exchange"
        ]
    );
    assert!(r.iter().all(|x| !x.bearer));
    assert!(r[2].body.contains("\"redirect_uri\":\"http://127.0.0.1:"));
    assert!(r[2].body.contains("code_verifier"));
    // The token works on the allowed routes.
    let c = client(&m).with_token(g.access_token);
    let me = c.me().unwrap();
    assert_eq!(me.guilds.len(), 2);
    assert_eq!(me.guilds[0].discord_id, "100000000000000001");
}

#[test]
fn disabled_server_is_refused_before_the_browser_opens() {
    let m = MockServer::start();
    m.state().enabled = false;
    let opened = std::sync::atomic::AtomicBool::new(false);
    let r = client(&m).sign_in(
        &|_| {
            opened.store(true, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        },
        Duration::from_secs(5),
        &|| false,
    );
    assert_eq!(r.unwrap_err(), StarOrgError::Disabled);
    assert!(!opened.load(std::sync::atomic::Ordering::Relaxed));
}

#[test]
fn cancel_and_foreign_state_end_the_attempt_without_a_token() {
    let m = MockServer::start();
    m.state().login = LoginMode::Deny;
    assert!(matches!(sign_in(&m), Err(StarOrgError::Cancelled(_))));
    m.state().login = LoginMode::WrongState;
    assert_eq!(sign_in(&m).unwrap_err(), StarOrgError::StateMismatch);
    assert_eq!(m.valid_tokens(), 0);
    assert!(
        !m.requests()
            .iter()
            .any(|r| r.path == "/api/auth/strings/exchange")
    );
}

#[test]
fn exchange_needs_the_right_verifier_and_redirect() {
    let m = MockServer::start();
    let c = client(&m);
    let e = c
        .exchange(
            "code-not-issued-by-the-server",
            &"v".repeat(43),
            "http://127.0.0.1:5000/callback",
        )
        .unwrap_err();
    assert!(
        matches!(e, StarOrgError::Refused { status: 400, .. }),
        "{e:?}"
    );
}

#[test]
fn blueprint_routes_pull_push_and_lookup() {
    let m = MockServer::start();
    let t = m.issue_token();
    let c = client(&m).with_token(t);
    let guild = "100000000000000001";
    let mine = c.my_blueprints(guild).unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].name, "Sample Medical Gun");
    let lookup = c.blueprint_lookup().unwrap();
    assert_eq!(lookup.blueprints.len(), 5);
    let at = BTreeMap::from([(1, "2026-03-01T10:05:00.000Z".to_string())]);
    let r = c.add_blueprints(guild, &[1, 3], &at).unwrap();
    assert_eq!(r.added, [1]);
    assert_eq!(r.already_owned, [3]);
    let reqs = m.requests();
    let lk = reqs
        .iter()
        .find(|r| r.path == "/api/industrial/blueprint-lookup")
        .unwrap();
    assert!(!lk.bearer, "the public lookup gets no token");
    let add = reqs.iter().find(|r| r.method == "POST").unwrap();
    assert!(
        add.body.contains("\"source\":\"starorg_strings\""),
        "{}",
        add.body
    );
    assert!(add.body.contains("\"acquired_at\":{\"1\":"), "{}", add.body);
    // An org without blueprint tracking is a 404.
    assert!(matches!(
        c.my_blueprints("100000000000000002"),
        Err(StarOrgError::NotFound(_))
    ));
    assert!(c.my_blueprints("../../auth/me").is_err());
}

#[test]
fn hangar_logout_and_revoked_token() {
    let m = MockServer::start();
    let t = m.issue_token();
    let c = client(&m).with_token(t);
    let h = c.hangar().unwrap();
    assert_eq!(h.len(), 2);
    assert_eq!(h[1].amount, 2);
    assert_eq!(h[0].vehicle.name.as_deref(), Some("Synthetic Alpha"));
    c.logout().unwrap();
    assert_eq!(m.state().revoked, 1);
    assert!(matches!(c.me(), Err(StarOrgError::Unauthorized(_))));
    // Sign-in turned off on the server: an old token is a 403 "disabled".
    let t = m.issue_token();
    m.state().enabled = false;
    let c = client(&m).with_token(t);
    assert_eq!(c.me().unwrap_err(), StarOrgError::Disabled);
    // No token at all: refused locally, nothing sent.
    let before = m.requests().len();
    assert!(matches!(
        client(&m).hangar(),
        Err(StarOrgError::Unauthorized(_))
    ));
    assert_eq!(m.requests().len(), before);
}
