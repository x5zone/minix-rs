//! Production kernel image binary (multi-arch: x86_64, aarch64, riscv64):
//! links minix-kernel into a kernel ELF at the boot-shim contract location.
//! boot-shim reads it from `/EFI/minix/kernel.elf`
//! (`os/boot-shim/src/loader.rs` `KERNEL_PATH`) and does two things:
//! `compute_kernel_layout` reads the layout into `KernelInfo`
//! (kern_virt_base / kern_phys_base / kern_size), and
//! `load_segments_into_phys_memory` copies segment bodies to their PT_LOAD
//! physical addresses and zeroes BSS. The layout contract is fixed by the
//! per-arch script in this directory (high VMA + low physical LMA, consistent
//! inter-segment offsets): `x86_64.ld` for x86_64, `aarch64.ld` for aarch64,
//! `riscv64.ld` for riscv64, selected by `build.rs` from `--target`. The Rust
//! surface is architecture-independent; only the entry instruction sequence
//! and the early console split by `target_arch`.
//!
//! The consumer differs per architecture: x86_64 / aarch64 go through the UEFI
//! install path (xtask image → boot-shim reads the contract location);
//! riscv64 is not a UEFI disk image, so today it is only produced and
//! statically layout-checked by `os/kernel-image/check-layout.sh riscv64`, and
//! its install path is not implemented yet (`os/xtask/src/image.rs:195-203`
//! still bails honestly for that architecture).
//!
//! ## Delivery boundary (honest status)
//!
//! `_start` currently sets up the stack and parks (halt). The boot-shim →
//! kernel image jump handoff protocol (how KernelInfo is passed, what the
//! entry state is) has been settled (per-process independent root primitive;
//! aarch64's `BootHandoffV1` arena handoff is its implementation), and this
//! image's closure is that implementation. The current UEFI production line
//! still runs boot-shim with an inlined kernel, calling `arch_boot` in-process
//! (`os/boot-shim/src/main.rs:56`); this image turns the contract location
//! from "no producer" into "produced per the layout contract", which is what
//! the x86_64 install chain needs.
//!
//! ## Link retention (anti-strip)
//!
//! The point of the image is "contains the kernel body". `KERNEL_ENTRY_ANCHOR`
//! holds a fn pointer to `minix_kernel::arch_boot`: the link script `KEEP`s
//! that section and `rust_image_main` touches it on the run path — redundant
//! protection against `--gc-sections` or archive-member granularity stripping
//! the entire kernel boot graph out of the image.
//!
//! C counterpart: the Minix3 C kernel image is produced by the kernel Makefile
//! plus link script (`minix3/minix/kernel/`), and the boot module (bootx64.efi)
//! loads it — this bin is the Rust-side expression of the same shape
//! (Rewrite, not Translate).

#![no_std]
#![no_main]

// aarch64 kernel NEON/FP ban gate: the lazy-FPU model's "the kernel never uses
// the FPU" premise is enforced by build flags (xtask/src/image.rs injects
// `-C target-feature=-neon,-fp-armv8` for this package's aarch64 build). A
// direct cargo build that bypasses xtask would silently produce a kernel that
// clobbers user Q registers — reject it at compile time here. minix-arch's
// explicit FPSIMD primitives (fpu.rs) survive via per-function
// target_feature and do not trip this gate; boot-shim's UEFI carrier build does
// not compile this package (and the carrier context has no user processes, so
// there is no clobber surface), while host tests and other architectures are
// unaffected.
#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
compile_error!(
    "aarch64 kernel image must be built with -C target-feature=-neon,-fp-armv8 \
     (the lazy-FPU model requires a kernel that never touches Q0-Q31; \
     see SD-23 and xtask/src/image.rs)"
);

// riscv64 sibling gate: sstatus.FS=Off gating has no per-mode split — any FP
// execution in the kernel (including precompiled core residue) traps itself,
// so the kernel must be truly FP-free.
#[cfg(all(target_arch = "riscv64", target_feature = "f"))]
compile_error!(
    "riscv64 kernel image must be built with -C target-feature=-f,-d \
     (the lazy-FPU model requires a kernel that never touches f0-f31; \
     see SD-23 and xtask/src/image.rs)"
);

extern crate alloc;

use core::arch::asm;
// The riscv64 leg's anchor touch lives in `bootface`; this entry does not use
// black_box.
#[cfg(not(target_arch = "riscv64"))]
use core::hint::black_box;
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicUsize, Ordering};

use alloc::alloc::{GlobalAlloc, Layout};

use minix_boot::KernelInfo;
use minix_types::PhysBytes;

// The early console picks a board implementation per architecture (all three
// expose a same-named `write_str`, so the Rust surface needs no branching):
// x86_64 = COM1 serial, aarch64 = PL011, riscv64 = the QEMU virt 16550 UART
// (`os/plat/src/riscv64/early_console.rs`).
#[cfg(target_arch = "aarch64")]
use minix_plat::arm64::early_console;
#[cfg(target_arch = "riscv64")]
use minix_plat::riscv64::early_console;
#[cfg(target_arch = "x86_64")]
use minix_plat::x86_64::early_console;

// ── Kernel boot graph anchor ──────────────────────────────────────
//
// The type is the contract: `arch_boot(&KernelInfo, PhysBytes) -> !` is the
// kernel boot surface shared by the boot-shim in-process production line
// (`os/boot-shim/src/main.rs:56`) and the future jump production line. The
// anchor is pinned by the link script's `KEEP(*(.rodata.kernel_anchor))`.

#[used]
#[unsafe(link_section = ".rodata.kernel_anchor")]
static KERNEL_ENTRY_ANCHOR: fn(&KernelInfo, PhysBytes) -> ! = minix_kernel::arch_boot;

// ── Entry (global_asm: set up the stack first, then enter the Rust surface) ──
//
// The stack area is reserved in .bss by the link script
// (kernel_boot_stack_bottom/top, 64 KiB) — same shape as the Linux kernel
// init_stack. The `.text.boot` section is placed in the image's first
// executable page (link script KEEP + first position).
//
// `kernel_boot_stack_top` is a link-script symbol: it has no Rust declaration
// here, leaving resolution to the linker (a missing symbol fails at link time,
// which is the fail-fast we want).
//
// Each architecture has its own instruction sequence with a one-to-one
// semantic correspondence: load stack top → clear frame pointer → call the
// Rust surface → halt and park. The aarch64 `adrp` + `add :lo12:` symbol
// addressing copies an existing carrier in the repo
// (`os/qemu-tests/test-kernels/kernel/bootstrap/test-rt-birth-aarch64/src/main.rs:250-251`);
// riscv64 uses `la` (relaxed by the linker to PC-relative `auipc` + `addi`) so
// the stack-top symbol resolves under both low-half and high-half execution
// views.

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text.boot, "ax"
    .globl _start
    _start:
        // T2 entry contract: machine entry, not a called function. No return
        // address is pushed and no caller guarantees the flags/stack state.
        // `cli` before the first memory write; `cld` because the SysV ABI
        // (and the Rust code below) assumes DF=0.
        cli
        cld
        // SD-7 / P-X86-03（2026-10-06）：防御性 .bss 清零，镜像 riscv64
        // 甲案（UEFI boot-shim 的 loader 已按 memsz-filesz 清零；本循环
        // 是 SD-1 多入口形态下的入口自持——未来直载入口不依赖装载者）。
        // RIP 相对取运行位址：分页开启（高半映射）与恒等映射两种交接
        // 视图下都落在正确的物理页。rdi 游标/rcx 界限在入口处无活值；
        // 清零只碰 rdi/rax/rcx，`rbx`（arena 物理地址）全程不动。
        lea rdi, [rip + __bss_start]
        lea rcx, [rip + __bss_end]
    1:  cmp rdi, rcx
        jae 2f
        mov qword ptr [rdi], 0
        add rdi, 8
        jmp 1b
    2:
        lea rsp, [rip + kernel_boot_stack_top]
        and rsp, -16
        xor ebp, ebp
        mov rdi, rbx
        call {rust_image_main}
    3:  cli
        hlt
        jmp 3b
    "#,
    rust_image_main = sym rust_image_main,
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text.boot, "ax"
    .globl _start
    _start:
        // ── 早期 hart 选举（§续-406 起的保留格）：MPIDR+1 存选举格
        //（LD/EXCL 独占语义），落选者本地停车。UEFI/boot-shim 链只派
        // 一个 CPU 进本入口，选举恒由它赢；直核链的门读数（test-smp-aps-
        // aarch64，ap-arrived=1）坐实 PSCI CPU_ON 是唯一次级核交付通道、
        // 链上无落选者——选举格退为身份记录，落选臂只作防御性兜底。
        mrs x8, mpidr_el1
        and x8, x8, #0xffffff        // Aff2:1:0
        add x10, x8, #1              // 存 hartid+1：与「未选 0」恒可区分
        adrp x9, BOOT_HART_ELECTED
        add x9, x9, :lo12:BOOT_HART_ELECTED
    1:  ldaxr w11, [x9]
        cbnz w11, park               // 旧值非 0 ⇒ 已有 BSP ⇒ 落选停车
        stlxr w12, w10, [x9]
        cbnz w12, 1b
        b 4f
    park:
        // 落选核本地停车（SD-37 乙案）：次级核交付＝内核 PSCI CPU_ON，
        // 它只作用于仍持在固件手的核；进过 payload 的落选者没有任何
        // 可投递通道——wfi 常驻与旧「无人发布的 AP_GO 邮箱」可观测
        // 行为逐位一致，仅去掉永不兑现的 GO 契约（SD-37 台账乙案原文）。
    2:  wfi
        b 2b
    4:
        // SD-7 / P-X86-03（2026-10-06）：防御性 .bss 清零，镜像 riscv64
        // 甲案（同 x86_64 注释：loader 已清是现行 UEFI 路径事实，入口
        // 自持为 SD-1 多入口形态）。adrp 取运行页位址（MMU 交接态下
        // 无论恒等或高半映射都落正确物理页）。x9 游标/x10 界限无活值。
        adrp x9, __bss_start
        add x9, x9, :lo12:__bss_start
        adrp x10, __bss_end
        add x10, x10, :lo12:__bss_end
    5:  cmp x9, x10
        b.hs 6f
        str xzr, [x9], #8
        b 5b
    6:
        adrp x9, kernel_boot_stack_top
        add x9, x9, :lo12:kernel_boot_stack_top
        mov sp, x9
        mov x29, xzr
        bl {rust_image_main}
    3:  wfi
        b 3b
    "#,
    rust_image_main = sym rust_image_main,
);

#[cfg(target_arch = "riscv64")]
core::arch::global_asm!(
    r#"
    .attribute arch, "rv64gc"
    .section .text.boot, "ax"
    .globl _start
    _start:
        # ── 早期 hart 选举（§续-400 起的保留格）：每个进 _start 的 hart
        # 带着自身 hartid（a0），先到者当选存 hartid+1，落选者本地停车。
        # 次级核的实际交付通道是内核 smp_init 的 SBI HSM hart_start（它
        # 只作用于仍留在 OpenSBI 的 hart）；门读数（test-smp-aps-riscv64，
        # ap-arrived=1）坐实现链无落选者——选举格退为身份记录，落选臂
        # 只作防御性兜底。选举格在 .data（loader 按 filesz 写过初值 0，
        # 非 NOBITS 侥幸）。amoswap.aq 给当选者 acquire 序。
        la t0, BOOT_HART_ELECTED
        addi t5, a0, 1          # 存 hartid+1：hart 0 存 1，与「未选 0」恒可区分
        amoswap.d.aq t2, t5, (t0)
        bnez t2, park           # t2=旧值非 0 ⇒ 已有 BSP ⇒ 落选停车路径

        # ── 当选 BSP：NK4-C 续-88 甲案 .bss 清零（原注释全保留）。
        # OpenSBI 递入的是裸 RAM（NOBITS 不写），而镜像的 bump 分配器
        # 游标/平台冻结全局就住在 .bss（NK4B M4.4 事实五），非零垃圾让
        # 首次分配返回乱址——先清 .bss 再立栈（K10 载体先例的形式，bgeu
        # 无符号界判定 + 8 字节步进）。清零循环全走 `la` 松弛后的 PC 相
        # 对引用：执行视图 = 物理基址（medany，M4.3 实测事实），栈顶符
        # 号同样按运行位址解析，清零只会扫自己下方的镜像内区域。
        la t0, __bss_start
        la t1, __bss_end
    1:  bgeu t0, t1, 2f
        sd zero, 0(t0)
        addi t0, t0, 8
        j 1b
    2:
        la sp, kernel_boot_stack_top
        li ra, 0
        li fp, 0
        # 甲案交接：OpenSBI 递 a0 = boot hartid、a1 = DTB 物理指针；U-Boot
        # 腿另约定 a2 = BootFileTable 物理指针（本 boot 面同时读 DTB
        # /chosen/opensbi,boot-file-table，a2 不合法时靠 DTB 通道；两者均
        # 过 magic 把关才解引用）。`call` 是 PC 相对，不改动 a0–a2，三个
        # 入参自然落位 Rust 面（rt-birth 载体 :426 同形先例）。
        call {rust_image_main}
    3:  wfi
        j 3b

    # ── 落选 hart 本地停车（SD-37 乙案）：次级核交付＝SBI HSM hart_start，
    # 只作用于仍留在 OpenSBI 的 hart；进过 payload 的落选者没有任何可
    # 投递通道——wfi 常驻与旧「无人发布的 AP_GO 邮箱」可观测行为一致，
    # 仅去掉永不兑现的 GO 契约（旧 park 跳汇聚点时 a0 未按桩契约换成
    # 记录 VA，本身残缺，从未被踩过）。
    .align 2
park:
    wfi
    j park
    "#,
    rust_image_main = sym rust_image_main,
);

#[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
mod bootface_common;

#[cfg(target_arch = "riscv64")]
mod bootface;

#[cfg(target_arch = "aarch64")]
mod bootface_a64;

/// Rust-surface entry (x86_64 shape; aarch64/riscv64 have their own wiring
/// below): banner + x86-64 image entry: boot-shim builds the tables, enables
/// paging, then jumps absolutely into this image's high-half `_start` with
/// handoff register `rbx` = physical address of the T1 handoff arena. After
/// clearing `.bss`, `_start`'s entry trampoline moves `rbx` into the SysV first
/// argument `rdi`, so the Rust surface receives it like an ordinary function
/// argument; `arena_pa == 0` only means the jumper did not hand off (x86 has no
/// direct-boot self-bootstrap face), in which case we halt honestly instead of
/// dereferencing a null pointer.
///
/// Before the handoff protocol was wired up this used to be an "print one line
/// and park" observation point; it now calls `arch_boot_resume_v1`, sharing the
/// same v1 decoding and additive resume with aarch64.
#[cfg(target_arch = "x86_64")]
fn rust_image_main(arena_pa: usize) -> ! {
    // Touch the anchor: reference it once on the run path so the anchor static
    // and the kernel boot graph it points to are pulled into the link even at
    // archive-member granularity (KEEP is the second, link-time safeguard).
    let _kernel_entry: usize = black_box(KERNEL_ENTRY_ANCHOR as usize);

    if arena_pa == 0 {
        early_console::write_str(
            "### minix-rs kernel-image: no handoff arena (x86 has no direct-boot face); halting\n",
        );
        return halt();
    }
    early_console::write_str(
        "### minix-rs kernel-image: resuming high-half boot (handoff v1 arena, x86_64)\n",
    );
    minix_kernel::arch_boot_resume_v1(arena_pa as u64)
}

/// aarch64 image entry: boot-shim builds the tables + enables translation,
/// then jumps absolutely into this image's high-half `_start` with `x0` =
/// physical address of the T1 handoff arena (identity reachable). `_start`'s
/// `bl` leaves `x0` untouched, so it arrives as the first argument.
///
/// This image links at the high half (`aarch64.ld`'s `KERN_VIRT_BASE`), so
/// when `x0 != 0` we hand over to `minix_kernel::arch_boot_resume_v1`: it
/// decodes the five payload blocks from the arena, copies them into the T5
/// landing area inside this image's `.bss`, validates `KernelInfo`, then runs
/// the additive-only high-half resume (record root, build DM, jump to kmain)
/// and **never rebuilds the root** — the root was built by boot-shim and is
/// active right now; zeroing it via `new_from_page` would immediately destroy
/// our own instruction-fetch mapping.
///
/// `x0 == 0` is the direct-kernel `-kernel` path (QEMU passes no arguments),
/// which uses the `bootface_a64` self-bootstrap face; both chains share this
/// entry and `.text.boot`.
#[cfg(target_arch = "aarch64")]
fn rust_image_main(arena_pa: usize) -> ! {
    let _kernel_entry: usize = black_box(KERNEL_ENTRY_ANCHOR as usize);
    if arena_pa == 0 {
        bootface_a64::bootface_a64();
    }
    // Use the TTBR1 high-half MMIO alias (this image's own HIGH_MMIO_LIVE
    // global) to prove the cross-image high-half console works; the resume
    // path sets it again internally (idempotent).
    early_console::use_high_mmio_window();
    early_console::write_str(
        "### minix-rs kernel-image: resuming high-half boot (handoff v1 arena)\n",
    );
    minix_kernel::arch_boot_resume_v1(arena_pa as u64)
}

/// riscv64 image entry: kernel-image is its own bootstrapper (no cross-image
/// handoff, no BootHandoff). The entry asm has already cleared `.bss` (the
/// common precondition), and a0/a1/a2 land as three arguments; the real work is
/// in `bootface`: decode the DTB memmap from a1 → BootFileTable (a2/chosen
/// dual channel, gated by magic) → load 12 modules → `minix_kernel::arch_boot`
/// passes the validation gate.
#[cfg(target_arch = "riscv64")]
fn rust_image_main(boot_hart: u64, dtb_phys: u64, boot_file_table_pa: u64) -> ! {
    bootface::bootface(boot_hart, dtb_phys, boot_file_table_pa)
}

fn halt() -> ! {
    loop {
        // Halt and park with interrupts disabled. No memory writes, no stack
        // use.
        // - x86_64: `cli` + `hlt`;
        // - aarch64: `wfi` (this image does not enable WFI trap control, and
        //   the entry state will be defined by the future handoff protocol;
        //   here we only need to guarantee parking);
        // - riscv64: the same-named `wfi` (usable in S-mode; whether
        //   `mstatus.TW` traps it depends on the OpenSBI configuration, the
        //   same unwired entry-state boundary).
        #[cfg(target_arch = "x86_64")]
        unsafe {
            asm!("cli", "hlt", options(nomem, nostack))
        }
        #[cfg(target_arch = "aarch64")]
        unsafe {
            asm!("wfi", options(nomem, nostack))
        }
        #[cfg(target_arch = "riscv64")]
        unsafe {
            asm!("wfi", options(nomem, nostack))
        }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // The image-time panic surface cannot reach the formatting machinery (no
    // heap dependency): write the panic message + location into the early
    // console through a fixed-size stack buffer, then park (debugging need: a
    // bare "PANIC" line cannot locate anything, the payload is mandatory).
    early_console::write_str("### minix-rs kernel image: PANIC — ");
    struct StackWriter<'a> {
        buf: &'a mut [u8],
        pos: usize,
    }
    impl core::fmt::Write for StackWriter<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            for &b in s.as_bytes() {
                if self.pos >= self.buf.len() {
                    break;
                }
                self.buf[self.pos] = b;
                self.pos += 1;
            }
            Ok(())
        }
    }
    let mut buf = [0u8; 200];
    let mut w = StackWriter { buf: &mut buf, pos: 0 };
    // `write_fmt` on `PanicInfo` renders the message; append the location.
    let _ = core::fmt::write(&mut w, format_args!("{info}"));
    let n = w.pos;
    // The 200-byte cap can truncate mid-`char`, so validate rather than
    // assume ASCII: a non-UTF-8 tail falls back to a shorter valid prefix
    // (`utf8::floor`-style loop) instead of UB via `from_utf8_unchecked`.
    let mut end = n;
    while end > 0 && core::str::from_utf8(&buf[..end]).is_err() {
        end -= 1;
    }
    let text = core::str::from_utf8(&buf[..end]).unwrap_or("");
    early_console::write_str(text);
    early_console::write_str(" — halting\n");
    halt()
}

// ── Minimal image-time bump allocator ──────────────────────────────
//
// Once the kernel boot graph is pulled into the link through the anchor, the
// alloc-surface symbols (__rust_alloc etc.) enter the link requirements, and
// the bin must supply a global_allocator. This one only satisfies the "image
// link surface": a one-shot bump over a static region, never reclaimed
// (dealloc is a contract-safe no-op). The real heap initialization for the
// running kernel (C counterpart: the kmalloc/mempool init half) belongs to the
// entry-wiring work and is not decided here.

/// 1 MiB: sized for the link surface and minimal early boot, not a runtime
/// quota promise. Raised from 128 KiB to 1 MiB: `.bss` is zeroed by the entry
/// asm (so the cursor is trustworthy), and any boot leg that uses the heap
/// (including panic diagnostics) needs module-scale headroom; module copying
/// itself goes through physical bump scratch pages and does not consume the
/// heap (see `bootface`). Note that heap width moves the image span:
/// `check-layout.sh` L9 measures the ELF span with readelf (last segment
/// VMA+memsz − first segment VMA) and reconciles it against the `KERN_SIZE`
/// literal in `bootface.rs` (wider heap → wider span → L9 fails immediately);
/// do not open a symbol section in `.ld` (one more PT_LOAD pushes the span off
/// a 2 MiB multiple and breaks L5/L7 — tried and reverted).
const IMAGE_HEAP_LEN: usize = 1024 * 1024;

/// The image heap body. Confirmed issue: `static IMAGE_HEAP = [0; N]` that is
/// only addressed via `as_ptr()` is treated by rustc as a read-only constant
/// placed in `.rodata` — and the bump allocator writes into it! Use
/// `static mut` instead (no safe references, only `&raw` address arithmetic);
/// the zero-initialized object lands in `.bss`, matching "writable heap"
/// semantics and removing the latent misbehavior of writing a read-only
/// section once page protection is active (all three architectures benefit:
/// x86/aarch64 currently go through boot-shim and do not expose it — latent
/// debt).
static mut IMAGE_HEAP: [u8; IMAGE_HEAP_LEN] = [0; IMAGE_HEAP_LEN];

struct ImageBump {
    cursor: AtomicUsize,
}

impl ImageBump {
    const fn new() -> Self {
        Self {
            cursor: AtomicUsize::new(0),
        }
    }
}

unsafe impl GlobalAlloc for ImageBump {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: image-time single-threaded phase with no prior concurrency
        // contract (the boot CPU is exclusive); the atomic cursor only
        // satisfies GlobalAlloc's &self contract; all bounds are checked
        // explicitly.
        let old = self
            .cursor
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |cur| {
                let aligned = cur.checked_add(layout.align() - 1)? & !(layout.align() - 1);
                let end = aligned.checked_add(layout.size())?;
                (end <= IMAGE_HEAP_LEN).then_some(end)
            });
        match old {
            // SAFETY: `IMAGE_HEAP` is the exclusive image-time heap region,
            // written only by this allocator through address arithmetic (no
            // concurrent references); the offset is explicitly bounded by
            // `end <= IMAGE_HEAP_LEN` above.
            Ok(new_end) => unsafe {
                (&raw const IMAGE_HEAP).cast::<u8>().add(new_end - layout.size()) as *mut u8
            },
            // Arena exhausted: hand back null, which the `GlobalAlloc` contract
            // turns into the caller's allocation failure.
            Err(_) => core::ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // The image-time bump never reclaims: the one-shot phase has no
        // paired-free need; the real reclamation surface lands with the heap
        // design (at which point this allocator retires entirely).
    }
}

#[global_allocator]
static IMAGE_ALLOCATOR: ImageBump = ImageBump::new();
