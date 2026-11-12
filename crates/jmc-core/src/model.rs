//! Simple models of the drive's control loops, for the help drawings.
//!
//! These are teaching models, not a copy of the drive's firmware: a rigid load with the inertia
//! ratio set right, a position loop (P), a speed loop (PI with PDFF), first-order filters and
//! speed feedforward. They show the direction and the size of an effect, not exact numbers.

use std::f64::consts::PI;

/// The parameters of one gain set, in the drive's display units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Loop {
    /// Position loop gain, 1/s.
    pub kp: f64,
    /// Speed loop gain, Hz (the speed loop bandwidth when the inertia ratio is right).
    pub kv_hz: f64,
    /// Speed loop integral time, ms. 1000 ms or more is treated as no integral action.
    pub ti_ms: f64,
    /// PDFF factor, %: 100 is a PI controller, 0 puts all proportional action on the feedback.
    pub pdff: f64,
    /// Speed feedforward, %.
    pub ff: f64,
    /// Speed feedforward filter, ms.
    pub ff_filter_ms: f64,
    /// Torque command filter, ms.
    pub torque_filter_ms: f64,
    /// Speed feedback filter, ms.
    pub speed_filter_ms: f64,
    /// Position command filter, ms.
    pub cmd_filter_ms: f64,
    /// Real speed loop gain divided by the set gain. 1 when the inertia ratio is set right.
    pub gain_scale: f64,
}

/// The registers that feed [`Loop`], for gain set 1 or 2.
pub fn loop_codes(set: u8) -> [(&'static str, &'static str); 9] {
    // Both gain sets use the torque command filter P08-20. JMC names P08-21 the filter for low
    // speed, not a filter of gain set 2, so the model leaves it out.
    let (kp, kv, ti, pdff) = if set == 2 {
        ("P02-01", "P02-13", "P02-14", "P02-15")
    } else {
        ("P02-00", "P02-10", "P02-11", "P02-12")
    };
    [
        ("kp", kp),
        ("kv", kv),
        ("ti", ti),
        ("pdff", pdff),
        ("ff", "P02-03"),
        ("ff_filter", "P02-04"),
        ("torque_filter", "P08-20"),
        ("speed_filter", "P08-19"),
        ("cmd_filter", "P03-16"),
    ]
}

impl Loop {
    /// Build a gain set from display values looked up by code. Missing codes get a typical value.
    pub fn from_values(set: u8, get: impl Fn(&str) -> Option<f64>) -> Loop {
        let c = loop_codes(set);
        let v = |i: usize, fallback: f64| get(c[i].1).unwrap_or(fallback);
        Loop {
            kp: v(0, 48.0),
            kv_hz: v(1, 27.0),
            ti_ms: v(2, 10.0),
            pdff: v(3, 100.0),
            ff: v(4, 30.0),
            ff_filter_ms: v(5, 0.5),
            torque_filter_ms: v(6, 0.8),
            speed_filter_ms: v(7, 0.8),
            cmd_filter_ms: v(8, 0.0),
            gain_scale: 1.0,
        }
    }

    fn wc(&self) -> f64 {
        2.0 * PI * self.kv_hz.max(0.1) * self.gain_scale.max(0.01)
    }
}

/// What the model is asked to follow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    /// A move with a trapezoid speed profile: up to `speed` (rpm) in `ramp_ms`, hold for
    /// `hold_ms`, down again in `ramp_ms`.
    Move { speed: f64, ramp_ms: f64, hold_ms: f64 },
    /// A step of the speed command to `speed` (rpm), with the position loop open.
    SpeedStep { speed: f64 },
}

impl Command {
    /// The commanded speed (rpm) and position (motor turns) at time `t` (s).
    fn at(&self, t: f64) -> (f64, f64) {
        match *self {
            Command::SpeedStep { speed } => (if t >= 0.0 { speed } else { 0.0 }, 0.0),
            Command::Move { speed, ramp_ms, hold_ms } => {
                let vmax = speed / 60.0;
                let tr = (ramp_ms / 1000.0).max(1e-6);
                let th = hold_ms / 1000.0;
                let a = vmax / tr;
                let (v, x) = if t <= 0.0 {
                    (0.0, 0.0)
                } else if t < tr {
                    (a * t, 0.5 * a * t * t)
                } else if t < tr + th {
                    (vmax, 0.5 * vmax * tr + vmax * (t - tr))
                } else if t < 2.0 * tr + th {
                    let u = t - tr - th;
                    (vmax - a * u, 0.5 * vmax * tr + vmax * th + vmax * u - 0.5 * a * u * u)
                } else {
                    (0.0, vmax * (tr + th))
                };
                (v * 60.0, x)
            }
        }
    }
}

/// Samples of one model run. Times in ms, speeds in rpm, positions in motor turns. The error is
/// the following error in turns for a move, and the speed error in rpm for a speed step.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Trace {
    pub t_ms: Vec<f64>,
    pub speed_cmd: Vec<f64>,
    pub speed: Vec<f64>,
    pub pos_cmd: Vec<f64>,
    pub pos: Vec<f64>,
    pub error: Vec<f64>,
}

/// One first-order filter step: `alpha` for a filter of `t_ms` with a step of `dt` seconds.
fn alpha(t_ms: f64, dt: f64) -> f64 {
    if t_ms <= 0.0 { 1.0 } else { 1.0 - (-dt / (t_ms / 1000.0)).exp() }
}

/// Run the model for `t_end_ms` and keep `samples` evenly spaced samples.
pub fn simulate(p: &Loop, cmd: Command, t_end_ms: f64, samples: usize) -> Trace {
    let samples = samples.max(2);
    let t_end = (t_end_ms / 1000.0).max(1e-4);
    let wc = p.wc();
    // Small enough for the fastest part of the loop; capped so a frame stays cheap.
    let fastest = [1.0 / wc, p.torque_filter_ms / 1000.0, p.speed_filter_ms / 1000.0]
        .into_iter()
        .filter(|&x| x > 0.0)
        .fold(f64::INFINITY, f64::min);
    let dt = (fastest / 20.0).max(t_end / 400_000.0).min(t_end / 2000.0);
    let steps = (t_end / dt).ceil() as usize;
    let ti = p.ti_ms / 1000.0;
    let ki = if p.ti_ms > 0.0 && p.ti_ms < 1000.0 { wc / ti } else { 0.0 };
    let f = (p.pdff / 100.0).clamp(0.0, 1.0);
    let (a_cmd, a_ff, a_tq, a_sp) =
        (alpha(p.cmd_filter_ms, dt), alpha(p.ff_filter_ms, dt), alpha(p.torque_filter_ms, dt), alpha(p.speed_filter_ms, dt));

    // State, in turns and turns per second.
    let (mut xr, mut xr_prev, mut vff, mut x, mut v, mut v_meas, mut integ, mut tq) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let mut out = Trace::default();
    let mut next = 0usize;
    for i in 0..=steps {
        let t = i as f64 * dt;
        let (v_cmd_rpm, x_cmd) = cmd.at(t);
        let v_ref = match cmd {
            Command::SpeedStep { .. } => v_cmd_rpm / 60.0,
            Command::Move { .. } => {
                xr += (x_cmd - xr) * a_cmd;
                let d = (xr - xr_prev) / dt;
                xr_prev = xr;
                vff += (d - vff) * a_ff;
                p.kp * (xr - x) + p.ff / 100.0 * vff
            }
        };
        let e = v_ref - v_meas;
        integ += e * dt;
        let tq_cmd = wc * (f * v_ref - v_meas) + ki * integ;
        tq += (tq_cmd - tq) * a_tq;
        v += tq * dt;
        x += v * dt;
        v_meas += (v - v_meas) * a_sp;

        while next < samples && t >= t_end * next as f64 / (samples - 1) as f64 - dt * 0.5 {
            out.t_ms.push(t * 1000.0);
            out.speed_cmd.push(v_cmd_rpm);
            out.speed.push(v * 60.0);
            out.pos_cmd.push(x_cmd);
            out.pos.push(x);
            out.error.push(match cmd {
                Command::Move { .. } => x_cmd - x,
                Command::SpeedStep { .. } => v_cmd_rpm - v * 60.0,
            });
            next += 1;
        }
    }
    out
}

/// Real speed loop gain divided by the set gain, when the inertia ratio is set to `set` but the
/// load has `real`.
pub fn inertia_scale(set: f64, real: f64) -> f64 {
    (1.0 + set.max(0.0)) / (1.0 + real.max(0.0))
}

/// Output of a first-order low-pass filter `t` ms after a unit step.
pub fn lowpass_step(t_ms: f64, tau_ms: f64) -> f64 {
    if t_ms < 0.0 {
        0.0
    } else if tau_ms <= 0.0 {
        1.0
    } else {
        1.0 - (-t_ms / tau_ms).exp()
    }
}

/// Gain (0..1) of a first-order low-pass filter at `hz`.
pub fn lowpass_gain(hz: f64, tau_ms: f64) -> f64 {
    let w = 2.0 * PI * hz * tau_ms / 1000.0;
    1.0 / (1.0 + w * w).sqrt()
}

/// Corner frequency (Hz) of a first-order low-pass filter: 1 / (2π × time constant).
pub fn corner_hz(tau_ms: f64) -> Option<f64> {
    (tau_ms > 0.0).then(|| 1000.0 / (2.0 * PI * tau_ms))
}

/// The notch frequency that means "off".
pub const NOTCH_OFF_HZ: f64 = 5000.0;

/// −3 dB width ÷ centre frequency for a width grade. The manual gives no scale, so this is a
/// model: each 4 grades double the width, grade 4 is as wide as the centre frequency.
pub fn notch_width_ratio(grade: f64) -> f64 {
    2f64.powf((grade - 4.0) / 4.0)
}

/// Gain (0..1) of a notch filter at `hz`. Depth grade = output/input at the centre in %.
pub fn notch_gain(hz: f64, centre_hz: f64, width_grade: f64, depth_grade: f64) -> f64 {
    if centre_hz >= NOTCH_OFF_HZ || centre_hz <= 0.0 {
        return 1.0;
    }
    let zp = notch_width_ratio(width_grade) / 2.0;
    let zz = zp * (depth_grade / 100.0).clamp(0.0, 1.0);
    let (w, w0) = (hz, centre_hz);
    let a = w0 * w0 - w * w;
    ((a * a + (2.0 * zz * w0 * w).powi(2)) / (a * a + (2.0 * zp * w0 * w).powi(2))).sqrt()
}

/// Speed (rpm) `t` ms into a ramp to `target` rpm with a ramp time of `ms_per_1000` ms per
/// 1000 rpm. A ramp time of 0 is a step.
pub fn ramp(t_ms: f64, target: f64, ms_per_1000: f64) -> f64 {
    if t_ms < 0.0 {
        return 0.0;
    }
    if ms_per_1000 <= 0.0 {
        return target;
    }
    let v = t_ms * 1000.0 / ms_per_1000;
    v.min(target.abs()) * target.signum()
}

/// Points of a Catmull-Rom spline through `points`, `per` samples between each pair.
pub fn catmull_rom(points: &[(f64, f64)], per: usize) -> Vec<(f64, f64)> {
    if points.len() < 3 || per == 0 {
        return points.to_vec();
    }
    let n = points.len();
    let mut out = Vec::with_capacity((n - 1) * per + 1);
    for i in 0..n - 1 {
        let p0 = points[i.saturating_sub(1)];
        let (p1, p2) = (points[i], points[i + 1]);
        let p3 = points[(i + 2).min(n - 1)];
        for k in 0..per {
            let t = k as f64 / per as f64;
            let (t2, t3) = (t * t, t * t * t);
            let f = |a: f64, b: f64, c: f64, d: f64| {
                0.5 * (2.0 * b + (-a + c) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (-a + 3.0 * b - 3.0 * c + d) * t3)
            };
            out.push((f(p0.0, p1.0, p2.0, p3.0), f(p0.1, p1.1, p2.1, p3.1)));
        }
    }
    out.push(points[n - 1]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Loop {
        Loop::from_values(1, |_| None)
    }

    fn at(tr: &Trace, t_ms: f64) -> usize {
        tr.t_ms.iter().position(|&t| t >= t_ms).unwrap()
    }

    #[test]
    fn following_error_at_constant_speed_is_speed_over_gain() {
        let p = Loop { ff: 0.0, ..base() };
        let tr = simulate(&p, Command::Move { speed: 1000.0, ramp_ms: 50.0, hold_ms: 400.0 }, 700.0, 701);
        let e = tr.error[at(&tr, 400.0)];
        let want = 1000.0 / 60.0 / p.kp;
        assert!((e - want).abs() / want < 0.02, "error {e}, want {want}");
        // It returns to zero after the move.
        assert!(tr.error.last().unwrap().abs() < want * 0.05);
    }

    #[test]
    fn full_feedforward_removes_most_of_the_error() {
        let p = Loop { ff: 100.0, ff_filter_ms: 0.0, ..base() };
        let tr = simulate(&p, Command::Move { speed: 1000.0, ramp_ms: 50.0, hold_ms: 400.0 }, 700.0, 701);
        let without = 1000.0 / 60.0 / p.kp;
        assert!(tr.error[at(&tr, 400.0)].abs() < without * 0.05);
    }

    #[test]
    fn speed_step_settles_on_the_command() {
        let tr = simulate(&base(), Command::SpeedStep { speed: 1000.0 }, 200.0, 201);
        assert!((tr.speed.last().unwrap() - 1000.0).abs() < 10.0);
        // A lower gain is slower: less speed after 5 ms.
        let slow = simulate(&Loop { kv_hz: 10.0, ..base() }, Command::SpeedStep { speed: 1000.0 }, 200.0, 201);
        assert!(slow.speed[at(&slow, 5.0)] < tr.speed[at(&tr, 5.0)]);
    }

    #[test]
    fn filters() {
        assert!((lowpass_step(2.0, 2.0) - 0.632).abs() < 0.001);
        assert_eq!(lowpass_step(1.0, 0.0), 1.0);
        let fc = corner_hz(0.8).unwrap();
        assert!((fc - 198.9).abs() < 0.1);
        assert!((lowpass_gain(fc, 0.8) - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-9);
        assert!((notch_gain(800.0, 800.0, 2.0, 0.0)).abs() < 1e-9);
        assert!((notch_gain(800.0, 800.0, 2.0, 50.0) - 0.5).abs() < 1e-9);
        assert!(notch_gain(80.0, 800.0, 2.0, 0.0) > 0.98);
        assert_eq!(notch_gain(800.0, NOTCH_OFF_HZ, 2.0, 0.0), 1.0);
        assert_eq!(ramp(50.0, 1000.0, 100.0), 500.0);
        assert_eq!(ramp(500.0, -1000.0, 100.0), -1000.0);
        assert_eq!(inertia_scale(3.0, 3.0), 1.0);
        assert!(inertia_scale(1.0, 3.0) < 1.0);
    }

    #[test]
    fn spline_passes_through_its_points() {
        let pts = [(0.0, 0.0), (1.0, 2.0), (2.0, 1.0), (3.0, 3.0)];
        let s = catmull_rom(&pts, 8);
        assert_eq!(s.len(), 3 * 8 + 1);
        for (i, p) in pts.iter().enumerate() {
            let q = s[(i * 8).min(s.len() - 1)];
            assert!((q.0 - p.0).abs() < 1e-9 && (q.1 - p.1).abs() < 1e-9);
        }
    }
}
