//! App state, background jobs and message plumbing.
//!
//! egui is synchronous, so every network call runs on a tokio runtime and sends a
//! [`Msg`] back through a channel; the UI drains the channel each frame.

use eframe::egui;
use std::future::Future;
use std::sync::mpsc::{channel, Receiver, Sender};
use streamgoat_core::host::{HostClient, PluginInfo, ProviderInfo};
use streamgoat_core::live::{Channel, Guide, LiveSource};
use streamgoat_core::model::{MediaItem, StreamLink};
use streamgoat_core::player;
use streamgoat_core::stremio::AddonClient;
use streamgoat_core::{resolve, Settings, Store};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Home,
    Search,
    Live,
    Settings,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Home, Tab::Search, Tab::Live, Tab::Settings];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Home => "Inicio",
            Tab::Search => "Buscar",
            Tab::Live => "Live TV",
            Tab::Settings => "Ajustes",
        }
    }
}

pub enum Msg {
    Providers(Vec<ProviderInfo>),
    Plugins(Vec<PluginInfo>),
    Items(Vec<MediaItem>),
    Details(Box<MediaItem>),
    Links(Vec<StreamLink>),
    Channels(Vec<Channel>),
    Guide(Box<Guide>),
    Status(String),
    Error(String),
}

pub struct App {
    pub store: Store,
    pub settings: Settings,
    pub tab: Tab,

    /// The extension host, once it answered.
    pub host: Option<HostClient>,
    pub addons: Vec<AddonClient>,

    pub providers: Vec<ProviderInfo>,
    pub plugins: Vec<PluginInfo>,
    pub provider: Option<String>,
    pub items: Vec<MediaItem>,
    pub details: Option<MediaItem>,
    pub links: Vec<StreamLink>,
    pub detail_from_live: bool,

    pub search: String,

    pub live_index: usize,
    pub channels: Vec<Channel>,
    pub channel_filter: String,
    pub guide: Option<Guide>,

    pub addon_buf: String,
    pub live_buf: String,
    pub host_addr_buf: String,
    pub host_token_buf: String,
    pub host_command_buf: String,
    pub player_buf: String,

    pub status: String,
    pub error: Option<String>,
    pub busy: usize,

    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    rt: tokio::runtime::Runtime,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let store = Store::default();
        let settings = store.load_settings();

        let addon_buf = settings.addons.join("\n");
        let live_buf = format_live_sources(&settings.live_sources);
        let host_addr_buf = settings.host_addr.clone().unwrap_or_default();
        let host_token_buf = settings.host_token.clone().unwrap_or_default();
        let host_command_buf = settings.host_command.clone().unwrap_or_default();
        let player_buf = settings.player_command.clone().unwrap_or_default();

        let host = settings.host_addr.as_ref().map(|addr| {
            HostClient::new(
                addr.clone(),
                settings.host_token.clone().unwrap_or_default(),
            )
        });

        let (tx, rx) = channel();
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("tokio runtime");

        cc.egui_ctx.style_mut(|style| {
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            style.spacing.button_padding = egui::vec2(10.0, 6.0);
        });

        let mut app = App {
            store,
            settings,
            tab: Tab::Home,
            host,
            addons: Vec::new(),
            providers: Vec::new(),
            plugins: Vec::new(),
            provider: None,
            items: Vec::new(),
            details: None,
            links: Vec::new(),
            detail_from_live: false,
            search: String::new(),
            live_index: 0,
            channels: Vec::new(),
            channel_filter: String::new(),
            guide: None,
            addon_buf,
            live_buf,
            host_addr_buf,
            host_token_buf,
            host_command_buf,
            player_buf,
            status: "listo".to_string(),
            error: None,
            busy: 0,
            tx,
            rx,
            rt,
        };
        if app.host.is_some() {
            app.connect(&cc.egui_ctx);
        }
        app
    }

    // --- background plumbing --------------------------------------------------

    fn spawn_job<F>(&mut self, ctx: &egui::Context, job: F)
    where
        F: Future<Output = Msg> + Send + 'static,
    {
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        self.busy += 1;
        self.rt.spawn(async move {
            let msg = job.await;
            let _ = tx.send(msg);
            ctx.request_repaint();
        });
    }

    fn drain(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            self.busy = self.busy.saturating_sub(1);
            match msg {
                Msg::Providers(providers) => {
                    if let Some(first) = providers.first() {
                        let id = first.id.clone();
                        if self.provider.is_none() {
                            self.provider = Some(id);
                        }
                    }
                    self.status = format!("{} proveedores", providers.len());
                    self.providers = providers;
                }
                Msg::Plugins(plugins) => {
                    self.status = format!("{} extensiones", plugins.len());
                    self.plugins = plugins;
                }
                Msg::Items(items) => {
                    self.status = format!("{} resultados", items.len());
                    self.items = items;
                }
                Msg::Details(item) => {
                    self.details = Some(*item);
                }
                Msg::Links(links) => {
                    self.status = format!("{} fuentes", links.len());
                    self.links = links;
                }
                Msg::Channels(channels) => {
                    self.status = format!("{} canales", channels.len());
                    self.channels = channels;
                }
                Msg::Guide(guide) => {
                    self.status = format!("guía: {} programas", guide.programmes.len());
                    self.guide = Some(*guide);
                }
                Msg::Status(text) => self.status = text,
                Msg::Error(error) => {
                    self.status = "error".to_string();
                    self.error = Some(error);
                }
            }
        }
    }

    // --- actions --------------------------------------------------------------

    /// Health check, then providers and plugins.
    pub fn connect(&mut self, ctx: &egui::Context) {
        let Some(host) = self.host.clone() else {
            self.error = Some("no hay host configurado (Ajustes → Extensión host)".to_string());
            return;
        };
        self.spawn_job(ctx, async move {
            if let Err(error) = host.health().await {
                return Msg::Error(format!("{error}"));
            }
            match host.providers().await {
                Ok(providers) => Msg::Providers(providers),
                Err(error) => Msg::Error(format!("{error}")),
            }
        });
    }

    pub fn load_plugins(&mut self, ctx: &egui::Context) {
        let Some(host) = self.host.clone() else {
            return;
        };
        self.spawn_job(ctx, async move {
            match host.plugins().await {
                Ok(plugins) => Msg::Plugins(plugins),
                Err(error) => Msg::Error(format!("{error}")),
            }
        });
    }

    pub fn load_home(&mut self, ctx: &egui::Context, provider: String) {
        let Some(host) = self.host.clone() else {
            return;
        };
        self.spawn_job(ctx, async move {
            match host.home(&provider, 1).await {
                Ok(page) => Msg::Items(
                    page.items
                        .into_iter()
                        .map(|mut item| {
                            if item.provider.is_none() {
                                item.provider = Some(provider.clone());
                            }
                            item
                        })
                        .collect(),
                ),
                Err(error) => Msg::Error(format!("{error}")),
            }
        });
    }

    pub fn search(&mut self, ctx: &egui::Context) {
        let query = self.search.trim().to_string();
        if query.is_empty() {
            return;
        }
        let Some(host) = self.host.clone() else {
            self.error = Some("la búsqueda usa el host de extensiones".to_string());
            return;
        };
        let provider = self.provider.clone();
        self.spawn_job(ctx, async move {
            let result = match provider {
                Some(provider) => host.search(&provider, &query).await,
                None => host.quick_search(&query).await,
            };
            match result {
                Ok(items) => Msg::Items(items),
                Err(error) => Msg::Error(format!("{error}")),
            }
        });
    }

    /// Details and links for a card.
    pub fn open_item(&mut self, ctx: &egui::Context, item: MediaItem) {
        let host = self.host.clone();
        self.details = Some(item.clone());
        self.links.clear();
        self.detail_from_live =
            item.source.as_deref() == Some("live") || item.source.as_deref() == Some("xtream");
        self.spawn_job(ctx, async move {
            match resolve::details(host.as_ref(), &item).await {
                Ok(full) => Msg::Details(Box::new(full)),
                Err(error) => Msg::Error(format!("{error}")),
            }
        });

        let host = self.host.clone();
        self.spawn_job(ctx, async move {
            match resolve::links(host.as_ref(), &item).await {
                Ok(links) => Msg::Links(links),
                Err(error) => Msg::Error(format!("{error}")),
            }
        });
    }

    /// Episode (or item) links.
    pub fn open_episode(&mut self, ctx: &egui::Context, url: String) {
        let host = self.host.clone();
        let Some(mut item) = self.details.clone() else {
            return;
        };
        item.url = url;
        self.spawn_job(ctx, async move {
            match resolve::links(host.as_ref(), &item).await {
                Ok(links) => Msg::Links(links),
                Err(error) => Msg::Error(format!("{error}")),
            }
        });
    }

    pub fn play(&mut self, link: &StreamLink) {
        let player = self.settings.player_command.clone();
        match player::play_link(link, player.as_deref()) {
            Ok(()) => self.status = format!("reproduciendo {}", link.label()),
            Err(error) => self.error = Some(format!("no se pudo abrir el reproductor: {error}")),
        }
    }

    pub fn load_live(&mut self, ctx: &egui::Context) {
        let Some(source) = self.settings.live_sources.get(self.live_index).cloned() else {
            self.error = Some("añade una lista M3U o un login Xtream en Ajustes".to_string());
            return;
        };
        self.spawn_job(ctx, async move {
            match source.channels().await {
                Ok(channels) => Msg::Channels(channels),
                Err(error) => Msg::Error(format!("{error}")),
            }
        });
    }

    pub fn load_guide(&mut self, ctx: &egui::Context) {
        let Some(source) = self.settings.live_sources.get(self.live_index).cloned() else {
            return;
        };
        let epg_url = self.settings.epg_url.clone();
        self.spawn_job(ctx, async move {
            let result = match source.guide().await {
                Ok(Some(guide)) => Ok(guide),
                Ok(None) => match epg_url {
                    Some(url) => streamgoat_core::live::load_guide(&url).await,
                    None => Err(streamgoat_core::Error::Missing(
                        "esta lista no trae guía y no hay XMLTV configurado".to_string(),
                    )),
                },
                Err(error) => Err(error),
            };
            match result {
                Ok(guide) => Msg::Guide(Box::new(guide)),
                Err(error) => Msg::Error(format!("{error}")),
            }
        });
    }

    /// Opens every configured add-on and merges their first catalogs.
    pub fn load_addons(&mut self, ctx: &egui::Context) {
        let specs = self.settings.addons.clone();
        if specs.is_empty() {
            self.error = Some("añade add-ons de Stremio en Ajustes".to_string());
            return;
        }
        self.spawn_job(ctx, async move {
            let mut items = Vec::new();
            let mut failures = 0usize;
            for spec in &specs {
                match AddonClient::open(spec).await {
                    Ok(addon) => {
                        let Some(catalog) = addon.catalogs().first() else {
                            continue;
                        };
                        let kind = catalog.kind.clone();
                        let id = catalog.id.clone();
                        let name = addon.manifest.name.clone();
                        match addon.catalog(&kind, &id, &[]).await {
                            Ok(mut found) => {
                                for item in &mut found {
                                    if item.provider.is_none() {
                                        item.provider = Some(name.clone());
                                    }
                                }
                                items.extend(found);
                            }
                            Err(_) => failures += 1,
                        }
                    }
                    Err(_) => failures += 1,
                }
            }
            if items.is_empty() && failures > 0 {
                Msg::Error("ningún add-on respondió".to_string())
            } else {
                Msg::Items(items)
            }
        });
    }

    pub fn save_settings(&mut self) {
        let mut settings = self.settings.clone();
        settings.addons = self
            .addon_buf
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect();
        settings.live_sources = parse_live_sources(&self.live_buf);
        settings.host_addr = none_if_empty(&self.host_addr_buf);
        settings.host_token = none_if_empty(&self.host_token_buf);
        settings.host_command = none_if_empty(&self.host_command_buf);
        settings.player_command = none_if_empty(&self.player_buf);

        match self.store.save_settings(&settings) {
            Ok(()) => {
                self.settings = settings;
                self.host = self.settings.host_addr.as_ref().map(|addr| {
                    HostClient::new(
                        addr.clone(),
                        self.settings.host_token.clone().unwrap_or_default(),
                    )
                });
                self.status = format!("guardado en {}", self.store.root().display());
            }
            Err(error) => self.error = Some(format!("no se pudo guardar: {error}")),
        }
    }

    /// Live source that is selected right now.
    pub fn current_live(&self) -> Option<&LiveSource> {
        self.settings.live_sources.get(self.live_index)
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain();

        egui::TopBottomPanel::top("bar").show(ctx, |ui| {
            panels::top_bar(self, ctx, ui);
        });

        if self.details.is_some() {
            egui::SidePanel::right("details")
                .default_width(380.0)
                .show(ctx, |ui| {
                    panels::details_panel(self, ctx, ui);
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Home => panels::home_tab(self, ctx, ui),
            Tab::Search => panels::search_tab(self, ctx, ui),
            Tab::Live => panels::live_tab(self, ctx, ui),
            Tab::Settings => panels::settings_tab(self, ctx, ui),
        });
    }
}

// --- helpers ------------------------------------------------------------------

fn none_if_empty(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// `m3u | Nombre | url` or `xtream | Nombre | base | usuario | contraseña`.
pub fn parse_live_sources(text: &str) -> Vec<LiveSource> {
    let mut sources = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split('|').map(str::trim).collect();
        match parts.first().copied() {
            Some("m3u") if parts.len() >= 3 => sources.push(LiveSource::M3u {
                name: parts[1].to_string(),
                spec: parts[2].to_string(),
            }),
            Some("xtream") if parts.len() >= 5 => sources.push(LiveSource::Xtream {
                name: parts[1].to_string(),
                base: parts[2].to_string(),
                username: parts[3].to_string(),
                password: parts[4].to_string(),
            }),
            _ => {}
        }
    }
    sources
}

pub fn format_live_sources(sources: &[LiveSource]) -> String {
    sources
        .iter()
        .map(|source| match source {
            LiveSource::M3u { name, spec } => format!("m3u | {name} | {spec}"),
            LiveSource::Xtream {
                name,
                base,
                username,
                password,
            } => format!("xtream | {name} | {base} | {username} | {password}"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}
