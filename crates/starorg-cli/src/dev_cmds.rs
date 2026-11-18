//! `dev`: the Developer mode explorer from the command line (addendum 7.9). Runs in this
//! process under `catch_unwind` (about 1 s and 0.9 GB peak on LIVE), takes
//! `locks/extract.lock` while Data.p4k is read, writes no cache. Read only: `export-type`
//! writes only the `--out` file, never inside the install or a Wine prefix.

use crate::commands::{JSON_SCHEMA, canonical_lenient, fail, refuse_protected, run_discovery};
use crate::{DevCmd, GlobalOpts, exit};
use serde_json::json;
use starorg_data::devworker::{self, DevInput, LoadedDcb, TypeExport};
use starorg_data::extract::{ExtractError, guarded};
use starorg_data::starbreaker::DevDatabase;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

/// The DataCore of the channel (or the test hook's file), read as the app's worker does.
fn load(
    g: &GlobalOpts,
    channel: &str,
    test_dcb: Option<PathBuf>,
) -> Result<(LoadedDcb, Option<PathBuf>), ExitCode> {
    if let Some(dcb) = test_dcb {
        return devworker::load(&DevInput::Dcb { dcb, ini: None })
            .map(|l| (l, None))
            .map_err(|e| fail(g, exit::ERROR, "error", &e.to_string()));
    }
    let d = run_discovery(g);
    let Some(install) = d.default_install() else {
        return Err(fail(
            g,
            exit::NOT_FOUND,
            "not-found",
            "no Star Citizen install found (try --game-dir or --prefix)",
        ));
    };
    let Some(ch) = install.channel(channel) else {
        return Err(fail(
            g,
            exit::NOT_FOUND,
            "not-found",
            &format!("channel {channel} not found in {}", install.root.display()),
        ));
    };
    let Some(dirs) = starorg_core::appdirs::AppDirs::from_env() else {
        return Err(fail(g, exit::ERROR, "error", "cannot find the app folders"));
    };
    // The archive index is the peak (about 0.9 GB): never at the same time as an extraction.
    let _x = match starorg_core::lock::try_lock(&dirs.extract_lock()) {
        Ok(l) => l,
        Err(starorg_core::lock::LockError::Held(_)) => {
            return Err(fail(
                g,
                exit::SKIPPED,
                "busy",
                "another StarOrg Codex run is reading game data; try again",
            ));
        }
        Err(e) => return Err(fail(g, exit::ERROR, "error", &e.to_string())),
    };
    let root = install.root.clone();
    devworker::load(&DevInput::P4k(ch.p4k.clone()))
        .map(|l| (l, Some(root)))
        .map_err(|e| fail(g, exit::ERROR, "error", &e.to_string()))
}

fn parse<'a>(g: &GlobalOpts, bytes: &'a [u8]) -> Result<DevDatabase<'a>, ExitCode> {
    guarded("DataCore parse", || {
        DevDatabase::parse(bytes).map_err(ExtractError::DataCore)
    })
    .map_err(|e| fail(g, exit::ERROR, "error", &e.to_string()))
}

pub fn run(g: &GlobalOpts, cmd: DevCmd) -> ExitCode {
    match cmd {
        DevCmd::Types { channel, test_dcb } => types(g, &channel, TestDcb::from(test_dcb).0),
        DevCmd::Record {
            what,
            channel,
            compact,
            test_dcb,
        } => record(g, &channel, &what, compact, TestDcb::from(test_dcb).0),
        DevCmd::ExportType {
            name,
            out,
            channel,
            ndjson,
            main_only,
            test_dcb,
        } => export_type(
            g,
            &channel,
            &name,
            &out,
            ndjson,
            main_only,
            TestDcb::from(test_dcb).0,
        ),
        cmd @ (DevCmd::Extract { .. }
        | DevCmd::ExportModel { .. }
        | DevCmd::ModelPaints { .. }
        | DevCmd::PrintGlb { .. }
        | DevCmd::ExportSounds { .. }) => crate::asset_cmds::run(g, cmd),
    }
}

/// `--test-dcb` exists only in builds with the `test-hooks` feature.
pub struct TestDcb(pub Option<PathBuf>);

fn types(g: &GlobalOpts, channel: &str, test_dcb: Option<PathBuf>) -> ExitCode {
    let (l, _) = match load(g, channel, test_dcb) {
        Ok(x) => x,
        Err(c) => return c,
    };
    let db = match parse(g, &l.dcb) {
        Ok(d) => d,
        Err(c) => return c,
    };
    let types = db.index().types();
    if g.json {
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "records": db.index().len(), "types": types })
        );
    } else {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{:<56} {:>8} {:>8}", "type", "records", "main");
        for t in &types {
            let _ = writeln!(out, "{:<56} {:>8} {:>8}", t.name, t.records, t.main);
        }
    }
    ExitCode::from(exit::OK)
}

fn record(
    g: &GlobalOpts,
    channel: &str,
    what: &str,
    compact: bool,
    test_dcb: Option<PathBuf>,
) -> ExitCode {
    let (l, _) = match load(g, channel, test_dcb) {
        Ok(x) => x,
        Err(c) => return c,
    };
    let db = match parse(g, &l.dcb) {
        Ok(d) => d,
        Err(c) => return c,
    };
    let Some(i) = db.index().find(what) else {
        return fail(
            g,
            exit::NOT_FOUND,
            "not-found",
            &format!("no record {what:?} (a GUID, record path or record name)"),
        );
    };
    let mut buf = Vec::new();
    let r = guarded("record export", || {
        db.write_export(i, &mut buf, !compact)
            .map_err(ExtractError::DataCore)
    });
    if let Err(e) = r {
        return fail(g, exit::ERROR, "error", &e.to_string());
    }
    if compact {
        buf.retain(|&b| b != b'\n' && b != b'\r');
    }
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(&buf);
    let _ = writeln!(out);
    ExitCode::from(exit::OK)
}

fn export_type(
    g: &GlobalOpts,
    channel: &str,
    name: &str,
    out: &std::path::Path,
    ndjson: bool,
    main_only: bool,
    test_dcb: Option<PathBuf>,
) -> ExitCode {
    let testing = test_dcb.is_some();
    let (l, root) = match load(g, channel, test_dcb) {
        Ok(x) => x,
        Err(c) => return c,
    };
    let out_abs = canonical_lenient(out);
    if !testing {
        let d = run_discovery(g);
        if let Some(install) = d.default_install()
            && Some(&install.root) == root.as_ref()
            && let Some(code) = refuse_protected(g, &out_abs, install)
        {
            return code;
        }
    }
    if starorg_core::discovery::enclosing_wine_prefix(&out_abs).is_some() {
        return fail(
            g,
            exit::USAGE,
            "usage",
            &format!("--out {} is inside a Wine prefix", out_abs.display()),
        );
    }
    let db = match parse(g, &l.dcb) {
        Ok(d) => d,
        Err(c) => return c,
    };
    let records = db.index().of_type(name, main_only);
    if records.is_empty() {
        return fail(
            g,
            exit::NOT_FOUND,
            "not-found",
            &format!("no records of type {name}"),
        );
    }
    let r = guarded("type export", || {
        let mut x = TypeExport::start(records, &out_abs, ndjson).map_err(ExtractError::Output)?;
        while !x.step(&db, 256).map_err(ExtractError::Output)? {}
        x.finish().map_err(ExtractError::Output)
    });
    match r {
        Ok(done) => {
            if g.json {
                println!(
                    "{}",
                    json!({ "schema": JSON_SCHEMA, "type": name, "records": done.records, "bytes": done.bytes, "out": done.path })
                );
            } else if !g.quiet {
                eprintln!(
                    "starorg-codex: {} {name} records ({} bytes) written to {}",
                    done.records, done.bytes, done.path
                );
            }
            ExitCode::from(exit::OK)
        }
        Err(e) => fail(g, exit::ERROR, "error", &e.to_string()),
    }
}
