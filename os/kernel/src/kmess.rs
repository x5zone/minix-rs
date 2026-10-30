//! Kernel message ring — the Rust rewrite of C `struct kmessages`
//! (minix/include/minix/type.h:170-176) driven by `kputc`
//! (kernel/utility.c:55-90).
//!
//! C contract: every kernel diagnostic character accumulates into a
//! 10000-byte circular buffer; `km_next` is the write cursor, `km_size`
//! the count of valid bytes (capped at the buffer size). The Information
//! Server's `kmessages_dmp` (dmp_kernel.c:60-93) replays the ring from
//! `(km_next - km_size) mod SIZE`.
//!
//! The ring was previously absent: an earlier evolution replaced the C kmess
//! consumption path with direct EarlyConsole output (write-and-discard). This
//! module restores the recordable buffer so `GET_KMESSAGES` (04 §4.1) has a
//! data source, while console output continues unchanged: recording and
//! display run in parallel (C's `kputc` under DEBUG_SERIAL writes both,
//! utility.c:64-75).
//!
//! Concurrency: writes happen with the BKL held (all diagnostic paths in
//! minix-rs run under it, matching C's implicit single-printf-at-a-time).

use core::sync::atomic::{AtomicUsize, Ordering};

use crate::globals::SyncUnsafeCell;
use minix_plat::CurrentEarlyConsole as Console;

// The choke-point type and the trait must come from the same source: call
// sites in the kernel that write `use crate::kmess::{KernelConsole as
// Console, EarlyConsole as _};` obtain the trait through this re-export
// (otherwise every file needs two `use` lines and the indentation fights
// rustfmt). The trait itself still belongs to minix-plat.
pub use minix_plat::EarlyConsole;

/// Ring capacity. C: `_KMESS_BUF_SIZE 10000` — sys_config.h:22.
pub const KMESS_BUF_SIZE: usize = 10000;

/// Wire layout of the `GET_KMESSAGES` payload (04 §4.1):
/// `km_next` (i32) + `km_size` (i32) + the 10000-byte buffer = 10008
/// bytes, copied as one block.
pub const KMESS_SNAPSHOT_SIZE: usize = 8 + KMESS_BUF_SIZE;

/// The kernel message ring (C: `struct kmessages` minus the printable
/// `kmess_buf`/`blpos` scratch fields, which are console-local in C).
#[derive(Debug, Clone, Copy)]
pub struct KmessRing {
    buf: [u8; KMESS_BUF_SIZE],
    next: usize,
    size: usize,
}

impl Default for KmessRing {
    fn default() -> Self {
        Self::new()
    }
}

impl KmessRing {
    /// New empty ring.
    pub const fn new() -> Self {
        Self { buf: [0; KMESS_BUF_SIZE], next: 0, size: 0 }
    }

    /// Append one byte (C: `kputc` body — utility.c:71-75).
    pub fn putc(&mut self, c: u8) {
        self.buf[self.next] = c;
        if self.size < KMESS_BUF_SIZE {
            self.size += 1;
        }
        self.next = (self.next + 1) % KMESS_BUF_SIZE;
    }

    /// Append a string, byte-wise (C drives `kputc` per character from
    /// the printf machinery).
    pub fn write_str(&mut self, s: &str) {
        for &b in s.as_bytes() {
            self.putc(b);
        }
    }

    /// Copy the valid window in stream order into `dst`
    /// (C: `kmessages_dmp` replay — dmp_kernel.c:77-85: start at
    /// `(next - size) mod SIZE`, walk `size` bytes). Returns the number
    /// of bytes written to `dst` (≤ both `size` and `dst.len()`).
    pub fn snapshot_ordered(&self, dst: &mut [u8]) -> usize {
        let start = (self.next + KMESS_BUF_SIZE - self.size) % KMESS_BUF_SIZE;
        let n = self.size.min(dst.len());
        for i in 0..n {
            dst[i] = self.buf[(start + i) % KMESS_BUF_SIZE];
        }
        n
    }

    /// Raw write cursor (C: `km_next`, type.h:171).
    pub const fn next(&self) -> usize {
        self.next
    }

    /// Valid byte count (C: `km_size`, type.h:172).
    pub const fn size(&self) -> usize {
        self.size
    }
}

// ── Global ring + console passthrough ──
//
// Single-CPU build: the ring has one writer (the BSP running kernel
// diagnostic output with the BKL held). Storage follows the
// `globals.rs` `SyncUnsafeCell` pattern; `KmessRing` was added to the
// sealed `BklProtected` approved list — the intended friction point.

static KMESS_RING: SyncUnsafeCell<KmessRing> = SyncUnsafeCell::new(KmessRing::new());

/// Record a diagnostic string into the kernel message ring.
///
/// C: the `kputc` accumulation half of every kernel printf
/// (utility.c:71-75). Errors cannot occur (a ring write is pure memory).
pub fn record_str(s: &str) {
    // SAFETY: kernel diagnostic output runs with the BKL held (or during
    // single-threaded boot), the same precondition every other global
    // accessor in this crate documents.
    let ring = unsafe { &mut *KMESS_RING.get() };
    ring.write_str(s);
}

/// Record a raw byte slice (C: `kputc` per byte).
pub fn record_bytes(bytes: &[u8]) {
    // SAFETY: same BKL contract as `record_str`.
    let ring = unsafe { &mut *KMESS_RING.get() };
    for &b in bytes {
        ring.putc(b);
    }
}

/// Console output with kmess recording (C: `kputc` + the DEBUG_SERIAL
/// console passthrough — utility.c:64-75): feed the ring, then forward
/// to the real console. Recording and display are parallel, never
/// alternative.
pub fn console_write_str(s: &str) {
    // Go through the choke-point type (the [`KernelConsole`] below): it only
    // overrides `write_byte`, so the escaping on the string path (`\n` →
    // `\r\n`) and the expansion on the hex path are recorded in one place —
    // the byte stream in the ring has the same shape as the one on the console.
    <KernelConsole as EarlyConsole>::write_str(s);
}

/// The single choke point for kernel diagnostics (C: `kputc`,
/// `minix3/minix/kernel/utility.c:55-90`).
///
/// **Overriding only `write_byte`** is deliberate: the default bodies of
/// `EarlyConsole`'s `write_str`/`write_hex` ultimately go byte by byte
/// through `Self::write_byte`, so recording at `write_byte` alone means
/// "every byte that appears on the console enters the ring" — including the
/// `\r` inserted before `\n` and the expansion performed by `write_hex`.
/// Recording a second time inside `write_str` would make the ring and the
/// console differ in shape because of the escaping (two sources of truth).
/// The ring had been restored without a feeder attached at one point; this
/// type is that feeder: every diagnostic-printing `Console` alias in the
/// kernel points at it.
pub struct KernelConsole;

impl EarlyConsole for KernelConsole {
    fn init() {
        Console::init();
    }

    fn write_byte(byte: u8) {
        record_bytes(core::slice::from_ref(&byte));
        Console::write_byte(byte);
    }
}

/// Snapshot the valid window in stream order into `dst`
/// (C: `kmessages_dmp` replay — dmp_kernel.c:77-85: start at
/// `(next - size) mod SIZE`, walk `size` bytes). Returns
/// `(km_next, km_size, moved)`.
pub fn snapshot_ordered(dst: &mut [u8]) -> (usize, usize, usize) {
    // SAFETY: same BKL/single-writer contract as `record_str`.
    let ring = unsafe { &mut *KMESS_RING.get() };
    let next = ring.next();
    let size = ring.size();
    let moved = ring.snapshot_ordered(dst);
    (next, size, moved)
}

/// Read the cursors (the metadata half of a snapshot copy). C: `km_next`/`km_size`.
pub fn cursor() -> (usize, usize) {
    // SAFETY: same BKL/single-writer contract as `record_str`.
    let ring = unsafe { &mut *KMESS_RING.get() };
    (ring.next(), ring.size())
}

/// Copy the `GET_KMESSAGES` snapshot (the km_next/km_size header plus the
/// ring body expanded in stream order, [`KMESS_SNAPSHOT_SIZE`] bytes in
/// total) into the caller's buffer. C's do_getinfo kmess arm does not exist
/// on 32-bit systems; this is the LP64 arm added by design.
///
/// `val_len < snapshot size` → E2BIG (the caller's buffer is too small; the
/// same semantics C do_getinfo applies to the whole-table arms). The snapshot
/// is staged out through one kernel-heap copy (ring order → stream order)
/// rather than a large kernel-stack buffer.
pub fn copy_snapshot_to_caller(
    caller_nr: crate::proc::ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    val_ptr: u64,
    val_len: i32,
) -> crate::syscall::KcallResult {
    use crate::cross_space::data_copy_vmcheck;
    use crate::syscall::KcallResult;
    use crate::vm::{AddressRef, CrossSpaceResult};
    use minix_types::VirBytes;

    const E2BIG: i32 = 7;
    const EFAULT: i32 = 14;

    if (val_len as i64) >= 0 && (val_len as i64) < KMESS_SNAPSHOT_SIZE as i64 {
        return KcallResult::Ok(E2BIG);
    }

    // Expand the snapshot in stream order (staged through one kernel-heap
    // cross-space copy; ring → stream expansion matches C kmessages_dmp's
    // print_buf assembly, dmp_kernel.c:77-85).
    let mut snap = alloc::vec![0u8; KMESS_SNAPSHOT_SIZE];
    let (next, size, _) = snapshot_ordered(&mut snap[8..]);
    snap[..4].copy_from_slice(&(next as i32).to_le_bytes());
    snap[4..8].copy_from_slice(&(size as i32).to_le_bytes());

    // SAFETY: the snapshot buffer lives on the kernel heap (`IMAGE_HEAP`,
    // `.bss`, in the image's higher-half region outside both Direct Map
    // windows) — `virt_to_phys` would produce a bogus physical address. Use
    // AddressRef::Process instead (the caller CR3 maps the kernel
    // higher-half, so resolve_physical walks the real page tables and gets
    // the correct PA). The copy into the caller's buffer is a single
    // cross-space copy; the caller's cr3 is resolved by proc_cr3 (same as
    // dispatch_trace).
    let caller_endpt = proc_table
        .get(caller_nr)
        .map(|p| p.p_endpoint)
        .expect("copy_snapshot_to_caller: caller slot must exist");
    let caller_cr3 = proc_table
        .get(caller_nr)
        .map(|p| p.p_seg.phys_root)
        .expect("copy_snapshot_to_caller: caller slot must exist");
    let proc_cr3 = |_pt: &crate::proc_table::ProcessTable, endpt: minix_types::Endpoint| {
        if endpt == caller_endpt { Some(caller_cr3) } else { None }
    };
    let r = data_copy_vmcheck(
        caller_nr,
        proc_table,
        AddressRef::Process { endpoint: caller_endpt, offset: VirBytes(snap.as_ptr() as u64) },
        AddressRef::Process { endpoint: caller_endpt, offset: VirBytes(val_ptr) },
        KMESS_SNAPSHOT_SIZE,
        &proc_cr3,
    );
    if let crate::vm::CrossSpaceResult::Completed(Err(_)) = r {
        return KcallResult::Ok(crate::errno::EFAULT);
    }
    if matches!(r, crate::vm::CrossSpaceResult::Suspended(_)) {
        return KcallResult::VmSuspend;
    }
    KcallResult::Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// C utility.c:71-75 — sequential writes do not wrap: size grows to the
    /// amount written, next advances linearly.
    #[test]
    fn test_putc_sequential_accumulates() {
        let mut ring = KmessRing::new();
        for b in b"kernel boot ok" {
            ring.putc(*b);
        }
        assert_eq!(ring.size(), 14);
        assert_eq!(ring.next(), 14);
        let mut dst = [0u8; 64];
        let moved = ring.snapshot_ordered(&mut dst);
        assert_eq!((ring.next(), ring.size(), moved), (14, 14, 14));
        assert_eq!(&dst[..14], b"kernel boot ok");
    }

    /// Wraparound semantics (C kputc's % _KMESS_BUF_SIZE): capacity 10000,
    /// so after writing 10005 bytes size caps at 10000 and next=5; the
    /// snapshot starts from the oldest byte at (next-size)%SIZE.
    #[test]
    fn test_putc_wraps_and_snapshot_reorders() {
        let mut ring = KmessRing::new();
        // Write 10005 distinct bytes (modulo marker).
        for i in 0..10005u64 {
            ring.putc((i % 251) as u8);
        }
        assert_eq!(ring.size(), 10000);
        assert_eq!(ring.next(), 5);
        let mut dst = [0u8; 10000];
        let moved = ring.snapshot_ordered(&mut dst);
        assert_eq!((ring.next(), ring.size(), moved), (5, 10000, 10000));
        // Expansion order: dst[0] must hold the value of write-order byte 5.
        assert_eq!(dst[0], (5 % 251) as u8);
        assert_eq!(dst[9999], (10004 % 251) as u8);
    }

    /// Snapshot size contract: the GET_KMESSAGES reply body is an 8-byte
    /// header plus 10000 bytes.
    #[test]
    fn test_kmess_snapshot_size_constant() {
        assert_eq!(KMESS_SNAPSHOT_SIZE, 10008);
        assert_eq!(KMESS_BUF_SIZE, 10000); // sys_config.h:22
    }

    /// Feeder invariant (a regression pin for "the message ring's character
    /// write path had no feeder").
    ///
    /// The choke-point type overrides only `write_byte`: the default bodies
    /// of `write_str`/`write_hex` go byte by byte through `Self::write_byte`,
    /// so "every byte seen on the console enters the ring" holds in one
    /// place (including the `\r` inserted before `\n` and hex expansion).
    /// Recording a second time inside `write_str` would make the ring and
    /// the console differ in shape (two sources of truth), so this pin also
    /// forbids that form.
    #[test]
    fn test_kernel_console_is_the_single_choke_point() {
        let src = include_str!("kmess.rs");
        let at = src
            .find("impl EarlyConsole for KernelConsole {")
            .expect("收口类型的 impl 存在");
        let body = &src[at..(src.len().min(at + 600))];
        assert!(
            body.contains("fn write_byte(") && body.contains("record_bytes("),
            "收口必须在 write_byte 处记录；现体：\n{body}"
        );
        assert!(
            !body.contains("fn write_str(") && !body.contains("fn write_hex("),
            "不得在 write_str/write_hex 里另记一次（会与 `\\r` 转义不同形）；现体：\n{body}"
        );
    }
}
