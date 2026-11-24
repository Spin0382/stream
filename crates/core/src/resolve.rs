//! One place that knows where an item came from and how to get its links.
//!
//! The UI and the CLI both call these two functions instead of branching on
//! `item.source` themselves.

use crate::error::{Error, Result};
use crate::host::HostClient;
use crate::model::{MediaItem, StreamLink};
use crate::stremio::{self, AddonClient};

/// Full details (episode list for series) of an item.
pub async fn details(host: Option<&HostClient>, item: &MediaItem) -> Result<MediaItem> {
    match item.source.as_deref() {
        Some("host") => {
            let host = host.ok_or_else(|| Error::Missing("extension host not running".into()))?;
            let provider = provider_of(item)?;
            host.load(provider, &item.url).await
        }
        Some("stremio") => {
            let (base, kind, id) = stremio::parse_item_url(&item.url)
                .ok_or_else(|| Error::parse("stremio item url", item.url.clone()))?;
            let addon = AddonClient::open(base).await?;
            addon.meta(kind, id).await
        }
        // Live channels and Xtream items are already complete.
        _ => Ok(item.clone()),
    }
}

/// Every playable link for an item or episode.
pub async fn links(host: Option<&HostClient>, item: &MediaItem) -> Result<Vec<StreamLink>> {
    match item.source.as_deref() {
        Some("host") => {
            let host = host.ok_or_else(|| Error::Missing("extension host not running".into()))?;
            let provider = provider_of(item)?;
            host.links(provider, &item.url).await
        }
        Some("stremio") => {
            let (base, kind, id) = stremio::parse_item_url(&item.url)
                .ok_or_else(|| Error::parse("stremio item url", item.url.clone()))?;
            let addon = AddonClient::open(base).await?;
            addon.streams(kind, id).await
        }
        Some("live") | Some("xtream") => Ok(vec![StreamLink::new(&item.url)]),
        _ => Err(Error::parse(
            "item source",
            format!("don't know how to play {:?}", item.source),
        )),
    }
}

fn provider_of(item: &MediaItem) -> Result<&str> {
    item.provider
        .as_deref()
        .ok_or_else(|| Error::Missing(format!("provider for {}", item.name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn live_items_play_directly() {
        let mut item = MediaItem::new("La 1", "https://cdn/la1.m3u8");
        item.source = Some("live".to_string());
        let links = links(None, &item).await.unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, "https://cdn/la1.m3u8");
    }

    #[tokio::test]
    async fn unknown_sources_are_rejected() {
        let item = MediaItem::new("Mystery", "https://cdn/x");
        assert!(links(None, &item).await.is_err());
    }
}
