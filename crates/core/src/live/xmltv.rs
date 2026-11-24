//! XMLTV (EPG) parsing.
//!
//! The scanner is deliberately small and forgiving: it understands tags, quoted
//! attributes, comments and CDATA, which is everything XMLTV uses. No XML crate,
//! because the subset is tiny and the tight loop matters for 100 MB guides.

use chrono::{DateTime, NaiveDateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GuideChannel {
    pub id: String,
    #[serde(default)]
    pub names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

impl GuideChannel {
    pub fn name(&self) -> &str {
        self.names.first().map(String::as_str).unwrap_or(&self.id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Programme {
    pub channel: String,
    pub start: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<DateTime<Utc>>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

impl Programme {
    /// `20:15` – `21:45` in the guide's own timezone-free UTC frame; the UI formats it.
    pub fn time_label(&self) -> String {
        match self.stop {
            Some(stop) => format!("{} - {}", fmt_hm(self.start), fmt_hm(stop)),
            None => fmt_hm(self.start),
        }
    }
}

fn fmt_hm(t: DateTime<Utc>) -> String {
    format!("{:02}:{:02}", t.hour(), t.minute())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Guide {
    #[serde(default)]
    pub channels: BTreeMap<String, GuideChannel>,
    #[serde(default)]
    pub programmes: Vec<Programme>,
}

impl Guide {
    pub fn channel_name(&self, id: &str) -> String {
        self.channels
            .get(id)
            .map(|c| c.name().to_string())
            .unwrap_or_else(|| id.to_string())
    }

    /// Programmes of one channel, in time order.
    pub fn on_channel<'a>(&'a self, channel: &'a str) -> impl Iterator<Item = &'a Programme> + 'a {
        self.programmes.iter().filter(move |p| p.channel == channel)
    }

    /// What is on right now (or the next programme starting soon when there is a gap).
    pub fn now_on(&self, channel: &str, at: DateTime<Utc>) -> Option<&Programme> {
        let mut upcoming: Option<&Programme> = None;
        for programme in self.programmes.iter().filter(|p| p.channel == channel) {
            let ends_after_start = programme.stop.map(|stop| stop > at).unwrap_or(true);
            if programme.start <= at && ends_after_start {
                return Some(programme);
            }
            if programme.start > at {
                upcoming = Some(programme);
                break;
            }
        }
        upcoming
    }

    pub fn is_empty(&self) -> bool {
        self.programmes.is_empty() && self.channels.is_empty()
    }
}

/// Parses an XMLTV document.
pub fn parse_xmltv(text: &str) -> Guide {
    let mut guide = Guide::default();
    let mut channel: Option<GuideChannel> = None;
    let mut programme: Option<ProgrammeBuilder> = None;
    let mut text_target = TextTarget::None;

    for token in tokenize(text) {
        match token {
            Token::Start(tag) => match tag.name {
                "channel" => {
                    channel = Some(GuideChannel {
                        id: tag.attr("id").unwrap_or_default().to_string(),
                        ..Default::default()
                    });
                    text_target = TextTarget::None;
                }
                "display-name" => text_target = TextTarget::DisplayName,
                "programme" => {
                    programme = Some(ProgrammeBuilder {
                        channel: tag.attr("channel").unwrap_or_default().to_string(),
                        start: tag.attr("start").and_then(parse_time),
                        stop: tag.attr("stop").and_then(parse_time),
                        ..Default::default()
                    });
                    text_target = TextTarget::None;
                }
                "title" => text_target = TextTarget::Title,
                "sub-title" => text_target = TextTarget::SubTitle,
                "desc" => text_target = TextTarget::Description,
                "category" => text_target = TextTarget::Category,
                "icon" => {
                    if let Some(src) = tag.attr("src") {
                        if let Some(programme) = programme.as_mut() {
                            programme.icon = Some(src.to_string());
                        } else if let Some(channel) = channel.as_mut() {
                            channel.icon = Some(src.to_string());
                        }
                    }
                    text_target = TextTarget::None;
                }
                _ => text_target = TextTarget::None,
            },
            Token::End(name) => match name {
                "channel" => {
                    if let Some(channel) = channel.take() {
                        if !channel.id.is_empty() {
                            guide.channels.insert(channel.id.clone(), channel);
                        }
                    }
                    text_target = TextTarget::None;
                }
                "programme" => {
                    if let Some(builder) = programme.take() {
                        if let Some(programme) = builder.build() {
                            guide.programmes.push(programme);
                        }
                    }
                    text_target = TextTarget::None;
                }
                "display-name" | "title" | "sub-title" | "desc" | "category" => {
                    text_target = TextTarget::None;
                }
                _ => {}
            },
            Token::Text(text) => {
                let text = text.trim();
                if text.is_empty() {
                    continue;
                }
                match text_target {
                    TextTarget::DisplayName => {
                        if let Some(channel) = channel.as_mut() {
                            channel.names.push(text.to_string());
                        }
                    }
                    TextTarget::Title => {
                        if let Some(programme) = programme.as_mut() {
                            if programme.title.is_empty() {
                                programme.title = text.to_string();
                            } else {
                                programme.title.push(' ');
                                programme.title.push_str(text);
                            }
                        }
                    }
                    TextTarget::SubTitle => {
                        if let Some(programme) = programme.as_mut() {
                            programme.sub_title = Some(text.to_string());
                        }
                    }
                    TextTarget::Description => {
                        if let Some(programme) = programme.as_mut() {
                            let slot = programme.description.get_or_insert_with(String::new);
                            if !slot.is_empty() {
                                slot.push(' ');
                            }
                            slot.push_str(text);
                        }
                    }
                    TextTarget::Category => {
                        if let Some(programme) = programme.as_mut() {
                            programme.categories.push(text.to_string());
                        }
                    }
                    TextTarget::None => {}
                }
            }
        }
    }

    guide.programmes.sort_by_key(|p| p.start);
    guide
}

#[derive(Debug, Default)]
struct ProgrammeBuilder {
    channel: String,
    start: Option<DateTime<Utc>>,
    stop: Option<DateTime<Utc>>,
    title: String,
    sub_title: Option<String>,
    description: Option<String>,
    categories: Vec<String>,
    icon: Option<String>,
}

impl ProgrammeBuilder {
    fn build(self) -> Option<Programme> {
        let start = self.start?;
        if self.channel.is_empty() {
            return None;
        }
        Some(Programme {
            channel: self.channel,
            start,
            stop: self.stop,
            title: if self.title.is_empty() {
                "(sin título)".to_string()
            } else {
                self.title
            },
            sub_title: self.sub_title,
            description: self.description,
            categories: self.categories,
            icon: self.icon,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TextTarget {
    None,
    DisplayName,
    Title,
    SubTitle,
    Description,
    Category,
}

/// `20240101120000 +0100`, `20240101120000` (UTC) and `20240101120000 +0000`.
pub fn parse_time(value: &str) -> Option<DateTime<Utc>> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let (stamp, offset) = match value.split_once(' ') {
        Some((stamp, offset)) => (stamp.trim(), offset.trim()),
        None => (value, ""),
    };
    let naive = NaiveDateTime::parse_from_str(stamp, "%Y%m%d%H%M%S")
        .or_else(|_| NaiveDateTime::parse_from_str(stamp, "%Y%m%d%H%M"))
        .ok()?;

    // The stamp is local time at `offset`; UTC is that minus the offset.
    let offset_seconds = parse_offset(offset).unwrap_or(0);
    let utc = naive - chrono::Duration::seconds(offset_seconds as i64);
    Some(utc.and_utc())
}

fn parse_offset(offset: &str) -> Option<i32> {
    if offset.len() < 5 {
        return None;
    }
    let (sign, rest) = offset.split_at(1);
    let sign = match sign {
        "+" => 1,
        "-" => -1,
        _ => return None,
    };
    let (hours, minutes) = rest.split_at(2);
    let hours: i32 = hours.parse().ok()?;
    let minutes: i32 = minutes.get(..2)?.parse().ok()?;
    Some(sign * (hours * 3600 + minutes * 60))
}

// --- tiny XML tokenizer -------------------------------------------------------

#[derive(Debug)]
enum Token<'a> {
    Start(Tag<'a>),
    End(&'a str),
    Text(&'a str),
}

#[derive(Debug)]
struct Tag<'a> {
    name: &'a str,
    attrs: Vec<(&'a str, &'a str)>,
    self_closing: bool,
}

impl<'a> Tag<'a> {
    fn attr(&self, key: &str) -> Option<&'a str> {
        self.attrs
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| *value)
            .filter(|value| !value.is_empty())
    }
}

fn tokenize(text: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut pos = 0usize;

    while pos < text.len() {
        let Some(rel) = text[pos..].find('<') else {
            push_text(&mut tokens, &text[pos..]);
            break;
        };
        let lt = pos + rel;
        if lt > pos {
            push_text(&mut tokens, &text[pos..lt]);
        }
        let rest = &text[lt..];

        if let Some(after) = rest.strip_prefix("<!--") {
            match after.find("-->") {
                Some(end) => pos = lt + 4 + end + 3,
                None => break,
            }
            continue;
        }
        if let Some(after) = rest.strip_prefix("<![CDATA[") {
            match after.find("]]>") {
                Some(end) => {
                    push_text(&mut tokens, &after[..end]);
                    pos = lt + 9 + end + 3;
                    continue;
                }
                None => break,
            }
        }
        if rest.starts_with("<?") {
            match rest.find("?>") {
                Some(end) => pos = lt + end + 2,
                None => break,
            }
            continue;
        }
        if rest.starts_with("<!") {
            match rest.find('>') {
                Some(end) => pos = lt + end + 1,
                None => break,
            }
            continue;
        }

        // Find the closing '>', ignoring any inside quoted attribute values.
        let bytes = rest.as_bytes();
        let mut index = 1usize;
        let mut in_quotes = false;
        while index < bytes.len() {
            match bytes[index] {
                b'"' => in_quotes = !in_quotes,
                b'>' if !in_quotes => break,
                _ => {}
            }
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }

        let inner = &rest[1..index];
        pos = lt + index + 1;

        if let Some(name) = inner.strip_prefix('/') {
            tokens.push(Token::End(name.trim()));
        } else {
            tokens.push(Token::Start(parse_tag(inner)));
        }
    }

    tokens
}

fn push_text<'a>(tokens: &mut Vec<Token<'a>>, text: &'a str) {
    if !text.trim().is_empty() {
        tokens.push(Token::Text(text));
    }
}

fn parse_tag(inner: &str) -> Tag<'_> {
    let trimmed = inner.trim();
    let self_closing = trimmed.ends_with('/');
    let trimmed = trimmed.trim_end_matches('/');
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or("").trim();
    let rest = parts.next().unwrap_or("");
    Tag {
        name,
        attrs: parse_attrs(rest),
        self_closing,
    }
}

fn parse_attrs(input: &str) -> Vec<(&str, &str)> {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i] as char).is_whitespace() {
            i += 1;
        }
        let key_start = i;
        while i < bytes.len() && bytes[i] != b'=' && !(bytes[i] as char).is_whitespace() {
            i += 1;
        }
        if key_start == i {
            i += 1;
            continue;
        }
        let key = input[key_start..i].trim();
        if i >= bytes.len() || bytes[i] != b'=' {
            continue;
        }
        i += 1;
        if i < bytes.len() && bytes[i] == b'"' {
            i += 1;
            let value_start = i;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            let value = &input[value_start..i.min(input.len())];
            if i < bytes.len() {
                i += 1;
            }
            out.push((key, value));
        } else {
            let value_start = i;
            while i < bytes.len() && !(bytes[i] as char).is_whitespace() {
                i += 1;
            }
            out.push((key, &input[value_start..i]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE tv SYSTEM "xmltv.dtd">
<tv generator-info-name="test">
  <channel id="tve.es">
    <display-name>La 1</display-name>
    <display-name>La Primera</display-name>
    <icon src="https://example.com/la1.png"/>
  </channel>
  <programme start="20240101190000 +0100" stop="20240101210000 +0100" channel="tve.es">
    <title lang="es">Telediario</title>
    <desc lang="es">Noticias &amp; deportes</desc>
    <category>News</category>
  </programme>
  <programme start="20240101210000 +0100" stop="20240101223000 +0100" channel="tve.es">
    <title>Película</title>
    <sub-title>Estreno</sub-title>
    <desc><![CDATA[Una buena <película>]]></desc>
    <icon src="https://example.com/film.png"/>
  </programme>
</tv>
"#;

    #[test]
    fn parses_channels_and_programmes() {
        let guide = parse_xmltv(SAMPLE);
        assert_eq!(guide.channels.len(), 1);
        let channel = &guide.channels["tve.es"];
        assert_eq!(channel.names, vec!["La 1", "La Primera"]);
        assert_eq!(channel.icon.as_deref(), Some("https://example.com/la1.png"));

        assert_eq!(guide.programmes.len(), 2);
        let first = &guide.programmes[0];
        assert_eq!(first.title, "Telediario");
        assert_eq!(first.channel, "tve.es");
        assert_eq!(first.categories, vec!["News"]);
        // 19:00 +0100 is 18:00 UTC.
        assert_eq!(first.start.hour(), 18);
        assert!(first.stop.is_some());

        let second = &guide.programmes[1];
        assert_eq!(second.sub_title.as_deref(), Some("Estreno"));
        assert_eq!(second.description.as_deref(), Some("Una buena <película>"));
        assert_eq!(second.icon.as_deref(), Some("https://example.com/film.png"));
    }

    #[test]
    fn finds_what_is_on_now() {
        let guide = parse_xmltv(SAMPLE);
        let at = parse_time("20240101200000 +0100").unwrap();
        let current = guide.now_on("tve.es", at).unwrap();
        assert_eq!(current.title, "Telediario");

        let later = parse_time("20240101220000 +0100").unwrap();
        assert_eq!(guide.now_on("tve.es", later).unwrap().title, "Película");
        assert!(guide.now_on("otro", later).is_none());
    }

    #[test]
    fn parses_time_formats() {
        assert!(parse_time("20240101120000 +0000").is_some());
        assert!(parse_time("20240101120000").is_some());
        assert!(parse_time("202401011200").is_some());
        assert!(parse_time("").is_none());
        assert!(parse_time("nonsense").is_none());
        let utc = parse_time("20240101120000 +0000").unwrap();
        let plus_two = parse_time("20240101140000 +0200").unwrap();
        assert_eq!(utc, plus_two);
    }
}
