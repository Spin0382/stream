//! Live TV: M3U playlists, Xtream Codes panels and XMLTV guides.

pub mod m3u;
pub mod xmltv;
pub mod xtream;

use crate::error::Result;
use crate::http::read_url_or_file;
use serde::{Deserialize, Serialize};

pub use m3u::{filter_channels, group_channels, parse_playlist, Channel};
pub use xmltv::{parse_xmltv, Guide, GuideChannel, Programme};
pub use xtream::{XtreamAccount, XtreamCategory, XtreamClient};

/// Where a Live TV section's channels come from.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LiveSource {
    /// An M3U/M3U8 playlist: a url or a local file.
    M3u { name: String, spec: String },
    /// An Xtream Codes login.
    Xtream {
        name: String,
        base: String,
        username: String,
        password: String,
    },
}

impl LiveSource {
    pub fn name(&self) -> &str {
        match self {
            LiveSource::M3u { name, .. } => name,
            LiveSource::Xtream { name, .. } => name,
        }
    }

    pub fn id(&self) -> String {
        match self {
            LiveSource::M3u { spec, .. } => format!("m3u:{spec}"),
            LiveSource::Xtream { base, username, .. } => format!("xtream:{base}:{username}"),
        }
    }

    pub async fn channels(&self) -> Result<Vec<Channel>> {
        match self {
            LiveSource::M3u { spec, .. } => {
                let text = read_url_or_file(spec).await?;
                Ok(parse_playlist(&text))
            }
            LiveSource::Xtream {
                base,
                username,
                password,
                ..
            } => {
                XtreamClient::new(base.clone(), username.clone(), password.clone())
                    .live_streams(None)
                    .await
            }
        }
    }

    /// An XMLTV guide for this source, when it has one.
    pub async fn guide(&self) -> Result<Option<Guide>> {
        match self {
            LiveSource::M3u { .. } => Ok(None),
            LiveSource::Xtream {
                base,
                username,
                password,
                ..
            } => {
                let client = XtreamClient::new(base.clone(), username.clone(), password.clone());
                let text = read_url_or_file(&client.xmltv_url()).await?;
                Ok(Some(parse_xmltv(&text)))
            }
        }
    }
}

/// Loads a guide from any url/path (used for the standalone EPG setting).
pub async fn load_guide(spec: &str) -> Result<Guide> {
    let text = read_url_or_file(spec).await?;
    Ok(parse_xmltv(&text))
}
