//! M3U/M3U8 playlist parsing for Live TV.
//!
//! Hand written on purpose: playlists in the wild are loose about quoting, and a
//! tolerant scanner is shorter (and easier to test) than wiring an XML-ish parser
//! to a format that is not XML.

use crate::model::{MediaItem, MediaKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One channel from a playlist.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Channel {
    pub name: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tvg_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Extra headers from `#EXTVLCOPT` lines (`user-agent`, `referrer`…).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    /// Every `key="value"` of the `#EXTINF` line, lowercased keys.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attrs: BTreeMap<String, String>,
}

impl Channel {
    pub fn to_media_item(&self) -> MediaItem {
        let mut item = MediaItem::new(&self.name, &self.url);
        item.poster = self.logo.clone().filter(|l| !l.is_empty());
        item.kind = MediaKind::Live;
        item.provider = self.group.clone();
        item.source = Some("live".to_string());
        item
    }
}

#[derive(Debug, Default)]
struct Pending {
    name: String,
    attrs: BTreeMap<String, String>,
    headers: BTreeMap<String, String>,
    group: Option<String>,
}

/// Parses a playlist into channels. Anything that is not `#EXTINF`-annotated
/// (bare urls) is kept too, named after the url.
pub fn parse_playlist(text: &str) -> Vec<Channel> {
    let mut channels = Vec::new();
    let mut pending: Option<Pending> = None;

    for raw in text.lines() {
        let line = raw.trim().trim_start_matches('\u{feff}');
        if line.is_empty() {
            continue;
        }
        if line.starts_with('#') {
            if let Some(rest) = line.strip_prefix("#EXTINF:") {
                let (attrs, title) = split_extinf(rest);
                pending = Some(Pending {
                    name: title,
                    attrs,
                    ..Default::default()
                });
            } else if let Some(rest) = line.strip_prefix("#EXTGRP:") {
                if let Some(pending) = pending.as_mut() {
                    pending.group = Some(rest.trim().to_string());
                }
            } else if let Some(rest) = line.strip_prefix("#EXTVLCOPT:") {
                if let Some((key, value)) = rest.trim().split_once('=') {
                    let key = key.trim().to_ascii_lowercase();
                    let key = match key.as_str() {
                        "http-user-agent" => "user-agent".to_string(),
                        "http-referrer" | "http-referer" => "referer".to_string(),
                        other => other.to_string(),
                    };
                    if let Some(pending) = pending.as_mut() {
                        pending.headers.insert(key, value.trim().to_string());
                    }
                }
            }
            continue;
        }

        let channel = match pending.take() {
            Some(pending) => channel_from(pending, line),
            None => Channel {
                name: line.to_string(),
                url: line.to_string(),
                ..Default::default()
            },
        };
        if !channel.url.is_empty() {
            channels.push(channel);
        }
    }

    channels
}

fn channel_from(pending: Pending, url: &str) -> Channel {
    let attr = |key: &str| {
        pending
            .attrs
            .get(key)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let group = attr("group-title")
        .or_else(|| pending.group.clone())
        .or_else(|| attr("group"));
    let name = if pending.name.is_empty() {
        url.to_string()
    } else {
        pending.name
    };
    Channel {
        name,
        url: url.to_string(),
        tvg_id: attr("tvg-id"),
        logo: attr("tvg-logo").or_else(|| attr("logo")),
        group,
        headers: pending.headers,
        attrs: pending.attrs,
    }
}

/// Splits `tvg-id="x" group-title="a, b",Channel name` into attributes and title.
fn split_extinf(rest: &str) -> (BTreeMap<String, String>, String) {
    let chars: Vec<char> = rest.chars().collect();
    let mut i = 0;
    let mut in_quotes = false;
    let mut split_at = None;
    while i < chars.len() {
        match chars[i] {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                split_at = Some(i);
                break;
            }
            _ => {}
        }
        i += 1;
    }

    let (attr_part, title) = match split_at {
        Some(pos) => {
            let attrs: String = chars[..pos].iter().collect();
            let title: String = chars[pos + 1..].iter().collect();
            (attrs, title)
        }
        None => (rest.to_string(), String::new()),
    };

    let mut attrs = BTreeMap::new();
    for (key, value) in parse_attrs(&attr_part) {
        let key = key.to_ascii_lowercase();
        // The first token may be the duration (`-1`); keep it, it is harmless.
        attrs.insert(key, value);
    }
    (attrs, title.trim().to_string())
}

/// `key="value"` and `key=value` pairs, ignoring the bare tokens in between.
fn parse_attrs(input: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = input.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let start = i;
        while i < chars.len() && chars[i] != '=' && !chars[i].is_whitespace() {
            i += 1;
        }
        if start == i {
            i += 1;
            continue;
        }
        let key: String = chars[start..i].iter().collect();
        if i < chars.len() && chars[i] == '=' {
            i += 1;
            let value = if i < chars.len() && chars[i] == '"' {
                i += 1;
                let value_start = i;
                while i < chars.len() && chars[i] != '"' {
                    i += 1;
                }
                let value: String = chars[value_start..i].iter().collect();
                if i < chars.len() {
                    i += 1;
                }
                value
            } else {
                let value_start = i;
                while i < chars.len() && !chars[i].is_whitespace() {
                    i += 1;
                }
                chars[value_start..i].iter().collect()
            };
            out.push((key, value));
        } else {
            out.push((key, String::new()));
        }
    }
    out
}

/// Groups channels by `group-title`, preserving playlist order inside each group.
pub fn group_channels(channels: &[Channel]) -> BTreeMap<String, Vec<&Channel>> {
    let mut out: BTreeMap<String, Vec<&Channel>> = BTreeMap::new();
    for channel in channels {
        let group = channel
            .group
            .clone()
            .unwrap_or_else(|| "Sin grupo".to_string());
        out.entry(group).or_default().push(channel);
    }
    out
}

/// Case-insensitive name/id search, for the Live TV search box.
pub fn filter_channels<'a>(channels: &'a [Channel], query: &str) -> Vec<&'a Channel> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return channels.iter().collect();
    }
    channels
        .iter()
        .filter(|c| {
            c.name.to_ascii_lowercase().contains(&query)
                || c.group
                    .as_deref()
                    .map(|g| g.to_ascii_lowercase().contains(&query))
                    .unwrap_or(false)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"#EXTM3U
#EXTINF:-1 tvg-id="tve.es" tvg-logo="https://example.com/tve.png" group-title="España",La 1 HD
#EXTVLCOPT:http-user-agent=Mozilla/5.0
#EXTVLCOPT:http-referrer=https://example.com/
https://example.com/la1.m3u8
#EXTINF:-1 group-title="Series, Anime" tvg-id="x",Un, dos, tres
https://example.com/unos.m3u8
#EXTGRP:Pelis
#EXTINF:-1,Peli suelta
https://example.com/peli.mp4
https://example.com/bare.m3u8
"#;

    #[test]
    fn parses_channels_with_attributes_and_headers() {
        let channels = parse_playlist(SAMPLE);
        assert_eq!(channels.len(), 4);

        let first = &channels[0];
        assert_eq!(first.name, "La 1 HD");
        assert_eq!(first.tvg_id.as_deref(), Some("tve.es"));
        assert_eq!(first.logo.as_deref(), Some("https://example.com/tve.png"));
        assert_eq!(first.group.as_deref(), Some("España"));
        assert_eq!(
            first.headers.get("user-agent").map(String::as_str),
            Some("Mozilla/5.0")
        );
        assert_eq!(
            first.headers.get("referer").map(String::as_str),
            Some("https://example.com/")
        );
        assert_eq!(first.url, "https://example.com/la1.m3u8");

        // A comma inside a quoted attribute must not split the title.
        assert_eq!(channels[1].name, "Un, dos, tres");
        assert_eq!(channels[1].group.as_deref(), Some("Series, Anime"));

        // #EXTGRP is used when there is no group-title.
        assert_eq!(channels[2].group.as_deref(), Some("Pelis"));

        // A bare url still becomes a channel.
        assert_eq!(channels[3].name, "https://example.com/bare.m3u8");
    }

    #[test]
    fn groups_and_filters() {
        let channels = parse_playlist(SAMPLE);
        let groups = group_channels(&channels);
        assert!(groups.contains_key("España"));
        assert_eq!(groups["España"].len(), 1);

        let hits = filter_channels(&channels, "la 1");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].url, "https://example.com/la1.m3u8");
    }
}
