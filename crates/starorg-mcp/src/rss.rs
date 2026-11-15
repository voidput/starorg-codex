//! Resident memory of this process: current and peak, in bytes.
//!
//! Linux reads `/proc/self/status` (`VmRSS`, `VmHWM`). Other systems report `None`: the
//! workspace forbids `unsafe`, and the Windows call (`GetProcessMemoryInfo`) needs it.

use serde::Serialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rss {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_bytes: Option<u64>,
}

/// `kB` lines of a `/proc/<pid>/status` text.
pub fn parse_status(text: &str) -> Rss {
    let field = |name: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(name))
            .and_then(|v| v.trim().strip_suffix("kB"))
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(|kb| kb * 1024)
    };
    Rss {
        current_bytes: field("VmRSS:"),
        peak_bytes: field("VmHWM:"),
    }
}

pub fn now() -> Rss {
    if cfg!(target_os = "linux") {
        std::fs::read_to_string("/proc/self/status")
            .map(|t| parse_status(&t))
            .unwrap_or_default()
    } else {
        Rss::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_proc_status() {
        let r = parse_status("Name:\tx\nVmHWM:\t  2048 kB\nVmRSS:\t  1024 kB\n");
        assert_eq!(r.current_bytes, Some(1024 * 1024));
        assert_eq!(r.peak_bytes, Some(2048 * 1024));
        assert_eq!(parse_status("nothing"), Rss::default());
        if cfg!(target_os = "linux") {
            assert!(now().peak_bytes.is_some());
        }
    }
}
