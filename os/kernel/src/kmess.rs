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
//! E-ISKMESS (A-3): the ring was previously absent — the W-7 evolution
//! replaced the C kmess consumption path with direct EarlyConsole output
//! (write-and-discard). This module restores the recordable buffer so
//! `GET_KMESSAGES` (04 §4.1) has a data source, while console output
//! continues unchanged: recording and display run in parallel (C's
//! `kputc` under DEBUG_SERIAL writes both, utility.c:64-75).
//!
//! Concurrency: writes happen with the BKL held (all diagnostic paths in
//! minix-rs run under it, matching C's implicit single-printf-at-a-time).

use core::sync::atomic::{AtomicUsize, Ordering};

use crate::globals::SyncUnsafeCell;
use minix_plat::CurrentEarlyConsole as Console;

// 收口与 trait 必须同一来源：内核里那些 `use crate::kmess::{KernelConsole as
// Console, EarlyConsole as _};` 的调用点靠这条 re-export 拿到 trait（否则每个
// 文件要写两条 use，缩进与 rustfmt 打架）。trait 本体仍归 minix-plat。
pub use minix_plat::EarlyConsole;

/// Ring capacity. C: `_KMESS_BUF_SIZE 10000` — sys_config.h:22.
pub const KMESS_BUF_SIZE: usize = 10000;

/// Wire layout of the `GET_KMESSAGES` payload (E-ISKMESS, 04 §4.1):
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
    // 统一走收口类型（下方 [`KernelConsole`]）：它只覆写 `write_byte`，
    // 因此字符串路径上的转义（`\n` → `\r\n`）与十六进制路径上的展开
    // 都被同一处记录——环里存的字节串与控制台看到的字节串同形。
    <KernelConsole as EarlyConsole>::write_str(s);
}

/// 内核诊断的唯一收口（C: `kputc`，`minix3/minix/kernel/utility.c:55-90）。
///
/// **只覆写 `write_byte`** 是刻意的：`EarlyConsole` 的 `write_str`/`write_hex`
/// 默认体最终逐字节走 `Self::write_byte`，所以在 `write_byte` 这一处记录就
/// 等于「控制台上出现的每个字节都进环」——包括 `\n` 前插的 `\r` 与 `write_hex`
/// 的展开。若在 `write_str` 里另记一次，环与控制台就会因转义而不同形（两个
/// 真相）。E-ISKMESS 当时恢复了环却没接喂入者（台账 §8 那条），本类型就是
/// 那个喂入者：内核里所有诊断打印的 `Console` 别名都指向它。
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

/// 读取游标(快照拷贝的元数据半)。C: `km_next`/`km_size`。
pub fn cursor() -> (usize, usize) {
    // SAFETY: same BKL/single-writer contract as `record_str`.
    let ring = unsafe { &mut *KMESS_RING.get() };
    (ring.next(), ring.size())
}

/// 把 `GET_KMESSAGES` 快照(km_next/km_size 头 + 顺序展开的 ring 体,
/// 共 [`KMESS_SNAPSHOT_SIZE`] 字节)拷到调用方缓冲。C 的 do_getinfo
/// kmess 臂在 32 位下不存在(04 §2.1);A-3 决策的 LP64 新臂。
///
/// `val_len < 快照大小` → E2BIG(调用方缓冲不足,C do_getinfo 对整表类
/// 臂的同款语义)。快照经内核堆中转一次拷出(ring 环形 → 顺序流),
/// 不占内核栈大缓冲。
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

    // 顺序展开快照(kernel 堆中转一次跨空间拷;环形→流式展开与 C
    // kmessages_dmp 的 print_buf 组装等价,dmp_kernel.c:77-85)。
    let mut snap = alloc::vec![0u8; KMESS_SNAPSHOT_SIZE];
    let (next, size, _) = snapshot_ordered(&mut snap[8..]);
    snap[..4].copy_from_slice(&(next as i32).to_le_bytes());
    snap[4..8].copy_from_slice(&(size as i32).to_le_bytes());

    // SAFETY: 快照缓冲在内核堆 `IMAGE_HEAP`(`.bss`,落 image 高半区，两个
    // Direct Map 窗口之外)——`virt_to_phys` 会产伪物理地址(NK4-C 1.51 B27 残留
    // 根因)。改用 AddressRef::Process(caller CR3 映射内核 higher-half，
    // resolve_physical 走真页表得正确 PA)。拷至调用方缓冲为单次跨空间拷，
    // caller 的 cr3 由 proc_cr3 解析(与 dispatch_trace 同款)。
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

    /// C utility.c:71-75 — 顺序写入不环绕:size 增至写入量,next 线性推进。
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

    /// 环绕语义(C kputc 的 % _KMESS_BUF_SIZE):容量 10000,写 10005 字节
    /// 后 size 封顶 10000、next=5;快照按 (next-size)%SIZE 从最旧字节展开。
    #[test]
    fn test_putc_wraps_and_snapshot_reorders() {
        let mut ring = KmessRing::new();
        // 写 10005 个不同字节(取模标记)。
        for i in 0..10005u64 {
            ring.putc((i % 251) as u8);
        }
        assert_eq!(ring.size(), 10000);
        assert_eq!(ring.next(), 5);
        let mut dst = [0u8; 10000];
        let moved = ring.snapshot_ordered(&mut dst);
        assert_eq!((ring.next(), ring.size(), moved), (5, 10000, 10000));
        // 展开顺序:dst[0] 应为写入序第 5 字节的值。
        assert_eq!(dst[0], (5 % 251) as u8);
        assert_eq!(dst[9999], (10004 % 251) as u8);
    }

    /// E-ISKMESS 快照大小约定:GET_KMESSAGES 应答体 = 8 字节头 + 10000。
    #[test]
    fn test_kmess_snapshot_size_constant() {
        assert_eq!(KMESS_SNAPSHOT_SIZE, 10008);
        assert_eq!(KMESS_BUF_SIZE, 10000); // sys_config.h:22
    }

    /// 喂入者不变量（台账 §8「消息环的字符写入口没有喂入者」的回归）。
    ///
    /// 收口类型只覆写 `write_byte`：`write_str`/`write_hex` 的默认体逐字节走
    /// `Self::write_byte`，故「控制台看到的每个字节都进环」在一处成立（含
    /// `\n` 前插的 `\r` 与十六进制展开）。在 `write_str` 里另记一次就会让
    /// 环与控制台不同形（两个真相），故本钉同时禁止那种写法。
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
