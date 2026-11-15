//! Command-line interface: `run(args) -> ExitCode` and the `starorg-strings` binary
//! (spec 8).
//!
//! The CLI never opens a window, needs no display, and links no GTK, WebView or HTTP client.

mod apply_cmds;
mod commands;

use clap::{Args, Parser, Subcommand};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

/// Exit codes (spec 8).
pub use starorg_core::apply::exit;

#[derive(Debug, Parser)]
#[command(
    name = "starorg-strings",
    version,
    about = "Enhanced Star Citizen strings"
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalOpts,
    #[command(subcommand)]
    pub command: Command,
}

/// Global options (spec 8).
#[derive(Debug, Clone, Default, Args)]
pub struct GlobalOpts {
    /// Install root (the folder that contains LIVE). Skips discovery.
    #[arg(long, global = true, value_name = "PATH")]
    pub game_dir: Option<PathBuf>,
    /// Linux: Wine prefix to look in. Defaults to $WINEPREFIX when set.
    #[arg(long, global = true, value_name = "PATH")]
    pub prefix: Option<PathBuf>,
    /// One JSON object on stdout; logs go to stderr.
    #[arg(long, global = true)]
    pub json: bool,
    /// Errors only.
    #[arg(long, global = true)]
    pub quiet: bool,
    /// Linux: desktop notification with the result (notify-send). Ignored on Windows.
    #[arg(long, global = true)]
    pub notify: bool,
    /// Force a fresh extraction.
    #[arg(long, global = true)]
    pub no_cache: bool,
    /// Test hook: use this file as the stock global.ini instead of extracting Data.p4k.
    #[arg(long, global = true, hide = true, value_name = "FILE")]
    pub test_stock_ini: Option<PathBuf>,
    /// Test hook: sleep this many milliseconds between writing global.ini and recording it.
    #[arg(long, global = true, hide = true, value_name = "MS")]
    pub test_pause_after_write_ms: Option<u64>,
}

/// `(--channel <NAME> | --all)`.
#[derive(Debug, Clone, Args)]
pub struct ChannelSel {
    #[arg(long, value_name = "NAME", conflicts_with = "all")]
    pub channel: Option<String>,
    /// Every set-up channel of the install.
    #[arg(long)]
    pub all: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List installs and channels with branch, game version and change number.
    Channels,
    /// Write the output for a channel to a file; touch nothing else.
    Build {
        #[arg(long, value_name = "NAME")]
        channel: String,
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
        /// Give up after this many seconds (exit 3).
        #[arg(long, value_name = "SECONDS")]
        timeout: Option<u64>,
    },
    /// Set up a channel: consent to write into it, and pick a starting profile.
    Setup {
        #[arg(long, value_name = "NAME")]
        channel: String,
        /// Start from a preset.
        #[arg(long, value_parser = ["recommended", "minimal", "everything"], required_unless_present = "copy_from", conflicts_with = "copy_from")]
        preset: Option<String>,
        /// Copy the profile of another channel.
        #[arg(long, value_name = "CH")]
        copy_from: Option<String>,
        /// Agree to replace another g_language value in user.cfg with english.
        #[arg(long)]
        switch_language: bool,
        /// Confirm. Without it, setup only says what it would do (exit 6).
        #[arg(long)]
        yes: bool,
    },
    /// Build and apply the enhanced global.ini to a channel.
    Apply {
        #[command(flatten)]
        sel: ChannelSel,
        /// Do nothing when the status is Applied.
        #[arg(long)]
        if_stale: bool,
        /// Run up to validation and preflight; write nothing.
        #[arg(long)]
        dry_run: bool,
        /// Use this profile file instead of the channel's profile.toml.
        #[arg(long, value_name = "FILE")]
        profile: Option<PathBuf>,
        /// Give up when the budget runs out before the write phase (exit 3).
        #[arg(long, value_name = "SECONDS")]
        timeout: Option<u64>,
    },
    /// Applied, Stale, Changed by another tool, or Not applied, per channel.
    Status {
        #[arg(long, value_name = "NAME")]
        channel: Option<String>,
    },
    /// Remove StarOrg changes: back to stock.
    Remove {
        #[command(flatten)]
        sel: ChannelSel,
        /// Also remove a global.ini from another tool (it is backed up first).
        #[arg(long)]
        yes: bool,
    },
    /// Put a backup back.
    Restore {
        #[arg(long, value_name = "NAME")]
        channel: String,
        #[arg(long, value_name = "ID")]
        backup: String,
        /// Restore a backup made for another build.
        #[arg(long)]
        allow_build_mismatch: bool,
    },
    /// List a channel's backups.
    Backups {
        #[arg(long, value_name = "NAME")]
        channel: String,
    },
    /// List patches and, when a cached extraction exists, their outcome.
    Patches {
        #[arg(long, value_name = "NAME")]
        channel: Option<String>,
    },
    /// Report installs, paths, casing, links, other tools' traces. Changes nothing.
    Doctor,
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
        Command::Channels => commands::channels(&cli.global),
        Command::Build {
            channel,
            out,
            timeout,
        } => commands::build(&cli.global, &channel, &out, timeout),
        Command::Setup {
            channel,
            preset,
            copy_from,
            switch_language,
            yes,
        } => apply_cmds::setup(
            &cli.global,
            &channel,
            preset.as_deref(),
            copy_from.as_deref(),
            switch_language,
            yes,
        ),
        Command::Apply {
            sel,
            if_stale,
            dry_run,
            profile,
            timeout,
        } => apply_cmds::apply(&cli.global, &sel, if_stale, dry_run, profile, timeout),
        Command::Status { channel } => apply_cmds::status(&cli.global, channel.as_deref()),
        Command::Remove { sel, yes } => apply_cmds::remove(&cli.global, &sel, yes),
        Command::Restore {
            channel,
            backup,
            allow_build_mismatch,
        } => apply_cmds::restore(&cli.global, &channel, &backup, allow_build_mismatch),
        Command::Backups { channel } => apply_cmds::backups(&cli.global, &channel),
        Command::Patches { channel } => apply_cmds::patches(&cli.global, channel.as_deref()),
        Command::Doctor => apply_cmds::doctor(&cli.global),
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
