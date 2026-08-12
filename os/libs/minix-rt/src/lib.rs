//! Minix-RS Runtime Library.
//!
//! User-space runtime support. Provides the program entry point (`_start`),
//! runtime initialization, a placeholder allocator, and a panic handler for
//! `no_std` builds.
//!
//! # Crate status
//!
//! This crate is a **minimal runtime stub**. The functions here have correct
//! signatures and well-defined behavior (no silent failures), but the actual
//! implementations are placeholders:
//!
//! - `init()` — no-op. Will set up TLS, allocator, signal handlers.
//! - `_start()` — calls `init`, then `main`, then `minix_sys::exit`.
//! - `alloc()` / `free()` — return null / no-op. Will back onto a slab
//!   allocator fed by `minix_sys::mmap` once VM IPC is wired.
//! - `panic` handler — loops forever. Will print to stderr via
//!   `minix_sys::write` once that syscall lands.
//!
//! # Std vs no_std
//!
//! With the default `std` feature, this crate compiles as a normal std
//! library: `_start` is **not** defined (the std runtime provides its own),
//! and the panic handler is **not** defined (std provides one). Only `init`,
//! `alloc`, and `free` are exported.
//!
//! Without the `std` feature (`--no-default-features`), the crate is
//! `#![no_std]` and provides `_start` + a panic handler suitable for
//! linking into a freestanding Minix-RS user process.
//!
//! # Redox comparison
//!
//! Redox OS uses the `linker` crate to provide `_start`. The pattern is
//! similar: parse argc/argv from the stack, call `main`, call `exit`.
//! Minix-RS's `_start` is intentionally simpler — it does not parse
//! argc/argv because Minix3's PM passes initial args via a different
//! mechanism (the `bootinfo` struct, mirror of the kernel's). When that
//! mechanism is wired, `_start` will read argv from `bootinfo`.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
use core::panic::PanicInfo;

/// Runtime initialization.
///
/// Performs any one-time setup required before `main` runs.
///
/// # Current behavior
///
/// No-op. Future extensions (in order of dependency):
/// 1. Initialize the global allocator (when slab allocator lands).
/// 2. Set up thread-local storage (when SMP user-space lands).
/// 3. Install default signal handlers (when `minix_sys::sigaction` lands).
///
/// # When to call
///
/// - In `no_std` mode: called automatically by `_start` before `main`.
/// - In `std` mode: caller must invoke explicitly (typically the first
///   line of `main`).
pub fn init() {
    // Intentionally empty. See doc comment above for the roadmap.
}

/// Program entry point (`no_std` mode only).
///
/// In `no_std` mode, this is the actual entry point the linker emits.
/// Calls `minix_rt::init()`, then the user-provided `main` function,
/// then exits via `minix_sys::exit`.
///
/// In `std` mode, the std runtime provides its own `_start`; this
/// function is not compiled.
///
/// # Safety
///
/// `main` is declared as an extern Rust symbol. The linker is expected
/// to provide a `main` function in the final binary (typically from the
/// `commands/` crate). If `main` is missing, linking fails — there is
/// no runtime fallback.
///
/// C: equivalent of `crt0`'s `_start` in Minix3's `lib/crtso`.
#[cfg(all(not(test), not(feature = "std")))]
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    init();

    // The user's `main` function. Declared as extern because it lives
    // in a different crate (the executable that links against minix-rt).
    // Returning `i32` matches the C convention (exit code).
    unsafe extern "Rust" {
        fn main() -> i32;
    }

    let exit_code = unsafe { main() };

    // Delegate to minix-sys for the actual exit syscall. In the current
    // stub state, `minix_sys::exit` loops forever; once the PM syscall
    // path lands, it will send an `EXIT` message to PM and never return.
    minix_sys::exit(exit_code);
}

/// Allocates `size` bytes of uninitialized memory.
///
/// Returns a pointer to the allocated memory, or null if the allocation
/// fails (including the current stub state where no allocator is wired).
///
/// # Current behavior
///
/// Returns null unconditionally. Callers that dereference the result
/// will fault — this is intentional (fail-fast) rather than returning
/// a dangling pointer.
///
/// # Future implementation
///
/// Will back onto a slab allocator that obtains pages from VM via
/// `minix_sys::mmap`. Small allocations (< page size) will be served
/// from per-CPU slab caches; large allocations will be direct `mmap`s.
///
/// # Alignment
///
/// The returned pointer is guaranteed to be aligned to `core::mem::align_of::<usize>()`.
/// (Future contract; the stub does not allocate.)
pub fn alloc(size: usize) -> *mut u8 {
    let _ = size;
    // Stub: no allocator wired. Return null so callers fail fast
    // rather than silently corrupting memory.
    core::ptr::null_mut()
}

/// Frees memory previously allocated by [`alloc`].
///
/// # Current behavior
///
/// No-op. Memory allocated by the future allocator will be returned to
/// the slab cache here.
///
/// # Safety contract
///
/// - `ptr` must be either null or a pointer previously returned by `alloc`.
/// - Passing null is explicitly allowed (matches C `free(NULL)`).
/// - The behavior when passing a dangling or already-freed pointer is
///   undefined (will be a panic in the future implementation).
pub fn free(ptr: *mut u8) {
    let _ = ptr;
    // Stub: no-op. Future implementation will return memory to the slab.
}

/// Panic handler (`no_std` mode only).
///
/// In `std` mode, the std panic handler is used.
///
/// # Current behavior
///
/// Loops forever. This is the safest minimal behavior for a freestanding
/// binary: it does not require any subsystem (no console, no allocator,
/// no syscalls) and halts forward progress deterministically.
///
/// # Future implementation
///
/// Will:
/// 1. Format the panic message into a stack-allocated buffer.
/// 2. Call `minix_sys::write(STDERR, buf)` to print to stderr.
/// 3. Call `minix_sys::exit(1)` to terminate the process.
///
/// Until `minix_sys::write` lands, the loop is the correct behavior —
/// attempting to print without a working write syscall would itself
/// panic, recursing into this handler.
#[cfg(all(not(test), not(feature = "std")))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // Halt forever. See doc comment for the roadmap.
    loop {
        // On most architectures, a tight `core::hint::spin_loop` is
        // preferable to a tight `loop {}` because it signals "I am
        // waiting" to the CPU, reducing power consumption. In a panic
        // handler we are not waiting for anything, but `spin_loop`
        // also emits `pause`/`yield`/`nop` which is friendlier to
        // hypervisors (QEMU) than a raw busy loop.
        core::hint::spin_loop();
    }
}
