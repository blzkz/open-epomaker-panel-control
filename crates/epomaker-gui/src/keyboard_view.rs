//! Draws a keyboard from the model's key geometry and reports clicks.

use crate::preview::Spot;
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2};
use epomaker_core::devices::ModelInfo;

pub struct KeyStyle {
    pub fill: Color32,
    pub text: String,
    pub text_color: Color32,
    pub highlight: bool,
    /// Shown when hovering the key.
    pub tooltip: String,
}

/// Matrix entries of the rotary knob (volume down, press, volume up).
const KNOB_MINUS: u32 = 0x0300_EA00;
const KNOB_PLUS: u32 = 0x0300_E900;

/// One key of the layout, in layout units (origin at the top-left corner, y down).
pub struct GeoKey {
    pub idx: usize,
    pub label: String,
    pub rect: Rect,
    /// Relative position, row and walking order (for the lighting preview).
    pub spot: Spot,
}

/// The three keys drawn as a round dial.
pub struct Knob {
    pub minus: usize,
    pub press: usize,
    pub plus: usize,
    pub center: Pos2,
    pub radius: f32,
}

/// Which part of the dial a point falls on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum KnobZone {
    Minus,
    Press,
    Plus,
}

pub struct Geometry {
    pub keys: Vec<GeoKey>,
    pub size: Vec2,
    pub knob: Option<Knob>,
}

impl Geometry {
    pub fn new(model: &ModelInfo) -> Geometry {
        let keys: Vec<_> = model.keys.iter().filter(|k| k.matrix_index.is_some()).collect();
        if keys.is_empty() {
            return Geometry { keys: vec![], size: Vec2::ZERO, knob: None };
        }
        // vendor geometry: x to the right, y grows upwards (rows have negative y)
        let min_x = keys.iter().map(|k| k.x).fold(f32::MAX, f32::min);
        let max_x = keys.iter().map(|k| k.x + k.w).fold(f32::MIN, f32::max);
        let min_top = keys.iter().map(|k| -k.y).fold(f32::MAX, f32::min);
        let max_bottom = keys.iter().map(|k| -k.y + k.h).fold(f32::MIN, f32::max);
        let size = Vec2::new(max_x - min_x, max_bottom - min_top);
        let mut geo: Vec<GeoKey> = keys
            .iter()
            .map(|k| {
                let rect = Rect::from_min_size(Pos2::new(k.x - min_x, -k.y - min_top), Vec2::new(k.w, k.h));
                let idx = k.matrix_index.unwrap_or_default();
                let pos = Vec2::new(rect.center().x / size.x.max(1.0), rect.center().y / size.y.max(1.0));
                let spot = Spot { idx, pos, row: 0, rows: 1, order: 0, serp: 0, count: keys.len() };
                GeoKey { idx, label: k.label.clone(), rect, spot }
            })
            .collect();

        // rows: groups of at least three keys whose top edges are close together; odd keys
        // (offset navigation keys, the knob) join the nearest row
        let mut heights: Vec<f32> = keys.iter().map(|k| k.h).collect();
        heights.sort_by(f32::total_cmp);
        let unit = heights[heights.len() / 2];
        let mut tops: Vec<f32> = geo.iter().map(|k| k.rect.top()).collect();
        tops.sort_by(f32::total_cmp);
        let mut clusters: Vec<(f32, usize)> = vec![];
        for t in tops {
            match clusters.last_mut() {
                Some((first, n)) if t - *first <= unit * 0.4 => *n += 1,
                _ => clusters.push((t, 1)),
            }
        }
        let mut row_tops: Vec<f32> = clusters.iter().filter(|(_, n)| *n >= 3).map(|(t, _)| *t).collect();
        if row_tops.is_empty() {
            row_tops = clusters.iter().map(|(t, _)| *t).collect();
        }
        for k in &mut geo {
            let dist = |t: &f32| (k.rect.top() - t).abs();
            k.spot.row =
                (0..row_tops.len()).min_by(|a, b| dist(&row_tops[*a]).total_cmp(&dist(&row_tops[*b]))).unwrap_or(0);
            k.spot.rows = row_tops.len();
        }
        let mut walk: Vec<usize> = (0..geo.len()).collect();
        let row_x = |k: &GeoKey, back: bool| {
            let x = k.rect.left();
            (k.spot.row, if back && k.spot.row % 2 == 1 { -x } else { x })
        };
        walk.sort_by(|a, b| row_x(&geo[*a], false).partial_cmp(&row_x(&geo[*b], false)).unwrap());
        for (n, i) in walk.iter().enumerate() {
            geo[*i].spot.order = n;
        }
        walk.sort_by(|a, b| row_x(&geo[*a], true).partial_cmp(&row_x(&geo[*b], true)).unwrap());
        for (n, i) in walk.iter().enumerate() {
            geo[*i].spot.serp = n;
        }

        let mut knob = find_knob(model, &geo);
        let mut size = size;
        if let Some(k) = &mut knob {
            // the dial may grow past the keys: make room for it
            let dial = Rect::from_center_size(k.center, Vec2::splat(2.0 * k.radius + 8.0));
            let all = Rect::from_min_size(Pos2::ZERO, size).union(dial);
            let shift = -all.min.to_vec2();
            size = all.size();
            k.center += shift;
            for g in &mut geo {
                g.rect = g.rect.translate(shift);
                g.spot.pos = Vec2::new(g.rect.center().x / size.x, g.rect.center().y / size.y);
            }
        }
        Geometry { keys: geo, size, knob }
    }

    fn is_knob(&self, idx: usize) -> bool {
        self.knob.as_ref().is_some_and(|k| [k.minus, k.press, k.plus].contains(&idx))
    }
}

fn find_knob(model: &ModelInfo, geo: &[GeoKey]) -> Option<Knob> {
    let idx_of = |code: u32| model.keys.iter().find(|k| k.code == code).and_then(|k| k.matrix_index);
    let (minus, plus) = (idx_of(KNOB_MINUS)?, idx_of(KNOB_PLUS)?);
    let rect_of = |i: usize| geo.iter().find(|k| k.idx == i).map(|k| k.rect);
    let (rm, rp) = (rect_of(minus)?, rect_of(plus)?);
    // the press key sits between the two volume keys
    let press = geo
        .iter()
        .find(|k| k.idx != minus && k.idx != plus && k.rect.center().x > rm.right() && k.rect.center().x < rp.left())
        .filter(|k| (k.rect.center().y - rm.center().y).abs() < rm.height())?;
    let bounds = rm.union(rp).union(press.rect);
    let center = bounds.center();
    // the vendor draws three thin keys; use the free space around them for a readable dial
    let min = bounds.width().min(bounds.height()) / 2.0;
    let free = geo
        .iter()
        .filter(|k| ![minus, plus, press.idx].contains(&k.idx))
        .map(|k| k.rect.distance_to_pos(center) - 4.0)
        .fold(f32::MAX, f32::min);
    let radius = free.min(min * 1.5).max(min);
    Some(Knob { minus, press: press.idx, plus, center, radius })
}

pub struct ViewResponse {
    /// Matrix index of the key clicked this frame.
    pub clicked: Option<usize>,
    /// Matrix index under a pressed pointer (for painting by dragging).
    pub dragged_over: Option<usize>,
    /// Response of the whole keyboard area (drags, pointer position).
    pub response: egui::Response,
    /// Screen rectangle of every key, by matrix index.
    pub rects: Vec<(usize, Rect)>,
}

/// Draws every key of `model`; `style(key)` decides its look. The keyboard is scaled to
/// the available width, and at most to `max_height` pixels.
pub fn show(
    ui: &mut egui::Ui,
    model: &ModelInfo,
    max_height: f32,
    mut style: impl FnMut(&GeoKey) -> KeyStyle,
) -> ViewResponse {
    let geo = Geometry::new(model);
    if geo.keys.is_empty() {
        let response = ui.label("(no layout for this model)");
        return ViewResponse { clicked: None, dragged_over: None, response, rects: vec![] };
    }
    let pad = 8.0;
    let avail = ui.available_width().min(1100.0) - 2.0 * pad - 2.0;
    let scale = (avail / geo.size.x).min((max_height - 2.0 * pad) / geo.size.y).max(0.5);
    let full = Vec2::new(geo.size.x * scale + 2.0 * pad, geo.size.y * scale + 2.0 * pad);
    // center horizontally when the height limit makes the keyboard narrower than the panel
    let (response, painter) = ui
        .horizontal(|ui| {
            ui.add_space(((ui.available_width() - full.x) / 2.0).max(0.0));
            ui.allocate_painter(full, Sense::click_and_drag())
        })
        .inner;
    let origin = response.rect.min + Vec2::splat(pad);
    painter.rect_filled(response.rect, CornerRadius::same(10), ui.visuals().extreme_bg_color);
    let to_screen = |r: Rect| Rect::from_min_max(origin + r.min.to_vec2() * scale, origin + r.max.to_vec2() * scale);

    let pointer = response.interact_pointer_pos().or(response.hover_pos());
    let mut hovered = None;
    let mut tooltip = String::new();
    let mut rects = Vec::with_capacity(geo.keys.len());
    let font = FontId::proportional((10.5 * scale).clamp(8.0, 14.0));
    let idle_stroke = Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
    let hover_stroke = Stroke::new(1.5, ui.visuals().widgets.hovered.fg_stroke.color);
    let select_stroke = Stroke::new(2.5, ui.visuals().selection.stroke.color);
    let mut knob_styles: Vec<(usize, KeyStyle)> = vec![];
    for k in &geo.keys {
        let rect = to_screen(k.rect).shrink(1.5);
        rects.push((k.idx, rect));
        let s = style(k);
        if geo.is_knob(k.idx) {
            knob_styles.push((k.idx, s));
            continue;
        }
        let is_hover = pointer.is_some_and(|p| rect.contains(p));
        if is_hover {
            hovered = Some(k.idx);
            tooltip = s.tooltip.clone();
        }
        painter.rect_filled(rect, CornerRadius::same(4), s.fill);
        let stroke = if s.highlight {
            select_stroke
        } else if is_hover {
            hover_stroke
        } else {
            idle_stroke
        };
        painter.rect_stroke(rect, CornerRadius::same(4), stroke, StrokeKind::Inside);
        // shrink long labels a little before cutting them
        let mut size = font.size;
        while size > 8.0 && s.text.lines().any(|l| l.chars().count() as f32 * size * 0.55 > rect.width()) {
            size -= 0.5;
        }
        let text = fit(&s.text, rect.width(), size);
        painter.text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(size), s.text_color);
    }

    if let Some(knob) = &geo.knob {
        let center = origin + knob.center.to_vec2() * scale;
        let radius = knob.radius * scale - 1.5;
        let inner = radius * 0.5;
        let zone_at = |p: Pos2| {
            let d = p - center;
            if d.length() > radius {
                None
            } else if d.length() <= inner {
                Some(KnobZone::Press)
            } else if d.x < 0.0 {
                Some(KnobZone::Minus)
            } else {
                Some(KnobZone::Plus)
            }
        };
        let hover_zone = pointer.and_then(zone_at);
        let style_of = |i: usize| knob_styles.iter().find(|(idx, _)| *idx == i).map(|(_, s)| s);
        let (Some(sm), Some(sp), Some(sc)) = (style_of(knob.minus), style_of(knob.plus), style_of(knob.press)) else {
            unreachable!("knob keys are part of the layout")
        };
        let zones =
            [(KnobZone::Minus, knob.minus, sm), (KnobZone::Plus, knob.plus, sp), (KnobZone::Press, knob.press, sc)];
        for (zone, idx, s) in zones {
            let is_hover = hover_zone == Some(zone);
            if is_hover {
                hovered = Some(idx);
                tooltip = s.tooltip.clone();
            }
            let stroke = if s.highlight {
                select_stroke
            } else if is_hover {
                hover_stroke
            } else {
                idle_stroke
            };
            match zone {
                KnobZone::Press => {
                    painter.circle(center, inner, s.fill, stroke);
                    let text = fit(&s.text, inner * 2.0 - 4.0, font.size * 0.9);
                    painter.text(center, Align2::CENTER_CENTER, text, font.clone(), s.text_color);
                }
                KnobZone::Minus | KnobZone::Plus => {
                    let left = zone == KnobZone::Minus;
                    painter.add(Shape::convex_polygon(half_disc(center, radius, left), s.fill, Stroke::NONE));
                    painter.add(Shape::closed_line(half_disc(center, radius, left), stroke));
                    let x = center.x + if left { -1.0 } else { 1.0 } * (inner + radius) / 2.0;
                    let big = FontId::proportional(font.size * 1.4);
                    let sign = if left { "-" } else { "+" };
                    painter.text(Pos2::new(x, center.y), Align2::CENTER_CENTER, sign, big, s.text_color);
                }
            }
            // the press zone covers the inner part of both halves
            if zone == KnobZone::Plus || zone == KnobZone::Minus {
                rects.retain(|(i, _)| *i != idx);
                let half = if zone == KnobZone::Minus {
                    Rect::from_min_max(center - Vec2::splat(radius), Pos2::new(center.x - inner, center.y + radius))
                } else {
                    Rect::from_min_max(Pos2::new(center.x + inner, center.y - radius), center + Vec2::splat(radius))
                };
                rects.push((idx, half));
            } else {
                rects.retain(|(i, _)| *i != idx);
                rects.push((idx, Rect::from_center_size(center, Vec2::splat(inner * 2.0))));
            }
        }
        // grip marks around the dial
        for n in 0..24 {
            let a = n as f32 / 24.0 * std::f32::consts::TAU;
            let dir = Vec2::angled(a);
            painter.line_segment(
                [center + dir * (radius + 1.0), center + dir * (radius + 3.5)],
                Stroke::new(1.0, idle_stroke.color),
            );
        }
    }

    let response =
        if hovered.is_some() && !tooltip.is_empty() { response.on_hover_text_at_pointer(tooltip) } else { response };
    let clicked = if response.clicked() { hovered } else { None };
    let dragged_over = if response.dragged() || response.is_pointer_button_down_on() { hovered } else { None };
    ViewResponse { clicked, dragged_over, response, rects }
}

/// Left or right half of a disc, as a convex polygon.
fn half_disc(center: Pos2, radius: f32, left: bool) -> Vec<Pos2> {
    let steps = 24;
    (0..=steps)
        .map(|i| {
            let a = std::f32::consts::FRAC_PI_2 + i as f32 / steps as f32 * std::f32::consts::PI;
            let p = center + Vec2::angled(a) * radius;
            if left {
                p
            } else {
                Pos2::new(2.0 * center.x - p.x, p.y)
            }
        })
        .collect()
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

/// Draws a translucent selection rectangle (used while dragging a selection).
pub fn selection_rect(ui: &egui::Ui, rect: Rect) {
    let c = ui.visuals().selection.bg_fill;
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("kb_sel")));
    painter.rect(
        rect,
        CornerRadius::same(2),
        c.gamma_multiply(0.25),
        Stroke::new(1.5, ui.visuals().selection.stroke.color),
        StrokeKind::Inside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use epomaker_core::devices::Database;

    #[test]
    fn th80_knob_and_rows() {
        let db = Database::get();
        let model = db.model("yc500_th80").unwrap();
        let geo = Geometry::new(model);
        let knob = geo.knob.as_ref().expect("TH80 has a knob");
        let entry = |i| model.default_entry(i).unwrap();
        assert_eq!(entry(knob.minus), [3, 0, 0xEA, 0]);
        assert_eq!(entry(knob.press), [3, 0, 0xE2, 0]);
        assert_eq!(entry(knob.plus), [3, 0, 0xE9, 0]);
        let esc = geo.keys.iter().find(|k| k.label == "Esc").unwrap();
        assert_eq!(esc.spot.rows, 6);
        assert_eq!((esc.spot.row, esc.spot.order, esc.spot.serp), (0, 0, 0));
        // orders are permutations
        let mut o: Vec<_> = geo.keys.iter().map(|k| k.spot.serp).collect();
        o.sort();
        assert!(o.iter().enumerate().all(|(i, v)| i == *v));
        assert!(Geometry::new(db.model("yc500_th21").unwrap()).knob.is_none());
    }
}
