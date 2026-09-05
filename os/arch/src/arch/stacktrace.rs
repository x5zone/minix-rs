//! Stack trace abstraction.
//!
//! Defines the arch-level trait for walking a process's user-space stack
//! and emitting a backtrace via the early console.
//!
//! # C Reference
//!
//! C: `proc_stacktrace()` — arch/i386/exception.c:333-373.
//! Gated by `USE_SYSDEBUG` in C (compile-time opt-in). The Rust equivalent
//! is always available but emits a "no debug info" message if the arch
//! cannot unwind (e.g., frame pointer omitted).
//!
//! # Why a trait (not `#[cfg(target_arch)]`)
//!
//! Stack unwinding is fundamentally architecture-specific:
//! - x86_64: walks `rbp`-linked list, 16-byte stack slots
//! - aarch64: walks `x29` (FP)-linked list, 16-byte stack slots
//! - riscv64: walks `s0` (FP)-linked list, 8-byte stack slots
//!
//! Pattern 14 (hardware abstracted as trait): no `#[cfg(target_arch)]` for
//! behavior selection.

use crate::arch::boot::CpuContextArch;

/// Maximum number of stack frames to emit before giving up.
///
/// C: `proc_stacktrace_execute` has no explicit limit but relies on
/// `printf` and the console being writable. Rust caps at 32 to bound
/// kernel time spent in the diagnostic path.
pub const MAX_STACK_FRAMES: usize = 32;

/// Arch trait for emitting a stack backtrace of a process.
///
/// Each architecture implements `walk_frames` to traverse the process's
/// user-space stack starting from its saved register state, calling
/// `emit` for each discovered frame's PC.
///
/// The trait method takes a callback rather than returning a `Vec<u64>`
/// because the kernel is `no_std` and the diagnostic path should not
/// allocate.
pub trait StacktraceArch: CpuContextArch {
    /// Walk the stack frames of a process and emit each frame's PC via
    /// the provided callback.
    ///
    /// `cpu_context` is the target process's saved CPU context (register
    /// state at the time of the last trap). `read_word` is a closure that
    /// reads a 64-bit word from the process's user address space at the
    /// given virtual address, returning `None` on fault.
    ///
    /// The default implementation walks the frame-pointer chain (used by
    /// x86_64, aarch64, and riscv64 when frame pointers are enabled).
    /// Architectures with a different unwind mechanism override this.
    ///
    /// # Safety
    ///
    /// `read_word` must safely read from user space (the caller is
    /// responsible for bounds checking / page-table walking).
    fn walk_frames<F>(
        cpu_context: &Self::CpuContext,
        read_word: impl Fn(u64) -> Option<u64>,
        mut emit: F,
    )
    where
        F: FnMut(u64),
    {
        // Default frame-pointer walk: fp → [saved_fp, return_addr]
        // Works for x86_64 (rbp), aarch64 (x29), riscv64 (s0) when
        // frame pointers are enabled.
        let pc = Self::program_counter(cpu_context);
        emit(pc);
        // The leading pc counts toward MAX_STACK_FRAMES (total-emit cap,
        // preserved by test_stacktrace_walk_frames_caps_at_max).
        Self::walk_frames_from(read_word, emit, Self::frame_pointer(cpu_context), 1);
    }

    /// Walk the frame-pointer chain starting at `fp` (without emitting a
    /// leading current-PC). Shared by [`Self::walk_frames`] (saved
    /// context) and the kernel self-trace `util_stacktrace` (D-47, C
    /// libsys/stacktrace.c:17-37 — `bp = get_bp()` then walk `bp[1]`).
    ///
    /// `already_emitted` counts frames already emitted by the caller so
    /// the MAX_STACK_FRAMES total-emit cap holds across the shared loop.
    ///
    /// Frame layout: `[saved_fp, return_addr]` — x86_64 (rbp) and
    /// aarch64 (x29) share it; riscv64 stores `ra` at `fp-8` and its
    /// impl overrides `walk_frames` accordingly.
    fn walk_frames_from<F>(
        read_word: impl Fn(u64) -> Option<u64>,
        mut emit: F,
        mut fp: u64,
        already_emitted: usize,
    ) where
        F: FnMut(u64),
    {
        let mut count = already_emitted;
        while fp != 0 && count < MAX_STACK_FRAMES {
            // Frame layout: [saved_fp, return_addr]
            // saved_fp at fp+0, return_addr at fp+8
            let saved_fp = match read_word(fp) {
                Some(v) => v,
                None => break, // unmapped/unreadable — stop
            };
            let return_addr = match read_word(fp + 8) {
                Some(v) => v,
                None => break,
            };

            if return_addr == 0 {
                break;
            }

            emit(return_addr);
            count += 1;

            // Sanity: stop if fp doesn't advance (cycle/corruption).
            if saved_fp <= fp {
                break;
            }
            fp = saved_fp;
        }
    }

    /// Read the CURRENT frame pointer for a kernel self-backtrace
    /// (`util_stacktrace` — no process context; C libsys/stacktrace.c
    /// `get_bp()`). Requires arch asm and frame pointers enabled at
    /// compile time.
    ///
    /// Default: unsupported (`None`) — the diagnostic caller prints a
    /// placeholder and stops. C ships this utility for i386 only; the
    /// x86_64 implementation overrides it. `[ARCH: scope]` — aarch64
    /// (`x29`) / riscv64 (`s0`) can be added with one `asm!` each when
    /// their kernel self-trace is needed.
    fn current_frame_pointer() -> Option<u64> {
        None
    }

    /// Extract the frame pointer register from a CpuContext.
    ///
    /// - x86_64: `rbp` (gp_regs[5], GP_RBP)
    /// - aarch64: `x29` (gp_regs[28], X29 — layout skips X0; see arm64/boot.rs)
    /// - riscv64: `s0`/`fp` (gp_regs[6], GP_S0 — layout skips X0/X2/X10)
    fn frame_pointer(cpu_context: &Self::CpuContext) -> u64;

    /// Extract the program counter from a CpuContext.
    fn program_counter(cpu_context: &Self::CpuContext) -> u64;
}
