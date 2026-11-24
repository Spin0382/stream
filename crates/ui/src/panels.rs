//! The screen: a top bar, a hero and horizontal rows of posters.
//!
//! Dark surface, red accent, artwork first. Live TV, extensions and settings keep
//! plain lists, because they are worked through rather than browsed.

use crate::app::{App, Row, Tab};
use crate::theme;
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Rect, RichText, Sense, Vec2};
use streamgoat_core::live::filter_channels;
use streamgoat_core::model::{MediaItem, StreamLink};

pub(crate) fn top_bar(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        theme::logo(ui);
        ui.add_space(18.0);
        for tab in Tab::ALL {
            let selected = app.tab == tab;
            let text = RichText::new(tab.title()).size(14.0).color(if selected {
                theme::TEXT
            } else {
                theme::MUTED
            });
            let response = ui.add(egui::Label::new(text).sense(Sense::click()));
            if selected {
                ui.painter().line_segment(
                    [
                        egui::pos2(response.rect.left(), response.rect.bottom() + 2.0),
                        egui::pos2(response.rect.right(), response.rect.bottom() + 2.0),
                    ],
                    egui::Stroke::new(2.0_f32, theme::ACCENT),
                );
            }
            if response.clicked() {
                app.tab = tab;
            }
        }
        ui.add_space(18.0);

        let search = ui.add(
            egui::TextEdit::singleline(&mut app.search)
                .hint_text("Buscar")
                .desired_width(220.0),
        );
        let enter = search.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if enter {
            app.tab = Tab::Search;
            app.search(ctx);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if app.busy > 0 {
                ui.spinner();
            } else {
                ui.label(RichText::new(&app.status).small().color(theme::MUTED));
            }
            let connected = !app.providers.is_empty();
            let colour = if connected {
                theme::ACCENT
            } else {
                theme::MUTED
            };
            let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), Sense::hover());
            ui.painter().circle_filled(rect.center(), 4.0, colour);
            if ui
                .add(
                    egui::Label::new(RichText::new("Host").small().color(theme::MUTED))
                        .sense(Sense::click()),
                )
                .on_hover_text(if connected {
                    "host conectado"
                } else {
                    "pulsa para conectar"
                })
                .clicked()
            {
                app.connect(ctx);
            }
        });
    });
}

pub(crate) fn home_tab(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui) {
    let mut open: Option<MediaItem> = None;

    ui.horizontal(|ui| {
        if app.providers.is_empty() {
            ui.label(
                RichText::new("Conecta el host para leer los providers")
                    .small()
                    .color(theme::MUTED),
            );
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
                .unwrap_or_else(|| "Provider".to_string());
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
            if ui.button("Cargar").clicked() {
                if let Some(provider) = app.provider.clone() {
                    app.load_home(ctx, provider);
                }
            }
        }
        if ui.button("Add-ons").clicked() {
            app.load_addons(ctx);
        }
    });

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if let Some(hero) = app.hero.clone() {
                hero_block(app, ui, &hero, &mut open);
            } else {
                ui.add_space(24.0);
                ui.label(
                    RichText::new("Carga los add-ons de Stremio desde Ajustes y pulsa Cargar.")
                        .color(theme::MUTED),
                );
            }

            for row in &app.rows {
                row_block(app, ui, row, &mut open);
            }
            ui.add_space(24.0);
        });

    if let Some(item) = open {
        app.open_item(ctx, item);
    }
}

fn hero_block(app: &App, ui: &mut egui::Ui, hero: &MediaItem, open: &mut Option<MediaItem>) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 280.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(10), theme::SURFACE);

    let artwork = hero.background.as_ref().or(hero.poster.as_ref());
    if let Some(texture) = artwork.and_then(|url| app.posters.get(url)) {
        painter.image(
            texture.id(),
            rect,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }

    // Two scrims: a light one over the artwork, a darker one under the text.
    painter.rect_filled(
        rect,
        CornerRadius::same(10),
        Color32::from_rgba_unmultiplied(8, 8, 10, 120),
    );
    let bottom = Rect::from_min_max(
        egui::pos2(rect.left(), rect.bottom() - 130.0),
        rect.right_bottom(),
    );
    painter.rect_filled(
        bottom,
        CornerRadius::same(10),
        Color32::from_rgba_unmultiplied(8, 8, 10, 165),
    );

    let left = rect.left() + 28.0;
    painter.text(
        egui::pos2(left, rect.bottom() - 84.0),
        Align2::LEFT_BOTTOM,
        &hero.name,
        FontId::proportional(30.0),
        theme::TEXT,
    );
    let mut meta: Vec<String> = Vec::new();
    if let Some(year) = hero.year {
        meta.push(year.to_string());
    }
    if let Some(provider) = &hero.provider {
        meta.push(provider.clone());
    }
    if !meta.is_empty() {
        painter.text(
            egui::pos2(left, rect.bottom() - 76.0),
            Align2::LEFT_TOP,
            meta.join(" · "),
            FontId::proportional(12.0),
            theme::MUTED,
        );
    }

    let button = Rect::from_min_size(
        egui::pos2(left, rect.bottom() - 52.0),
        Vec2::new(140.0, 34.0),
    );
    let response = ui.put(
        button,
        egui::Button::new(RichText::new("Ver detalles").strong().color(theme::TEXT))
            .fill(theme::ACCENT)
            .corner_radius(CornerRadius::same(4)),
    );
    if response.clicked() {
        *open = Some(hero.clone());
    }
}

fn row_block(app: &App, ui: &mut egui::Ui, row: &Row, open: &mut Option<MediaItem>) {
    if row.items.is_empty() {
        return;
    }
    ui.add_space(20.0);
    ui.label(RichText::new(&row.title).size(17.0).strong());
    ui.add_space(2.0);
    egui::ScrollArea::horizontal()
        .id_salt(&row.title)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for item in &row.items {
                    let texture = item.poster.as_ref().and_then(|url| app.posters.get(url));
                    let year = item.year.map(|year| year.to_string());
                    let response = theme::poster_card(ui, texture, &item.name, year.as_deref());
                    if response.clicked() {
                        *open = Some(item.clone());
                    }
                }
            });
        });
}

pub(crate) fn search_tab(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui) {
    let mut open: Option<MediaItem> = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if app.items.is_empty() {
                ui.add_space(24.0);
                ui.label(
                    RichText::new("Busca arriba para llenar esta pantalla.").color(theme::MUTED),
                );
                return;
            }
            ui.horizontal_wrapped(|ui| {
                for item in &app.items {
                    let texture = item.poster.as_ref().and_then(|url| app.posters.get(url));
                    let year = item.year.map(|year| year.to_string());
                    let response = theme::poster_card(ui, texture, &item.name, year.as_deref());
                    if response.clicked() {
                        open = Some(item.clone());
                    }
                }
            });
        });

    if let Some(item) = open {
        app.open_item(ctx, item);
    }
}

pub(crate) fn live_tab(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let names: Vec<String> = app
            .settings
            .live_sources
            .iter()
            .map(|source| source.name().to_string())
            .collect();

        if names.is_empty() {
            ui.label(
                RichText::new("Añade una lista M3U o un login Xtream en Ajustes.")
                    .color(theme::MUTED),
            );
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

        ui.add_space(12.0);
        ui.add(
            egui::TextEdit::singleline(&mut app.channel_filter)
                .hint_text("Filtrar canales")
                .desired_width(200.0),
        );
    });

    let filter = app.channel_filter.clone();
    let visible = filter_channels(&app.channels, &filter);
    let now = chrono_now();
    let mut clicked_url: Option<String> = None;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
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

                let (rect, response) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
                let painter = ui.painter();
                if response.hovered() {
                    painter.rect_filled(rect, CornerRadius::same(4), theme::CARD);
                }
                painter.text(
                    egui::pos2(rect.left() + 8.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    &channel.name,
                    FontId::proportional(14.0),
                    theme::TEXT,
                );
                painter.text(
                    egui::pos2(rect.right() - 8.0, rect.center().y),
                    Align2::RIGHT_CENTER,
                    if programme.is_empty() {
                        channel.group.clone().unwrap_or_default()
                    } else {
                        programme
                    },
                    FontId::proportional(12.0),
                    theme::MUTED,
                );
                if response.clicked() {
                    clicked_url = Some(channel.url.clone());
                }
            }
            if visible.is_empty() && !app.channels.is_empty() {
                ui.label(RichText::new("Ningún canal coincide.").color(theme::MUTED));
            }
        });

    if let Some(url) = clicked_url {
        if let Some(channel) = app.channels.iter().find(|channel| channel.url == url) {
            let mut link = StreamLink::new(&channel.url);
            link.name = Some(channel.name.clone());
            link.headers = channel.headers.clone();
            app.play(&link);
        }
    }

    if let Some(guide) = app.guide.as_ref() {
        ui.label(
            RichText::new(format!(
                "{} canales · {} programas · {}",
                guide.channels.len(),
                guide.programmes.len(),
                now
            ))
            .small()
            .color(theme::MUTED),
        );
    }
}

pub(crate) fn extensions_tab(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        if ui.button("Recargar").clicked() {
            app.load_extensions(ctx);
        }
        if ui.button("Actualizar todas").clicked() {
            app.update_extensions(ctx);
        }
        ui.add_space(12.0);
        ui.add(
            egui::TextEdit::singleline(&mut app.ext_filter)
                .hint_text("Filtrar extensiones")
                .desired_width(200.0),
        );
    });

    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(260.0);
            ui.label(RichText::new("Repositorios").strong());
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut app.repo_buf)
                        .hint_text("url o código corto")
                        .desired_width(170.0),
                );
                if ui.button("Añadir").clicked() {
                    app.add_repo(ctx);
                }
            });
            let mut remove = None;
            egui::ScrollArea::vertical()
                .id_salt("repos")
                .max_height(ui.available_height())
                .show(ui, |ui| {
                    if app.repos.is_empty() {
                        ui.label(RichText::new("Sin repositorios todavía.").color(theme::MUTED));
                    }
                    for repo in &app.repos {
                        ui.horizontal(|ui| {
                            if ui.small_button("✕").on_hover_text("quitar").clicked() {
                                remove = Some(repo.url.clone());
                            }
                            ui.label(&repo.name);
                        });
                        ui.label(RichText::new(&repo.url).small().color(theme::MUTED));
                        ui.add_space(4.0);
                    }
                });
            if let Some(url) = remove {
                app.remove_repo(ctx, url);
            }
        });

        ui.add_space(12.0);

        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Extensiones").strong());
                ui.label(
                    RichText::new(format!(
                        "{} · {} add-ons",
                        app.available.len(),
                        app.addons.len()
                    ))
                    .small()
                    .color(theme::MUTED),
                );
            });
            let needle = app.ext_filter.trim().to_ascii_lowercase();
            let mut action: Option<(bool, String)> = None;
            egui::ScrollArea::vertical()
                .id_salt("available")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let mut shown = 0usize;
                    for plugin in &app.available {
                        if !needle.is_empty()
                            && !plugin.name.to_ascii_lowercase().contains(&needle)
                            && !plugin.internal_name.to_ascii_lowercase().contains(&needle)
                        {
                            continue;
                        }
                        shown += 1;
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&plugin.name).strong());
                                    ui.label(
                                        RichText::new(format!("v{}", plugin.version))
                                            .small()
                                            .color(theme::MUTED),
                                    );
                                    ui.label(
                                        RichText::new(plugin.status_label())
                                            .small()
                                            .color(theme::MUTED),
                                    );
                                    if let Some(language) = &plugin.language {
                                        ui.label(
                                            RichText::new(language).small().color(theme::MUTED),
                                        );
                                    }
                                });
                                if let Some(description) = &plugin.description {
                                    ui.label(
                                        RichText::new(description).small().color(theme::MUTED),
                                    );
                                }
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let installed = plugin.is_installed();
                                    let outdated = plugin.update_available();
                                    let update = ui.add_enabled(
                                        installed && outdated,
                                        egui::Button::new("Actualizar"),
                                    );
                                    let remove =
                                        ui.add_enabled(installed, egui::Button::new("Quitar"));
                                    let install =
                                        ui.add_enabled(!installed, egui::Button::new("Instalar"));
                                    if update.clicked() {
                                        action = Some((true, plugin.internal_name.clone()));
                                    } else if remove.clicked() {
                                        action = Some((false, plugin.internal_name.clone()));
                                    } else if install.clicked() {
                                        action = Some((true, plugin.internal_name.clone()));
                                    }
                                },
                            );
                        });
                        ui.separator();
                    }
                    if shown == 0 {
                        ui.label(
                            RichText::new("Añade un repositorio y recarga para ver extensiones.")
                                .color(theme::MUTED),
                        );
                    }
                });
            if let Some((install, internal_name)) = action {
                if install {
                    app.install_plugin(ctx, internal_name);
                } else {
                    app.uninstall_plugin(ctx, internal_name);
                }
            }
        });
    });
}

pub(crate) fn settings_tab(app: &mut App, _ctx: &egui::Context, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.label(RichText::new("Host de extensiones").size(18.0).strong());
            ui.label(
                RichText::new(
                    "El host es un proceso Java aparte; aquí solo se le dice dónde escucha.",
                )
                .small()
                .color(theme::MUTED),
            );
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

            ui.add_space(16.0);
            ui.label(RichText::new("Stremio").size(18.0).strong());
            ui.label(
                RichText::new("Un add-on por línea: https://…/manifest.json")
                    .small()
                    .color(theme::MUTED),
            );
            ui.add(
                egui::TextEdit::multiline(&mut app.addon_buf)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );

            ui.add_space(16.0);
            ui.label(RichText::new("Live TV").size(18.0).strong());
            ui.label(
                RichText::new("Una fuente por línea:")
                    .small()
                    .color(theme::MUTED),
            );
            ui.label(
                RichText::new("  m3u | Nombre | https://…/lista.m3u")
                    .small()
                    .color(theme::MUTED),
            );
            ui.label(
                RichText::new("  xtream | Nombre | http://panel:8080 | usuario | contraseña")
                    .small()
                    .color(theme::MUTED),
            );
            ui.add(
                egui::TextEdit::multiline(&mut app.live_buf)
                    .desired_rows(4)
                    .desired_width(f32::INFINITY),
            );

            ui.add_space(16.0);
            ui.label(RichText::new("Reproductor").size(18.0).strong());
            ui.add(
                egui::TextEdit::singleline(&mut app.player_buf)
                    .hint_text("mpv")
                    .desired_width(320.0),
            );

            ui.add_space(18.0);
            ui.horizontal(|ui| {
                if theme::primary_button(ui, "Guardar").clicked() {
                    app.save_settings();
                    app.live_buf = crate::app::format_live_sources(
                        &crate::app::parse_live_sources(&app.live_buf),
                    );
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
            ui.label(
                RichText::new(format!(
                    "ajustes en {}/settings.json",
                    app.store.root().display()
                ))
                .small()
                .color(theme::MUTED),
            );
            ui.add_space(24.0);
        });
}

pub(crate) fn details_panel(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui) {
    let Some(details) = app.details.clone() else {
        return;
    };
    if let Some(poster) = details.poster.clone() {
        app.request_poster(ctx, &poster);
    }

    let mut close = false;
    let mut chosen: Option<usize> = None;
    let mut episode: Option<String> = None;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&details.name).size(20.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Cerrar").clicked() {
                        close = true;
                    }
                });
            });

            if let Some(texture) = details.poster.as_ref().and_then(|url| app.posters.get(url)) {
                ui.add_space(8.0);
                ui.add(
                    egui::Image::new(egui::load::SizedTexture::new(
                        texture.id(),
                        Vec2::new(200.0, 300.0),
                    ))
                    .corner_radius(CornerRadius::same(6)),
                );
            }

            ui.add_space(6.0);
            let mut meta: Vec<String> = Vec::new();
            if let Some(year) = details.year {
                meta.push(year.to_string());
            }
            if let Some(source) = &details.source {
                meta.push(source.clone());
            }
            if !meta.is_empty() {
                ui.label(RichText::new(meta.join(" · ")).small().color(theme::MUTED));
            }
            if let Some(description) = &details.description {
                ui.add_space(4.0);
                ui.label(description);
            }

            ui.add_space(12.0);
            ui.label(RichText::new(format!("Fuentes · {}", app.links.len())).strong());
            if app.links.is_empty() {
                ui.label(
                    RichText::new("Cargando fuentes…")
                        .small()
                        .color(theme::MUTED),
                );
            }
            for (index, link) in app.links.iter().enumerate() {
                if ui.button(format!("▶ {}", link.label())).clicked() {
                    chosen = Some(index);
                }
            }

            if !details.episodes.is_empty() {
                ui.add_space(12.0);
                ui.label(RichText::new(format!("Episodios · {}", details.episodes.len())).strong());
                for episode_item in &details.episodes {
                    if ui.button(episode_item.label()).clicked() {
                        episode = Some(episode_item.url.clone());
                    }
                }
            }
            ui.add_space(20.0);
        });

    if close {
        app.details = None;
        app.links.clear();
        return;
    }
    if let Some(url) = episode {
        app.open_episode(ctx, url);
    }
    if let Some(index) = chosen {
        if let Some(link) = app.links.get(index).cloned() {
            app.play(&link);
        }
    }
}

fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let hours = (now / 3600) % 24;
    let minutes = (now / 60) % 60;
    format!("{hours:02}:{minutes:02} UTC")
}
