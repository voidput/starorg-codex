//! Command-line interface: `run(args) -> ExitCode` and the `starorg-strings` binary
//! (spec 8).
//!
//! The CLI never opens a window, needs no display, and links no GTK, WebView or HTTP client.

use clap::{Parser, Subcommand};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

/// Exit codes (spec 8).
pub mod exit {
    pub const OK: u8 = 0;
    pub const ERROR: u8 = 1;
    pub const USAGE: u8 = 2;
    pub const SKIPPED: u8 = 3;
    pub const NOT_FOUND: u8 = 4;
    pub const WARNINGS: u8 = 5;
    pub const NEEDS_DECISION: u8 = 6;
}

#[derive(Debug, Parser)]
#[command(
    name = "starorg-strings",
    version,
    about = "Enhanced Star Citizen strings"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Internal: extract one Data.p4k in a separate process (spec 3.3).
    #[command(hide = true)]
    ExtractWorker {
        #[arg(long)]
        p4k: PathBuf,
        #[arg(long)]
        out: PathBuf,
        /// Test hook: panic inside the guarded extraction.
        #[arg(long, hide = true)]
        test_panic: bool,
        /// Test hook: never finish.
        #[arg(long, hide = true, conflicts_with = "test_panic")]
        test_hang: bool,
    },
}

/// Parse `args` (including the program name) and run. Never panics on bad input.
pub fn run<I, T>(args: I) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(c) => c,
        Err(e) => {
            let _ = e.print();
            return ExitCode::from(if e.use_stderr() {
                exit::USAGE
            } else {
                exit::OK
            });
        }
    };
    match cli.command {
        Command::ExtractWorker {
            p4k,
            out,
            test_panic,
            test_hang,
        } => {
            use starorg_data::worker::{TestHook, run};
            let hook = if test_panic {
                TestHook::Panic
            } else if test_hang {
                TestHook::Hang
            } else {
                TestHook::None
            };
            ExitCode::from(run(&p4k, &out, hook) as u8)
        }
    }
}
