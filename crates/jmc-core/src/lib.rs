//! Core library of the JMC Servo Tool: Modbus RTU to JMC JAND/JASD servo drives (model 512),
//! the parameter table, JMC's XML export format, snapshots, the guarded write sequence and a
//! simulated drive.

pub mod docs;
pub mod drive;
pub mod error;
pub mod guard;
pub mod jmcxml;
pub mod link;
pub mod model;
pub mod monitor;
pub mod params;
pub mod rtu;
pub mod sim;
pub mod snapshot;
#[cfg(test)]
pub(crate) mod ste;

pub use drive::{Drive, Persist, WriteOptions, WriteOutcome};
pub use error::{Error, Result};
pub use link::{Framing, Link, SerialSettings};
pub use params::{ParamDef, Schema};
pub use snapshot::Snapshot;
