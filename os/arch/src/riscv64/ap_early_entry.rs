//! riscv64 AP early stub — S-3c (two-arch stub deliverable, riscv half).
//!
//! # Entry state (SBI hart_start, HSM extension — see `smp.rs::boot_ap`)
//!
//! - a0 = hartid
//! - a1 = the `opaque` cookie (`boot_ap` passes the bootstrap pointer —
//!   S-3c fixed the old "priv mode" mislabel)
//! - PC = the physical entry address `boot_ap` passed as a1's neighbor
//!   start_addr
//! - S-mode, MMU **off**
//!
//! # Stub obligations (frozen §3.1; the body lands with S-4)
//!
//! 1. Read the `ApBootstrap` record (the riscv form is **arch-local static
//!    storage** — v7 #4; record address = PC-relative from the stub inside
//!    the kernel image, computable with MMU off).
//! 2. `fence rw, rw` consumer-side ordering barrier (§3.9 first-read).
//! 3. satp ← the record's page-table root (BSP value, SATP-mode Sv39);
//!    `sfence.vma`.
//! 4. Branch to the `ap_early_entry` Rust convergence point (high VA via
//!    the kernel mapping the new satp establishes).
//!
//! # Identity-coverage verification (S-3c 核验记录)
//!
//! Same argument as the arm half: the stub and record live inside the
//! kernel image; the BSP root (boot-shim) maps the kernel image identity
//! **and** high plus the DM window — so MMU-on at the image's PA keeps the
//! PC mapped, and the record is reachable both identity (MMU-off reads)
//! and through the kernel mapping after `sfence.vma`. The hartid comes
//! from `boot_ap`'s a0 — no <1MiB constraint on riscv64.
//!
//! # Status
//!
//! **Skeleton** — the obligation list above is the contract; the asm body
//! lands with S-4 (satp sequencing per §3.3 riscv row).

/// Unit-test sentinel: proves the module's contract constants stay in sync
/// with the SBI channel (`boot_ap` passes the bootstrap cookie in a1/a2).
pub const OPAQUE_IS_BOOTSTRAP_COOKIE: bool = true;

/// Per-AP bootstrap record — arch-local static storage（义务清单 v7 #4：aarch64/
/// riscv64 形态无 1MiB 拷贝约束，固件直接把 AP 起到镜像物理位址，记录随镜像
/// 驻留）。BSP 在 `SbiEidHsm` hart_start 之前经 [`record`] 填字段；AP 侧桩
/// 用 PC 相对取址读取（MMU off 时 `la` 解析为运行位址＝物理）。
///
/// 消费纪律（§3.9）：AP 侧先 `fence rw, rw` 再首读（BSP 侧写后同样先栅栏
/// 再 hart_start——两面的 happens-before 由两道 fence 夹 SBI 的 hart 间
/// 交接构成）。
use crate::arch::ap_early_entry::ApBootstrap;

// #[no_mangle]：桩 asm 以字面符号名 `la t2, AP_BOOTSTRAP_RECORD` 引用
//（跨 crate 符号面），必须有未修饰的导出名。
/// 早期 hart 选举格（§续-400 定谳：OpenSBI QEMU virt＝全 hart 进 payload，
/// 每个 hart 都带 a0=自身 hartid 进 _start——先到者当选 BSP）。0=未选；
/// 否则=当选 hartid+1。住在 .data（loader 按 filesz 写初值 0，非 NOBITS
/// 侥幸），no_mangle 供 kernel-image 桩 asm 字面引用与内核读取。
#[unsafe(link_section = ".data")]
#[unsafe(no_mangle)]
pub static BOOT_HART_ELECTED: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// 次级核停车邮箱的 GO 格（§续-400 停车邮箱设计）：0=等待；1=记录已填、
/// 可跳汇聚点。.data 同上（初值由 loader 写入，非 NOBITS 侥幸）。
#[unsafe(link_section = ".data")]
#[unsafe(no_mangle)]
pub static AP_GO: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

#[unsafe(no_mangle)]
pub static mut AP_BOOTSTRAP_RECORD: ApBootstrap = ApBootstrap {
    logical_id: 0,
    _pad: 0,
    hw_id: 0,
    page_table_root_pa: 0,
    kernel_stack_top_va: 0,
    rust_entry_va: 0,
};

/// BSP 侧填充窗（hart_start 之前调用）。`&mut` 访问的安全性由调用序列
/// 独占保证：BSP 在 AP 未到（boot_ack 语义在 riscv 形态＝convergence 点
/// 的到达标记）之前独占写。
#[allow(static_mut_refs)]
pub fn record() -> &'static mut ApBootstrap {
    unsafe { &mut AP_BOOTSTRAP_RECORD }
}

/// AP 早期桩的物理入口地址（`boot_ap` 的 `entry` 实参）。
///
/// 桩住在内核镜像内（`.text.ap_early_entry` 段），镜像被装载到物理内存：
/// 符号地址按链接视图（medany＝物理）即物理地址——与 kernel-image 的
/// 执行视图（§续-88「执行视图 = 物理基址」）一致。
pub fn entry_start_pa() -> usize {
    // 链接视图＝高半（kernel-image riscv64.ld：VMA=0xFFFFFFC0_…，
    // PA=0x80200000 起）；hart_start 的 start_addr 必须是**物理**地址
    //（目标 hart MMU off 起跳）。换算 = 减 (KERNEL_VIRT - KERNEL_PHYS)。
    const KVIRT: usize = 0xFFFF_FFC0_0000_0000 as usize;
    const KPHYS: usize = 0x8020_0000;
    let va = ap_early_entry_start as usize;
    va - (KVIRT - KPHYS)
}

// ── AP 早期桩（义务清单 §3.1 的汇编体，S-4 落地）──────────────────
//
// 入口态（SBI HSM hart_start）：a0=hartid、a1=opaque cookie、PC=start_addr、
// S-mode、MMU off。
//
// 四步义务（frozen §3.1）：
// 1. `la t2, AP_BOOTSTRAP_RECORD`——PC 相对取记录物理位址（MMU off）；
// 2. `fence rw, rw`——消费侧首读栅栏（§3.9）；
// 3. satp ← 记录 page_table_root_pa（Sv39 模式位 8<<60）+ `sfence.vma`；
// 4. 跳 `ap_early_entry`（Rust 汇聚点，链接位址经记录 rust_entry_va 携带
//    ——kernel-image 执行视图为物理，链接位址即运行位址；未来高半形态
//    改由 BSP 写高位值，字段不变）。
core::arch::global_asm!(
    r#"
    .section .text.ap_early_entry, "ax"
    .globl ap_early_entry_start
    ap_early_entry_start:
        fence rw, rw                    # consumer-side fence
        la  t2, AP_BOOTSTRAP_RECORD     # record phys addr (MMU off)
        ld  t3, 16(t2)                  # page_table_root_pa
        ld  t4, 24(t2)                  # kernel_stack_top_va
        ld  t5, 32(t2)                  # rust_entry_va
        # UART step marker "A1" = stub entered, record fields loaded.
        # QEMU virt 16550 THR @ 0x10000000 — device write, MMU off safe.
        li  t0, 0x10000000
        li  t1, 0x41          # 'A'
        sb  t1, 0(t0)
        li  t1, 0x31          # '1'
        sb  t1, 0(t0)
        srli t3, t3, 12                 # PPN = pa >> 12
        li  t6, 0x8000000000000000      # SATP_MODE_SV39
        or  t3, t3, t6
        csrw satp, t3
        sfence.vma
        # UART step marker "A2" = MMU on with BSP root, fetch still alive
        # (proves the root identity-maps the image).
        li  t0, 0x10000000
        li  t1, 0x41          # 'A'
        sb  t1, 0(t0)
        li  t1, 0x32          # '2'
        sb  t1, 0(t0)
        mv  sp, t4                      # per-AP kernel stack
        # a0 = record HIGH VA = PA + (KVIRT - KPHYS)，MMU on 后经内核根映射
        li  t6, (0xFFFFFFC000000000 - 0x80200000) >> 12
        slli t6, t6, 12
        add a0, t2, t6
        jr  t5                          # jump to Rust convergence (high VA)
    "#,
);

unsafe extern "C" {
    /// 桩物理入口（`boot_ap` 的 start_addr）。
    fn ap_early_entry_start();
}

/// 汇聚点到达标记（§续-398 交付：门判据「次级核打印/可观测汇聚点到达」
/// 的观测面）。0 = 未到；1 = 已到（fence 之后的 Rust 视图）。
pub static AP_ARRIVED: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);

/// 汇聚点——AP 从桩跳入（a0 = 记录物理位址）。
///
/// x86 同名函数的 riscv 形：读回记录字段进局部（单镜像串行复用纪律的
/// riscv 版＝静态记录无需归还 BSP，但字段快照仍取进局部，防 BSP 为下一
/// hart 复写），置到达标记，然后**驻留**（wfi 停车环）。
///
/// 驻留而非继续内核初始化是本增量的诚实边界：SD-24（钳主核）未撤，
/// 每核运行队列/抢占下发未通电——AP 此刻「醒着但无所事事」，继续推进
/// 会踩 P-ALL-03 红线（撤钳不先补每核队列＝进程被安到无队列的核）。
pub unsafe extern "C" fn ap_early_entry(bootstrap_pa: usize) -> ! {
    // UART step marker "A3" = Rust convergence entered（经内核 DM 窗写
    // UART：VA = KDM_BASE + UART_PA，krewalk 同一换算约定）。
    unsafe {
        let uart = (0xFFFF_FFC0_4000_0000u64 + 0x1000_0000u64) as *mut u8;
        core::ptr::write_volatile(uart, b'A');
        core::ptr::write_volatile(uart.add(1), b'3');
    }
    // a0 = 记录高半 VA（桩换算；MMU on 后经内核根可达）。
    let record = unsafe { &*(bootstrap_pa as *const ApBootstrap) };
    // 消费快照（volatile 读：防与 BSP 的潜在复写合并）。
    let _logical = unsafe { core::ptr::read_volatile(&record.logical_id) };
    let _hw_id = unsafe { core::ptr::read_volatile(&record.hw_id) };
    let _ = (_logical, _hw_id);
    // 到达标记（Release：后续 BSP 读到 1 时，上方的快照读已完成）。
    core::sync::atomic::AtomicUsize::store(
        &AP_ARRIVED,
        1,
        core::sync::atomic::Ordering::Release,
    );
    loop {
        unsafe { core::arch::asm!("wfi", options(nomem, nostack)); }
    }
}
