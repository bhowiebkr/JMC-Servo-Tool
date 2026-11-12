//! The Parameters tab: the table of values and the detail panel with the editor.

use eframe::egui::{self, RichText};
use egui_extras::{Column, TableBuilder};
use jmc_core::params::{Access, ParamDef, Schema, normalize_code};
use jmc_core::snapshot;

use crate::app::{App, Compare, PendingWrite, Values};
use crate::theme;

/// The filters of the parameter table.
pub struct Filter<'a> {
    pub search: &'a str,
    pub group: Option<u8>,
    pub only_changed: bool,
    pub show_spares: bool,
}

/// Parameters that pass the filter, in table order.
pub fn rows(schema: &'static Schema, values: &Values, compare: Option<&Compare>, f: &Filter) -> Vec<&'static ParamDef> {
    let search = f.search.trim().to_ascii_lowercase();
    let code = normalize_code(f.search.trim());
    schema
        .real()
        .filter(|d| f.show_spares || !d.placeholder())
        .filter(|d| f.group.is_none_or(|g| d.group == g))
        .filter(|d| {
            search.is_empty()
                || code.as_deref() == Some(d.code.as_str())
                || d.code.to_ascii_lowercase().contains(&search)
                || d.name.to_ascii_lowercase().contains(&search)
        })
        .filter(|d| !f.only_changed || differs(d, values, compare))
        .collect()
}

/// The value on show differs from the compare column.
pub fn differs(d: &ParamDef, values: &Values, compare: Option<&Compare>) -> bool {
    match (values.raw.get(&d.code), compare.and_then(|c| c.raw.get(&d.code))) {
        (Some(a), Some(b)) => a != b,
        _ => false,
    }
}

pub fn unit_text(d: &ParamDef) -> &str {
    d.unit.trim()
}

/// When a change takes effect, in words. None when the table does not say.
pub fn applies_text(d: &ParamDef) -> Option<&'static str> {
    d.applies.text()
}

fn value_text(d: &ParamDef, raw: u16) -> String {
    match d.choice_label(raw) {
        Some(l) => format!("{}  {l}", d.format(raw)),
        None => d.format(raw),
    }
}

impl App {
    pub(crate) fn parameters_tab(&mut self, ui: &mut egui::Ui) {
        let compare = self.compare();
        self.values_bar(ui, compare.as_ref());
        ui.add_space(4.0);
        self.filter_bar(ui, compare.is_some());
        ui.add_space(4.0);
        if let Some(b) = &self.busy
            && let Some(p) = b.progress
        {
            ui.add(egui::ProgressBar::new(p).text(format!("{} {:.0}%", b.what, p * 100.0)));
        }
        if self.values.raw.is_empty() {
            ui.add_space(20.0);
            ui.label(
                RichText::new("No values yet. Connect and read a drive, or open a snapshot or JMC export with Open file.")
                    .color(theme::TEXT_DIM),
            );
            ui.add_space(8.0);
        }
        let rows = rows(
            self.schema,
            &self.values,
            compare.as_ref(),
            &Filter { search: &self.search, group: self.group, only_changed: self.only_changed, show_spares: self.show_spares },
        );
        self.table(ui, &rows, compare.as_ref());
    }

    fn values_bar(&mut self, ui: &mut egui::Ui, compare: Option<&Compare>) {
        // The buttons go in first, from the right; the text gets what is left and is cut short there.
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can_save = self.values.from_drive && !self.values.raw.is_empty();
                if ui.add_enabled(can_save, egui::Button::new("Save snapshot…")).clicked() {
                    self.save_snapshot_dialog();
                }
                if self.compare_file.is_some() && ui.button("Compare with baseline").clicked() {
                    self.compare_file = None;
                }
                if ui.button("Compare with…").on_hover_text("Show another snapshot or JMC export in the compare column").clicked()
                    && let Some(p) = rfd::FileDialog::new().add_filter("Snapshot or JMC export", &["json", "xml"]).pick_file()
                {
                    self.open_compare_file(&p);
                }
                let can_open = !self.is_busy() && !self.monitoring;
                if ui
                    .add_enabled(can_open, egui::Button::new("Open file…"))
                    .on_hover_text("Show a snapshot or JMC export instead of the drive's values")
                    .clicked()
                    && let Some(p) = rfd::FileDialog::new().add_filter("Snapshot or JMC export", &["json", "xml"]).pick_file()
                {
                    self.open_values_file(&p);
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let style = ui.style().clone();
                    let mut job = egui::text::LayoutJob::default();
                    let mut add = |text: String, rich: fn(RichText) -> RichText| {
                        rich(RichText::new(text)).append_to(&mut job, &style, egui::FontSelection::Default, egui::Align::Center);
                    };
                    if self.values.raw.is_empty() {
                        add("Values: none".into(), |t| t.color(theme::TEXT_DIM));
                    } else {
                        let what = if self.values.from_drive { "drive" } else { "file" };
                        add(format!("Values from {what}: {}", self.values.source), |t| t.strong().color(theme::TEXT));
                        if !self.values.unread.is_empty() {
                            add(format!("   {} unread", self.values.unread.len()), |t| t.color(theme::ERROR));
                        }
                    }
                    let compared = match compare {
                        Some(c) => format!("   ·   Compared with {}", c.label),
                        None => "   ·   Nothing to compare with".into(),
                    };
                    add(compared, |t| t.color(theme::TEXT_DIM));
                    let full = job.text.clone();
                    ui.add(egui::Label::new(job).truncate()).on_hover_text(full);
                });
            });
        });
    }

    fn filter_bar(&mut self, ui: &mut egui::Ui, has_compare: bool) {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search code or name").desired_width(220.0));
            let shown = match self.group.and_then(|g| self.schema.group(g)) {
                Some(g) => format!("P{:02} {}", g.number, g.title),
                None => "All groups".into(),
            };
            egui::ComboBox::from_id_salt("group").width(240.0).selected_text(shown).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.group, None, "All groups");
                for g in &self.schema.groups {
                    ui.selectable_value(&mut self.group, Some(g.number), format!("P{:02} {}", g.number, g.title));
                }
            });
            ui.add_enabled(has_compare, egui::Checkbox::new(&mut self.only_changed, "Only differences"));
            ui.checkbox(&mut self.show_spares, "Spare slots")
                .on_hover_text("Show the spare registers. They read as values but have no function.");
        });
    }

    fn table(&mut self, ui: &mut egui::Ui, rows: &[&'static ParamDef], compare: Option<&Compare>) {
        let mut clicked = None;
        let compare_title = self.compare_title();
        // Plain labels, so a click anywhere on a row selects it; selectable text would take the click.
        ui.style_mut().interaction.selectable_labels = false;
        // No gaps between rows: the stripes join up and more rows fit.
        ui.spacing_mut().item_spacing.y = 0.0;
        TableBuilder::new(ui)
            .id_salt("params")
            .striped(true)
            .sense(egui::Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(64.0))
            .column(Column::initial(250.0).at_least(120.0).clip(true).resizable(true))
            .column(Column::initial(120.0).at_least(70.0).clip(true).resizable(true))
            .column(Column::initial(120.0).at_least(70.0).clip(true).resizable(true))
            .column(Column::initial(80.0).at_least(50.0).clip(true).resizable(true))
            .column(Column::initial(70.0).at_least(40.0).clip(true).resizable(true))
            .column(Column::remainder().at_least(80.0).clip(true))
            .header(28.0, |mut h| {
                for title in ["Code", "Name", "Value", compare_title, "Default", "Unit", "Range"] {
                    h.col(|ui| {
                        ui.label(RichText::new(title).strong().color(theme::TEXT_DIM));
                    });
                }
            })
            .body(|body| {
                body.rows(26.0, rows.len(), |mut row| {
                    let d = rows[row.index()];
                    row.set_selected(self.selected.as_deref() == Some(d.code.as_str()));
                    let changed = differs(d, &self.values, compare);
                    row.col(|ui| {
                        ui.label(RichText::new(&d.code).monospace());
                    });
                    row.col(|ui| {
                        ui.label(&d.name).on_hover_text(&d.name);
                    });
                    row.col(|ui| match (self.values.raw.get(&d.code), self.values.unread.get(&d.code)) {
                        (Some(&raw), _) => {
                            let color = if changed { theme::ACCENT } else { theme::TEXT };
                            ui.label(RichText::new(value_text(d, raw)).color(color));
                        }
                        (None, Some(why)) => {
                            ui.label(RichText::new("unread").color(theme::ERROR)).on_hover_text(why);
                        }
                        (None, None) => {
                            ui.label(RichText::new("—").color(theme::TEXT_DIM));
                        }
                    });
                    row.col(|ui| match compare.and_then(|c| c.raw.get(&d.code)) {
                        Some(&raw) => {
                            let color = if changed { theme::TEXT } else { theme::TEXT_DIM };
                            ui.label(RichText::new(value_text(d, raw)).color(color));
                        }
                        None => {
                            ui.label(RichText::new("—").color(theme::TEXT_DIM));
                        }
                    });
                    row.col(|ui| {
                        let text = d.default.map(|v| d.format_value(v)).unwrap_or_default();
                        ui.label(RichText::new(text).color(theme::TEXT_DIM));
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(unit_text(d)).color(theme::TEXT_DIM));
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(d.range_text()).color(theme::TEXT_DIM));
                    });
                    if row.response().clicked() {
                        clicked = Some(d);
                    }
                });
            });
        if let Some(d) = clicked {
            self.select(d);
        }
    }

    /// What the compare column is called: the axis baseline, or a file picked with Compare with.
    fn compare_title(&self) -> &'static str {
        if self.compare_file.is_some() { "Compare" } else { "Baseline" }
    }

    pub fn select(&mut self, d: &ParamDef) {
        self.selected = Some(d.code.clone());
        self.edit_text = self.values.raw.get(&d.code).map(|&r| d.format(r)).unwrap_or_default();
    }

    fn save_snapshot_dialog(&mut self) {
        let Some(s) = self.snapshot_of_values() else { return };
        let axis = s.axis.clone().unwrap_or_else(|| "drive".into());
        let name = format!("{axis}-{}.json", snapshot::stamp_for_file(&s.taken_utc));
        let mut dialog = rfd::FileDialog::new().add_filter("Snapshot", &["json"]).set_file_name(&name);
        if !self.settings.baselines_dir.trim().is_empty() {
            dialog = dialog.set_directory(self.settings.baselines_dir.trim());
        }
        let Some(path) = dialog.save_file() else { return };
        match s.save(&path) {
            Ok(()) => self.info(format!("Saved snapshot {}", path.display())),
            Err(e) => self.error(format!("Cannot save {}: {e}", path.display())),
        }
    }

    pub(crate) fn detail_panel(&mut self, root: &mut egui::Ui) {
        egui::Panel::right("detail")
            .resizable(true)
            .default_size(340.0)
            .min_size(260.0)
            .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(egui::Margin::same(12)))
            .show(root, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                    let def = self.selected.as_deref().and_then(|c| self.schema.get(c).ok());
                    match def {
                        Some(d) => self.detail(ui, d),
                        None => {
                            ui.label(RichText::new("Click a parameter to see it here.").color(theme::TEXT_DIM));
                        }
                    }
                });
            });
    }

    fn detail(&mut self, ui: &mut egui::Ui, d: &'static ParamDef) {
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(&d.code).monospace().color(theme::ACCENT));
            ui.label(RichText::new(&d.name).size(16.0).strong());
            if let Some(g) = self.schema.group(d.group) {
                ui.label(RichText::new(format!("P{:02} {}", g.number, g.title)).small().color(theme::TEXT_DIM));
            }
            ui.add_space(4.0);
            let compare = self.compare();
            egui::Grid::new("detail-grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                let line = |ui: &mut egui::Ui, k: &str, v: String| {
                    ui.label(RichText::new(k).color(theme::TEXT_DIM));
                    ui.label(v);
                    ui.end_row();
                };
                if let Some(&raw) = self.values.raw.get(&d.code) {
                    line(ui, "Value", value_text(d, raw));
                }
                if let Some(why) = self.values.unread.get(&d.code) {
                    line(ui, "Value", format!("unread: {why}"));
                }
                if let Some(c) = &compare
                    && let Some(&raw) = c.raw.get(&d.code)
                {
                    line(ui, self.compare_title(), value_text(d, raw));
                }
                if let Some(v) = d.default {
                    line(ui, "Default", d.format_value(v));
                }
                line(ui, "Range", d.range_text());
                if !unit_text(d).is_empty() {
                    line(ui, "Unit", unit_text(d).to_string());
                }
                if let Some(t) = applies_text(d) {
                    line(ui, "Takes effect", t.to_string());
                }
                if d.needs_servo_off() {
                    line(ui, "Change", "Only with the servo off".to_string());
                }
                match d.access {
                    Access::User => {}
                    Access::Motor => line(ui, "Access", "Motor data (protected)".to_string()),
                    Access::Factory => line(ui, "Access", "Factory setting (protected)".to_string()),
                }
                line(ui, "Register", format!("{} (0x{:04X})", d.address, d.address));
            });
        });
        // The editor before the description, so the write button is on screen without scrolling.
        ui.add_space(6.0);
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::heading(ui, "Change");
            match self.write_block(d) {
                Some(why) => {
                    ui.label(RichText::new(why).color(theme::TEXT_DIM));
                }
                None => self.editor(ui, d),
            }
        });

        ui.add_space(6.0);
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                theme::heading(ui, "About");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("More…").on_hover_text("The full page: drawing, links and \"Try a value\"").clicked() {
                        self.more.show(&d.code);
                    }
                });
            });
            ui.label(RichText::new(d.brief.trim()).color(theme::TEXT_DIM));
            if !d.choices.is_empty() {
                ui.add_space(4.0);
                for c in &d.choices {
                    ui.label(RichText::new(format!("{} = {}", d.format_value(c.value), c.label)).small().color(theme::TEXT_DIM));
                }
            }
        });
    }

    fn editor(&mut self, ui: &mut egui::Ui, d: &'static ParamDef) {
        let Some(&current) = self.values.raw.get(&d.code) else { return };
        ui.horizontal(|ui| {
            if d.choices.is_empty() {
                ui.add(egui::TextEdit::singleline(&mut self.edit_text).desired_width(140.0));
                ui.label(RichText::new(unit_text(d)).color(theme::TEXT_DIM));
            } else {
                let shown = d
                    .parse_raw(self.edit_text.trim())
                    .ok()
                    .map(|r| value_text(d, r))
                    .unwrap_or_else(|| self.edit_text.clone());
                egui::ComboBox::from_id_salt("choice").width(220.0).selected_text(shown).show_ui(ui, |ui| {
                    for c in &d.choices {
                        let text = d.format_value(c.value);
                        ui.selectable_value(&mut self.edit_text, text.clone(), format!("{text}  {}", c.label));
                    }
                });
            }
        });
        match d.parse(self.edit_text.trim()) {
            Err(e) => {
                ui.label(RichText::new(e.to_string()).color(theme::ERROR));
            }
            Ok(raw) if raw == current => {
                ui.label(RichText::new("Same as the drive.").color(theme::TEXT_DIM));
            }
            Ok(raw) => {
                ui.label(RichText::new(format!("{}  →  {}", d.format(current), d.format(raw))).strong());
                if ui.add_enabled(!self.is_busy(), theme::primary("Write to RAM…")).clicked() {
                    self.pending_write = Some(PendingWrite {
                        code: d.code.clone(),
                        name: d.name.clone(),
                        from: d.format(current),
                        to: d.format(raw),
                    });
                }
            }
        }
        if d.needs_servo_off() {
            ui.label(RichText::new("The drive accepts this one only with the servo off.").small().color(theme::ACCENT));
        }
        ui.label(
            RichText::new("RAM only. Saving to EEPROM stays in the command-line tool until the save is confirmed on these drives.")
                .small()
                .color(theme::TEXT_DIM),
        );
    }
}
