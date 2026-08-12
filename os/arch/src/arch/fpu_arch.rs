//! FPU / Extended Register state abstraction.
//!
//! Defines the `FpuArch` trait for architecture-specific FPU state
//! save/restore, enable/disable, and exception control.
//!
//! # Design (16-smp.md §3.3)
//!
//! The kernel's SMP scheduling path calls `save_local_fpu` / `release_fpu`
//! during context switches (smp.rs:496-508). In the C source these are
//! inline assembly routines (`fpu.c` / `fpu_asm.S`). The Rust rewrite
//! abstracts them through `FpuArch` so kernel code has no `#[cfg(target_arch)]`.
//!
//! # Architecture mapping
//!
//! | Method | x86-64 | ARM64 | RISC-V |
//! |--------|--------|-------|--------|
//! | `init()` | CR0.MP=1, CR0.EM=0, CR0.NE=1, CR0.TS=0 | CPACR_EL1.FPEN=3 | sstatus.FS=Initial |
//! | `save(dst)` | `fxsave [dst]` (512 bytes) | `stp q0..q31` + FPSR/FPCR (528 bytes) | `frcsr` + `fsd f0..f31` (264 bytes) |
//! | `restore(src)` | `fxrstor [src]` | `ldp q0..q31` + FPSR/FPCR | `fld f0..f31` + `fscsr` |
//! | `enable()` | `clts` (clear CR0.TS) | NOP (no lazy disable on AArch64) | `csrsi sstatus.FS, Off→Initial` |
//! | `disable()` | `stts` (set CR0.TS → #NM on next FPU insn) | NOP | `csrsi sstatus.FS, Off` |
//! | `disable_exception()` | `clts` + clear CR4.OSXMMEXCPT | NOP | NOP |
//!
//! # State buffer size
//!
//! - x86-64 FXSAVE: 512 bytes (no XSAVE extensions)
//! - ARM64 FPSIMD: 528 bytes (FPSR + FPCR + 32 × 16-byte registers)
//! - RISC-V F/D: 264 bytes (FCSR + 32 × 8-byte registers)
//!
//! Each architecture defines its own `State` associated type. The kernel
//! stores this inside `KProcess` via the `CpuContext` (arch-internal).
//!
//! C: `fpu.c` / `fpu_asm.S` — `save_local_fpu()`, `restore_fpu()`,
//! `fpu_init()`, `disable_fpu_exception()`, `release_fpu()`

use minix_types::PhysBytes;

/// FPU / Extended Register state abstraction.
///
/// Provides the low-level operations needed by the kernel's SMP scheduling
/// path (`smp.rs`) and the `SYS_GETMCONTEXT` / `SYS_SETMCONTEXT` syscalls.
///
/// # Safety
///
/// `save` and `restore` issue privileged SIMD instructions. They must be
/// called with interrupts disabled (BKL held) and with a valid `State`
/// buffer that is 16-byte aligned (required by `fxsave` / `fxrstor`).
///
/// # Instance-Based design
///
/// Unlike `ClockArch`, `FpuArch` is **stateless** — the trait methods are
/// all `&self`. This is because FPU control registers (CR0.TS,
/// CPACR_EL1.FPEN) are global per-CPU, not per-instance. The instance
/// exists only so the kernel can call `CurrentFpuArch::default()` and
/// invoke methods on it (same pattern as `CurrentClockArch::new(...)`).
///
/// The `Default` bound lets the kernel construct a stateless ZST instance
/// without knowing the concrete architecture type.
pub trait FpuArch: Sized + Send + Sync + Default {
    /// Architecture-specific FPU state buffer.
    ///
    /// Must be `Default` (zero-init is safe for all three architectures:
    /// x86-64 FXSAVE of all-zeros is valid; ARM64 FPSIMD all-zeros is
    /// valid; RISC-V F/D all-zeros is valid).
    type State: Default + Copy + Send + Sync + core::fmt::Debug;

    /// Initialize the FPU for the current CPU.
    ///
    /// Called once during `bsp_finish_booting()` (kernel/src/lib.rs:1382)
    /// and during AP boot (`smp.rs::start_ap`).
    ///
    /// C: `fpu_init()` — fpu.c (x86) / fpu_asm.S (ARM)
    fn init(&self);

    /// Save current FPU state to the buffer.
    ///
    /// Called by the SMP scheduler when switching away from a process
    /// that owns the FPU (`smp.rs:496-508`).
    ///
    /// # Safety
    ///
    /// Caller must hold the BKL (interrupts disabled) and ensure `dst`
    /// is 16-byte aligned and not aliased by any other live reference.
    ///
    /// C: `save_local_fpu(p, FALSE)` — fpu.c:save_local_fpu
    fn save(&self, dst: &mut Self::State);

    /// Restore FPU state from the buffer.
    ///
    /// Called when switching to a process that previously saved its FPU
    /// state (lazy restore on first #NM trap or eager restore).
    ///
    /// # Safety
    ///
    /// Caller must hold the BKL and ensure `src` points to a valid
    /// `State` (previously saved or zero-initialized).
    ///
    /// C: `restore_fpu(p)` — fpu.c:restore_fpu
    fn restore(&self, src: &Self::State);

    /// Enable the FPU (allow FP instructions without trapping).
    ///
    /// x86-64: clears CR0.TS (`clts`).
    /// ARM64: NOP (FPEN is set once at init, no lazy disable).
    /// RISC-V: sets sstatus.FS to Initial.
    ///
    /// C: `enable_fpu()` — called after `save_local_fpu` in restore path
    fn enable(&self);

    /// Disable the FPU (trap on next FP instruction).
    ///
    /// x86-64: sets CR0.TS (`stts`) — next FP instruction triggers #NM.
    /// ARM64: NOP (uses lazy trap via CPACR_EL1.FPEN, not per-context).
    /// RISC-V: sets sstatus.FS to Off.
    ///
    /// C: `disable_fpu()` — called before `save_local_fpu` in switch path
    fn disable(&self);

    /// Disable FPU exceptions during save/restore.
    ///
    /// x86-64: clears CR4.OSXMMEXCPT bit (suppresses #XM during fxsave).
    /// ARM64: NOP (FPSIMD exceptions are managed via FPCR, not CPACR).
    /// RISC-V: NOP (no separate exception enable for F/D).
    ///
    /// C: `disable_fpu_exception()` — fpu.c:disable_fpu_exception
    fn disable_exception(&self);

    /// Check if the FPU is present on this CPU.
    ///
    /// Returns `true` after `init()` has been called successfully.
    /// Used by the kernel to set `CpuLocal::fpu_presence`.
    ///
    /// C: `fpu_present()` — fpu.c
    fn is_present(&self) -> bool;
}

// ── Mock FpuArch (for tests) ──

/// FPU state buffer for mock (zero-length; no actual state).
#[derive(Debug, Clone, Copy, Default)]
pub struct MockFpuState;

impl MockFpuState {
    /// Const-constructible zeroed state (for `const fn` table init).
    pub const fn new() -> Self {
        Self
    }
}

/// Mock FPU implementation — all operations are no-ops.
///
/// Used when `feature = "mock"` is enabled (test mode). The mock
/// `State` is a zero-sized type, so `save`/`restore` are no-ops.
#[derive(Debug, Default, Clone, Copy)]
pub struct MockFpuArch;

impl FpuArch for MockFpuArch {
    type State = MockFpuState;

    fn init(&self) {}
    fn save(&self, _dst: &mut Self::State) {}
    fn restore(&self, _src: &Self::State) {}
    fn enable(&self) {}
    fn disable(&self) {}
    fn disable_exception(&self) {}
    fn is_present(&self) -> bool { true }
}

// ── Cross-space copy helper ──

/// Copy FPU state between two process's FPU buffers via physical address.
///
/// Used by `SYS_GETMCONTEXT` / `SYS_SETMCONTEXT` to copy FPU state
/// across address spaces. The kernel resolves the physical addresses
/// via Direct Map, then performs a byte-wise copy.
///
/// # Safety
///
/// Caller must ensure `src_phys` and `dst_phys` point to valid
/// 16-byte-aligned FPU state buffers in the Direct Map region.
///
/// C: `memcpy(rp->p_seg.fpu_state, ...)` in do_mcontext.c
pub fn copy_fpu_state_phys(
    src_phys: PhysBytes,
    dst_phys: PhysBytes,
    bytes: usize,
) {
    // SAFETY: Both physical addresses are in the Direct Map region.
    // The kernel's BKL ensures no concurrent access.
    // Caller guarantees alignment and valid buffer sizes.
    unsafe {
        let src = src_phys.0 as *const u8;
        let dst = dst_phys.0 as *mut u8;
        core::ptr::copy_nonoverlapping(src, dst, bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_fpu_state_is_zst() {
        assert_eq!(core::mem::size_of::<MockFpuState>(), 0);
    }

    #[test]
    fn test_mock_fpu_init_is_noop() {
        let arch = MockFpuArch;
        arch.init();
        assert!(arch.is_present());
    }

    #[test]
    fn test_mock_fpu_save_restore_roundtrip() {
        let arch = MockFpuArch;
        let mut state = MockFpuState;
        arch.save(&mut state);
        arch.restore(&state);
    }

    #[test]
    fn test_mock_fpu_enable_disable() {
        let arch = MockFpuArch;
        arch.enable();
        arch.disable();
        arch.disable_exception();
    }
}
