//! A simulated model 512 drive for tests and offline work. It speaks Modbus RTU over the
//! `Transport` trait, keeps RAM and EEPROM copies of the parameters, and can inject faults.

use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crate::link::Transport;
use crate::params::Schema;
use crate::rtu::{crc16, Request, SAVE_BIT};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    /// Don't answer the next request.
    DropReply,
    /// Answer with a broken CRC.
    BadCrc,
    /// Send these bytes before the reply.
    JunkBefore(Vec<u8>),
    /// Send these bytes after the reply (a late fragment).
    JunkAfter(Vec<u8>),
    /// Answer with this exception code instead.
    Exception(u8),
}

#[derive(Debug)]
pub struct SimState {
    pub slave: u8,
    pub ram: BTreeMap<u16, u16>,
    pub eeprom: BTreeMap<u16, u16>,
    /// Read-only live registers (status, monitor, identity). Writes to 2000/2001 are accepted.
    pub live: BTreeMap<u16, u16>,
    pub max_read: u16,
    /// Every request that arrived with a good CRC, decoded.
    pub log: Vec<Request>,
    pub faults: VecDeque<Fault>,
    /// Addresses that answer exception 02 even though they are in the table (hidden parameters).
    pub hidden: Vec<u16>,
    /// Addresses whose writes are refused with exception 03 (drive-side range check).
    pub reject_writes: Vec<u16>,
    out: Vec<u8>,
}

/// Cloneable handle: hand one clone to a `Link`, keep another to inspect or inject faults.
#[derive(Clone)]
pub struct SimDrive(Arc<Mutex<SimState>>);

impl SimDrive {
    /// A drive holding the factory values from the parameter table.
    pub fn new(schema: &Schema) -> SimDrive {
        let mut ram = BTreeMap::new();
        for p in schema.real() {
            let raw = p.default.and_then(|v| p.to_raw(v).ok()).unwrap_or(0);
            ram.insert(p.address, raw);
        }
        let mut live = BTreeMap::new();
        for a in [2000, 2001] {
            live.insert(a, 0);
        }
        for a in 2020..2032 {
            live.insert(a, 0);
        }
        for a in 2106..=2134 {
            live.insert(a, 0);
        }
        for a in 2150..2155 {
            live.insert(a, 0);
        }
        live.insert(2128, 310); // bus voltage
        SimDrive(Arc::new(Mutex::new(SimState {
            slave: 1,
            eeprom: ram.clone(),
            ram,
            live,
            max_read: 125,
            log: Vec::new(),
            faults: VecDeque::new(),
            hidden: Vec::new(),
            reject_writes: Vec::new(),
            out: Vec::new(),
        })))
    }

    pub fn state(&self) -> MutexGuard<'_, SimState> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn push_fault(&self, f: Fault) {
        self.state().faults.push_back(f);
    }

    /// Power off and on: RAM reloads from EEPROM.
    pub fn power_cycle(&self) {
        let mut s = self.state();
        s.ram = s.eeprom.clone();
        s.live.insert(2000, 0);
    }

    pub fn set_param(&self, addr: u16, value: u16) {
        let mut s = self.state();
        s.ram.insert(addr, value);
        s.eeprom.insert(addr, value);
    }
}

fn frame(body: Vec<u8>) -> Vec<u8> {
    let mut f = body;
    let crc = crc16(&f);
    f.extend_from_slice(&crc.to_le_bytes());
    f
}

impl SimState {
    fn get(&self, addr: u16) -> Option<u16> {
        if self.hidden.contains(&addr) {
            return None;
        }
        self.ram.get(&addr).or_else(|| self.live.get(&addr)).copied()
    }

    fn decode(&self, data: &[u8]) -> Option<Request> {
        if data.len() < 8 || crc16(&data[..data.len() - 2]) != u16::from_le_bytes([data[data.len() - 2], data[data.len() - 1]]) {
            return None;
        }
        let w = |i: usize| u16::from_be_bytes([data[i], data[i + 1]]);
        match data[1] {
            0x03 => Some(Request::Read { addr: w(2), count: w(4) }),
            0x06 => Some(Request::WriteOne { addr: w(2), value: w(4) }),
            0x10 => {
                let n = usize::from(w(4));
                (data.len() == 9 + 2 * n).then(|| Request::WriteMany { addr: w(2), values: (0..n).map(|k| w(7 + 2 * k)).collect() })
            }
            _ => None,
        }
    }

    fn write(&mut self, addr: u16, value: u16) -> Result<(), u8> {
        let save = addr & SAVE_BIT != 0;
        let a = addr & !SAVE_BIT;
        if self.reject_writes.contains(&a) {
            return Err(3);
        }
        if self.ram.contains_key(&a) && !self.hidden.contains(&a) {
            self.ram.insert(a, value);
            if save {
                self.eeprom.insert(a, value);
            }
            Ok(())
        } else if a == 2000 || a == 2001 {
            self.live.insert(a, value);
            if a == 2000 {
                let s1 = self.live.get(&2021).copied().unwrap_or(0);
                self.live.insert(2021, if value == 1 { s1 | 0x0008 } else { s1 & !0x0008 });
            }
            if a == 2001 && value == 1 {
                self.live.insert(2020, self.live.get(&2020).copied().unwrap_or(0) & 0x3FFF);
                for r in 2025..2032 {
                    self.live.insert(r, 0);
                }
            }
            Ok(())
        } else {
            Err(2)
        }
    }

    fn answer(&mut self, req: &Request) -> Result<Vec<u8>, u8> {
        let slave = self.slave;
        match req {
            Request::Read { addr, count } => {
                if *count == 0 || *count > self.max_read {
                    return Err(3);
                }
                let mut body = vec![slave, 0x03, (*count * 2) as u8];
                for k in 0..*count {
                    let v = self.get(addr.wrapping_add(k)).ok_or(2u8)?;
                    body.extend_from_slice(&v.to_be_bytes());
                }
                Ok(body)
            }
            Request::WriteOne { addr, value } => {
                self.write(*addr, *value)?;
                let mut body = vec![slave, 0x06];
                body.extend_from_slice(&addr.to_be_bytes());
                body.extend_from_slice(&value.to_be_bytes());
                Ok(body)
            }
            Request::WriteMany { addr, values } => {
                for (k, v) in values.iter().enumerate() {
                    self.write(addr.wrapping_add(k as u16), *v)?;
                }
                let mut body = vec![slave, 0x10];
                body.extend_from_slice(&addr.to_be_bytes());
                body.extend_from_slice(&(values.len() as u16).to_be_bytes());
                Ok(body)
            }
        }
    }
}

impl Transport for SimDrive {
    fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        let mut s = self.state();
        if data.first() != Some(&s.slave) {
            return Ok(());
        }
        let Some(req) = s.decode(data) else { return Ok(()) };
        s.log.push(req.clone());
        let fault = s.faults.pop_front();
        if fault == Some(Fault::DropReply) {
            return Ok(());
        }
        // An injected exception stands in for the drive refusing, so the request is not applied.
        let answer = match &fault {
            Some(Fault::Exception(code)) => Err(*code),
            _ => s.answer(&req),
        };
        let mut reply = match answer {
            Err(code) => frame(vec![s.slave, req.function() | 0x80, code]),
            Ok(body) => frame(body),
        };
        if fault == Some(Fault::BadCrc) {
            let n = reply.len();
            reply[n - 1] ^= 0x5A;
        }
        if let Some(Fault::JunkBefore(j)) = &fault {
            s.out.extend_from_slice(j);
        }
        s.out.extend_from_slice(&reply);
        if let Some(Fault::JunkAfter(j)) = &fault {
            s.out.extend_from_slice(j);
        }
        Ok(())
    }

    fn read_some(&mut self, buf: &mut [u8], wait: Duration) -> io::Result<usize> {
        let n = {
            let mut s = self.state();
            let n = s.out.len().min(buf.len());
            buf[..n].copy_from_slice(&s.out[..n]);
            s.out.drain(..n);
            n
        };
        if n == 0 {
            std::thread::sleep(wait.min(Duration::from_millis(2)));
        }
        Ok(n)
    }

    fn clear_input(&mut self) -> io::Result<()> {
        // A real port only drops what has already arrived; the sim reply is generated
        // synchronously, so dropping it here matches a reply that arrived late.
        self.state().out.clear();
        Ok(())
    }

    fn char_time(&self) -> Duration {
        Duration::from_micros(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use crate::link::Link;

    fn link(sim: &SimDrive) -> Link {
        let mut l = Link::new(Box::new(sim.clone()), 1);
        l.timeout = Duration::from_millis(30);
        l
    }

    #[test]
    fn read_and_write() {
        let sim = SimDrive::new(Schema::model512());
        let mut l = link(&sim);
        sim.set_param(104, 250);
        assert_eq!(l.read_one(104).unwrap(), 250);
        l.write_one(104, 300).unwrap();
        assert_eq!(l.read_one(104).unwrap(), 300);
        assert_eq!(sim.state().eeprom[&104], 250, "plain write is RAM only");
        sim.power_cycle();
        assert_eq!(l.read_one(104).unwrap(), 250);
        l.write_one(104 | SAVE_BIT, 320).unwrap();
        sim.power_cycle();
        assert_eq!(l.read_one(104).unwrap(), 320);
    }

    #[test]
    fn chunked_reads() {
        let sim = SimDrive::new(Schema::model512());
        sim.state().max_read = 10;
        let mut l = link(&sim);
        l.max_read = 10;
        assert_eq!(l.read(100, 25).unwrap().len(), 25);
        assert_eq!(sim.state().log.len(), 3);
    }

    #[test]
    fn faults_are_retried_or_reported() {
        let sim = SimDrive::new(Schema::model512());
        sim.set_param(5, 77);
        let mut l = link(&sim);
        sim.push_fault(Fault::DropReply);
        assert_eq!(l.read_one(5).unwrap(), 77);
        sim.push_fault(Fault::BadCrc);
        assert_eq!(l.read_one(5).unwrap(), 77);
        sim.push_fault(Fault::JunkBefore(vec![0xFF, 0x01, 0x83]));
        assert_eq!(l.read_one(5).unwrap(), 77);
        sim.push_fault(Fault::JunkAfter(vec![0x01, 0x03]));
        assert_eq!(l.read_one(5).unwrap(), 77);
        assert_eq!(l.read_one(5).unwrap(), 77);
        sim.push_fault(Fault::Exception(3));
        assert!(matches!(l.read_one(5), Err(Error::Exception { code: 3, .. })));
        assert!(matches!(l.read_one(9999), Err(Error::Exception { code: 2, .. })));
        for _ in 0..3 {
            sim.push_fault(Fault::DropReply);
        }
        assert!(matches!(l.read_one(5), Err(Error::Timeout { .. })));
        assert_eq!(l.read_one(5).unwrap(), 77, "recovers after a timeout");
        let st = l.stats;
        assert!(st.timeouts >= 4 && st.crc_errors >= 1 && st.retries >= 4, "{st:?}");
    }

    #[test]
    fn busy_is_retried_once() {
        let sim = SimDrive::new(Schema::model512());
        let mut l = link(&sim);
        sim.push_fault(Fault::Exception(6));
        assert!(l.read_one(5).is_ok());
    }
}
