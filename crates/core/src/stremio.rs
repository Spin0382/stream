//! Stremio add-on client.
//!
//! Protocol: `<base>/manifest.json`, `<base>/catalog/{type}/{id}[/{extra}].json`,
//! `<base>/meta/{type}/{id}.json`, `<base>/stream/{type}/{id}.json`.
//! Add-ons are the easiest part of the app to support natively: plain HTTP + JSON.

use crate::error::{Error, Result};
use crate::model::{Episode, MediaItem, MediaKind, StreamKind, StreamLink, Subtitle};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub logo: Option<String>,
    #[serde(default, rename = "types")]
    pub types: Vec<String>,
    /// Either `"catalog"` or `{"name": "catalog", "types": [...], "idPrefixes": [...]}`.
    #[serde(default)]
    pub resources: Vec<Value>,
    #[serde(default)]
    pub catalogs: Vec<Catalog>,
    #[serde(default, rename = "idPrefixes")]
    pub id_prefixes: Vec<String>,
    #[serde(default, rename = "behaviorHints")]
    pub behavior_hints: Value,
}

impl Manifest {
    /// Does this add-on expose the resource for the given media type?
    pub fn has_resource(&self, resource: &str, kind: &str) -> bool {
        self.resources.iter().any(|entry| match entry {
            Value::String(name) => name == resource,
            Value::Object(map) => {
                let name_matches = map
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(|name| name == resource)
                    .unwrap_or(false);
                if !name_matches {
                    return false;
                }
                match map.get("types").and_then(|v| v.as_array()) {
                    Some(types) => types
                        .iter()
                        .filter_map(|t| t.as_str())
                        .any(|t| t == kind || t == "other"),
                    None => true,
                }
            }
            _ => false,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub extra: Vec<Value>,
    #[serde(default, rename = "extraRequired")]
    pub extra_required: Vec<String>,
}

impl Catalog {
    pub fn title(&self) -> String {
        self.name
            .clone()
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| format!("{} · {}", self.kind, self.id))
    }

    /// True when the catalog supports the search box (`extra=search=`).
    pub fn supports_search(&self) -> bool {
        self.extra.iter().any(|entry| match entry {
            Value::String(text) => text == "search",
            Value::Object(map) => map
                .get("name")
                .and_then(|v| v.as_str())
                .map(|name| name == "search")
                .unwrap_or(false),
            _ => false,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Meta {
    pub id: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub poster: Option<String>,
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default)]
    pub logo: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, rename = "releaseInfo")]
    pub release_info: Option<String>,
    #[serde(default, rename = "imdbRating")]
    pub imdb_rating: Option<Value>,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub videos: Vec<Video>,
    #[serde(default)]
    pub runtime: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Video {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub season: Option<i32>,
    #[serde(default)]
    pub episode: Option<i32>,
    #[serde(default)]
    pub overview: Option<String>,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub released: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StreamObject {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default, rename = "externalUrl")]
    pub external_url: Option<String>,
    #[serde(default, rename = "infoHash")]
    pub info_hash: Option<String>,
    #[serde(default, rename = "fileIdx")]
    pub file_idx: Option<i64>,
    #[serde(default, rename = "ytId")]
    pub yt_id: Option<String>,
    #[serde(default)]
    pub subtitles: Vec<StremioSubtitle>,
    #[serde(default, rename = "behaviorHints")]
    pub behavior_hints: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StremioSubtitle {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub lang: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct CatalogResponse {
    #[serde(default)]
    metas: Vec<Meta>,
}

#[derive(Debug, Clone, Deserialize)]
struct MetaResponse {
    meta: Meta,
}

#[derive(Debug, Clone, Deserialize)]
struct StreamsResponse {
    #[serde(default)]
    streams: Vec<StreamObject>,
}

/// A configured add-on: its base url and the manifest it served.
#[derive(Debug, Clone)]
pub struct AddonClient {
    /// Everything before `/manifest.json`, e.g. `https://v3-cinemeta.strem.io`.
    pub base: String,
    pub manifest: Manifest,
    http: reqwest::Client,
}

impl AddonClient {
    /// Accepts `https://addon.example/manifest.json`, `stremio://addon.example/manifest.json`
    /// or a bare base url.
    pub async fn open(spec: &str) -> Result<Self> {
        Self::open_with(crate::http::client(), spec).await
    }

    pub async fn open_with(http: reqwest::Client, spec: &str) -> Result<Self> {
        let base = normalize_base(spec)?;
        let url = format!("{base}/manifest.json");
        let response = http.get(&url).send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::Status {
                status: status.as_u16(),
                url,
            });
        }
        let manifest: Manifest = response.json().await?;
        Ok(AddonClient {
            base,
            manifest,
            http,
        })
    }

    pub fn catalogs(&self) -> &[Catalog] {
        &self.manifest.catalogs
    }

    pub fn supports_streams(&self) -> bool {
        self.manifest.has_resource("stream", "movie")
            || self.manifest.has_resource("stream", "series")
    }

    pub async fn catalog(
        &self,
        kind: &str,
        id: &str,
        extra: &[(&str, &str)],
    ) -> Result<Vec<MediaItem>> {
        let mut url = format!("{}/catalog/{}/{}.json", self.base, kind, id);
        if !extra.is_empty() {
            let query = extra
                .iter()
                .map(|(key, value)| format!("{}={}", key, urlencode(value)))
                .collect::<Vec<_>>()
                .join("&");
            url = format!("{}/catalog/{}/{}/{}.json", self.base, kind, id, query);
        }
        let response: CatalogResponse = self.get_json(&url).await?;
        Ok(response
            .metas
            .into_iter()
            .map(|meta| self.meta_to_item(meta))
            .collect())
    }

    /// Full details for one item, including the episode list of a series.
    pub async fn meta(&self, kind: &str, id: &str) -> Result<MediaItem> {
        let url = format!("{}/meta/{}/{}.json", self.base, kind, id);
        let response: MetaResponse = self.get_json(&url).await?;
        Ok(self.meta_to_item(response.meta))
    }

    pub async fn streams(&self, kind: &str, id: &str) -> Result<Vec<StreamLink>> {
        let url = format!("{}/stream/{}/{}.json", self.base, kind, id);
        let response: StreamsResponse = self.get_json(&url).await?;
        Ok(response
            .streams
            .into_iter()
            .filter_map(|stream| self.stream_to_link(stream))
            .collect())
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<T> {
        let response = self.http.get(url).send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::Status {
                status: status.as_u16(),
                url: url.to_string(),
            });
        }
        let bytes = response.bytes().await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    fn meta_to_item(&self, meta: Meta) -> MediaItem {
        let kind = MediaKind::from_stremio(&meta.kind);
        let mut item = MediaItem::new(&meta.name, item_url(&self.base, &meta.kind, &meta.id));
        item.poster = meta.poster.clone().or(meta.logo.clone());
        item.background = meta.background.clone();
        item.description = meta.description.clone();
        item.year = meta
            .release_info
            .as_deref()
            .and_then(|info| info.split(['-', '–']).next())
            .and_then(|year| year.trim().parse().ok());
        item.kind = kind;
        item.provider = Some(self.manifest.name.clone());
        item.source = Some("stremio".to_string());
        item.episodes = meta
            .videos
            .iter()
            .filter(|video| video.season.unwrap_or(0) > 0 || video.episode.is_some())
            .map(|video| Episode {
                name: video
                    .title
                    .clone()
                    .unwrap_or_else(|| format!("Episode {}", video.id)),
                url: item_url(&self.base, &meta.kind, &video.id),
                season: video.season,
                episode: video.episode,
                thumbnail: video.thumbnail.clone(),
                description: video.overview.clone(),
            })
            .collect();
        item
    }

    fn stream_to_link(&self, stream: StreamObject) -> Option<StreamLink> {
        let name = stream
            .name
            .clone()
            .or_else(|| stream.title.clone())
            .or_else(|| Some(self.manifest.name.clone()));

        if let Some(url) = stream.url.clone().filter(|url| !url.is_empty()) {
            let mut link = StreamLink::new(url);
            link.name = name;
            link.source = Some(self.manifest.name.clone());
            link.headers = proxy_headers(&stream.behavior_hints);
            link.subtitles = self.map_subtitles(&stream.subtitles);
            if link.headers.contains_key("Referer") && link.referer.is_none() {
                link.referer = link.headers.get("Referer").cloned();
            }
            return Some(link);
        }

        if let Some(magnet) = stream.info_hash.as_ref().map(|hash| {
            let mut magnet = format!("magnet:?xt=urn:btih:{hash}");
            if let Some(idx) = stream.file_idx {
                magnet.push_str(&format!("&ix={idx}"));
            }
            magnet
        }) {
            let mut link = StreamLink::new(magnet);
            link.name = name;
            link.source = Some(self.manifest.name.clone());
            return Some(link);
        }

        if let Some(external) = stream.external_url.clone().or(stream
            .yt_id
            .map(|id| format!("https://www.youtube.com/watch?v={id}")))
        {
            let mut link = StreamLink::new(external);
            link.kind = StreamKind::External;
            link.name = name;
            link.source = Some(self.manifest.name.clone());
            return Some(link);
        }

        None
    }

    fn map_subtitles(&self, subtitles: &[StremioSubtitle]) -> Vec<Subtitle> {
        subtitles
            .iter()
            .filter_map(|subtitle| {
                let url = subtitle.url.clone().filter(|url| !url.is_empty())?;
                Some(Subtitle {
                    lang: subtitle.lang.clone().unwrap_or_else(|| "und".to_string()),
                    url,
                    name: subtitle.id.clone(),
                })
            })
            .collect()
    }
}

fn proxy_headers(hints: &Value) -> BTreeMap<String, String> {
    let mut headers = BTreeMap::new();
    if let Some(request) = hints
        .get("proxyHeaders")
        .and_then(|value| value.get("request"))
        .and_then(|value| value.as_object())
    {
        for (key, value) in request {
            if let Some(text) = value.as_str() {
                headers.insert(key.clone(), text.to_string());
            }
        }
    }
    headers
}

/// `stremio://host/manifest.json` and bare bases become a plain https base.
pub fn normalize_base(spec: &str) -> Result<String> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return Err(Error::Missing("add-on url".to_string()));
    }
    let mut base = trimmed.trim_end_matches('/').to_string();
    if base.starts_with("stremio://") {
        base = format!("https://{}", base.trim_start_matches("stremio://"));
    }
    if let Some(stripped) = base.strip_suffix("/manifest.json") {
        base = stripped.to_string();
    }
    if !base.starts_with("http://") && !base.starts_with("https://") {
        base = format!("https://{base}");
    }
    Ok(base)
}

/// Opaque link stored on a [`MediaItem`]: `stremio:<base>#<type>#<id>`.
pub fn item_url(base: &str, kind: &str, id: &str) -> String {
    format!("stremio:{base}#{kind}#{id}")
}

/// Splits a [`item_url`] back into `(base, type, id)`.
pub fn parse_item_url(url: &str) -> Option<(&str, &str, &str)> {
    let rest = url.strip_prefix("stremio:")?;
    let mut parts = rest.splitn(3, '#');
    let base = parts.next()?;
    let kind = parts.next()?;
    let id = parts.next()?;
    Some((base, kind, id))
}

fn urlencode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(byte))
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"{
        "id": "org.stremio.cinemeta",
        "version": "3.0.14",
        "name": "Cinemeta",
        "types": ["movie", "series"],
        "resources": ["catalog", "meta", "stream"],
        "catalogs": [
            {"type": "movie", "id": "top", "name": "Top"},
            {"type": "series", "id": "search", "name": "Search", "extra": [{"name": "search", "isRequired": true}]}
        ]
    }"#;

    #[test]
    fn parses_manifest_and_resources() {
        let manifest: Manifest = serde_json::from_str(MANIFEST).unwrap();
        assert_eq!(manifest.name, "Cinemeta");
        assert!(manifest.has_resource("stream", "movie"));
        assert!(manifest.has_resource("catalog", "series"));
        assert!(!manifest.has_resource("subtitles", "movie"));
        assert_eq!(manifest.catalogs.len(), 2);
        assert!(manifest.catalogs[1].supports_search());
        assert!(!manifest.catalogs[0].supports_search());
        assert_eq!(manifest.catalogs[0].title(), "Top");
    }

    #[test]
    fn normalizes_addon_urls() {
        assert_eq!(
            normalize_base("https://a.example/manifest.json").unwrap(),
            "https://a.example"
        );
        assert_eq!(
            normalize_base("stremio://a.example/manifest.json").unwrap(),
            "https://a.example"
        );
        assert_eq!(normalize_base("a.example/").unwrap(), "https://a.example");
        assert!(normalize_base("   ").is_err());
    }

    #[test]
    fn round_trips_item_urls() {
        let url = item_url("https://a.example", "series", "tt123:1:2");
        assert_eq!(
            parse_item_url(&url),
            Some(("https://a.example", "series", "tt123:1:2"))
        );
        assert!(parse_item_url("https://nope").is_none());
    }

    #[test]
    fn maps_streams_to_links() {
        let client = AddonClient {
            base: "https://a.example".to_string(),
            manifest: serde_json::from_str(MANIFEST).unwrap(),
            http: crate::http::client(),
        };

        let direct: StreamObject = serde_json::from_str(
            r#"{"name":"1080p","title":"File","url":"https://cdn/x.m3u8",
                "behaviorHints":{"proxyHeaders":{"request":{"Referer":"https://site/"}}},
                "subtitles":[{"id":"es","url":"https://cdn/es.vtt","lang":"es"}]}"#,
        )
        .unwrap();
        let link = client.stream_to_link(direct).unwrap();
        assert_eq!(link.kind, StreamKind::Hls);
        assert_eq!(
            link.headers.get("Referer").map(String::as_str),
            Some("https://site/")
        );
        assert_eq!(link.subtitles.len(), 1);

        let torrent: StreamObject =
            serde_json::from_str(r#"{"infoHash":"ABC","fileIdx":2,"name":"Torrent"}"#).unwrap();
        assert_eq!(
            client.stream_to_link(torrent).unwrap().url,
            "magnet:?xt=urn:btih:ABC&ix=2"
        );

        let external: StreamObject = serde_json::from_str(r#"{"ytId":"dQw4w9WgXcQ"}"#).unwrap();
        let link = client.stream_to_link(external).unwrap();
        assert_eq!(link.kind, StreamKind::External);
        assert_eq!(link.url, "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
    }
}
