//! Look of the app: dark surface, red accent, poster cards.
//!
//! Cards are painted by hand instead of composed from widgets: it keeps the rows
//! tight and avoids depending on the exact `Frame` API of the egui version.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, Vec2};

pub(crate) const BG: Color32 = Color32::from_rgb(10, 10, 12);
pub(crate) const SURFACE: Color32 = Color32::from_rgb(18, 18, 21);
pub(crate) const CARD: Color32 = Color32::from_rgb(28, 28, 33);
pub(crate) const ACCENT: Color32 = Color32::from_rgb(229, 9, 20);
pub(crate) const TEXT: Color32 = Color32::from_rgb(244, 244, 245);
pub(crate) const MUTED: Color32 = Color32::from_rgb(150, 150, 158);
pub(crate) const BORDER: Color32 = Color32::from_rgb(38, 38, 44);

/// Poster card size, and the taller one used by the first row.
pub(crate) const CARD_SIZE: Vec2 = Vec2::new(132.0, 198.0);

/// Room under a poster for its title and year.
const CAPTION_HEIGHT: f32 = 30.0;

pub(crate) fn install(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    style.visuals.dark_mode = true;
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = SURFACE;
    style.visuals.extreme_bg_color = Color32::from_rgb(8, 8, 10);
    style.visuals.faint_bg_color = SURFACE;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.window_stroke = Stroke::new(1.0_f32, BORDER);

    {
        let widgets = &mut style.visuals.widgets;
        widgets.noninteractive.bg_fill = SURFACE;
        widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
        widgets.inactive.bg_fill = CARD;
        widgets.inactive.weak_bg_fill = CARD;
        widgets.hovered.bg_fill = Color32::from_rgb(44, 44, 50);
        widgets.hovered.weak_bg_fill = Color32::from_rgb(44, 44, 50);
        widgets.active.bg_fill = ACCENT;
        widgets.active.weak_bg_fill = ACCENT;
        for widget in [
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
            &mut widgets.noninteractive,
            &mut widgets.open,
        ] {
            widget.corner_radius = CornerRadius::same(6);
        }
    }
    style.visuals.selection.bg_fill = ACCENT;
    style.visuals.selection.stroke = Stroke::new(1.0_f32, TEXT);

    style.spacing.item_spacing = Vec2::new(10.0, 8.0);
    style.spacing.button_padding = Vec2::new(14.0, 7.0);
    style.spacing.scroll = egui::style::ScrollStyle::solid();

    use egui::{FontFamily::Proportional, FontId as F, TextStyle::*};
    style.text_styles = [
        (Heading, F::new(24.0, Proportional)),
        (Body, F::new(15.0, Proportional)),
        (Monospace, F::new(13.0, egui::FontFamily::Monospace)),
        (Button, F::new(14.0, Proportional)),
        (Small, F::new(12.0, Proportional)),
    ]
    .into();

    ctx.set_style(style);
}

/// The logo, as the top bar draws it.
pub(crate) fn logo(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("STREAM")
                .size(22.0)
                .strong()
                .color(ACCENT),
        );
        ui.label(egui::RichText::new("GOAT").size(22.0).strong().color(TEXT));
    });
}

/// A clickable poster with its title underneath, Netflix style.
///
/// Returns the response so the caller decides what a click means.
pub(crate) fn poster_card(
    ui: &mut egui::Ui,
    texture: Option<&egui::TextureHandle>,
    title: &str,
    subtitle: Option<&str>,
) -> egui::Response {
    let (outer, response) =
        ui.allocate_exact_size(CARD_SIZE + Vec2::new(0.0, CAPTION_HEIGHT), Sense::click());
    let art = Rect::from_min_size(outer.min, CARD_SIZE);
    let painter = ui.painter();

    painter.rect_filled(art, CornerRadius::same(6), CARD);
    if let Some(texture) = texture {
        painter.image(
            texture.id(),
            art,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }

    if response.hovered() {
        painter.rect_filled(
            art,
            CornerRadius::same(6),
            Color32::from_rgba_unmultiplied(0, 0, 0, 100),
        );
        painter.rect_stroke(
            art,
            CornerRadius::same(6),
            Stroke::new(1.5_f32, TEXT),
            egui::StrokeKind::Inside,
        );
        painter.text(
            art.center(),
            Align2::CENTER_CENTER,
            "▶",
            FontId::proportional(26.0),
            TEXT,
        );
    } else if texture.is_none() {
        painter.text(
            art.center(),
            Align2::CENTER_CENTER,
            "…",
            FontId::proportional(16.0),
            MUTED,
        );
    }

    painter.text(
        egui::pos2(outer.left(), art.bottom() + 4.0),
        Align2::LEFT_TOP,
        fit(title, 20),
        FontId::proportional(12.0),
        if response.hovered() { TEXT } else { MUTED },
    );
    if let Some(subtitle) = subtitle {
        painter.text(
            egui::pos2(outer.left(), art.bottom() + 18.0),
            Align2::LEFT_TOP,
            subtitle,
            FontId::proportional(10.5),
            Color32::from_rgb(110, 110, 118),
        );
    }

    response
}

/// The small red button the hero uses for the main action.
pub(crate) fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(text).strong().color(TEXT))
            .fill(ACCENT)
            .corner_radius(CornerRadius::same(4))
            .min_size(Vec2::new(120.0, 34.0)),
    )
}

/// Cuts a title to what fits under a poster.
fn fit(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    cut.push('…');
    cut
}
