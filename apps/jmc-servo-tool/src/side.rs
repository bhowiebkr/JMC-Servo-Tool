//! The left panel: connection, axis and drive status.

use eframe::egui::{self, RichText};
use jmc_core::link::Framing;

use crate::app::App;
use crate::theme;
use crate::worker::Cmd;

pub const AXES: [&str; 4] = ["X", "YL", "YR", "Z"];
const BAUDS: [u32; 5] = [9600, 19200, 38400, 57600, 115200];
const FRAMINGS: [Framing; 4] = [Framing::Even1, Framing::None2, Framing::None1, Framing::Odd1];

impl App {
    pub(crate) fn side_panel(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("side")
            .exact_size(300.0)
            .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(egui::Margin::same(12)))
            .show(root, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                    self.connection_card(ui);
                    ui.add_space(6.0);
                    self.axis_card(ui);
                    ui.add_space(6.0);
                    self.status_card(ui);
                });
            });
    }

    fn connection_card(&mut self, ui: &mut egui::Ui) {
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::heading(ui, "Connection");
            let connected = self.connected.is_some();
            let busy = self.is_busy();
            ui.add_enabled_ui(!connected && !busy, |ui| {
                ui.checkbox(&mut self.settings.sim, "Simulated drive (no hardware)");
                if self.settings.sim {
                    ui.horizontal(|ui| {
                        let name = self
                            .settings
                            .sim_from
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "factory values".into());
                        ui.label(RichText::new(name).color(theme::TEXT_DIM));
                        if ui.small_button("Load…").on_hover_text("Start the simulated drive from a snapshot or JMC export").clicked()
                            && let Some(p) = rfd::FileDialog::new().add_filter("Snapshot or JMC export", &["json", "xml"]).pick_file()
                        {
                            self.settings.sim_from = Some(p);
                        }
                        if self.settings.sim_from.is_some() && ui.small_button("Clear").clicked() {
                            self.settings.sim_from = None;
                        }
                    });
                } else {
                    self.serial_grid(ui);
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if connected {
                    if ui.add_enabled(!busy, egui::Button::new("Disconnect")).clicked() {
                        self.disconnect();
                    }
                    if ui.add_enabled(!busy && !self.monitoring, egui::Button::new("Read all")).clicked() {
                        self.read_all();
                    }
                } else {
                    if ui.add_enabled(!busy, theme::primary("Connect")).clicked() {
                        self.connect();
                    }
                    if !self.settings.sim
                        && ui
                            .add_enabled(!busy, egui::Button::new("Find settings"))
                            .on_hover_text("Try the usual baud rates and framings on this port and keep the one that answers")
                            .clicked()
                    {
                        self.probe();
                    }
                }
            });
        });
    }

    fn serial_grid(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("serial").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label("Port");
            ui.horizontal(|ui| {
                let shown = if self.settings.port.is_empty() { "Choose port".to_string() } else { self.settings.port.clone() };
                egui::ComboBox::from_id_salt("port").width(130.0).selected_text(shown).show_ui(ui, |ui| {
                    if self.ports.is_empty() {
                        ui.label(RichText::new("No serial ports found").color(theme::TEXT_DIM));
                    }
                    for p in &self.ports {
                        let detail = p.product.clone().or_else(|| p.manufacturer.clone()).unwrap_or_else(|| p.kind.clone());
                        ui.selectable_value(&mut self.settings.port, p.name.clone(), format!("{}  {detail}", p.name));
                    }
                });
                if ui.small_button("⟳").on_hover_text("Look for serial ports again").clicked() {
                    self.worker.send(Cmd::ListPorts);
                }
            });
            ui.end_row();

            ui.label("Baud");
            egui::ComboBox::from_id_salt("baud").width(130.0).selected_text(self.settings.baud.to_string()).show_ui(ui, |ui| {
                for b in BAUDS {
                    ui.selectable_value(&mut self.settings.baud, b, b.to_string());
                }
            });
            ui.end_row();

            ui.label("Framing");
            egui::ComboBox::from_id_salt("framing").width(130.0).selected_text(self.settings.framing.to_string()).show_ui(ui, |ui| {
                for f in FRAMINGS {
                    ui.selectable_value(&mut self.settings.framing, f, f.to_string());
                }
            });
            ui.end_row();

            ui.label("Station");
            ui.add(egui::DragValue::new(&mut self.settings.slave).range(1..=247));
            ui.end_row();
        });
    }

    fn axis_card(&mut self, ui: &mut egui::Ui) {
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::heading(ui, "Axis");
            ui.horizontal(|ui| {
                // Framed buttons, so the choices read as buttons and not as plain text.
                for a in AXES {
                    let on = self.settings.axis == a;
                    let b = egui::Button::new(RichText::new(a).strong()).selected(on).min_size(egui::vec2(52.0, 0.0));
                    if ui.add(b).clicked() {
                        self.settings.axis = a.to_string();
                    }
                }
            });
            ui.add_space(2.0);
            ui.label(RichText::new("Baselines folder").small().color(theme::TEXT_DIM));
            let mut reload = false;
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 80.0).max(60.0);
                let r = ui.add(egui::TextEdit::singleline(&mut self.settings.baselines_dir).desired_width(width));
                if r.lost_focus() {
                    reload = true;
                }
                if ui.small_button("Browse…").clicked()
                    && let Some(d) = rfd::FileDialog::new().pick_folder()
                {
                    self.settings.baselines_dir = d.display().to_string();
                    reload = true;
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(&self.baselines_note).small().color(theme::TEXT_DIM));
                if ui.small_button("Reload").clicked() {
                    reload = true;
                }
            });
            if reload {
                self.load_baselines();
            }
            ui.add_space(4.0);
            self.identify_view(ui);
        });
    }

    fn identify_view(&self, ui: &mut egui::Ui) {
        let Some(id) = &self.identify else {
            ui.label(RichText::new("Read a drive to check which axis it is.").color(theme::TEXT_DIM));
            return;
        };
        let best = if id.tied.is_empty() { "nothing".to_string() } else { id.tied.join(" and ") };
        let axis = self.settings.axis.as_str();
        let (text, color) = if id.tied.is_empty() {
            ("No baseline to compare with.".to_string(), theme::TEXT_DIM)
        } else if axis.is_empty() {
            (format!("Best match: {best}. Choose the axis above."), theme::TEXT)
        } else if id.consistent_with(axis) {
            let text = if id.tied.len() > 1 {
                format!("Matches {best} equally; {axis} is one of them.")
            } else {
                format!("Matches the {axis} baseline.")
            };
            (text, theme::OK)
        } else {
            (format!("Does not look like {axis}: best match is {best}."), theme::ERROR)
        };
        ui.label(RichText::new(text).color(color));
        for c in &id.candidates {
            let name = c.file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            ui.label(
                RichText::new(format!("{}: {} of {} differ", c.axis.as_deref().unwrap_or("?"), c.differing, c.compared))
                    .small()
                    .color(theme::TEXT_DIM),
            )
            .on_hover_text(name);
        }
    }

    fn status_card(&mut self, ui: &mut egui::Ui) {
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::heading(ui, "Drive status");
            match &self.status {
                Some(s) => {
                    ui.horizontal_wrapped(|ui| {
                        if s.servo_on {
                            theme::chip(ui, "Servo on", theme::OK);
                        } else {
                            theme::chip(ui, "Servo off", theme::TEXT_DIM);
                        }
                        if s.fault {
                            theme::chip(ui, "Fault", theme::ERROR);
                        }
                        if s.warning {
                            theme::chip(ui, "Warning", theme::ACCENT);
                        }
                    });
                    if !s.alarms.is_empty() {
                        let codes: Vec<String> = s.alarms.iter().map(|a| format!("0x{a:04X}")).collect();
                        ui.label(RichText::new(format!("Alarm registers: {}", codes.join(", "))).color(theme::ERROR));
                    }
                    ui.label(
                        RichText::new(format!("status words 0x{:04X} 0x{:04X}", s.raw[0], s.raw[1])).small().color(theme::TEXT_DIM),
                    );
                }
                None => {
                    ui.label(RichText::new("Not read yet.").color(theme::TEXT_DIM));
                }
            }
            let enabled = self.connected.is_some() && !self.is_busy();
            ui.horizontal(|ui| {
                if ui.add_enabled(enabled, egui::Button::new("Read status")).clicked() {
                    self.worker.send(Cmd::Status);
                }
                if ui.add_enabled(enabled, egui::Button::new("Reset alarm…")).clicked() {
                    self.confirm_reset = true;
                }
            });
        });
    }
}
