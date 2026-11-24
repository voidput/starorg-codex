//! How hard StarOrg Codex may use the machine (CPU, threads, wall time).
//!
//! The game is usually running while we work, so nothing we do runs at full priority:
//!
//! - [`Priority::Normal`]: work the user started (a click in the app, `apply` in a
//!   terminal). Below-normal priority (Linux nice 10; Windows `BELOW_NORMAL` for workers)
//!   and at most [`normal_threads`] threads, leaving two cores free.
//! - [`Priority::Background`]: work nobody is waiting for (the LUG launch hooks and the
//!   post-exit background job, dataset extraction an MCP client asks for in the app or in
//!   `starorg-codex mcp`). Lowest priority (Linux nice 19, which also puts its disk reads
//!   at the lowest best-effort I/O level; Windows `IDLE` for workers) and at most
//!   [`background_threads`] threads.
//!
//! `STARORG_THREADS=<n>` overrides the thread cap for both. Worker processes get the
//! priority, the thread cap and the caller's deadline through the environment
//! ([`prepare`], [`spawn`]) and apply them first thing ([`arm_from_env`]).
//!
//! Linux priority is per thread: [`lower_current_thread`] lowers the calling thread only, and
//! threads (and child processes) it starts later inherit it. A thread's priority is never
//! raised again (that needs privileges), so [`spawn`] starts the child from a short-lived
//! thread of its own instead of lowering a thread the caller keeps using. Windows: the
//! workspace forbids `unsafe`, so the own process keeps its class; workers get theirs through
//! the creation flags, and the thread cap applies everywhere.

use std::io;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// `background` or `normal`, for a worker process (and a `STARORG_PRIORITY=background`
/// override of a CLI run started by a script).
pub const ENV_PRIORITY: &str = "STARORG_PRIORITY";
/// Thread cap override: a whole number of threads, at least 1.
pub const ENV_THREADS: &str = "STARORG_THREADS";
/// The caller's deadline for a worker, as Unix milliseconds: the worker exits at it.
pub const ENV_DEADLINE: &str = "STARORG_DEADLINE_MS";
/// How much later than its caller's deadline a worker stops itself ([`prepare`]).
pub const CHILD_GRACE: Duration = Duration::from_secs(1);
/// Exit code of a worker that stopped itself at [`ENV_DEADLINE`] (like `timeout(1)`).
pub const EXIT_DEADLINE: i32 = 124;

/// How much of the machine a job may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Priority {
    /// Work nobody is waiting for: the launch hooks, automatic extraction.
    Background,
    /// Work the user started.
    #[default]
    Normal,
}

impl Priority {
    pub fn as_str(self) -> &'static str {
        match self {
            Priority::Background => "background",
            Priority::Normal => "normal",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "background" | "idle" | "low" => Some(Priority::Background),
            "normal" | "foreground" => Some(Priority::Normal),
            _ => None,
        }
    }

    /// From [`ENV_PRIORITY`]; [`Priority::Normal`] when unset or unknown.
    pub fn from_env() -> Self {
        std::env::var(ENV_PRIORITY)
            .ok()
            .and_then(|v| Self::parse(&v))
            .unwrap_or_default()
    }

    /// The Unix nice value of this priority.
    pub fn nice(self) -> i32 {
        match self {
            Priority::Background => 19,
            Priority::Normal => 10,
        }
    }

    /// The Windows priority class flag for a child process (`CreateProcess` flags).
    pub fn windows_class(self) -> u32 {
        match self {
            // IDLE_PRIORITY_CLASS
            Priority::Background => 0x0000_0040,
            // BELOW_NORMAL_PRIORITY_CLASS
            Priority::Normal => 0x0000_4000,
        }
    }

    /// The thread cap on a machine with `cores` logical cores, `over` being
    /// [`ENV_THREADS`] (ignored unless a whole number of at least 1).
    pub fn threads_for(self, cores: usize, over: Option<&str>) -> usize {
        if let Some(n) = over
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|n| *n >= 1)
        {
            return n;
        }
        match self {
            Priority::Background => background_threads(cores),
            Priority::Normal => normal_threads(cores),
        }
    }

    /// The thread cap on this machine.
    pub fn threads(self) -> usize {
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
        self.threads_for(cores, std::env::var(ENV_THREADS).ok().as_deref())
    }
}

/// Background work: a quarter of the cores, at most 4, at least 1.
pub fn background_threads(cores: usize) -> usize {
    (cores / 4).clamp(1, 4)
}

/// User-started work: all cores but two, at most 8, at least 1.
pub fn normal_threads(cores: usize) -> usize {
    cores.saturating_sub(2).clamp(1, 8)
}

/// The nice value of the calling thread (`None` off Linux).
pub fn current_nice() -> Option<i32> {
    #[cfg(target_os = "linux")]
    {
        rustix::process::getpriority_process(None).ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Lower the calling thread to `p` (Linux; a no-op elsewhere). Never raises it: a thread
/// already lower keeps its value. Threads and processes it starts later inherit it.
/// Returns whether the thread now runs at `p` or lower.
pub fn lower_current_thread(p: Priority) -> bool {
    #[cfg(target_os = "linux")]
    {
        match current_nice() {
            Some(n) if n >= p.nice() => true,
            _ => rustix::process::setpriority_process(None, p.nice()).is_ok(),
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = p;
        false
    }
}

static PROCESS: AtomicU8 = AtomicU8::new(PRIO_NORMAL);
const PRIO_NORMAL: u8 = 0;
const PRIO_BACKGROUND: u8 = 1;

/// The priority [`init_process`] set (else [`Priority::Normal`]): the default of the
/// workers this process starts (`LocalExtraction::new`, `ProcessExtractor::new`).
pub fn process_priority() -> Priority {
    match PROCESS.load(Ordering::SeqCst) {
        PRIO_BACKGROUND => Priority::Background,
        _ => Priority::Normal,
    }
}

/// Set up this process for `p`: lower the calling thread (call it from `main` before any
/// thread starts, so every later thread inherits it), cap rayon's global pool, and make
/// `p` the default of the workers it starts. A second call only lowers the thread and sets
/// the default (the pool is built once).
pub fn init_process(p: Priority) {
    PROCESS.store(
        match p {
            Priority::Background => PRIO_BACKGROUND,
            Priority::Normal => PRIO_NORMAL,
        },
        Ordering::SeqCst,
    );
    lower_current_thread(p);
    init_pool(p);
}

/// Build rayon's global pool for `p` (thread cap; on Linux its threads at `p`, started from
/// a short-lived thread lowered to it), leaving the calling thread as it is: for the app,
/// whose window must not be slowed. A no-op once the pool exists.
pub fn init_pool(p: Priority) {
    let n = p.threads();
    let _ = std::thread::spawn(move || {
        lower_current_thread(p);
        rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build_global()
            .is_ok()
    })
    .join();
}

/// The thread cap of this process: rayon's global pool (after [`init_process`]).
pub fn process_threads() -> usize {
    rayon::current_num_threads()
}

/// `deadline` as Unix milliseconds (for [`ENV_DEADLINE`]).
pub fn deadline_ms(deadline: Instant) -> u64 {
    let left = deadline.saturating_duration_since(Instant::now());
    (SystemTime::now() + left)
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// The time left until Unix milliseconds `ms` (zero once past).
pub fn left_until_ms(ms: u64) -> Duration {
    let at = UNIX_EPOCH + Duration::from_millis(ms);
    at.duration_since(SystemTime::now()).unwrap_or_default()
}

/// Put `p`, its thread cap and `deadline` into a worker's environment, and on Windows its
/// priority class into the creation flags (with `CREATE_NO_WINDOW` when `no_window`).
/// Start it with [`spawn`] so it also gets `p` on Linux.
pub fn prepare(cmd: &mut Command, p: Priority, deadline: Option<Instant>, no_window: bool) {
    cmd.env(ENV_PRIORITY, p.as_str());
    cmd.env(ENV_THREADS, p.threads().to_string());
    match deadline {
        // A little later than the caller's own: the caller stops the worker at its deadline;
        // the worker's own stop is for when the caller is gone.
        Some(d) => cmd.env(ENV_DEADLINE, deadline_ms(d + CHILD_GRACE).to_string()),
        None => cmd.env_remove(ENV_DEADLINE),
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let flags = p.windows_class() | if no_window { 0x0800_0000 } else { 0 };
        cmd.creation_flags(flags);
    }
    #[cfg(not(windows))]
    let _ = no_window;
}

/// Start `cmd` at priority `p`: on Linux from a short-lived thread lowered to `p`, which
/// the child inherits; the caller's thread keeps its own priority.
pub fn spawn(cmd: &mut Command, p: Priority) -> io::Result<Child> {
    std::thread::scope(|s| {
        s.spawn(|| {
            lower_current_thread(p);
            cmd.spawn()
        })
        .join()
        .unwrap_or_else(|_| Err(io::Error::other("spawning thread panicked")))
    })
}

/// For a worker: apply [`ENV_PRIORITY`] and [`ENV_THREADS`] to this process, and when the
/// caller set [`ENV_DEADLINE`], exit with [`EXIT_DEADLINE`] once it passes (the caller
/// stops waiting for it then; a worker writes its output only by renaming it into place,
/// so stopping it leaves nothing half-written). Call first thing in the worker's `main`.
pub fn arm_from_env() -> Priority {
    arm_from_env_with(|| {})
}

/// [`arm_from_env`] that runs `cleanup` (remove a temp file) before stopping at the
/// deadline.
pub fn arm_from_env_with(cleanup: impl FnOnce() + Send + 'static) -> Priority {
    let p = Priority::from_env();
    init_process(p);
    if let Some(ms) = std::env::var(ENV_DEADLINE)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
    {
        let left = left_until_ms(ms);
        watchdog(Instant::now() + left, Duration::ZERO, move || {
            cleanup();
            eprintln!("worker: out of time (the caller's deadline passed); stopping");
            std::process::exit(EXIT_DEADLINE);
        });
    }
    p
}

/// The critical sections running, whether a watchdog is ending the process, and whether
/// the process has disarmed its watchdogs ([`disarm`]).
#[derive(Default)]
struct Gate {
    running: usize,
    expiring: bool,
    disarmed: bool,
}

static GATE: Mutex<Gate> = Mutex::new(Gate {
    running: 0,
    expiring: false,
    disarmed: false,
});
/// End this process and every process in its group (the background job and its workers;
/// it runs in a group of its own). Off Linux only this process ends.
pub fn stop_process_group() -> ! {
    #[cfg(target_os = "linux")]
    {
        let _ = rustix::process::kill_current_process_group(rustix::process::Signal::KILL);
    }
    std::process::exit(EXIT_DEADLINE);
}

static GATE_CHANGED: Condvar = Condvar::new();

fn gate() -> MutexGuard<'static, Gate> {
    GATE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Held while a write that must not be cut short runs (backup, `global.ini`, `user.cfg`):
/// a [`watchdog`] waits for it to end.
#[must_use = "the section ends when the guard is dropped"]
pub struct Critical(());

impl Drop for Critical {
    fn drop(&mut self) {
        gate().running -= 1;
        GATE_CHANGED.notify_all();
    }
}

/// Start a [`Critical`] section. Once a watchdog has begun to end the process, this waits
/// (the process ends before the write starts), so a write is either whole or not begun.
pub fn critical() -> Critical {
    let mut g = gate();
    while g.expiring {
        g = GATE_CHANGED.wait(g).unwrap_or_else(PoisonError::into_inner);
    }
    g.running += 1;
    Critical(())
}

/// Whether a [`Critical`] section is running in this process.
pub fn in_critical() -> bool {
    gate().running > 0
}

/// The work is done (its report written): no [`watchdog`] of this process fires any more.
pub fn disarm() {
    gate().disarmed = true;
    GATE_CHANGED.notify_all();
}

/// After waiting for a write, how long a [`watchdog`] gives the next step to begin.
const HANDOFF: Duration = Duration::from_millis(250);

/// Run `on_expire` on a thread of its own at `deadline + grace`, once no [`Critical`]
/// section runs, unless the process [`disarm`]ed first. `on_expire` is expected to end the
/// process (`std::process::exit`); new critical sections wait meanwhile. The thread is
/// detached: when the process ends first, it ends with it.
pub fn watchdog(deadline: Instant, grace: Duration, on_expire: impl FnOnce() + Send + 'static) {
    let at = deadline + grace;
    std::thread::spawn(move || {
        loop {
            let now = Instant::now();
            if now >= at {
                break;
            }
            std::thread::sleep((at - now).min(Duration::from_millis(200)));
        }
        let mut g = gate();
        loop {
            let mut waited = false;
            while g.running > 0 && !g.disarmed {
                waited = true;
                g = GATE_CHANGED.wait(g).unwrap_or_else(PoisonError::into_inner);
            }
            if g.disarmed {
                return;
            }
            if !waited {
                break;
            }
            // A write just ended: the work is nearly done, and its next step (the report)
            // may follow at once. Give it a moment to start before ending the process.
            drop(g);
            std::thread::sleep(HANDOFF);
            g = gate();
            if g.running == 0 && !g.disarmed {
                break;
            }
        }
        g.expiring = true;
        drop(g);
        on_expire();
        // Only reached when `on_expire` did not end the process (tests).
        gate().expiring = false;
        GATE_CHANGED.notify_all();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_caps() {
        assert_eq!(background_threads(1), 1);
        assert_eq!(background_threads(4), 1);
        assert_eq!(background_threads(8), 2);
        assert_eq!(background_threads(32), 4);
        assert_eq!(normal_threads(1), 1);
        assert_eq!(normal_threads(2), 1);
        assert_eq!(normal_threads(6), 4);
        assert_eq!(normal_threads(32), 8);
        let b = Priority::Background;
        assert_eq!(b.threads_for(32, None), 4);
        assert_eq!(b.threads_for(32, Some("12")), 12);
        assert_eq!(b.threads_for(32, Some("0")), 4);
        assert_eq!(b.threads_for(32, Some("lots")), 4);
        assert_eq!(Priority::Normal.threads_for(32, Some(" 3 ")), 3);
    }

    #[test]
    fn names_and_values() {
        for p in [Priority::Background, Priority::Normal] {
            assert_eq!(Priority::parse(p.as_str()), Some(p));
        }
        assert_eq!(Priority::parse("Idle"), Some(Priority::Background));
        assert_eq!(Priority::parse("max"), None);
        assert_eq!(Priority::default(), Priority::Normal);
        assert!(Priority::Background.nice() > Priority::Normal.nice());
        assert_eq!(Priority::Background.windows_class(), 0x40);
        assert_eq!(Priority::Normal.windows_class(), 0x4000);
    }

    #[test]
    fn deadline_round_trip() {
        let d = Instant::now() + Duration::from_secs(30);
        let left = left_until_ms(deadline_ms(d));
        assert!(left > Duration::from_secs(28) && left <= Duration::from_secs(30));
        assert_eq!(left_until_ms(0), Duration::ZERO);
    }

    #[test]
    fn prepare_sets_the_worker_environment() {
        let mut c = Command::new("x");
        prepare(
            &mut c,
            Priority::Background,
            Some(Instant::now() + Duration::from_secs(5)),
            true,
        );
        let env: std::collections::HashMap<_, _> = c
            .get_envs()
            .map(|(k, v)| (k.to_owned(), v.map(|v| v.to_owned())))
            .collect();
        assert_eq!(
            env.get(std::ffi::OsStr::new(ENV_PRIORITY))
                .cloned()
                .flatten(),
            Some("background".into())
        );
        let threads = env
            .get(std::ffi::OsStr::new(ENV_THREADS))
            .cloned()
            .flatten();
        assert!(threads.is_some_and(|t| t.to_string_lossy().parse::<usize>().unwrap() >= 1));
        assert!(env.contains_key(std::ffi::OsStr::new(ENV_DEADLINE)));
        let mut c = Command::new("x");
        prepare(&mut c, Priority::Normal, None, false);
        assert_eq!(
            c.get_envs()
                .find(|(k, _)| *k == ENV_DEADLINE)
                .map(|(_, v)| v.is_none()),
            Some(true)
        );
    }

    /// Lowering is per thread and only ever goes down.
    #[cfg(target_os = "linux")]
    #[test]
    fn lowering_is_per_thread_and_never_raises() {
        let before = current_nice().unwrap();
        let (normal, background, again) = std::thread::spawn(|| {
            assert!(lower_current_thread(Priority::Normal));
            let normal = current_nice().unwrap();
            assert!(lower_current_thread(Priority::Background));
            let background = current_nice().unwrap();
            assert!(lower_current_thread(Priority::Normal));
            (normal, background, current_nice().unwrap())
        })
        .join()
        .unwrap();
        assert!(normal >= 10, "{normal}");
        assert_eq!(background, 19);
        assert_eq!(again, 19, "never raised");
        assert_eq!(
            current_nice().unwrap(),
            before,
            "the caller's thread is untouched"
        );
    }

    /// A child started with [`spawn`] runs at the priority; the caller's thread does not.
    #[cfg(target_os = "linux")]
    #[test]
    fn spawned_child_inherits_the_priority() {
        let before = current_nice().unwrap();
        let mut c = Command::new("/bin/sh");
        c.args(["-c", "cut -d' ' -f19 /proc/self/stat"])
            .stdout(std::process::Stdio::piped());
        prepare(&mut c, Priority::Background, None, true);
        let out = spawn(&mut c, Priority::Background)
            .unwrap()
            .wait_with_output()
            .unwrap();
        // /proc/self/stat of `cut` itself: field 19 is the nice value.
        let nice: i32 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();
        assert_eq!(nice, 19);
        assert_eq!(current_nice().unwrap(), before);
    }

    #[test]
    fn critical_sections_nest() {
        let a = critical();
        let b = critical();
        assert!(in_critical());
        drop(a);
        assert!(in_critical());
        drop(b);
        // Other tests may hold one; at least ours are gone.
    }

    #[test]
    fn watchdog_fires_after_the_deadline() {
        let (tx, rx) = std::sync::mpsc::channel();
        let start = Instant::now();
        watchdog(
            start + Duration::from_millis(50),
            Duration::ZERO,
            move || {
                let _ = tx.send(Instant::now());
            },
        );
        let fired = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(fired - start >= Duration::from_millis(50));
    }
}
