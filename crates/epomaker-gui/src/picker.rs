//! Searchable, categorized pickers for key actions and HID keys.

use crate::i18n::{tr, Lang};
use eframe::egui::{self, Popup, PopupCloseBehavior, RichText, ScrollArea, Vec2};
use epomaker_core::keycodes;
use epomaker_core::keymap::{MouseAction, SpecialFn};

/// Something that can be assigned to a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Key(u8),
    Mouse(MouseAction),
    Func(SpecialFn),
    Macro(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Letters,
    Symbols,
    FKeys,
    Navigation,
    Modifiers,
    Numpad,
    Media,
    Mouse,
    Macros,
    Layers,
}

impl Category {
    pub const ALL: [Category; 10] = [
        Category::Letters,
        Category::Symbols,
        Category::FKeys,
        Category::Navigation,
        Category::Modifiers,
        Category::Numpad,
        Category::Media,
        Category::Mouse,
        Category::Macros,
        Category::Layers,
    ];
    /// Categories made only of HID keys (usable inside macros).
    pub const KEYS: [Category; 6] = [
        Category::Letters,
        Category::Symbols,
        Category::FKeys,
        Category::Navigation,
        Category::Modifiers,
        Category::Numpad,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Letters => "Letters & numbers",
            Category::Symbols => "Symbols",
            Category::FKeys => "F-keys",
            Category::Navigation => "Navigation & editing",
            Category::Modifiers => "Modifiers",
            Category::Numpad => "Numpad",
            Category::Media => "Media & functions",
            Category::Mouse => "Mouse",
            Category::Macros => "Macros",
            Category::Layers => "Layers",
        }
    }

    fn of_key(code: u8) -> Category {
        match code {
            0x04..=0x27 => Category::Letters,
            0x3A..=0x45 | 0x68..=0x73 => Category::FKeys,
            0x28..=0x2C | 0x39 | 0x46..=0x52 | 0x65 | 0x66 => Category::Navigation,
            0x53..=0x63 | 0x67 => Category::Numpad,
            0xE0..=0xE7 => Category::Modifiers,
            _ => Category::Symbols,
        }
    }

    pub fn of(p: Pick) -> Category {
        match p {
            Pick::Key(k) => Category::of_key(k),
            Pick::Mouse(_) => Category::Mouse,
            Pick::Func(SpecialFn::Fn | SpecialFn::RightFn | SpecialFn::FnLock) => Category::Layers,
            Pick::Func(_) => Category::Media,
            Pick::Macro(_) => Category::Macros,
        }
    }

    /// Items of the category; `macro_slots` bounds the macro list.
    pub fn items(self, macro_slots: u8) -> Vec<Pick> {
        match self {
            Category::Mouse => MouseAction::ALL.into_iter().map(Pick::Mouse).collect(),
            Category::Macros => (0..macro_slots).map(Pick::Macro).collect(),
            Category::Media | Category::Layers => {
                SpecialFn::ALL.into_iter().map(Pick::Func).filter(|p| Category::of(*p) == self).collect()
            }
            _ => keycodes::all().map(|(c, _)| Pick::Key(c)).filter(|p| Category::of(*p) == self).collect(),
        }
    }
}

/// Text shown for an item (translated where it makes sense; key names stay as printed on keycaps).
pub fn pick_label(p: Pick, lang: Lang) -> String {
    match p {
        Pick::Key(k) => keycodes::display(k),
        Pick::Mouse(m) => tr(lang, m.label()).to_string(),
        Pick::Func(f) => tr(lang, f.label()).to_string(),
        Pick::Macro(i) => format!("{} {i}", tr(lang, "Macro")),
    }
}

fn matches(p: Pick, query: &str, lang: Lang) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    let english = match p {
        Pick::Key(k) => keycodes::display(k),
        Pick::Mouse(m) => m.label().to_string(),
        Pick::Func(f) => f.label().to_string(),
        Pick::Macro(i) => format!("macro {i}"),
    };
    pick_label(p, lang).to_lowercase().contains(&q) || english.to_lowercase().contains(&q)
}

/// State of a picker (selected category and search text).
pub struct PickerState {
    pub category: Category,
    pub search: String,
}

impl PickerState {
    pub fn new(category: Category) -> PickerState {
        PickerState { category, search: String::new() }
    }
}

/// Category list + search box + grid of items. Returns the item clicked this frame.
pub fn picker_ui(
    ui: &mut egui::Ui,
    lang: Lang,
    state: &mut PickerState,
    categories: &[Category],
    macro_slots: u8,
    selected: Option<Pick>,
    grid_height: f32,
) -> Option<Pick> {
    let t = |s| tr(lang, s);
    let mut picked = None;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(170.0);
            for c in categories {
                let active = state.search.trim().is_empty() && state.category == *c;
                let marker = selected.is_some_and(|s| Category::of(s) == *c);
                let mut text = RichText::new(t(c.label()));
                if marker {
                    text = text.strong();
                }
                if ui.add(egui::Button::selectable(active, text).min_size(Vec2::new(165.0, 0.0))).clicked() {
                    state.category = *c;
                    state.search.clear();
                }
            }
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(t("Search"));
                ui.add(
                    egui::TextEdit::singleline(&mut state.search).hint_text(t("key, function…")).desired_width(220.0),
                );
                if !state.search.is_empty() && ui.small_button("x").on_hover_text(t("Clear")).clicked() {
                    state.search.clear();
                }
            });
            ui.add_space(4.0);
            let items: Vec<Pick> = if state.search.trim().is_empty() {
                state.category.items(macro_slots)
            } else {
                categories
                    .iter()
                    .flat_map(|c| c.items(macro_slots))
                    .filter(|p| matches(*p, &state.search, lang))
                    .collect()
            };
            ScrollArea::vertical().id_salt("picker_grid").max_height(grid_height).auto_shrink([false, true]).show(
                ui,
                |ui| {
                    if items.is_empty() {
                        ui.weak(t("Nothing found"));
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                        for p in items {
                            let text = pick_label(p, lang);
                            let min = Vec2::new(if text.chars().count() <= 3 { 40.0 } else { 0.0 }, 26.0);
                            let b = egui::Button::selectable(selected == Some(p), text).min_size(min);
                            if ui.add(b).clicked() {
                                picked = Some(p);
                            }
                        }
                    });
                },
            );
        });
    });
    picked
}

/// A button showing the HID key `value`; clicking it opens a searchable key picker.
/// Returns true when the value changed.
pub fn key_button(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    value: &mut u8,
    allow_none: bool,
    lang: Lang,
) -> bool {
    let id = ui.id().with(id);
    let text = if *value == 0 && allow_none { "—".to_string() } else { keycodes::display(*value) };
    let resp = ui.add(egui::Button::new(text).min_size(Vec2::new(110.0, 0.0)));
    let mut changed = false;
    let state_id = id.with("state");
    let mut state = ui
        .data(|d| d.get_temp::<Category>(state_id))
        .map(PickerState::new)
        .unwrap_or_else(|| PickerState::new(Category::of_key(*value)));
    state.search = ui.data(|d| d.get_temp::<String>(state_id.with("q"))).unwrap_or_default();
    let popup_id = Popup::default_response_id(&resp);
    Popup::from_toggle_button_response(&resp)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .width(640.0)
        .show(|ui| {
            if allow_none && ui.button(tr(lang, "No key")).clicked() {
                *value = 0;
                changed = true;
            }
            let sel = (*value != 0).then_some(Pick::Key(*value));
            if let Some(Pick::Key(k)) = picker_ui(ui, lang, &mut state, &Category::KEYS, 0, sel, 260.0) {
                *value = k;
                changed = true;
            }
        });
    if changed {
        Popup::close_id(ui.ctx(), popup_id);
    }
    ui.data_mut(|d| {
        d.insert_temp(state_id, state.category);
        d.insert_temp(state_id.with("q"), state.search);
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_has_one_category() {
        let total: usize = Category::KEYS.iter().map(|c| c.items(0).len()).sum();
        assert_eq!(total, keycodes::all().count());
        assert!(Category::Letters.items(0).contains(&Pick::Key(0x04)));
        assert!(Category::FKeys.items(0).contains(&Pick::Key(0x45)));
        assert!(Category::Navigation.items(0).contains(&Pick::Key(0x4F)));
        assert!(Category::Modifiers.items(0).contains(&Pick::Key(0xE1)));
        assert!(Category::Numpad.items(0).contains(&Pick::Key(0x62)));
        assert_eq!(Category::Layers.items(0).len(), 3);
        assert_eq!(Category::Macros.items(50).len(), 50);
        assert!(matches(Pick::Func(SpecialFn::VolumeUp), "volu", Lang::En));
        assert!(matches(Pick::Func(SpecialFn::VolumeUp), "subir", Lang::Es));
    }
}
