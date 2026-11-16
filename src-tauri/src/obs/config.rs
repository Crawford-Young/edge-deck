//! Connection settings read from OBS's own obs-websocket config file.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// obs-websocket's default server port.
pub const DEFAULT_PORT: u16 = 4455;

/// Where to connect and how to authenticate. `Debug` never prints the password.
#[derive(Clone, PartialEq, Eq)]
pub struct Settings {
    pub(crate) port: u16,
    pub(crate) password: Option<String>,
}

#[cfg(test)]
impl Settings {
    pub fn new(port: u16, password: Option<&str>) -> Self {
        Self {
            port,
            password: password.map(str::to_owned),
        }
    }
}

impl fmt::Debug for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Settings")
            .field("port", &self.port)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// The keys of obs-websocket's `config.json` the watcher uses.
#[derive(Deserialize)]
struct RawConfig {
    #[serde(default)]
    server_enabled: bool,
    #[serde(default)]
    server_port: Option<u16>,
    #[serde(default)]
    auth_required: bool,
    #[serde(default)]
    server_password: String,
}

/// Parse obs-websocket's `config.json`. `None` means "off": the server is
/// disabled or the text is not a readable config.
pub fn parse_config(text: &str) -> Option<Settings> {
    let raw: RawConfig = serde_json::from_str(text).ok()?;
    raw.server_enabled.then(|| Settings {
        port: raw.server_port.unwrap_or(DEFAULT_PORT),
        password: raw.auth_required.then_some(raw.server_password),
    })
}

/// Read and parse the config file; a missing or unreadable file is `None`.
pub fn read_config(path: &Path) -> Option<Settings> {
    parse_config(&fs::read_to_string(path).ok()?)
}

/// The config file under a given `%APPDATA%` directory.
fn config_path_in(appdata: &Path) -> PathBuf {
    appdata
        .join("obs-studio")
        .join("plugin_config")
        .join("obs-websocket")
        .join("config.json")
}

/// `%APPDATA%\obs-studio\plugin_config\obs-websocket\config.json`, if `APPDATA` is set.
pub fn default_config_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|dir| config_path_in(Path::new(&dir)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_with_auth_yields_port_and_password() {
        let text = r#"{"alerts_enabled":false,"auth_required":true,"first_load":false,
            "server_enabled":true,"server_password":"placeholder-pw","server_port":4567}"#;
        assert_eq!(
            parse_config(text),
            Some(Settings::new(4567, Some("placeholder-pw")))
        );
    }

    #[test]
    fn enabled_without_auth_ignores_a_leftover_password() {
        let text = r#"{"auth_required":false,"server_enabled":true,
            "server_password":"placeholder-pw","server_port":4567}"#;
        assert_eq!(parse_config(text), Some(Settings::new(4567, None)));
    }

    #[test]
    fn missing_port_falls_back_to_the_obs_default() {
        let text = r#"{"server_enabled":true}"#;
        assert_eq!(parse_config(text), Some(Settings::new(DEFAULT_PORT, None)));
    }

    #[test]
    fn disabled_server_is_off() {
        let text = r#"{"auth_required":true,"server_enabled":false,
            "server_password":"placeholder-pw","server_port":4455}"#;
        assert_eq!(parse_config(text), None);
    }

    #[test]
    fn missing_enabled_flag_is_off() {
        assert_eq!(parse_config(r#"{"server_port":4455}"#), None);
    }

    #[test]
    fn malformed_text_is_off() {
        assert_eq!(parse_config("not json"), None);
        assert_eq!(parse_config(r#"{"server_enabled":"yes"}"#), None);
        assert_eq!(
            parse_config(r#"{"server_enabled":true,"server_port":70000}"#),
            None
        );
    }

    #[test]
    fn debug_output_never_shows_the_password() {
        let shown = format!("{:?}", Settings::new(4567, Some("placeholder-pw")));
        assert!(!shown.contains("placeholder-pw"), "{shown}");
        assert!(shown.contains("4567"), "{shown}");
        let none = format!("{:?}", Settings::new(4567, None));
        assert!(none.contains("None"), "{none}");
    }

    #[test]
    fn missing_file_is_off() {
        let path = std::env::temp_dir().join("edge-deck-obs-config-test-does-not-exist.json");
        assert_eq!(read_config(&path), None);
    }

    #[test]
    fn reads_a_file_from_disk() {
        let dir = std::env::temp_dir().join(format!("edge-deck-obs-config-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        fs::write(&path, r#"{"server_enabled":true,"server_port":4999}"#).unwrap();
        let read = read_config(&path);
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(read, Some(Settings::new(4999, None)));
    }

    #[test]
    fn config_lives_under_obs_plugin_config() {
        let base = Path::new("appdata-placeholder");
        assert_eq!(
            config_path_in(base),
            base.join("obs-studio")
                .join("plugin_config")
                .join("obs-websocket")
                .join("config.json")
        );
    }
}
