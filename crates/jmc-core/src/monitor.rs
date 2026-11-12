//! Live state: the monitor block 2100..2134 and the status/alarm registers.
//! Register meanings come from the JAND manual (chapter 10) and JMC's monitor list. They are not
//! yet checked on a drive.
//!
//! Positions come in two units. JMC's monitor list gives 2100..2105 in command pulses (the steps
//! that LinuxCNC sends, and the unit of P03-15) and 2106..2111 in encoder counts (131072 for each
//! turn on an absolute encoder). The manual gives no unit for either.
//!
//! P00-29 swaps 2125 and 2126: one holds the absolute turn count and the other the position in the turn.

use serde::Serialize;

use crate::error::{Error, Result};

pub const MONITOR_ADDR: u16 = 2100;
pub const MONITOR_LEN: u16 = 35;
pub const STATUS_ADDR: u16 = 2020;
pub const ALARM_ADDR: u16 = 2025;
pub const ALARM_LEN: u16 = 7;
pub const SERVO_ENABLE: u16 = 2000;
pub const ALARM_RESET: u16 = 2001;
pub const IDENTITY_ADDR: u16 = 2150;

/// Which register holds the absolute turn count. P00-29 swaps 2125 and 2126.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AbsLayout {
    /// P00-29 = 0: 2125 holds the turns and 2126 the position in the turn.
    TurnsAt2125,
    /// P00-29 = 1, the factory value: 2126 holds the turns and 2125 the position in the turn.
    #[default]
    TurnsAt2126,
}

impl AbsLayout {
    pub fn from_p00_29(value: u16) -> AbsLayout {
        if value == 0 { AbsLayout::TurnsAt2125 } else { AbsLayout::TurnsAt2126 }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Sample {
    /// Command pulses (2100..2105): the unit of P03-15 (× 10) and of LinuxCNC's steps.
    pub pos_cmd: i32,
    pub pos_fb: i32,
    pub following_error: i32,
    /// Encoder counts (2106..2111): the unit of P03-06.
    pub pos_cmd_counts: i32,
    pub pos_fb_counts: i32,
    pub following_error_counts: i32,
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

impl Sample {
    /// Decode the block read from `MONITOR_ADDR`. `layout` comes from P00-29.
    pub fn decode(r: &[u16], layout: AbsLayout) -> Result<Sample> {
        if r.len() < usize::from(MONITOR_LEN) {
            return Err(Error::BadReply(format!("monitor block has {} registers, need {MONITOR_LEN}", r.len())));
        }
        // Register `addr` of the block.
        let u = |addr: u16| r[usize::from(addr - MONITOR_ADDR)];
        let s = |addr: u16| u(addr) as i16;
        let tenth = |addr: u16| f32::from(s(addr)) / 10.0;
        // High word first.
        let i32_at = |addr: u16| ((u32::from(u(addr)) << 16) | u32::from(u(addr + 1))) as i32;
        let (turns, in_turn) = match layout {
            AbsLayout::TurnsAt2125 => (2125, 2126),
            AbsLayout::TurnsAt2126 => (2126, 2125),
        };
        Ok(Sample {
            pos_cmd: i32_at(2100),
            pos_fb: i32_at(2102),
            following_error: i32_at(2104),
            pos_cmd_counts: i32_at(2106),
            pos_fb_counts: i32_at(2108),
            following_error_counts: i32_at(2110),
            speed_cmd_rpm: s(2113),
            speed_rpm: s(2114),
            torque_cmd_pct: tenth(2115),
            torque_pct: tenth(2116),
            load_pct: f32::from(u(2117)) / 10.0,
            peak_torque_pct: tenth(2118),
            regen_pct: f32::from(u(2120)) / 10.0,
            io: u(2121),
            angle_deg: f32::from(u(2123)) / 10.0,
            abs_turns: s(turns),
            abs_pos: u(in_turn),
            bus_v: u(2128),
            ain_speed_v: f32::from(s(2133)) / 100.0,
            ain_torque_v: f32::from(s(2134)) / 100.0,
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
        let mut r = vec![0u16; usize::from(MONITOR_LEN)];
        let mut set = |addr: u16, v: u16| r[usize::from(addr - MONITOR_ADDR)] = v;
        set(2100, 0xFFFF);
        set(2101, 0xFFFE); // -2
        set(2105, 12);
        set(2110, 0x0001);
        set(2111, 0x0000); // 65536
        set(2114, (-150i16) as u16);
        set(2116, (-123i16) as u16);
        set(2125, 0x4000);
        set(2126, 3);
        set(2128, 312);
        set(2134, (-250i16) as u16);
        let s = Sample::decode(&r, AbsLayout::TurnsAt2126).unwrap();
        assert_eq!((s.abs_turns, s.abs_pos), (3, 0x4000));
        let s = Sample::decode(&r, AbsLayout::TurnsAt2125).unwrap();
        assert_eq!((s.abs_turns, s.abs_pos), (0x4000, 3));
        assert_eq!(s.pos_cmd, -2);
        assert_eq!(s.following_error, 12);
        assert_eq!(s.following_error_counts, 65536);
        assert_eq!(s.speed_rpm, -150);
        assert!((s.torque_pct + 12.3).abs() < 1e-4);
        assert_eq!(s.bus_v, 312);
        assert!((s.ain_torque_v + 2.5).abs() < 1e-4);
        assert!(Sample::decode(&r[..10], AbsLayout::default()).is_err());
    }

    #[test]
    fn decode_status() {
        let s = Status::decode(&[0xC000, 0x0008], &[0, 0x0101, 0, 0, 0, 0, 0]).unwrap();
        assert!(s.fault && s.warning && s.servo_on);
        assert_eq!(s.alarms, vec![0x0101]);
    }
}
