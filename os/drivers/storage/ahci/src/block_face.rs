//! Blockdriver wiring: the AHCI HBA's command-slot discipline behind the
//! blockdriver transfer hook.
//!
//! C correspondence: the AHCI driver's block transfer path — commands
//! issue into one of 32 slots (`port_start`/`ahci.c:1254` area), timeout
//! locks the port and reset reopens it (`port_timeout`, `ahci.c:1715`),
//! and the blockdriver entry answers how many bytes were admitted.
//!
//! The face here is the admission half: it refuses transfers while the
//! port is not running (after reset), accepts into the slot discipline,
//! and answers the byte count. Descriptor-table assembly (command FIS,
//! PRDT) and register traffic are the service's `minix-virtio`-style
//! transport job.

use minix_blockdriver::driver::BlockDriver;
use minix_blockdriver::protocol::{DeviceExtent, DeviceMinor};
use minix_chardriver::protocol::OpenDeviceSet;

use crate::port::Port;

/// The AHCI driver's block face over one port.
pub struct AhciFace {
    pub port: Port,
    /// Sector capacity reported for the minor.
    pub sectors: u64,
    /// Opened minors since the last announce (the restart gate).
    pub opened: OpenDeviceSet,
}

/// Bytes per ATA sector (`at_wini.h`/AHCI contract).
const SECTOR: u64 = 512;

impl AhciFace {
    /// A face over a running port with the given capacity.
    pub fn new(sectors: u64) -> Self {
        let mut face = AhciFace {
            port: Port::new(),
            sectors,
            opened: OpenDeviceSet::new(),
        };
        face.port.start();
        face
    }
}

impl BlockDriver for AhciFace {
    fn open(&mut self, minor: DeviceMinor, _access: i32) -> i32 {
        self.opened.insert_raw(minor.0);
        0
    }

    fn transfer(
        &mut self,
        _minor: DeviceMinor,
        _do_write: bool,
        position: u64,
        count: u64,
        _flags: i32,
        _id: minix_blockdriver::protocol::RequestId,
        _vectored: bool,
    ) -> i64 {
        let capacity = self.sectors * SECTOR;
        if position >= capacity {
            return 0;
        }
        let room = capacity - position;
        // Slot admission: the port takes the command only when a slot is
        // free; otherwise the caller retries (the queue policy lives in
        // the service's pending queue).
        match self.port.issue() {
            crate::port::IssueOutcome::Accepted(slot) => {
                let _ = slot;
                (count.min(room)) as i64
            }
            crate::port::IssueOutcome::NoSlot | crate::port::IssueOutcome::NotStarted => {
                -(minix_types::EAGAIN as i64)
            }
        }
    }

    fn partition(&mut self, _minor: DeviceMinor) -> Option<DeviceExtent> {
        Some(DeviceExtent { base: 0, size: self.sectors * SECTOR })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face() -> AhciFace {
        AhciFace::new(1024) // 512 KiB
    }

    #[test]
    fn test_transfer_admits_within_capacity() {
        let mut face = face();
        assert_eq!(
            BlockDriver::transfer(
                &mut face,
                DeviceMinor(0),
                true,
                0,
                512,
                0,
                minix_blockdriver::protocol::RequestId(1),
                false
            ),
            512
        );
    }

    #[test]
    fn test_transfer_clips_at_capacity() {
        let mut face = face();
        assert_eq!(
            BlockDriver::transfer(
                &mut face,
                DeviceMinor(0),
                true,
                1023 * 512,
                1024,
                0,
                minix_blockdriver::protocol::RequestId(1),
                false
            ),
            512
        );
        assert_eq!(
            BlockDriver::transfer(
                &mut face,
                DeviceMinor(0),
                true,
                1024 * 512,
                512,
                0,
                minix_blockdriver::protocol::RequestId(1),
                false
            ),
            0
        );
    }

    #[test]
    fn test_busy_port_answers_eagain() {
        let mut face = face();
        // Fill all 32 slots; the next command answers "try again".
        for _ in 0..32 {
            assert!(matches!(
                face.port.issue(),
                crate::port::IssueOutcome::Accepted(_)
            ));
        }
        assert_eq!(
            BlockDriver::transfer(
                &mut face,
                DeviceMinor(0),
                true,
                0,
                512,
                0,
                minix_blockdriver::protocol::RequestId(1),
                false
            ),
            -(minix_types::EAGAIN as i64)
        );
    }

    #[test]
    fn test_reset_port_answers_eagain_until_restart() {
        let mut face = face();
        face.port.note_timeout();
        face.port.reset();
        assert_eq!(
            BlockDriver::transfer(
                &mut face,
                DeviceMinor(0),
                true,
                0,
                512,
                0,
                minix_blockdriver::protocol::RequestId(1),
                false
            ),
            -(minix_types::EAGAIN as i64)
        );
        face.port.start();
        assert_eq!(
            BlockDriver::transfer(
                &mut face,
                DeviceMinor(0),
                true,
                0,
                512,
                0,
                minix_blockdriver::protocol::RequestId(1),
                false
            ),
            512
        );
    }
}
