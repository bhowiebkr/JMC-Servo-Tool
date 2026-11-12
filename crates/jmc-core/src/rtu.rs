//! Modbus RTU framing: requests, reply scanning with resync, exception replies, CRC.

/// Modbus CRC-16 (poly 0xA001, init 0xFFFF). Sent low byte first.
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for &b in data {
        crc ^= u16::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xA001
            } else {
                crc >> 1
            };
        }
    }
    crc
}

/// The Modbus limit for one read. The drive's own limit may be lower (measured in phase 0).
pub const MAX_READ: u16 = 125;
/// Bit 15 of a parameter address: write to RAM and EEPROM (what JMC's tool does; not yet tested).
pub const SAVE_BIT: u16 = 0x8000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Read { addr: u16, count: u16 },
    WriteOne { addr: u16, value: u16 },
    WriteMany { addr: u16, values: Vec<u16> },
}

impl Request {
    pub fn function(&self) -> u8 {
        match self {
            Request::Read { .. } => 0x03,
            Request::WriteOne { .. } => 0x06,
            Request::WriteMany { .. } => 0x10,
        }
    }

    pub fn encode(&self, slave: u8) -> Vec<u8> {
        let mut f = vec![slave, self.function()];
        let mut push = |v: u16| f.extend_from_slice(&v.to_be_bytes());
        match self {
            Request::Read { addr, count } => {
                push(*addr);
                push(*count);
            }
            Request::WriteOne { addr, value } => {
                push(*addr);
                push(*value);
            }
            Request::WriteMany { addr, values } => {
                push(*addr);
                push(values.len() as u16);
                f.push((values.len() * 2) as u8);
                for v in values {
                    f.extend_from_slice(&v.to_be_bytes());
                }
            }
        }
        let crc = crc16(&f);
        f.extend_from_slice(&crc.to_le_bytes());
        f
    }

    /// Short text for logs and errors, e.g. `R03 2106 x29`.
    pub fn describe(&self) -> String {
        match self {
            Request::Read { addr, count } => format!("R03 {addr} x{count}"),
            Request::WriteOne { addr, value } => format!("W06 {addr} = {value}"),
            Request::WriteMany { addr, values } => format!("W10 {addr} x{}", values.len()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    Registers(Vec<u16>),
    Written,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameResult {
    Reply(Reply),
    Exception(u8),
    /// CRC was fine but the content doesn't match the request (wrong count or echo).
    Mismatch(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scan {
    /// Not enough bytes yet.
    NeedMore,
    /// Drop this many bytes from the front and scan again. `crc` is set when a frame failed its CRC.
    Skip { n: usize, crc: bool },
    /// A whole frame of `len` bytes was found at the front.
    Frame { len: usize, result: FrameResult },
}

/// Look for the reply to `req` at the start of `buf`. Junk in front of a real reply is skipped
/// one byte at a time, so a stray byte or a late fragment cannot shift every later reply.
pub fn scan(buf: &[u8], slave: u8, req: &Request) -> Scan {
    if buf.is_empty() {
        return Scan::NeedMore;
    }
    if buf[0] != slave {
        return Scan::Skip { n: 1, crc: false };
    }
    if buf.len() < 2 {
        return Scan::NeedMore;
    }
    let func = buf[1];
    let len = if func == req.function() | 0x80 {
        5
    } else if func != req.function() {
        return Scan::Skip { n: 1, crc: false };
    } else {
        match req {
            Request::Read { count, .. } => {
                if buf.len() < 3 {
                    return Scan::NeedMore;
                }
                if usize::from(buf[2]) != usize::from(*count) * 2 {
                    return Scan::Skip { n: 1, crc: false };
                }
                5 + usize::from(buf[2])
            }
            Request::WriteOne { .. } | Request::WriteMany { .. } => 8,
        }
    };
    if buf.len() < len {
        return Scan::NeedMore;
    }
    let body = &buf[..len - 2];
    let crc = u16::from_le_bytes([buf[len - 2], buf[len - 1]]);
    if crc16(body) != crc {
        return Scan::Skip { n: 1, crc: true };
    }
    let result = if func & 0x80 != 0 {
        FrameResult::Exception(buf[2])
    } else {
        let word = |i: usize| u16::from_be_bytes([buf[i], buf[i + 1]]);
        match req {
            Request::Read { .. } => FrameResult::Reply(Reply::Registers(
                (0..usize::from(buf[2]) / 2).map(|k| word(3 + 2 * k)).collect(),
            )),
            Request::WriteOne { addr, value } => {
                if word(2) == *addr && word(4) == *value {
                    FrameResult::Reply(Reply::Written)
                } else {
                    FrameResult::Mismatch(format!(
                        "write echo {} = {} for {addr} = {value}",
                        word(2),
                        word(4)
                    ))
                }
            }
            Request::WriteMany { addr, values } => {
                if word(2) == *addr && usize::from(word(4)) == values.len() {
                    FrameResult::Reply(Reply::Written)
                } else {
                    FrameResult::Mismatch(format!(
                        "write echo {} x{} for {addr} x{}",
                        word(2),
                        word(4),
                        values.len()
                    ))
                }
            }
        }
    };
    Scan::Frame { len, result }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_known_frame() {
        // Standard example: read 1 holding register at 0 from slave 1.
        assert_eq!(
            Request::Read { addr: 0, count: 1 }.encode(1),
            vec![0x01, 0x03, 0x00, 0x00, 0x00, 0x01, 0x84, 0x0A]
        );
    }

    fn reply(bytes: &[u8]) -> Vec<u8> {
        let mut v = bytes.to_vec();
        v.extend_from_slice(&crc16(bytes).to_le_bytes());
        v
    }

    #[test]
    fn read_reply_and_exception() {
        let req = Request::Read { addr: 104, count: 2 };
        let ok = reply(&[1, 3, 4, 0x00, 0xFA, 0x12, 0x34]);
        assert_eq!(
            scan(&ok, 1, &req),
            Scan::Frame {
                len: 9,
                result: FrameResult::Reply(Reply::Registers(vec![250, 0x1234]))
            }
        );
        assert_eq!(scan(&ok[..6], 1, &req), Scan::NeedMore);
        let ex = reply(&[1, 0x83, 2]);
        assert_eq!(
            scan(&ex, 1, &req),
            Scan::Frame {
                len: 5,
                result: FrameResult::Exception(2)
            }
        );
    }

    #[test]
    fn junk_and_bad_crc_are_skipped() {
        let req = Request::WriteOne { addr: 5, value: 7 };
        let mut buf = vec![0x00, 0x7F];
        buf.extend(req.encode(1));
        assert_eq!(scan(&buf, 1, &req), Scan::Skip { n: 1, crc: false });
        let mut bad = req.encode(1);
        bad[7] ^= 0xFF;
        assert_eq!(scan(&bad, 1, &req), Scan::Skip { n: 1, crc: true });
    }

    #[test]
    fn wrong_echo_is_a_mismatch() {
        let req = Request::WriteOne { addr: 5, value: 7 };
        let other = Request::WriteOne { addr: 5, value: 8 }.encode(1);
        assert!(matches!(
            scan(&other, 1, &req),
            Scan::Frame {
                result: FrameResult::Mismatch(_),
                ..
            }
        ));
    }
}
