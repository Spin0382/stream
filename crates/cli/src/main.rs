//! `streamgoat`: the command line for everything the Rust half can do.
//!
//! It exists for two reasons: it is the fastest way to check a source (a playlist,
//! a Stremio add-on, a provider in the extension host) without opening the UI, and
//! it is how the CI pipeline exercises the core without a screen.

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use streamgoat_core::host::HostClient;
use streamgoat_core::live::{group_channels, parse_playlist, XtreamClient};
use streamgoat_core::model::{MediaItem, StreamLink};
use streamgoat_core::player;
use streamgoat_core::{http, stremio, MediaKind, Store};

#[derive(Parser, Debug)]
#[command(
    name = "streamgoat",
    version,
    about = "Stream Goat CLI: add-ons, Live TV, EPG and the CloudStream extension host",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Stremio add-ons: manifest, catalogs and streams.
    Stremio {
        #[command(subcommand)]
        command: StremioCommand,
    },
    /// Live TV: M3U playlists and Xtream Codes panels.
    Live {
        #[command(subcommand)]
        command: LiveCommand,
    },
    /// XMLTV guides.
    Epg {
        /// Url or local file of the XMLTV document.
        spec: String,
        /// Only this channel id.
        #[arg(long)]
        channel: Option<String>,
        /// How many programmes to print.
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// CloudStream extension repositories (`repo.json`).
    Repo {
        #[command(subcommand)]
        command: RepoCommand,
    },
    /// The JVM extension host (CloudStream extensions).
    Host {
        #[command(flatten)]
        connection: HostConnection,
        #[command(subcommand)]
        command: HostCommand,
    },
    /// Plays a url with mpv.
    Play {
        url: String,
        /// Extra header, `Name: value`.
        #[arg(long = "header")]
        headers: Vec<String>,
        /// Windowed instead of fullscreen.
        #[arg(long)]
        windowed: bool,
    },
    /// Shows the settings file and where it lives.
    Settings,
}

#[derive(Subcommand, Debug)]
enum StremioCommand {
    /// Prints an add-on manifest.
    Manifest { url: String },
    /// Lists a catalog.
    Catalog {
        url: String,
        #[arg(value_name = "TYPE")]
        kind: String,
        #[arg(value_name = "ID")]
        id: String,
        /// Value for the catalog's `search` extra.
        #[arg(long)]
        search: Option<String>,
        /// Value for the catalog's `skip` extra.
        #[arg(long)]
        skip: Option<String>,
    },
    /// Full metadata (episodes for series).
    Meta {
        url: String,
        #[arg(value_name = "TYPE")]
        kind: String,
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Streams, ready to play.
    Streams {
        url: String,
        #[arg(value_name = "TYPE")]
        kind: String,
        #[arg(value_name = "ID")]
        id: String,
        /// Play the first link instead of only printing them.
        #[arg(long)]
        play: bool,
    },
}

#[derive(Subcommand, Debug)]
enum LiveCommand {
    /// Reads an M3U/M3U8 playlist (url or file).
    M3u {
        spec: String,
        /// Only channels of this group (substring, case insensitive).
        #[arg(long)]
        group: Option<String>,
        /// Only channels matching this text.
        #[arg(long)]
        search: Option<String>,
        #[arg(long, default_value_t = 30)]
        limit: usize,
        /// Prints one url per line, no decoration.
        #[arg(long)]
        urls_only: bool,
    },
    /// Logs into an Xtream Codes panel and lists its live channels.
    Xtream {
        base: String,
        username: String,
        password: String,
        /// Lists categories instead of channels.
        #[arg(long)]
        categories: bool,
        #[arg(long)]
        search: Option<String>,
        #[arg(long, default_value_t = 30)]
        limit: usize,
        /// Prints one url per line, no decoration.
        #[arg(long)]
        urls_only: bool,
    },
}

#[derive(Args, Debug)]
struct HostConnection {
    /// `127.0.0.1:PORT` of a running host.
    #[arg(long, env = "STREAMGOAT_HOST")]
    addr: Option<String>,
    /// Bearer token printed by the host.
    #[arg(long, env = "STREAMGOAT_TOKEN")]
    token: Option<String>,
}

#[derive(Subcommand, Debug)]
enum HostCommand {
    /// Version, provider and plugin counts.
    Health,
    /// Lists the providers of every installed extension.
    Providers,
    /// Lists installed extensions.
    Plugins,
    /// A provider's home page.
    Home {
        provider: String,
        #[arg(long, default_value_t = 1)]
        page: i64,
    },
    /// Searches inside one provider.
    Search { provider: String, query: String },
    /// Searches every provider at once.
    QuickSearch { query: String },
    /// Loads an item (`--url` is the data url printed by a previous search).
    Load { provider: String, url: String },
    /// Lists the configured extension repositories.
    Repos,
    /// Adds a repository (url, cloudstreamrepo:// link or short code).
    RepoAdd { url: String },
    /// Removes a repository.
    RepoRemove { url: String },
    /// Lists the extensions offered by the repositories.
    Available {
        /// Only this repository.
        #[arg(long)]
        repo: Option<String>,
        /// Only extensions whose name matches this text.
        #[arg(long)]
        search: Option<String>,
    },
    /// Installs or updates one extension.
    Install { internal_name: String },
    /// Updates every installed extension.
    Update,
    /// Removes an extension.
    Uninstall { internal_name: String },
    /// Lists the links of an item and optionally plays one.
    Links {
        provider: String,
        url: String,
        /// Plays this link index (1-based).
        #[arg(long)]
        play: Option<usize>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Stremio { command } => stremio_command(command).await,
        Command::Live { command } => live_command(command).await,
        Command::Epg {
            spec,
            channel,
            limit,
        } => epg_command(&spec, channel.as_deref(), limit).await,
        Command::Repo { command } => repo_command(command).await,
        Command::Host {
            connection,
            command,
        } => host_command(connection, command).await,
        Command::Play {
            url,
            headers,
            windowed,
        } => play_command(&url, &headers, windowed),
        Command::Settings => settings_command(),
    }
}

#[derive(Debug, Subcommand)]
enum RepoCommand {
    /// Downloads a repository and lists what it offers.
    Inspect {
        /// A `repo.json` url, a `cloudstreamrepo://` link or a bare host.
        spec: String,
        /// Only extensions whose name or id matches this text.
        #[arg(long)]
        search: Option<String>,
        /// Only extensions of this tv type (Movie, Series, Anime, Live).
        #[arg(long)]
        kind: Option<String>,
    },
}

async fn repo_command(command: RepoCommand) -> Result<()> {
    match command {
        RepoCommand::Inspect { spec, search, kind } => {
            let inspection = streamgoat_core::repo::inspect(&spec).await?;
            let repo = &inspection.repo;
            println!("{}", repo.name);
            println!("  {}", inspection.url);
            if let Some(description) = &repo.description {
                println!("  {description}");
            }
            let dalvik = inspection
                .plugins
                .iter()
                .filter(|plugin| plugin.runtime() == "Dalvik")
                .count();
            let jvm = inspection.plugins.len() - dalvik;
            println!(
                "  {} extensiones: {dalvik} .cs3 (Dalvik, necesitan el host) y {jvm} en jar",
                inspection.plugins.len()
            );
            println!();

            let needle = search.map(|text| text.to_ascii_lowercase());
            let kind = kind.map(|text| text.to_ascii_lowercase());
            let mut shown = 0usize;
            for plugin in &inspection.plugins {
                if let Some(needle) = needle.as_deref() {
                    let haystack = format!(
                        "{} {}",
                        plugin.name.to_ascii_lowercase(),
                        plugin.internal_name.to_ascii_lowercase()
                    );
                    if !haystack.contains(needle) {
                        continue;
                    }
                }
                if let Some(kind) = kind.as_deref() {
                    let matches = plugin
                        .tv_types
                        .iter()
                        .any(|tv| tv.to_ascii_lowercase().contains(kind));
                    if !matches {
                        continue;
                    }
                }
                shown += 1;
                println!(
                    "{:<30} v{:<4} {:<6} {:<7} {}",
                    truncate(&plugin.name, 30),
                    plugin.version,
                    plugin.language.as_deref().unwrap_or("--"),
                    plugin.status_label(),
                    plugin.internal_name
                );
                if let Some(description) = &plugin.description {
                    if !description.trim().is_empty() {
                        println!("    {}", truncate(description, 96));
                    }
                }
            }
            if shown == 0 {
                println!("ninguna extensión coincide con el filtro");
            }
        }
    }
    Ok(())
}

async fn stremio_command(command: StremioCommand) -> Result<()> {
    match command {
        StremioCommand::Manifest { url } => {
            let addon = stremio::AddonClient::open(&url).await?;
            println!("{} v{}", addon.manifest.name, addon.manifest.version);
            println!("base: {}", addon.base);
            if let Some(description) = &addon.manifest.description {
                println!("{description}");
            }
            println!("\ncatálogos:");
            for catalog in addon.catalogs() {
                let search = if catalog.supports_search() {
                    " · buscable"
                } else {
                    ""
                };
                println!("  {} ({}){}", catalog.title(), catalog.kind, search);
            }
        }
        StremioCommand::Catalog {
            url,
            kind,
            id,
            search,
            skip,
        } => {
            let addon = stremio::AddonClient::open(&url).await?;
            let mut extra: Vec<(&str, &str)> = Vec::new();
            if let Some(search) = search.as_deref() {
                extra.push(("search", search));
            }
            if let Some(skip) = skip.as_deref() {
                extra.push(("skip", skip));
            }
            let items = addon.catalog(&kind, &id, &extra).await?;
            print_items(&items);
        }
        StremioCommand::Meta { url, kind, id } => {
            let addon = stremio::AddonClient::open(&url).await?;
            let item = addon.meta(&kind, &id).await?;
            print_item(&item);
            for episode in &item.episodes {
                println!("  · {}", episode.label());
            }
        }
        StremioCommand::Streams {
            url,
            kind,
            id,
            play,
        } => {
            let addon = stremio::AddonClient::open(&url).await?;
            let links = addon.streams(&kind, &id).await?;
            if links.is_empty() {
                println!("sin fuentes para {kind}/{id}");
                return Ok(());
            }
            print_links(&links);
            if play {
                player::play_link(&links[0], None)?;
                println!("\nreproduciendo la primera fuente…");
            }
        }
    }
    Ok(())
}

async fn live_command(command: LiveCommand) -> Result<()> {
    match command {
        LiveCommand::M3u {
            spec,
            group,
            search,
            limit,
            urls_only,
        } => {
            let text = http::read_url_or_file(&spec).await?;
            let mut channels = parse_playlist(&text);
            println!("{} canales en {}", channels.len(), spec);
            if let Some(group) = group.as_deref() {
                let needle = group.to_ascii_lowercase();
                channels.retain(|channel| {
                    channel
                        .group
                        .as_deref()
                        .map(|g| g.to_ascii_lowercase().contains(&needle))
                        .unwrap_or(false)
                });
            }
            if let Some(search) = search.as_deref() {
                let needle = search.to_ascii_lowercase();
                channels.retain(|channel| channel.name.to_ascii_lowercase().contains(&needle));
            }
            if urls_only {
                for channel in channels.iter().take(limit) {
                    println!("{}", channel.url);
                }
                return Ok(());
            }
            let groups = group_channels(&channels);
            println!("{} grupos:", groups.len());
            for (group, list) in groups.iter().take(12) {
                println!("  {group} ({})", list.len());
            }
            println!();
            for channel in channels.iter().take(limit) {
                println!(
                    "{:<40} {:<24} {}",
                    truncate(&channel.name, 40),
                    truncate(channel.group.as_deref().unwrap_or("-"), 24),
                    channel.tvg_id.as_deref().unwrap_or("")
                );
            }
        }
        LiveCommand::Xtream {
            base,
            username,
            password,
            categories,
            search,
            limit,
            urls_only,
        } => {
            let client = XtreamClient::new(base, username, password);
            let account = client.login().await.context("login Xtream")?;
            println!(
                "cuenta {} · estado {} · {} conexiones",
                account.username,
                account.status,
                account.max_connections.unwrap_or(1)
            );
            if categories {
                for category in client.live_categories().await? {
                    println!("{:>6}  {}", category.id, category.name);
                }
                return Ok(());
            }
            let mut channels = client.live_streams(None).await?;
            if let Some(search) = search.as_deref() {
                let needle = search.to_ascii_lowercase();
                channels.retain(|channel| channel.name.to_ascii_lowercase().contains(&needle));
            }
            println!("{} canales", channels.len());
            for channel in channels.iter().take(limit) {
                if urls_only {
                    println!("{}", channel.url);
                } else {
                    println!(
                        "{:<40} {}",
                        truncate(&channel.name, 40),
                        channel.tvg_id.as_deref().unwrap_or("")
                    );
                }
            }
        }
    }
    Ok(())
}

async fn epg_command(spec: &str, channel: Option<&str>, limit: usize) -> Result<()> {
    let guide = streamgoat_core::live::load_guide(spec).await?;
    println!(
        "{} canales · {} programas",
        guide.channels.len(),
        guide.programmes.len()
    );
    let now = chrono::Utc::now();
    let programmes: Vec<&streamgoat_core::Programme> = match channel {
        Some(channel) => guide.on_channel(channel),
        None if !guide.channels.is_empty() => {
            let first = guide.channels.keys().next().cloned().unwrap_or_default();
            guide.on_channel(&first)
        }
        None => guide.programmes.iter().collect(),
    };
    for programme in programmes.iter().take(limit) {
        let marker = if guide
            .now_on(&programme.channel, now)
            .map(|current| current.start == programme.start)
            .unwrap_or(false)
        {
            "▶"
        } else {
            " "
        };
        println!(
            "{marker} {}  {:<28} {}",
            programme.time_label(),
            truncate(&guide.channel_name(&programme.channel), 28),
            programme.title
        );
    }
    Ok(())
}

async fn host_command(connection: HostConnection, command: HostCommand) -> Result<()> {
    let store = Store::default();
    let settings = store.load_settings();
    let addr = connection
        .addr
        .or(settings.host_addr)
        .context("falta --addr (o host_addr en settings.json)")?;
    let token = connection.token.or(settings.host_token).unwrap_or_default();
    let host = HostClient::new(addr, token);

    match command {
        HostCommand::Health => {
            let health = host.health().await?;
            println!(
                "host {} · {} proveedores · {} extensiones",
                health.version, health.providers, health.plugins
            );
        }
        HostCommand::Providers => {
            for provider in host.providers().await? {
                println!(
                    "{:<28} {:<16} {}",
                    truncate(&provider.name, 28),
                    provider.id,
                    provider.plugin.unwrap_or_default()
                );
            }
        }
        HostCommand::Plugins => {
            for plugin in host.plugins().await? {
                println!(
                    "{:<32} v{:<8} {} proveedores",
                    truncate(&plugin.name, 32),
                    plugin.version,
                    plugin.provider_count
                );
            }
        }
        HostCommand::Home { provider, page } => {
            let home = host.home(&provider, page).await?;
            print_items(&home.items);
            println!("hay más: {}", home.has_next);
        }
        HostCommand::Search { provider, query } => {
            print_items(&host.search(&provider, &query).await?);
        }
        HostCommand::QuickSearch { query } => {
            print_items(&host.quick_search(&query).await?);
        }
        HostCommand::Load { provider, url } => {
            let item = host.load(&provider, &url).await?;
            print_item(&item);
            for episode in &item.episodes {
                println!("  · {}", episode.label());
            }
        }
        HostCommand::Repos => {
            for repo in host.repos().await? {
                println!("{:<32} {}", truncate(&repo.name, 32), repo.url);
            }
        }
        HostCommand::RepoAdd { url } => {
            let repo = host.add_repo(&url).await?;
            println!("añadido {} ({})", repo.name, repo.url);
        }
        HostCommand::RepoRemove { url } => {
            let removed = host.remove_repo(&url).await?;
            println!("eliminado: {removed}");
        }
        HostCommand::Available { repo, search } => {
            let mut plugins = host.available(repo.as_deref()).await?;
            if let Some(search) = search.as_deref() {
                let needle = search.to_ascii_lowercase();
                plugins.retain(|plugin| {
                    plugin.name.to_ascii_lowercase().contains(&needle)
                        || plugin.internal_name.to_ascii_lowercase().contains(&needle)
                });
            }
            println!("{} extensiones\n", plugins.len());
            for plugin in plugins {
                let mark = if plugin.update_available() {
                    "actualizar"
                } else if plugin.is_installed() {
                    "instalada"
                } else {
                    "disponible"
                };
                println!(
                    "{:<34} v{:<6} {:<11} {:<9} {}",
                    truncate(&plugin.name, 34),
                    plugin.version,
                    plugin.status_label(),
                    mark,
                    plugin.internal_name
                );
            }
        }
        HostCommand::Install { internal_name } => {
            let version = host.install_plugin(&internal_name).await?;
            println!("instalada {internal_name} v{version}");
        }
        HostCommand::Update => {
            let updated = host.update_extensions().await?;
            if updated.is_empty() {
                println!("todo al día");
            } else {
                println!("actualizadas: {}", updated.join(", "));
            }
        }
        HostCommand::Uninstall { internal_name } => {
            host.uninstall_plugin(&internal_name).await?;
            println!("desinstalada {internal_name}");
        }
        HostCommand::Links {
            provider,
            url,
            play,
        } => {
            let links = host.links(&provider, &url).await?;
            if links.is_empty() {
                println!("el proveedor no devolvió fuentes");
                return Ok(());
            }
            print_links(&links);
            if let Some(index) = play {
                let link = links
                    .get(index.saturating_sub(1))
                    .context("índice de fuente fuera de rango")?;
                player::play_link(link, settings.player_command.as_deref())?;
                println!("\nreproduciendo la fuente {index}…");
            }
        }
    }
    Ok(())
}

fn play_command(url: &str, headers: &[String], windowed: bool) -> Result<()> {
    let mut request = player::PlayRequest {
        url: url.to_string(),
        fullscreen: !windowed,
        ..Default::default()
    };
    for header in headers {
        if let Some((key, value)) = header.split_once(':') {
            request
                .headers
                .insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    player::play(&request, None)?;
    println!("mpv abierto con {url}");
    Ok(())
}

fn settings_command() -> Result<()> {
    let store = Store::default();
    let settings = store.load_settings();
    println!("carpeta: {}", store.root().display());
    println!("{}", serde_json::to_string_pretty(&settings)?);
    Ok(())
}

fn print_items(items: &[MediaItem]) {
    if items.is_empty() {
        println!("(sin resultados)");
        return;
    }
    println!("{} resultados:\n", items.len());
    for item in items {
        let kind = match item.kind {
            MediaKind::Movie => "película",
            MediaKind::Series => "serie",
            MediaKind::Anime => "anime",
            MediaKind::Live => "directo",
            MediaKind::Other => "otro",
        };
        println!(
            "{:<44} {:<9} {:<16} {}",
            truncate(&item.name, 44),
            kind,
            truncate(item.provider.as_deref().unwrap_or("-"), 16),
            item.url
        );
    }
}

fn print_item(item: &MediaItem) {
    println!("{}", item.name);
    if let Some(year) = item.year {
        println!("  año: {year}");
    }
    if let Some(description) = &item.description {
        println!("  {}", truncate(description, 200));
    }
    println!("  url: {}", item.url);
    if let Some(poster) = &item.poster {
        println!("  póster: {poster}");
    }
}

fn print_links(links: &[StreamLink]) {
    for (index, link) in links.iter().enumerate() {
        println!(
            "{:>2}. {:<32} {:<10} {:?}",
            index + 1,
            truncate(&link.label(), 32),
            link.quality.as_deref().unwrap_or(""),
            link.kind
        );
        println!("    {}", link.url);
        if !link.headers.is_empty() {
            let headers: Vec<String> = link
                .headers
                .iter()
                .map(|(key, value)| format!("{key}: {value}"))
                .collect();
            println!("    cabeceras: {}", headers.join(", "));
        }
    }
}

fn truncate(text: &str, width: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= width {
        return text.to_string();
    }
    let mut out: String = chars.iter().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}
