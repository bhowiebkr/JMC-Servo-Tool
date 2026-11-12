//! Millwright's dark theme (colours, fonts and spacing taken from Millwright Sound).

use eframe::egui::{self, Color32, RichText, Stroke};

pub const ACCENT: Color32 = Color32::from_rgb(232, 163, 61);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(58, 50, 32);
pub const BACKGROUND: Color32 = Color32::from_rgb(22, 24, 27);
pub const CANVAS: Color32 = Color32::from_rgb(18, 20, 23);
pub const PANEL: Color32 = Color32::from_rgb(31, 34, 38);
pub const LINE: Color32 = Color32::from_rgb(53, 58, 65);
pub const NODE: Color32 = Color32::from_rgb(39, 43, 48);
pub const TEXT: Color32 = Color32::from_rgb(230, 232, 234);
pub const TEXT_DIM: Color32 = Color32::from_rgb(154, 161, 169);
pub const ERROR: Color32 = Color32::from_rgb(224, 108, 90);
/// Not in Millwright: for "servo on" and "matches".
pub const OK: Color32 = Color32::from_rgb(126, 196, 122);
/// Second plot line (commands).
pub const BLUE: Color32 = Color32::from_rgb(110, 160, 220);

/// Colour meanings in the help drawings. Each colour has one meaning, and every drawing also
/// names its lines in a legend, so the colour is never the only clue.
pub mod ink {
    use super::Color32;

    /// What the drive is told to do (dashed).
    pub const COMMAND: Color32 = super::BLUE;
    /// The result with the value read from the drive or the open file (solid, thick).
    pub const VALUE: Color32 = super::ACCENT;
    /// The result with the factory value (short dashes).
    pub const DEFAULT: Color32 = super::TEXT_DIM;
    /// The result with the value typed in "Try a value" (long dashes).
    pub const TRIAL: Color32 = Color32::from_rgb(180, 140, 230);
    /// An output that is on, or a range that is good.
    pub const GOOD: Color32 = super::OK;
    pub const GOOD_FILL: Color32 = Color32::from_rgba_unmultiplied_const(126, 196, 122, 34);
    /// An alarm, a fault or a range that the drive cuts off.
    pub const ALARM: Color32 = super::ERROR;
    pub const ALARM_FILL: Color32 = Color32::from_rgba_unmultiplied_const(224, 108, 90, 38);
    pub const GRID: Color32 = super::LINE;
    /// A signal the drive measures and watches, in an example (solid, thin).
    pub const SIGNAL: Color32 = super::TEXT;
}

pub fn apply(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for (name, path, family) in [
        ("segoe", "C:/Windows/Fonts/segoeui.ttf", egui::FontFamily::Proportional),
        ("consolas", "C:/Windows/Fonts/consola.ttf", egui::FontFamily::Monospace),
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(name.into(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
            fonts.families.entry(family).or_default().insert(0, name.into());
        }
    }
    ctx.set_fonts(fonts);
    ctx.set_theme(egui::Theme::Dark);
    let mut v = egui::Visuals::dark();
    v.panel_fill = BACKGROUND;
    v.window_fill = PANEL;
    v.extreme_bg_color = CANVAS;
    v.faint_bg_color = PANEL;
    v.code_bg_color = NODE;
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = ACCENT_DIM;
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.text_cursor.stroke = Stroke::new(1.0, ACCENT);
    v.override_text_color = Some(TEXT);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.bg_fill = NODE;
        w.weak_bg_fill = NODE;
        w.bg_stroke = Stroke::new(1.0, LINE);
        w.corner_radius = 5.into();
    }
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, TEXT_DIM);
    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    ctx.set_visuals_of(egui::Theme::Dark, v);
    ctx.style_mut_of(egui::Theme::Dark, |s| {
        for (kind, size) in [
            (egui::TextStyle::Body, 14.0),
            (egui::TextStyle::Button, 14.0),
            (egui::TextStyle::Small, 12.0),
            (egui::TextStyle::Heading, 16.0),
        ] {
            s.text_styles.insert(kind, egui::FontId::proportional(size));
        }
        s.spacing.item_spacing = egui::vec2(10.0, 8.0);
        s.spacing.button_padding = egui::vec2(8.0, 4.0);
        s.spacing.interact_size = egui::vec2(40.0, 30.0);
        s.spacing.slider_width = 150.0;
    });
}

pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(14, 12))
}

pub fn primary(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_owned()).color(Color32::from_rgb(27, 20, 6)).strong())
        .fill(ACCENT)
        .stroke(Stroke::new(1.0, ACCENT))
}

/// A small coloured label with a frame, for states like "Servo on".
pub fn chip(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .stroke(Stroke::new(1.0, color))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(color).small());
        });
}

pub fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).strong().color(ACCENT));
}
