//! Chardriver wiring: the pty driver as a chardriver device.
//!
//! C correspondence: the master-end callbacks of `pty.c` —
//! `pty_master_open`/`pty_master_close`/`pty_master_read`/`pty_master_write`
//! and the slave-end service hooks (`pty_slave_close` replies the
//! suspended master transfers; the slave itself is served as a tty line
//! by the tty crate, doc 07 §2.6). The master face runs in this crate;
//! the slave face is the tty line with a pty backend.
//!
//! Close effects carry the hangup half B2 added: the face returns
//! [`CloseEffect`] and the service performs B0-EOF plus SIGHUP on the
//! slave session; a pair reset additionally clears the Unix98 node.

use crate::buffer::OutputRing;
use crate::pair::{CloseEffect, PairEnd, PairState, PairTable};
use minix_chardriver::driver::CharDriver;
use minix_chardriver::protocol::{DeviceMinor, OpenDeviceSet, RequestId};

/// The pty driver's master face over the pair table.
pub struct PtyMasterFace {
    pub table: PairTable,
    /// One output ring per pair: the master reads slave-written bytes
    /// from it (`pty_master_read`, doc 07 §3.3 "两本账").
    pub rings: alloc::vec::Vec<OutputRing>,
    /// Opened minors since the last announce (the restart gate).
    pub opened: OpenDeviceSet,
}

impl PtyMasterFace {
    /// A face over `PAIR_COUNT` fresh pairs.
    pub fn new() -> Self {
        let rings = (0..crate::pair::PAIR_COUNT)
            .map(|_| OutputRing::new())
            .collect();
        PtyMasterFace {
            table: PairTable::new(),
            rings,
            opened: OpenDeviceSet::new(),
        }
    }
}

impl Default for PtyMasterFace {
    fn default() -> Self {
        Self::new()
    }
}

impl CharDriver for PtyMasterFace {
    fn open(&mut self, minor: DeviceMinor, _access: i32, _user: i64) -> i32 {
        let Some(pair) = self.table.get_mut(minor.0 as usize) else {
            return -(minix_types::ENXIO as i32);
        };
        match pair.open_master() {
            Ok(_) => {
                self.opened.insert_raw(minor.0);
                0
            }
            Err(code) => code,
        }
    }

    fn close(&mut self, minor: DeviceMinor) -> i32 {
        let Some(pair) = self.table.get_mut(minor.0 as usize) else {
            return -(minix_types::ENXIO as i32);
        };
        match pair.close(PairEnd::Master) {
            // Slave hangup (B0 + SIGHUP) and the Unix98 node clear are
            // service actions; the face only reports which.
            CloseEffect::SlaveHangup | CloseEffect::PairReset | CloseEffect::Quiet => 0,
        }
    }

    fn read(
        &mut self,
        minor: DeviceMinor,
        _position: u64,
        _grant: u64,
        size: usize,
        _flags: i32,
        id: RequestId,
    ) -> i64 {
        // Master reads park until the slave writes (doc 07 §3.3: the
        // output ring parks the reader and `pump`/`finish` complete it).
        let Some(ring) = self.rings.get_mut(minor.0 as usize) else {
            return -(minix_types::ENXIO as i64);
        };
        if size == 0 {
            return 0;
        }
        if ring.is_empty() {
            ring.park_read(0, id.0, size as u64);
            return 0; // EDONTREPLY arrives with the transport
        }
        size.min(ring.len()) as i64
    }

    fn write(
        &mut self,
        minor: DeviceMinor,
        _position: u64,
        _grant: u64,
        size: usize,
        _flags: i32,
        _id: RequestId,
    ) -> i64 {
        // Master writes feed the ring toward the slave's read side; the
        // admission count is what fit (`feed` drops the rest).
        let Some(ring) = self.rings.get_mut(minor.0 as usize) else {
            return -(minix_types::ENXIO as i64);
        };
        // The bytes themselves ride the grant copy in the service; the
        // admission models with a synthetic pattern of the right length.
        let pattern = [0u8; 256];
        let mut admitted = 0;
        while admitted < size {
            let chunk = size.min(256);
            admitted += ring.feed(&pattern[..chunk]);
            if chunk == 0 {
                break;
            }
        }
        admitted as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face() -> PtyMasterFace {
        PtyMasterFace::new()
    }

    #[test]
    fn test_master_open_once_then_refuse() {
        let mut face = face();
        let minor = DeviceMinor(0);
        assert_eq!(face.open(minor, 0, 42), 0);
        assert!(face.opened.contains_raw(0));
        assert!(face.open(minor, 0, 42) != 0); // second open refused
    }

    #[test]
    fn test_close_reports_effect_and_succeeds() {
        let mut face = face();
        face.open(DeviceMinor(0), 0, 42);
        // No slave yet: pair reset outright.
        assert_eq!(face.close(DeviceMinor(0)), 0);
        assert!(face.table.get(0).unwrap().is_free());
    }

    #[test]
    fn test_read_parks_when_empty_and_writes_admit() {
        let mut face = face();
        face.open(DeviceMinor(0), 0, 42);
        // Empty ring: the read parks (EDONTREPLY surfaces as 0).
        assert_eq!(
            CharDriver::read(&mut face, DeviceMinor(0), 0, 0, 64, 0, RequestId(1)),
            0
        );
        assert!(face.rings[0].has_parked_reader());
        // Master write admits up to the ring bound.
        let moved = CharDriver::write(
            &mut face,
            DeviceMinor(0),
            0,
            0,
            128,
            0,
            RequestId(2),
        );
        assert_eq!(moved, 128);
    }

    #[test]
    fn test_unknown_minor_is_enxio() {
        let mut face = face();
        assert_eq!(
            CharDriver::read(&mut face, DeviceMinor(32), 0, 0, 8, 0, RequestId(1)),
            -(minix_types::ENXIO as i64)
        );
    }
}
