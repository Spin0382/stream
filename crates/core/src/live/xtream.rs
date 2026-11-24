//! Xtream Codes panels (the `player_api.php` API used by most IPTV providers).
//!
//! The panel answers with loose JSON where ids are sometimes numbers and sometimes
//! strings and keys are sometimes missing, so everything is read through
//! [`Value`] instead of deriving structs.

use crate::error::{Error, Result};
use crate::live::m3u::Channel;
use crate::model::{MediaItem, MediaKind};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct XtreamClient {
    pub base: String,
    pub username: String,
    pub password: String,
    http: reqwest::Client,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct XtreamAccount {
    pub username: String,
    pub status: String,
    pub expires: Option<i64>,
    pub max_connections: Option<i64>,
    /// The server's own `server_info.url`, used for absolute stream urls.
    pub server_url: Option<String>,
    pub allowed_output_formats: Vec<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct XtreamCategory {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
}

impl XtreamClient {
    /// `base` may be `http://host:8080`, `http://host:8080/`, or a full
    /// `get.php`/`player_api.php` url; the panel root is extracted either way.
    pub fn new(
        base: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        XtreamClient {
            base: normalize_base(&base.into()),
            username: username.into(),
            password: password.into(),
            http: crate::http::client(),
        }
    }

    pub fn with_http(mut self, http: reqwest::Client) -> Self {
        self.http = http;
        self
    }

    async fn api(&self, action: &str, extra: &[(&str, &str)]) -> Result<Value> {
        let mut url = format!(
            "{}/player_api.php?username={}&password={}&action={}",
            self.base,
            urlencode(&self.username),
            urlencode(&self.password),
            action
        );
        for (key, value) in extra {
            url.push('&');
            url.push_str(key);
            url.push('=');
            url.push_str(&urlencode(value));
        }
        let response = self.http.get(&url).send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::Status {
                status: status.as_u16(),
                url,
            });
        }
        Ok(response.json::<Value>().await?)
    }

    pub async fn account_info(&self) -> Result<XtreamAccount> {
        let value = self.api("", &[]).await?;
        let user = value.get("user_info").cloned().unwrap_or(Value::Null);
        let server = value.get("server_info").cloned().unwrap_or(Value::Null);
        Ok(XtreamAccount {
            username: user
                .get("username")
                .and_then(json_string)
                .unwrap_or_else(|| self.username.clone()),
            status: user.get("status").and_then(json_string).unwrap_or_default(),
            expires: user.get("exp_date").and_then(json_i64),
            max_connections: user.get("max_connections").and_then(json_i64),
            server_url: server.get("url").and_then(json_string),
            allowed_output_formats: server
                .get("allowed_output_formats")
                .and_then(|v| v.as_array())
                .map(|list| list.iter().filter_map(json_string).collect())
                .unwrap_or_default(),
        })
    }

    /// Checks the credentials; returns the account when the panel accepts them.
    pub async fn login(&self) -> Result<XtreamAccount> {
        let account = self.account_info().await?;
        if account.status.eq_ignore_ascii_case("active")
            || account.status.eq_ignore_ascii_case("trial")
        {
            Ok(account)
        } else if account.status.is_empty() {
            Err(Error::parse(
                "xtream login",
                "the panel did not return user_info",
            ))
        } else {
            Err(Error::parse(
                "xtream login",
                format!("account status is {}", account.status),
            ))
        }
    }

    pub async fn live_categories(&self) -> Result<Vec<XtreamCategory>> {
        let value = self.api("get_live_categories", &[]).await?;
        Ok(map_categories(&value))
    }

    pub async fn live_streams(&self, category: Option<&str>) -> Result<Vec<Channel>> {
        let extra: Vec<(&str, &str)> = category
            .map(|id| vec![("category_id", id)])
            .unwrap_or_default();
        let value = self.api("get_live_streams", &extra).await?;
        let list = value.as_array().cloned().unwrap_or_default();
        Ok(list
            .iter()
            .filter_map(|entry| self.map_live_stream(entry))
            .collect())
    }

    pub async fn vod_streams(&self, category: Option<&str>) -> Result<Vec<MediaItem>> {
        let extra: Vec<(&str, &str)> = category
            .map(|id| vec![("category_id", id)])
            .unwrap_or_default();
        let value = self.api("get_vod_streams", &extra).await?;
        let list = value.as_array().cloned().unwrap_or_default();
        Ok(list
            .iter()
            .filter_map(|entry| self.map_vod(entry))
            .collect())
    }

    pub async fn series(&self, category: Option<&str>) -> Result<Vec<MediaItem>> {
        let extra: Vec<(&str, &str)> = category
            .map(|id| vec![("category_id", id)])
            .unwrap_or_default();
        let value = self.api("get_series", &extra).await?;
        let list = value.as_array().cloned().unwrap_or_default();
        Ok(list
            .iter()
            .filter_map(|entry| self.map_series(entry))
            .collect())
    }

    pub fn stream_url(&self, stream_id: &str) -> String {
        let host = self.base.trim_end_matches('/');
        format!(
            "{host}/live/{}/{}/{stream_id}.m3u8",
            self.username, self.password
        )
    }

    /// The panel's full XMLTV document, when it has one.
    pub fn xmltv_url(&self) -> String {
        format!(
            "{}/xmltv.php?username={}&password={}",
            self.base.trim_end_matches('/'),
            urlencode(&self.username),
            urlencode(&self.password)
        )
    }

    fn map_live_stream(&self, entry: &Value) -> Option<Channel> {
        let stream_id = entry.get("stream_id").and_then(json_string)?;
        let name = entry
            .get("name")
            .and_then(json_string)
            .unwrap_or_else(|| stream_id.clone());
        let mut attrs = BTreeMap::new();
        if let Some(id) = entry.get("stream_id").and_then(json_string) {
            attrs.insert("stream_id".to_string(), id);
        }
        if let Some(num) = entry.get("num").and_then(json_string) {
            attrs.insert("num".to_string(), num);
        }
        if let Some(archive) = entry.get("tv_archive").and_then(json_i64) {
            attrs.insert("tv_archive".to_string(), archive.to_string());
        }
        Some(Channel {
            name,
            url: self.stream_url(&stream_id),
            tvg_id: entry.get("epg_channel_id").and_then(json_string),
            logo: entry.get("stream_icon").and_then(json_string),
            group: entry.get("category_name").and_then(json_string),
            headers: BTreeMap::new(),
            attrs,
        })
    }

    fn map_vod(&self, entry: &Value) -> Option<MediaItem> {
        let stream_id = entry.get("stream_id").and_then(json_string)?;
        let mut item = MediaItem::new(
            entry
                .get("name")
                .and_then(json_string)
                .unwrap_or_else(|| stream_id.clone()),
            format!(
                "{}/movie/{}/{}/{}.{}",
                self.base.trim_end_matches('/'),
                self.username,
                self.password,
                stream_id,
                entry
                    .get("container_extension")
                    .and_then(json_string)
                    .unwrap_or_else(|| "mp4".into())
            ),
        );
        item.poster = entry.get("stream_icon").and_then(json_string);
        item.kind = MediaKind::Movie;
        item.provider = entry.get("category_name").and_then(json_string);
        item.source = Some("xtream".to_string());
        item.year = entry
            .get("year")
            .and_then(json_string)
            .and_then(|y| y.parse().ok());
        item.description = entry.get("plot").and_then(json_string);
        Some(item)
    }

    fn map_series(&self, entry: &Value) -> Option<MediaItem> {
        let series_id = entry.get("series_id").and_then(json_string)?;
        let mut item = MediaItem::new(
            entry
                .get("name")
                .and_then(json_string)
                .unwrap_or_else(|| series_id.clone()),
            format!("xtream:series:{series_id}"),
        );
        item.poster = entry.get("cover").and_then(json_string);
        item.kind = MediaKind::Series;
        item.provider = entry.get("category_name").and_then(json_string);
        item.source = Some("xtream".to_string());
        item.description = entry.get("plot").and_then(json_string);
        Some(item)
    }
}

pub fn normalize_base(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    // Strip a full panel url down to its root.
    let mut base = trimmed
        .split("/player_api.php")
        .next()
        .unwrap_or(trimmed)
        .split("/get.php")
        .next()
        .unwrap_or(trimmed)
        .trim_end_matches('/')
        .to_string();
    if !base.starts_with("http://") && !base.starts_with("https://") {
        base = format!("http://{base}");
    }
    base
}

pub fn map_categories(value: &Value) -> Vec<XtreamCategory> {
    value
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|entry| {
                    let id = entry.get("category_id").and_then(json_string)?;
                    Some(XtreamCategory {
                        name: entry
                            .get("category_name")
                            .and_then(json_string)
                            .unwrap_or_else(|| id.clone()),
                        parent_id: entry.get("parent_id").and_then(json_string),
                        id,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn json_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            if text.trim().is_empty() {
                None
            } else {
                Some(text.trim().to_string())
            }
        }
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn json_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

/// Percent-encoding for query values (user and password are often odd).
fn urlencode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_panel_urls() {
        assert_eq!(normalize_base("http://host:8080/"), "http://host:8080");
        assert_eq!(
            normalize_base("http://host:8080/get.php?username=a&password=b&type=m3u_plus"),
            "http://host:8080"
        );
        assert_eq!(
            normalize_base("https://host/player_api.php?username=a"),
            "https://host"
        );
        assert_eq!(normalize_base("host:8080"), "http://host:8080");
    }

    #[test]
    fn maps_live_streams_to_channels() {
        let client = XtreamClient::new("http://panel.example:8080", "user", "pass");
        let entry = json!({
            "num": 1,
            "name": "La 1 HD",
            "stream_type": "live",
            "stream_id": 42,
            "stream_icon": "https://img/la1.png",
            "epg_channel_id": "tve.es",
            "category_name": "España",
            "tv_archive": 1
        });
        let channel = client.map_live_stream(&entry).unwrap();
        assert_eq!(channel.name, "La 1 HD");
        assert_eq!(channel.tvg_id.as_deref(), Some("tve.es"));
        assert_eq!(channel.logo.as_deref(), Some("https://img/la1.png"));
        assert_eq!(channel.group.as_deref(), Some("España"));
        assert_eq!(
            channel.url,
            "http://panel.example:8080/live/user/pass/42.m3u8"
        );
        assert_eq!(
            channel.attrs.get("tv_archive").map(String::as_str),
            Some("1")
        );
    }

    #[test]
    fn maps_categories() {
        let value = json!([
            {"category_id": "1", "category_name": "Deportes", "parent_id": 0},
            {"category_id": 2, "category_name": "Pelis"}
        ]);
        let categories = map_categories(&value);
        assert_eq!(categories.len(), 2);
        assert_eq!(categories[0].name, "Deportes");
        assert_eq!(categories[1].id, "2");
    }

    #[test]
    fn builds_xmltv_url() {
        let client = XtreamClient::new("http://panel", "u", "p");
        assert_eq!(
            client.xmltv_url(),
            "http://panel/xmltv.php?username=u&password=p"
        );
    }
}
