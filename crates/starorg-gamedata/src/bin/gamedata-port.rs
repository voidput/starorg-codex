//! `gamedata-port`: dev harness for the sc-gamedata port. Not part of the shipped CLI.
//!
//! ```text
//! gamedata-port extract --channel LIVE --out .tmp/gamedata/LIVE [--domain manufacturers]
//! gamedata-port compare .tmp/gamedata/LIVE ~/git/sc-gamedata/data [--domain manufacturers]
//! gamedata-port list
//! ```
//!
//! `extract` only reads the archive. Write `--out` somewhere under `.tmp/`, never into the
//! game install or the sc-gamedata checkout.

use clap::{Parser, Subcommand};
use starorg_gamedata::compare::{Outcome, compare_text};
use starorg_gamedata::domain::{Domain, FileSource, NoFiles};
use starorg_gamedata::pipeline::{self, Game};
use starorg_gamedata::source::{self, ArchiveFiles, DataCoreSource};
use starorg_gamedata::{domains, output, strings};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "gamedata-port",
    about = "Extract and compare sc-gamedata datasets"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write game-<domain>.json for the registered domains (and the string table).
    Extract {
        /// LIVE, PTU, EPTU, HOTFIX: picks the install when --p4k is not given.
        #[arg(long, default_value = "LIVE")]
        channel: String,
        /// Data.p4k to read (else SC_DATA_P4K, else the usual install locations).
        #[arg(long)]
        p4k: Option<PathBuf>,
        #[arg(long)]
        out: PathBuf,
        /// Only these domains (`manufacturers` or `game-manufacturers`; `strings-english`
        /// for the string table). Repeatable. Default: every registered domain.
        #[arg(long)]
        domain: Vec<String>,
        /// Redact the string table as published dumps do (off by default, like data/).
        #[arg(long)]
        redact_strings: bool,
    },
    /// Semantic diff of our datasets against golden ones. Exits 1 on any difference.
    Compare {
        ours: PathBuf,
        golden: PathBuf,
        /// Only these datasets. Default: every game-*.json in <ours>.
        #[arg(long)]
        domain: Vec<String>,
        /// Example paths printed per dataset.
        #[arg(long, default_value_t = 20)]
        examples: usize,
    },
    /// List the registered domains.
    List,
}

fn file_name(domain: &str) -> String {
    let base = domain.strip_suffix(".json").unwrap_or(domain);
    if base.starts_with("game-") {
        format!("{base}.json")
    } else {
        format!("game-{base}.json")
    }
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::List => {
            for d in domains::all() {
                println!("{:28} {}", d.name(), d.description());
            }
            println!(
                "{:28} The string table (not a record domain).",
                strings::NAME
            );
            ExitCode::SUCCESS
        }
        Command::Extract {
            channel,
            p4k,
            out,
            domain,
            redact_strings,
        } => match extract(&channel, p4k.as_deref(), &out, &domain, redact_strings) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::from(2)
            }
        },
        Command::Compare {
            ours,
            golden,
            domain,
            examples,
        } => compare(&ours, &golden, &domain, examples),
    }
}

fn extract(
    channel: &str,
    p4k: Option<&Path>,
    out: &Path,
    names: &[String],
    redact_strings: bool,
) -> Result<(), String> {
    let want_strings = names.is_empty()
        || names
            .iter()
            .any(|n| file_name(n) == format!("{}.json", strings::NAME));
    let selected: Vec<&'static dyn Domain> = if names.is_empty() {
        domains::all()
    } else {
        names
            .iter()
            .filter(|n| file_name(n) != format!("{}.json", strings::NAME))
            .map(|n| {
                domains::find(n.strip_suffix(".json").unwrap_or(n)).ok_or_else(|| {
                    let known: Vec<_> = domains::all().iter().map(|d| d.name()).collect();
                    format!("unknown domain {n:?}; registered: {}", known.join(", "))
                })
            })
            .collect::<Result<_, _>>()?
    };

    let p4k = source::find_data_p4k(p4k, channel)?;
    eprintln!("reading {}", p4k.display());
    let keep_archive = selected.iter().any(|d| !d.files().is_empty());
    let files = source::open(&p4k, keep_archive)?;
    eprintln!(
        "  build {}, {} localized strings",
        files.build.as_deref().unwrap_or("unknown"),
        files.localization.len()
    );
    let records = DataCoreSource::parse(&files.dcb)?;
    eprintln!(
        "  {} main records",
        starorg_gamedata::records::RecordSource::metas(&records).len()
    );
    let archive_files = files.archive.map(ArchiveFiles::new);
    let file_source: &dyn FileSource = match &archive_files {
        Some(a) => a,
        None => &NoFiles,
    };
    let game = Game {
        records: &records,
        localization: &files.localization,
        files: file_source,
        build: files.build.clone(),
    };

    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let write = |name: &str, text: &str| -> Result<(), String> {
        let path = out.join(format!("{name}.json"));
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        eprintln!("  wrote {}", path.display());
        Ok(())
    };
    for d in &selected {
        let dataset = pipeline::run(*d, &game, &output::iso_now()).map_err(|e| e.to_string())?;
        for w in &dataset.warnings {
            eprintln!("  WARNING  {w}");
        }
        write(dataset.name, &dataset.to_json())?;
    }
    if want_strings {
        let v = strings::dataset(
            &files.localization,
            &output::iso_now(),
            files.build.as_deref(),
            redact_strings,
        );
        write(strings::NAME, &output::to_js_json(&v))?;
    }
    Ok(())
}

fn compare(ours: &Path, golden: &Path, names: &[String], examples: usize) -> ExitCode {
    let files: Vec<String> = if names.is_empty() {
        let mut v: Vec<String> = std::fs::read_dir(ours)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.starts_with("game-") && n.ends_with(".json"))
            .collect();
        v.sort();
        v
    } else {
        names.iter().map(|n| file_name(n)).collect()
    };
    if files.is_empty() {
        eprintln!("no game-*.json in {}", ours.display());
        return ExitCode::from(2);
    }
    let mut failed = 0;
    for f in &files {
        let name = f.strip_suffix(".json").unwrap_or(f);
        let read = |dir: &Path| std::fs::read_to_string(dir.join(f));
        let outcome = match (read(ours), read(golden)) {
            (Ok(a), Ok(b)) => compare_text(&a, &b),
            (Err(e), _) => Outcome::Unreadable(format!("ours: {e}")),
            (_, Err(_)) => Outcome::MissingGolden,
        };
        println!("{}", outcome.report(name, examples));
        if !outcome.is_ok() {
            failed += 1;
        }
    }
    println!(
        "{} of {} datasets identical",
        files.len() - failed,
        files.len()
    );
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
