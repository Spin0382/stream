//! Client for the JVM **extension host**: the Kotlin/Java sidecar that runs the
//! CloudStream `.cs3` extensions.
//!
//! This is the whole point of the hybrid design: the Rust app owns the UI, Live TV,
//! Stremio, downloads and the player, while everything that needs the JVM (dex2jar,
//! `URLClassLoader`, the CloudStream API, OkHttp/Jsoup inside extensions) stays in
//! the JVM. They talk over a tiny JSON-RPC-ish protocol on loopback:
//!
//! ```text
//! POST http://127.0.0.1:<port>/rpc
//! Authorization: Bearer <token>
//! {"id":1,"method":"providers.search","params":{"provider":"...","query":"..."}}
//! -> {"id":1,"result":{"items":[...]}}
//! -> {"id":1,"error":{"code":-1,"message":"..."}}
//! ```
//!
//! See `docs/host-protocol.md` for the full method list and payload shapes.

use crate::error::{Error, Result};
use crate::model::{Episode, MediaItem, MediaKind, StreamKind, StreamLink, Subtitle};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub plugin: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default, rename = "tvTypes")]
    pub tv_types: Vec<String>,
    #[serde(default, rename = "iconUrl")]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub supported_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "internalName")]
    pub internal_name: String,
    #[serde(default)]
    pub version: i64,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default, rename = "providerCount")]
    pub provider_count: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostHealth {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub providers: i64,
    #[serde(default)]
    pub plugins: i64,
}

#[derive(Debug, Clone, Default)]
pub struct HomePage {
    pub items: Vec<MediaItem>,
    pub has_next: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoInfo {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default, rename = "iconUrl")]
    pub icon_url: Option<String>,
    #[serde(default, rename = "pluginLists")]
    pub plugin_lists: i64,
}

/// An extension offered by one of the configured repositories.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AvailablePlugin {
    #[serde(default, rename = "internalName")]
    pub internal_name: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: i64,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    /// 0 down, 1 ok, 2 slow, 3 beta only.
    #[serde(default)]
    pub status: i64,
    /// The `.cs3` url.
    #[serde(default)]
    pub url: Option<String>,
    /// The plain JVM `.jar` url, when the extension is published for JVM too.
    #[serde(default, rename = "jarUrl")]
    pub jar_url: Option<String>,
    #[serde(default, rename = "repositoryUrl")]
    pub repository_url: Option<String>,
    /// Set when the extension is installed already.
    #[serde(default, rename = "installedVersion")]
    pub installed_version: Option<i64>,
}

impl AvailablePlugin {
    pub fn is_installed(&self) -> bool {
        self.installed_version.is_some()
    }

    pub fn update_available(&self) -> bool {
        matches!(self.installed_version, Some(installed) if installed < self.version)
    }

    pub fn status_label(&self) -> &'static str {
        match self.status {
            1 => "ok",
            2 => "lento",
            3 => "beta",
            _ => "caído",
        }
    }

    pub fn download_url(&self) -> Option<&str> {
        self.jar_url.as_deref().or(self.url.as_deref())
    }
}

/// Talks to a running host. Cheap to clone: all state is the http client.
#[derive(Debug, Clone)]
pub struct HostClient {
    pub addr: String,
    pub token: String,
    http: reqwest::Client,
}

impl HostClient {
    /// `addr` is `127.0.0.1:PORT` (no scheme).
    pub fn new(addr: impl Into<String>, token: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_default();
        HostClient {
            addr: addr.into(),
            token: token.into(),
            http,
        }
    }

    pub fn url(&self) -> String {
        format!("http://{}/rpc", self.addr)
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let body = json!({"id": id, "method": method, "params": params});

        let response = self
            .http
            .post(self.url())
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .map_err(|error| Error::HostUnreachable {
                addr: self.addr.clone(),
                reason: error.to_string(),
            })?;

        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|error| Error::HostUnreachable {
                addr: self.addr.clone(),
                reason: error.to_string(),
            })?;

        let envelope: RpcResponse =
            serde_json::from_slice(&bytes).map_err(|error| Error::Parse {
                what: format!("host reply ({status}) for {method}"),
                reason: error.to_string(),
            })?;

        match (envelope.result, envelope.error) {
            (Some(result), _) => Ok(result),
            (None, Some(error)) => Err(Error::Host {
                code: error.code,
                message: error.message,
            }),
            (None, None) => Err(Error::parse(
                format!("host reply for {method}"),
                "neither result nor error",
            )),
        }
    }

    pub async fn health(&self) -> Result<HostHealth> {
        let value = self.call("health", json!({})).await?;
        Ok(serde_json::from_value(value)?)
    }

    pub async fn providers(&self) -> Result<Vec<ProviderInfo>> {
        let value = self.call("providers.list", json!({})).await?;
        let list = value
            .get("providers")
            .cloned()
            .unwrap_or(Value::Array(Vec::new()));
        Ok(serde_json::from_value(list)?)
    }

    pub async fn plugins(&self) -> Result<Vec<PluginInfo>> {
        let value = self.call("extensions.list", json!({})).await?;
        let list = value
            .get("plugins")
            .cloned()
            .unwrap_or(Value::Array(Vec::new()));
        Ok(serde_json::from_value(list)?)
    }

    pub async fn home(&self, provider: &str, page: i64) -> Result<HomePage> {
        let value = self
            .call(
                "providers.home",
                json!({"provider": provider, "page": page}),
            )
            .await?;
        let page: HomeWire = serde_json::from_value(value)?;
        Ok(HomePage {
            items: page.items.into_iter().map(WireItem::into_model).collect(),
            has_next: page.has_next,
        })
    }

    pub async fn search(&self, provider: &str, query: &str) -> Result<Vec<MediaItem>> {
        let value = self
            .call(
                "providers.search",
                json!({"provider": provider, "query": query}),
            )
            .await?;
        Ok(items_from(value))
    }

    /// Quick search across every provider (CloudStream's `quickSearch`).
    pub async fn quick_search(&self, query: &str) -> Result<Vec<MediaItem>> {
        let value = self
            .call("providers.quickSearch", json!({"query": query}))
            .await?;
        Ok(items_from(value))
    }

    pub async fn load(&self, provider: &str, url: &str) -> Result<MediaItem> {
        let value = self
            .call("providers.load", json!({"provider": provider, "url": url}))
            .await?;
        let item: WireItem = serde_json::from_value(match value.get("item") {
            Some(item) => item.clone(),
            None => value,
        })?;
        Ok(item.into_model())
    }

    pub async fn links(&self, provider: &str, url: &str) -> Result<Vec<StreamLink>> {
        let value = self
            .call("providers.links", json!({"provider": provider, "url": url}))
            .await?;
        let list = value
            .get("links")
            .cloned()
            .unwrap_or(Value::Array(Vec::new()));
        let links: Vec<WireLink> = serde_json::from_value(list)?;
        Ok(links.into_iter().map(WireLink::into_model).collect())
    }

    pub async fn repos(&self) -> Result<Vec<RepoInfo>> {
        let value = self.call("extensions.repos", json!({})).await?;
        let list = value
            .get("repos")
            .cloned()
            .unwrap_or(Value::Array(Vec::new()));
        Ok(serde_json::from_value(list)?)
    }

    pub async fn add_repo(&self, url: &str) -> Result<RepoInfo> {
        let value = self
            .call("extensions.repos.add", json!({"url": url}))
            .await?;
        let repo = value.get("repo").cloned().unwrap_or(Value::Null);
        Ok(serde_json::from_value(repo)?)
    }

    pub async fn remove_repo(&self, url: &str) -> Result<bool> {
        let value = self
            .call("extensions.repos.remove", json!({"url": url}))
            .await?;
        Ok(value
            .get("removed")
            .and_then(Value::as_bool)
            .unwrap_or(false))
    }

    /// Every extension of the configured repositories, with its installed version.
    pub async fn available(&self, repo: Option<&str>) -> Result<Vec<AvailablePlugin>> {
        let params = match repo {
            Some(url) => json!({"repo": url}),
            None => json!({}),
        };
        let value = self.call("extensions.available", params).await?;
        let list = value
            .get("plugins")
            .cloned()
            .unwrap_or(Value::Array(Vec::new()));
        Ok(serde_json::from_value(list).unwrap_or_default())
    }

    /// Downloads and loads an extension from the repositories. Returns its version.
    pub async fn install_plugin(&self, internal_name: &str) -> Result<i64> {
        let value = self
            .call("extensions.install", json!({"internalName": internal_name}))
            .await?;
        Ok(value
            .get("version")
            .and_then(Value::as_i64)
            .unwrap_or_default())
    }

    /// Updates every installed extension. Returns the names that changed.
    pub async fn update_extensions(&self) -> Result<Vec<String>> {
        let value = self.call("extensions.update", json!({})).await?;
        Ok(value
            .get("updated")
            .and_then(|list| list.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|name| name.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default())
    }

    pub async fn uninstall_plugin(&self, internal_name: &str) -> Result<bool> {
        let value = self
            .call(
                "extensions.uninstall",
                json!({"internalName": internal_name}),
            )
            .await?;
        Ok(value
            .get("removed")
            .and_then(Value::as_bool)
            .unwrap_or(false))
    }

    /// True when the host answers `health` inside a couple of seconds.
    pub async fn is_alive(&self) -> bool {
        matches!(
            tokio::time::timeout(Duration::from_secs(3), self.health()).await,
            Ok(Ok(_))
        )
    }
}

fn items_from(value: Value) -> Vec<MediaItem> {
    let list = match value.get("items") {
        Some(items) => items.clone(),
        None => value,
    };
    serde_json::from_value::<Vec<WireItem>>(list)
        .map(|items| items.into_iter().map(WireItem::into_model).collect())
        .unwrap_or_default()
}

#[derive(Debug, Deserialize)]
struct RpcResponse {
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    message: String,
}

// --- wire DTOs (camelCase, the shape the Kotlin host emits) -------------------

#[derive(Debug, Deserialize)]
struct HomeWire {
    #[serde(default)]
    items: Vec<WireItem>,
    #[serde(default, rename = "hasNext")]
    has_next: bool,
}

#[derive(Debug, Default, Deserialize)]
struct WireItem {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    poster: Option<String>,
    #[serde(default)]
    background: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    year: Option<i32>,
    /// `movie`, `series`, `anime`, `live`…
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    episodes: Vec<WireEpisode>,
}

impl WireItem {
    fn into_model(self) -> MediaItem {
        let mut item = MediaItem::new(&self.name, &self.url);
        if !self.id.is_empty() {
            item.id = self.id;
        }
        item.poster = self.poster;
        item.background = self.background;
        item.description = self.description;
        item.year = self.year;
        item.kind = match self.kind.as_deref().unwrap_or_default() {
            "series" => MediaKind::Series,
            "anime" => MediaKind::Anime,
            "movie" => MediaKind::Movie,
            "live" | "tv" => MediaKind::Live,
            _ => MediaKind::Other,
        };
        item.provider = self.provider;
        item.source = Some("host".to_string());
        item.episodes = self
            .episodes
            .into_iter()
            .map(|episode| episode.into_model())
            .collect();
        item
    }
}

#[derive(Debug, Default, Deserialize)]
struct WireEpisode {
    #[serde(default)]
    name: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    season: Option<i32>,
    #[serde(default)]
    episode: Option<i32>,
    #[serde(default)]
    thumbnail: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

impl WireEpisode {
    fn into_model(self) -> Episode {
        Episode {
            name: self.name,
            url: self.url,
            season: self.season,
            episode: self.episode,
            thumbnail: self.thumbnail,
            description: self.description,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct WireLink {
    #[serde(default)]
    url: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    quality: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    referer: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    /// `hls`, `dash`, `mp4`, `external`…
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    subtitles: Vec<WireSubtitle>,
}

#[derive(Debug, Default, Deserialize)]
struct WireSubtitle {
    #[serde(default)]
    lang: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    name: Option<String>,
}

impl WireLink {
    fn into_model(self) -> StreamLink {
        let mut link = StreamLink::new(self.url);
        link.name = self.name;
        link.quality = self.quality;
        link.source = self.source;
        link.referer = self.referer;
        link.headers = self.headers;
        link.kind = match self.kind.as_deref().unwrap_or_default() {
            "hls" => StreamKind::Hls,
            "dash" => StreamKind::Dash,
            "mp4" | "progressive" => StreamKind::Mp4,
            "torrent" => StreamKind::Torrent,
            "external" | "youtube" => StreamKind::External,
            _ => link.kind,
        };
        link.subtitles = self
            .subtitles
            .into_iter()
            .filter(|subtitle| !subtitle.url.is_empty())
            .map(|subtitle| Subtitle {
                lang: if subtitle.lang.is_empty() {
                    "und".to_string()
                } else {
                    subtitle.lang
                },
                url: subtitle.url,
                name: subtitle.name,
            })
            .collect();
        link
    }
}

/// A host process the app started itself (`java -jar streamgoat-host.jar`).
///
/// The host prints one line of JSON on stdout once it is listening:
/// `{"addr":"127.0.0.1:41235","token":"..."}`.
#[derive(Debug)]
pub struct HostProcess {
    pub client: HostClient,
    child: tokio::process::Child,
}

#[derive(Debug, Deserialize)]
struct Handshake {
    addr: String,
    #[serde(default)]
    token: String,
}

impl HostProcess {
    /// Spawns the host and waits for its handshake line.
    pub async fn spawn(program: &str, args: &[String], envs: &[(String, String)]) -> Result<Self> {
        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio::process::Command;

        let mut command = Command::new(program);
        command
            .args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true);
        for (key, value) in envs {
            command.env(key, value);
        }

        let mut child = command.spawn().map_err(|error| Error::HostUnreachable {
            addr: program.to_string(),
            reason: error.to_string(),
        })?;

        let stdout = child.stdout.take().ok_or_else(|| Error::HostUnreachable {
            addr: program.to_string(),
            reason: "the host has no stdout".to_string(),
        })?;
        let mut lines = BufReader::new(stdout).lines();

        let handshake = tokio::time::timeout(Duration::from_secs(120), async {
            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim();
                if line.starts_with('{') {
                    if let Ok(handshake) = serde_json::from_str::<Handshake>(line) {
                        return Some(handshake);
                    }
                }
            }
            None
        })
        .await
        .map_err(|_| Error::HostUnreachable {
            addr: program.to_string(),
            reason: "timed out waiting for the host handshake".to_string(),
        })?;

        let handshake = handshake.ok_or_else(|| Error::HostUnreachable {
            addr: program.to_string(),
            reason: "the host exited before saying where it listens".to_string(),
        })?;

        // Keep draining stdout so the child never blocks on a full pipe.
        tokio::spawn(async move { while let Ok(Some(_line)) = lines.next_line().await {} });

        Ok(HostProcess {
            client: HostClient::new(handshake.addr, handshake.token),
            child,
        })
    }

    pub fn addr(&self) -> &str {
        &self.client.addr
    }

    pub fn token(&self) -> &str {
        &self.client.token
    }

    pub async fn shutdown(mut self) {
        let _ = self.child.kill().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_wire_items() {
        let wire: WireItem = serde_json::from_str(
            r#"{"id":"host:1","name":"Dune","url":"https://p/1","type":"movie",
                "poster":"https://img/dune.jpg","year":2021,"provider":"Cine"}"#,
        )
        .unwrap();
        let item = wire.into_model();
        assert_eq!(item.name, "Dune");
        assert_eq!(item.kind, MediaKind::Movie);
        assert_eq!(item.source.as_deref(), Some("host"));
        assert_eq!(item.year, Some(2021));
    }

    #[test]
    fn maps_wire_links_and_subtitles() {
        let wire: WireLink = serde_json::from_str(
            r#"{"url":"https://cdn/a.m3u8","name":"Server 1","quality":"1080p",
                "type":"hls","headers":{"Referer":"https://x/"},
                "subtitles":[{"lang":"es","url":"https://cdn/es.vtt"}]}"#,
        )
        .unwrap();
        let link = wire.into_model();
        assert_eq!(link.kind, StreamKind::Hls);
        assert_eq!(link.label(), "Server 1 · 1080p");
        assert_eq!(
            link.headers.get("Referer").map(String::as_str),
            Some("https://x/")
        );
        assert_eq!(link.subtitles[0].lang, "es");
    }

    #[test]
    fn knows_when_an_extension_needs_updating() {
        let plugin: AvailablePlugin =
            serde_json::from_str(r#"{"internalName":"Cine","name":"Cine","version":7,"status":1}"#)
                .unwrap();
        assert!(!plugin.is_installed());
        assert!(!plugin.update_available());
        assert_eq!(plugin.status_label(), "ok");

        let installed: AvailablePlugin = serde_json::from_str(
            r#"{"internalName":"Cine","version":8,"installedVersion":7,"jarUrl":"https://r/x.jar"}"#,
        )
        .unwrap();
        assert!(installed.is_installed());
        assert!(installed.update_available());
        assert_eq!(installed.download_url(), Some("https://r/x.jar"));
    }

    #[test]
    fn builds_rpc_url() {
        let client = HostClient::new("127.0.0.1:9000", "secret");
        assert_eq!(client.url(), "http://127.0.0.1:9000/rpc");
    }
}
