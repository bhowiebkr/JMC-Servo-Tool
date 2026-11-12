//! Window state, drive events, the top bar, the log and the confirmation dialogs.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Duration;

use eframe::egui::{self, RichText};
use jmc_core::guard::WriteReport;
use jmc_core::link::{LinkStats, PortInfo, SerialSettings};
use jmc_core::monitor::{Sample, Status};
use jmc_core::params::{ParamDef, Schema};
use jmc_core::snapshot::{self, Identification, Snapshot};

use crate::settings::Settings;
use crate::theme;
use crate::worker::{Cmd, Event, Target, Worker, WriteJob};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Parameters,
    Live,
}

/// The parameter values on show: from the drive or from a file.
#[derive(Debug, Clone, Default)]
pub struct Values {
    /// Where they came from, e.g. "COM5 57600 8E1" or "X-20230725-142138.json".
    pub source: String,
    pub raw: BTreeMap<String, u16>,
    /// Parameters the drive refused or the file left unread.
    pub unread: BTreeMap<String, String>,
    pub from_drive: bool,
}

/// What the "compare" column shows.
#[derive(Debug, Clone)]
pub struct Compare {
    pub label: String,
    pub raw: BTreeMap<String, u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Error,
}

pub struct LogLine {
    pub time: String,
    pub level: Level,
    pub text: String,
}

/// A write waiting for the user's yes.
#[derive(Debug, Clone)]
pub struct PendingWrite {
    pub code: String,
    pub name: String,
    pub from: String,
    pub to: String,
}

pub struct Busy {
    pub what: &'static str,
    pub progress: Option<f32>,
}

pub struct App {
    pub schema: &'static Schema,
    pub worker: Worker,
    pub settings: Settings,
    saved_settings: Settings,
    settings_path: Option<PathBuf>,

    pub ports: Vec<PortInfo>,
    /// Label of the open link.
    pub connected: Option<String>,
    pub busy: Option<Busy>,
    pub stats: Option<LinkStats>,

    pub values: Values,
    pub baselines: Vec<(PathBuf, Snapshot)>,
    pub baselines_note: String,
    /// A file chosen with "Compare with"; otherwise the axis baseline is used.
    pub compare_file: Option<Compare>,
    pub identify: Option<Identification>,

    pub tab: Tab,
    pub search: String,
    pub group: Option<u8>,
    pub only_changed: bool,
    pub show_spares: bool,
    pub show_factory: bool,
    pub selected: Option<String>,
    pub edit_text: String,
    pub pending_write: Option<PendingWrite>,
    pub last_report: Option<WriteReport>,

    pub monitoring: bool,
    pub samples: VecDeque<(f64, Sample)>,
    pub status: Option<Status>,
    pub confirm_reset: bool,

    /// The More window: help pages with drawings.
    pub more: crate::more::More,

    pub log: Vec<LogLine>,
}

/// Samples older than this are dropped.
const KEEP_SECONDS: f64 = 120.0;

impl App {
    pub fn new(ctx: egui::Context, settings_path: Option<PathBuf>) -> App {
        let settings = settings_path.as_deref().map(Settings::load).unwrap_or_default();
        let worker = Worker::spawn(ctx);
        worker.send(Cmd::ListPorts);
        let mut app = App {
            schema: Schema::model512(),
            worker,
            saved_settings: settings.clone(),
            settings,
            settings_path,
            ports: Vec::new(),
            connected: None,
            busy: None,
            stats: None,
            values: Values::default(),
            baselines: Vec::new(),
            baselines_note: String::new(),
            compare_file: None,
            identify: None,
            tab: Tab::Parameters,
            search: String::new(),
            group: None,
            only_changed: false,
            show_spares: false,
            show_factory: false,
            selected: None,
            edit_text: String::new(),
            pending_write: None,
            last_report: None,
            monitoring: false,
            samples: VecDeque::new(),
            status: None,
            confirm_reset: false,
            more: crate::more::More::default(),
            log: Vec::new(),
        };
        app.load_baselines();
        app
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.push_log(Level::Info, text.into());
    }

    pub fn warn(&mut self, text: impl Into<String>) {
        self.push_log(Level::Warn, text.into());
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.push_log(Level::Error, text.into());
    }

    fn push_log(&mut self, level: Level, text: String) {
        let now = snapshot::now_utc();
        let time = now.get(11..19).unwrap_or("").to_string();
        self.log.push(LogLine { time, level, text });
        if self.log.len() > 500 {
            self.log.drain(..100);
        }
    }

    pub fn is_busy(&self) -> bool {
        self.busy.is_some()
    }

    pub fn start(&mut self, what: &'static str, cmd: Cmd) {
        self.busy = Some(Busy { what, progress: None });
        self.worker.send(cmd);
    }

    // ---- Connection ----

    pub fn connect(&mut self) {
        let s = &self.settings;
        let target = if s.sim {
            Target::Sim(s.sim_from.clone())
        } else {
            if s.port.trim().is_empty() {
                self.error("Choose a serial port first.");
                return;
            }
            Target::Serial(SerialSettings { port: s.port.trim().to_string(), baud: s.baud, framing: s.framing })
        };
        let cmd = Cmd::Connect { target, slave: s.slave, max_read: s.max_read };
        self.start("Connecting", cmd);
    }

    pub fn probe(&mut self) {
        if self.settings.port.trim().is_empty() {
            self.error("Choose a serial port first.");
            return;
        }
        let cmd = Cmd::Probe { port: self.settings.port.trim().to_string(), slave: self.settings.slave, max_read: self.settings.max_read };
        self.start("Trying baud rates", cmd);
    }

    pub fn disconnect(&mut self) {
        self.worker.send(Cmd::Disconnect);
    }

    pub fn read_all(&mut self) {
        self.start("Reading all parameters", Cmd::ReadAll);
    }

    pub fn set_monitor(&mut self, on: bool) {
        if on {
            self.samples.clear();
            self.monitoring = true;
            self.worker.send(Cmd::Monitor(Some(Duration::from_millis(self.settings.monitor_ms.max(10)))));
        } else {
            self.monitoring = false;
            self.worker.send(Cmd::Monitor(None));
        }
    }

    // ---- Baselines and comparison ----

    pub fn load_baselines(&mut self) {
        self.baselines.clear();
        let dir = self.settings.baselines_dir.trim().to_string();
        if dir.is_empty() {
            self.baselines_note = "No baselines folder set.".into();
        } else {
            match snapshot::load_baselines(Path::new(&dir)) {
                Ok(b) => {
                    let mut axes: Vec<String> = b.iter().filter_map(|(_, s)| s.axis.clone()).collect();
                    axes.sort();
                    axes.dedup();
                    self.baselines_note = format!("{} baselines: {}", b.len(), axes.join(", "));
                    self.baselines = b;
                }
                Err(e) => self.baselines_note = format!("Cannot read baselines: {e}"),
            }
        }
        self.update_identify();
    }

    pub fn has_baseline_for_axis(&self) -> bool {
        let axis = &self.settings.axis;
        !axis.is_empty() && self.baselines.iter().any(|(_, s)| s.axis.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(axis)))
    }

    /// The newest baseline for the chosen axis.
    pub fn axis_baseline(&self) -> Option<(&Path, &Snapshot)> {
        let axis = &self.settings.axis;
        self.baselines
            .iter()
            .filter(|(_, s)| s.axis.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(axis)))
            .max_by(|a, b| a.1.taken_utc.cmp(&b.1.taken_utc))
            .map(|(p, s)| (p.as_path(), s))
    }

    /// The compare column: a chosen file, else the axis baseline.
    pub fn compare(&self) -> Option<Compare> {
        if let Some(c) = &self.compare_file {
            return Some(c.clone());
        }
        let (path, s) = self.axis_baseline()?;
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Some(Compare { label: format!("baseline {name}"), raw: s.raw(self.schema).ok()? })
    }

    pub fn update_identify(&mut self) {
        self.identify = (!self.values.raw.is_empty() && !self.baselines.is_empty())
            .then(|| snapshot::identify(self.schema, &self.values.raw, &self.baselines));
    }

    pub fn open_values_file(&mut self, path: &Path) {
        match Snapshot::load_any(self.schema, path).and_then(|s| Ok((s.raw(self.schema)?, s))) {
            Ok((raw, s)) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.values = Values { source: name.clone(), raw, unread: s.unread, from_drive: false };
                self.info(format!("Showing {name} (a file, not the drive)."));
                self.update_identify();
            }
            Err(e) => self.error(format!("Cannot open {}: {e}", path.display())),
        }
    }

    pub fn open_compare_file(&mut self, path: &Path) {
        match Snapshot::load_any(self.schema, path).and_then(|s| s.raw(self.schema)) {
            Ok(raw) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.compare_file = Some(Compare { label: name, raw });
            }
            Err(e) => self.error(format!("Cannot open {}: {e}", path.display())),
        }
    }

    /// A snapshot of what is on show, if it came from the drive.
    pub fn snapshot_of_values(&self) -> Option<Snapshot> {
        if !self.values.from_drive || self.values.raw.is_empty() {
            return None;
        }
        let mut s = Snapshot::from_raw(self.schema, &format!("drive on {}", self.values.source), &self.values.raw);
        s.axis = (!self.settings.axis.is_empty()).then(|| self.settings.axis.clone());
        s.unread = self.values.unread.clone();
        Some(s)
    }

    // ---- Writing ----

    /// Why `def` cannot be written from the window right now, if it cannot.
    pub fn write_block(&self, def: &ParamDef) -> Option<String> {
        use jmc_core::drive::{Drive, Persist, WriteOptions};
        if let Err(e) = Drive::check_write(def, WriteOptions { persist: Persist::Ram, allow_protected: false }) {
            return Some(format!("Not writable here: {e}"));
        }
        if def.needs_power_cycle() {
            return Some(
                "Takes effect only after a power cycle, and a power cycle clears a RAM write. It needs an EEPROM save, \
                 which stays in the command-line tool (jmc write --save) until the save is confirmed on these drives."
                    .into(),
            );
        }
        if self.connected.is_none() {
            return Some("Connect to a drive to change values.".into());
        }
        if !self.values.from_drive || !self.values.raw.contains_key(&def.code) {
            return Some("Read the drive first.".into());
        }
        if self.settings.axis.is_empty() {
            return Some("Choose the axis this drive is on.".into());
        }
        if !self.has_baseline_for_axis() {
            return Some(format!("No baseline for axis {}: set the baselines folder.", self.settings.axis));
        }
        if self.monitoring {
            return Some("Stop the live monitor first.".into());
        }
        if let Some(id) = &self.identify
            && !id.consistent_with(&self.settings.axis)
        {
            return Some(format!(
                "This drive does not look like axis {}: best match is {}.",
                self.settings.axis,
                if id.tied.is_empty() { "nothing".into() } else { id.tied.join("/") }
            ));
        }
        None
    }

    pub fn confirm_write(&mut self) {
        let Some(p) = self.pending_write.take() else { return };
        let job = WriteJob {
            axis: self.settings.axis.clone(),
            items: vec![format!("{}={}", p.code, p.to)],
            baselines_dir: PathBuf::from(self.settings.baselines_dir.trim()),
            backup_dir: self.settings.backup_dir(),
        };
        self.info(format!("Writing {} = {} to RAM (axis {}).", p.code, p.to, job.axis));
        self.start("Writing", Cmd::Write(job));
    }

    // ---- Events from the drive thread ----

    pub fn handle_events(&mut self) {
        for e in self.worker.poll() {
            self.handle(e);
        }
    }

    pub fn handle(&mut self, e: Event) {
        match e {
            Event::Ports(p) => {
                if self.settings.port.is_empty()
                    && let [only] = p.as_slice()
                {
                    self.settings.port = only.name.clone();
                }
                self.ports = p;
            }
            Event::Connected { label } => {
                self.busy = None;
                self.info(format!("Connected: {label}."));
                self.connected = Some(label);
                self.status = None;
                self.read_all();
            }
            Event::Probed(hit) => {
                self.settings.baud = hit.settings.baud;
                self.settings.framing = hit.settings.framing;
                self.info(format!("The drive answers at {} {} (tried {}).", hit.settings.baud, hit.settings.framing, hit.tried));
            }
            Event::Disconnected => {
                self.connected = None;
                self.monitoring = false;
                self.busy = None;
                self.info("Disconnected.");
            }
            Event::Progress { done, total } => {
                if let Some(b) = &mut self.busy {
                    b.progress = Some(done as f32 / total.max(1) as f32);
                }
            }
            Event::ReadAll { values, errors, label } => {
                self.busy = None;
                let n = values.len();
                if !errors.is_empty() {
                    self.warn(format!("{} parameters did not read: {}", errors.len(), errors.keys().cloned().collect::<Vec<_>>().join(", ")));
                }
                self.values = Values { source: label, raw: values, unread: errors, from_drive: true };
                self.update_identify();
                self.info(format!("Read {n} parameters."));
                if let Some(id) = &self.identify {
                    let text = match &id.axis {
                        Some(a) => format!("This drive matches the {a} baseline best."),
                        None if id.tied.is_empty() => "No baseline to compare with.".into(),
                        None => format!("This drive matches {} equally well.", id.tied.join(" and ")),
                    };
                    let mismatch = !self.settings.axis.is_empty() && !id.consistent_with(&self.settings.axis);
                    self.info(text);
                    if mismatch {
                        self.warn(format!("The chosen axis is {}, but the drive does not match its baseline best.", self.settings.axis));
                    }
                }
            }
            Event::Written(r) => {
                self.busy = None;
                for o in &r.results {
                    if let Ok(def) = self.schema.get(&o.code)
                        && let Ok(raw) = def.parse_raw(&o.after)
                    {
                        self.values.raw.insert(o.code.clone(), raw);
                        if self.selected.as_deref() == Some(o.code.as_str()) {
                            self.edit_text = o.after.clone();
                        }
                    }
                    if o.written {
                        self.info(format!("{} ({}): {} → {} (read back {}).", o.code, o.name, o.before, o.requested, o.after));
                    } else {
                        self.info(format!("{} was already {}; nothing written.", o.code, o.after));
                    }
                    for n in &o.notes {
                        self.warn(format!("{}: {n}", o.code));
                    }
                }
                if let Some(err) = &r.error {
                    self.error(format!("Write failed: {err}"));
                }
                for w in &r.warnings {
                    self.warn(sentence(w));
                }
                self.info(format!("Backup saved first: {}", r.backup.display()));
                self.update_identify();
                self.last_report = Some(*r);
            }
            Event::Status(s) => self.status = Some(s),
            Event::AlarmReset => self.info("Alarm reset sent."),
            Event::Sample { t, sample } => {
                self.samples.push_back((t, sample));
                while self.samples.front().is_some_and(|(t0, _)| *t0 < t - KEEP_SECONDS) {
                    self.samples.pop_front();
                }
            }
            Event::MonitorStopped { reason } => {
                self.monitoring = false;
                if let Some(r) = reason {
                    self.error(format!("Live monitor stopped: {r}"));
                }
            }
            Event::Stats(s) => self.stats = Some(s),
            Event::Error { what, message } => {
                self.busy = None;
                if what == "monitor" {
                    self.monitoring = false;
                }
                self.error(format!("{what}: {message}"));
            }
        }
    }

    fn save_settings(&mut self) {
        if self.settings == self.saved_settings {
            return;
        }
        self.saved_settings = self.settings.clone();
        if let Some(p) = &self.settings_path
            && let Err(e) = self.settings.save(p)
        {
            self.warn(format!("Cannot save settings to {}: {e}", p.display()));
        }
    }

    // ---- Drawing ----

    /// The whole window. Kept apart from `eframe::App` so tests can run it without a window.
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        self.handle_events();
        self.top_bar(ui);
        self.log_panel(ui);
        self.side_panel(ui);
        if self.tab == Tab::Parameters {
            self.detail_panel(ui);
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(egui::Margin::symmetric(14, 10)))
            .show(ui, |ui| match self.tab {
                Tab::Parameters => self.parameters_tab(ui),
                Tab::Live => self.live_tab(ui),
            });
        let ctx = ui.ctx().clone();
        self.write_dialog(&ctx);
        self.reset_dialog(&ctx);
        self.more_window(&ctx);
        self.save_settings();
        if self.monitoring || self.busy.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn top_bar(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("top")
            .exact_size(46.0)
            .frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(16, 8)))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("JMC Servo Tool").size(17.0).strong());
                    ui.label(RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).size(12.0).color(theme::TEXT_DIM))
                        .on_hover_text("Version of this tool. See CHANGELOG.md for what changed.");
                    ui.add_space(16.0);
                    ui.selectable_value(&mut self.tab, Tab::Parameters, "Parameters");
                    ui.selectable_value(&mut self.tab, Tab::Live, "Live");
                    ui.add_space(16.0);
                    if let Some(b) = &self.busy {
                        ui.spinner();
                        let text = match b.progress {
                            Some(p) => format!("{} {:.0}%", b.what, p * 100.0),
                            None => format!("{}…", b.what),
                        };
                        ui.label(RichText::new(text).color(theme::TEXT_DIM));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        match &self.connected {
                            Some(l) => theme::chip(ui, &format!("Connected: {l}"), theme::OK),
                            None => theme::chip(ui, "Not connected", theme::TEXT_DIM),
                        }
                        if let Some(s) = &self.stats
                            && (s.timeouts > 0 || s.crc_errors > 0)
                        {
                            ui.label(
                                RichText::new(format!("{} timeouts, {} CRC errors", s.timeouts, s.crc_errors))
                                    .color(theme::ERROR)
                                    .small(),
                            );
                        }
                    });
                });
            });
    }

    fn log_panel(&mut self, root: &mut egui::Ui) {
        egui::Panel::bottom("log")
            .resizable(true)
            .default_size(110.0)
            .min_size(50.0)
            .frame(egui::Frame::new().fill(theme::CANVAS).inner_margin(egui::Margin::symmetric(12, 6)))
            .show(root, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false; 2]).stick_to_bottom(true).show(ui, |ui| {
                    // One label per line: a horizontal row would stretch each line to a button's height.
                    ui.spacing_mut().item_spacing.y = 2.0;
                    if self.log.is_empty() {
                        ui.label(RichText::new("Messages appear here.").color(theme::TEXT_DIM).small());
                    }
                    let style = ui.style().clone();
                    for l in &self.log {
                        let color = match l.level {
                            Level::Info => theme::TEXT,
                            Level::Warn => theme::ACCENT,
                            Level::Error => theme::ERROR,
                        };
                        let mut job = egui::text::LayoutJob::default();
                        RichText::new(format!("{}   ", l.time)).monospace().small().color(theme::TEXT_DIM).append_to(
                            &mut job,
                            &style,
                            egui::FontSelection::Default,
                            egui::Align::Center,
                        );
                        RichText::new(&l.text).color(color).append_to(&mut job, &style, egui::FontSelection::Default, egui::Align::Center);
                        ui.label(job);
                    }
                });
            });
    }

    fn write_dialog(&mut self, ctx: &egui::Context) {
        let Some(p) = self.pending_write.clone() else { return };
        let mut decision = None;
        let resp = egui::Modal::new(egui::Id::unique("confirm-write")).show(ctx, |ui| {
            ui.set_max_width(460.0);
            ui.heading("Write to the drive?");
            ui.add_space(4.0);
            egui::Grid::new("write-grid").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                ui.label("Parameter");
                ui.label(format!("{} {}", p.code, p.name));
                ui.end_row();
                ui.label("Change");
                ui.label(RichText::new(format!("{}  →  {}", p.from, p.to)).strong());
                ui.end_row();
                ui.label("Drive");
                ui.label(self.connected.clone().unwrap_or_default());
                ui.end_row();
                ui.label("Axis");
                ui.label(&self.settings.axis);
                ui.end_row();
            });
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "RAM only: the drive forgets it at power-off. First the tool reads the whole drive, checks it \
                     matches this axis's baseline, and saves a backup. Then it writes and reads the value back.",
                )
                .color(theme::TEXT_DIM),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add(theme::primary("Write to RAM")).clicked() {
                    decision = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    decision = Some(false);
                }
            });
        });
        if resp.should_close() && decision.is_none() {
            decision = Some(false);
        }
        match decision {
            Some(true) => {
                self.pending_write = Some(p);
                self.confirm_write();
            }
            Some(false) => self.pending_write = None,
            None => {}
        }
    }

    fn reset_dialog(&mut self, ctx: &egui::Context) {
        if !self.confirm_reset {
            return;
        }
        let mut decision = None;
        let resp = egui::Modal::new(egui::Id::unique("confirm-reset")).show(ctx, |ui| {
            ui.set_max_width(440.0);
            ui.heading("Reset the drive's alarm?");
            ui.add_space(4.0);
            ui.label(
                "If servo-on is wired permanently on, the motor can power up again as soon as the alarm clears. \
                 Make sure the axis is safe to move. If the cause is still there, the alarm comes back.",
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add(theme::primary("Reset alarm")).clicked() {
                    decision = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    decision = Some(false);
                }
            });
        });
        if resp.should_close() && decision.is_none() {
            decision = Some(false);
        }
        match decision {
            Some(true) => {
                self.confirm_reset = false;
                self.worker.send(Cmd::ResetAlarm);
            }
            Some(false) => self.confirm_reset = false,
            None => {}
        }
    }
}

/// The text with its first letter raised, for log lines built from lower-case notes.
pub fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}
