//! What the Besfa editor and the panels a game draws for it share: the
//! theme, the languages, and the tree both list things in.

use std::sync::Arc;

use egui::{
    Color32, Id, RichText, Ui, WidgetInfo, WidgetType,
    collapsing_header::{CollapsingState, paint_default_icon},
};

pub mod i18n;
pub mod inspector;

use i18n::t;

pub const TEXT: Color32 = Color32::from_rgb(0xB3, 0xBB, 0xC8);
pub const MUTED: Color32 = Color32::from_rgb(0x7E, 0x87, 0x95);
pub const ACCENT: Color32 = Color32::from_rgb(0x8C, 0xB4, 0xFF);
pub const WARNING: Color32 = Color32::from_rgb(0xE8, 0xB8, 0x6D);
pub const GOOD: Color32 = Color32::from_rgb(0x7E, 0xC9, 0x8F);
pub const ERROR: Color32 = Color32::from_rgb(0xF2, 0x8B, 0x82);

/// A panel's title with its buttons on the right.
pub fn panel_header(ui: &mut Ui, title: &str, actions: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(t(title).to_uppercase())
                .small()
                .strong()
                .color(MUTED),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), actions);
    });
    ui.separator();
}

/// `rows`, given in tree order, each indented by its `depth` after a toggle
/// that folds away the rows under it, when it has any. Folds are named by
/// `fold`, which also says whether one starts open; `show` draws the rest of
/// a row.
pub fn tree<T>(
    ui: &mut Ui,
    rows: &[T],
    depth: impl Fn(&T) -> usize,
    fold: impl Fn(&T) -> (Id, bool),
    mut show: impl FnMut(&mut Ui, &T),
) {
    for at in unfolded(ui.ctx(), rows, &depth, &fold) {
        tree_row(ui, rows, at, &depth, &fold, &mut show);
    }
}

/// The indices of the `rows` [`tree`] shows: those no folded row above hides.
pub fn unfolded<T>(
    ctx: &egui::Context,
    rows: &[T],
    depth: impl Fn(&T) -> usize,
    fold: impl Fn(&T) -> (Id, bool),
) -> Vec<usize> {
    let mut shown = Vec::new();
    // The depth of the folded row whose rows are being skipped.
    let mut folded = None;
    for (at, row) in rows.iter().enumerate() {
        let row_depth = depth(row);
        if folded.is_some_and(|folded| row_depth > folded) {
            continue;
        }
        shown.push(at);
        let closed = || {
            let (id, open) = fold(row);
            !CollapsingState::load_with_default_open(ctx, id, open).is_open()
        };
        let parent = rows.get(at + 1).is_some_and(|next| depth(next) > row_depth);
        folded = (parent && closed()).then_some(row_depth);
    }
    shown
}

/// Row `at` of [`tree`].
pub fn tree_row<T>(
    ui: &mut Ui,
    rows: &[T],
    at: usize,
    depth: impl Fn(&T) -> usize,
    fold: impl Fn(&T) -> (Id, bool),
    show: impl FnOnce(&mut Ui, &T),
) {
    let row = &rows[at];
    let row_depth = depth(row);
    ui.horizontal(|ui| {
        ui.add_space(row_depth as f32 * 16.0);
        if rows.get(at + 1).is_some_and(|next| depth(next) > row_depth) {
            let (id, open) = fold(row);
            let mut state = CollapsingState::load_with_default_open(ui.ctx(), id, open);
            let response = state.show_toggle_button(ui, paint_default_icon);
            let label = if state.is_open() {
                t("Fold")
            } else {
                t("Unfold")
            };
            response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label));
            state.store(ui.ctx());
        } else {
            ui.allocate_space(egui::vec2(ui.spacing().indent, ui.spacing().icon_width));
        }
        show(ui, row);
    });
}

pub fn section(ui: &mut Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(t(title)).small().strong().color(MUTED));
}

pub fn tag(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(t(text)).small().color(MUTED));
}

/// The editor's dark theme, with a system font for text egui's fonts lack,
/// like Korean in paths and build errors.
pub fn style(ctx: &egui::Context) {
    if let Ok(font) = std::fs::read(r"C:\Windows\Fonts\malgun.ttf") {
        let mut fonts = egui::FontDefinitions::default();
        fonts
            .font_data
            .insert("system".into(), Arc::new(egui::FontData::from_owned(font)));
        for family in fonts.families.values_mut() {
            family.push("system".into());
        }
        ctx.set_fonts(fonts);
    }
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        let visuals = &mut style.visuals;
        visuals.panel_fill = Color32::from_rgb(0x1A, 0x1D, 0x23);
        visuals.window_fill = Color32::from_rgb(0x20, 0x24, 0x2B);
        visuals.extreme_bg_color = Color32::from_rgb(0x11, 0x13, 0x16);
        visuals.selection.bg_fill = Color32::from_rgb(0x2E, 0x3D, 0x5A);
        visuals.hyperlink_color = ACCENT;
    });
}
