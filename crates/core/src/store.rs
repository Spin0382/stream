//! Settings and the library, stored as plain JSON under the XDG config dir.
//!
//! JSON on purpose: the app is small, the files are human readable, and the Kotlin
//! side (or a user with a text editor) can read them too.

use crate::error::Result;
use crate::live::LiveSource;
use crate::model::MediaItem;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Where the JVM extension host listens (`127.0.0.1:PORT`).
    pub host_addr: Option<String>,
    /// Bearer token the host wants.
    pub host_token: Option<String>,
    /// How to start the host when it is not running, e.g.
    /// `java -jar /opt/streamgoat/streamgoat-host.jar`.
    pub host_command: Option<String>,
    /// Start the host automatically on launch.
    pub host_autostart: bool,
    /// Stremio add-on urls (`https://…/manifest.json`).
    pub addons: Vec<String>,
    /// Live TV playlists and Xtream logins.
    pub live_sources: Vec<LiveSource>,
    /// Optional standalone XMLTV url.
    pub epg_url: Option<String>,
    /// mpv binary (empty = `mpv` from PATH).
    pub player_command: Option<String>,
    /// Extra mpv flags.
    pub player_args: Vec<String>,
    /// Accent colour, `#rrggbb`.
    pub accent: Option<String>,
    /// Ask before playing torrent sources.
    pub confirm_torrents: bool,
}

impl Settings {
    pub fn host_command_argv(&self) -> Vec<String> {
        self.host_command
            .as_deref()
            .map(split_command)
            .unwrap_or_default()
    }
}

/// Splits a command line respecting very simple quoting.
pub fn split_command(command: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for ch in command.chars() {
        match ch {
            '"' | '\'' if quote == Some(ch) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(ch),
            c if c.is_whitespace() && quote.is_none() => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

impl Default for Store {
    fn default() -> Self {
        Store::at(default_root())
    }
}

pub fn default_root() -> PathBuf {
    dirs::config_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("streamgoat")
}

impl Store {
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Store { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn settings_path(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    fn library_path(&self) -> PathBuf {
        self.root.join("library.json")
    }

    pub fn load_settings(&self) -> Settings {
        read_json(&self.settings_path()).unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        write_json(&self.settings_path(), settings)
    }

    pub fn load_library(&self) -> Vec<MediaItem> {
        read_json(&self.library_path()).unwrap_or_default()
    }

    pub fn save_library(&self, items: &[MediaItem]) -> Result<()> {
        write_json(&self.library_path(), items)
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Writes to a temp file first so a crash never leaves half a settings file.
fn write_json<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_commands_with_quotes() {
        assert_eq!(
            split_command("java -jar \"/opt/my app/host.jar\" --port 0"),
            vec![
                "java".to_string(),
                "-jar".to_string(),
                "/opt/my app/host.jar".to_string(),
                "--port".to_string(),
                "0".to_string()
            ]
        );
    }

    #[test]
    fn round_trips_settings_and_library() {
        let dir = std::env::temp_dir().join(format!("streamgoat-test-{}", std::process::id()));
        let store = Store::at(&dir);

        let mut settings = store.load_settings();
        settings
            .addons
            .push("https://a.example/manifest.json".to_string());
        settings.live_sources.push(LiveSource::M3u {
            name: "Demo".to_string(),
            spec: "https://a.example/list.m3u".to_string(),
        });
        store.save_settings(&settings).unwrap();

        let loaded = store.load_settings();
        assert_eq!(loaded.addons, settings.addons);
        assert_eq!(loaded.live_sources.len(), 1);
        assert_eq!(loaded.live_sources[0].name(), "Demo");

        store
            .save_library(&[MediaItem::new("Dune", "https://p/1")])
            .unwrap();
        assert_eq!(store.load_library().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
