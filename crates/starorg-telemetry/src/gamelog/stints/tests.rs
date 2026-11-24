//! The join history: synthetic lines only (TEST-NET addresses, fake shard ids, no handles).

use super::*;
use crate::gamelog::GameLog;
use crate::gamelog::mock::iso;

const T0: i64 = 1_795_000_000_000;

fn at(ms: i64, body: &str) -> String {
    format!("<{}> {body}", iso(ms))
}

fn header(started: &str) -> Vec<String> {
    vec![
        format!("Log started on {started}"),
        "FileVersion: 9.9.0.1".into(),
        "Executable: C:\\Games\\StarCitizen\\PTU\\Bin64\\StarCitizen.exe".into(),
    ]
}

fn join(ms: i64, shard: &str) -> String {
    at(
        ms,
        &format!(
            "[Notice] <Join PU> address[203.0.113.7] port[64090] shard[{shard}] locationId[-3] [Team_GameServices][Login]"
        ),
    )
}

fn system(ms: i64, sys: &str) -> String {
    at(
        ms,
        &format!("Loading screen for {sys} : Mock closed after 9.1 seconds"),
    )
}

fn dc(ms: i64, reason: &str, rules: &str, up: f64) -> String {
    at(
        ms,
        &format!(
            "[Notice] <Channel Disconnected> cause=11 reason=\"{reason}\" frame=1 map=\"x\" gamerules=\"{rules}\" remoteAddr=203.0.113.7:64090 localAddr=192.0.2.5:5000 connection={{1,2}} session=00000000-0000-0000-0000-00000000aaaa node_id=n nickname=\"TestPilot_Self\" playerGEID=1 uptime_secs={up:.1}"
        ),
    )
}

fn fold(lines: &[String]) -> Vec<Stint> {
    let mut f = SessionFold::default();
    for l in lines {
        f.line(l);
    }
    f.finish()
}

#[test]
fn a_join_to_the_menu_is_one_stint_with_its_systems() {
    let mut l = header("Mon Nov 23 10:00:00 2026");
    l.push(join(T0, "ptu_use1b_13248297_020"));
    l.push(system(T0 + 20_000, "Stanton"));
    l.push(system(T0 + 90_000, "Pyro"));
    l.push(system(T0 + 100_000, "Pyro"));
    l.push(system(T0 + 600_000, "Frontend_Main"));
    let s = fold(&l);
    assert_eq!(s.len(), 1);
    let r = &s[0];
    assert_eq!(r.channel.as_deref(), Some("PTU"));
    assert_eq!(r.build.as_deref(), Some("9.9.0.1"));
    assert_eq!(r.shard, "ptu_use1b_13248297_020");
    assert_eq!(r.shard_short.as_deref(), Some("USE1B · 020"));
    assert_eq!(r.region_code.as_deref(), Some("use1b"));
    assert_eq!(r.region.as_deref(), Some("US East"));
    assert_eq!(r.joined_ms, T0);
    assert_eq!(r.left_ms, Some(T0 + 600_000));
    assert_eq!(r.left_reason, Some(LeftReason::Leave));
    assert_eq!(r.uptime_s, Some(600.0));
    assert_eq!(r.systems, ["Stanton", "Pyro"]);
    assert_eq!((r.disconnects, r.dropped), (0, false));
    // Nothing private in the row.
    let j = serde_json::to_string(r).unwrap();
    for bad in [
        "203.0.113",
        "64090",
        "192.0.2",
        "aaaa",
        "TestPilot",
        "Disconnected",
    ] {
        assert!(!j.contains(bad), "{bad} in {j}");
    }
}

#[test]
fn a_drop_ends_the_stint_at_the_disconnect_and_lines_within_2_s_are_one() {
    let mut l = header("Mon Nov 23 10:00:00 2026");
    l.push(join(T0, "pub_euw1b_1_007"));
    l.push(dc(
        T0 + 300_000,
        "Remote Disconnect - player inactive",
        "SC_Default",
        299.5,
    ));
    // The same disconnect on another channel, 1.5 s later.
    l.push(dc(
        T0 + 301_500,
        "Remote Disconnect - other",
        "SC_Default",
        301.0,
    ));
    l.push(system(T0 + 330_000, "Frontend_Main"));
    let s = fold(&l);
    assert_eq!(s.len(), 1);
    let r = &s[0];
    assert_eq!(r.left_reason, Some(LeftReason::Dropped));
    assert!(r.dropped);
    assert_eq!(r.disconnects, 1, "merged");
    assert_eq!(r.left_ms, Some(T0 + 300_000));
    assert_eq!(r.uptime_s, Some(299.5));
    assert_eq!(r.region.as_deref(), Some("EU West"));
}

#[test]
fn your_own_quit_is_a_disconnect_and_the_menu_channel_never_counts() {
    let mut l = header("Mon Nov 23 10:00:00 2026");
    // A menu (Frontend) disconnect before and during: not a game-server disconnect.
    l.push(dc(T0 - 5000, "Remote Disconnect - x", "SC_Frontend", 1.0));
    l.push(join(T0, "pub_usw2a_1_001"));
    l.push(dc(T0 + 1000, "Remote Disconnect - x", "SC_Frontend", 1.0));
    l.push(dc(
        T0 + 60_000,
        "Remote Disconnect - Player requested disconnect",
        "SC_Default",
        60.0,
    ));
    let s = fold(&l);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].left_reason, Some(LeftReason::Disconnect));
    assert!(!s[0].dropped);
    assert_eq!(s[0].disconnects, 1);
    assert_eq!(s[0].left_ms, Some(T0 + 60_000));
}

#[test]
fn a_repeated_shard_id_is_no_change_and_a_new_one_starts_a_stint() {
    let mut l = header("Mon Nov 23 10:00:00 2026");
    l.push(join(T0, "ptu_use1b_1_020"));
    l.push(at(
        T0 + 2000,
        "[Notice] <Update Shard Id> New Shard Id: ptu_use1b_1_020.",
    ));
    l.push(at(
        T0 + 500_000,
        "[Notice] <Update Shard Id> New Shard Id: ptu_use1b_1_031",
    ));
    l.push(at(T0 + 900_000, "<Some other line>"));
    let s = fold(&l);
    assert_eq!(s.len(), 2);
    assert_eq!(s[0].shard, "ptu_use1b_1_020");
    assert_eq!(s[0].left_reason, Some(LeftReason::ShardChange));
    assert_eq!(s[0].left_ms, Some(T0 + 500_000));
    assert_eq!(s[1].shard, "ptu_use1b_1_031");
    assert_eq!(s[1].joined_ms, T0 + 500_000);
    // The log ends on the shard: its last time stamp.
    assert_eq!(s[1].left_reason, Some(LeftReason::LogEnd));
    assert_eq!(s[1].left_ms, Some(T0 + 900_000));
}

#[test]
fn a_new_log_or_a_rotation_ends_the_open_stint() {
    // A new "Log started" in the same stream of lines.
    let mut l = header("Mon Nov 23 10:00:00 2026");
    l.push(join(T0, "ptu_use1b_1_020"));
    l.push(at(T0 + 10_000, "<x>"));
    l.extend(header("Mon Nov 23 11:00:00 2026"));
    let s = fold(&l);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].left_reason, Some(LeftReason::LogEnd));
    assert_eq!(s[0].left_ms, Some(T0 + 10_000));

    // The live reader: a rotation (the tail's reset) ends it too, and it is drained once.
    let mut g = GameLog::new(false);
    for x in header("Mon Nov 23 10:00:00 2026") {
        g.fold_line(&x);
    }
    g.fold_line(&join(T0, "ptu_use1b_1_020"));
    let v = g.view("game_log", true, 0);
    assert_eq!(v.stint.as_ref().unwrap().shard, "ptu_use1b_1_020");
    assert!(v.stint.unwrap().left_ms.is_none());
    assert!(g.drain_stints().is_empty());
    g.fold_line(&system(T0 + 4000, "Stanton"));
    g.reset();
    let d = g.drain_stints();
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].left_reason, Some(LeftReason::LogEnd));
    assert_eq!(d[0].systems, ["Stanton"]);
    assert!(g.drain_stints().is_empty());
    assert!(g.view("game_log", true, 0).stint.is_none());
}

#[test]
fn a_rejoin_without_leaving_closes_the_first() {
    let mut l = header("Mon Nov 23 10:00:00 2026");
    l.push(join(T0, "pub_use1b_1_001"));
    l.push(join(T0 + 60_000, "pub_use1b_1_002"));
    l.push(system(T0 + 120_000, "Frontend_Main"));
    let s = fold(&l);
    assert_eq!(s.len(), 2);
    assert_eq!(s[0].left_reason, Some(LeftReason::Leave));
    assert_eq!(s[0].left_ms, Some(T0 + 60_000));
    assert_eq!(s[1].left_ms, Some(T0 + 120_000));
}

fn row(joined: i64, shard: &str, started: &str) -> Stint {
    Stint {
        channel: Some("LIVE".into()),
        build: None,
        shard: shard.into(),
        shard_short: None,
        region_code: None,
        region: None,
        log_started: Some(started.into()),
        joined_ms: joined,
        left_ms: Some(joined + 1000),
        left_reason: Some(LeftReason::Leave),
        uptime_s: Some(1.0),
        systems: Vec::new(),
        disconnects: 0,
        dropped: false,
    }
}

#[test]
fn the_store_dedupes_caps_survives_a_restart_and_clears() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path().join("sessions");
    let mut s = Store::new(Some(d.clone()));
    assert!(s.rows().is_empty(), "no file yet");
    assert_eq!(
        s.add(vec![row(T0, "a", "L1"), row(T0 + 5, "b", "L1")], None),
        2
    );
    // The same log read again (the live file, then its backup) adds nothing.
    assert_eq!(s.add(vec![row(T0, "a", "L1")], None), 0);
    // Same join time and shard in another log is another row.
    assert_eq!(s.add(vec![row(T0, "a", "L2")], None), 1);
    let rev = s.revision;
    let mut again = Store::new(Some(d.clone()));
    let rows = again.rows();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].shard, "b", "newest first");
    assert_eq!(again.add(vec![row(T0 + 5, "b", "L1")], None), 0);
    assert!(rev > 0);
    // Capped: the oldest go first.
    let many: Vec<Stint> = (0..MAX_ROWS as i64 + 10)
        .map(|i| row(T0 + 1000 + i, "c", "L3"))
        .collect();
    again.add(many, None);
    let rows = again.rows();
    assert_eq!(rows.len(), MAX_ROWS);
    assert_eq!(rows.last().unwrap().joined_ms, T0 + 1010);
    assert!(std::fs::metadata(d.join(FILE_NAME)).unwrap().len() as usize <= MAX_FILE_BYTES);
    assert!(!d.join("history.json.tmp").exists());
    again.clear();
    assert!(again.rows().is_empty());
    assert!(Store::new(Some(d.clone())).rows().is_empty());
    // A broken file reads as empty (and says so).
    std::fs::write(d.join(FILE_NAME), b"{not json").unwrap();
    let mut broken = Store::new(Some(d));
    assert!(broken.rows().is_empty());
    assert!(broken.error.is_some());
}

#[test]
fn backfill_reads_the_newest_backups_once_in_bounded_chunks() {
    let dir = tempfile::tempdir().unwrap();
    let ch = dir.path().join("LIVE");
    let backups = ch.join("logbackups");
    std::fs::create_dir_all(&backups).unwrap();
    // Not a log, and a directory named like one: neither is read.
    std::fs::write(backups.join("notes.txt"), "x").unwrap();
    std::fs::create_dir(backups.join("odd.log")).unwrap();
    for (i, shard) in ["pub_use1b_1_001", "pub_euc1a_1_002"].iter().enumerate() {
        let t = T0 + i as i64 * 3_600_000;
        let mut l = header(&format!("Mon Nov 23 1{i}:00:00 2026"));
        l.push(join(t, shard));
        l.push(system(t + 50_000, "Nyx"));
        // No newline at the end: the last line still counts.
        let text = l.join("\r\n");
        std::fs::write(backups.join(format!("Game Build(1) 2{i}.log")), text).unwrap();
    }
    assert_eq!(backups_dir(&ch).as_deref(), Some(backups.as_path()));
    let mut store = Store::new(Some(dir.path().join("sessions")));
    let mut b = Backfill::plan(backups_dir(&ch).as_deref(), &mut store);
    assert_eq!(b.pending(), 2);
    let mut steps = 0;
    while !b.is_done() {
        if let Some((rows, mark)) = b.step() {
            store.add(rows, Some(mark));
        }
        steps += 1;
        assert!(steps < 10);
    }
    let rows = store.rows();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|r| r.left_reason == Some(LeftReason::LogEnd))
    );
    assert_eq!(rows[1].systems, ["Nyx"]);
    // Read once: a second plan has nothing left; the files themselves are not held open.
    assert!(Backfill::plan(Some(&backups), &mut store).is_done());
    for e in std::fs::read_dir(&backups).unwrap().flatten() {
        if e.path().is_file() {
            std::fs::remove_file(e.path()).unwrap();
        }
    }
    assert!(Backfill::plan(None, &mut store).is_done());
}

#[test]
fn a_chunk_boundary_inside_a_line_is_fine() {
    let mut l = header("Mon Nov 23 10:00:00 2026");
    l.push(join(T0, "pub_use1b_1_001"));
    l.push(system(T0 + 9000, "Frontend_Main"));
    let text = l.join("\n") + "\n";
    let mut f = SessionFold::default();
    for c in text.as_bytes().chunks(7) {
        f.bytes(c);
    }
    let s = f.finish();
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].left_reason, Some(LeftReason::Leave));
}
