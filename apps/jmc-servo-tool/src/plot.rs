//! Live monitor strips, drawn with the painter (no plotting crate).

use std::collections::VecDeque;

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke};
use jmc_core::monitor::Sample;

use crate::theme;

pub struct Series {
    pub label: &'static str,
    pub color: Color32,
    pub get: fn(&Sample) -> f64,
}

pub struct Channel {
    /// Key stored in the settings.
    pub key: &'static str,
    pub title: &'static str,
    pub unit: &'static str,
    pub series: &'static [Series],
}

pub const CHANNELS: &[Channel] = &[
    Channel {
        key: "following_error",
        title: "Following error",
        unit: "pulses",
        series: &[Series { label: "error", color: theme::ACCENT, get: |s| f64::from(s.following_error) }],
    },
    Channel {
        key: "speed",
        title: "Speed",
        unit: "rpm",
        series: &[
            Series { label: "command", color: theme::BLUE, get: |s| f64::from(s.speed_cmd_rpm) },
            Series { label: "actual", color: theme::ACCENT, get: |s| f64::from(s.speed_rpm) },
        ],
    },
    Channel {
        key: "torque",
        title: "Torque",
        unit: "% rated",
        series: &[
            Series { label: "command", color: theme::BLUE, get: |s| f64::from(s.torque_cmd_pct) },
            Series { label: "actual", color: theme::ACCENT, get: |s| f64::from(s.torque_pct) },
        ],
    },
    Channel {
        key: "load",
        title: "Load",
        unit: "%",
        series: &[Series { label: "load", color: theme::ACCENT, get: |s| f64::from(s.load_pct) }],
    },
    Channel {
        key: "position",
        title: "Position",
        unit: "pulses",
        series: &[
            Series { label: "command", color: theme::BLUE, get: |s| f64::from(s.pos_cmd) },
            Series { label: "feedback", color: theme::ACCENT, get: |s| f64::from(s.pos_fb) },
        ],
    },
    Channel {
        key: "bus_v",
        title: "Bus voltage",
        unit: "V",
        series: &[Series { label: "bus", color: theme::ACCENT, get: |s| f64::from(s.bus_v) }],
    },
];

#[cfg(test)]
fn channel(key: &str) -> Option<&'static Channel> {
    CHANNELS.iter().find(|c| c.key == key)
}

/// Lowest and highest value of `series` over samples newer than `t0`.
fn range(samples: &VecDeque<(f64, Sample)>, series: &[Series], t0: f64) -> Option<(f64, f64)> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for (t, s) in samples.iter().rev() {
        if *t < t0 {
            break;
        }
        for se in series {
            let v = (se.get)(s);
            lo = lo.min(v);
            hi = hi.max(v);
        }
    }
    (lo <= hi).then_some((lo, hi))
}

/// Scale with some headroom; never a zero span.
fn padded(lo: f64, hi: f64) -> (f64, f64) {
    let span = (hi - lo).max(1e-9);
    let pad = if hi - lo < 1e-9 { lo.abs().max(1.0) * 0.1 } else { span * 0.1 };
    (lo - pad, hi + pad)
}

pub fn format_value(v: f64) -> String {
    if v.abs() >= 1000.0 || v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v:.1}") }
}

/// One strip: title, latest values, the lines over the last `window` seconds.
pub fn strip(ui: &mut egui::Ui, ch: &Channel, samples: &VecDeque<(f64, Sample)>, window: f64, height: f32) {
    let width = ui.available_width();
    let (resp, painter) = ui.allocate_painter(egui::vec2(width, height), Sense::hover());
    let rect = resp.rect;
    painter.rect_filled(rect, 6.0, theme::CANVAS);
    painter.rect_stroke(rect, 6.0, Stroke::new(1.0, theme::LINE), egui::StrokeKind::Inside);
    let plot = Rect::from_min_max(rect.min + egui::vec2(10.0, 24.0), rect.max - egui::vec2(70.0, 8.0));
    let small = FontId::proportional(12.0);

    // Title and latest values.
    let mut x = rect.min.x + 10.0;
    let r = painter.text(Pos2::new(x, rect.min.y + 5.0), Align2::LEFT_TOP, ch.title, FontId::proportional(13.0), theme::TEXT);
    x = r.max.x + 6.0;
    let r = painter.text(Pos2::new(x, rect.min.y + 6.0), Align2::LEFT_TOP, ch.unit, small.clone(), theme::TEXT_DIM);
    x = r.max.x + 16.0;
    if let Some((_, last)) = samples.back() {
        for se in ch.series {
            let text = if ch.series.len() > 1 {
                format!("{} {}", se.label, format_value((se.get)(last)))
            } else {
                format_value((se.get)(last))
            };
            let r = painter.text(Pos2::new(x, rect.min.y + 6.0), Align2::LEFT_TOP, text, small.clone(), se.color);
            x = r.max.x + 14.0;
        }
    }

    let Some(&(now, _)) = samples.back() else {
        painter.text(plot.center(), Align2::CENTER_CENTER, "no samples", small, theme::TEXT_DIM);
        return;
    };
    let t0 = now - window;
    let Some((lo, hi)) = range(samples, ch.series, t0) else { return };
    let (lo, hi) = padded(lo, hi);
    let to_y = |v: f64| plot.max.y - ((v - lo) / (hi - lo)) as f32 * plot.height();
    let to_x = |t: f64| plot.min.x + ((t - t0) / window) as f32 * plot.width();

    // Grid: zero line when in range, plus top and bottom labels.
    if lo < 0.0 && hi > 0.0 {
        painter.hline(plot.x_range(), to_y(0.0), Stroke::new(1.0, theme::LINE));
    }
    for v in [hi, lo] {
        let y = to_y(v).clamp(plot.min.y, plot.max.y);
        painter.text(Pos2::new(plot.max.x + 6.0, y), Align2::LEFT_CENTER, format_value(v), small.clone(), theme::TEXT_DIM);
    }

    let clip = painter.with_clip_rect(plot.expand(1.0));
    for se in ch.series {
        let pts: Vec<Pos2> = samples
            .iter()
            .skip_while(|(t, _)| *t < t0)
            .map(|(t, s)| Pos2::new(to_x(*t), to_y((se.get)(s))))
            .collect();
        if pts.len() >= 2 {
            clip.line(pts, Stroke::new(1.5, se.color));
        }
    }

    // Hover: value under the cursor.
    if let Some(p) = resp.hover_pos().filter(|p| plot.contains(*p)) {
        let t = t0 + f64::from((p.x - plot.min.x) / plot.width()) * window;
        if let Some((ts, s)) = samples.iter().find(|(ts, _)| *ts >= t) {
            let x = to_x(*ts);
            clip.vline(x, plot.y_range(), Stroke::new(1.0, theme::TEXT_DIM));
            let mut text = format!("{:.2} s", ts - now);
            for se in ch.series {
                text.push_str(&format!("   {} {}", se.label, format_value((se.get)(s))));
            }
            painter.text(Pos2::new(plot.min.x + 4.0, plot.max.y - 4.0), Align2::LEFT_BOTTOM, text, small, theme::TEXT);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padding_never_gives_zero_span() {
        let (a, b) = padded(5.0, 5.0);
        assert!(b > a);
        let (a, b) = padded(0.0, 0.0);
        assert!(b > a);
        let (a, b) = padded(-10.0, 10.0);
        assert!(a < -10.0 && b > 10.0);
    }

    #[test]
    fn range_uses_only_the_window() {
        let mut v = VecDeque::new();
        for k in 0..10 {
            v.push_back((f64::from(k), Sample { following_error: k * 10, ..Default::default() }));
        }
        let (lo, hi) = range(&v, channel("following_error").unwrap().series, 5.0).unwrap();
        assert_eq!((lo, hi), (50.0, 90.0));
    }
}
