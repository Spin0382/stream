//! Shared HTTP helpers: one client for the whole app, tolerant text fetching.

use crate::error::{Error, Result};
use std::time::Duration;

pub const USER_AGENT: &str = concat!(
    "StreamGoat/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/Spin0382/stream)"
);

/// The app-wide client: rustls, gzip/brotli, sane timeouts, our user agent.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .unwrap_or_default()
}

pub fn is_url(spec: &str) -> bool {
    let spec = spec.trim();
    spec.starts_with("http://") || spec.starts_with("https://")
}

/// Reads a spec that may be an http(s) url or a local path.
pub async fn read_url_or_file(spec: &str) -> Result<String> {
    if is_url(spec) {
        fetch_text(spec).await
    } else {
        let path = std::path::Path::new(spec);
        if !path.exists() {
            return Err(Error::Missing(format!(
                "{spec} is neither a url nor a file"
            )));
        }
        Ok(tokio::fs::read_to_string(path).await?)
    }
}

pub async fn fetch_text(url: &str) -> Result<String> {
    let response = client().get(url).send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(Error::Status {
            status: status.as_u16(),
            url: url.to_string(),
        });
    }
    Ok(response.text().await?)
}

pub async fn fetch_bytes(url: &str) -> Result<Vec<u8>> {
    let response = client().get(url).send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(Error::Status {
            status: status.as_u16(),
            url: url.to_string(),
        });
    }
    Ok(response.bytes().await?.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_urls() {
        assert!(is_url("https://example.com/x.m3u"));
        assert!(!is_url("/home/user/list.m3u"));
        assert!(!is_url("ftp://example.com/x"));
    }
}
