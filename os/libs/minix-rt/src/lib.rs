//! Minix-RS Runtime Library.
//!
//! User-space runtime support for every Minix-RS user program (servers, file
//! systems, drivers, and commands share this layer). The crate covers four
//! documents of the runtime stage:
//!
//! - `01-kernel-handoff`: the kernel information page and the initial stack
//!   ([`handoff`]).
//! - `02-crt0-start`: the program entry sequence from the first instruction
//!   to the `main` call ([`start`]).
//! - `03-runtime-init`: publishing the kernel information page and the
//!   communication vector table ([`init`]).
//! - `06-allocator`: break management and slab allocation ([`alloc`]).
//!
//! The remaining runtime concerns (panic output, system call wrappers) live
//! in later stage documents and keep their existing placeholder
//! implementations until their own documents land.
//!
//! # Crate status
//!
//! The four modules have complete logic with unit tests. The functions below
//! remain placeholders with well-defined behavior (no silent failures):
//!
//! - `_start()` — calls `init`, then `main`, then `minix_sys::exit`.
//! - `panic` handler — loops forever. Will print to standard error via
//!   `minix_sys::write` once that system call lands.
//!
//! `init()` now initializes the global allocator (idempotent). It will
//! delegate to [`init::initialize_runtime`] once the communication trap is
//! wired.
//!
//! # Standard library versus freestanding builds
//!
//! With the default `std` feature, this crate compiles as a normal standard
//! library crate: `_start` is **not** defined (the standard runtime provides
//! its own), and the panic handler is **not** defined (the standard library
//! provides one). Only `init`, `alloc`, `free`, and the three new modules
//! are exported.
//!
//! Without the `std` feature (`--no-default-features`), the crate is
//! `#![no_std]` and provides `_start` plus a panic handler suitable for
//! linking into a freestanding Minix-RS user process.
//!
//! # Relation to Redox
//!
//! Redox ships the same shape in its `linker` crate: parse the startup
//! information the kernel left behind, run the startup function lists, call
//! `main`, pass the result to `exit`. The Minix variant differs in its input:
//! Linux and Redox place the argument count and pointers directly on the
//! initial stack, while Minix passes a pointer to a process string
//! descriptor plus two loader values in registers. The [`start`] module
//! therefore takes the descriptor as its input rather than re-parsing a raw
//! stack image.

#![cfg_attr(not(feature = "std"), no_std)]

/// Kernel handoff: kernel information page and initial stack (document 01).
pub mod handoff;
/// Program entry: descriptor check through the `main` call (document 02).
pub mod start;
/// Runtime initialization: kernel page query and vector install (document 03).
pub mod init;
/// Memory allocator: break management plus slab allocation (document 06).
pub mod alloc;

#[cfg(not(feature = "std"))]
use core::panic::PanicInfo;

/// Runtime initialization.
///
/// Performs any one-time setup required before `main` runs.
///
/// # Current behavior
///
/// Initializes the global allocator (idempotent: later calls do nothing).
/// Future extensions (in order of dependency):
/// 1. Set up thread-local storage (when symmetric multiprocessing user-space lands).
/// 2. Install default signal handlers (when `minix_sys::sigaction` lands).
///
/// # When to call
///
/// - In `no_std` mode: called automatically by `_start` before `main`.
/// - In `std` mode: caller must invoke explicitly (typically the first
///   line of `main`).
pub fn init() {
    ensure_global_allocator();
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
/// Served by the global slab allocator (see [`alloc`]): small objects come
/// from size-class slabs, large objects from whole page runs, all currently
/// supplied by an embedded static pool. Returns null when the pool is
/// exhausted or when `size` is zero — fail-fast rather than faulting later.
///
/// # Alignment
///
/// The returned pointer is guaranteed to be eight-byte aligned.
///
/// # Threading contract
///
/// Assumes a single-threaded user process (same assumption as the rest of
/// the startup path). Thread support will revisit this contract.
pub fn alloc(size: usize) -> *mut u8 {
    with_global_allocator(|allocator| allocator.alloc(size))
}

/// Frees memory previously allocated by [`alloc`].
///
/// Returns the block to the global slab allocator. Passing null is allowed
/// and does nothing (matches C `free(NULL)`).
///
/// # Safety contract
///
/// - `ptr` must be either null or a pointer previously returned by `alloc`
///   on this same process image.
/// - A pointer that was already freed, or that never came from `alloc`,
///   stops the process with a panic instead of corrupting the heap.
pub fn free(ptr: *mut u8) {
    with_global_allocator(|allocator| allocator.free(ptr))
}

/// The C heap starts at the linker-provided `_end` symbol and grows through
/// the virtual memory server. Until that server channel lands, this embedded
/// pool plays the role of the initial heap: sixteen pages owned by the
/// binary itself, with virtual memory mapping chained behind it later.
/// Page-aligned so the same memory can later be described to the virtual
/// memory server without copying.
#[repr(align(4096))]
struct PoolStorage(core::cell::UnsafeCell<[u8; alloc::GLOBAL_POOL_BYTES]>);

// SAFETY: only touched through the global allocator functions, which assume
// a single-threaded user process (see the threading contract on `alloc`).
unsafe impl Sync for PoolStorage {}

static POOL_STORAGE: PoolStorage = PoolStorage(core::cell::UnsafeCell::new(
    [0u8; alloc::GLOBAL_POOL_BYTES],
));

/// Holder for the lazily created global allocator.
struct GlobalAllocator {
    inner: core::cell::UnsafeCell<Option<alloc::SlabAllocator<alloc::FixedPoolSupplier<'static>>>>,
    ready: core::sync::atomic::AtomicBool,
}

impl GlobalAllocator {
    const fn new() -> Self {
        GlobalAllocator {
            inner: core::cell::UnsafeCell::new(None),
            ready: core::sync::atomic::AtomicBool::new(false),
        }
    }
}

// SAFETY: same single-threaded contract as PoolStorage; the `ready` flag is
// an atomic so initialization itself is ordered.
unsafe impl Sync for GlobalAllocator {}

static GLOBAL_ALLOCATOR: GlobalAllocator = GlobalAllocator::new();

/// Creates the global allocator on first use (idempotent).
fn ensure_global_allocator() {
    if !GLOBAL_ALLOCATOR
        .ready
        .swap(true, core::sync::atomic::Ordering::SeqCst)
    {
        // SAFETY: this branch runs exactly once (the swap above hands out a
        // single "first" ticket), and no other code touches the pool before
        // the flag is set, so exclusive access holds.
        unsafe {
            let pool = &mut *POOL_STORAGE.0.get();
            let supplier = alloc::FixedPoolSupplier::new(pool);
            *GLOBAL_ALLOCATOR.inner.get() = Some(alloc::SlabAllocator::new(supplier));
        }
    }
}

/// Runs `action` against the global allocator, creating it first if needed.
fn with_global_allocator<R>(action: impl FnOnce(&mut alloc::SlabAllocator<alloc::FixedPoolSupplier<'static>>) -> R) -> R {
    ensure_global_allocator();
    // SAFETY: creation above precedes this read (same SeqCst ordering), and
    // single-threaded use rules out concurrent access.
    unsafe {
        let allocator = (*GLOBAL_ALLOCATOR.inner.get())
            .as_mut()
            .expect("global allocator is ready after ensure");
        action(allocator)
    }
}

#[cfg(test)]
mod global_tests {
    use super::*;

    // Serializes the global-allocator tests: they share one static pool, so
    // they must not interleave. Owned-allocator tests in `alloc.rs` never
    // touch the globals and run freely in parallel.
    static GLOBAL_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_global_alloc_free_round_trip() {
        let _guard = GLOBAL_TEST_LOCK.lock().unwrap();
        let pointer = alloc(64);
        assert!(!pointer.is_null());
        assert_eq!(pointer as usize % 8, 0);
        // SAFETY: 64 bytes were just handed out to this test.
        unsafe {
            core::ptr::write_bytes(pointer, 0x5A, 64);
            assert_eq!(core::ptr::read(pointer), 0x5A);
        }
        free(pointer);
    }

    #[test]
    fn test_global_zero_request_returns_null() {
        let _guard = GLOBAL_TEST_LOCK.lock().unwrap();
        assert!(alloc(0).is_null());
    }

    #[test]
    fn test_global_init_is_idempotent() {
        let _guard = GLOBAL_TEST_LOCK.lock().unwrap();
        init();
        init();
        assert!(!alloc(8).is_null());
    }
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
