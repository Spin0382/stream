//! CloudStream extension repositories: `repo.json` plus its plugin lists.
//!
//! The format is CloudStream's own. Parsing it here means the Rust side can list,
//! filter and validate a repository without a JVM; downloading and running the
//! `.cs3` files is the host's job.

use crate::error::{Error, Result};
use crate::http;
use serde::{Deserialize, Serialize};

/// A `repo.json`: the entry point of a CloudStream repository.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CloudstreamRepo {
    pub name: String,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub manifest_version: i64,
    /// Urls of the `plugins.json` files that list the extensions.
    pub plugin_lists: Vec<String>,
}

/// One entry of a `plugins.json`, the same fields CloudStream's `SitePlugin` uses.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CloudstreamPlugin {
    /// The `.cs3` url.
    pub url: String,
    /// 0 down, 1 ok, 2 slow, 3 beta only.
    pub status: i64,
    pub version: i64,
    pub api_version: i64,
    pub name: String,
    pub internal_name: String,
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub repository_url: Option<String>,
    pub tv_types: Vec<String>,
    pub language: Option<String>,
    pub icon_url: Option<String>,
    pub file_size: Option<i64>,
    pub file_hash: Option<String>,
    /// Set when the extension is published as plain JVM bytecode instead of `.cs3`.
    pub jar_url: Option<String>,
    pub jar_hash: Option<String>,
}

impl CloudstreamPlugin {
    pub fn status_label(&self) -> &'static str {
        match self.status {
            1 => "ok",
            2 => "lento",
            3 => "beta",
            _ => "caído",
        }
    }

    /// `Dalvik` for a `.cs3`, `JVM` for a published jar.
    pub fn runtime(&self) -> &'static str {
        if self.jar_url.is_some() {
            "JVM"
        } else {
            "Dalvik"
        }
    }
}

/// A repository together with everything it offers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoInspection {
    pub url: String,
    pub repo: CloudstreamRepo,
    pub plugins: Vec<CloudstreamPlugin>,
}

/// Accepts a plain url, a `cloudstreamrepo://host/path` link or a url without `.json`.
pub fn normalize_repo_url(spec: &str) -> String {
    let spec = spec.trim();
    if let Some(rest) = spec.strip_prefix("cloudstreamrepo://") {
        let part = rest.trim_end_matches('/');
        // CloudStream treats the path as the repository host, not as a file.
        if part.ends_with(".json") {
            return format!("https://{part}");
        }
        return format!("https://{part}/repo.json");
    }
    if spec.starts_with("http://") || spec.starts_with("https://") {
        return spec.to_string();
    }
    format!("https://{spec}")
}

pub fn parse_repo(json: &str) -> Result<CloudstreamRepo> {
    let repo: CloudstreamRepo = serde_json::from_str(json)?;
    if repo.name.trim().is_empty() {
        return Err(Error::Missing("repo.json without a name".to_string()));
    }
    if repo.plugin_lists.is_empty() {
        return Err(Error::Missing("repo.json without plugin lists".to_string()));
    }
    Ok(repo)
}

pub fn parse_plugins(json: &str) -> Result<Vec<CloudstreamPlugin>> {
    let plugins: Vec<CloudstreamPlugin> = serde_json::from_str(json)?;
    Ok(plugins)
}

/// Downloads a repository and every plugin list it points at.
pub async fn inspect(spec: &str) -> Result<RepoInspection> {
    let url = normalize_repo_url(spec);
    let repo = parse_repo(&http::fetch_text(&url).await?)?;
    let mut plugins = Vec::new();
    for list in &repo.plugin_lists {
        let raw = http::fetch_text(list).await?;
        plugins.extend(parse_plugins(&raw)?);
    }
    plugins.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(RepoInspection {
        url,
        repo,
        plugins,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPO: &str = r#"{
        "name": "Storm-ext Fork by redblacker8",
        "description": "A fork.",
        "iconUrl": "https://example.com/icon.jpg",
        "manifestVersion": 1,
        "pluginLists": ["https://example.com/builds/plugins.json"]
    }"#;

    const PLUGINS: &str = r#"[
        {
            "url": "https://example.com/builds/Cine.cs3",
            "status": 1,
            "version": 7,
            "apiVersion": 1,
            "name": "Cine",
            "internalName": "CineProvider",
            "authors": ["someone"],
            "description": "Películas",
            "language": "es",
            "tvTypes": ["Movie"]
        },
        {
            "url": "https://example.com/builds/Jvm.jar",
            "jarUrl": "https://example.com/builds/Jvm.jar",
            "status": 0,
            "version": 2,
            "name": "Jvm",
            "internalName": "JvmProvider"
        }
    ]"#;

    #[test]
    fn parses_a_cloudstream_repository() {
        let repo = parse_repo(REPO).unwrap();
        assert_eq!(repo.name, "Storm-ext Fork by redblacker8");
        assert_eq!(repo.plugin_lists.len(), 1);
        assert_eq!(repo.manifest_version, 1);
        assert!(repo.icon_url.is_some());
    }

    #[test]
    fn rejects_a_repository_without_plugin_lists() {
        let error = parse_repo(r#"{"name":"x","pluginLists":[]}"#).unwrap_err();
        assert!(matches!(error, Error::Missing(_)));
    }

    #[test]
    fn parses_plugins_and_their_runtime() {
        let plugins = parse_plugins(PLUGINS).unwrap();
        assert_eq!(plugins.len(), 2);
        assert_eq!(plugins[0].internal_name, "CineProvider");
        assert_eq!(plugins[0].runtime(), "Dalvik");
        assert_eq!(plugins[0].status_label(), "ok");
        assert_eq!(plugins[0].tv_types, vec!["Movie".to_string()]);
        assert_eq!(plugins[1].runtime(), "JVM");
        assert_eq!(plugins[1].status_label(), "caído");
    }

    #[test]
    fn normalizes_repository_links() {
        assert_eq!(
            normalize_repo_url("cloudstreamrepo://example.com/repo.json"),
            "https://example.com/repo.json"
        );
        assert_eq!(
            normalize_repo_url("cloudstreamrepo://example.com/repo"),
            "https://example.com/repo.json"
        );
        assert_eq!(
            normalize_repo_url("example.com/repo.json"),
            "https://example.com/repo.json"
        );
        assert_eq!(
            normalize_repo_url("https://example.com/repo.json"),
            "https://example.com/repo.json"
        );
    }
}
