//! Game.log signals and people: synthetic lines only (fake entity ids, TEST-NET addresses,
//! `TestPilot_*` handles).

use super::mock::{iso, session_lines};
use super::people::{clean_name, is_npc, toast_hits, valid_handle};
use super::signals::classify_zone;
use super::tail::Tail;
use super::*;
use std::io::Write;

const T0: i64 = 1_795_000_000_000;

fn at(ms: i64, body: &str) -> String {
    format!("<{}> {body}", iso(ms))
}

fn kinds(log: &GameLog) -> Vec<EventKind> {
    log.view("game_log", true, 0)
        .events
        .iter()
        .map(|e| e.kind)
        .collect()
}

fn nav(e: &str) -> String {
    format!(
        "[Notice] <Local Route Guard - Server Rerouted> [ItemNavigation][CL][1] | NOT AUTH | {e}[{e}]|CSCItemNavigation::PostInitialize::<lambda_1>::operator ()|FinalStop=0"
    )
}

fn hud(t: &str) -> String {
    format!(
        "[Notice] <SHUDEvent_OnNotification> Added notification \"{t}: \" [3] to queue. New queue size: 1"
    )
}

const TAIL_Q: &str =
    ": \" [1] to queue. New queue size: 1, MissionId: [00000000-0000-0000-0000-000000000000]";

#[test]
fn time_stamps() {
    assert_eq!(parse_ts("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(parse_ts("2000-03-01T00:00:00.5Z"), Some(951_868_800_500));
    assert_eq!(
        parse_ts("2026-11-21T12:34:56.789Z").map(iso),
        Some("2026-11-21T12:34:56.789Z".into())
    );
    assert_eq!(iso(T0).len(), 24);
    assert_eq!(parse_ts(&iso(T0)), Some(T0));
    for bad in [
        "",
        "2026-11-21",
        "2026-13-01T00:00:00Z",
        "2026-11-21T12:34:56",
        "2026-11-21T12:34:56.Z",
        "２０２６-11-21T12:34:56Z",
    ] {
        assert_eq!(parse_ts(bad), None, "{bad}");
    }
    assert_eq!(
        split_ts("<2026-11-21T00:00:00.000Z> body"),
        (Some("2026-11-21T00:00:00.000Z"), "body")
    );
    assert_eq!(split_ts("<Join PU> x"), (None, "<Join PU> x"));
}

#[test]
fn session_lines_build_the_session_without_addresses() {
    let mut log = GameLog::new(false);
    for l in [
        "Log started on Mon Nov 23 10:00:00 2026".to_owned(),
        "FileVersion: 5.0.202.1".into(),
        "Executable: D:\\Games\\StarCitizen\\PTU\\Bin64\\StarCitizen.exe".into(),
        at(
            T0,
            "[Trace] @session: 'aaaaaaaa-0000-0000-0000-000000000000'",
        ),
        at(
            T0,
            "[Notice] <AccountLoginQueueStatus> Queue: queue name pu - position 12 - eta (seconds) 90 [Login]",
        ),
        at(
            T0 + 1000,
            "[Notice] <AccountLoginQueueStatus> Queue: queue name pu - position 12 - eta (seconds) 80 [Login]",
        ),
        at(
            T0 + 2000,
            "[Notice] <Join PU> address[203.0.113.7] port[64090] shard[ptu_use1b_1_020] locationId[7] [Login]",
        ),
        at(
            T0 + 3000,
            "Loading screen for Stanton : abc closed after 10.0 seconds",
        ),
        at(T0 + 4000, "[Notice] <Hub> disconnected - status[3]"),
        at(T0 + 5000, "[Notice] <Hub> connected"),
        at(
            T0 + 6000,
            "[Notice] <Update Shard Id> New Shard Id: ptu_euw1a_2_007.",
        ),
        at(
            T0 + 7000,
            "[Notice] <Channel Disconnected> cause=1 reason=\"Timeout\" remoteAddr=203.0.113.7 gamerules=\"SC_Default\" uptime_secs=125.0",
        ),
        at(
            T0 + 7500,
            "[Notice] <Channel Disconnected> cause=1 reason=\"Timeout\" gamerules=\"SC_Default\"",
        ),
        at(
            T0 + 9000,
            "Loading screen for Frontend_Main : abc closed after 1.0 seconds",
        ),
    ] {
        log.fold_line(&l);
    }
    let v = log.view("game_log", true, 0);
    let s = &v.session;
    assert_eq!(s.channel.as_deref(), Some("PTU"));
    assert_eq!(s.build.as_deref(), Some("5.0.202.1"));
    assert_eq!(s.system.as_deref(), Some("Main menu"));
    assert_eq!(s.shard, None, "left the shard at the menu");
    assert_eq!(
        s.disconnects, 1,
        "the second channel's line is the same disconnect"
    );
    assert_eq!(s.last_disconnect_expected, Some(false));
    assert_eq!(s.queue_position, None, "cleared on join");
    use EventKind as K;
    assert_eq!(
        kinds(&log),
        [
            K::LogStarted,
            K::SessionQueue,
            K::SessionJoin,
            K::SessionSystem,
            K::SessionHubDown,
            K::SessionHubUp,
            K::SessionShardChange,
            K::SessionDropped,
            K::SessionLeave,
            K::SessionSystem
        ]
    );
    let e = &v.events;
    assert_eq!(e[2].detail, "ptu_use1b_1_020 (US East)");
    assert_eq!(e[6].detail, "ptu_euw1a_2_007 (EU West)");
    assert_eq!(e[7].detail, "Timeout (after 2m 05s)");
    let json = serde_json::to_string(&v).unwrap();
    for private in ["203.0.113.7", "64090", "aaaaaaaa"] {
        assert!(!json.contains(private), "{private} in {json}");
    }
}

#[test]
fn nearby_ships_burst_into_one_event_and_own_ships_are_excluded() {
    let mut log = GameLog::new(false);
    log.fold_line(&at(T0, "[Notice] <Vehicle Control Flow> CVehicleMovementBase::ClearDriver: Local client node [1] releasing control token for 'RSI_Perseus_000000000001' [1]"));
    for e in [
        "DRAK_Corsair_000000000101",
        "DRAK_Corsair_000000000102",
        "ANVL_Lightning_F8C_Exec_000000000103",
        "RSI_Perseus_000000000001",
        "DRAK_Corsair_000000000101",
    ] {
        log.fold_line(&at(T0 + 1000, &nav(e)));
    }
    // A NULL ENTITY line has no class: skipped.
    log.fold_line(&at(T0 + 1000, "[Notice] <Local Route Guard - Server Rerouted> [ItemNavigation][CL][1] | NOT AUTH | NULL ENTITY|CSCItemNavigation::PostInitialize|x"));
    log.end_poll(T0);
    log.fold_line(&at(T0 + 1500, &nav("ORIG_300i_000000000104")));
    log.end_poll(T0 + 200);
    let v = log.view("game_log", true, 0);
    let ev: Vec<&LogEvent> = v
        .events
        .iter()
        .filter(|e| e.kind == EventKind::NearbyShip)
        .collect();
    assert_eq!(ev.len(), 1, "the second sighting waits for the rate limit");
    assert_eq!(
        ev[0].detail,
        "3 new ships: DRAK Corsair, ANVL Lightning F8C Exec"
    );
    log.end_poll(T0 + 1300);
    let v = log.view("game_log", true, 0);
    assert_eq!(v.events.last().unwrap().detail, "ORIG 300i");
    assert_eq!(v.signals.nearby_count, 4);
    assert_eq!(v.signals.nearby[0].class, "ORIG_300i");
    let corsair = v
        .signals
        .nearby
        .iter()
        .find(|n| n.class == "DRAK_Corsair")
        .unwrap();
    assert_eq!(corsair.count, 2);
    assert_eq!(v.signals.own_ship.as_ref().unwrap().display, "RSI Perseus");
    let json = serde_json::to_string(&v).unwrap();
    assert!(
        !json.contains("000000000101"),
        "entity ids stay inside: {json}"
    );
}

#[test]
fn quantum_route_arrival_and_location() {
    let mut log = GameLog::new(false);
    let pipe = "[ItemNavigation][CL][1] | NOT AUTH | AEGS_Gladius_000000000001[000000000001]";
    log.fold_line(&at(T0, &format!("[Notice] <Player Selected Quantum Target - Local> {pipe}|CSCItemNavigation::OnPlayerSelectedQuantumTarget|Player has selected point Mock_Dest_1 as their destination, routing locally [Team_X]")));
    log.fold_line(&at(T0, &format!("[Notice] <Player Selected Quantum Target - Local> {pipe}|CSCItemNavigation::OnPlayerSelectedQuantumTarget|Player has selected point Mock_Dest_1 as their destination, routing locally [Team_X]")));
    log.fold_line(&at(T0 + 10, &format!("[Notice] <Calculate Route> {pipe}|CSCItemNavigation::CalculateRoute|Successfully calculated route to Mock_Dest_1 fuel estimate 250.5 [Team_X]")));
    log.fold_line(&at(T0 + 10, &format!("[Notice] <Calculate Route> {pipe}|CSCItemNavigation::CalculateRoute|Projected Start Location is Mock Moon for route to destination Mock_Dest_1 [Team_X]")));
    let v = log.view("game_log", true, 0);
    assert_eq!(v.signals.qt.state, "routed");
    assert_eq!(v.signals.qt.destination.as_deref(), Some("Mock_Dest_1"));
    assert_eq!(v.signals.qt.fuel_estimate, Some(250.5));
    assert_eq!(v.signals.location.as_deref(), Some("Mock Moon"));
    assert_eq!(
        kinds(&log),
        [EventKind::QtRoute],
        "a re-plan to the same point is one route"
    );
    log.fold_line(&at(T0 + 60_000, &format!("[Notice] <Quantum Drive Arrived - Arrived at Final Destination> {pipe}|CSCItemNavigation::OnQuantumDriveArrived|arrived")));
    log.fold_line(&at(T0 + 61_000, "[Notice] <Quantum Drive Arrived - Arrived at Final Destination> [ItemNavigation][CL][1] | NOT AUTH | MISC_Freelancer_000000000014[000000000014]|CSCItemNavigation::OnQuantumDriveArrived|arrived"));
    let v = log.view("game_log", true, 0);
    assert_eq!(v.signals.qt.state, "idle");
    assert_eq!(v.signals.qt.last_arrival.as_deref(), Some("Mock_Dest_1"));
    let d: Vec<&str> = v.events.iter().map(|e| e.detail.as_str()).collect();
    assert_eq!(d[1], "You (AEGS Gladius) arrived at Mock_Dest_1");
    assert_eq!(d[2], "MISC Freelancer arrived from quantum");
    assert_eq!(
        v.signals.nearby_count, 1,
        "another ship's arrival is a sighting; yours is not"
    );
    // A route without arrival times out (no abort line).
    log.fold_line(&at(T0 + 70_000, &format!("[Notice] <Player Selected Quantum Target - Local> {pipe}|CSCItemNavigation::OnPlayerSelectedQuantumTarget|Player has selected point Mock_Dest_2 as their destination, routing locally")));
    log.fold_line(&at(
        T0 + 70_000 + 11 * 60_000,
        &hud("Hangar Request Completed"),
    ));
    assert_eq!(log.view("game_log", true, 0).signals.qt.state, "idle");
}

#[test]
fn zones() {
    assert_eq!(
        classify_zone("Entering Armistice Zone - Combat Prohibited: ")
            .unwrap()
            .0,
        "armistice"
    );
    assert_eq!(
        classify_zone("Leaving Armistice Zone - Caution Advised")
            .unwrap()
            .2,
        Some(false)
    );
    let (k, _, _, j) = classify_zone("Entered Ungoverned Jurisdiction: ").unwrap();
    assert_eq!((k, j.as_deref()), ("ungoverned", Some("Ungoverned")));
    let (k, _, _, j) = classify_zone("Entered Mock Alliance Jurisdiction").unwrap();
    assert_eq!((k, j.as_deref()), ("jurisdiction", Some("Mock Alliance")));
    assert_eq!(
        classify_zone("Entered Monitored Space").unwrap().0,
        "monitored"
    );
    assert!(classify_zone("Hangar Request Completed").is_none());
    assert!(classify_zone("Entered the hangar").is_none());
    let mut log = GameLog::new(false);
    log.fold_line(&at(T0, &hud("Entering Armistice Zone - Combat Prohibited")));
    log.fold_line(&at(
        T0 + 1,
        &hud("Entering Armistice Zone - Combat Prohibited"),
    ));
    log.fold_line(&at(T0 + 2, &hud("Entered Monitored Space")));
    let v = log.view("game_log", true, 0);
    assert!(v.signals.armistice, "only Leaving Armistice clears it");
    assert_eq!(v.signals.zone.unwrap().kind, "monitored");
    assert_eq!(kinds(&log), [EventKind::Zone, EventKind::Zone]);
}

fn corpse(log: &mut GameLog, ms: i64, n: u32) {
    for i in 0..n {
        log.fold_line(&at(ms + i64::from(i), &format!("[Notice] <Adding non kept item [CSCActorCorpseUtils::PopulateItemPortForItemRecoveryEntitlement]> Item 'mock_{i}_00000000009{i} - Class(mock_{i})', Recorded data is: Port Name 'Body_ItemPort'")));
    }
}

const SPAWNED: &str = "[Notice] [CSessionManager::OnClientSpawned] Spawned! [Team_GameServices]";

#[test]
fn a_corpse_followed_by_your_respawn_is_a_death() {
    let mut log = GameLog::new(false);
    log.fold_line(&at(T0, SPAWNED));
    corpse(&mut log, T0 + 10_000, 3);
    log.end_poll(T0); // touched this poll
    log.end_poll(T0); // idle poll closes the list
    // Another corpse far from any respawn of yours: not your death.
    corpse(&mut log, T0 + 100_000, 2);
    corpse(&mut log, T0 + 200_000, 4); // a new list: the previous one closes
    log.fold_line(&at(T0 + 230_000, SPAWNED));
    let v = log.view("game_log", true, 0);
    use EventKind as K;
    assert_eq!(
        kinds(&log),
        [
            K::Spawned,
            K::CorpseItems,
            K::CorpseItems,
            K::CorpseItems,
            K::Respawned
        ]
    );
    assert_eq!(v.events[1].detail, "Corpse nearby: 3 items (owner unknown)");
    assert_eq!(
        v.events[4].detail,
        "Respawned after a death (4 items left on the body)"
    );
    assert_eq!((v.signals.deaths, v.signals.respawns), (1, 1));
    // A respawn long after the last corpse: a respawn, not a counted death.
    corpse(&mut log, T0 + 300_000, 1);
    log.end_poll(T0);
    log.end_poll(T0);
    log.fold_line(&at(T0 + 500_000, SPAWNED));
    let v = log.view("game_log", true, 0);
    assert_eq!(v.events.last().unwrap().detail, "Respawned");
    assert_eq!((v.signals.deaths, v.signals.respawns), (1, 2));
    // A new shard: the next spawn is a login again; the counts stay for the file.
    log.fold_line(&at(
        T0 + 600_000,
        "[Notice] <Join PU> address[203.0.113.1] port[1] shard[pub_use1b_1_002]",
    ));
    log.fold_line(&at(T0 + 610_000, SPAWNED));
    let v = log.view("game_log", true, 0);
    assert_eq!(v.events.last().unwrap().kind, K::Spawned);
    assert_eq!(v.signals.deaths, 1);
}

#[test]
fn vehicle_destroyed_names_no_one_in_events() {
    let mut log = GameLog::new(true);
    log.fold_line(&at(T0, "[Notice] <Vehicle Destroyed> Vehicle 'ANVL_Arrow_000000000032' [000000000032] class 'ANVL_Arrow' (owner player [000000000042], driver [0]) destroyed: reason [Combat] level [0] -> [1] flags [0] killer 'TestPilot_D' [000000000043] authority [0] [Team_X][Vehicle]"));
    let v = log.view("game_log", true, 0);
    assert_eq!(
        v.events[0].detail,
        "ANVL Arrow nearby was disabled (Combat)"
    );
    assert_eq!(v.signals.vehicles_destroyed, 1);
    let ev = serde_json::to_string(&v.events).unwrap();
    assert!(!ev.contains("TestPilot"), "{ev}");
    let p = v.people.unwrap();
    assert_eq!(p.people[0].name, "TestPilot_D");
    assert_eq!(p.people[0].relation, "nearby");
}

#[test]
fn names_and_npcs() {
    for npc in [
        "",
        "none",
        "PU_Pilots-Human",
        "AI_Guard",
        "Citizen-12",
        "Pilot_123456",
    ] {
        assert!(is_npc(npc), "{npc}");
    }
    for real in ["TestPilot_A", "Test-Pilot", "AB_12345"] {
        assert!(!is_npc(real), "{real}");
    }
    assert!(clean_name("x'y").is_none());
    assert!(clean_name(&"a".repeat(65)).is_none());
    assert!(valid_handle("TestPilot_A") && !valid_handle("Test Pilot") && !valid_handle("a"));
}

#[test]
fn toast_rules() {
    let h = toast_hits("You have joined channel 'Drake Cutlass Black : TestPilot_Own'.");
    assert_eq!(
        (h[0].0, h[0].1, h[0].3),
        ("crew", "TestPilot_Own", Some("Drake Cutlass Black"))
    );
    let h = toast_hits("New Member Joined\nTestPilot_B has joined the party.");
    assert_eq!((h[0].0, h[0].1), ("party", "TestPilot_B"));
    let h = toast_hits("Member Left\nTestPilot_B has left the party.");
    assert_eq!(h[0].4, Some(true));
    assert!(
        toast_hits("You have joined\nparty").is_empty(),
        "the literal word party is no leader"
    );
    let h = toast_hits("New Party Leader\nTestPilot_L is now party leader.");
    assert!(h[0].5);
    let h = toast_hits("Quantum Travel Calibration Started By TestPilot_C");
    assert_eq!((h[0].0, h[0].1), ("party_likely", "TestPilot_C"));
    let h = toast_hits("Party\nTestPilot_E disconnected.");
    assert_eq!(h[0].1, "TestPilot_E");
    let h = toast_hits("TestPilot_F has joined the channel 'Cutlass : TestPilot_G'.");
    assert_eq!((h[0].1, h[1].1), ("TestPilot_F", "TestPilot_G"));
    let h = toast_hits("Incoming\nTestPilot_H has started to QT to your current location.");
    assert_eq!(h[0].1, "TestPilot_H");
    // Not under their titles: ignored.
    assert!(toast_hits("Something\nTestPilot_B has joined the party.").is_empty());
}

#[test]
fn people_from_multi_line_toasts_across_reads_and_self_is_never_listed() {
    let mut log = GameLog::new(true);
    let note = "[Notice] <SHUDEvent_OnNotification> Added notification \"";
    let text = [
        at(T0, "[Notice] <Legacy login response> [CIG-net] User Login Success - Handle[TestPilot_Self] - Time[1]"),
        at(T0, "[Notice] <AccountLoginCharacterStatus_Character> Character: createdAt 1 - updatedAt 2 - geid 1001 - accountId 77 - name TestPilot_Self - state STATE_CURRENT [Login]"),
        at(T0 + 1, &format!("{note}New Member Joined")),
        at(T0 + 1, &format!("TestPilot_B has joined the party.{TAIL_Q}")),
        at(T0 + 2, &format!("{note}Quantum Travel Calibration Started By TestPilot_Self{TAIL_Q}")),
        at(T0 + 3, &format!("{note}Quantum Travel Calibration Started By PU_Pilot{TAIL_Q}")),
        // A toast interrupted by a tagged line: dropped.
        at(T0 + 4, &format!("{note}New Member Joined")),
        at(T0 + 4, "[Notice] <Other> something"),
        at(T0 + 4, &format!("TestPilot_X has joined the party.{TAIL_Q}")),
        at(T0 + 5, &format!("{note}You have joined channel 'Drake Cutlass Black : TestPilot_Lead'.{TAIL_Q}")),
    ]
    .join("\r\n")
        + "\r\n";
    // Split inside a line and between the two lines of a toast.
    let bytes = text.as_bytes();
    let cut = text.find("TestPilot_B has").unwrap() + 4;
    log.feed_bytes(&bytes[..cut]);
    log.end_poll(T0);
    log.feed_bytes(&bytes[cut..]);
    let p = log.view("game_log", true, 0).people.unwrap();
    let names: Vec<&str> = p.people.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(
        names,
        ["TestPilot_Lead", "TestPilot_B"],
        "crew first, then party"
    );
    assert_eq!(p.self_name.as_deref(), Some("TestPilot_Self"));
    assert_eq!(p.people[0].ship.as_deref(), Some("Drake Cutlass Black"));
    assert_eq!((p.counts.crew, p.counts.party, p.counts.total), (1, 1, 2));
    let mut redact = log.handles();
    redact.sort();
    assert_eq!(redact, ["TestPilot_B", "TestPilot_Lead", "TestPilot_Self"]);
    // Clear: everyone goes, you stay known (a later line naming you is still not listed).
    log.clear_people();
    log.fold_line(&at(
        T0 + 6,
        &format!("{note}Quantum Travel Calibration Started By TestPilot_Self{TAIL_Q}"),
    ));
    let p = log.view("game_log", true, 0).people.unwrap();
    assert!(p.people.is_empty());
    assert_eq!(p.self_name.as_deref(), Some("TestPilot_Self"));
}

#[test]
fn killers_and_victims_and_the_shard_clears_nearby_people() {
    let mut log = GameLog::new(true);
    let vd = |owner: &str, killer: &str, kid: &str| {
        format!(
            "[Notice] <Vehicle Destroyed> Vehicle 'ANVL_Arrow_000000000032' [000000000032] class 'ANVL_Arrow' (owner player [{owner}], driver [0]) destroyed: reason [Combat] level [0] -> [2] flags [0] killer '{killer}' [{kid}] authority [0]"
        )
    };
    // The victim (owner 55) is not named yet; later 55 kills someone and is named.
    log.fold_line(&at(T0, &vd("55", "TestPilot_K", "66")));
    log.fold_line(&at(T0 + 1, &vd("0", "TestPilot_V", "55")));
    log.fold_line(&at(T0 + 2, &vd("55", "PU_Criminal_000000000043", "77")));
    let p = log.view("game_log", true, 0).people.unwrap();
    let mut names: Vec<&str> = p.people.iter().map(|x| x.name.as_str()).collect();
    names.sort();
    assert_eq!(names, ["TestPilot_K", "TestPilot_V"]);
    assert_eq!(p.counts.nearby, 2);
    log.fold_line(&at(
        T0 + 3,
        "[Notice] <Join PU> address[203.0.113.1] port[1] shard[pub_use1b_1_002]",
    ));
    assert!(
        log.view("game_log", true, 0)
            .people
            .unwrap()
            .people
            .is_empty()
    );
}

#[test]
fn people_off_keeps_no_handle() {
    let mut log = GameLog::new(false);
    for l in session_lines(T0, true) {
        log.fold_line(&l);
    }
    let v = log.view("mock", false, 0);
    assert!(v.people.is_none());
    assert!(log.handles().is_empty());
    let json = serde_json::to_string(&v).unwrap();
    assert!(!json.contains("TestPilot"), "{json}");
}

#[test]
fn the_mock_session_has_every_kind_of_signal() {
    let mut log = GameLog::new(true);
    for l in session_lines(T0, true) {
        log.fold_line(&l);
    }
    log.end_poll(T0);
    let v = log.view("mock", false, 0);
    assert_eq!(v.session.channel.as_deref(), Some("PTU"));
    assert_eq!(v.session.system.as_deref(), Some("Stanton"));
    assert_eq!(v.signals.deaths, 1);
    assert_eq!(
        v.signals.qt.last_arrival.as_deref(),
        Some("Mock_Station_Alpha")
    );
    assert_eq!(v.signals.location.as_deref(), Some("Mock Planet One"));
    assert_eq!(v.signals.own_ship.as_ref().unwrap().class, "AEGS_Gladius");
    let p = v.people.unwrap();
    assert_eq!(p.counts.total, 4, "{:?}", p.people);
    assert_eq!(p.self_name.as_deref(), Some("TestPilot_Self"));
    use EventKind as K;
    for k in [
        K::LogStarted,
        K::SessionJoin,
        K::Zone,
        K::NearbyShip,
        K::QtRoute,
        K::QtArrived,
        K::VehicleDestroyed,
        K::VehicleReleased,
        K::CorpseItems,
        K::Respawned,
    ] {
        assert!(v.events.iter().any(|e| e.kind == k), "{k:?}");
    }
}

#[test]
fn events_are_bounded() {
    let mut log = GameLog::new(false);
    for i in 0..(EVENTS_CAP as i64 + 50) {
        let text = if i % 2 == 0 {
            "Entered Monitored Space"
        } else {
            "Entering Armistice Zone"
        };
        log.fold_line(&at(T0 + i, &hud(text)));
    }
    let v = log.view("game_log", true, 0);
    assert_eq!(v.events.len(), EVENTS_CAP);
    assert_eq!(v.events[0].seq, 51);
    // Ships: at most ENTITY_CAP remembered.
    for i in 0..(signals::ENTITY_CAP + 10) {
        log.fold_line(&at(T0, &nav(&format!("MOCK_Ship_{:012}", 1000 + i))));
    }
    assert_eq!(
        log.view("game_log", true, 0).signals.nearby_count as usize,
        signals::ENTITY_CAP
    );
    // A huge "line" is dropped, the next one still parses.
    let mut big = vec![b'x'; LINE_MAX + 10];
    big.extend_from_slice(b"\n");
    log.feed_bytes(&big);
    log.feed_bytes(format!("{}\n", at(T0, &hud("Entered Monitored Space"))).as_bytes());
    assert_eq!(
        log.view("game_log", true, 0).events.last().unwrap().detail,
        "Entered Monitored Space"
    );
}

// ---- the tail ---------------------------------------------------------------------------

fn append(path: &std::path::Path, text: &str) {
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    f.write_all(text.as_bytes()).unwrap();
}

/// Replace a file (retrying while Windows still reports the old handle as busy).
fn replace(path: &std::path::Path, text: &str) {
    for i in 0..50 {
        match std::fs::write(path, text) {
            Ok(()) => return,
            Err(_) if i < 49 => std::thread::sleep(std::time::Duration::from_millis(20)),
            Err(e) => panic!("{e}"),
        }
    }
}

#[test]
fn tail_reads_only_new_bytes_and_handles_truncation_and_rotation() {
    let dir = tempfile::tempdir().unwrap();
    let ch = dir.path().join("PTU");
    std::fs::create_dir(&ch).unwrap();
    let mut tail = Tail::for_channel(&ch);
    let mut log = GameLog::new(false);
    // No Game.log yet.
    assert!(!tail.step(&mut log, T0));
    assert!(!tail.found);
    // Lower-case on disk: found in its own spelling.
    let path = ch.join("game.log");
    let first = format!(
        "Log started on A\n{}\n",
        at(T0, &hud("Entered Monitored Space"))
    );
    append(&path, &first);
    assert!(tail.step(&mut log, T0));
    assert!(tail.found);
    assert_eq!(log.view("game_log", true, 0).events.len(), 2);
    // A partial line waits for its newline.
    let line = at(T0 + 1, &hud("Entering Armistice Zone"));
    append(&path, &line[..20]);
    tail.step(&mut log, T0);
    assert_eq!(log.view("game_log", true, 0).events.len(), 2);
    append(&path, &format!("{}\n", &line[20..]));
    tail.step(&mut log, T0);
    assert_eq!(log.view("game_log", true, 0).events.len(), 3);
    // Nothing new: nothing read.
    assert!(!tail.step(&mut log, T0));
    // Truncated and rewritten shorter (a new launch): read again from the start.
    replace(
        &path,
        &format!(
            "Log started on B\n{}\n",
            at(T0, &hud("Entered Monitored Space"))
        ),
    );
    assert!(tail.step(&mut log, T0));
    assert_eq!(tail.rotations, 1);
    let v = log.view("game_log", true, tail.rotations);
    assert_eq!(v.events.len(), 2, "{:?}", v.events);
    assert_eq!(v.session.log_started.as_deref(), Some("B"));
    // Same size, different head (rewritten in place): also a new file.
    let same_len = std::fs::metadata(&path).unwrap().len() as usize;
    let mut other = format!(
        "Log started on C\n{}\n",
        at(T0, &hud("Entered Monitored Space"))
    );
    other.truncate(same_len);
    replace(&path, &other);
    tail.step(&mut log, T0);
    assert_eq!(tail.rotations, 2);
    // Gone (moved to logbackups/): the session ends; a new file is read from its start.
    std::fs::create_dir(ch.join("logbackups")).unwrap();
    let mut moved = false;
    for _ in 0..50 {
        if std::fs::rename(&path, ch.join("logbackups").join("old.log")).is_ok() {
            moved = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(moved, "the tail holds no handle between polls");
    assert!(tail.step(&mut log, T0));
    assert!(!tail.found);
    assert!(log.view("game_log", false, 0).events.is_empty());
    append(&ch.join("Game.log"), "Log started on D\n");
    tail.step(&mut log, T0);
    assert!(tail.found);
    assert_eq!(
        log.view("game_log", true, 0).session.log_started.as_deref(),
        Some("D")
    );
}

#[test]
fn tail_reads_a_big_log_in_bounded_steps() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Game.log");
    let line = format!("{}\n", at(T0, &nav("MOCK_Ship_000000000001")));
    let n = (tail::READ_MAX_PER_POLL as usize / line.len()) + 100;
    append(&path, &line.repeat(n));
    let mut tail = Tail::for_file(&path);
    let r = tail.read();
    assert!(r.more);
    assert_eq!(r.data.len() as u64, tail::READ_MAX_PER_POLL);
    let r = tail.read();
    assert!(!r.more);
    assert_eq!(
        r.data.len() + tail::READ_MAX_PER_POLL as usize,
        line.len() * n
    );
}

#[test]
fn a_directory_named_game_log_is_not_opened() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("Game.log")).unwrap();
    let mut tail = Tail::for_channel(dir.path());
    let mut log = GameLog::new(false);
    assert!(!tail.step(&mut log, T0));
    assert!(!tail.found);
}
