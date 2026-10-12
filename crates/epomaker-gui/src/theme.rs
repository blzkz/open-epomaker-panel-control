//! Look and feel: fonts, colors and the few widgets every tab shares (cards, switches,
//! segmented buttons, sidebar entries), in the spirit of current gaming peripheral apps.

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Response, RichText, Sense,
    Shadow, Stroke, StrokeKind, TextStyle, Theme, Vec2,
};
pub use egui_phosphor::regular as icon;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

/// Highlight color, chosen in the preferences.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Accent {
    #[default]
    Cyan,
    Green,
    Violet,
    Orange,
    Red,
}

impl Accent {
    pub const ALL: [Accent; 5] = [Accent::Cyan, Accent::Green, Accent::Violet, Accent::Orange, Accent::Red];

    pub fn label(self) -> &'static str {
        match self {
            Accent::Cyan => "Cyan",
            Accent::Green => "Green",
            Accent::Violet => "Violet",
            Accent::Orange => "Orange",
            Accent::Red => "Red",
        }
    }

    pub fn color(self) -> Color32 {
        match self {
            Accent::Cyan => Color32::from_rgb(0x12, 0xB4, 0xF5),
            Accent::Green => Color32::from_rgb(0x3D, 0xD6, 0x5A),
            Accent::Violet => Color32::from_rgb(0x9B, 0x6B, 0xFF),
            Accent::Orange => Color32::from_rgb(0xFF, 0x8A, 0x1F),
            Accent::Red => Color32::from_rgb(0xFF, 0x3D, 0x5E),
        }
    }

    fn from_u8(v: u8) -> Accent {
        Accent::ALL.get(v as usize).copied().unwrap_or_default()
    }
}

static ACCENT: AtomicU8 = AtomicU8::new(0);

/// Colors of the current theme.
#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    /// Main content background.
    pub bg: Color32,
    /// Sidebar and status bar.
    pub sidebar: Color32,
    /// Header with the device.
    pub header: Color32,
    pub card: Color32,
    /// Sunken areas (keyboard case, meters, text fields).
    pub inset: Color32,
    pub control: Color32,
    pub control_hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_strong: Color32,
    pub text_dim: Color32,
    pub accent: Color32,
    /// Accent dark enough to carry white text.
    pub accent_fill: Color32,
    /// Text drawn on `accent_fill`.
    pub on_accent: Color32,
    pub ok: Color32,
    pub warn: Color32,
    pub error: Color32,
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

impl Palette {
    pub fn new(dark: bool, accent: Accent) -> Palette {
        let a = accent.color();
        let hex = |v: u32| Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8);
        if dark {
            Palette {
                dark,
                bg: hex(0x121419),
                sidebar: hex(0x0B0C10),
                header: hex(0x0E1014),
                card: hex(0x191C22),
                inset: hex(0x0C0E11),
                control: hex(0x262A32),
                control_hover: hex(0x30353F),
                border: hex(0x272B33),
                text: hex(0xC9CED8),
                text_strong: hex(0xFFFFFF),
                text_dim: hex(0x7D8594),
                accent: a,
                accent_fill: mix(a, Color32::BLACK, 0.22),
                on_accent: Color32::WHITE,
                ok: hex(0x3DD68C),
                warn: hex(0xFFB547),
                error: hex(0xFF5C6C),
            }
        } else {
            Palette {
                dark,
                bg: hex(0xF2F3F6),
                sidebar: hex(0xE6E8ED),
                header: hex(0xFFFFFF),
                card: hex(0xFFFFFF),
                inset: hex(0xE9EBF0),
                control: hex(0xE4E6EB),
                control_hover: hex(0xD7DAE1),
                border: hex(0xD8DBE2),
                text: hex(0x2A2E36),
                text_strong: hex(0x0B0C10),
                text_dim: hex(0x6B7280),
                accent: mix(a, Color32::BLACK, 0.12),
                accent_fill: mix(a, Color32::BLACK, 0.28),
                on_accent: Color32::WHITE,
                ok: hex(0x14955B),
                warn: hex(0xB86E00),
                error: hex(0xD62839),
            }
        }
    }
}

/// The palette that matches `ui`'s current light/dark mode.
pub fn palette(ui: &egui::Ui) -> Palette {
    palette_of(ui.ctx())
}

pub fn palette_of(ctx: &egui::Context) -> Palette {
    Palette::new(ctx.theme() == Theme::Dark, Accent::from_u8(ACCENT.load(Ordering::Relaxed)))
}

/// Font family for page titles and other display text (condensed, bold).
pub fn display() -> FontFamily {
    FontFamily::Name("display".into())
}

/// Semi-bold body text.
pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let data = |bytes: &'static [u8]| Arc::new(FontData::from_static(bytes));
    fonts.font_data.insert("fira".into(), data(include_bytes!("../assets/fonts/FiraSans-Regular.ttf")));
    fonts.font_data.insert("fira-semibold".into(), data(include_bytes!("../assets/fonts/FiraSans-SemiBold.ttf")));
    fonts.font_data.insert("fira-condensed".into(), data(include_bytes!("../assets/fonts/FiraSansCondensed-Bold.ttf")));
    // egui's own fonts stay as fallbacks (symbols, emoji)
    let fallbacks = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "fira".into());
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    for (family, main) in [(display(), "fira-condensed"), (semibold(), "fira-semibold")] {
        let mut list = vec![main.to_string(), "phosphor".to_string()];
        list.extend(fallbacks.iter().cloned());
        fonts.families.insert(family, list);
    }
    ctx.set_fonts(fonts);
}

/// Applies the colors, spacing and text sizes to both the light and the dark style.
pub fn apply(ctx: &egui::Context, accent: Accent) {
    ACCENT.store(Accent::ALL.iter().position(|a| *a == accent).unwrap_or(0) as u8, Ordering::Relaxed);
    for theme in [Theme::Dark, Theme::Light] {
        let p = Palette::new(theme == Theme::Dark, accent);
        ctx.style_mut_of(theme, |style| configure(style, &p));
    }
}

fn configure(style: &mut egui::Style, p: &Palette) {
    use FontFamily::{Monospace, Proportional};
    style.text_styles = [
        (TextStyle::Small, FontId::new(11.5, Proportional)),
        (TextStyle::Body, FontId::new(14.0, Proportional)),
        (TextStyle::Button, FontId::new(14.0, Proportional)),
        (TextStyle::Monospace, FontId::new(13.0, Monospace)),
        (TextStyle::Heading, FontId::new(22.0, display())),
    ]
    .into();

    let s = &mut style.spacing;
    s.item_spacing = Vec2::new(8.0, 7.0);
    s.button_padding = Vec2::new(12.0, 5.0);
    s.interact_size = Vec2::new(40.0, 26.0);
    s.slider_width = 170.0;
    s.combo_width = 120.0;
    s.icon_width = 16.0;
    s.icon_width_inner = 9.0;
    s.menu_margin = Margin::same(8);
    s.window_margin = Margin::same(14);
    s.scroll.bar_width = 8.0;
    s.scroll.floating = true;

    let v = &mut style.visuals;
    v.dark_mode = p.dark;
    v.override_text_color = None;
    v.panel_fill = p.bg;
    v.window_fill = if p.dark { mix(p.card, Color32::WHITE, 0.03) } else { p.card };
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    let shadow_alpha = if p.dark { 110 } else { 40 };
    v.window_shadow = Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(shadow_alpha) };
    v.popup_shadow = Shadow { offset: [0, 6], blur: 16, spread: 0, color: Color32::from_black_alpha(shadow_alpha) };
    v.extreme_bg_color = p.inset;
    v.text_edit_bg_color = Some(p.inset);
    v.faint_bg_color = mix(p.card, p.control, 0.35);
    v.code_bg_color = p.inset;
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.error;
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
    v.striped = false;
    v.indent_has_left_vline = false;
    v.selection.bg_fill = p.accent_fill;
    v.selection.stroke = Stroke::new(1.5, p.on_accent);
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);

    let w = &mut v.widgets;
    let r6 = CornerRadius::same(6);
    w.noninteractive.bg_fill = p.card;
    w.noninteractive.weak_bg_fill = p.card;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = CornerRadius::same(8);

    w.inactive.bg_fill = p.control;
    w.inactive.weak_bg_fill = p.control;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = r6;
    w.inactive.expansion = 0.0;

    w.hovered.bg_fill = p.control_hover;
    w.hovered.weak_bg_fill = p.control_hover;
    w.hovered.bg_stroke = Stroke::new(1.0, p.accent.gamma_multiply(0.7));
    w.hovered.fg_stroke = Stroke::new(1.5, p.text_strong);
    w.hovered.corner_radius = r6;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = p.accent_fill;
    w.active.weak_bg_fill = p.accent_fill;
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(2.0, p.on_accent);
    w.active.corner_radius = r6;
    w.active.expansion = 0.0;

    w.open.bg_fill = p.control_hover;
    w.open.weak_bg_fill = p.control_hover;
    w.open.bg_stroke = Stroke::new(1.0, p.accent.gamma_multiply(0.7));
    w.open.fg_stroke = Stroke::new(1.0, p.text_strong);
    w.open.corner_radius = r6;
}

// ------------------------------------------------------------------ building blocks

/// Small uppercase label above a group of controls.
pub fn section_label(ui: &mut egui::Ui, text: &str) {
    let p = palette(ui);
    ui.label(
        RichText::new(text.to_uppercase()).size(11.5).color(p.text_dim).family(semibold()).extra_letter_spacing(0.8),
    );
}

/// Title of a tab, with an optional one-line description below it.
pub fn page_title(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    let p = palette(ui);
    ui.label(
        RichText::new(title.to_uppercase()).size(26.0).family(display()).color(p.text_strong).extra_letter_spacing(0.6),
    );
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).color(p.text_dim));
    }
    ui.add_space(10.0);
}

/// A raised panel grouping related controls. `title` (if not empty) is drawn as a section label.
pub fn card<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = palette(ui);
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            if !title.is_empty() {
                section_label(ui, title);
                ui.add_space(4.0);
            }
            add(ui)
        })
        .inner
}

/// Filled accent button for the main action of a card.
pub fn primary_button(ui: &mut egui::Ui, enabled: bool, text: &str) -> Response {
    let p = palette(ui);
    let b = egui::Button::new(RichText::new(text).color(p.on_accent).family(semibold()))
        .fill(p.accent_fill)
        .stroke(Stroke::NONE)
        .min_size(Vec2::new(96.0, 30.0));
    ui.add_enabled(enabled, b)
}

/// Button drawing attention to something destructive.
pub fn danger_button(ui: &mut egui::Ui, enabled: bool, text: &str) -> Response {
    let p = palette(ui);
    let b = egui::Button::new(RichText::new(text).color(p.error))
        .fill(p.error.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, p.error.gamma_multiply(0.6)));
    ui.add_enabled(enabled, b)
}

/// Square button showing only an icon.
pub fn icon_button(ui: &mut egui::Ui, glyph: &str, hint: &str) -> Response {
    let b = egui::Button::new(RichText::new(glyph).size(17.0)).min_size(Vec2::splat(30.0));
    ui.add(b).on_hover_text(hint)
}

/// An on/off switch. Returns the response of the switch (`changed()` when flipped).
pub fn toggle(ui: &mut egui::Ui, on: &mut bool) -> Response {
    let p = palette(ui);
    let size = Vec2::new(38.0, 21.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *on, ""));
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_responsive(resp.id, *on);
        let track = if *on { mix(p.control, p.accent_fill, t) } else { p.control };
        let track = if resp.hovered() && !*on { p.control_hover } else { track };
        let painter = ui.painter();
        let radius = rect.height() / 2.0;
        painter.rect_filled(rect, radius, track);
        let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
        let knob = if *on { Color32::WHITE } else { p.text_dim };
        painter.circle_filled(egui::pos2(x, rect.center().y), radius - 3.5, knob);
        if !ui.is_enabled() {
            painter.rect_filled(rect, radius, p.card.gamma_multiply(0.5));
        }
    }
    resp
}

/// A setting row: label (and optional hint) on the left, a switch on the right.
pub fn toggle_row(ui: &mut egui::Ui, on: &mut bool, label: &str, hint: &str) -> Response {
    let p = palette(ui);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(label).color(p.text_strong));
            if !hint.is_empty() {
                ui.label(RichText::new(hint).small().color(p.text_dim));
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| toggle(ui, on)).inner
    })
    .inner
}

/// Row of mutually exclusive buttons drawn as one control. Returns true when the value changed.
pub fn segmented<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, items: &[(T, String)]) -> bool {
    let p = palette(ui);
    let font = FontId::new(14.0, semibold());
    let (pad, gap, h) = (12.0, 2.0, 26.0);
    let widths: Vec<f32> = items
        .iter()
        .map(|(_, t)| (ui.painter().layout_no_wrap(t.clone(), font.clone(), p.text).size().x + 2.0 * pad).max(32.0))
        .collect();
    let inner = widths.iter().sum::<f32>() + gap * items.len().saturating_sub(1) as f32;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(inner + 6.0, h + 6.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(8), p.inset);
    let mut changed = false;
    let mut x = rect.left() + 3.0;
    for ((v, text), w) in items.iter().zip(widths) {
        let selected = *value == *v;
        let rich = if selected {
            RichText::new(text).color(p.on_accent).font(font.clone())
        } else {
            RichText::new(text).color(p.text).font(FontId::proportional(14.0))
        };
        let b = egui::Button::new(rich)
            .fill(if selected { p.accent_fill } else { Color32::TRANSPARENT })
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(6));
        let r = egui::Rect::from_min_size(egui::pos2(x, rect.top() + 3.0), Vec2::new(w, h));
        if ui.put(r, b).clicked() && !selected {
            *value = *v;
            changed = true;
        }
        x += w + gap;
    }
    changed
}

/// Small rounded badge (battery, connection…).
pub fn pill(ui: &mut egui::Ui, text: &str, color: Color32) -> Response {
    let p = palette(ui);
    let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::new(12.5, semibold()), color);
    let size = galley.size() + Vec2::new(18.0, 8.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(11), color.gamma_multiply(if p.dark { 0.16 } else { 0.12 }));
    painter.galley(rect.center() - galley.size() / 2.0, galley, color);
    resp
}

/// Entry of the sidebar: icon and label, an accent bar when selected.
pub fn nav_item(ui: &mut egui::Ui, glyph: &str, label: &str, selected: bool, enabled: bool) -> Response {
    let p = palette(ui);
    let size = Vec2::new(ui.available_width(), 38.0);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let hovered = resp.hovered() && enabled;
        if selected {
            painter.rect_filled(rect, CornerRadius::same(8), p.accent.gamma_multiply(if p.dark { 0.14 } else { 0.16 }));
            let bar =
                egui::Rect::from_min_size(rect.left_top() + Vec2::new(0.0, 9.0), Vec2::new(3.0, rect.height() - 18.0));
            painter.rect_filled(bar, CornerRadius::same(2), p.accent);
        } else if hovered {
            painter.rect_filled(rect, CornerRadius::same(8), p.control.gamma_multiply(0.7));
        }
        let (icon_color, text_color) = match (enabled, selected, hovered) {
            (false, _, _) => (p.text_dim.gamma_multiply(0.5), p.text_dim.gamma_multiply(0.6)),
            (true, true, _) => (p.accent, p.text_strong),
            (true, false, true) => (p.text_strong, p.text_strong),
            _ => (p.text_dim, p.text),
        };
        let y = rect.center().y;
        painter.text(
            egui::pos2(rect.left() + 16.0, y),
            egui::Align2::LEFT_CENTER,
            glyph,
            FontId::proportional(19.0),
            icon_color,
        );
        let family = if selected { semibold() } else { FontFamily::Proportional };
        painter.text(
            egui::pos2(rect.left() + 46.0, y),
            egui::Align2::LEFT_CENTER,
            label,
            FontId::new(14.5, family),
            text_color,
        );
    }
    resp
}

/// Status dot (connected, demo, error…).
pub fn dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.5, color);
    ui.painter().circle_stroke(rect.center(), 4.5 + 2.5, Stroke::new(2.0, color.gamma_multiply(0.25)));
}

/// Frame of the keyboard drawing (the case).
pub fn keyboard_case(painter: &egui::Painter, rect: egui::Rect, p: &Palette) {
    painter.rect(rect, CornerRadius::same(14), p.inset, Stroke::new(1.0, p.border), StrokeKind::Inside);
}

/// Draws a keycap: a darker skirt and a lighter top face.
pub fn keycap(painter: &egui::Painter, rect: egui::Rect, fill: Color32) {
    let unit = (rect.height() / 40.0).clamp(0.6, 1.6);
    let skirt = mix(fill, Color32::BLACK, 0.38);
    painter.rect_filled(rect, CornerRadius::same(5), skirt);
    let top = egui::Rect::from_min_max(
        rect.min + Vec2::new(2.5 * unit, 1.5 * unit),
        rect.max - Vec2::new(2.5 * unit, 4.0 * unit),
    );
    painter.rect_filled(top, CornerRadius::same(4), fill);
}

/// Mixes two colors (`t` = 0 → `a`, 1 → `b`).
pub fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    mix(a, b, t)
}
