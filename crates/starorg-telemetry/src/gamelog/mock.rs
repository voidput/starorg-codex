//! Synthetic Game.log lines for the mock backend, the UI preview and tests: the line shapes of
//! the real log (sc-telemetry `mock_signals` / `mock_people`) with fake entity ids
//! (`_0000000000NN`), a TEST-NET address and obviously fake handles (`TestPilot_*`). No line
//! from a real log.

/// Unix ms -> `2026-11-21T12:34:56.789Z`.
pub fn iso(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (days, sod) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        sod / 3600,
        sod / 60 % 60,
        sod % 60,
        ms.rem_euclid(1000)
    )
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

const NAV: &str = "[Notice] <Local Route Guard - Server Rerouted> [ItemNavigation][CL][1] | NOT AUTH | {e}[{e}]|CSCItemNavigation::PostInitialize::<lambda_1>::operator ()|FinalStop=0 [Team_CoreGameplayFeatures][Navigation]";
const HUD: &str = "[Notice] <SHUDEvent_OnNotification> Added notification \"{t}: \" [1] to queue. New queue size: 1, MissionId: [00000000-0000-0000-0000-000000000000], ObjectiveId: [] [Team_CoreGameplayFeatures][UI]";
const TAIL: &str = ": \" [1] to queue. New queue size: 1, MissionId: [00000000-0000-0000-0000-000000000000], ObjectiveId: [] [Team_CoreGameplayFeatures][UI]";

fn nav(e: &str) -> String {
    NAV.replace("{e}", e)
}

fn hud(t: &str) -> String {
    HUD.replace("{t}", t)
}

/// A whole synthetic session, the last line `now_ms`: (seconds before now, body).
fn script(people: bool) -> Vec<(i64, String)> {
    let mut v: Vec<(i64, String)> = vec![
        (900, "Log started on Mon Nov 23 10:00:00 2026".into()),
        (900, "FileVersion: 0.0.0.0".into()),
        (
            900,
            "Executable: C:\\Program Files\\Roberts Space Industries\\StarCitizen\\PTU\\Bin64\\StarCitizen.exe"
                .into(),
        ),
        (
            880,
            "[Notice] <Legacy login response> [CIG-net] User Login Success - Handle[TestPilot_Self] - Time[1] [Team_GameServices][Login]"
                .into(),
        ),
        (
            860,
            "[Notice] <Join PU> address[203.0.113.10] port[64000] shard[mock_use1b_0_001] locationId[0] [Team_GameServices][Login]"
                .into(),
        ),
        (840, "Loading screen for Stanton : Mock closed after 12.0 seconds".into()),
        (830, "[Notice] [CSessionManager::OnClientSpawned] Spawned! [Team_GameServices]".into()),
        (800, hud("Entered Monitored Space")),
        (790, hud("Entering Armistice Zone - Combat Prohibited")),
        (700, nav("DRAK_Cutlass_Black_000000000011")),
        (690, nav("AEGS_Avenger_Titan_000000000012")),
        (680, nav("DRAK_Cutlass_Black_000000000013")),
        (600, hud("Leaving Armistice Zone - Caution Advised")),
        (
            420,
            "[Notice] <Player Selected Quantum Target - Local> [ItemNavigation][CL][1] | NOT AUTH | AEGS_Gladius_000000000001[000000000001]|CSCItemNavigation::OnPlayerSelectedQuantumTarget|Player has selected point Mock_Station_Alpha as their destination, routing locally [Team_CoreGameplayFeatures][Navigation]"
                .into(),
        ),
        (
            419,
            "[Notice] <Calculate Route> [ItemNavigation][CL][1] | NOT AUTH | AEGS_Gladius_000000000001[000000000001]|CSCItemNavigation::CalculateRoute|Successfully calculated route to Mock_Station_Alpha fuel estimate 412.5 [Team_CoreGameplayFeatures][Navigation]"
                .into(),
        ),
        (
            419,
            "[Notice] <Calculate Route> [ItemNavigation][CL][1] | NOT AUTH | AEGS_Gladius_000000000001[000000000001]|CSCItemNavigation::CalculateRoute|Projected Start Location is Mock Planet One for route to destination Mock_Station_Alpha [Team_CoreGameplayFeatures][Navigation]"
                .into(),
        ),
        (
            360,
            "[Notice] <Quantum Drive Arrived - Arrived at Final Destination> [ItemNavigation][CL][1] | NOT AUTH | AEGS_Gladius_000000000001[000000000001]|CSCItemNavigation::OnQuantumDriveArrived|Quantum Drive has arrived at final destination [Team_CoreGameplayFeatures][Navigation]"
                .into(),
        ),
        (
            300,
            "[Notice] <Quantum Drive Arrived - Arrived at Final Destination> [ItemNavigation][CL][1] | NOT AUTH | MISC_Freelancer_000000000014[000000000014]|CSCItemNavigation::OnQuantumDriveArrived|Quantum Drive has arrived at final destination [Team_CoreGameplayFeatures][Navigation]"
                .into(),
        ),
        (
            240,
            "[Notice] <Vehicle Destroyed> Vehicle 'AEGS_Gladius_000000000031' [000000000031] class 'AEGS_Gladius' (owner player [000000000041], driver [0]) destroyed: reason [Combat] level [0] -> [2] flags [0] killer 'TestPilot_D' [000000000042] authority [0] [Team_X][Vehicle]"
                .into(),
        ),
        (
            200,
            "[Notice] <Vehicle Control Flow> CVehicleMovementBase::ClearDriver: Local client node [1] releasing control token for 'AEGS_Gladius_000000000001' [1] [Team_CoreGameplayFeatures][Vehicle]"
                .into(),
        ),
    ];
    for i in 0..3 {
        v.push((
            150,
            format!(
                "[Notice] <Adding non kept item [CSCActorCorpseUtils::PopulateItemPortForItemRecoveryEntitlement]> Item 'mock_item_{i}_00000000005{i} - Class(mock_item_{i})', Recorded data is: Port Name 'Body_ItemPort' [Team_X][Inventory]"
            ),
        ));
    }
    v.push((
        110,
        "[Notice] [CSessionManager::OnClientSpawned] Spawned! [Team_GameServices]".into(),
    ));
    v.push((30, nav("RSI_Constellation_Andromeda_000000000015")));
    if people {
        let note = "[Notice] <SHUDEvent_OnNotification> Added notification \"";
        v.extend([
            (500, format!("{note}You have joined")),
            (500, format!("TestPilot_Lead{TAIL}")),
            (480, format!("{note}New Member Joined")),
            (480, format!("TestPilot_B has joined the party.{TAIL}")),
            (
                460,
                format!(
                    "{note}You have joined channel 'Drake Cutlass Black : TestPilot_Lead'.{TAIL}"
                ),
            ),
            (
                430,
                format!("{note}Quantum Travel Calibration Started By TestPilot_C{TAIL}"),
            ),
        ]);
    }
    v.sort_by_key(|(ago, _)| -ago);
    v
}

/// The synthetic session as Game.log lines, the last one at `now_ms`.
pub fn session_lines(now_ms: i64, people: bool) -> Vec<String> {
    script(people)
        .into_iter()
        .map(|(ago, body)| {
            // Header lines carry no time stamp in the real log.
            let header = ["Log started", "FileVersion", "Executable"]
                .iter()
                .any(|h| body.starts_with(h));
            if header {
                body
            } else {
                format!("<{}> {body}", iso(now_ms - ago * 1000))
            }
        })
        .collect()
}

/// A line now and then while the mock runs (`i` counts up): ships and zone notices.
pub fn ambient_line(i: u64, now_ms: i64) -> String {
    const SHIPS: [&str; 4] = [
        "ANVL_Arrow",
        "MISC_Prospector",
        "CRUS_Starlifter_C2",
        "ORIG_300i",
    ];
    let body = match i % 6 {
        2 => hud("Entering Armistice Zone - Combat Prohibited"),
        5 => hud("Leaving Armistice Zone - Caution Advised"),
        _ => nav(&format!(
            "{}_{:012}",
            SHIPS[(i % SHIPS.len() as u64) as usize],
            100 + i
        )),
    };
    format!("<{}> {body}", iso(now_ms))
}
