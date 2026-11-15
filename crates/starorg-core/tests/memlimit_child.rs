//! The memory limit really stops a child (addendum 7.17): this test binary starts itself as
//! the child (`child_allocates`, ignored unless asked for), capped by `memlimit::apply`, and
//! the child allocates and touches memory until it is stopped.
//!
//! The watchdog stops it cleanly at 90% of the limit (exit `EXIT_MEMORY`), well before the
//! hard cap (`RLIMIT_DATA` on Linux, 512 MB above), so no allocation fails and the kernel
//! kills nothing. The opt-in cgroup cap runs only with `STARORG_TEST_REAL_CGROUP=1` (CI): on
//! a desktop a cgroup scope that ends badly shows up as a system notification. Caps are
//! small (256 MB) so the tests never weigh on the machine.

use starorg_core::memlimit::{self, EXIT_MEMORY, GIB, MARKER};
use std::path::Path;
use std::process::{Command, Output, Stdio};

const ALLOC_MB: &str = "STARORG_TEST_ALLOC_MB";
const MB: u64 = 1024 * 1024;

/// The child: arm from the environment, print its data limit, then allocate `ALLOC_MB` in
/// touched 8 MB chunks.
#[test]
#[ignore = "child process of the tests below"]
fn child_allocates() {
    let Some(mb) = std::env::var(ALLOC_MB)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    else {
        return;
    };
    let armed = memlimit::arm_from_env();
    eprintln!("armed: {armed:?}");
    if let Ok(l) = std::fs::read_to_string("/proc/self/limits") {
        for line in l.lines().filter(|l| l.starts_with("Max data size")) {
            eprintln!("limits: {line}");
        }
    }
    let mut held: Vec<Vec<u8>> = Vec::new();
    for i in 0..mb / 8 {
        let mut v = vec![0u8; (8 * MB) as usize];
        // Touch every page so the memory is resident, not just reserved.
        for p in (0..v.len()).step_by(4096) {
            v[p] = (i as u8).wrapping_add(1);
        }
        held.push(v);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    println!("allocated {} MB", held.len() as u64 * 8);
}

fn child(limit: u64, alloc_mb: u64, scope: Option<&Path>) -> Output {
    let exe = std::env::current_exe().unwrap();
    let mut c = Command::new(exe);
    c.args([
        "child_allocates",
        "--exact",
        "--ignored",
        "--nocapture",
        "--test-threads=1",
    ])
    .env(ALLOC_MB, alloc_mb.to_string());
    let mut c = memlimit::apply_via(c, Some(limit), scope);
    c.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    c.output().unwrap()
}

/// Stopped by its own watchdog: exit EXIT_MEMORY with the message, nothing killed.
fn assert_stopped(o: &Output, limit: u64, hard: &str) {
    let out = String::from_utf8_lossy(&o.stdout);
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(EXIT_MEMORY), "{out}\n{err}");
    assert!(memlimit::exit_was_memory(&o.status, Some(limit)));
    assert!(!out.contains("allocated"), "{out}");
    let human = memlimit::human(limit);
    assert!(err.contains(&format!("{MARKER} {human} ({hard})")), "{err}");
    assert!(
        err.contains(&format!("{MARKER} Stopped: needed more than {human}.")),
        "{err}"
    );
    eprintln!(
        "{hard}: {:?}; {}",
        o.status,
        err.lines()
            .filter(|l| l.starts_with(MARKER) || l.starts_with("limits:"))
            .collect::<Vec<_>>()
            .join(" | ")
    );
}

#[test]
fn a_child_over_its_limit_is_stopped_cleanly_and_reported() {
    let limit = 256 * MB;
    let o = child(limit, 1024, None);
    let hard = if cfg!(target_os = "linux") {
        "RLIMIT_DATA"
    } else {
        "watchdog only"
    };
    assert_stopped(&o, limit, hard);
    if cfg!(target_os = "linux") {
        // The hard cap sits above the limit (512 MB headroom), as RLIMIT_DATA.
        let cap = limit + memlimit::headroom(limit);
        let err = String::from_utf8_lossy(&o.stderr);
        let line = err.lines().find(|l| l.starts_with("limits:")).unwrap();
        assert!(line.contains(&cap.to_string()), "{line}");
    }
}

/// The opt-in cgroup cap (`STARORG_MEMORY_CGROUP=1`): CI only.
#[test]
fn the_cgroup_cap_when_asked_for() {
    if std::env::var("STARORG_TEST_REAL_CGROUP").as_deref() != Ok("1") || !cfg!(target_os = "linux")
    {
        eprintln!("skipped: set STARORG_TEST_REAL_CGROUP=1 to run a real systemd scope");
        return;
    }
    let limit = 256 * MB;
    let o = child(limit, 1024, Some(Path::new("systemd-run")));
    assert_stopped(&o, limit, "cgroup memory.max");
}

#[test]
fn a_child_under_its_limit_finishes() {
    let o = child(GIB / 2, 64, None);
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(
        o.status.success(),
        "{out}\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(out.contains("allocated 64 MB"), "{out}");
}
