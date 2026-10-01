//! RISC-V 64-bit FPU implementation using F/D extension registers.
//!
//! Implements `FpuArch` for RISC-V 64. Uses the F/D extension registers
//! (f0-f31) and the FCSR (Floating-point Control and Status Register).
//!
//! # FPU state buffer
//!
//! The F/D extension state consists of:
//! - 32 × 64-bit floating-point registers (f0-f31): 256 bytes
//! - FCSR (4 bytes)
//! Total: 260 bytes (padded to 264 for 8-byte alignment)
//!
//! # Lazy FPU on RISC-V
//!
//! RISC-V controls FPU access via `sstatus.FS` (bits 13-14):
//! - 0b00 (Off): FPU instructions trap
//! - 0b01 (Initial): FPU available, registers have initial values
//! - 0b10 (Clean): FPU available, registers unchanged since last save
//! - 0b11 (Dirty): FPU available, registers modified (must save on switch)
//!
//! The kernel gates FS=Off at boot (续-133 lazy model: the user's first
//! FP instruction traps into the rotation leg) and uses explicit save/restore
//! during context switch.
//!
//! C: No Minix3 equivalent (Minix3 has no RISC-V port). Based on the
//! RISC-V Privileged ISA Specification v1.11.

use crate::fpu_arch::FpuArch;

/// F/D extension state size: 32 × 8 bytes (f0-f31) + 4 (FCSR) + 4 (pad) = 264.
const FPU_SIZE: usize = 264;

/// RISC-V F/D extension state buffer.
///
/// Contains 32 × 64-bit floating-point registers + FCSR.
#[derive(Clone, Copy)]
#[repr(C, align(8))]
pub struct Riscv64FpuState {
    /// 32 × 64-bit floating-point registers (f0-f31).
    regs: [u64; 32],
    /// FCSR (Floating-point Control and Status Register).
    fcsr: u32,
    /// Padding to 264 bytes (256 + 4 + 4 = 264).
    _pad: u32,
}

impl core::fmt::Debug for Riscv64FpuState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Riscv64FpuState")
            .field("fcsr", &self.fcsr)
            .finish()
    }
}

impl Default for Riscv64FpuState {
    fn default() -> Self {
        Self {
            regs: [0u64; 32],
            fcsr: 0,
            _pad: 0,
        }
    }
}

impl Riscv64FpuState {
    /// Const-constructible zeroed state (for `const fn` table init).
    pub const fn new() -> Self {
        Self {
            regs: [0u64; 32],
            fcsr: 0,
            _pad: 0,
        }
    }
}

/// RISC-V 64-bit FPU implementation using F/D extension instructions.
#[derive(Debug, Default, Clone, Copy)]
pub struct Riscv64FpuArch;

// Raw FPU save/restore——`global_asm!` 实现（riscv 的
// `target_feature(enable="f")` 在 stable 不可用；裸 asm 无需特性门）。
// 序言 `csrs sstatus, fsm` 把 FS 提到 ≥Initial（FS=Off 时 fsd/fld 自
// 身非法；C 的 disable_fpu_exception-then-fxsave 对位）。restore 覆
// 写全部 32 个 f 寄存器=违反 extern "C" 的 fs0-fs11 保留约定——内核
// 全 `-f,-d` 编译、无任何存活 FP 值，违约无害（aarch64 v8-v15 豁免
// 同款前提）。
core::arch::global_asm! {
    ".section .text.rv_fpu, \"ax\"",
    ".globl minix_rv_fpu_save",
    "minix_rv_fpu_save:",           // a0 = dst ptr, a1 = &fcsr_out
    // 续-133：`.option arch, +d` 提升 asm 汇编视角的特性集——rustc 的
    // riscv64gc-unknown-none-elf 内联 asm 默认不含 D，fsd 直接被汇编器
    // 拒收（最小复现实证）；内核 -f,-d 构建下模块特性同样无 D。
    "   .option arch, +d",
    "   li      t2, 0x2000",        // 0b01 << 13 (FS = Initial)
    "   csrs    sstatus, t2",
    "   fsd     f0, 0(a0)",
    "   fsd     f1, 8(a0)",
    "   fsd     f2, 16(a0)",
    "   fsd     f3, 24(a0)",
    "   fsd     f4, 32(a0)",
    "   fsd     f5, 40(a0)",
    "   fsd     f6, 48(a0)",
    "   fsd     f7, 56(a0)",
    "   fsd     f8, 64(a0)",
    "   fsd     f9, 72(a0)",
    "   fsd     f10, 80(a0)",
    "   fsd     f11, 88(a0)",
    "   fsd     f12, 96(a0)",
    "   fsd     f13, 104(a0)",
    "   fsd     f14, 112(a0)",
    "   fsd     f15, 120(a0)",
    "   fsd     f16, 128(a0)",
    "   fsd     f17, 136(a0)",
    "   fsd     f18, 144(a0)",
    "   fsd     f19, 152(a0)",
    "   fsd     f20, 160(a0)",
    "   fsd     f21, 168(a0)",
    "   fsd     f22, 176(a0)",
    "   fsd     f23, 184(a0)",
    "   fsd     f24, 192(a0)",
    "   fsd     f25, 200(a0)",
    "   fsd     f26, 208(a0)",
    "   fsd     f27, 216(a0)",
    "   fsd     f28, 224(a0)",
    "   fsd     f29, 232(a0)",
    "   fsd     f30, 240(a0)",
    "   fsd     f31, 248(a0)",
    "   frcsr   t0",
    "   sw      t0, 0(a1)",
    "   .option arch, -d",
    "   ret",
    ".globl minix_rv_fpu_restore",
    "minix_rv_fpu_restore:",        // a0 = src ptr, a1 = fcsr
    "   .option arch, +d",
    "   li      t2, 0x2000",
    "   csrs    sstatus, t2",
    "   fld     f0, 0(a0)",
    "   fld     f1, 8(a0)",
    "   fld     f2, 16(a0)",
    "   fld     f3, 24(a0)",
    "   fld     f4, 32(a0)",
    "   fld     f5, 40(a0)",
    "   fld     f6, 48(a0)",
    "   fld     f7, 56(a0)",
    "   fld     f8, 64(a0)",
    "   fld     f9, 72(a0)",
    "   fld     f10, 80(a0)",
    "   fld     f11, 88(a0)",
    "   fld     f12, 96(a0)",
    "   fld     f13, 104(a0)",
    "   fld     f14, 112(a0)",
    "   fld     f15, 120(a0)",
    "   fld     f16, 128(a0)",
    "   fld     f17, 136(a0)",
    "   fld     f18, 144(a0)",
    "   fld     f19, 152(a0)",
    "   fld     f20, 160(a0)",
    "   fld     f21, 168(a0)",
    "   fld     f22, 176(a0)",
    "   fld     f23, 184(a0)",
    "   fld     f24, 192(a0)",
    "   fld     f25, 200(a0)",
    "   fld     f26, 208(a0)",
    "   fld     f27, 216(a0)",
    "   fld     f28, 224(a0)",
    "   fld     f29, 232(a0)",
    "   fld     f30, 240(a0)",
    "   fld     f31, 248(a0)",
    "   fscsr   a1",
    "   .option arch, -d",
    "   ret",
}

unsafe extern "C" {
    fn minix_rv_fpu_save(ptr: *mut u64, fcsr_out: &mut u32);
    fn minix_rv_fpu_restore(ptr: *const u64, fcsr: u32);
}

impl FpuArch for Riscv64FpuArch {
    type State = Riscv64FpuState;

    fn init(&self) {
        // NK4-C 续-133：懒门控默认 FS = Off (0b00)——用户首条 FP 指令
        // 陷阱（scause=2 illegal-instruction）→ riscv64_fpu_trap_body
        // 轮转归属（C copr_not_available_handler 对位；归属先于任何活
        // 值建立）。FS=Off 同时陷阱 S 模式——内核必须真无 FP（xtask
        // kernel-image 注入 `-C target-feature=-f,-d`）；fpu.rs 原语经
        // 自启用序言（csrs FS=Initial）与 per-function target_feature
        // 合法执行。
        // C: No Minix3 equivalent; based on RISC-V Privileged ISA v1.11
        unsafe {
            // sstatus.FS is bits 13:14. Set to 0b00 (Off).
            let mut sstatus: u64;
            core::arch::asm!("csrr {}, sstatus", out(reg) sstatus);
            sstatus &= !(0b11 << 13); // Clear FS → Off
            core::arch::asm!("csrw sstatus, {}", in(reg) sstatus);
        }
    }

    fn save(&self, dst: &mut Self::State) {
        // Save FPU state: 32 f registers + FCSR.
        //
        // 续-133：实现在 global_asm（riscv 的 target_feature 门 stable
        // 不可用）；序言 `csrs` 把 FS 提到 ≥Initial——FS=Off 下 fsd 本
        // 身会陷阱（C 的 disable_fpu_exception-then-fxsave 对位）。
        //
        // SAFETY: `dst` is 8-byte aligned. Caller must hold BKL.
        unsafe {
            let ptr = dst.regs.as_mut_ptr() as *mut u64;
            minix_rv_fpu_save(ptr, &mut dst.fcsr);
        }
    }

    fn restore(&self, src: &Self::State) {
        // Restore FPU state: 32 f registers + FCSR.
        //
        // 续-133：实现在 global_asm（`minix_rv_fpu_restore`），一次
        // fld 覆写全部 32 个 f 寄存器=违反 extern "C" 的 fs0-fs11
        // callee-saved 保留约定——安全前提＝内核全 `-f,-d` 编译、任何
        // 调用点无存活 FP 值（终版 kernel ELF FP=0 实证）；论证全文在
        // global_asm 头注释。
        //
        // SAFETY: `src` is 8-byte aligned and contains valid FPU data.
        // Caller must hold BKL.
        unsafe {
            let ptr = src.regs.as_ptr() as *const u64;
            minix_rv_fpu_restore(ptr, src.fcsr);
        }
    }

    fn enable(&self) {
        // Set sstatus.FS = Initial (0b01) to enable FPU.
        unsafe {
            let mut sstatus: u64;
            core::arch::asm!("csrr {}, sstatus", out(reg) sstatus);
            sstatus &= !(0b11 << 13);
            sstatus |= 0b01 << 13;
            core::arch::asm!("csrw sstatus, {}", in(reg) sstatus);
        }
    }

    fn disable(&self) {
        // Set sstatus.FS = Off (0b00) to trap on next FPU instruction.
        unsafe {
            let mut sstatus: u64;
            core::arch::asm!("csrr {}, sstatus", out(reg) sstatus);
            sstatus &= !(0b11 << 13); // Clear FS → Off
            core::arch::asm!("csrw sstatus, {}", in(reg) sstatus);
        }
    }

    fn disable_exception(&self) {
        // RISC-V does not have a separate FPU exception enable/disable.
        // FPU exceptions are controlled via FCSR flags, not sstatus.
        // No-op.
    }

    fn is_present(&self) -> bool {
        // Check if the F extension is present by reading misa.
        // If misa has bit 5 (F) set, the FPU is present.
        // On RISC-V 64-bit targets compiled with `riscv64gc`, the FPU
        // is always present.
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fpu_state_size() {
        // 32 * 8 (regs) + 4 (fcsr) + 4 (pad) = 264
        assert_eq!(core::mem::size_of::<Riscv64FpuState>(), FPU_SIZE);
    }

    #[test]
    fn test_fpu_state_alignment() {
        assert_eq!(core::mem::align_of::<Riscv64FpuState>(), 8);
    }

    #[test]
    fn test_fpu_state_default_is_zeroed() {
        let state = Riscv64FpuState::default();
        assert_eq!(state.fcsr, 0);
        assert!(state.regs.iter().all(|&r| r == 0));
    }
}
