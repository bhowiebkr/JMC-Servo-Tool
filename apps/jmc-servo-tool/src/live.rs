//! The Live tab: start and stop the monitor, choose the strips, save the samples.

use std::collections::VecDeque;
use std::fmt::Write as _;

use eframe::egui::{self, RichText};
use jmc_core::monitor::Sample;

use crate::app::App;
use crate::plot::{self, CHANNELS};
use crate::theme;

const INTERVALS_MS: [u64; 4] = [20, 50, 100, 250];
const WINDOWS_S: [f64; 4] = [5.0, 10.0, 30.0, 60.0];

/// The samples as CSV, one row per sample, time in seconds from the start.
pub fn samples_csv(samples: &VecDeque<(f64, Sample)>) -> String {
    let mut out = String::from(
        "t_s,pos_cmd,pos_fb,following_error,speed_cmd_rpm,speed_rpm,torque_cmd_pct,torque_pct,load_pct,\
         peak_torque_pct,regen_pct,io,angle_deg,abs_turns,abs_pos,bus_v,ain_speed_v,ain_torque_v\n",
    );
    for (t, s) in samples {
        let _ = writeln!(
            out,
            "{t:.4},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            s.pos_cmd,
            s.pos_fb,
            s.following_error,
            s.speed_cmd_rpm,
            s.speed_rpm,
            s.torque_cmd_pct,
            s.torque_pct,
            s.load_pct,
            s.peak_torque_pct,
            s.regen_pct,
            s.io,
            s.angle_deg,
            s.abs_turns,
            s.abs_pos,
            s.bus_v,
            s.ain_speed_v,
            s.ain_torque_v
        );
    }
    out
}

/// Samples per second over the last `window` seconds.
pub fn sample_rate(samples: &VecDeque<(f64, Sample)>, window: f64) -> Option<f64> {
    let &(last, _) = samples.back()?;
    let recent: Vec<f64> = samples.iter().rev().map(|(t, _)| *t).take_while(|t| *t >= last - window).collect();
    let first = *recent.last()?;
    (recent.len() >= 2 && last > first).then(|| (recent.len() - 1) as f64 / (last - first))
}

impl App {
    pub(crate) fn live_tab(&mut self, ui: &mut egui::Ui) {
        let connected = self.connected.is_some();
        ui.horizontal(|ui| {
            if self.monitoring {
                if ui.button("Stop").clicked() {
                    self.set_monitor(false);
                }
            } else if ui.add_enabled(connected && !self.is_busy(), theme::primary("Start")).clicked() {
                self.set_monitor(true);
            }
            ui.label("Every");
            ui.add_enabled_ui(!self.monitoring, |ui| {
                egui::ComboBox::from_id_salt("interval")
                    .width(80.0)
                    .selected_text(format!("{} ms", self.settings.monitor_ms))
                    .show_ui(ui, |ui| {
                        for ms in INTERVALS_MS {
                            ui.selectable_value(&mut self.settings.monitor_ms, ms, format!("{ms} ms"));
                        }
                    });
            });
            ui.label("Show");
            egui::ComboBox::from_id_salt("window").width(80.0).selected_text(format!("{} s", self.settings.window_s)).show_ui(
                ui,
                |ui| {
                    for w in WINDOWS_S {
                        ui.selectable_value(&mut self.settings.window_s, w, format!("{w} s"));
                    }
                },
            );
            if let Some(rate) = sample_rate(&self.samples, self.settings.window_s) {
                ui.label(RichText::new(format!("{rate:.0} samples/s")).color(theme::TEXT_DIM));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let have = !self.samples.is_empty();
                if ui.add_enabled(have && !self.monitoring, egui::Button::new("Clear")).clicked() {
                    self.samples.clear();
                }
                if ui.add_enabled(have, egui::Button::new("Save CSV…")).clicked() {
                    self.save_csv_dialog();
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            for ch in CHANNELS {
                let mut on = self.settings.channels.iter().any(|k| k == ch.key);
                if ui.checkbox(&mut on, ch.title).changed() {
                    self.settings.channels.retain(|k| k != ch.key);
                    if on {
                        self.settings.channels.push(ch.key.to_string());
                    }
                }
            }
        });
        ui.add_space(4.0);
        if self.samples.is_empty() {
            let hint = if connected {
                "Press Start to read the drive's monitor registers."
            } else {
                "Connect to a drive, or the simulated one, to watch live values."
            };
            ui.label(RichText::new(hint).color(theme::TEXT_DIM));
            ui.label(
                RichText::new("The monitor register layout is not yet checked on a real drive; treat the numbers with care.")
                    .small()
                    .color(theme::TEXT_DIM),
            );
            return;
        }
        // The numbers first, so they stay in view; the strips share the height left under them.
        if let Some((_, s)) = self.samples.back() {
            theme::card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                readout_grid(ui, s);
            });
            ui.add_space(4.0);
        }
        let shown: Vec<_> = CHANNELS.iter().filter(|c| self.settings.channels.iter().any(|k| k == c.key)).collect();
        let spacing = ui.spacing().item_spacing.y;
        let per = if shown.is_empty() {
            0.0
        } else {
            ((ui.available_height() - spacing * shown.len() as f32) / shown.len() as f32).clamp(90.0, 240.0)
        };
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            for ch in &shown {
                plot::strip(ui, ch, &self.samples, self.settings.window_s, per);
            }
        });
    }

    fn save_csv_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("live.csv").save_file() else {
            return;
        };
        match std::fs::write(&path, samples_csv(&self.samples)) {
            Ok(()) => self.info(format!("Saved {} samples to {}", self.samples.len(), path.display())),
            Err(e) => self.error(format!("Cannot save {}: {e}", path.display())),
        }
    }
}

fn readout_grid(ui: &mut egui::Ui, s: &Sample) {
    let items: [(&str, String); 12] = [
        ("Position command", s.pos_cmd.to_string()),
        ("Position feedback", s.pos_fb.to_string()),
        ("Following error", s.following_error.to_string()),
        ("Angle", format!("{:.1}°", s.angle_deg)),
        ("Speed command", format!("{} rpm", s.speed_cmd_rpm)),
        ("Speed", format!("{} rpm", s.speed_rpm)),
        ("Torque", format!("{:.1} %", s.torque_pct)),
        ("Peak torque", format!("{:.1} %", s.peak_torque_pct)),
        ("Load", format!("{:.1} %", s.load_pct)),
        ("Regen", format!("{:.1} %", s.regen_pct)),
        ("Bus", format!("{} V", s.bus_v)),
        ("I/O", format!("{:016b}", s.io)),
    ];
    // Four pairs a row: three rows instead of four.
    egui::Grid::new("readout").num_columns(8).spacing([14.0, 2.0]).min_col_width(40.0).show(ui, |ui| {
        for (i, (k, v)) in items.iter().enumerate() {
            ui.label(RichText::new(*k).small().color(theme::TEXT_DIM));
            ui.label(RichText::new(v).monospace());
            if i % 4 == 3 {
                ui.end_row();
            }
        }
    });
}
