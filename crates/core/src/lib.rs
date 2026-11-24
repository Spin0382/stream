//! # Stream Goat core
//!
//! The Rust half of the hybrid app. It owns everything that does **not** need a
//! JVM:
//!
//! * [`stremio`] – Stremio add-ons (catalogs, meta, streams).
//! * [`live`] – Live TV: M3U playlists, Xtream Codes panels, XMLTV guides.
//! * [`player`] – handing urls to mpv / the browser.
//! * [`store`] – settings and library as JSON.
//! * [`host`] – the client for the JVM extension host that runs CloudStream `.cs3`
//!   extensions untouched.
//! * [`resolve`] – where an item came from and how to get its links.
//!
//! The split is deliberate: CloudStream extensions are Dalvik bytecode linked
//! against CloudStream's Kotlin API, so they keep running on a JVM (see
//! `docs/host-protocol.md`); nothing else here needs one.

pub mod error;
pub mod host;
pub mod http;
pub mod live;
pub mod model;
pub mod player;
pub mod resolve;
pub mod store;
pub mod stremio;

pub use error::{Error, Result};
pub use host::{HostClient, HostHealth, HostProcess, PluginInfo, ProviderInfo};
pub use live::{Channel, Guide, LiveSource, Programme, XtreamClient};
pub use model::{Episode, MediaItem, MediaKind, StreamKind, StreamLink, Subtitle};
pub use store::{Settings, Store};

/// The version string shown in About, taken from Cargo.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
