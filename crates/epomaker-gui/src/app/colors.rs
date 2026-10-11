//! Per-key colors tab: painting, selections, gradients and the custom picture slots.

use super::App;
use crate::i18n::tr;
use crate::keyboard_view::{self, Geometry, KeyStyle};
use crate::preview;
use crate::worker::{JobInfo, Snapshot, Update, Worker};
use eframe::egui::{self, Pos2, Rect, RichText, Sense, StrokeKind, Vec2};
use epomaker_core::lighting::{LightEffect, LightMode, Rgb};
use std::collections::BTreeSet;

pub(super) const RECENT_MAX: usize = 12;
/// Custom pictures the firmware keeps (the bulk write always targets the first).
const SLOTS: u8 = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Paint,
    Select,
    Rows,
}

pub(super) struct ColorsState {
    brush: [u8; 3],
    gradient_end: [u8; 3],
    vertical: bool,
    tool: Tool,
    selection: BTreeSet<usize>,
    drag_origin: Option<Pos2>,
    paint_live: bool,
    last_painted: Option<usize>,
    pub recent: Vec<[u8; 3]>,
    slot: u8,
    /// Slots 2 and 3 cannot be read back: only what was painted in this session.
    extra: [Vec<Rgb>; 2],
}

impl Default for ColorsState {
    fn default() -> Self {
        ColorsState {
            brush: [255, 0, 128],
            gradient_end: [0, 128, 255],
            vertical: false,
            tool: Tool::Paint,
            selection: BTreeSet::new(),
            drag_origin: None,
            paint_live: true,
            last_painted: None,
            recent: vec![],
            slot: 0,
            extra: Default::default(),
        }
    }
}

impl ColorsState {
    /// Forgets everything tied to the connected keyboard.
    pub(super) fn reset(&mut self) {
        self.selection.clear();
        self.extra = Default::default();
        self.slot = 0;
        self.drag_origin = None;
        self.last_painted = None;
    }

    fn remember(&mut self, c: [u8; 3]) {
        self.recent.retain(|r| *r != c);
        self.recent.insert(0, c);
        self.recent.truncate(RECENT_MAX);
    }

    fn colors<'a>(&'a mut self, snap: &'a mut Snapshot) -> &'a mut Vec<Rgb> {
        let n = snap.model.matrix_len();
        let v = match self.slot {
            0 => &mut snap.colors,
            s => &mut self.extra[(s as usize - 1).min(1)],
        };
        if v.len() < n {
            v.resize(n, Rgb(0));
        }
        v
    }

    /// Keys the fill and gradient act on: the selection, or the whole keyboard.
    fn targets(&self, geo: &Geometry) -> Vec<usize> {
        geo.keys.iter().map(|k| k.idx).filter(|i| self.selection.is_empty() || self.selection.contains(i)).collect()
    }

    fn gradient(&self, geo: &Geometry) -> Vec<(usize, Rgb)> {
        let targets = self.targets(geo);
        let coord = |i: usize| {
            geo.keys.iter().find(|k| k.idx == i).map(|k| if self.vertical { k.spot.pos.y } else { k.spot.pos.x })
        };
        let values: Vec<f32> = targets.iter().filter_map(|i| coord(*i)).collect();
        let (lo, hi) = values.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
        let (a, b) = (self.brush, self.gradient_end);
        targets
            .into_iter()
            .map(|i| {
                let f = if hi > lo { (coord(i).unwrap_or(lo) - lo) / (hi - lo) } else { 0.0 };
                let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f).round() as u8;
                (i, Rgb::new(l(a[0], b[0]), l(a[1], b[1]), l(a[2], b[2])))
            })
            .collect()
    }

    #[cfg(feature = "autoshot")]
    pub(super) fn autoshot_demo(&mut self, snap: Option<&mut Snapshot>) {
        let Some(snap) = snap else { return };
        let geo = Geometry::new(snap.model);
        self.tool = Tool::Select;
        self.selection = geo.keys.iter().filter(|k| (2..=4).contains(&k.spot.row)).map(|k| k.idx).collect();
        self.recent = vec![[255, 0, 128], [0, 128, 255], [255, 200, 0], [40, 220, 120], [255, 255, 255]];
        for (i, c) in self.gradient(&geo) {
            snap.colors[i] = c;
        }
        self.selection = geo.keys.iter().filter(|k| k.spot.row == 1).map(|k| k.idx).collect();
    }
}

/// Writes key colors of `slot`: the fast bulk write for the first slot, key by key otherwise.
fn write_colors(worker: &Worker, slot: u8, all: Vec<Rgb>, keys: Vec<usize>) {
    if slot == 0 {
        worker.run_notify("Write colors", move |kb| {
            kb.set_key_colors(&all)?;
            Ok(Update::None)
        });
    } else {
        worker.run_with(JobInfo::notify("Write colors"), move |kb, progress| {
            for (n, i) in keys.iter().enumerate() {
                kb.set_key_color(slot, *i, all.get(*i).copied().unwrap_or_default())?;
                progress.set(n + 1, keys.len());
            }
            Ok(Update::None)
        });
    }
}

impl App {
    pub(super) fn colors_tab(&mut self, ui: &mut egui::Ui) {
        let Some(dev) = &mut self.dev else { return };
        let model = dev.snap.model;
        let lang = self.lang;
        let t = |s| tr(lang, s);
        let busy = self.worker.busy();
        let st = &mut self.colors;
        let geo = Geometry::new(model);

        // ---- custom picture slot
        ui.horizontal(|ui| {
            ui.label(RichText::new(t("Custom picture")).strong());
            for s in 0..SLOTS {
                if ui.selectable_value(&mut st.slot, s, format!("{}", s + 1)).changed() {
                    st.last_painted = None;
                }
            }
            let shown = matches!(dev.snap.light, Some(l) if l.mode == LightMode::UserPicture && l.option == st.slot);
            if shown {
                ui.weak(t("(shown on the keyboard)"));
            } else if ui.add_enabled(!busy, egui::Button::new(t("Show this picture on the keyboard"))).clicked() {
                let e = LightEffect {
                    mode: LightMode::UserPicture,
                    brightness: 4,
                    option: st.slot,
                    ..LightEffect::default()
                };
                self.light = e;
                self.worker.run_notify("Set lighting", move |kb| {
                    kb.set_light(&e)?;
                    Ok(Update::Light(Some(e)))
                });
            }
        });
        ui.weak(if st.slot == 0 {
            t("\"Write all\" sends the whole picture at once. The keyboard's bulk write always goes to picture 1, like in the vendor app.")
        } else {
            t("Pictures 2 and 3 cannot be read back and the bulk write only reaches picture 1, so they are written key by key (slower).")
        });
        ui.add_space(4.0);

        // ---- tools
        ui.horizontal_wrapped(|ui| {
            ui.label(t("Tool"));
            ui.selectable_value(&mut st.tool, Tool::Paint, t("Paint"))
                .on_hover_text(t("Click or drag over keys to paint them."));
            ui.selectable_value(&mut st.tool, Tool::Select, t("Select"))
                .on_hover_text(t("Click keys, or drag a box around them. Hold Shift to add to the selection."));
            ui.selectable_value(&mut st.tool, Tool::Rows, t("Select rows"))
                .on_hover_text(t("Click a key to select its row."));
            ui.separator();
            ui.label(t("Brush"));
            egui::color_picker::color_edit_button_srgb(ui, &mut st.brush);
            for c in st.recent.clone() {
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
                ui.painter().rect_filled(rect, 3.0, egui::Color32::from_rgb(c[0], c[1], c[2]));
                if c == st.brush {
                    ui.painter().rect_stroke(rect, 3.0, ui.visuals().selection.stroke, StrokeKind::Outside);
                }
                if resp.on_hover_text(format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])).clicked() {
                    st.brush = c;
                }
            }
            if st.recent.is_empty() {
                ui.weak(t("(recent colors appear here)"));
            }
            ui.separator();
            ui.checkbox(&mut st.paint_live, t("Paint live"))
                .on_hover_text(t("Send every change to the keyboard right away"));
        });

        // ---- keyboard
        let slot = st.slot;
        let colors = st.colors(&mut dev.snap).clone();
        let selection = st.selection.clone();
        let max_h = (ui.available_height() * 0.6).max(180.0);
        let resp = keyboard_view::show(ui, model, max_h, |k| {
            let fill = colors.get(k.idx).map(|c| preview::rgb(*c)).unwrap_or(preview::DARK);
            KeyStyle {
                fill,
                text: k.label.clone(),
                text_color: keyboard_view::contrast(fill),
                highlight: selection.contains(&k.idx),
                tooltip: format!("{}  #{:06X}", k.label.replace('\n', " "), colors.get(k.idx).map_or(0, |c| c.0)),
            }
        });
        let shift = ui.input(|i| i.modifiers.shift);
        match st.tool {
            Tool::Paint => {
                if !ui.input(|i| i.pointer.any_down()) {
                    st.last_painted = None;
                }
                if let Some(i) = resp.dragged_over.or(resp.clicked) {
                    if st.last_painted != Some(i) {
                        if st.last_painted.is_none() {
                            st.remember(st.brush);
                        }
                        st.last_painted = Some(i);
                        let c = Rgb::new(st.brush[0], st.brush[1], st.brush[2]);
                        if let Some(slot_color) = st.colors(&mut dev.snap).get_mut(i) {
                            *slot_color = c;
                        }
                        if st.paint_live {
                            self.worker.run("Paint key", move |kb| {
                                kb.set_key_color(slot, i, c)?;
                                Ok(Update::None)
                            });
                        }
                    }
                }
            }
            Tool::Select => {
                let r = &resp.response;
                if r.drag_started() {
                    st.drag_origin = r.interact_pointer_pos();
                }
                if let (Some(origin), Some(p)) = (st.drag_origin, r.interact_pointer_pos()) {
                    let rect = Rect::from_two_pos(origin, p);
                    if r.dragged() {
                        keyboard_view::selection_rect(ui, rect);
                    }
                    if r.drag_stopped() {
                        if !shift {
                            st.selection.clear();
                        }
                        st.selection.extend(resp.rects.iter().filter(|(_, k)| k.intersects(rect)).map(|(i, _)| *i));
                        st.drag_origin = None;
                    }
                }
                if !r.dragged() && !r.drag_stopped() {
                    st.drag_origin = None;
                }
                if let Some(i) = resp.clicked {
                    if !st.selection.remove(&i) {
                        st.selection.insert(i);
                    }
                }
            }
            Tool::Rows => {
                if let Some(row) = resp.clicked.and_then(|i| geo.keys.iter().find(|k| k.idx == i)).map(|k| k.spot.row) {
                    let keys: Vec<usize> = geo.keys.iter().filter(|k| k.spot.row == row).map(|k| k.idx).collect();
                    if keys.iter().all(|i| st.selection.contains(i)) {
                        keys.iter().for_each(|i| {
                            st.selection.remove(i);
                        });
                    } else {
                        st.selection.extend(keys);
                    }
                }
            }
        }
        ui.add_space(6.0);

        // ---- selection and fills
        let mut fill: Option<Vec<(usize, Rgb)>> = None;
        ui.horizontal_wrapped(|ui| {
            if st.selection.is_empty() {
                ui.label(t("No keys selected: fills apply to the whole keyboard."));
            } else {
                ui.label(RichText::new(format!("{} {}", st.selection.len(), t("keys selected"))).strong());
            }
            if ui.button(t("Select all")).clicked() {
                st.selection = geo.keys.iter().map(|k| k.idx).collect();
            }
            if ui.add_enabled(!st.selection.is_empty(), egui::Button::new(t("Clear selection"))).clicked() {
                st.selection.clear();
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button(t("Fill with brush")).clicked() {
                let c = Rgb::new(st.brush[0], st.brush[1], st.brush[2]);
                fill = Some(st.targets(&geo).into_iter().map(|i| (i, c)).collect());
            }
            ui.separator();
            ui.label(t("Gradient from brush to"));
            egui::color_picker::color_edit_button_srgb(ui, &mut st.gradient_end);
            ui.selectable_value(&mut st.vertical, false, t("left to right"));
            ui.selectable_value(&mut st.vertical, true, t("top to bottom"));
            if ui.button(t("Apply gradient")).clicked() {
                fill = Some(st.gradient(&geo));
                st.remember(st.gradient_end);
            }
        });
        if let Some(changes) = fill {
            st.remember(st.brush);
            let keys: Vec<usize> = changes.iter().map(|(i, _)| *i).collect();
            let live = st.paint_live;
            let all = st.colors(&mut dev.snap);
            for (i, c) in changes {
                if let Some(slot_color) = all.get_mut(i) {
                    *slot_color = c;
                }
            }
            if live {
                write_colors(&self.worker, slot, all.clone(), keys);
            }
        }
        ui.add_space(6.0);

        // ---- keyboard transfer
        ui.horizontal(|ui| {
            let label = if slot == 0 { t("Write all to keyboard") } else { t("Write all to keyboard (key by key)") };
            if ui.add_enabled(!busy, egui::Button::new(RichText::new(label).strong())).clicked() {
                let all = st.colors(&mut dev.snap).clone();
                write_colors(&self.worker, slot, all, geo.keys.iter().map(|k| k.idx).collect());
            }
            let read = ui
                .add_enabled(!busy && slot == 0, egui::Button::new(t("Read from keyboard")))
                .on_disabled_hover_text(t("Only picture 1 can be read back."));
            if read.clicked() {
                self.worker.run("Read colors", |kb| Ok(Update::Colors(kb.key_colors()?)));
            }
        });
    }
}
