//! Playing things: we hand urls to mpv (the same engine the Kotlin app uses) and
//! pages to the browser. No decoding happens in this process.

use crate::model::{StreamKind, StreamLink, Subtitle};
use std::collections::BTreeMap;
use std::io;
use std::process::{Child, Command};

#[derive(Debug, Clone, Default)]
pub struct PlayRequest {
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub title: Option<String>,
    pub subtitles: Vec<Subtitle>,
    pub fullscreen: bool,
    /// Seconds to start at (resume).
    pub start_at: Option<f64>,
    /// Extra mpv flags from the user's settings.
    pub extra_args: Vec<String>,
}

impl PlayRequest {
    pub fn from_link(link: &StreamLink) -> Self {
        let mut headers = link.headers.clone();
        if let Some(referer) = &link.referer {
            headers
                .entry("Referer".to_string())
                .or_insert_with(|| referer.clone());
        }
        PlayRequest {
            url: link.url.clone(),
            headers,
            title: link.name.clone(),
            subtitles: link.subtitles.clone(),
            fullscreen: true,
            start_at: None,
            extra_args: Vec::new(),
        }
    }
}

/// The argv for mpv. Kept pure so it can be unit tested without a player.
pub fn mpv_args(request: &PlayRequest) -> Vec<String> {
    let mut args: Vec<String> = vec!["--force-window=yes".to_string()];

    if request.fullscreen {
        args.push("--fullscreen".to_string());
    }
    if let Some(title) = request.title.as_ref().filter(|t| !t.is_empty()) {
        args.push(format!("--title={title}"));
    }
    if !request.headers.is_empty() {
        let fields = request
            .headers
            .iter()
            .map(|(key, value)| format!("{key}: {value}"))
            .collect::<Vec<_>>()
            .join(",");
        args.push(format!("--http-header-fields={fields}"));
    }
    for subtitle in &request.subtitles {
        // mpv takes remote subtitles as urls; the language is only a label.
        args.push(format!("--sub-file={}", subtitle.url));
    }
    if let Some(start) = request.start_at {
        if start > 1.0 {
            args.push(format!("--start={start:.0}"));
        }
    }
    args.extend(request.extra_args.iter().cloned());
    args.push("--".to_string());
    args.push(request.url.clone());
    args
}

/// Spawns mpv detached from the UI process.
pub fn play(request: &PlayRequest, mpv: Option<&str>) -> io::Result<Child> {
    let program = mpv.unwrap_or("mpv");
    Command::new(program)
        .args(mpv_args(request))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
}

/// Opens a page (external links: YouTube, Twitch, "report a bug"…).
pub fn open_external(url: &str) -> io::Result<Child> {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    Command::new(program)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
}

/// One call for the UI: plays a link, or opens it in the browser when it is a page.
pub fn play_link(link: &StreamLink, mpv: Option<&str>) -> io::Result<()> {
    match link.kind {
        StreamKind::External => open_external(&link.url).map(|_| ()),
        _ => play(&PlayRequest::from_link(link), mpv).map(|_| ()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_mpv_arguments() {
        let mut request = PlayRequest {
            url: "https://cdn/x.m3u8".to_string(),
            title: Some("Dune".to_string()),
            fullscreen: true,
            start_at: Some(90.0),
            ..Default::default()
        };
        request
            .headers
            .insert("Referer".to_string(), "https://site/".to_string());
        request.subtitles.push(Subtitle {
            lang: "es".to_string(),
            url: "https://cdn/es.vtt".to_string(),
            name: None,
        });

        let args = mpv_args(&request);
        assert!(args.contains(&"--force-window=yes".to_string()));
        assert!(args.contains(&"--fullscreen".to_string()));
        assert!(args.contains(&"--title=Dune".to_string()));
        assert!(args.contains(&"--http-header-fields=Referer: https://site/".to_string()));
        assert!(args.contains(&"--sub-file=https://cdn/es.vtt".to_string()));
        assert!(args.contains(&"--start=90".to_string()));
        assert_eq!(args.last().map(String::as_str), Some("https://cdn/x.m3u8"));
        // The `--` guard keeps a url with a leading dash from being a flag.
        assert_eq!(args[args.len() - 2], "--");
    }

    #[test]
    fn skips_headers_and_resume_when_empty() {
        let request = PlayRequest {
            url: "https://cdn/x.mp4".to_string(),
            ..Default::default()
        };
        let args = mpv_args(&request);
        assert!(!args
            .iter()
            .any(|arg| arg.starts_with("--http-header-fields")));
        assert!(!args.iter().any(|arg| arg.starts_with("--start")));
    }
}
