//! The screen: a rail on the left, a search pill on top, rows of posters below.
//!
//! The layout follows Stremio: no hero, no status text, artwork first. Everything
//! else (Live TV, extensions, settings) is a plain list because it is worked through.

use crate::app::{App, Row, Tab};
use crate::theme;
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Rect, RichText, Sense, Vec2};
use streamgoat_core::live::filter_channels;
use streamgoat_core::model::{MediaItem, StreamLink};

pub(crate) fn nav(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui) {
    ui.add_space(14.0);
    for tab in Tab::ALL {
        let selected = app.tab == tab;
        let colour = if selected {
            theme::ACCENT
        } else {
            theme::MUTED
        };
        let response = ui.add(
            egui::Label::new(RichText::new(tab.title()).size(12.5).color(colour))
                .sense(Sense::click()),
        );
        ui.add_space(14.0);
        if response.clicked() {
            app.tab = tab;
        }
    }
    let _ = ctx;
}

pub(crate) fn top_bar(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        theme::logo(ui);
        ui.add_space(24.0);
        let search = ui.add(
            egui::TextEdit::singleline(&mut app.search)
                .hint_text("Buscar o pegar un enlace")
                .desired_width(380.0),
        );
        let enter = search.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if enter {
            app.tab = Tab::Search;
            app.search(ctx);
        }
        if app.busy > 0 {
            ui.spinner();
        }
    });
}

pub(crate) fn home_tab(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui) {
    let mut open: Option<MediaItem> = None;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if app.rows.is_empty() {
                ui.add_space(18.0);
                ui.label(RichText::new("Añade un add-on en Ajustes.").color(theme::MUTED));
            }
            for row in &app.rows {
                row_block(app, ui, row, &mut open);
            }
            ui.add_space(20.0);
        });

    if let Some(item) = open {
        app.open_item(ctx, item);
    }
}

fn row_block(app: &App, ui: &mut egui::Ui, row: &Row, open: &mut Option<MediaItem>) {
    if row.items.is_empty() {
        return;
    }
    ui.add_space(18.0);
    ui.label(RichText::new(&row.title).size(19.0).strong());
    ui.add_space(6.0);
    egui::ScrollArea::horizontal()
        .id_salt(&row.title)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 11.0;
                for item in &row.items {
                    let texture = item.poster.as_ref().and_then(|url| app.posters.get(url));
                    if theme::poster_card(ui, texture, &item.name).clicked() {
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
                ui.add_space(18.0);
                ui.label(RichText::new("Sin resultados.").color(theme::MUTED));
                return;
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(11.0, 11.0);
                for item in &app.items {
                    let texture = item.poster.as_ref().and_then(|url| app.posters.get(url));
                    if theme::poster_card(ui, texture, &item.name).clicked() {
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
            ui.label(RichText::new("Añade una lista M3U en Ajustes.").color(theme::MUTED));
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
            if ui.button("Cargar").clicked() {
                app.channels.clear();
                app.guide = None;
                app.load_live(ctx);
            }
        }

        ui.add_space(12.0);
        ui.add(
            egui::TextEdit::singleline(&mut app.channel_filter)
                .hint_text("Filtrar")
                .desired_width(180.0),
        );
    });

    let filter = app.channel_filter.clone();
    let visible = filter_channels(&app.channels, &filter);
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
        });

    if let Some(url) = clicked_url {
        if let Some(channel) = app.channels.iter().find(|channel| channel.url == url) {
            let mut link = StreamLink::new(&channel.url);
            link.name = Some(channel.name.clone());
            link.headers = channel.headers.clone();
            app.play(&link);
        }
    }
}

pub(crate) fn extensions_tab(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.repo_buf)
                .hint_text("url de la repo")
                .desired_width(320.0),
        );
        if ui.button("Añadir").clicked() {
            app.add_repo(ctx);
        }
        ui.add_space(12.0);
        if ui.button("Recargar").clicked() {
            app.load_extensions(ctx);
        }
        if ui.button("Actualizar").clicked() {
            app.update_extensions(ctx);
        }
        ui.add_space(12.0);
        ui.add(
            egui::TextEdit::singleline(&mut app.ext_filter)
                .hint_text("Filtrar")
                .desired_width(160.0),
        );
    });

    ui.add_space(6.0);
    let mut remove: Option<String> = None;
    ui.horizontal_wrapped(|ui| {
        for repo in &app.repos {
            let response = ui
                .add(
                    egui::Button::new(RichText::new(&repo.name).small())
                        .fill(theme::CARD)
                        .corner_radius(CornerRadius::same(4)),
                )
                .on_hover_text("quitar");
            if response.clicked() {
                remove = Some(repo.url.clone());
            }
        }
    });
    if let Some(url) = remove {
        app.remove_repo(ctx, url);
    }

    let needle = app.ext_filter.trim().to_ascii_lowercase();
    let mut action: Option<(bool, String)> = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for plugin in &app.available {
                if !needle.is_empty()
                    && !plugin.name.to_ascii_lowercase().contains(&needle)
                    && !plugin.internal_name.to_ascii_lowercase().contains(&needle)
                {
                    continue;
                }
                ui.horizontal(|ui| {
                    ui.label(&plugin.name);
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
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let installed = plugin.is_installed();
                        let outdated = plugin.update_available();
                        let update =
                            ui.add_enabled(installed && outdated, egui::Button::new("Actualizar"));
                        let remove = ui.add_enabled(installed, egui::Button::new("Quitar"));
                        let install = ui.add_enabled(!installed, egui::Button::new("Instalar"));
                        if update.clicked() {
                            action = Some((true, plugin.internal_name.clone()));
                        } else if remove.clicked() {
                            action = Some((false, plugin.internal_name.clone()));
                        } else if install.clicked() {
                            action = Some((true, plugin.internal_name.clone()));
                        }
                    });
                });
            }
        });

    if let Some((install, internal_name)) = action {
        if install {
            app.install_plugin(ctx, internal_name);
        } else {
            app.uninstall_plugin(ctx, internal_name);
        }
    }
}

pub(crate) fn settings_tab(app: &mut App, _ctx: &egui::Context, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.label(RichText::new("Host").size(17.0).strong());
            egui::Grid::new("host_grid").num_columns(2).show(ui, |ui| {
                ui.label("Dirección");
                ui.add(
                    egui::TextEdit::singleline(&mut app.host_addr_buf)
                        .hint_text("127.0.0.1:9000")
                        .desired_width(300.0),
                );
                ui.end_row();
                ui.label("Token");
                ui.add(
                    egui::TextEdit::singleline(&mut app.host_token_buf)
                        .password(true)
                        .desired_width(300.0),
                );
                ui.end_row();
            });

            ui.add_space(14.0);
            ui.label(RichText::new("Add-ons").size(17.0).strong());
            ui.add(
                egui::TextEdit::multiline(&mut app.addon_buf)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );

            ui.add_space(14.0);
            ui.label(RichText::new("Live TV").size(17.0).strong());
            ui.add(
                egui::TextEdit::multiline(&mut app.live_buf)
                    .desired_rows(4)
                    .desired_width(f32::INFINITY),
            );

            ui.add_space(14.0);
            ui.label(RichText::new("Reproductor").size(17.0).strong());
            ui.add(
                egui::TextEdit::singleline(&mut app.player_buf)
                    .hint_text("mpv")
                    .desired_width(300.0),
            );

            ui.add_space(16.0);
            ui.horizontal(|ui| {
                if theme::primary_button(ui, "Guardar").clicked() {
                    app.save_settings();
                    app.live_buf = crate::app::format_live_sources(
                        &crate::app::parse_live_sources(&app.live_buf),
                    );
                }
                if ui.button("Descartar cambios").clicked() {
                    let settings = app.store.load_settings();
                    app.addon_buf = settings.addons.join("\n");
                    app.live_buf = crate::app::format_live_sources(&settings.live_sources);
                    app.host_addr_buf = settings.host_addr.clone().unwrap_or_default();
                    app.host_token_buf = settings.host_token.clone().unwrap_or_default();
                    app.player_buf = settings.player_command.clone().unwrap_or_default();
                    app.settings = settings;
                }
            });
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
            if let Some(texture) = details.poster.as_ref().and_then(|url| app.posters.get(url)) {
                ui.add(
                    egui::Image::new(egui::load::SizedTexture::new(
                        texture.id(),
                        Vec2::new(180.0, 256.0),
                    ))
                    .corner_radius(CornerRadius::same(6)),
                );
                ui.add_space(8.0);
            }

            ui.horizontal(|ui| {
                ui.label(RichText::new(&details.name).size(19.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Cerrar").clicked() {
                        close = true;
                    }
                });
            });

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

            ui.add_space(10.0);
            for (index, link) in app.links.iter().enumerate() {
                if ui.button(format!("▶ {}", link.label())).clicked() {
                    chosen = Some(index);
                }
            }

            if !details.episodes.is_empty() {
                ui.add_space(10.0);
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
