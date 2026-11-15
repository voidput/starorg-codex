//! Command-line interface: `run(args) -> ExitCode` and the `starorg-strings` binary
//! (spec 8).
//!
//! The CLI never opens a window, needs no display, and links no GTK, WebView or HTTP client.

mod apply_cmds;
mod blueprint_cmds;
mod commands;
mod dev_cmds;
mod fleet_cmds;
mod gamedata_cmd;
mod hook_cmds;
mod version_cmd;

use clap::{Args, Parser, Subcommand};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
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
    #[cfg(feature = "test-hooks")]
    #[arg(long, global = true, hide = true, value_name = "FILE")]
    pub test_stock_ini: Option<PathBuf>,
    /// Test hook: GameData JSON to go with `--test-stock-ini` (default: empty GameData).
    #[cfg(feature = "test-hooks")]
    #[arg(long, global = true, hide = true, value_name = "FILE")]
    pub test_gamedata: Option<PathBuf>,
    /// Test hook: sleep this many milliseconds between writing global.ini and recording it.
    #[cfg(feature = "test-hooks")]
    #[arg(long, global = true, hide = true, value_name = "MS")]
    pub test_pause_after_write_ms: Option<u64>,
    /// Test hook: sleep this many milliseconds before the user.cfg step.
    #[cfg(feature = "test-hooks")]
    #[arg(long, global = true, hide = true, value_name = "MS")]
    pub test_pause_before_cfg_ms: Option<u64>,
}

impl GlobalOpts {
    /// `--test-stock-ini` (only in builds with the `test-hooks` feature).
    pub fn test_stock_ini(&self) -> Option<&std::path::Path> {
        #[cfg(feature = "test-hooks")]
        return self.test_stock_ini.as_deref();
        #[cfg(not(feature = "test-hooks"))]
        None
    }

    /// `--test-gamedata` (only in builds with the `test-hooks` feature).
    pub fn test_gamedata(&self) -> Option<&std::path::Path> {
        #[cfg(feature = "test-hooks")]
        return self.test_gamedata.as_deref();
        #[cfg(not(feature = "test-hooks"))]
        None
    }

    /// `--test-pause-before-cfg-ms` (only in builds with the `test-hooks` feature).
    pub fn test_pause_before_cfg_ms(&self) -> Option<u64> {
        #[cfg(feature = "test-hooks")]
        return self.test_pause_before_cfg_ms;
        #[cfg(not(feature = "test-hooks"))]
        None
    }

    /// `--test-pause-after-write-ms` (only in builds with the `test-hooks` feature).
    pub fn test_pause_after_write_ms(&self) -> Option<u64> {
        #[cfg(feature = "test-hooks")]
        return self.test_pause_after_write_ms;
        #[cfg(not(feature = "test-hooks"))]
        None
    }
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
        /// Enhancement preset: recommended (default), minimal or everything.
        #[arg(long, value_name = "NAME", conflicts_with_all = ["profile", "no_enhancements"])]
        preset: Option<String>,
        /// A profile.toml (`preset = ...` plus an optional `[enhance]` table).
        #[arg(long, value_name = "FILE", conflicts_with = "no_enhancements")]
        profile: Option<PathBuf>,
        /// Write the stock text unchanged (byte-identical to the file in Data.p4k).
        #[arg(long)]
        no_enhancements: bool,
        /// Owned blueprints of the active character, for `[Owned]` markers: an owned-set
        /// JSON (`{"entries": {...}}`) or a JSON list of names.
        #[arg(long, value_name = "FILE")]
        owned_blueprints: Option<PathBuf>,
        /// Also write the coverage report (spec 13) to this file.
        #[arg(long, value_name = "FILE")]
        coverage_out: Option<PathBuf>,
        /// Also write every generated edit (key, category, record, added text) as JSON.
        #[arg(long, value_name = "FILE")]
        edits_out: Option<PathBuf>,
        /// Ship order and renames from a ship-names.toml (addendum §6.4), used as given:
        /// nothing is written unless it says `enabled = true`.
        #[arg(long, value_name = "FILE", conflicts_with = "no_enhancements")]
        ship_names: Option<PathBuf>,
    },
    /// Write a channel's GameData (items, ships, contracts, ...) as JSON to a file.
    Gamedata {
        #[arg(long, value_name = "NAME")]
        channel: String,
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
        /// Give up after this many seconds (exit 3).
        #[arg(long, value_name = "SECONDS")]
        timeout: Option<u64>,
        /// Network (opt-in): use the published dump of this exact build when there is one,
        /// verified by checksum; extract locally otherwise.
        #[arg(long)]
        use_published_data: bool,
        /// Where downloaded dumps are cached (default: the app cache directory).
        #[arg(long, value_name = "DIR")]
        cache_dir: Option<PathBuf>,
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
    /// Version and build details. No network: the app's Settings checks for updates.
    Version,
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
    /// Your fleet (RSI+ / FleetYards / FleetView export or a list) for ship order and names.
    Fleet {
        #[command(subcommand)]
        cmd: FleetCmd,
    },
    /// Developer tools: DataCore records as JSON, type exports, type counts. Read only.
    Dev {
        #[command(subcommand)]
        cmd: DevCmd,
    },
    /// Internal: extract one Data.p4k in a separate process (spec 3.3).
    #[command(hide = true)]
    ExtractWorker {
        #[arg(long)]
        p4k: PathBuf,
        #[arg(long)]
        out: PathBuf,
        /// Read only the stock global.ini (skip DataCore).
        #[arg(long)]
        ini_only: bool,
        /// Also write keyrefs.json (the loc-reference pass for Strings grouping).
        #[arg(long, conflicts_with = "ini_only")]
        keyrefs: bool,
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
        #[cfg(feature = "test-hooks")]
        #[arg(long, hide = true, value_name = "FILE")]
        test_catalog: Option<PathBuf>,
    },
}

/// `--test-dcb <FILE>`: a raw DCB instead of the channel's Data.p4k (test hook).
#[derive(Debug, Clone, Default, Args)]
pub struct TestDcbArg {
    #[cfg(feature = "test-hooks")]
    #[arg(long, hide = true, value_name = "FILE")]
    pub test_dcb: Option<PathBuf>,
}

impl From<TestDcbArg> for dev_cmds::TestDcb {
    fn from(a: TestDcbArg) -> Self {
        #[cfg(feature = "test-hooks")]
        return dev_cmds::TestDcb(a.test_dcb);
        #[cfg(not(feature = "test-hooks"))]
        {
            let _ = a;
            dev_cmds::TestDcb(None)
        }
    }
}

/// Developer mode from the command line (addendum 7.9): read only, no setting needed.
#[derive(Debug, Subcommand)]
pub enum DevCmd {
    /// One record as JSON on stdout (StarBreaker's form: references as relative paths).
    Record {
        /// A GUID, a record path or a record name.
        #[arg(value_name = "PATH|GUID|NAME")]
        what: String,
        #[arg(long, value_name = "NAME", default_value = "LIVE")]
        channel: String,
        /// One line instead of indented JSON.
        #[arg(long)]
        compact: bool,
        #[command(flatten)]
        test_dcb: TestDcbArg,
    },
    /// Every record of one struct type, streamed to a file as one JSON array.
    ExportType {
        #[arg(value_name = "TYPE")]
        name: String,
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
        #[arg(long, value_name = "NAME", default_value = "LIVE")]
        channel: String,
        /// One compact record per line instead of one array.
        #[arg(long)]
        ndjson: bool,
        /// Main records only (sub-records of shared files left out).
        #[arg(long)]
        main_only: bool,
        #[command(flatten)]
        test_dcb: TestDcbArg,
    },
    /// Struct types with their record and main-record counts.
    Types {
        #[arg(long, value_name = "NAME", default_value = "LIVE")]
        channel: String,
        #[command(flatten)]
        test_dcb: TestDcbArg,
    },
}

#[derive(Debug, Subcommand)]
pub enum FleetCmd {
    /// Import a fleet: HangarXPLOR JSON or CSV (RSI+ "Export fleet", FleetYards, FleetView,
    /// hangar.link) or a plain list, one ship per line. Replaces the fleet list unless
    /// --merge. Writes only the app's own files; reads the game read-only.
    Import {
        /// The file, or `-` for stdin.
        #[arg(value_name = "FILE|-")]
        file: PathBuf,
        /// Print the report; write nothing.
        #[arg(long)]
        dry_run: bool,
        /// Add to the current fleet (count = the larger) instead of replacing it.
        #[arg(long)]
        merge: bool,
        #[arg(long, value_name = "FORMAT", default_value = "auto",
              value_parser = ["auto", "xplor-json", "xplor-csv", "text"])]
        format: String,
        /// The build whose ships to match against (default: the default channel, else LIVE).
        #[arg(long, value_name = "NAME")]
        channel: Option<String>,
    },
    /// The fleet file, the ship-names settings in use for a channel, and the listed ships
    /// in order with their final names.
    List {
        #[arg(long, value_name = "NAME")]
        channel: Option<String>,
    },
    /// Match an unmatched or ambiguous row by hand: a hull id, or `none` (not in the game).
    Match {
        #[arg(value_name = "ROW NAME")]
        name: String,
        #[arg(value_name = "HULL|none")]
        hull: String,
        #[arg(long, value_name = "NAME")]
        channel: Option<String>,
    },
    /// Forget the imported fleet (hand matches and the ship order are kept).
    Clear,
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
            preset,
            profile,
            no_enhancements,
            owned_blueprints,
            coverage_out,
            edits_out,
            ship_names,
        } => commands::build(
            &cli.global,
            &commands::BuildArgs {
                channel: &channel,
                out: &out,
                timeout,
                preset,
                profile,
                no_enhancements,
                owned_blueprints,
                coverage_out,
                edits_out,
                ship_names,
            },
        ),
        Command::Gamedata {
            channel,
            out,
            timeout,
            use_published_data,
            cache_dir,
        } => gamedata_cmd::run(
            &cli.global,
            &gamedata_cmd::Args {
                channel: &channel,
                out: &out,
                timeout,
                use_published_data,
                cache_dir,
            },
        ),
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
        Command::Version => version_cmd::version(&cli.global),
        Command::Hook { cmd } => hook_cmds::run(&cli.global, cmd),
        Command::Blueprints { cmd } => blueprint_cmds::run(&cli.global, cmd),
        Command::Fleet { cmd } => fleet_cmds::run(&cli.global, cmd),
        Command::Dev { cmd } => dev_cmds::run(&cli.global, cmd),
        Command::ExtractWorker {
            p4k,
            out,
            ini_only,
            keyrefs,
            test_panic,
            test_hang,
        } => {
            use starorg_data::worker::{TestHook, run_opts};
            // The worker writes only below --out; never inside the game or its prefix.
            let out_abs = commands::canonical_lenient(&out);
            let p4k_abs = commands::canonical_lenient(&p4k);
            // The channel folder (it holds build_manifest.id) and the whole Wine prefix.
            let mut protected: Vec<PathBuf> = p4k_abs
                .parent()
                .filter(|d| d.join("build_manifest.id").is_file())
                .map(Path::to_path_buf)
                .into_iter()
                .collect();
            protected.extend(starorg_core::discovery::enclosing_wine_prefix(&p4k_abs));
            if let Some(r) = protected.iter().find(|r| out_abs.starts_with(r)) {
                eprintln!(
                    "starorg-strings: --out {} is inside {}; refusing",
                    out_abs.display(),
                    r.display()
                );
                return ExitCode::from(exit::USAGE);
            }
            let hook = if test_panic {
                TestHook::Panic
            } else if test_hang {
                TestHook::Hang
            } else {
                TestHook::None
            };
            let opts = starorg_data::extract::Options { ini_only, keyrefs };
            ExitCode::from(run_opts(&p4k, &out, hook, opts) as u8)
        }
    }
}
