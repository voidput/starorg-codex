//! Command-line interface: `run(args) -> ExitCode` and the `starorg-strings` binary
//! (spec 8).
//!
//! The CLI never opens a window, needs no display, and links no GTK, WebView or HTTP client.

mod apply_cmds;
mod blueprint_cmds;
mod commands;
mod hook_cmds;

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
        /// Whose owned blueprints to mark: a name or geid, `auto` or `any`. Default: the
        /// channel setting (`blueprints use`).
        #[arg(long, value_name = "NAME|GEID")]
        character: Option<String>,
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
    /// LUG Helper launch hooks: run them, or add/remove them in the hook scripts.
    Hook {
        #[command(subcommand)]
        cmd: HookCmd,
    },
    /// Owned blueprints per character: log scan, Smart Citizen import, active character.
    Blueprints {
        #[command(subcommand)]
        cmd: BlueprintCmd,
    },
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

/// Hook script locations (default: `sc-prelaunch.sh` / `sc-postexit.sh` in the Wine prefix).
#[derive(Debug, Clone, Default, Args)]
pub struct HookScripts {
    #[arg(long, value_name = "FILE")]
    pub prelaunch_script: Option<PathBuf>,
    #[arg(long, value_name = "FILE")]
    pub postexit_script: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum HookCmd {
    /// Before launch: re-apply every set-up channel whose build, game data or settings
    /// changed. Never fails the launch: exits 0 except on usage errors.
    Prelaunch {
        /// Time budget in seconds (default 60).
        #[arg(long, value_name = "SECONDS")]
        timeout: Option<u64>,
    },
    /// After exit: read new "Received Blueprint" lines from the logs, then re-apply what is
    /// stale. Never fails: exits 0 except on usage errors.
    Postexit {
        /// Time budget in seconds (default 120).
        #[arg(long, value_name = "SECONDS")]
        timeout: Option<u64>,
    },
    /// Add the StarOrg block to the hook scripts (backed up first; the rest is kept).
    Install {
        #[command(flatten)]
        scripts: HookScripts,
        /// Call this binary by absolute path instead of `starorg-strings` from PATH.
        #[arg(long, value_name = "PATH")]
        bin: Option<PathBuf>,
    },
    /// Remove the StarOrg block from the hook scripts (backed up first).
    Uninstall {
        #[command(flatten)]
        scripts: HookScripts,
    },
    /// Show whether the hook scripts carry the StarOrg block.
    Status {
        #[command(flatten)]
        scripts: HookScripts,
        /// Compare against the block `install --bin PATH` would write.
        #[arg(long, value_name = "PATH")]
        bin: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
pub enum BlueprintCmd {
    /// Read new blueprint lines from Game.log and logbackups (incremental).
    Scan {
        /// Default: every channel.
        #[arg(long, value_name = "NAME")]
        channel: Option<String>,
    },
    /// Characters seen in a channel's logs.
    Characters {
        #[arg(long, value_name = "NAME", default_value = "LIVE")]
        channel: String,
    },
    /// A character's owned blueprints.
    List {
        #[arg(long, value_name = "NAME", default_value = "LIVE")]
        channel: String,
        /// Name or geid. Default: the channel setting.
        #[arg(long, value_name = "NAME|GEID")]
        character: Option<String>,
    },
    /// Pick whose owned blueprints apply marks: `auto`, `any`, or a name or geid.
    Use {
        #[arg(long, value_name = "NAME", default_value = "LIVE")]
        channel: String,
        #[arg(value_name = "auto|any|NAME|GEID")]
        character: String,
    },
    /// Import Smart Citizen's blueprint tracker export (owned-blueprints.json).
    Import {
        #[arg(value_name = "FILE")]
        file: PathBuf,
        #[arg(long, value_name = "NAME|GEID")]
        character: String,
        #[arg(long, value_name = "NAME", default_value = "LIVE")]
        channel: String,
        /// Print the report; write nothing.
        #[arg(long)]
        dry_run: bool,
        /// Test hook: blueprint names to match against (`id<TAB>name` per line) instead of
        /// GameData.
        #[arg(long, hide = true, value_name = "FILE")]
        test_catalog: Option<PathBuf>,
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
            character,
        } => apply_cmds::apply(
            &cli.global,
            &sel,
            if_stale,
            dry_run,
            profile,
            timeout,
            character,
        ),
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
        Command::Hook { cmd } => hook_cmds::run(&cli.global, cmd),
        Command::Blueprints { cmd } => blueprint_cmds::run(&cli.global, cmd),
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
