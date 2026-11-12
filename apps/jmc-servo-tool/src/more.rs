//! The "More" window: the full help page of a parameter or a topic, with a colour drawing,
//! "Try a value", and links that open other pages in the same window (Back and Forward work
//! like a web browser).

use std::collections::BTreeMap;

use eframe::egui::{self, RichText};
use jmc_core::docs::{self, Kind, Target};
use jmc_core::params::{Access, ParamDef, Schema};

use crate::app::{App, Tab};
use crate::drawing::{self, Curve, Input, Role};
use crate::{markdown, table, theme};

/// The window's id, for both the native window and the embedded one.
fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("jmc-more")
}

/// The pages the window has shown, and its "Try a value" values.
#[derive(Debug, Default)]
pub struct More {
    pub open: bool,
    history: Vec<String>,
    pos: usize,
    /// "Try a value" for each parameter, in display units.
    pub(crate) trial: BTreeMap<String, f64>,
    reset_scroll: bool,
    focus: bool,
}

/// The longest history kept.
const HISTORY: usize = 200;

impl More {
    /// Open the window on the page `key` (a code or `topic:<name>`).
    pub fn show(&mut self, key: &str) {
        if self.current() != Some(key) {
            if !self.history.is_empty() {
                self.history.truncate(self.pos + 1);
            }
            self.history.push(key.to_string());
            if self.history.len() > HISTORY {
                self.history.remove(0);
            }
            self.pos = self.history.len() - 1;
        }
        self.open = true;
        self.reset_scroll = true;
        self.focus = true;
    }

    pub fn current(&self) -> Option<&str> {
        self.history.get(self.pos).map(String::as_str)
    }

    pub fn can_back(&self) -> bool {
        self.pos > 0
    }

    pub fn can_forward(&self) -> bool {
        self.pos + 1 < self.history.len()
    }

    pub fn back(&mut self) {
        if self.can_back() {
            self.pos -= 1;
            self.reset_scroll = true;
        }
    }

    pub fn forward(&mut self) {
        if self.can_forward() {
            self.pos += 1;
            self.reset_scroll = true;
        }
    }

    pub fn trial(&self, code: &str) -> Option<f64> {
        self.trial.get(code).copied()
    }
}

/// What the user did in the window, applied after it is drawn.
enum Action {
    Go(String),
    Back,
    Forward,
    Select(String),
    Close,
}

fn go(t: Target) -> Option<Action> {
    match t {
        Target::Web(_) => None,
        t => Some(Action::Go(t.key())),
    }
}

/// The page title: "P02-10  Speed gain 1", or the topic's title.
fn page_title(schema: &Schema, key: &str) -> String {
    if let Some(name) = key.strip_prefix("topic:") {
        return docs::page(key).and_then(|p| p.title.clone()).unwrap_or_else(|| name.to_string());
    }
    match schema.get(key) {
        Ok(d) => format!("{}  {}", d.code, d.name),
        Err(_) => key.to_string(),
    }
}

impl App {
    /// Draw the More window, if it is open.
    pub(crate) fn more_window(&mut self, ctx: &egui::Context) {
        if !self.more.open {
            return;
        }
        let Some(key) = self.more.current().map(str::to_string) else {
            self.more.open = false;
            return;
        };
        let title = page_title(self.schema, &key);
        let builder = egui::ViewportBuilder::default()
            .with_title(format!("More: {title}"))
            .with_inner_size([820.0, 900.0])
            .with_min_inner_size([520.0, 400.0])
            .with_resizable(true);
        let mut actions: Vec<Action> = Vec::new();
        ctx.show_viewport_immediate(viewport_id(), builder, |ui, class| {
            let embedded = class == egui::ViewportClass::EmbeddedWindow;
            if !embedded && ui.input(|i| i.viewport().close_requested()) {
                actions.push(Action::Close);
            }
            if ui.input(|i| (i.modifiers.alt && i.key_pressed(egui::Key::ArrowLeft)) || i.pointer.button_pressed(egui::PointerButton::Extra1)) {
                actions.push(Action::Back);
            }
            if ui.input(|i| (i.modifiers.alt && i.key_pressed(egui::Key::ArrowRight)) || i.pointer.button_pressed(egui::PointerButton::Extra2)) {
                actions.push(Action::Forward);
            }
            if embedded {
                self.more_bar(ui, &key, embedded, &mut actions);
                ui.separator();
                self.more_scroll(ui, &key, &mut actions);
            } else {
                egui::Panel::top("more-bar")
                    .frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(14, 8)))
                    .show(ui, |ui| self.more_bar(ui, &key, embedded, &mut actions));
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(egui::Margin::symmetric(14, 10)))
                    .show(ui, |ui| self.more_scroll(ui, &key, &mut actions));
            }
        });
        if std::mem::take(&mut self.more.focus) && !ctx.embed_viewports() {
            ctx.send_viewport_cmd_to(viewport_id(), egui::ViewportCommand::Focus);
        }
        for a in actions {
            match a {
                Action::Go(k) => self.more.show(&k),
                Action::Back => self.more.back(),
                Action::Forward => self.more.forward(),
                Action::Select(code) => {
                    if let Ok(d) = self.schema.get(&code) {
                        self.tab = Tab::Parameters;
                        // A hidden row cannot be selected: show the kind of row the user asked for.
                        if d.access == Access::Factory {
                            self.show_factory = true;
                        }
                        if d.placeholder() {
                            self.show_spares = true;
                        }
                        self.select(d);
                    }
                }
                Action::Close => self.more.open = false,
            }
        }
    }

    fn more_bar(&mut self, ui: &mut egui::Ui, key: &str, embedded: bool, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            if ui.add_enabled(self.more.can_back(), egui::Button::new("← Back")).on_hover_text("Alt+Left").clicked() {
                actions.push(Action::Back);
            }
            if ui.add_enabled(self.more.can_forward(), egui::Button::new("Forward →")).on_hover_text("Alt+Right").clicked() {
                actions.push(Action::Forward);
            }
            ui.menu_button("Topics", |ui| {
                for p in docs::topics() {
                    if ui.button(p.title.clone().unwrap_or_else(|| p.key.clone())).clicked() {
                        actions.push(Action::Go(p.key.clone()));
                        ui.close();
                    }
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if embedded && ui.button("Close").clicked() {
                    actions.push(Action::Close);
                }
                if self.schema.get(key).is_ok()
                    && ui.button("Select in table").on_hover_text("Show this parameter in the main window").clicked()
                {
                    actions.push(Action::Select(key.to_string()));
                }
            });
        });
    }

    fn more_scroll(&mut self, ui: &mut egui::Ui, key: &str, actions: &mut Vec<Action>) {
        let mut area = egui::ScrollArea::vertical().id_salt("more-body").auto_shrink([false; 2]);
        if std::mem::take(&mut self.more.reset_scroll) {
            area = area.vertical_scroll_offset(0.0);
        }
        area.show(ui, |ui| {
            ui.set_max_width(ui.available_width().min(900.0));
            match self.schema.get(key) {
                Ok(def) if !key.starts_with("topic:") => self.more_param(ui, def, actions),
                _ => self.more_topic(ui, key, actions),
            }
            ui.add_space(12.0);
        });
    }

    /// The display value of any parameter: the drive's or the file's value, else the default.
    fn display_value(&self, code: &str) -> Option<f64> {
        let d = self.schema.get(code).ok()?;
        let s = 10f64.powi(i32::from(d.decimals));
        match self.values.raw.get(code) {
            Some(&raw) => Some(d.value_of(raw) as f64 / s),
            None => d.default.map(|v| v as f64 / s),
        }
    }

    fn more_param(&mut self, ui: &mut egui::Ui, def: &'static ParamDef, actions: &mut Vec<Action>) {
        let schema = self.schema;
        let page = docs::page(&def.code);
        let s = 10f64.powi(i32::from(def.decimals));

        ui.label(RichText::new(format!("{}  {}", def.code, def.name)).size(20.0).strong());
        group_line(ui, def, actions);
        ui.add_space(4.0);
        facts(ui, def, &self.values);
        ui.add_space(8.0);

        // The drawing, with the default, the value and the trial value.
        let drawing = page.and_then(|p| p.drawing.clone()).unwrap_or_else(|| docs::drawing_for(def));
        let raw = self.values.raw.get(&def.code).map(|&r| def.value_of(r));
        let value = raw.map(|r| r as f64 / s);
        // When the value is the factory value, one curve shows both: two lines on top of each
        // other only make the labels hard to read.
        let same = raw.is_some() && raw == def.default;
        let mut curves = Vec::new();
        if let Some(d) = def.default.filter(|_| !same) {
            curves.push(Curve { role: Role::Default, v: d as f64 / s, label: "Default".into() });
        }
        if let Some(v) = value {
            let source = if self.values.from_drive { "Drive" } else { "File" };
            let label = if same { format!("{source} = default") } else { source.to_string() };
            curves.push(Curve { role: Role::Value, v, label });
        }
        let mut trial = self.more.trial(&def.code);
        if let Some(v) = trial {
            curves.push(Curve { role: Role::Trial, v, label: "Try".into() });
        }
        let start = value.or(def.default.map(|d| d as f64 / s)).unwrap_or(0.0);
        {
            let lookup = |code: &str| self.display_value(code);
            theme::card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::heading(ui, "Drawing");
                drawing::show(ui, &drawing, &Input { def, lookup: &lookup, curves });
                ui.add_space(6.0);
                drawing::trial_control(ui, def, &mut trial, start);
            });
        }
        match trial {
            Some(v) => self.more.trial.insert(def.code.clone(), v),
            None => self.more.trial.remove(&def.code),
        };
        ui.add_space(8.0);

        // The description: the brief, then the page.
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::heading(ui, "Description");
            if let Some(t) = markdown::show(ui, schema, def.brief.trim()) {
                actions.extend(go(t));
            }
            if drawing.kind != Kind::Choices && !def.choices.is_empty() {
                ui.add_space(4.0);
                for c in &def.choices {
                    ui.label(RichText::new(format!("{} = {}", def.format_value(c.value), c.label)).color(theme::TEXT_DIM));
                }
            }
            if let Some(p) = page.filter(|p| !p.body.is_empty()) {
                ui.add_space(4.0);
                if let Some(t) = markdown::show(ui, schema, &p.body) {
                    actions.extend(go(t));
                }
            }
        });

        let mut see: Vec<Target> = page.map(|p| p.see.clone()).unwrap_or_default();
        if let Some((n, _)) = docs::notch_role(&def.code) {
            for c in docs::notch_codes(n) {
                let t = Target::Param(c);
                if t.key() != def.code && !see.contains(&t) {
                    see.push(t);
                }
            }
        }
        links(ui, schema, &def.code, &see, actions);
    }

    fn more_topic(&mut self, ui: &mut egui::Ui, key: &str, actions: &mut Vec<Action>) {
        let schema = self.schema;
        let Some(page) = docs::page(key) else {
            ui.label(RichText::new(format!("There is no page {key}.")).color(theme::ERROR));
            return;
        };
        ui.label(RichText::new(page_title(schema, key)).size(20.0).strong());
        ui.add_space(4.0);
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            if let Some(t) = markdown::show(ui, schema, &page.body) {
                actions.extend(go(t));
            }
        });
        links(ui, schema, key, &page.see, actions);
    }
}

/// The group of a parameter, with links to the page that explains the group and to the start
/// page for beginners.
fn group_line(ui: &mut egui::Ui, def: &ParamDef, actions: &mut Vec<Action>) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format!("Group P{:02}", def.group)).color(theme::TEXT_DIM));
        if let Some(p) = docs::group_topic(def.group) {
            ui.label(RichText::new("·").color(theme::TEXT_DIM));
            let title = p.title.clone().unwrap_or_else(|| p.key.clone());
            if ui.link(RichText::new(title).color(theme::ACCENT)).on_hover_text("How this group of settings works").clicked() {
                actions.push(Action::Go(p.key.clone()));
            }
        }
        if docs::page("topic:basics").is_some() {
            ui.label(RichText::new("·  New to servo drives?").color(theme::TEXT_DIM));
            if ui.link(RichText::new("How a servo drive works").color(theme::ACCENT)).clicked() {
                actions.push(Action::Go("topic:basics".into()));
            }
        }
    });
}

/// Code, unit, range, default, value and when a change takes effect.
fn facts(ui: &mut egui::Ui, def: &ParamDef, values: &crate::app::Values) {
    let with_choice = |v: i64| match def.choices.iter().find(|c| c.value == v) {
        Some(c) => format!("{}  {}", def.format_value(v), c.label),
        None => def.format_value(v),
    };
    let unit = table::unit_text(def);
    let unit_suffix = if unit.is_empty() { String::new() } else { format!(" {unit}") };
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::Grid::new(("facts", def.code.as_str())).num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
            let mut row = |k: &str, v: String, color: egui::Color32| {
                ui.label(RichText::new(k).color(theme::TEXT_DIM));
                ui.label(RichText::new(v).color(color));
                ui.end_row();
            };
            row("Register", format!("{}  (address 0x{:04X})", def.code, def.address), theme::TEXT);
            if !unit.is_empty() {
                row("Unit", unit.to_string(), theme::TEXT);
            }
            row("Range", format!("{}{unit_suffix}", def.range_text()), theme::TEXT);
            if let Some(d) = def.default {
                row("Default", format!("{}{unit_suffix}", with_choice(d)), crate::theme::ink::DEFAULT);
            }
            if let Some(&raw) = values.raw.get(&def.code) {
                let from = if values.from_drive { "Drive" } else { "File" };
                row(from, format!("{}{unit_suffix}  ({})", with_choice(def.value_of(raw)), values.source), crate::theme::ink::VALUE);
            }
            if let Some(a) = table::applies_text(def) {
                row("Takes effect", a.to_string(), theme::TEXT);
            }
            let mut notes = Vec::new();
            if def.read_only() {
                notes.push("read only");
            }
            if def.protected() {
                notes.push("protected: the tool does not write it");
            }
            if def.needs_servo_off() {
                notes.push("change with the servo off");
            }
            if !notes.is_empty() {
                row("Notes", notes.join(", "), theme::TEXT);
            }
        });
    });
}

/// "See also" and "Mentioned in".
fn links(ui: &mut egui::Ui, schema: &Schema, key: &str, see: &[Target], actions: &mut Vec<Action>) {
    let back: Vec<Target> = docs::backlinks(key).iter().filter_map(|k| Target::parse(k)).filter(|t| !see.contains(t)).collect();
    if see.is_empty() && back.is_empty() {
        return;
    }
    ui.add_space(8.0);
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        for (title, list) in [("See also", see), ("Mentioned in", back.as_slice())] {
            if list.is_empty() {
                continue;
            }
            theme::heading(ui, title);
            for t in list {
                link_line(ui, schema, t, actions);
            }
            ui.add_space(4.0);
        }
    });
}

fn link_line(ui: &mut egui::Ui, schema: &Schema, t: &Target, actions: &mut Vec<Action>) {
    match t {
        Target::Param(code) => match schema.get(code) {
            Ok(d) => {
                if ui.link(RichText::new(format!("{}  {}", d.code, d.name)).color(theme::ACCENT)).clicked() {
                    actions.push(Action::Go(d.code.clone()));
                }
            }
            Err(_) => {
                ui.label(RichText::new(code).color(theme::TEXT_DIM));
            }
        },
        Target::Topic(_) => match docs::page(&t.key()) {
            Some(p) => {
                if ui.link(RichText::new(p.title.clone().unwrap_or_else(|| t.key())).color(theme::ACCENT)).clicked() {
                    actions.push(Action::Go(t.key()));
                }
            }
            None => {
                ui.label(RichText::new(t.key()).color(theme::TEXT_DIM));
            }
        },
        Target::Web(url) => {
            ui.hyperlink_to(RichText::new(url).color(theme::ACCENT), url);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_works_like_a_browser() {
        let mut m = More::default();
        assert!(m.current().is_none());
        m.show("P02-00");
        m.show("P02-10");
        m.show("P02-10");
        m.show("topic:filters");
        assert_eq!(m.current(), Some("topic:filters"));
        m.back();
        m.back();
        assert_eq!(m.current(), Some("P02-00"));
        assert!(!m.can_back());
        m.forward();
        assert_eq!(m.current(), Some("P02-10"));
        m.show("P08-30");
        assert!(!m.can_forward(), "a new page drops the forward pages");
        m.back();
        assert_eq!(m.current(), Some("P02-10"));
    }
}
