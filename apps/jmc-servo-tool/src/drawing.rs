//! The colour drawings of the More window, one for each drawing kind.
//!
//! Every colour has one meaning (see [`theme::ink`]), and every drawing names its lines in a
//! legend or a label, so the colour is never the only clue. The curves come from the simple
//! models in `jmc_core::model`: they show the direction and the size of an effect, not exact
//! numbers.

use eframe::egui::{self, Align2, Color32, FontId, Painter, Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, pos2, vec2};
use jmc_core::docs::{self, Drawing, Kind};
use jmc_core::model;
use jmc_core::params::ParamDef;

use crate::theme::{self, ink};

/// Which value of the parameter a curve shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The value read from the drive or the open file.
    Value,
    /// The factory value.
    Default,
    /// The value from "Try a value".
    Trial,
}

/// One value of the parameter to draw, in display units.
#[derive(Debug, Clone)]
pub struct Curve {
    pub role: Role,
    pub v: f64,
    /// A short name: "Drive", "File", "Default" or "Try". "Drive = default" or "File = default"
    /// when the value is the factory value (then there is no separate default curve).
    pub label: String,
}

impl Curve {
    /// The label without " = default", for the labels on a plot.
    fn source(&self) -> &str {
        self.label.split(" = ").next().unwrap_or(&self.label)
    }
}

/// What a drawing needs.
pub struct Input<'a> {
    pub def: &'a ParamDef,
    /// The display value of any parameter: the drive's or the file's value, else the default.
    pub lookup: &'a dyn Fn(&str) -> Option<f64>,
    /// In drawing order: default, value, trial.
    pub curves: Vec<Curve>,
}

impl Input<'_> {
    /// `lookup`, with this parameter set to `v`.
    fn get_with(&self, v: f64) -> impl Fn(&str) -> Option<f64> + '_ {
        move |c: &str| if c == self.def.code { Some(v) } else { (self.lookup)(c) }
    }

    /// The curve that sets the zones: the drive's or the file's value, else the default.
    fn main(&self) -> &Curve {
        let find = |r: Role| self.curves.iter().find(|c| c.role == r);
        find(Role::Value).or_else(|| find(Role::Default)).unwrap_or(&self.curves[0])
    }

    fn max_abs(&self) -> f64 {
        self.curves.iter().map(|c| c.v.abs()).fold(0.0, f64::max)
    }
}

/// A line style.
#[derive(Debug, Clone, Copy)]
struct Pen {
    color: Color32,
    width: f32,
    dash: Option<(f32, f32)>,
}

const COMMAND: Pen = Pen { color: ink::COMMAND, width: 1.6, dash: Some((6.0, 4.0)) };
const SIGNAL: Pen = Pen { color: ink::SIGNAL, width: 1.8, dash: None };

fn pen(role: Role) -> Pen {
    match role {
        Role::Value => Pen { color: ink::VALUE, width: 2.5, dash: None },
        Role::Default => Pen { color: ink::DEFAULT, width: 1.6, dash: Some((3.0, 3.0)) },
        Role::Trial => Pen { color: ink::TRIAL, width: 2.0, dash: Some((10.0, 4.0)) },
    }
}

impl Pen {
    fn stroke(self) -> Stroke {
        Stroke::new(self.width, self.color)
    }

    fn path(self, p: &Painter, pts: Vec<Pos2>) {
        if pts.len() < 2 {
            return;
        }
        match self.dash {
            Some((dash, gap)) => p.extend(Shape::dashed_line(&pts, self.stroke(), dash, gap)),
            None => {
                p.add(Shape::line(pts, self.stroke()));
            }
        }
    }

    fn hline(self, p: &Painter, x: (f32, f32), y: f32) {
        self.path(p, vec![pos2(x.0, y), pos2(x.1, y)]);
    }
}

fn small() -> FontId {
    FontId::proportional(11.0)
}

/// Draw the drawing `d` for `input`.
pub fn show(ui: &mut egui::Ui, d: &Drawing, input: &Input) {
    if input.curves.is_empty() {
        note(ui, "No value to draw.");
        return;
    }
    match d.kind {
        Kind::Gauge => gauge(ui, input),
        Kind::Choices => choices(ui, input),
        Kind::PositionLoop => position_loop(ui, input),
        Kind::SpeedLoop => speed_loop(ui, input),
        Kind::Lowpass => lowpass(ui, input),
        Kind::Notch => notch(ui, input),
        Kind::Ramp => ramp(ui, input, d.has("down")),
        Kind::Limit => limit(ui, input),
        Kind::AlarmLevel => alarm_level(ui, input),
        Kind::Band => band(ui, input, d.has("outside")),
        Kind::Gear => gear(ui, input),
    }
}

/// The "Try a value" control, in display units. Drawing only: nothing goes to the drive.
pub fn trial_control(ui: &mut egui::Ui, def: &ParamDef, trial: &mut Option<f64>, start: f64) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Try a value").color(ink::TRIAL).strong());
        let s = scale(def);
        if def.choices.is_empty() {
            let (lo, hi) = range(def);
            let mut v = trial.unwrap_or(start).clamp(lo, hi);
            let step = 1.0 / s;
            let log = lo >= 0.0 && (hi - lo) / step > 2000.0;
            let suffix = if def.unit.is_empty() { String::new() } else { format!(" {}", def.unit) };
            ui.spacing_mut().slider_width = 280.0;
            let slider = egui::Slider::new(&mut v, lo..=hi)
                .logarithmic(log)
                .smallest_positive(step)
                .step_by(step)
                .fixed_decimals(usize::from(def.decimals))
                .suffix(suffix);
            if ui.add(slider).changed() {
                *trial = Some(v);
            }
        } else {
            let text = match *trial {
                Some(v) => choice_text(def, (v * s).round() as i64),
                None => "Choose…".to_string(),
            };
            egui::ComboBox::from_id_salt(("trial", def.code.as_str())).selected_text(text).show_ui(ui, |ui| {
                for c in &def.choices {
                    let v = c.value as f64 / s;
                    if ui.selectable_label(*trial == Some(v), choice_text(def, c.value)).clicked() {
                        *trial = Some(v);
                    }
                }
            });
        }
        if trial.is_some() && ui.button("Clear").clicked() {
            *trial = None;
        }
    });
    ui.label(RichText::new("Drawing only. Nothing goes to the drive.").small().color(theme::TEXT_DIM));
}

fn choice_text(def: &ParamDef, v: i64) -> String {
    match def.choices.iter().find(|c| c.value == v) {
        Some(c) => format!("{}  {}", def.format_value(v), c.label),
        None => def.format_value(v),
    }
}

// ---------------------------------------------------------------------------------------------
// Units, ranges and numbers.

fn scale(def: &ParamDef) -> f64 {
    10f64.powi(i32::from(def.decimals))
}

/// The allowed range in display units.
fn range(def: &ParamDef) -> (f64, f64) {
    (def.min as f64 / scale(def), def.max as f64 / scale(def))
}

/// A display value with the parameter's decimals and unit.
fn with_unit(def: &ParamDef, v: f64) -> String {
    let n = format!("{v:.*}", usize::from(def.decimals));
    if def.unit.is_empty() { n } else { format!("{n} {}", def.unit) }
}

/// A number with up to 3 decimals and no trailing zeros.
fn short(v: f64) -> String {
    trim(format!("{v:.3}"))
}

/// Like [`short`], but a small number keeps 3 significant digits: 0.000336, not 0.
fn sig(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return short(v);
    }
    let mag = v.abs().log10().floor() as i32;
    let decimals = (2 - mag).clamp(3, 9) as usize;
    trim(format!("{v:.decimals$}"))
}

/// Takes the trailing zeros (and a trailing point) off a number.
fn trim(s: String) -> String {
    let t = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.') } else { &s };
    if t == "-0" { "0".to_string() } else { t.to_string() }
}

fn hz_text(v: f64) -> String {
    if v >= 1000.0 { format!("{}k", short(v / 1000.0)) } else { short(v) }
}

fn lin(a: f64, b: f64, n: usize) -> Vec<f64> {
    let n = n.max(2);
    (0..n).map(|i| a + (b - a) * i as f64 / (n - 1) as f64).collect()
}

fn logspace(a: f64, b: f64, n: usize) -> Vec<f64> {
    lin(a.ln(), b.ln(), n).into_iter().map(f64::exp).collect()
}

fn db(gain: f64) -> f64 {
    20.0 * gain.max(1e-3).log10()
}

// ---------------------------------------------------------------------------------------------
// Plots: axes, grid, legend.

struct Axes {
    rect: Rect,
    x: (f64, f64),
    y: (f64, f64),
    log_x: bool,
}

impl Axes {
    fn fx(&self, x: f64) -> f32 {
        let f = if self.log_x {
            (x.max(1e-9).ln() - self.x.0.ln()) / (self.x.1.ln() - self.x.0.ln())
        } else {
            (x - self.x.0) / (self.x.1 - self.x.0)
        };
        self.rect.left() + f as f32 * self.rect.width()
    }

    fn fy(&self, y: f64) -> f32 {
        let f = (y - self.y.0) / (self.y.1 - self.y.0);
        self.rect.bottom() - f as f32 * self.rect.height()
    }

    fn px(&self, x: f64, y: f64) -> Pos2 {
        pos2(self.fx(x), self.fy(y))
    }

    fn points(&self, xs: &[f64], ys: &[f64]) -> Vec<Pos2> {
        xs.iter().zip(ys).map(|(&x, &y)| self.px(x, y)).collect()
    }

    fn xr(&self) -> (f32, f32) {
        (self.rect.left(), self.rect.right())
    }
}

struct Plot<'a> {
    x: (f64, f64),
    y: (f64, f64),
    log_x: bool,
    x_label: &'a str,
    y_label: &'a str,
    height: f32,
}

/// Space left of and below the plot area, for the tick labels.
const LEFT: f32 = 50.0;
const RIGHT: f32 = 10.0;

fn plot(ui: &mut egui::Ui, spec: Plot, draw: impl FnOnce(&Painter, &Axes)) {
    let width = ui.available_width().max(240.0);
    let (resp, painter) = ui.allocate_painter(vec2(width, spec.height), Sense::hover());
    let outer = resp.rect;
    painter.rect_filled(outer, 6.0, theme::CANVAS);
    let rect = Rect::from_min_max(pos2(outer.left() + LEFT, outer.top() + 18.0), pos2(outer.right() - RIGHT, outer.bottom() - 34.0));
    let fix = |(a, b): (f64, f64)| if b > a { (a, b) } else { (a, a + 1.0) };
    let mut x = fix(spec.x);
    if spec.log_x {
        x.0 = x.0.max(1e-6);
        x.1 = x.1.max(x.0 * 10.0);
    }
    let ax = Axes { rect, x, y: fix(spec.y), log_x: spec.log_x };
    grid(&painter, &ax);
    painter.text(pos2(rect.right(), outer.bottom() - 4.0), Align2::RIGHT_BOTTOM, spec.x_label, small(), theme::TEXT_DIM);
    painter.text(pos2(rect.left() + 4.0, outer.top() + 3.0), Align2::LEFT_TOP, spec.y_label, small(), theme::TEXT_DIM);
    draw(&painter.with_clip_rect(rect.expand(1.0)), &ax);
}

fn nice_step(span: f64, target: f64) -> f64 {
    let raw = (span / target).max(1e-12);
    let mag = 10f64.powf(raw.log10().floor());
    let n = raw / mag;
    let m = if n <= 1.0 {
        1.0
    } else if n <= 2.0 {
        2.0
    } else if n <= 2.5 {
        2.5
    } else if n <= 5.0 {
        5.0
    } else {
        10.0
    };
    m * mag
}

fn ticks(r: (f64, f64)) -> Vec<f64> {
    let step = nice_step(r.1 - r.0, 5.0);
    let mut out = Vec::new();
    let mut v = (r.0 / step).ceil() * step;
    while v <= r.1 + step * 1e-6 && out.len() < 50 {
        out.push(if v.abs() < step * 1e-9 { 0.0 } else { v });
        v += step;
    }
    out
}

fn grid(p: &Painter, ax: &Axes) {
    let r = ax.rect;
    let major = Stroke::new(1.0, ink::GRID);
    let minor = Stroke::new(0.5, ink::GRID);
    if ax.log_x {
        let (lo, hi) = (ax.x.0.log10().floor() as i32, ax.x.1.log10().ceil() as i32);
        for k in lo..=hi {
            for (i, m) in [1.0, 2.0, 5.0].into_iter().enumerate() {
                let v = 10f64.powi(k) * m;
                if v < ax.x.0 * 0.999 || v > ax.x.1 * 1.001 {
                    continue;
                }
                let x = ax.fx(v);
                p.vline(x, r.y_range(), if i == 0 { major } else { minor });
                if i == 0 {
                    p.text(pos2(x, r.bottom() + 4.0), Align2::CENTER_TOP, hz_text(v), small(), theme::TEXT_DIM);
                }
            }
        }
    } else {
        for v in ticks(ax.x) {
            let x = ax.fx(v);
            p.vline(x, r.y_range(), major);
            p.text(pos2(x, r.bottom() + 4.0), Align2::CENTER_TOP, short(v), small(), theme::TEXT_DIM);
        }
    }
    for v in ticks(ax.y) {
        let y = ax.fy(v);
        p.hline(r.x_range(), y, if v == 0.0 { Stroke::new(1.0, theme::TEXT_DIM) } else { major });
        p.text(pos2(r.left() - 5.0, y), Align2::RIGHT_CENTER, short(v), small(), theme::TEXT_DIM);
    }
    p.rect_stroke(r, 0.0, major, StrokeKind::Inside);
}

/// One entry of a legend.
enum Key {
    Line(Pen),
    /// A filled zone: fill colour and text colour.
    Zone(Color32, Color32),
}

/// The legend. Each entry (swatch and text) is one piece, so a line never breaks inside it.
fn legend(ui: &mut egui::Ui, items: &[(Key, String)]) {
    let font = egui::TextStyle::Small.resolve(ui.style());
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(16.0, 4.0);
        for (key, text) in items {
            let color = match key {
                Key::Line(pen) => pen.color,
                Key::Zone(_, text_color) => *text_color,
            };
            let galley = ui.painter().layout_no_wrap(text.clone(), font.clone(), color);
            let size = vec2(26.0 + 6.0 + galley.size().x, galley.size().y.max(12.0));
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            let swatch = Rect::from_min_size(pos2(rect.left(), rect.center().y - 6.0), vec2(26.0, 12.0));
            match key {
                Key::Line(pen) => {
                    let y = swatch.center().y;
                    pen.path(ui.painter(), vec![pos2(swatch.left(), y), pos2(swatch.right(), y)]);
                }
                Key::Zone(fill, text_color) => {
                    ui.painter().rect_filled(swatch, 2.0, *fill);
                    ui.painter().rect_stroke(swatch, 2.0, Stroke::new(1.0, *text_color), StrokeKind::Inside);
                }
            }
            let at = pos2(swatch.right() + 6.0, rect.center().y - galley.size().y / 2.0);
            ui.painter().galley(at, galley, color);
        }
    });
}

/// True when the oscillation grows: the deviation at the end is larger than at the start.
/// With such a value the loop is unstable (in the model).
fn unstable(deviation: &[f64]) -> bool {
    let n = deviation.len();
    if n < 8 || deviation.iter().any(|v| !v.is_finite()) {
        return n >= 8;
    }
    let peak = |s: &[f64]| s.iter().fold(0.0f64, |a, &b| a.max(b.abs()));
    peak(&deviation[n * 85 / 100..]) > peak(&deviation[..n / 2]).max(1e-9)
}

/// The values of an unstable run, cut at the first number that is not finite and kept to a few
/// spans of the axis, so that the line goes off the plot without taking over the scale.
fn tame(ys: &[f64], y: (f64, f64)) -> Vec<f64> {
    let span = (y.1 - y.0).abs().max(1e-9);
    ys.iter().take_while(|v| v.is_finite()).map(|v| v.clamp(y.0 - 5.0 * span, y.1 + 5.0 * span)).collect()
}

const UNSTABLE_NOTE: &str =
    "A curve that grows without end is unstable. On the machine this is vibration and noise, and the drive can stop with an alarm.";

/// A label on a plot, on a patch of the plot colour, so that the lines under it do not hide it.
fn tag(p: &Painter, at: Pos2, align: Align2, text: impl ToString, color: Color32) {
    let galley = p.layout_no_wrap(text.to_string(), small(), color);
    let rect = align.anchor_size(at, galley.size());
    p.rect_filled(rect.expand2(vec2(3.0, 1.0)), 3.0, theme::CANVAS.gamma_multiply(0.85));
    p.galley(rect.min, galley, color);
}

fn note(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().italics().color(theme::TEXT_DIM));
}

fn curve_keys(input: &Input, text: impl Fn(&Curve) -> String) -> Vec<(Key, String)> {
    input.curves.iter().map(|c| (Key::Line(pen(c.role)), text(c))).collect()
}

// ---------------------------------------------------------------------------------------------
// The drawings.

/// The value on its range, with a marker for each curve.
fn gauge(ui: &mut egui::Ui, input: &Input) {
    let def = input.def;
    let (lo, hi) = range(def);
    let n = input.curves.len();
    let height = 34.0 + 18.0 * n as f32 + 26.0;
    let (resp, p) = ui.allocate_painter(vec2(ui.available_width().max(240.0), height), Sense::hover());
    let r = resp.rect;
    p.rect_filled(r, 6.0, theme::CANVAS);
    let track = r.bottom() - 30.0;
    let (x0, x1) = (r.left() + 24.0, r.right() - 24.0);
    let fx = |v: f64| x0 + ((v - lo) / (hi - lo).max(1e-12)).clamp(0.0, 1.0) as f32 * (x1 - x0);
    p.rect_filled(Rect::from_min_max(pos2(x0, track - 3.0), pos2(x1, track + 3.0)), 3.0, ink::GRID);
    if lo < 0.0 && hi > 0.0 {
        let x = fx(0.0);
        p.vline(x, (track - 7.0)..=(track + 7.0), Stroke::new(1.0, theme::TEXT_DIM));
        p.text(pos2(x, track + 8.0), Align2::CENTER_TOP, "0", small(), theme::TEXT_DIM);
    }
    p.text(pos2(x0, track + 8.0), Align2::LEFT_TOP, with_unit(def, lo), small(), theme::TEXT_DIM);
    p.text(pos2(x1, track + 8.0), Align2::RIGHT_TOP, with_unit(def, hi), small(), theme::TEXT_DIM);
    for (i, c) in input.curves.iter().enumerate() {
        let color = pen(c.role).color;
        let x = fx(c.v);
        let y = r.top() + 12.0 + 18.0 * i as f32;
        p.line_segment([pos2(x, y), pos2(x, track - 10.0)], Stroke::new(1.0, color));
        p.add(Shape::convex_polygon(vec![pos2(x - 6.0, track - 12.0), pos2(x + 6.0, track - 12.0), pos2(x, track - 3.0)], color, Stroke::NONE));
        let text = format!("{}: {}", c.label, with_unit(def, c.v));
        if x < r.center().x {
            p.text(pos2(x + 6.0, y), Align2::LEFT_CENTER, text, small(), color);
        } else {
            p.text(pos2(x - 6.0, y), Align2::RIGHT_CENTER, text, small(), color);
        }
    }
}

/// The list of choices, with a chip on the row of each curve's value.
fn choices(ui: &mut egui::Ui, input: &Input) {
    let def = input.def;
    let s = scale(def);
    let list: Vec<(i64, String)> = if def.choices.is_empty() {
        vec![(0, String::new()), (1, String::new())]
    } else {
        def.choices.iter().map(|c| (c.value, c.label.clone())).collect()
    };
    let raw = |c: &Curve| (c.v * s).round() as i64;
    egui::Frame::new().fill(theme::CANVAS).corner_radius(6).inner_margin(8).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::Grid::new(("choices", def.code.as_str())).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
            for (value, label) in &list {
                let hits: Vec<&Curve> = input.curves.iter().filter(|c| raw(c) == *value).collect();
                let color = if hits.is_empty() { theme::TEXT_DIM } else { theme::TEXT };
                ui.label(RichText::new(def.format_value(*value)).monospace().color(color));
                ui.label(RichText::new(label).color(color));
                ui.horizontal(|ui| {
                    for c in hits {
                        theme::chip(ui, &c.label, pen(c.role).color);
                    }
                });
                ui.end_row();
            }
        });
    });
    for c in &input.curves {
        if !list.iter().any(|(v, _)| *v == raw(c)) {
            note(ui, &format!("{}: {} is not on the list.", c.label, def.format_value(raw(c))));
        }
    }
}

/// A move: speed and following error, with the position loop gain and the speed feedforward.
fn position_loop(ui: &mut egui::Ui, input: &Input) {
    let def = input.def;
    let set = if def.code == "P02-01" { 2 } else { 1 };
    let cmd = model::Command::Move { speed: 1000.0, ramp_ms: 60.0, hold_ms: 120.0 };
    let t_end = 320.0;
    // (pen, curve, trace, unstable)
    let runs: Vec<(Pen, &Curve, model::Trace, bool)> = input
        .curves
        .iter()
        .map(|c| {
            let tr = model::simulate(&model::Loop::from_values(set, input.get_with(c.v)), cmd, t_end, 321);
            let bad = unstable(&tr.error);
            (pen(c.role), c, tr, bad)
        })
        .collect();
    let first = &runs[0].2;
    // An unstable run does not set the scale: it would make the others flat lines.
    let stable = || runs.iter().filter(|r| !r.3);
    let top = stable().flat_map(|r| r.2.speed.iter().copied()).fold(1000.0, f64::max);
    let bottom = stable().flat_map(|r| r.2.speed.iter().copied()).fold(0.0, f64::min);
    let y = (bottom * 1.1 - 50.0, top * 1.1);
    plot(ui, Plot { x: (0.0, t_end), y, log_x: false, x_label: "time (ms)", y_label: "speed (rpm)", height: 170.0 }, |p, ax| {
        COMMAND.path(p, ax.points(&first.t_ms, &first.speed_cmd));
        for (pen, _, tr, bad) in &runs {
            let ys = if *bad { tame(&tr.speed, y) } else { tr.speed.clone() };
            pen.path(p, ax.points(&tr.t_ms, &ys));
        }
    });
    let peak = |tr: &model::Trace| tr.error.iter().fold(0.0f64, |a, &b| if b.abs() > a.abs() { b } else { a });
    let emax = stable().map(|r| peak(&r.2).abs()).fold(1e-4, f64::max);
    let y = (-emax * 1.15, emax * 1.15);
    plot(
        ui,
        Plot { x: (0.0, t_end), y, log_x: false, x_label: "time (ms)", y_label: "following error (motor turns)", height: 150.0 },
        |p, ax| {
            for (pen, _, tr, bad) in &runs {
                let ys = if *bad { tame(&tr.error, y) } else { tr.error.clone() };
                pen.path(p, ax.points(&tr.t_ms, &ys));
            }
        },
    );
    let mut keys = vec![(Key::Line(COMMAND), "Command: a move to 1000 rpm".to_string())];
    keys.extend(runs.iter().map(|(pen, c, tr, bad)| {
        let text = if *bad {
            format!("{}: {} → unstable: the oscillation grows", c.label, with_unit(def, c.v))
        } else {
            let e = peak(tr).abs();
            format!("{}: {} → peak error {} turn ({}°)", c.label, with_unit(def, c.v), sig(e), short((e * 360.0).round()))
        };
        (Key::Line(*pen), text)
    }));
    legend(ui, &keys);
    if runs.iter().any(|r| r.3) {
        note(ui, UNSTABLE_NOTE);
    }
    note(ui, "Model: a rigid load with the inertia ratio set right. It shows the effect, not exact numbers.");
}

/// A step of the speed command: the speed loop's response.
fn speed_loop(ui: &mut egui::Ui, input: &Input) {
    let def = input.def;
    let code = def.code.as_str();
    let set = if matches!(code, "P02-13" | "P02-14" | "P02-15") { 2 } else { 1 };
    let inertia = code == "P01-04";
    let real = (input.lookup)("P01-04").unwrap_or(1.0);
    let loops: Vec<model::Loop> = input
        .curves
        .iter()
        .map(|c| {
            let mut l = model::Loop::from_values(set, input.get_with(c.v));
            if inertia {
                l.gain_scale = model::inertia_scale(c.v, real);
            }
            l
        })
        .collect();
    let wc = loops.iter().map(|l| 2.0 * std::f64::consts::PI * l.kv_hz.max(0.1) * l.gain_scale.max(0.01)).fold(f64::INFINITY, f64::min);
    let ti = loops.iter().map(|l| l.ti_ms).filter(|&t| t > 0.0 && t < 1000.0).fold(0.0, f64::max);
    let t_end = (8.0 / wc * 1000.0).max(4.0 * ti).clamp(10.0, 400.0);
    let target = 100.0;
    let runs: Vec<model::Trace> = loops.iter().map(|l| model::simulate(l, model::Command::SpeedStep { speed: target }, t_end, 401)).collect();
    // With a speed step, the error is the speed command less the speed.
    let bad: Vec<bool> = runs.iter().map(|r| unstable(&r.error)).collect();
    let top = runs.iter().zip(&bad).filter(|(_, b)| !**b).flat_map(|(r, _)| r.speed.iter().copied()).fold(target, f64::max);
    let lead = -0.05 * t_end;
    let y = (-5.0, top * 1.15);
    plot(ui, Plot { x: (lead, t_end), y, log_x: false, x_label: "time (ms)", y_label: "speed (rpm)", height: 190.0 }, |p, ax| {
        COMMAND.path(p, vec![ax.px(lead, 0.0), ax.px(0.0, 0.0), ax.px(0.0, target), ax.px(t_end, target)]);
        for ((c, tr), b) in input.curves.iter().zip(&runs).zip(&bad) {
            let ys = if *b { tame(&tr.speed, y) } else { tr.speed.clone() };
            pen(c.role).path(p, ax.points(&tr.t_ms, &ys));
        }
    });
    let mut keys = vec![(Key::Line(COMMAND), format!("Command: a step to {target:.0} rpm"))];
    for ((c, tr), b) in input.curves.iter().zip(&runs).zip(&bad) {
        let text = if *b {
            format!("{}: {} → unstable: the oscillation grows", c.label, with_unit(def, c.v))
        } else {
            let peak = tr.speed.iter().copied().fold(0.0, f64::max);
            let over = ((peak - target) / target * 100.0).max(0.0);
            format!("{}: {} → overshoot {over:.0} %", c.label, with_unit(def, c.v))
        };
        keys.push((Key::Line(pen(c.role)), text));
    }
    legend(ui, &keys);
    if bad.iter().any(|b| *b) {
        note(ui, UNSTABLE_NOTE);
    }
    if inertia {
        note(ui, &format!("The model takes the real load ratio as {} (the drive's value).", with_unit(def, real)));
        note(ui, "A set value below the real load gives a lower real gain: the response is slower.");
    }
    note(ui, "Model: a rigid load, the position loop open. It shows the effect, not exact numbers.");
}

/// A first-order filter: step response and gain against frequency.
fn lowpass(ui: &mut egui::Ui, input: &Input) {
    let def = input.def;
    let tmax = input.max_abs();
    let span = if tmax > 0.0 { tmax * 6.0 } else { 1.0 };
    let t0 = 0.1 * span;
    let t_end = t0 + span;
    let ts = lin(0.0, t_end, 300);
    plot(
        ui,
        Plot { x: (0.0, t_end), y: (-0.05, 1.15), log_x: false, x_label: "time (ms)", y_label: "output ÷ input", height: 160.0 },
        |p, ax| {
            COMMAND.path(p, vec![ax.px(0.0, 0.0), ax.px(t0, 0.0), ax.px(t0, 1.0), ax.px(t_end, 1.0)]);
            for c in &input.curves {
                let ys: Vec<f64> = ts.iter().map(|&t| model::lowpass_step(t - t0, c.v)).collect();
                pen(c.role).path(p, ax.points(&ts, &ys));
                if c.v > 0.0 {
                    p.circle_filled(ax.px(t0 + c.v, 1.0 - (-1.0f64).exp()), 3.5, pen(c.role).color);
                }
            }
        },
    );
    let fcs: Vec<f64> = input.curves.iter().filter_map(|c| model::corner_hz(c.v)).collect();
    let (mut lo, mut hi) = (10.0, 10_000.0);
    if !fcs.is_empty() {
        let (a, b) = (fcs.iter().copied().fold(f64::INFINITY, f64::min), fcs.iter().copied().fold(0.0, f64::max));
        lo = 10f64.powf((a / 10.0).log10().floor()).clamp(0.1, 100.0);
        hi = 10f64.powf((b * 10.0).log10().ceil()).clamp(1000.0, 100_000.0);
    }
    let fs = logspace(lo, hi, 300);
    plot(
        ui,
        Plot { x: (lo, hi), y: (-40.0, 3.0), log_x: true, x_label: "frequency (Hz)", y_label: "gain (dB)", height: 170.0 },
        |p, ax| {
            for (i, c) in input.curves.iter().enumerate() {
                let pn = pen(c.role);
                let ys: Vec<f64> = fs.iter().map(|&f| db(model::lowpass_gain(f, c.v))).collect();
                pn.path(p, ax.points(&fs, &ys));
                if let Some(fc) = model::corner_hz(c.v) {
                    let at = ax.px(fc, -3.0);
                    p.vline(at.x, at.y..=ax.rect.bottom(), Stroke::new(1.0, pn.color.gamma_multiply(0.6)));
                    p.circle_filled(at, 3.5, pn.color);
                    let y = ax.rect.bottom() - 4.0 - 16.0 * i as f32;
                    tag(p, pos2(at.x + 5.0, y), Align2::LEFT_BOTTOM, format!("fc {} Hz", short(fc.round())), pn.color);
                }
            }
        },
    );
    let mut keys = vec![(Key::Line(COMMAND), "Command: a step".to_string())];
    keys.extend(curve_keys(input, |c| match model::corner_hz(c.v) {
        Some(fc) => format!("{}: {} → fc {} Hz", c.label, with_unit(def, c.v), short(fc.round())),
        None => format!("{}: 0 = no filter", c.label),
    }));
    legend(ui, &keys);
    note(ui, "Dot on the step: 63 % of the step after one filter time.");
    note(ui, "fc = 1 ÷ (2π × filter time). Above fc, the gain falls by 20 dB for each 10 × the frequency.");
}

/// A notch filter: gain against frequency.
fn notch(ui: &mut egui::Ui, input: &Input) {
    let Some((n, _)) = docs::notch_role(&input.def.code) else {
        gauge(ui, input);
        return;
    };
    let codes = docs::notch_codes(n);
    let sets: Vec<(f64, f64, f64)> = input
        .curves
        .iter()
        .map(|c| {
            let g = input.get_with(c.v);
            (g(&codes[0]).unwrap_or(model::NOTCH_OFF_HZ), g(&codes[1]).unwrap_or(2.0), g(&codes[2]).unwrap_or(0.0))
        })
        .collect();
    let (lo, hi) = (20.0, 10_000.0);
    let mut fs = logspace(lo, hi, 400);
    fs.extend(sets.iter().map(|s| s.0).filter(|&f| f > lo && f < hi));
    fs.sort_by(f64::total_cmp);
    plot(
        ui,
        Plot { x: (lo, hi), y: (-40.0, 3.0), log_x: true, x_label: "frequency (Hz)", y_label: "gain (dB)", height: 200.0 },
        |p, ax| {
            let off = ax.fx(model::NOTCH_OFF_HZ);
            p.vline(off, ax.rect.y_range(), Stroke::new(1.0, theme::TEXT_DIM));
            tag(p, pos2(off - 5.0, ax.fy(-20.0)), Align2::RIGHT_CENTER, "5000 Hz = off", theme::TEXT_DIM);
            for (i, (c, &(f0, w, d))) in input.curves.iter().zip(&sets).enumerate() {
                let pn = pen(c.role);
                let ys: Vec<f64> = fs.iter().map(|&f| db(model::notch_gain(f, f0, w, d))).collect();
                pn.path(p, ax.points(&fs, &ys));
                if f0 < model::NOTCH_OFF_HZ && f0 > 0.0 {
                    let x = ax.fx(f0);
                    let y = ax.rect.bottom() - 4.0 - 16.0 * i as f32;
                    tag(p, pos2(x + 5.0, y), Align2::LEFT_BOTTOM, format!("{} Hz", short(f0)), pn.color);
                }
            }
        },
    );
    let keys = curve_keys(input, |c| {
        let (f0, w, d) = sets[input.curves.iter().position(|x| x.role == c.role).unwrap_or(0)];
        if f0 >= model::NOTCH_OFF_HZ {
            format!("{}: off (frequency {} Hz)", c.label, short(f0))
        } else {
            format!("{}: {} Hz, width {}, depth {} %", c.label, short(f0), short(w), short(d))
        }
    });
    legend(ui, &keys);
    note(ui, &format!("Notch {n}: frequency {}, width {}, depth {}.", codes[0], codes[1], codes[2]));
    let adaptive = (input.lookup)("P08-11").unwrap_or(0.0).round() as i64;
    if (n == 3 && matches!(adaptive, 1 | 2)) || (n == 4 && adaptive == 2) {
        note(ui, &format!("P08-11 is {adaptive}: the drive sets this notch itself."));
    }
    note(ui, "Depth = output ÷ input at the center, in %: 0 cuts fully. The width scale is a model: the manual gives no scale.");
}

/// A speed ramp, up or down.
fn ramp(ui: &mut egui::Ui, input: &Input, down: bool) {
    let def = input.def;
    let target = 1000.0;
    let span = input.max_abs().max(5.0);
    let t0 = 0.15 * span;
    let t_end = t0 + span * 1.25;
    let ts = lin(0.0, t_end, 300);
    let shape = |r: f64| if down { target - r } else { r };
    plot(
        ui,
        Plot { x: (0.0, t_end), y: (-50.0, target * 1.12), log_x: false, x_label: "time (ms)", y_label: "speed (rpm)", height: 180.0 },
        |p, ax| {
            let (a, b) = (shape(0.0), shape(target));
            COMMAND.path(p, vec![ax.px(0.0, a), ax.px(t0, a), ax.px(t0, b), ax.px(t_end, b)]);
            for c in &input.curves {
                let ys: Vec<f64> = ts.iter().map(|&t| shape(model::ramp(t - t0, target, c.v))).collect();
                pen(c.role).path(p, ax.points(&ts, &ys));
            }
        },
    );
    let mut keys = vec![(Key::Line(COMMAND), "Speed command: a step".to_string())];
    keys.extend(curve_keys(input, |c| {
        if c.v <= 0.0 {
            format!("{}: 0 = no ramp", c.label)
        } else if def.unit.contains("rpm") {
            format!("{}: {}", c.label, with_unit(def, c.v))
        } else {
            format!("{}: {} per 1000 rpm", c.label, with_unit(def, c.v))
        }
    }));
    legend(ui, &keys);
    note(ui, "Speed mode only. With 0, the speed follows the command step.");
}

/// A demand that goes past a limit, and the output the drive lets through.
fn limit(ui: &mut egui::Ui, input: &Input) {
    let def = input.def;
    let sign = if def.max <= 0 && def.min < 0 { -1.0 } else { 1.0 };
    let main = input.main();
    let mut refv = input.max_abs();
    if refv <= 0.0 {
        refv = (range(def).1.abs().max(range(def).0.abs()) / 2.0).max(1.0);
    }
    let peak = refv * 1.5;
    let shape = [(0.0, 0.0), (0.12, 0.05), (0.3, 0.55), (0.45, 1.0), (0.6, 0.92), (0.78, 0.35), (1.0, 0.0)];
    let demand: Vec<(f64, f64)> = model::catmull_rom(&shape, 16).into_iter().map(|(t, f)| (t * 100.0, sign * f * peak)).collect();
    let ts: Vec<f64> = demand.iter().map(|d| d.0).collect();
    let ds: Vec<f64> = demand.iter().map(|d| d.1).collect();
    let y = if sign < 0.0 { (-peak * 1.12, peak * 0.1) } else { (-peak * 0.1, peak * 1.12) };
    let y_label = if def.unit.is_empty() { "value".to_string() } else { def.unit.clone() };
    plot(ui, Plot { x: (0.0, 100.0), y, log_x: false, x_label: "time", y_label: &y_label, height: 190.0 }, |p, ax| {
        let edge = ax.fy(main.v);
        let zone = if sign < 0.0 {
            Rect::from_x_y_ranges(ax.rect.x_range(), edge..=ax.rect.bottom())
        } else {
            Rect::from_x_y_ranges(ax.rect.x_range(), ax.rect.top()..=edge)
        };
        p.rect_filled(zone, 0.0, ink::ALARM_FILL);
        let at = if sign < 0.0 { pos2(zone.right() - 6.0, zone.bottom() - 4.0) } else { pos2(zone.right() - 6.0, zone.top() + 4.0) };
        let anchor = if sign < 0.0 { Align2::RIGHT_BOTTOM } else { Align2::RIGHT_TOP };
        p.text(at, anchor, "Cut off", small(), ink::ALARM);
        COMMAND.path(p, ax.points(&ts, &ds));
        for c in &input.curves {
            let lim = c.v.abs();
            let out: Vec<f64> = ds.iter().map(|d| sign * d.abs().min(lim)).collect();
            pen(c.role).path(p, ax.points(&ts, &out));
        }
    });
    let mut keys = vec![
        (Key::Line(COMMAND), "Demand: what the drive is asked for".to_string()),
        (Key::Zone(ink::ALARM_FILL, ink::ALARM), format!("Cut off at the {} value", main.source().to_lowercase())),
    ];
    keys.extend(curve_keys(input, |c| format!("{}: limit {}", c.label, with_unit(def, c.v))));
    legend(ui, &keys);
    note(ui, "The demand is an example. The output follows it up to the limit.");
}

/// A signal that grows past an alarm level.
fn alarm_level(ui: &mut egui::Ui, input: &Input) {
    let def = input.def;
    let main = input.main();
    let refv = input.max_abs().max(1.0);
    let shape = [(0.0, 0.02), (0.2, 0.08), (0.4, 0.25), (0.6, 0.6), (0.8, 1.05), (1.0, 1.5)];
    let sig: Vec<(f64, f64)> = model::catmull_rom(&shape, 16).into_iter().map(|(t, f)| (t * 100.0, f * refv)).collect();
    let ts: Vec<f64> = sig.iter().map(|s| s.0).collect();
    let ss: Vec<f64> = sig.iter().map(|s| s.1).collect();
    let y_label = if def.unit.is_empty() { "value".to_string() } else { def.unit.clone() };
    plot(ui, Plot { x: (0.0, 100.0), y: (0.0, refv * 1.6), log_x: false, x_label: "time", y_label: &y_label, height: 190.0 }, |p, ax| {
        let edge = ax.fy(main.v);
        p.rect_filled(Rect::from_x_y_ranges(ax.rect.x_range(), ax.rect.top()..=edge), 0.0, ink::ALARM_FILL);
        p.text(pos2(ax.rect.left() + 6.0, ax.rect.top() + 4.0), Align2::LEFT_TOP, "Alarm zone", small(), ink::ALARM);
        SIGNAL.path(p, ax.points(&ts, &ss));
        for c in &input.curves {
            let pn = pen(c.role);
            let y = ax.fy(c.v);
            pn.hline(p, ax.xr(), y);
            tag(p, pos2(ax.rect.right() - 6.0, y - 3.0), Align2::RIGHT_BOTTOM, format!("{} {}", c.source(), short(c.v)), pn.color);
            if let Some(i) = ss.iter().position(|&s| s >= c.v) {
                let at = ax.px(ts[i], ss[i]);
                p.circle_filled(at, 4.0, pn.color);
                if c.role == main.role {
                    tag(p, at + vec2(-9.0, 0.0), Align2::RIGHT_CENTER, "Alarm", ink::ALARM);
                }
            }
        }
    });
    let mut keys = vec![
        (Key::Line(SIGNAL), "Signal the drive watches (example)".to_string()),
        (Key::Zone(ink::ALARM_FILL, ink::ALARM), format!("Alarm zone with the {} value", main.source().to_lowercase())),
    ];
    keys.extend(curve_keys(input, |c| format!("{}: alarm at {}", c.label, with_unit(def, c.v))));
    legend(ui, &keys);
    note(ui, "The dot is where the signal reaches the level and the drive gives the alarm.");
}

/// An output that is on while a signal is inside (or outside) a band around zero.
fn band(ui: &mut egui::Ui, input: &Input, outside: bool) {
    let def = input.def;
    let main = input.main();
    let refb = input.max_abs().max(1e-3);
    let ts = lin(0.0, 100.0, 400);
    let ss: Vec<f64> = if outside {
        ts.iter().map(|&t| 2.5 * refb * (std::f64::consts::PI * t / 100.0).sin().powi(2)).collect()
    } else {
        ts.iter().map(|&t| 3.0 * refb * (-t / 25.0).exp() * (2.0 * std::f64::consts::PI * t / 22.0).cos()).collect()
    };
    let y = if outside { (-0.6 * refb, 2.8 * refb) } else { (-3.3 * refb, 3.3 * refb) };
    let y_label = if def.unit.is_empty() { "value".to_string() } else { def.unit.clone() };
    plot(ui, Plot { x: (0.0, 100.0), y, log_x: false, x_label: "time", y_label: &y_label, height: 190.0 }, |p, ax| {
        let b = main.v.abs();
        let (top, bottom, lo, hi) = (ax.rect.top(), ax.rect.bottom(), ax.fy(-b), ax.fy(b));
        let xr = ax.rect.x_range();
        if outside {
            p.rect_filled(Rect::from_x_y_ranges(xr, top..=hi), 0.0, ink::GOOD_FILL);
            p.rect_filled(Rect::from_x_y_ranges(xr, lo..=bottom), 0.0, ink::GOOD_FILL);
        } else {
            p.rect_filled(Rect::from_x_y_ranges(xr, hi..=lo), 0.0, ink::GOOD_FILL);
        }
        SIGNAL.path(p, ax.points(&ts, &ss));
        for c in &input.curves {
            let pn = pen(c.role);
            for edge in [c.v.abs(), -c.v.abs()] {
                pn.hline(p, ax.xr(), ax.fy(edge));
            }
            tag(p, pos2(ax.rect.right() - 6.0, ax.fy(c.v.abs()) - 3.0), Align2::RIGHT_BOTTOM, format!("{} ±{}", c.source(), short(c.v.abs())), pn.color);
        }
    });
    // One strip per curve: green while the output is on.
    let row = 16.0;
    let height = row * input.curves.len() as f32 + 8.0;
    let (resp, p) = ui.allocate_painter(vec2(ui.available_width().max(240.0), height), Sense::hover());
    let r = resp.rect;
    let (x0, x1) = (r.left() + LEFT, r.right() - RIGHT);
    for (i, c) in input.curves.iter().enumerate() {
        let y0 = r.top() + 4.0 + row * i as f32;
        let strip = Rect::from_min_max(pos2(x0, y0 + 2.0), pos2(x1, y0 + row - 2.0));
        p.rect_filled(strip, 2.0, theme::CANVAS);
        p.text(pos2(x0 - 5.0, strip.center().y), Align2::RIGHT_CENTER, c.source(), small(), pen(c.role).color);
        let b = c.v.abs();
        let on = |s: f64| if outside { s.abs() > b } else { s.abs() < b };
        let fx = |t: f64| x0 + (t / 100.0) as f32 * (x1 - x0);
        let mut start: Option<f64> = None;
        for (k, (&t, &s)) in ts.iter().zip(&ss).enumerate() {
            let last = k + 1 == ts.len();
            match (on(s), start) {
                (true, None) => start = Some(t),
                (false, Some(t0)) => {
                    p.rect_filled(Rect::from_x_y_ranges(fx(t0)..=fx(t), strip.y_range()), 2.0, ink::GOOD);
                    start = None;
                }
                _ => {}
            }
            if last && let Some(t0) = start {
                p.rect_filled(Rect::from_x_y_ranges(fx(t0)..=fx(t), strip.y_range()), 2.0, ink::GOOD);
            }
        }
    }
    let zone = if outside { "Outside the band: output on" } else { "Inside the band: output on" };
    let mut keys = vec![
        (Key::Line(SIGNAL), "Signal (example)".to_string()),
        (Key::Zone(ink::GOOD_FILL, ink::GOOD), format!("{zone} (the {} value)", main.source().to_lowercase())),
    ];
    keys.extend(curve_keys(input, |c| format!("{}: ±{}", c.label, with_unit(def, c.v.abs()))));
    legend(ui, &keys);
    note(ui, "Green strip: the output is on with that value.");
}

/// Electronic gear and encoder output: what 8 input pulses give.
fn gear(ui: &mut egui::Ui, input: &Input) {
    let code = input.def.code.as_str();
    const PULSES: f64 = 8.0;
    let row = 70.0;
    let height = row * input.curves.len() as f32 + 8.0;
    let (resp, p) = ui.allocate_painter(vec2(ui.available_width().max(320.0), height), Sense::hover());
    let r = resp.rect;
    p.rect_filled(r, 6.0, theme::CANVAS);
    let in_w = 110.0;
    let box_w = 150.0;
    for (i, c) in input.curves.iter().enumerate() {
        let pn = pen(c.role);
        let g = input.get_with(c.v);
        let val = |k: &str| g(k).unwrap_or(0.0);
        let y0 = r.top() + 6.0 + row * i as f32;
        if i > 0 {
            p.hline((r.left() + 8.0)..=(r.right() - 8.0), y0 - 4.0, Stroke::new(1.0, ink::GRID));
        }
        let mid = y0 + 34.0;
        p.text(pos2(r.left() + 10.0, y0), Align2::LEFT_TOP, format!("{}: {}", c.label, with_unit(input.def, c.v)), small(), pn.color);
        // Input pulses.
        let xa = r.left() + 14.0;
        for k in 0..PULSES as usize {
            let x = xa + 6.0 + k as f32 * (in_w - 12.0) / (PULSES as f32 - 1.0);
            p.vline(x, (mid - 8.0)..=(mid + 8.0), Stroke::new(2.0, ink::COMMAND));
        }
        let (from, to, num, den) = match code {
            "P03-13" | "P03-14" => ("command pulses", "motor counts", val("P03-13"), val("P03-14")),
            "P03-22" | "P03-23" => ("encoder lines", "output lines", val("P03-23"), val("P03-22")),
            _ => ("command pulses", "motor counts", val("P03-12") * 10000.0 + val("P03-10"), val("P03-11")),
        };
        let per_turn = if code == "P03-09" { val("P03-09") } else { 0.0 };
        p.text(pos2(xa, mid + 11.0), Align2::LEFT_TOP, format!("{PULSES:.0} {from}"), small(), ink::COMMAND);
        // The box.
        let bx = xa + in_w + 24.0;
        let boxr = Rect::from_min_size(pos2(bx, mid - 12.0), vec2(box_w, 24.0));
        arrow(&p, pos2(xa + in_w + 4.0, mid), pos2(bx - 2.0, mid));
        p.rect_stroke(boxr, 4.0, Stroke::new(1.5, pn.color), StrokeKind::Inside);
        let out_x = boxr.right() + 26.0;
        arrow(&p, pos2(boxr.right() + 2.0, mid), pos2(out_x - 4.0, mid));
        if per_turn > 0.0 {
            p.text(boxr.center(), Align2::CENTER_CENTER, format!("{} pulses per turn", short(per_turn)), small(), pn.color);
            let turn = PULSES / per_turn;
            let centre = pos2(out_x + 14.0, mid);
            pie(&p, centre, 12.0, turn.min(1.0), pn.color);
            p.text(pos2(out_x + 34.0, mid), Align2::LEFT_CENTER, format!("= {} turn = {}°", sig(turn), sig(turn * 360.0)), small(), pn.color);
            continue;
        }
        let ratio_text = format!("× {} ÷ {}", short(num), short(den));
        p.text(boxr.center(), Align2::CENTER_CENTER, ratio_text, small(), pn.color);
        if den <= 0.0 {
            p.text(pos2(out_x, mid), Align2::LEFT_CENTER, "Denominator 0: not valid", small(), ink::ALARM);
            continue;
        }
        let count = PULSES * num / den;
        let out_w = (r.right() - 14.0 - out_x).max(40.0);
        let shown = count.round().clamp(0.0, 60.0) as usize;
        for k in 0..shown {
            let x = out_x + if shown > 1 { k as f32 * out_w / (shown as f32 - 1.0) } else { 0.0 };
            p.vline(x, (mid - 8.0)..=(mid + 8.0), Stroke::new(1.5, pn.color));
        }
        let more = if count > 60.0 { " (60 shown)" } else { "" };
        p.text(pos2(out_x, mid + 11.0), Align2::LEFT_TOP, format!("{} {to}{more}", sig(count)), small(), pn.color);
        if code == "P03-09" {
            p.text(pos2(r.right() - 10.0, y0), Align2::RIGHT_TOP, "0: the gear P03-10 ÷ P03-11 is used", small(), theme::TEXT_DIM);
        }
    }
    match code {
        "P03-10" | "P03-11" | "P03-12" => note(ui, "Gear 1 numerator = P03-12 × 10000 + P03-10. Denominator = P03-11."),
        "P03-22" | "P03-23" => note(ui, "Output lines = encoder lines × P03-23 ÷ P03-22."),
        _ => {}
    }
    note(ui, "Each row shows what 8 input pulses give with that value.");
}

fn arrow(p: &Painter, from: Pos2, to: Pos2) {
    let s = Stroke::new(1.2, theme::TEXT_DIM);
    p.line_segment([from, to], s);
    p.line_segment([to, to + vec2(-5.0, -4.0)], s);
    p.line_segment([to, to + vec2(-5.0, 4.0)], s);
}

/// A part of a circle: `frac` of a turn, clockwise from the top, with a mark at the start.
fn pie(p: &Painter, c: Pos2, radius: f32, frac: f64, color: Color32) {
    p.circle_stroke(c, radius, Stroke::new(1.0, theme::TEXT_DIM));
    // A thick arc. Filled thin slivers would give long spikes at their sharp corners.
    let mid = radius / 2.0;
    if frac * std::f64::consts::TAU * f64::from(mid) > 0.5 {
        let segs = ((frac * 64.0).ceil() as usize).max(2);
        let pts = (0..=segs)
            .map(|k| {
                let a = (frac * k as f64 / segs as f64) as f32 * std::f32::consts::TAU;
                c + vec2(a.sin(), -a.cos()) * mid
            })
            .collect();
        p.add(Shape::line(pts, Stroke::new(radius, color)));
    }
    p.line_segment([c, c + vec2(0.0, -radius)], Stroke::new(1.5, color));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(short(2.5), "2.5");
        assert_eq!(short(10.0), "10");
        assert_eq!(short(-0.0001), "0");
        assert_eq!(sig(0.000336), "0.000336");
        assert_eq!(sig(1234.5), "1234.5");
        assert_eq!(sig(0.12), "0.12");
        assert!(unstable(&(0..100).map(|i| (i as f64 * 0.3).sin() * 1.1f64.powi(i)).collect::<Vec<_>>()));
        assert!(!unstable(&(0..100).map(|i| (i as f64 * 0.3).sin() * 0.95f64.powi(i)).collect::<Vec<_>>()));
        assert_eq!(hz_text(2000.0), "2k");
        assert_eq!(nice_step(1000.0, 5.0), 200.0);
        assert_eq!(nice_step(12.0, 5.0), 2.5);
        assert_eq!(ticks((0.0, 1.0)), vec![0.0, 0.2, 0.4, 0.6000000000000001, 0.8, 1.0]);
    }
}
