//! Errors shared by every part of the core crate.

use thiserror::Error;

/// Everything in `streamgoat-core` fails with this type.
#[derive(Debug, Error)]
pub enum Error {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("invalid url {url}: {source}")]
    Url {
        url: String,
        #[source]
        source: url::ParseError,
    },

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("server answered {status} for {url}")]
    Status { status: u16, url: String },

    /// The JVM extension host answered with an error object.
    #[error("extension host error {code}: {message}")]
    Host { code: i64, message: String },

    /// The extension host could not be reached at all (not running, wrong port…).
    #[error("extension host unreachable at {addr}: {reason}")]
    HostUnreachable { addr: String, reason: String },

    #[error("could not parse {what}: {reason}")]
    Parse { what: String, reason: String },

    #[error("missing configuration: {0}")]
    Missing(String),
}

impl Error {
    pub(crate) fn parse(what: impl Into<String>, reason: impl Into<String>) -> Self {
        Error::Parse {
            what: what.into(),
            reason: reason.into(),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
