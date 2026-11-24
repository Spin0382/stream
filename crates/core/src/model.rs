//! The app-wide data model: media items, episodes, stream links and subtitles.
//!
//! Every source (Stremio add-on, Live TV playlist, CloudStream extension through the
//! JVM host) is normalised into these types so the UI and the player never need to
//! know where something came from.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What kind of thing a [`MediaItem`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Movie,
    Series,
    Anime,
    Live,
    #[default]
    Other,
}

impl MediaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MediaKind::Movie => "movie",
            MediaKind::Series => "series",
            MediaKind::Anime => "anime",
            MediaKind::Live => "live",
            MediaKind::Other => "other",
        }
    }

    /// Best-effort mapping from Stremio's `type` field and CloudStream's tvTypes.
    pub fn from_stremio(value: &str) -> MediaKind {
        match value {
            "movie" => MediaKind::Movie,
            "series" => MediaKind::Series,
            "anime" => MediaKind::Anime,
            "tv" | "channel" | "live" => MediaKind::Live,
            _ => MediaKind::Other,
        }
    }
}

/// A card the UI can show: a movie, a series, a channel or a search result.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MediaItem {
    /// Stable identifier (`"host:provider:url"`, `"stremio:addon:type:id"`, …).
    pub id: String,
    pub name: String,
    /// The opaque data url the provider needs to load details or links.
    #[serde(default)]
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poster: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    #[serde(default)]
    pub kind: MediaKind,
    /// Which provider/add-on produced it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Where it came from: `stremio`, `host` or `live`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Episodes when the item is a series that has been fully loaded.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub episodes: Vec<Episode>,
}

impl MediaItem {
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        let url = url.into();
        let name = name.into();
        MediaItem {
            id: format!(
                "{}:{}",
                name.to_lowercase().replace(' ', "-"),
                short_id(&url)
            ),
            name,
            url,
            ..Default::default()
        }
    }

    pub fn with_poster(mut self, poster: Option<String>) -> Self {
        self.poster = poster.filter(|p| !p.is_empty());
        self
    }
}

fn short_id(url: &str) -> String {
    // Cheap stable hash, enough to give every card a unique egui id.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in url.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Episode {
    pub name: String,
    /// The data url to call `loadLinks` with.
    #[serde(default)]
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Episode {
    /// `S01E04` when both numbers are known.
    pub fn label(&self) -> String {
        match (self.season, self.episode) {
            (Some(s), Some(e)) => format!("S{s:02}E{e:02} · {}", self.name),
            _ => self.name.clone(),
        }
    }
}

/// How the player should open a link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StreamKind {
    Hls,
    Dash,
    Mp4,
    Torrent,
    /// A page to open in a browser (YouTube, Twitch…).
    External,
    #[default]
    Other,
}

impl StreamKind {
    pub fn from_extension(url: &str) -> StreamKind {
        let path = url
            .split(['?', '#'])
            .next()
            .unwrap_or(url)
            .to_ascii_lowercase();
        if path.ends_with(".m3u8") {
            StreamKind::Hls
        } else if path.ends_with(".mpd") {
            StreamKind::Dash
        } else if path.ends_with(".mp4") || path.ends_with(".mkv") || path.ends_with(".webm") {
            StreamKind::Mp4
        } else if url.starts_with("magnet:") {
            StreamKind::Torrent
        } else {
            StreamKind::Other
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Subtitle {
    pub lang: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// One playable source for an item or episode.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StreamLink {
    pub url: String,
    /// The source/server name shown to the user ("Server 2", "Vidhide", …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referer: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub kind: StreamKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subtitles: Vec<Subtitle>,
}

impl StreamLink {
    pub fn new(url: impl Into<String>) -> Self {
        let url = url.into();
        StreamLink {
            kind: StreamKind::from_extension(&url),
            url,
            ..Default::default()
        }
    }

    pub fn label(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(name) = &self.name {
            parts.push(name.clone());
        }
        if let Some(quality) = &self.quality {
            parts.push(quality.clone());
        }
        if parts.is_empty() {
            self.url.clone()
        } else {
            parts.join(" · ")
        }
    }
}

/// A row of items (a Stremio catalog, a CloudStream home row, a live category…).
#[derive(Debug, Clone, Default)]
pub struct CatalogRow {
    pub title: String,
    pub items: Vec<MediaItem>,
}
