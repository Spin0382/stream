//! Rendering of the four tabs and the details side panel.
//!
//! Deliberately plain egui: lists and buttons. The Kotlin app's look (liquid glass,
//! pills, animations) is a later stage; getting sources, links and playback right
//! comes first.

use crate::app::{App, Tab};
use eframe::egui;
use streamgoat_core::live::filter_channels;
use streamgoat_core::model::{MediaKind, StreamLink};

pub fn top_bar(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.heading("Stream Goat");
        ui.separator();
        for tab in Tab::ALL {
            if ui.selectable_label(app.tab == tab, tab.title()).clicked() {
                app.tab = tab;
            }
        }
        ui.separator();
        if app.busy > 0 {
            ui.spinner();
            ui.label("trabajando…");
        } else {
            ui.label(&app.status);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let connected = app.host.is_some() && !app.providers.is_empty();
            let label = if connected {
                "Host conectado"
            } else {
                "Conectar host"
            };
            if ui.button(label).clicked() {
                app.connect(ctx);
            }
        });
    });

    if let Some(error) = app.error.clone() {
        ui.horizontal(|ui| {
            ui.colored_label(egui::Color32::from_rgb(220, 90, 90), format!("⚠ {error}"));
            if ui.small_button("descartar").clicked() {
                app.error = None;
            }
        });
    }
}

pub fn home_tab(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        if app.providers.is_empty() {
            ui.label("Sin providers: arranca el host de extensiones y pulsa Conectar host.");
        } else {
            let current = app
                .provider
                .as_ref()
                .and_then(|id| {
                    app.providers
                        .iter()
                        .find(|provider| &provider.id == id)
                        .map(|provider| provider.name.clone())
                })
                .unwrap_or_else(|| "Elige provider".to_string());
            egui::ComboBox::from_id_salt("provider")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    let ids: Vec<(String, String)> = app
                        .providers
                        .iter()
                        .map(|provider| (provider.id.clone(), provider.name.clone()))
                        .collect();
                    for (id, name) in ids {
                        ui.selectable_value(&mut app.provider, Some(id), name);
                    }
                });
            if ui.button("Cargar inicio").clicked() {
                if let Some(provider) = app.provider.clone() {
                    app.load_home(ctx, provider);
                }
            }
        }
        if ui.button("Cargar add-ons").clicked() {
            app.load_addons(ctx);
        }
    });
    ui.separator();

    ui.horizontal(|ui| {
        ui.small(format!(
            "{} providers · {} add-ons · {} tarjetas",
            app.providers.len(),
            app.settings.addons.len(),
            app.items.len()
        ));
    });

    let mut clicked = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for (index, item) in app.items.iter().enumerate() {
                let kind = match item.kind {
                    MediaKind::Movie => "🎬",
                    MediaKind::Series => "📺",
                    MediaKind::Anime => "🌸",
                    MediaKind::Live => "📡",
                    MediaKind::Other => "▪",
                };
                let text = format!("{kind} {}", item.name);
                if ui
                    .add(egui::Button::new(text).min_size(egui::vec2(200.0, 34.0)))
                    .on_hover_text(format!(
                        "{}\n{}",
                        item.provider.clone().unwrap_or_default(),
                        item.description.clone().unwrap_or_default()
                    ))
                    .clicked()
                {
                    clicked = Some(index);
                }
            }
        });
    });

    if let Some(index) = clicked {
        if let Some(item) = app.items.get(index).cloned() {
            app.open_item(ctx, item);
        }
    }
}

pub fn search_tab(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let response = ui.add(
            egui::TextEdit::singleline(&mut app.search)
                .hint_text("Título, serie, anime…")
                .desired_width(320.0),
        );
        let enter = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if ui.button("Buscar").clicked() || enter {
            app.search(ctx);
        }
        ui.label(
            app.provider
                .clone()
                .unwrap_or_else(|| "búsqueda rápida".to_string()),
        );
    });
    ui.separator();

    let mut clicked = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        for (index, item) in app.items.iter().enumerate() {
            if ui
                .button(format!(
                    "{} — {}",
                    item.name,
                    item.provider.clone().unwrap_or_default()
                ))
                .clicked()
            {
                clicked = Some(index);
            }
        }
    });

    if let Some(index) = clicked {
        if let Some(item) = app.items.get(index).cloned() {
            app.open_item(ctx, item);
        }
    }
}

pub fn live_tab(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let names: Vec<String> = app
            .settings
            .live_sources
            .iter()
            .map(|source| source.name().to_string())
            .collect();

        if names.is_empty() {
            ui.label("Añade una lista M3U o un login Xtream en Ajustes.");
        } else {
            let selected = names
                .get(app.live_index)
                .cloned()
                .unwrap_or_else(|| names[0].clone());
            egui::ComboBox::from_id_salt("live_source")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for (index, name) in names.iter().enumerate() {
                        ui.selectable_value(&mut app.live_index, index, name);
                    }
                });
            if ui.button("Cargar canales").clicked() {
                app.channels.clear();
                app.guide = None;
                app.load_live(ctx);
            }
            if ui.button("Guía (EPG)").clicked() {
                app.load_guide(ctx);
            }
        }

        ui.separator();
        ui.label("filtrar:");
        ui.add(
            egui::TextEdit::singleline(&mut app.channel_filter)
                .hint_text("nombre o grupo")
                .desired_width(200.0),
        );
    });
    ui.separator();

    let filter = app.channel_filter.clone();
    let visible = filter_channels(&app.channels, &filter);
    let now = chrono_now();

    let mut play: Option<usize> = None;
    egui::ScrollArea::vertical()
        .max_height(ui.available_height() * 0.65)
        .show(ui, |ui| {
            for channel in &visible {
                let programme = app
                    .guide
                    .as_ref()
                    .and_then(|guide| {
                        guide
                            .on_channel(channel.tvg_id.as_deref().unwrap_or(""))
                            .first()
                            .copied()
                    })
                    .map(|programme| format!("{} · {}", programme.time_label(), programme.title))
                    .unwrap_or_default();
                let label = format!(
                    "{:<38} {:<18} {}",
                    channel.name,
                    channel.group.clone().unwrap_or_default(),
                    programme
                );
                if ui.selectable_label(false, label).clicked() {
                    play = Some(
                        app.channels
                            .iter()
                            .position(|c| c.url == channel.url)
                            .unwrap_or(0),
                    );
                }
            }
        });

    if let Some(index) = play {
        let link = app.channels.get(index).map(|channel| {
            let mut link = StreamLink::new(&channel.url);
            link.name = Some(channel.name.clone());
            link.headers = channel.headers.clone();
            link
        });
        if let Some(link) = link {
            app.play(&link);
        }
    }

    if let Some(guide) = app.guide.as_ref() {
        ui.separator();
        ui.label(format!(
            "guía: {} canales, {} programas (hora del servidor {})",
            guide.channels.len(),
            guide.programmes.len(),
            now
        ));
    }
}

pub fn settings_tab(app: &mut App, _ctx: &egui::Context, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading("Extensión host (CloudStream)");
        ui.label("El host es un proceso Java aparte; aquí solo se le dice dónde escucha.");
        egui::Grid::new("host_grid").num_columns(2).show(ui, |ui| {
            ui.label("Dirección");
            ui.add(
                egui::TextEdit::singleline(&mut app.host_addr_buf)
                    .hint_text("127.0.0.1:9000")
                    .desired_width(320.0),
            );
            ui.end_row();

            ui.label("Token");
            ui.add(
                egui::TextEdit::singleline(&mut app.host_token_buf)
                    .hint_text("el que imprime el host al arrancar")
                    .password(true)
                    .desired_width(320.0),
            );
            ui.end_row();

            ui.label("Comando");
            ui.add(
                egui::TextEdit::singleline(&mut app.host_command_buf)
                    .hint_text("java -jar /opt/streamgoat/streamgoat-host.jar")
                    .desired_width(420.0),
            );
            ui.end_row();
        });

        ui.add_space(10.0);
        ui.heading("Stremio");
        ui.label("Un add-on por línea: https://…/manifest.json");
        ui.add(
            egui::TextEdit::multiline(&mut app.addon_buf)
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );

        ui.add_space(10.0);
        ui.heading("Live TV");
        ui.label("Una fuente por línea:");
        ui.label("  m3u | Nombre | https://…/lista.m3u");
        ui.label("  xtream | Nombre | http://panel:8080 | usuario | contraseña");
        ui.add(
            egui::TextEdit::multiline(&mut app.live_buf)
                .desired_rows(4)
                .desired_width(f32::INFINITY),
        );

        ui.add_space(10.0);
        ui.heading("Reproductor");
        egui::Grid::new("player_grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("mpv");
                ui.add(
                    egui::TextEdit::singleline(&mut app.player_buf)
                        .hint_text("mpv")
                        .desired_width(320.0),
                );
                ui.end_row();
            });

        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if ui.button("Guardar ajustes").clicked() {
                app.save_settings();
                app.live_buf =
                    crate::app::format_live_sources(&crate::app::parse_live_sources(&app.live_buf));
            }
            if ui.button("Recargar").clicked() {
                let settings = app.store.load_settings();
                app.addon_buf = settings.addons.join("\n");
                app.live_buf = crate::app::format_live_sources(&settings.live_sources);
                app.host_addr_buf = settings.host_addr.clone().unwrap_or_default();
                app.host_token_buf = settings.host_token.clone().unwrap_or_default();
                app.host_command_buf = settings.host_command.clone().unwrap_or_default();
                app.player_buf = settings.player_command.clone().unwrap_or_default();
                app.settings = settings;
            }
        });

        ui.add_space(8.0);
        ui.small(format!(
            "ajustes en {}/settings.json",
            app.store.root().display()
        ));
    });
}

pub fn details_panel(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui) {
    let Some(details) = app.details.clone() else {
        return;
    };

    ui.horizontal(|ui| {
        ui.heading(&details.name);
        if ui.small_button("cerrar").clicked() {
            app.details = None;
            app.links.clear();
        }
    });
    if let Some(year) = details.year {
        ui.small(format!("{year}"));
    }
    if let Some(description) = &details.description {
        ui.label(description);
    }
    if let Some(poster) = &details.poster {
        ui.small(poster);
    }
    ui.separator();

    ui.label(format!("Fuentes ({})", app.links.len()));
    let mut chosen: Option<usize> = None;
    egui::ScrollArea::vertical()
        .max_height(220.0)
        .show(ui, |ui| {
            for (index, link) in app.links.iter().enumerate() {
                if ui.button(format!("▶ {}", link.label())).clicked() {
                    chosen = Some(index);
                }
            }
        });

    if !details.episodes.is_empty() {
        ui.separator();
        ui.label(format!("Episodios ({})", details.episodes.len()));
        let mut episode_url: Option<String> = None;
        egui::ScrollArea::vertical()
            .max_height(240.0)
            .show(ui, |ui| {
                for episode in &details.episodes {
                    if ui.button(episode.label()).clicked() {
                        episode_url = Some(episode.url.clone());
                    }
                }
            });
        if let Some(url) = episode_url {
            app.open_episode(ctx, url);
        }
    }

    if let Some(index) = chosen {
        if let Some(link) = app.links.get(index).cloned() {
            app.play(&link);
        }
    }
}

fn chrono_now() -> String {
    // Only used in a label; UTC is fine and keeps the dependency list short.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let hours = (now / 3600) % 24;
    let minutes = (now / 60) % 60;
    format!("{hours:02}:{minutes:02} UTC")
}
