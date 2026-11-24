//! Look of the app: dark surface, red accent, poster cards.
//!
//! Cards are painted by hand instead of composed from widgets: it keeps the rows
//! tight and avoids depending on the exact `Frame` API of the egui version.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, Vec2};

pub(crate) const BG: Color32 = Color32::from_rgb(15, 17, 32);
pub(crate) const SURFACE: Color32 = Color32::from_rgb(24, 26, 44);
pub(crate) const CARD: Color32 = Color32::from_rgb(32, 34, 56);
pub(crate) const ACCENT: Color32 = Color32::from_rgb(124, 92, 255);
pub(crate) const TEXT: Color32 = Color32::from_rgb(244, 244, 245);
pub(crate) const MUTED: Color32 = Color32::from_rgb(150, 150, 158);
pub(crate) const BORDER: Color32 = Color32::from_rgb(44, 47, 78);

/// Poster card size, and the taller one used by the first row.
pub(crate) const CARD_SIZE: Vec2 = Vec2::new(137.0, 195.0);

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
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(CARD_SIZE, Sense::click());
    let painter = ui.painter();

    painter.rect_filled(rect, CornerRadius::same(8), CARD);
    if let Some(texture) = texture {
        painter.image(
            texture.id(),
            rect,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }

    if response.hovered() {
        let strip = Rect::from_min_max(
            egui::pos2(rect.left(), rect.bottom() - 46.0),
            rect.right_bottom(),
        );
        painter.rect_filled(
            strip,
            CornerRadius::same(8),
            Color32::from_rgba_unmultiplied(8, 9, 18, 210),
        );
        painter.text(
            egui::pos2(rect.left() + 10.0, rect.bottom() - 10.0),
            Align2::LEFT_BOTTOM,
            fit(title, 19),
            FontId::proportional(12.5),
            TEXT,
        );
        painter.rect_stroke(
            rect,
            CornerRadius::same(8),
            Stroke::new(1.0_f32, ACCENT),
            egui::StrokeKind::Inside,
        );
    } else if texture.is_none() {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "…",
            FontId::proportional(16.0),
            MUTED,
        );
    }

    response
}

/// The main action of a form.
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
