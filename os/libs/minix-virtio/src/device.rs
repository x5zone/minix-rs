//! Device life cycle: setup, queues, ready, reset, kick policy.
//!
//! C correspondence: `virtio_setup_device`, `virtio_alloc_queues`,
//! `virtio_device_ready`, `virtio_reset_device`, `virtio_free_device`
//! (`virtio.c`), the status bytes `VIRTIO_STATUS_ACK/DRV/DRV_OK/FAIL`
//! (`virtio.h`), and the kick rule `wants_kick`
//! (`virtio.c:766-783`).
//!
//! Register reads and writes stay in the service crate behind the
//! [`IoPort`] trait; this module owns the order (acknowledge, driver,
//! ready) and the kick policy.

/// Status byte: guest recognized the device.
///
/// C: `VIRTIO_STATUS_ACK 0x01` (`virtio.h`).
pub const STATUS_ACK: u8 = 0x01;
/// Status byte: guest knows how to drive it.
///
/// C: `VIRTIO_STATUS_DRV 0x02` (`virtio.h`).
pub const STATUS_DRIVER: u8 = 0x02;
/// Status byte: driver is ready.
///
/// C: `VIRTIO_STATUS_DRV_OK 0x04` (`virtio.h`).
pub const STATUS_READY: u8 = 0x04;
/// Status byte: something failed.
///
/// C: `VIRTIO_STATUS_FAIL 0x80` (`virtio.h`).
pub const STATUS_FAILED: u8 = 0x80;

/// Device stage in the setup order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Nothing done yet.
    Fresh,
    /// Acknowledged and driver-known, queues missing.
    Known,
    /// Queues allocated, not yet ready.
    Queued,
    /// Ready: interrupts on, host may run.
    Ready,
    /// Failed: reset required before reuse.
    Failed,
}

/// Life-cycle policy for one virtual device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceLife {
    stage: Stage,
    status: u8,
}

impl DeviceLife {
    /// Fresh device (no status bits).
    pub const fn new() -> DeviceLife {
        DeviceLife {
            stage: Stage::Fresh,
            status: 0,
        }
    }

    /// Current stage.
    pub const fn stage(self) -> Stage {
        self.stage
    }

    /// Status byte to write (accumulates through setup).
    pub const fn status(self) -> u8 {
        self.status
    }

    /// Acknowledge the device and declare driver knowledge.
    ///
    /// C: the status writes at the head of `virtio_setup_device`.
    pub fn acknowledge(&mut self) {
        if self.stage == Stage::Fresh {
            self.status |= STATUS_ACK | STATUS_DRIVER;
            self.stage = Stage::Known;
        }
    }

    /// Note queues allocated (sizing already validated by the caller).
    pub fn note_queues(&mut self) {
        if self.stage == Stage::Known {
            self.stage = Stage::Queued;
        }
    }

    /// Mark ready (interrupts enabled by the service crate next).
    ///
    /// C: `virtio_device_ready` (`virtio.c:340-351`).
    pub fn mark_ready(&mut self) {
        if self.stage == Stage::Queued {
            self.status |= STATUS_READY;
            self.stage = Stage::Ready;
        }
    }

    /// Mark failed (reset required).
    pub fn mark_failed(&mut self) {
        self.status |= STATUS_FAILED;
        self.stage = Stage::Failed;
    }

    /// Reset to fresh (clears status, like the device reset).
    ///
    /// C: `virtio_reset_device` (`virtio.c:742-748`).
    pub fn reset(&mut self) {
        self.status = 0;
        self.stage = Stage::Fresh;
    }
}

impl Default for DeviceLife {
    fn default() -> Self {
        DeviceLife::new()
    }
}

/// Kick policy: ring the host bell only when it helps.
///
/// C: `wants_kick` (`virtio.c:766-783`): kick when the host asked for
/// notification (no-notify clear) or when the queue ran out of room
/// (the host might be waiting for space). Pure function of three facts.
pub const fn wants_kick(no_notify: bool, queue_full: bool) -> bool {
    !no_notify || queue_full
}

/// Port and register access behind one trait: the service crate talks to
/// PCI ports or memory-mapped registers; tests use the vector port below.
///
pub trait IoPort {
    /// Read one byte at this offset.
    fn read_byte(&mut self, offset: u16) -> u8;
    /// Write one byte at this offset.
    fn write_byte(&mut self, offset: u16, value: u8);
}

/// Null port: reads zero, writes vanish (no hardware wired).
#[derive(Debug, Default, Clone, Copy)]
pub struct NullPort;

impl IoPort for NullPort {
    fn read_byte(&mut self, _offset: u16) -> u8 {
        0
    }

    fn write_byte(&mut self, _offset: u16, _value: u8) {}
}

/// Vector port for tests: 256 byte cells with a write log.
#[derive(Debug, Clone)]
pub struct VecPort {
    cells: [u8; 256],
    writes: alloc::vec::Vec<(u16, u8)>,
}

impl VecPort {
    /// Fresh port (all cells zero).
    pub fn new() -> VecPort {
        VecPort {
            cells: [0; 256],
            writes: alloc::vec::Vec::new(),
        }
    }

    /// Write log (test inspection).
    pub fn writes(&self) -> &[(u16, u8)] {
        &self.writes
    }
}

impl Default for VecPort {
    fn default() -> Self {
        VecPort::new()
    }
}

impl IoPort for VecPort {
    fn read_byte(&mut self, offset: u16) -> u8 {
        self.cells[offset as usize % 256]
    }

    fn write_byte(&mut self, offset: u16, value: u8) {
        self.cells[offset as usize % 256] = value;
        self.writes.push((offset, value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifecycle_walks_acknowledge_queue_ready() {
        let mut life = DeviceLife::new();
        life.acknowledge();
        assert_eq!(life.stage(), Stage::Known);
        assert_eq!(life.status(), STATUS_ACK | STATUS_DRIVER);
        life.note_queues();
        life.mark_ready();
        assert_eq!(life.stage(), Stage::Ready);
        assert_eq!(life.status() & STATUS_READY, STATUS_READY);
        life.reset();
        assert_eq!(life.stage(), Stage::Fresh);
        assert_eq!(life.status(), 0);
    }

    #[test]
    fn test_ready_out_of_order_is_refused() {
        let mut life = DeviceLife::new();
        life.mark_ready();
        assert_eq!(life.stage(), Stage::Fresh);
        life.mark_failed();
        assert_eq!(life.stage(), Stage::Failed);
    }

    #[test]
    fn test_kick_policy() {
        assert!(wants_kick(false, false));
        assert!(!wants_kick(true, false));
        assert!(wants_kick(true, true));
    }

    #[test]
    fn test_vector_port_records_writes() {
        let mut port = VecPort::new();
        port.write_byte(0x12, 0x04);
        assert_eq!(port.read_byte(0x12), 0x04);
        assert_eq!(port.writes(), &[(0x12, 0x04)]);
        let mut null = NullPort;
        assert_eq!(null.read_byte(0), 0);
    }

    #[test]
    fn test_status_constants_match_header() {
        assert_eq!(STATUS_ACK, 0x01);
        assert_eq!(STATUS_DRIVER, 0x02);
        assert_eq!(STATUS_READY, 0x04);
        assert_eq!(STATUS_FAILED, 0x80);
    }
}
