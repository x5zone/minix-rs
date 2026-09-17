//! Character-face wiring: the memory driver as a chardriver device.
//!
//! C correspondence: `m_char_open`/`m_char_close`/`m_char_read`/
//! `m_char_write` (`memory.c:295-410`) riding `audio_tab`-style inside
//! the chardriver table (`memory.c:64` registers the char face).
//!
//! The data-movement split: hooks answer COUNTS from the plans
//! (`transfer::char_read_plan`/`char_write_plan`); the physical copies
//! through the backend or the zero-fill into the caller's grant are the
//! service's job (plan-guided, transport-executed). This keeps the hook
//! half testable with a plain backend and no transport at all.
//!
//! Block face (ram/imgrd minors) wires to `minix-blockdriver` in its own
//! face module; the dispatch between faces (char request vs block
//! request on the same driver process) is the service's first gate.

use minix_chardriver::driver::CharDriver;
use minix_chardriver::protocol::{DeviceMinor, OpenDeviceSet};

use crate::device::DeviceTable;
use crate::transfer::{char_read_plan, char_write_plan, CharReadPlan};

/// Backing storage behind the RAM-disk and image minors.
pub trait MemBackend {
    /// Read `buf.len()` bytes of device memory at `offset`.
    fn read_phys(&mut self, offset: u64, buf: &mut [u8]);
    /// Write `buf` bytes of device memory at `offset`.
    fn write_phys(&mut self, offset: u64, buf: &[u8]);
}

/// A simple byte-vector backend (tests and small images).
#[derive(Debug, Default, Clone)]
pub struct VecBackend(pub alloc::vec::Vec<u8>);

impl MemBackend for VecBackend {
    fn read_phys(&mut self, offset: u64, buf: &mut [u8]) {
        let start = offset as usize;
        if let Some(slice) = self.0.get_mut(start..start + buf.len()) {
            buf.copy_from_slice(slice);
        }
    }

    fn write_phys(&mut self, offset: u64, buf: &[u8]) {
        let start = offset as usize;
        if let Some(slice) = self.0.get_mut(start..start + buf.len()) {
            slice.copy_from_slice(buf);
        }
    }
}

/// The memory driver's character face over a device table and backend.
pub struct MemoryChar<B: MemBackend> {
    pub table: DeviceTable,
    pub backend: B,
    /// Opened minors since the last announce (the restart gate).
    pub opened: OpenDeviceSet,
}

impl<B: MemBackend> MemoryChar<B> {
    /// Fresh face over the given backend.
    pub fn new(backend: B) -> Self {
        MemoryChar {
            table: DeviceTable::fresh(),
            backend,
            opened: OpenDeviceSet::new(),
        }
    }
}

impl<B: MemBackend> CharDriver for MemoryChar<B> {
    fn open(&mut self, minor: DeviceMinor, _access: i32, _user: i64) -> i32 {
        let result = self.table.open(minor.0, false);
        if result == 0 {
            self.opened.insert_raw(minor.0);
        }
        result
    }

    fn close(&mut self, minor: DeviceMinor) -> i32 {
        self.table.close(minor.0, false)
    }

    fn read(
        &mut self,
        minor: DeviceMinor,
        position: u64,
        _grant: u64,
        size: usize,
        _flags: i32,
        _id: minix_chardriver::protocol::RequestId,
    ) -> i64 {
        let Some(extent) = self.table.extent(minor.0) else {
            return -(no_such_device() as i64);
        };
        match char_read_plan(minor.0, position, size as u64, extent) {
            Ok(CharReadPlan::Eof) => 0,
            Ok(CharReadPlan::ZeroFill(want)) => want as i64,
            Ok(CharReadPlan::Backed(count)) | Ok(CharReadPlan::PageWindow(count)) => count as i64,
            Err(code) => code as i64,
        }
    }

    fn write(
        &mut self,
        minor: DeviceMinor,
        position: u64,
        _grant: u64,
        size: usize,
        _flags: i32,
        _id: minix_chardriver::protocol::RequestId,
    ) -> i64 {
        let Some(extent) = self.table.extent(minor.0) else {
            return -(no_such_device() as i64);
        };
        match char_write_plan(minor.0, position, size as u64, extent) {
            // Null and zero swallow everything (the plan decides which).
            Ok(plan @ crate::transfer::CharWritePlan::Sink(want)) => {
                let _ = plan;
                want as i64
            }
            Ok(crate::transfer::CharWritePlan::Backed(count))
            | Ok(crate::transfer::CharWritePlan::PageWindow(count)) => count as i64,
            Err(code) => code as i64,
        }
    }
}

const fn no_such_device() -> i32 {
    minix_types::ENXIO
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face() -> MemoryChar<VecBackend> {
        let mut storage = alloc::vec::Vec::new();
        storage.resize(16, 0xAA);
        MemoryChar::new(VecBackend(storage))
    }

    #[test]
    fn test_char_open_close_flow_through_the_table() {
        let mut face = face();
        let minor = DeviceMinor(1); // /dev/mem, character face
        assert_eq!(face.open(minor, 0, 42), 0);
        assert_eq!(face.table.open_count(1), 1);
        assert!(face.opened.contains_raw(1));
        assert_eq!(face.close(minor), 0);
        assert_eq!(face.table.open_count(1), 0);
    }

    #[test]
    fn test_zero_read_answers_full_length() {
        let mut face = face();
        face.open(DeviceMinor(5), 0, 42);
        let moved = CharDriver::read(
            &mut face,
            DeviceMinor(5),
            0,
            0,
            128,
            0,
            minix_chardriver::protocol::RequestId(1),
        );
        assert_eq!(moved, 128);
    }

    #[test]
    fn test_null_read_answers_eof() {
        let mut face = face();
        face.open(DeviceMinor(3), 0, 42);
        assert_eq!(
            CharDriver::read(
                &mut face,
                DeviceMinor(3),
                0,
                0,
                128,
                0,
                minix_chardriver::protocol::RequestId(1)
            ),
            0
        );
    }
}
