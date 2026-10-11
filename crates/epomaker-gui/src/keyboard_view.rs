//! Draws a keyboard from the model's key geometry and reports clicks.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use epomaker_core::devices::ModelInfo;

pub struct KeyStyle {
    pub fill: Color32,
    pub text: String,
    pub text_color: Color32,
    pub highlight: bool,
    /// Shown when hovering the key.
    pub tooltip: String,
}

pub struct ViewResponse {
    /// Matrix index of the key clicked this frame.
    pub clicked: Option<usize>,
    /// Matrix index under a pressed pointer (for painting by dragging).
    pub dragged_over: Option<usize>,
}

/// Draws every key of `model`; `style(matrix_index, label)` decides its look.
pub fn show(ui: &mut egui::Ui, model: &ModelInfo, mut style: impl FnMut(usize, &str) -> KeyStyle) -> ViewResponse {
    let keys = &model.keys;
    if keys.is_empty() {
        ui.label("(no layout for this model)");
        return ViewResponse { clicked: None, dragged_over: None };
    }
    // vendor geometry: x to the right, y grows upwards (rows have negative y)
    let min_x = keys.iter().map(|k| k.x).fold(f32::MAX, f32::min);
    let max_x = keys.iter().map(|k| k.x + k.w).fold(f32::MIN, f32::max);
    let min_top = keys.iter().map(|k| -k.y).fold(f32::MAX, f32::min);
    let max_bottom = keys.iter().map(|k| -k.y + k.h).fold(f32::MIN, f32::max);
    let (w, h) = (max_x - min_x, max_bottom - min_top);
    let pad = 8.0;
    let avail = ui.available_width().min(1100.0) - 2.0 * pad - 2.0;
    let scale = (avail / w).max(0.5);
    let (response, painter) =
        ui.allocate_painter(Vec2::new(w * scale + 2.0 * pad, h * scale + 2.0 * pad), Sense::click_and_drag());
    let origin = response.rect.min + Vec2::splat(pad);
    painter.rect_filled(response.rect, CornerRadius::same(10), ui.visuals().extreme_bg_color);

    let pointer = response.interact_pointer_pos().or(response.hover_pos());
    let mut hovered = None;
    let mut tooltip = String::new();
    for k in keys {
        let Some(idx) = k.matrix_index else { continue };
        let rect = Rect::from_min_size(
            Pos2::new(origin.x + (k.x - min_x) * scale, origin.y + (-k.y - min_top) * scale),
            Vec2::new(k.w * scale, k.h * scale),
        )
        .shrink(1.5);
        let s = style(idx, &k.label);
        let is_hover = pointer.is_some_and(|p| rect.contains(p));
        if is_hover {
            hovered = Some(idx);
            tooltip = s.tooltip.clone();
        }
        painter.rect_filled(rect, CornerRadius::same(4), s.fill);
        let stroke = if s.highlight {
            Stroke::new(2.5, ui.visuals().selection.stroke.color)
        } else if is_hover {
            Stroke::new(1.5, ui.visuals().widgets.hovered.fg_stroke.color)
        } else {
            Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)
        };
        painter.rect_stroke(rect, CornerRadius::same(4), stroke, StrokeKind::Inside);
        let font = FontId::proportional((10.5 * scale).clamp(8.0, 14.0));
        let text = fit(&s.text, rect.width(), font.size);
        painter.text(rect.center(), Align2::CENTER_CENTER, text, font, s.text_color);
    }
    let response =
        if hovered.is_some() && !tooltip.is_empty() { response.on_hover_text_at_pointer(tooltip) } else { response };
    let clicked = if response.clicked() { hovered } else { None };
    let dragged_over = if response.dragged() || response.is_pointer_button_down_on() { hovered } else { None };
    ViewResponse { clicked, dragged_over }
}

/// Shortens a label so that it roughly fits in `width` pixels.
fn fit(text: &str, width: f32, font_size: f32) -> String {
    let max_chars = ((width / (font_size * 0.55)).floor() as usize).max(1);
    text.lines()
        .map(|l| {
            if l.chars().count() > max_chars {
                l.chars().take(max_chars.saturating_sub(1).max(1)).collect::<String>() + "…"
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Readable text color over `bg`.
pub fn contrast(bg: Color32) -> Color32 {
    let l = 0.299 * bg.r() as f32 + 0.587 * bg.g() as f32 + 0.114 * bg.b() as f32;
    if l > 140.0 {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}
