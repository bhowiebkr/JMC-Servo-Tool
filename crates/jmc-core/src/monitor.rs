//! Live state: the monitor block 2106..2134 and the status/alarm registers.
//! Register meanings come from the JAND manual (chapter 10) and are not yet checked on a drive.

use serde::Serialize;

use crate::error::{Error, Result};

pub const MONITOR_ADDR: u16 = 2106;
pub const MONITOR_LEN: u16 = 29;
pub const STATUS_ADDR: u16 = 2020;
pub const ALARM_ADDR: u16 = 2025;
pub const ALARM_LEN: u16 = 7;
pub const SERVO_ENABLE: u16 = 2000;
pub const ALARM_RESET: u16 = 2001;
pub const IDENTITY_ADDR: u16 = 2150;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Sample {
    /// Pulses.
    pub pos_cmd: i32,
    pub pos_fb: i32,
    pub following_error: i32,
    pub speed_cmd_rpm: i16,
    pub speed_rpm: i16,
    /// Percent of rated torque.
    pub torque_cmd_pct: f32,
    pub torque_pct: f32,
    pub load_pct: f32,
    pub peak_torque_pct: f32,
    pub regen_pct: f32,
    /// Low 8 bits inputs, middle 5 bits outputs, top 3 bits Hall.
    pub io: u16,
    pub angle_deg: f32,
    pub abs_turns: i16,
    /// 65536 counts per turn.
    pub abs_pos: u16,
    pub bus_v: u16,
    pub ain_speed_v: f32,
    pub ain_torque_v: f32,
}

fn i32_at(r: &[u16], k: usize) -> i32 {
    ((u32::from(r[k]) << 16) | u32::from(r[k + 1])) as i32
}

impl Sample {
    pub fn decode(r: &[u16]) -> Result<Sample> {
        if r.len() < usize::from(MONITOR_LEN) {
            return Err(Error::BadReply(format!("monitor block has {} registers, need {MONITOR_LEN}", r.len())));
        }
        let s = |k: usize| r[k] as i16;
        let tenth = |k: usize| f32::from(r[k] as i16) / 10.0;
        Ok(Sample {
            pos_cmd: i32_at(r, 0),
            pos_fb: i32_at(r, 2),
            following_error: i32_at(r, 4),
            speed_cmd_rpm: s(7),
            speed_rpm: s(8),
            torque_cmd_pct: tenth(9),
            torque_pct: tenth(10),
            load_pct: f32::from(r[11]) / 10.0,
            peak_torque_pct: tenth(12),
            regen_pct: f32::from(r[14]) / 10.0,
            io: r[15],
            angle_deg: f32::from(r[17]) / 10.0,
            abs_turns: s(19),
            abs_pos: r[20],
            bus_v: r[22],
            ain_speed_v: f32::from(s(27)) / 100.0,
            ain_torque_v: f32::from(s(28)) / 100.0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Status {
    pub raw: [u16; 2],
    pub fault: bool,
    pub warning: bool,
    pub servo_on: bool,
    /// Non-zero entries of the alarm block 2025..2031, as read.
    pub alarms: Vec<u16>,
}

impl Status {
    pub fn decode(status: &[u16], alarms: &[u16]) -> Result<Status> {
        if status.len() < 2 {
            return Err(Error::BadReply("status needs 2 registers".into()));
        }
        Ok(Status {
            raw: [status[0], status[1]],
            fault: status[0] & 0x8000 != 0,
            warning: status[0] & 0x4000 != 0,
            servo_on: status[1] & 0x0008 != 0,
            alarms: alarms.iter().copied().filter(|&a| a != 0).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_monitor() {
        let mut r = vec![0u16; 29];
        r[0] = 0xFFFF;
        r[1] = 0xFFFE; // -2
        r[4] = 0;
        r[5] = 12;
        r[8] = (-150i16) as u16;
        r[10] = (-123i16) as u16;
        r[22] = 312;
        let s = Sample::decode(&r).unwrap();
        assert_eq!(s.pos_cmd, -2);
        assert_eq!(s.following_error, 12);
        assert_eq!(s.speed_rpm, -150);
        assert!((s.torque_pct + 12.3).abs() < 1e-4);
        assert_eq!(s.bus_v, 312);
        assert!(Sample::decode(&r[..10]).is_err());
    }

    #[test]
    fn decode_status() {
        let s = Status::decode(&[0xC000, 0x0008], &[0, 0x0101, 0, 0, 0, 0, 0]).unwrap();
        assert!(s.fault && s.warning && s.servo_on);
        assert_eq!(s.alarms, vec![0x0101]);
    }
}
