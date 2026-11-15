//! Tauri commands: thin, typed wrappers over `starorg-core` (spec 3.1, 11).
//!
//! The webview is untrusted: commands take no paths from it. Paths are sent to it for display
//! only.

use serde::Serialize;
use starorg_core::discovery::{
    Channel, Discovery, DiscoveryEnv, DiscoveryRequest, Install, discover,
};

/// Same schema number and field names as `starorg-strings channels --json`.
pub const CHANNELS_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ChannelView {
    pub channel: String,
    pub branch: Option<String>,
    pub game_version: Option<String>,
    pub change: Option<u64>,
    pub version: Option<String>,
    pub build_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InstallView {
    pub root: String,
    pub prefix: Option<String>,
    /// Machine id, as in the CLI JSON (`default-prefix`, `lug-config`, ...).
    pub found_by: String,
    /// Human label, as in the CLI table (`default prefix location`, ...).
    pub found_by_label: String,
    pub channels: Vec<ChannelView>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ChannelsView {
    pub schema: u32,
    pub installs: Vec<InstallView>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
}

fn channel_view(c: &Channel) -> ChannelView {
    let b = c.build.as_ref();
    ChannelView {
        channel: c.name.clone(),
        branch: b.map(|b| b.branch.clone()),
        game_version: b.and_then(|b| b.game_version).map(|v| v.to_string()),
        change: b.and_then(|b| b.change),
        version: b.map(|b| b.version.clone()),
        build_error: c.build_error.clone(),
    }
}

fn install_view(i: &Install) -> InstallView {
    let found_by = serde_json::to_value(&i.found_by)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    InstallView {
        root: i.root.display().to_string(),
        prefix: i.prefix.as_ref().map(|p| p.display().to_string()),
        found_by,
        found_by_label: i.found_by.describe().to_string(),
        channels: i.channels.iter().map(channel_view).collect(),
    }
}

pub fn channels_view(d: &Discovery) -> ChannelsView {
    ChannelsView {
        schema: CHANNELS_SCHEMA,
        installs: d.installs.iter().map(install_view).collect(),
    }
}

/// Where discovery looks. The app passes no options today (setup wizard and saved config
/// arrive in M1/M3); tests point it at a fixture.
#[derive(Debug, Clone, Default)]
pub struct DiscoveryConfig {
    pub request: DiscoveryRequest,
}

/// Find installs and their channels: the same data as `starorg-strings channels`.
#[tauri::command]
pub async fn list_channels(
    config: tauri::State<'_, DiscoveryConfig>,
) -> Result<ChannelsView, String> {
    let req = config.request.clone();
    // Discovery reads files (launcher logs, manifests); keep it off the main thread.
    tauri::async_runtime::spawn_blocking(move || {
        channels_view(&discover(&req, &DiscoveryEnv::from_process()))
    })
    .await
    .map_err(|e| format!("discovery failed: {e}"))
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        name: "StarOrg Strings".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder, mock_context, noop_assets};
    use tauri::webview::InvokeRequest;

    fn fake_install(root: &std::path::Path) {
        for (name, manifest) in [
            (
                "LIVE",
                r#"{"Data":{"Branch":"sc-alpha-4.10.0","RequestedP4ChangeNum":"13165946","Version":"4.10.200.58746"}}"#,
            ),
            (
                "HOTFIX",
                r#"{"Data":{"Branch":"sc-alpha-4.10.0-hotfix","RequestedP4ChangeNum":"12572603","Version":"1.0.191.55227"}}"#,
            ),
            ("PTU", "not json"),
        ] {
            let d = root.join(name);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("Data.p4k"), b"not a real archive").unwrap();
            std::fs::write(d.join("build_manifest.id"), manifest).unwrap();
        }
    }

    fn fixture(game_dir: std::path::PathBuf) -> DiscoveryConfig {
        DiscoveryConfig {
            request: DiscoveryRequest {
                game_dir: Some(game_dir),
                prefix: None,
            },
        }
    }

    fn invoke(app: &tauri::App<tauri::test::MockRuntime>, cmd: &str) -> serde_json::Value {
        let webview = tauri::WebviewWindowBuilder::new(app, "main", Default::default())
            .build()
            .unwrap();
        get_ipc_response(
            &webview,
            InvokeRequest {
                cmd: cmd.into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: "tauri://localhost".parse().unwrap(),
                body: tauri::ipc::InvokeBody::default(),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .expect("command failed")
        .deserialize::<serde_json::Value>()
        .unwrap()
    }

    #[test]
    fn list_channels_over_ipc_returns_the_cli_shape() {
        let t = tempfile::tempdir().unwrap();
        fake_install(t.path());
        let app = crate::builder(mock_builder(), fixture(t.path().to_path_buf()))
            .build(mock_context(noop_assets()))
            .unwrap();
        let v = invoke(&app, "list_channels");

        assert_eq!(v["schema"], 1);
        let installs = v["installs"].as_array().unwrap();
        assert_eq!(installs.len(), 1);
        let i = &installs[0];
        assert_eq!(i["found_by"], "game-dir");
        assert_eq!(i["found_by_label"], "--game-dir");
        assert!(i["prefix"].is_null());
        let chans = i["channels"].as_array().unwrap();
        let by = |n: &str| chans.iter().find(|c| c["channel"] == n).unwrap().clone();
        let live = by("LIVE");
        assert_eq!(live["branch"], "sc-alpha-4.10.0");
        assert_eq!(live["game_version"], "4.10.0");
        assert_eq!(live["change"], 13165946);
        assert_eq!(live["version"], "4.10.200.58746");
        assert!(live["build_error"].is_null());
        // HOTFIX: game version from Data.Branch, not Data.Version (spec 4.1, M0 acceptance).
        assert_eq!(by("HOTFIX")["game_version"], "4.10.0");
        let ptu = by("PTU");
        assert!(ptu["branch"].is_null());
        assert!(ptu["build_error"].is_string());
    }

    #[test]
    fn list_channels_with_nothing_found_is_empty_not_an_error() {
        let t = tempfile::tempdir().unwrap();
        let app = crate::builder(mock_builder(), fixture(t.path().join("nope")))
            .build(mock_context(noop_assets()))
            .unwrap();
        let v = invoke(&app, "list_channels");
        assert_eq!(v["installs"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn app_info_over_ipc_reports_the_workspace_version() {
        let app = crate::builder(mock_builder(), DiscoveryConfig::default())
            .build(mock_context(noop_assets()))
            .unwrap();
        let v = invoke(&app, "app_info");
        assert_eq!(v["name"], "StarOrg Strings");
        assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
    }

    /// Prints what the app shows for this machine, for comparing with
    /// `starorg-strings channels --json`. Needs a real install.
    #[test]
    #[ignore]
    fn print_real_channels() {
        let d = discover(&DiscoveryRequest::default(), &DiscoveryEnv::from_process());
        println!("{}", serde_json::to_string(&channels_view(&d)).unwrap());
    }
}
