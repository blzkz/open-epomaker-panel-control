//! Lighting tab: animated preview and effect cards.

use super::App;
use crate::i18n::{option_label, tr};
use crate::keyboard_view::{self, KeyStyle};
use crate::preview::{self, Sim, Spot};
use eframe::egui::{self, Color32, CornerRadius, Rect, RichText, Sense, Slider, Stroke, StrokeKind, Vec2};
use epomaker_core::lighting::{LightEffect, LightMode, Rgb};

const PRESETS: [u32; 8] = [0xFF0000, 0xFF8000, 0xFFFF00, 0x00FF00, 0x00FFFF, 0x0000FF, 0xFF00FF, 0xFFFFFF];
const CARD: Vec2 = Vec2::new(128.0, 72.0);

impl App {
    pub(super) fn lighting_tab(&mut self, ui: &mut egui::Ui) {
        let Some(dev) = &self.dev else { return };
        let model = dev.snap.model;
        let user_colors = dev.snap.colors.clone();
        let lang = self.lang;
        let t = |s| tr(lang, s);
        let now = ui.input(|i| i.time);
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));

        let caps = model.caps(self.light.mode).cloned();
        let sim = Sim {
            effect: self.light,
            max_speed: caps.as_ref().and_then(|c| c.max_speed).unwrap_or(4),
            max_brightness: caps.as_ref().and_then(|c| c.max_brightness).unwrap_or(4),
            user_colors: &user_colors,
        };
        let max_h = (ui.available_height() * 0.42).clamp(150.0, 300.0);
        keyboard_view::show(ui, model, max_h, |k| {
            let fill = sim.color(&k.spot, now);
            KeyStyle {
                fill,
                text: k.label.clone(),
                text_color: keyboard_view::contrast(fill).gamma_multiply(0.55),
                highlight: false,
                tooltip: String::new(),
            }
        });
        ui.add_space(8.0);

        let mut modes: Vec<LightMode> = vec![LightMode::Off];
        modes.extend(model.light_modes.iter().map(|c| c.mode));
        let mut changed = false;
        let params_w = 340.0;
        ui.horizontal_top(|ui| {
            let cards_w = (ui.available_width() - params_w - 16.0).max(CARD.x + 20.0);
            ui.allocate_ui(Vec2::new(cards_w, ui.available_height()), |ui| {
                egui::ScrollArea::vertical().id_salt("effects").auto_shrink([false, false]).show(ui, |ui| {
                    // stretch the cards so that each row fills the width
                    let gap = 6.0;
                    let width = ui.available_width() - 2.0;
                    let cols = ((width + gap) / (CARD.x + gap)).floor().max(1.0);
                    let card = Vec2::new((width - gap * (cols - 1.0)) / cols, CARD.y);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::splat(gap);
                        for m in &modes {
                            let selected = self.light.mode == *m;
                            let resp = effect_card(ui, *m, selected, card, &self.light, model, &user_colors, now, lang);
                            if resp.clicked()
                                && !selected
                            {
                                self.light.mode = *m;
                                self.light.option = 0;
                                if let Some(c) = model.caps(*m) {
                                    if !c.rgb && c.dazzle {
                                        self.light.dazzle = true;
                                    }
                                    if let Some(max) = c.max_brightness {
                                        self.light.brightness = self.light.brightness.min(max);
                                    }
                                }
                                changed = true;
                            }
                        }
                    });
                });
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.set_width(params_w);
                ui.label(RichText::new(t(self.light.mode.label())).heading().strong());
                ui.add_space(4.0);
                egui::Grid::new("light").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
                    if let Some(c) = &caps {
                        if let Some(max) = c.max_speed.filter(|_| self.light.mode.uses_speed()) {
                            ui.label(t("Speed"));
                            changed |= ui.add(Slider::new(&mut self.light.speed, 0..=max)).changed();
                            ui.end_row();
                        }
                        if let Some(max) = c.max_brightness {
                            ui.label(t("Brightness"));
                            changed |= ui.add(Slider::new(&mut self.light.brightness, 0..=max)).changed();
                            ui.end_row();
                        }
                        if !c.options.is_empty() {
                            ui.label(t("Direction / variant"));
                            ui.horizontal_wrapped(|ui| {
                                for (i, o) in c.options.iter().enumerate() {
                                    let text = t(option_label(o));
                                    changed |= ui.selectable_value(&mut self.light.option, i as u8, text).changed();
                                }
                            });
                            ui.end_row();
                        }
                        if c.dazzle {
                            ui.label("");
                            changed |= ui.checkbox(&mut self.light.dazzle, t("Rainbow colors")).changed();
                            ui.end_row();
                        }
                        if c.rgb && !self.light.dazzle {
                            ui.label(t("Color"));
                            changed |= color_row(ui, &mut self.light.color);
                            ui.end_row();
                        }
                    }
                });
                if self.light.mode.needs_host() {
                    ui.add_space(6.0);
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        t("The effect needs the host to stream audio/screen data; the keyboard will stay dark without it."),
                    );
                }
                if matches!(self.light.mode, LightMode::UserPicture | LightMode::UserColor) {
                    ui.add_space(6.0);
                    ui.weak(t("Paint the keys on the \"Per-key colors\" tab."));
                }
                ui.add_space(12.0);
                let busy = self.worker.busy();
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.light_auto, t("Apply automatically"));
                });
                ui.horizontal(|ui| {
                    if ui.add_enabled(!busy, egui::Button::new(RichText::new(t("Apply")).strong())).clicked() {
                        self.send_light();
                    }
                    if ui.add_enabled(!busy, egui::Button::new(t("Read from keyboard"))).clicked() {
                        self.worker.run("Read lighting", |kb| Ok(super::Update::Light(kb.light()?)));
                    }
                });
                if self.light_dirty && !self.light_auto {
                    ui.weak(t("Not applied yet"));
                }
            });
        });
        if changed {
            self.light_dirty = true;
        }
        let dragging = ui.input(|i| i.pointer.any_down());
        if self.light_auto && self.light_dirty && !dragging && !self.worker.busy() {
            self.send_light();
        }
    }

    pub(super) fn send_light(&mut self) {
        self.light_dirty = false;
        let e = self.light;
        self.worker.run("Set lighting", move |kb| {
            kb.set_light(&e)?;
            Ok(super::Update::Light(Some(e)))
        });
    }
}

/// Color button and preset swatches. Returns true when the color changed.
fn color_row(ui: &mut egui::Ui, color: &mut Rgb) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        let mut rgb = color.to_array();
        if egui::color_picker::color_edit_button_srgb(ui, &mut rgb).changed() {
            *color = Rgb::new(rgb[0], rgb[1], rgb[2]);
            changed = true;
        }
        for preset in PRESETS {
            let c = Rgb(preset);
            let (rect, resp) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
            ui.painter().rect_filled(rect, 3.0, preview::rgb(c));
            if *color == c {
                ui.painter().rect_stroke(rect, 3.0, ui.visuals().selection.stroke, StrokeKind::Outside);
            }
            if resp.on_hover_text(format!("#{preset:06X}")).clicked() {
                *color = c;
                changed = true;
            }
        }
    });
    changed
}

/// A clickable card with an animated thumbnail of the effect.
#[allow(clippy::too_many_arguments)]
fn effect_card(
    ui: &mut egui::Ui,
    mode: LightMode,
    selected: bool,
    size: Vec2,
    current: &LightEffect,
    model: &epomaker_core::devices::ModelInfo,
    user_colors: &[Rgb],
    now: f64,
    lang: crate::i18n::Lang,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let v = ui.visuals();
    let fill = if selected { v.selection.bg_fill.gamma_multiply(0.35) } else { v.widgets.inactive.weak_bg_fill };
    let stroke = if selected {
        Stroke::new(2.0, v.selection.stroke.color)
    } else if resp.hovered() {
        v.widgets.hovered.bg_stroke
    } else {
        v.widgets.noninteractive.bg_stroke
    };
    let painter = ui.painter();
    painter.rect(rect, CornerRadius::same(6), fill, stroke, StrokeKind::Inside);

    // thumbnail: a tiny 14 x 4 keyboard
    let caps = model.caps(mode);
    let effect = if selected {
        *current
    } else {
        LightEffect {
            mode,
            speed: 2,
            brightness: caps.and_then(|c| c.max_brightness).unwrap_or(4),
            option: 0,
            color: Rgb(0x00A0FF),
            dazzle: caps.is_some_and(|c| c.dazzle),
        }
    };
    let sim = Sim {
        effect,
        max_speed: caps.and_then(|c| c.max_speed).unwrap_or(4),
        max_brightness: caps.and_then(|c| c.max_brightness).unwrap_or(4),
        user_colors,
    };
    let (cols, rows) = (14usize, 4usize);
    let thumb = Rect::from_min_size(rect.min + Vec2::new(8.0, 8.0), Vec2::new(rect.width() - 16.0, 24.0));
    let cell = Vec2::new(thumb.width() / cols as f32, thumb.height() / rows as f32);
    for r in 0..rows {
        for c in 0..cols {
            let n = r * cols + c;
            let spot = Spot {
                // spread the thumbnail keys over the real matrix so custom colors show up
                idx: n * 6 % user_colors.len().max(1),
                pos: Vec2::new((c as f32 + 0.5) / cols as f32, (r as f32 + 0.5) / rows as f32),
                row: r,
                rows,
                order: n,
                serp: if r % 2 == 1 { r * cols + cols - 1 - c } else { n },
                count: rows * cols,
            };
            let min = thumb.min + Vec2::new(c as f32 * cell.x, r as f32 * cell.y);
            let key = Rect::from_min_size(min, cell).shrink(0.6);
            painter.rect_filled(key, CornerRadius::same(1), sim.color(&spot, now));
        }
    }
    let name = tr(lang, mode.label());
    let font = egui::FontId::proportional(12.0);
    let text_color = if selected { v.strong_text_color() } else { v.text_color() };
    let galley = painter.layout(name.to_string(), font, text_color, rect.width() - 10.0);
    let pos = egui::pos2(rect.center().x - galley.size().x / 2.0, thumb.bottom() + 4.0);
    painter.galley(pos, galley, Color32::PLACEHOLDER);
    resp.on_hover_text(name)
}
