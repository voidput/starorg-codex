//! WebKitGTK rendering workarounds for NVIDIA on Linux (spec 10.4).
//!
//! [`apply`] runs first thing in `main()`, before `tauri::Builder` and before any thread
//! starts, because it calls `std::env::set_var`. It only adds variables the user has not set,
//! and remembers what it added so that [`clean_command`] can give child processes (file
//! manager, browser, the CLI worker) the original environment.
//!
//! Written from the variables in Tauri's Linux graphics notes; no third-party quirk crate.

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

/// Every variable this module may set. If the user set any of them, we leave all of them alone
/// (except under `--safe-gpu`, which the user asked for explicitly).
pub const MANAGED_VARS: &[&str] = &[
    "__NV_DISABLE_EXPLICIT_SYNC",
    "WEBKIT_DISABLE_DMABUF_RENDERER",
    "WEBKIT_DISABLE_COMPOSITING_MODE",
];

/// Command-line flag that forces the slowest, safest rendering path.
pub const SAFE_GPU_FLAG: &str = "--safe-gpu";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Session {
    Wayland,
    X11,
    Unknown,
}

/// What the decision needs to know about the machine; read once from the process and `/sys`.
#[derive(Debug, Clone, Default)]
pub struct Probe {
    pub nvidia: bool,
    pub gdk_backend: Option<String>,
    pub xdg_session_type: Option<String>,
    pub wayland_display: bool,
    pub display: bool,
    pub current_desktop: Option<String>,
    pub hyprland_signature: bool,
    /// Any of [`MANAGED_VARS`] already set by the user.
    pub user_set_any: bool,
    pub safe_gpu: bool,
}

impl Probe {
    pub fn session(&self) -> Session {
        let parse = |v: &str| {
            let v = v.to_ascii_lowercase();
            // GDK_BACKEND may be a list such as "wayland,x11"; the first entry wins.
            match v.split(',').next().map(str::trim) {
                Some("wayland") => Some(Session::Wayland),
                Some("x11") => Some(Session::X11),
                _ => None,
            }
        };
        if let Some(s) = self.gdk_backend.as_deref().and_then(parse) {
            return s;
        }
        if let Some(s) = self.xdg_session_type.as_deref().and_then(parse) {
            return s;
        }
        if self.wayland_display {
            Session::Wayland
        } else if self.display {
            Session::X11
        } else {
            Session::Unknown
        }
    }

    fn desktop_has(&self, name: &str) -> bool {
        self.current_desktop
            .as_deref()
            .is_some_and(|d| d.split(':').any(|p| p.eq_ignore_ascii_case(name)))
    }

    fn hyprland(&self) -> bool {
        self.hyprland_signature || self.desktop_has("Hyprland")
    }

    /// KDE Plasma's KWin uses explicit sync on Wayland.
    fn explicit_sync_wayland(&self) -> bool {
        self.desktop_has("KDE")
    }
}

/// The variables to set for this machine, in order. Pure; see the tests.
pub fn decide(p: &Probe) -> Vec<(&'static str, &'static str)> {
    if p.safe_gpu {
        return vec![
            ("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
            ("WEBKIT_DISABLE_COMPOSITING_MODE", "1"),
        ];
    }
    if p.user_set_any || !p.nvidia {
        return Vec::new();
    }
    match p.session() {
        Session::Wayland if p.hyprland() => vec![("WEBKIT_DISABLE_DMABUF_RENDERER", "1")],
        Session::Wayland if p.explicit_sync_wayland() => {
            vec![("__NV_DISABLE_EXPLICIT_SYNC", "1")]
        }
        Session::X11 => vec![("WEBKIT_DISABLE_DMABUF_RENDERER", "1")],
        Session::Wayland | Session::Unknown => Vec::new(),
    }
}

/// True when a DRM device under `drm_root` (normally `/sys/class/drm`) is an NVIDIA GPU
/// (PCI vendor 0x10de) bound to the proprietary `nvidia` driver. Nouveau does not need this.
pub fn nvidia_in(drm_root: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(drm_root) else {
        return false;
    };
    entries.flatten().any(|e| {
        let dev = e.path().join("device");
        let vendor = std::fs::read_to_string(dev.join("vendor")).unwrap_or_default();
        if !vendor.trim().eq_ignore_ascii_case("0x10de") {
            return false;
        }
        std::fs::read_link(dev.join("driver"))
            .ok()
            .and_then(|l| l.file_name().map(|n| n == "nvidia"))
            .unwrap_or(false)
    })
}

fn probe_process(args: &[OsString]) -> Probe {
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    Probe {
        nvidia: cfg!(target_os = "linux") && nvidia_in(Path::new("/sys/class/drm")),
        gdk_backend: var("GDK_BACKEND"),
        xdg_session_type: var("XDG_SESSION_TYPE"),
        wayland_display: var("WAYLAND_DISPLAY").is_some(),
        display: var("DISPLAY").is_some(),
        current_desktop: var("XDG_CURRENT_DESKTOP"),
        hyprland_signature: var("HYPRLAND_INSTANCE_SIGNATURE").is_some(),
        user_set_any: MANAGED_VARS.iter().any(|k| std::env::var_os(k).is_some()),
        safe_gpu: args.iter().any(|a| a == SAFE_GPU_FLAG),
    }
}

/// A variable we changed and what it was before (`None` = unset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changed {
    pub name: &'static str,
    pub original: Option<OsString>,
}

static CHANGED: OnceLock<Vec<Changed>> = OnceLock::new();

/// Set the workaround variables. Call once, first thing in `main()`, while the process is
/// still single-threaded. A no-op off Linux.
#[allow(unsafe_code)]
pub fn apply() {
    if !cfg!(target_os = "linux") {
        let _ = CHANGED.set(Vec::new());
        return;
    }
    let args: Vec<OsString> = std::env::args_os().collect();
    let plan = decide(&probe_process(&args));
    let mut changed = Vec::with_capacity(plan.len());
    for (name, value) in plan {
        let original = std::env::var_os(name);
        if original.as_deref() == Some(std::ffi::OsStr::new(value)) {
            continue;
        }
        // SAFETY: called from `main()` before `tauri::Builder` or anything else spawns a
        // thread, so no other thread can be reading the environment concurrently.
        unsafe { std::env::set_var(name, value) };
        changed.push(Changed { name, original });
    }
    let _ = CHANGED.set(changed);
}

/// The variables [`apply`] changed in this process.
pub fn changed() -> &'static [Changed] {
    CHANGED.get().map(Vec::as_slice).unwrap_or(&[])
}

/// Undo our changes on a child command so it sees the user's original environment.
pub fn restore_env(cmd: &mut Command, changes: &[Changed]) {
    for c in changes {
        match &c.original {
            Some(v) => cmd.env(c.name, v),
            None => cmd.env_remove(c.name),
        };
    }
}

/// A `Command` for any program the app starts. Always use this instead of `Command::new`.
pub fn clean_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new(program);
    restore_env(&mut cmd, changed());
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nv() -> Probe {
        Probe {
            nvidia: true,
            ..Probe::default()
        }
    }

    #[test]
    fn kde_wayland_on_nvidia_disables_explicit_sync() {
        let p = Probe {
            xdg_session_type: Some("wayland".into()),
            current_desktop: Some("KDE".into()),
            ..nv()
        };
        assert_eq!(decide(&p), vec![("__NV_DISABLE_EXPLICIT_SYNC", "1")]);
    }

    #[test]
    fn x11_and_hyprland_on_nvidia_disable_dmabuf() {
        let x11 = Probe {
            xdg_session_type: Some("x11".into()),
            ..nv()
        };
        assert_eq!(decide(&x11), vec![("WEBKIT_DISABLE_DMABUF_RENDERER", "1")]);
        let hypr = Probe {
            xdg_session_type: Some("wayland".into()),
            hyprland_signature: true,
            ..nv()
        };
        assert_eq!(decide(&hypr), vec![("WEBKIT_DISABLE_DMABUF_RENDERER", "1")]);
    }

    #[test]
    fn nothing_without_nvidia_or_when_user_set_one() {
        let amd = Probe {
            xdg_session_type: Some("x11".into()),
            ..Probe::default()
        };
        assert!(decide(&amd).is_empty());
        let user = Probe {
            xdg_session_type: Some("x11".into()),
            user_set_any: true,
            ..nv()
        };
        assert!(decide(&user).is_empty());
    }

    #[test]
    fn session_order_gdk_backend_then_xdg_then_displays() {
        let p = Probe {
            gdk_backend: Some("x11".into()),
            xdg_session_type: Some("wayland".into()),
            wayland_display: true,
            ..Probe::default()
        };
        assert_eq!(p.session(), Session::X11);
        let p = Probe {
            gdk_backend: Some("wayland,x11".into()),
            ..Probe::default()
        };
        assert_eq!(p.session(), Session::Wayland);
        let p = Probe {
            xdg_session_type: Some("tty".into()),
            display: true,
            ..Probe::default()
        };
        assert_eq!(p.session(), Session::X11);
        assert_eq!(Probe::default().session(), Session::Unknown);
    }

    #[test]
    fn safe_gpu_forces_the_slow_path_even_if_user_set_vars() {
        let p = Probe {
            safe_gpu: true,
            user_set_any: true,
            ..Probe::default()
        };
        assert_eq!(
            decide(&p),
            vec![
                ("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
                ("WEBKIT_DISABLE_COMPOSITING_MODE", "1")
            ]
        );
    }

    #[test]
    fn nvidia_detection_needs_vendor_and_bound_nvidia_driver() {
        let t = tempfile::tempdir().unwrap();
        let mk = |card: &str, vendor: &str, driver: &str| {
            let dev = t.path().join(card).join("device");
            std::fs::create_dir_all(&dev).unwrap();
            std::fs::write(dev.join("vendor"), format!("{vendor}\n")).unwrap();
            let drv = t.path().join("drivers").join(driver);
            std::fs::create_dir_all(&drv).unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(&drv, dev.join("driver")).unwrap();
        };
        mk("card0", "0x1002", "amdgpu");
        assert!(!nvidia_in(t.path()));
        mk("card1", "0x10de", "nouveau");
        assert!(!nvidia_in(t.path()));
        mk("card2", "0x10de", "nvidia");
        assert!(cfg!(not(unix)) || nvidia_in(t.path()));
        assert!(!nvidia_in(&t.path().join("missing")));
    }

    #[test]
    fn child_commands_get_the_original_environment() {
        let changes = vec![
            Changed {
                name: "__NV_DISABLE_EXPLICIT_SYNC",
                original: None,
            },
            Changed {
                name: "WEBKIT_DISABLE_DMABUF_RENDERER",
                original: Some("0".into()),
            },
        ];
        let mut cmd = Command::new("true");
        restore_env(&mut cmd, &changes);
        let envs: Vec<_> = cmd.get_envs().collect();
        assert!(envs.contains(&(std::ffi::OsStr::new("__NV_DISABLE_EXPLICIT_SYNC"), None)));
        assert!(envs.contains(&(
            std::ffi::OsStr::new("WEBKIT_DISABLE_DMABUF_RENDERER"),
            Some(std::ffi::OsStr::new("0"))
        )));
    }
}
