//! RSI Launcher log parsing (spec 10.1).
//!
//! The format is undocumented. Each line is roughly a JSON object followed by a comma:
//!
//! ```text
//! { "t":"2026-11-12 12:09:07.764", "[main][info] ": "[Pipeline] Installing Star Citizen LIVE 4.10.3-live.13165946 at C:\\Program Files\\Roberts Space Industries\\StarCitizen (type: update, forceDP: false)"  },
//! { "t":"2026-11-12 12:21:30.287", "[main][info] ": "[Launcher::launch] Launching Star Citizen LIVE from (C:\\Program Files\\Roberts Space Industries\\StarCitizen\\LIVE)"  },
//! ```
//!
//! Parsing is defensive: a line that does not parse is skipped, and a log that yields nothing
//! falls through to path scanning.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// At most this much of the end of each log is read.
pub const MAX_TAIL: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HitKind {
    /// `[Launcher::launch] Launching Star Citizen <CH> from (<path>)`: the channel played.
    Launch,
    /// `[Pipeline] Installing Star Citizen <CH> <ver> at <library root>`.
    Install,
}

/// One useful line. `library_root` is a Windows path, already unescaped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogHit {
    pub kind: HitKind,
    pub channel: String,
    pub library_root: String,
}

const LAUNCH: &str = "[Launcher::launch] Launching Star Citizen ";
const INSTALL: &str = "[Pipeline] Installing Star Citizen ";

/// Parse log text. Hits are returned newest first.
pub fn parse(text: &str) -> Vec<LogHit> {
    let mut hits: Vec<LogHit> = text.lines().filter_map(parse_line).collect();
    hits.reverse();
    hits
}

fn parse_line(line: &str) -> Option<LogHit> {
    if !line.contains(LAUNCH) && !line.contains(INSTALL) {
        return None;
    }
    let trimmed = line.trim().trim_end_matches(',').trim_end();
    let obj: serde_json::Map<String, serde_json::Value> = serde_json::from_str(trimmed).ok()?;
    obj.values()
        .filter_map(serde_json::Value::as_str)
        .find_map(parse_message)
}

/// Parse one unescaped log message.
pub fn parse_message(msg: &str) -> Option<LogHit> {
    if let Some(i) = msg.find(LAUNCH) {
        let rest = &msg[i + LAUNCH.len()..];
        let (channel, rest) = rest.split_once(' ')?;
        let rest = rest.strip_prefix("from (")?;
        let path = rest.rsplit_once(')')?.0.trim_end_matches(['\\', '/']);
        let (root, last) = path.rsplit_once(['\\', '/'])?;
        if !last.eq_ignore_ascii_case(channel) || root.is_empty() {
            return None;
        }
        return Some(LogHit {
            kind: HitKind::Launch,
            channel: channel.to_string(),
            library_root: root.to_string(),
        });
    }
    if let Some(i) = msg.find(INSTALL) {
        let rest = &msg[i + INSTALL.len()..];
        let (channel, rest) = rest.split_once(' ')?;
        let (_version, rest) = rest.split_once(" at ")?;
        let root = match rest.rfind(" (type:") {
            Some(p) => &rest[..p],
            None => rest,
        }
        .trim()
        .trim_end_matches(['\\', '/']);
        if root.is_empty() {
            return None;
        }
        return Some(LogHit {
            kind: HitKind::Install,
            channel: channel.to_string(),
            library_root: root.to_string(),
        });
    }
    None
}

/// Read at most the last [`MAX_TAIL`] bytes of a log and parse them. Missing or unreadable
/// files give no hits.
pub fn read_tail(path: &Path) -> Vec<LogHit> {
    read_tail_text(path).map(|t| parse(&t)).unwrap_or_default()
}

/// The last [`MAX_TAIL`] bytes of a log as text, starting at a line boundary.
pub fn read_tail_text(path: &Path) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.seek(SeekFrom::End(0)).ok()?;
    let start = len.saturating_sub(MAX_TAIL);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::new();
    f.take(MAX_TAIL).read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf).into_owned();
    // Drop a partial first line when we started mid-file.
    Some(if start > 0 {
        text.split_once('\n')
            .map(|(_, r)| r.to_string())
            .unwrap_or_default()
    } else {
        text
    })
}

/// Markers that an install pipeline finished (or the game was started afterwards).
const PIPELINE_DONE: &[&str] = &[
    "cig-data-patcher-worker ended",
    "[ShaderCompilationPhase] Compile completed",
    "[Launcher::launch]",
];

/// Does the log text show an install pipeline for `channel` that has not finished? The last
/// `[Pipeline] Installing Star Citizen <CH>` line counts as in progress until a later line
/// shows the patcher worker ending, shader compilation completing, or a game launch.
pub fn pipeline_in_progress(text: &str, channel: &str) -> bool {
    let mut open = false;
    for line in text.lines() {
        if let Some(i) = line.find(INSTALL) {
            let rest = &line[i + INSTALL.len()..];
            if rest
                .split(' ')
                .next()
                .is_some_and(|c| c.eq_ignore_ascii_case(channel))
            {
                open = true;
            }
        } else if open && PIPELINE_DONE.iter().any(|m| line.contains(m)) {
            open = false;
        }
    }
    open
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{ "t":"2026-11-12 12:09:07.764", "[main][info] ": "[Pipeline] Installing Star Citizen LIVE 4.10.3-live.13165946 at C:\\Program Files\\Roberts Space Industries\\StarCitizen (type: update, forceDP: false)"  },
{ "t":"2026-11-12 12:10:00.000", "[main][info] ": "something else"  },
garbage line with [Launcher::launch] Launching Star Citizen but not json
{ "t":"2026-11-12 12:21:30.287", "[main][info] ": "[Launcher::launch] Launching Star Citizen PTU from (D:\\Games\\StarCitizen\\PTU)"  },
"#;

    #[test]
    fn pipeline_progress() {
        let start = r#"{ "t":"x", "[main][info] ": "[Pipeline] Installing Star Citizen PTU 5.0.0-ptu.1 at C:\\X (type: install, forceDP: false)"  },"#;
        let end = r#"{ "t":"x", "[main][info] ": "[Worker] cig-data-patcher-worker ended (code 0, alive 1930.3s, 0 live)"  },"#;
        assert!(pipeline_in_progress(start, "PTU"));
        assert!(!pipeline_in_progress(start, "LIVE"));
        assert!(!pipeline_in_progress(&format!("{start}\n{end}"), "PTU"));
        assert!(pipeline_in_progress(&format!("{end}\n{start}"), "ptu"));
    }

    #[test]
    fn parses_install_and_launch_lines_newest_first() {
        let hits = parse(SAMPLE);
        assert_eq!(
            hits,
            vec![
                LogHit {
                    kind: HitKind::Launch,
                    channel: "PTU".into(),
                    library_root: r"D:\Games\StarCitizen".into(),
                },
                LogHit {
                    kind: HitKind::Install,
                    channel: "LIVE".into(),
                    library_root: r"C:\Program Files\Roberts Space Industries\StarCitizen".into(),
                },
            ]
        );
    }

    #[test]
    fn rejects_malformed_messages() {
        assert_eq!(
            parse_message("[Launcher::launch] Launching Star Citizen LIVE"),
            None
        );
        assert_eq!(
            parse_message(r"[Launcher::launch] Launching Star Citizen LIVE from (C:\X\PTU)"),
            None,
            "channel must be the last path component"
        );
        assert_eq!(
            parse_message("[Pipeline] Installing Star Citizen LIVE 1.0 at "),
            None
        );
        assert_eq!(parse_message("unrelated"), None);
    }

    #[test]
    fn install_line_without_type_suffix() {
        let hit =
            parse_message(r"[Pipeline] Installing Star Citizen HOTFIX 4.10 at E:\SC\").unwrap();
        assert_eq!(hit.library_root, r"E:\SC");
    }

    #[test]
    fn tail_read_of_missing_file_is_empty() {
        assert!(read_tail(Path::new("/nonexistent/log.log")).is_empty());
    }

    #[test]
    fn tail_read_drops_partial_first_line() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("log.log");
        let mut big = "x".repeat(MAX_TAIL as usize);
        big.push_str(LAUNCH); // would be a partial line at the cut
        big.push('\n');
        big.push_str(SAMPLE);
        std::fs::write(&p, big).unwrap();
        assert_eq!(read_tail(&p).len(), 2);
    }
}
