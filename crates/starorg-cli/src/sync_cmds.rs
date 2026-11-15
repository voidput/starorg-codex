//! `sync status|set`: the StarOrg Tools environment and the "What to sync" switches
//! (`[sync]` in config.toml, shared with the app; addendum §5).
//!
//! The CLI links no HTTP client, so it never talks to StarOrg Tools itself: these switches
//! steer the app, and `import-game-log` also steers `blueprints scan` and `hook postexit`
//! here. `--environment` defaults to `$STARORG_ENV` when that is set.

use crate::apply_cmds::ctx;
use crate::commands::{JSON_SCHEMA, fail};
use crate::{GlobalOpts, SyncCmd, exit};
use serde_json::json;
use starorg_core::config::Config;
use starorg_core::starorg::{Environment, SyncPrefs};
use std::process::ExitCode;

pub fn run(g: &GlobalOpts, cmd: SyncCmd) -> ExitCode {
    let c = match ctx(g) {
        Ok(c) => c,
        Err(e) => return e,
    };
    match cmd {
        SyncCmd::Status => print(g, &SyncPrefs::load(&c.dirs), "sync status"),
        SyncCmd::Set {
            environment,
            import_game_log,
            push,
            pull,
            pull_fleet,
            auto,
        } => {
            let mut cfg = match Config::load(&c.dirs) {
                Ok(cfg) => cfg,
                Err(e) => return fail(g, exit::ERROR, "io", &e.to_string()),
            };
            if let Some(e) = environment {
                match Environment::parse(&e) {
                    Some(e) => cfg.sync.environment = e,
                    None => {
                        return fail(
                            g,
                            exit::USAGE,
                            "usage",
                            &format!("unknown environment {e:?}: production or dev"),
                        );
                    }
                }
            }
            let set = |slot: &mut bool, v: Option<OnOff>| {
                if let Some(v) = v {
                    *slot = v == OnOff::On;
                }
            };
            set(&mut cfg.sync.import_game_log, import_game_log);
            set(&mut cfg.sync.push_blueprints, push);
            set(&mut cfg.sync.pull_blueprints, pull);
            set(&mut cfg.sync.pull_fleet, pull_fleet);
            set(&mut cfg.sync.auto_sync, auto);
            if let Err(e) = cfg.save(&c.dirs) {
                return fail(g, exit::ERROR, "io", &e.to_string());
            }
            print(g, &cfg.sync, "sync set")
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum OnOff {
    On,
    Off,
}

fn print(g: &GlobalOpts, p: &SyncPrefs, command: &str) -> ExitCode {
    if g.json {
        println!(
            "{}",
            json!({ "schema": JSON_SCHEMA, "command": command, "exit_code": exit::OK,
                    "environment": p.environment.as_str(), "host": p.environment.host(),
                    "sync": p })
        );
    } else if !g.quiet {
        let on = |b: bool| if b { "on" } else { "off" };
        println!(
            "StarOrg Tools: {} ({})",
            p.environment.as_str(),
            p.environment.host()
        );
        println!(
            "  import blueprints from Game.log: {}",
            on(p.import_game_log)
        );
        println!(
            "  push owned blueprints (app):     {}",
            on(p.push_blueprints)
        );
        println!(
            "  pull owned blueprints (app):     {}",
            on(p.pull_blueprints)
        );
        println!("  pull fleet/hangar (app):         {}", on(p.pull_fleet));
        println!("  sync after game exit (app):      {}", on(p.auto_sync));
        println!("The CLI never connects to StarOrg Tools; sign-in and sync are in the app.");
    }
    ExitCode::from(exit::OK)
}
