//! aarch64 AP early stub — S-4 body landed (§续-405, mirrors the riscv64
//! half of §续-398).
//!
//! # Entry state (PSCI CPU_ON, SMC64 form — see `smp.rs::boot_ap`)
//!
//! - x0 = context_id (the bootstrap cookie; `boot_ap` hands the entry
//!   cookie through — §3.1)
//! - PC = the physical entry address `boot_ap` passed as x2
//! - EL1, MMU **off**, caches in their reset state
//!
//! Unlike the riscv64 half there is **no boot election**: on the UEFI
//! carrier only one CPU is ever dispatched into boot services (the boot
//! processor); the secondaries sit in the firmware PSCI parking loop and
//! are extracted purely by `CPU_ON`. The wiring therefore self-skips via
//! the kernel's identity anchor (`current_cpu_id`) instead of an election
//! cell.
//!
//! # Stub obligations (frozen §3.1)
//!
//! 1. `dsb ish` — consumer-side ordering barrier (§3.9 first-read).
//! 2. Read the `ApBootstrap` record (arch-local static storage, v7 #4;
//!    `adrp+add` PC-relative resolves to the runtime address — a
//!    physical address while the MMU is off, same argument as the
//!    riscv64 `la`).
//! 3. TTBR0/TTBR1 ← the record's page-table root (the kernel pins both
//!    TTBRs to the same root — `paging.rs::enable`; MAIR ← the kernel's
//!    `0xFF`; TCR ← the same constant expression `enable()` builds).
//! 4. `dsb sy` + `isb`; SCTLR.M = 1 + `isb` (MMU on — the boot root
//!    covers the kernel image both identity and high, so the PC remains
//!    valid across the enable).
//! 5. `sp` ← the record's per-AP stack; x0 ← the record's high VA;
//!    branch to the `ap_early_entry` Rust convergence point.
//!
//! # Identity-coverage verification (S-3c 核验记录，aarch64 半)
//!
//! The stub and record live inside the kernel image; the boot-shim root
//! covers the image **both at its high VMA and identity** plus the DM
//! windows. The aarch64 layout is a fixed-offset model exactly like the
//! riscv64 one (`aarch64.ld`: KERNEL_VIRT_BASE=0xFFFF800000000000,
//! KERNEL_PHYS_BASE=0x40200000, every section's LMA = VMA − KVIRT +
//! KPHYS) — so `entry_start_pa` is a link-time constant subtraction, not
//! a dynamic-loader query.
//!
//! # Deviation from the riscv64 half (deliberate)
//!
//! No UART step marker anywhere: §续-403② proved on the riscv half that
//! device-address assumptions break with the root-table shape (the A3
//! write faulted there), and `AP_ARRIVED` is the gate's authoritative
//! marker anyway. The convergence goes straight to the arrival store.

use crate::arch::ap_early_entry::ApBootstrap;

/// 早期 hart 选举格（§续-406：QEMU 直核 `-kernel` ELF＝全 CPU 进同一
/// 入口，与 riscv fw_dynamic 同模型——kernel-image `_start` 以 LD/EXCL
/// 选主，存 MPIDR+1；0=未选）。UEFI/boot-shim 链单 CPU 进入口，选举恒
/// 由它赢，格语义不变。.data（loader 按 filesz 写初值 0，非 NOBITS
/// 侥幸），no_mangle 供 kernel-image 桩 asm 字面引用与内核读取。
#[unsafe(link_section = ".data")]
#[unsafe(no_mangle)]
pub static BOOT_HART_ELECTED: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// 次级核停车邮箱的 GO 格（§续-406 直核链的 AP 交付通道：内核接线填记
/// 录后 Release 发布，停车者独占读清后跳桩；UEFI 链无人发布＝停车者不
/// 醒，与 riscv 半的 §续-400 停车邮箱同格名同语义）。.data 同上。
#[unsafe(link_section = ".data")]
#[unsafe(no_mangle)]
pub static AP_GO: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Per-AP bootstrap record — arch-local static storage (v7 #4; the arm
/// form has no <1MiB copy constraint, firmware starts the AP directly at
/// an image physical address). BSP fills via [`record`] before
/// `boot_ap`; the AP-side stub reads it PC-relative (`adrp+add`
/// resolves to the runtime address while the MMU is off).
#[unsafe(no_mangle)]
pub static mut AP_BOOTSTRAP_RECORD: ApBootstrap = ApBootstrap {
    logical_id: 0,
    _pad: 0,
    hw_id: 0,
    page_table_root_pa: 0,
    kernel_stack_top_va: 0,
    rust_entry_va: 0,
};

/// BSP-side fill window (before `boot_ap`). `&mut` access is exclusive
/// by the boot sequence: the BSP writes while the AP is not yet running
/// (the riscv-half parity note applies verbatim).
#[allow(static_mut_refs)]
pub fn record() -> &'static mut ApBootstrap {
    unsafe { &mut AP_BOOTSTRAP_RECORD }
}

/// Physical entry address of the AP stub (`boot_ap`'s `entry` argument).
///
/// Fixed-offset link model (`aarch64.ld`): every section's LMA = VMA −
/// KERNEL_VIRT_BASE + KERNEL_PHYS_BASE, so the stub's physical address
/// is its link address minus the constant delta. PSCI CPU_ON's
/// entry_point must be physical (the AP starts with the MMU off).
pub fn entry_start_pa() -> usize {
    const KVIRT: usize = 0xFFFF_8000_0000_0000;
    // KPHYS = 0x4020_0000（aarch64.ld 的 KERNEL_PHYS_BASE）。§续-406 定谳：
    // 曾误写 0x0402_0000（下划线错位＝差一个 0，值差 16 倍）——AP 被送进
    // 垃圾地址 undef 死，桩/记录/通道全对也白搭。常量必须与链接脚本对表。
    const KPHYS: usize = 0x4020_0000;
    let va = ap_early_entry_start as usize;
    va - (KVIRT - KPHYS)
}

unsafe extern "C" {
    /// 桩物理入口（`boot_ap` 的 start_addr）。
    fn ap_early_entry_start();
}

// ── AP 早期桩（义务清单 §3.1 的汇编体，§续-405 落地）──────────────────
//
// 入口态（PSCI CPU_ON SMC64）：x0=cookie、PC=start_addr、EL1、MMU off。
//
// 五步义务：dsb ish（消费侧栅栏）→ adrp+add 取记录运行位址并读三字段 →
// TTBR0/TTBR1 同根 + MAIR 0xFF + TCR 常量（与 paging.rs::enable 同值，
// 常量经 global_asm 的 const 模板参数注入——手写立即数已被 §续-403①
// 的心算教训封死）→ dsb sy + isb、SCTLR.M=1、isb（镜像 enable() 的
// 使能序列：只置 M 位，RES1 位保持复位值）→ sp/x0/br 进汇聚点。
core::arch::global_asm!(
    r#"
    .section .text.ap_early_entry, "ax"
    .globl ap_early_entry_start
    ap_early_entry_start:
        dsb ish                         // consumer-side fence (S3.9)
        adrp x2, AP_BOOTSTRAP_RECORD
        add  x2, x2, :lo12:AP_BOOTSTRAP_RECORD
        ldr  x3, [x2, #16]              // page_table_root_pa
        ldr  x4, [x2, #24]              // kernel_stack_top_va
        ldr  x5, [x2, #32]              // rust_entry_va
        msr  ttbr0_el1, x3              // kernel pins both TTBRs to one root
        msr  ttbr1_el1, x3
        mov  x6, #0xff                  // MAIR — same constant as enable()
        msr  mair_el1, x6
        movz x6, #{tcr_a}               // TCR — same constant expression as
        movk x6, #{tcr_b}, lsl #16      //   enable() (T0SZ=T1SZ=16 halves)
        movk x6, #{tcr_c}, lsl #32
        msr  tcr_el1, x6
        dsb  sy
        isb
        mrs  x6, sctlr_el1
        orr  x6, x6, #1                 // SCTLR.M = 1 (mirror enable())
        msr  sctlr_el1, x6
        isb
        // B2=MMU-on 后仍可取指执行（恒等映射覆盖桩的实证）；缺失=根表
        // 恒等映射缺失（riscv A2/A3 同族取证位）。
        ldr x9, =0x09000000
        mov x10, #0x42                  // 'B'
        strb w10, [x9]
        mov x10, #0x32                  // '2'
        strb w10, [x9]
        mov  sp, x4                     // per-AP kernel stack (high VA)
        movz x6, #{d_a}                 // x0 = record HIGH VA =
        movk x6, #{d_b}, lsl #16        //   record PA + (KVIRT - KPHYS);
        movk x6, #{d_c}, lsl #32        //   reachable through the kernel
        movk x6, #{d_d}, lsl #48        //   mapping now that MMU is on
        add  x0, x2, x6
        br   x5                         // jump to Rust convergence (high VA)
    "#,
    tcr_a = const (AP_TCR & 0xffff),
    tcr_b = const ((AP_TCR >> 16) & 0xffff),
    tcr_c = const ((AP_TCR >> 32) & 0xffff),
    d_a = const (VA_DELTA & 0xffff),
    d_b = const ((VA_DELTA >> 16) & 0xffff),
    d_c = const ((VA_DELTA >> 32) & 0xffff),
    d_d = const ((VA_DELTA >> 48) & 0xffff),
);

/// TCR_EL1 value the stub installs — the same constant expression
/// `paging.rs::enable()` builds (T0SZ=T1SZ=16, both 48-bit halves;
/// IRGN/ORGN/RSH shares as in enable). Kept next to the asm so the two
/// cannot drift apart; the pin test asserts the assembly matches this
/// constant.
const AP_TCR: u64 = (16u64 << 0)
    | (0 << 6)
    | (0 << 7)
    | (1 << 8)
    | (1 << 10)
    | (3 << 12)
    | (0 << 14)
    | (16u64 << 16)
    | (0 << 22)
    | (0 << 23)
    | (1 << 24)
    | (1 << 26)
    | (3 << 28)
    | (2 << 30)
    | (5 << 32)
    | (0 << 36)
    | (0 << 37);

/// Record PA → record high-VA delta, identical to `entry_start_pa`'s
/// link-model constants (KVIRT − KPHYS). KPHYS 同款 typo（0x0402_0000）
/// 曾在此第二处存活：x0 被算到无效高半地址，汇聚点首读 fault，AP_ARRIVED
/// 永不置位（§续-406 smpd9 定谳）。常量与 aarch64.ld 对表。
const VA_DELTA: usize = 0xFFFF_8000_0000_0000 - 0x4020_0000;

/// 汇聚点到达标记（门判据「次级核可观测汇聚点到达」的观测面，与 riscv
/// 半同名同义）。0 = 未到；1 = 已到（Release 之后的 Rust 视图）。
pub static AP_ARRIVED: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// 汇聚点——AP 从桩跳入（x0 = 记录高半 VA）。
///
/// riscv 同名函数的 aarch64 形：快照记录字段进局部（防 BSP 为下一 hart
/// 复写），置到达标记（Release），然后**驻留**（wfi 停车环）。
///
/// 与 riscv 半的两点刻意差异：①无 UART 标记（§续-403② 教训：设备地址
/// 依赖根表形状不可靠；AP_ARRIVED 才是权威判据）；②a0/x0 传记录**高半
/// VA**（桩已换算），内核根映射可达。
pub unsafe extern "C" fn ap_early_entry(bootstrap_va: usize) -> ! {
    let record = unsafe { &*(bootstrap_va as *const ApBootstrap) };
    // 消费快照（volatile 读：防与 BSP 的潜在复写合并）。
    let _logical = unsafe { core::ptr::read_volatile(&record.logical_id) };
    let _hw_id = unsafe { core::ptr::read_volatile(&record.hw_id) };
    let _ = (_logical, _hw_id);
    // 到达标记（Release：后续 BSP 读到 1 时，上方的快照读已完成）。
    core::sync::atomic::AtomicUsize::store(&AP_ARRIVED, 1, core::sync::atomic::Ordering::Release);
    loop {
        unsafe {
            core::arch::asm!("wfi", options(nomem, nostack));
        }
    }
}
