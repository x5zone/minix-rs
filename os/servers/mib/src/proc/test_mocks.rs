//! Shared test doubles for the process-format assemblies (17/18/19).
//!
//! `FakeKernel` serves the pulled tables and records the copy-out sink
//! (address = sink offset, the tests' convention); `FakeServices` answers
//! PM's `SI_PROC_TAB` and refuses VFS's light table — the A-7 degrade lane
//! every assembly must survive. Test-only: never linked into the server.

#![cfg(test)]

use alloc::vec::Vec;
use core::cell::RefCell;
use minix_types::Endpoint;

use crate::io::relay::{RelayDir, RemoteCall, RemoteReplyWire};
use crate::transport::{MibKernel, MibServices};

/// Kernel-side double: whole-table `getproctab`, fixed clock, sink recorder.
pub(crate) struct FakeKernel {
    /// Whole GET_PROCTAB image (rows × [`minix_types::ProcInfoStruct`]).
    pub proctab: Vec<u8>,
    /// `getticks` answer.
    pub ticks: u64,
    /// `hz` answer.
    pub hz: u32,
    /// `boottime` answer (epoch seconds).
    pub boot: u64,
    /// The caller sink: `datacopy_to(addr)` lands at this offset.
    pub sink: RefCell<Vec<u8>>,
    /// 目标进程内存像（KERN_PROC_ARGS 的取页源）：`target_base` 起的
    /// 线性字节；`datacopy_from` 越界即 EFAULT（与 C 的失败路径同形）。
    pub target_base: u64,
    pub target_mem: Vec<u8>,
}

impl MibKernel for FakeKernel {
    fn datacopy_from(&mut self, _s: Endpoint, a: u64, b: &mut [u8]) -> Result<(), i32> {
        let at = (a - self.target_base) as usize;
        if a < self.target_base || at + b.len() > self.target_mem.len() {
            return Err(minix_types::EFAULT);
        }
        b.copy_from_slice(&self.target_mem[at..at + b.len()]);
        Ok(())
    }

    fn datacopy_to(&mut self, _d: Endpoint, a: u64, b: &[u8]) -> Result<(), i32> {
        let at = a as usize;
        let mut sink = self.sink.borrow_mut();
        if sink.len() < at + b.len() {
            sink.resize(at + b.len(), 0);
        }
        sink[at..at + b.len()].copy_from_slice(b);
        Ok(())
    }

    fn grant_magic(&mut self, _: Endpoint, _: u64, _: u64, _: RelayDir) -> Result<minix_types::GrantId, i32> {
        Ok(1)
    }

    fn grant_revoke(&mut self, _: minix_types::GrantId) {}

    fn getproctab(&mut self, buf: &mut [u8]) -> Result<(), i32> {
        let n = buf.len().min(self.proctab.len());
        buf[..n].copy_from_slice(&self.proctab[..n]);
        Ok(())
    }

    fn getticks(&mut self) -> Result<u64, i32> {
        Ok(self.ticks)
    }

    fn hz(&mut self) -> Result<u32, i32> {
        Ok(self.hz)
    }

    fn boottime(&mut self) -> Result<u64, i32> {
        Ok(self.boot)
    }
}

/// Peer-side double: PM answers `SI_PROC_TAB`, VFS stays dark (A-7).
pub(crate) struct FakeServices {
    /// Whole `SI_PROC_TAB` image (rows × [`minix_types::MProcSnap`]).
    pub pm_tab: Vec<u8>,
    /// Whole `SI_PROCLIGHT_TAB` image (rows × 16 B)——C-22 后半的 VFS
    /// light 行；测试里缺省给空表（A-7 降级路径）。
    pub light_tab: Vec<u8>,
}

impl MibServices for FakeServices {
    fn getnuid(&mut self, _: Endpoint) -> Result<u32, i32> {
        Ok(0)
    }

    fn getsysinfo(&mut self, target: Endpoint, what: i32, buf: &mut [u8]) -> Result<(), i32> {
        if target == Endpoint::VFS {
            if what != minix_types::SI_PROCLIGHT_TAB {
                return Err(minix_types::EINVAL);
            }
            // C-22 后半：light 表按需服务（空表仍在=生产者缺席的降级面）。
            let n = buf.len().min(self.light_tab.len());
            buf[..n].copy_from_slice(&self.light_tab[..n]);
            return Ok(());
        }
        assert_eq!((target, what), (Endpoint::PM, minix_types::SI_PROC_TAB));
        let n = buf.len().min(self.pm_tab.len());
        buf[..n].copy_from_slice(&self.pm_tab[..n]);
        Ok(())
    }

    fn ds_retrieve_label_name(&mut self, _: Endpoint, _: &mut [u8]) -> Result<usize, i32> {
        Err(minix_types::EIO)
    }

    fn remote_info(&mut self, _: Endpoint, _: &mut [u8], _: &mut [u8]) -> Result<(), i32> {
        Err(minix_types::EIO)
    }

    fn remote_call(
        &mut self,
        _: Endpoint,
        _: RemoteCall,
        _: &mut RemoteReplyWire,
    ) -> Result<(), i32> {
        Err(minix_types::EIO)
    }

    fn vm_info(&mut self, _: i32, _: Endpoint, _: &mut [u8]) -> Result<(), i32> {
        Err(minix_types::EIO) // VM usage 生产者未接（C 忽略失败，语义不变）
    }

    fn pm_getparam(&mut self, _: i32, _: &mut [u8]) -> Result<(), i32> {
        Err(minix_types::EIO)
    }
}

/// NUL-terminated name into a 16-byte `mp_name`/`p_name` lane.
pub(crate) fn name_bytes(name: &str) -> [u8; 16] {
    let mut n = [0u8; 16];
    let b = name.as_bytes();
    n[..b.len()].copy_from_slice(b);
    n
}

/// Write one typed row into a whole-table image. `T` must be the row type
/// the producer ships (`ProcInfoStruct`/`MProcSnap`).
pub(crate) fn put_row<T: Copy>(table: &mut Vec<u8>, kslot: usize, row: &T) {
    let row_sz = core::mem::size_of::<T>();
    // SAFETY(test): `T` is a `#[repr(C)]` POD snapshot row; the slot window
    // is in bounds by construction.
    unsafe {
        core::ptr::write_unaligned(table.as_mut_ptr().add(kslot * row_sz) as *mut T, *row);
    }
}
