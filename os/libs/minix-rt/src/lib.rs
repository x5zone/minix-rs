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
//! The remaining runtime concern (system call wrappers) lives in a later
//! stage document and keeps its existing placeholder implementation until
//! its own document lands.
//!
//! # Crate status
//!
//! The five modules have complete logic with unit tests. The function below
//! remains a placeholder with well-defined behavior (no silent failures):
//!
//! - `_start()` — calls `init`, then `main`, then `minix_sys::exit`.
//!
//! The `panic` handler formats the location and message into a stack buffer
//! and emits it through the diagnostic sink (see [`diag`]); the default sink
//! spins, preserving the previous observable behavior.
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
/// Diagnostic output: buffering, number formatting, panic ladder (document 07).
pub mod diag;

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
/// Formats the panic location and message into a stack buffer, hands the
/// buffer to the diagnostic sink, and then stops. The default sink spins
/// forever: the safest minimal behavior for a freestanding binary, requiring
/// no subsystem (no console, no allocator, no syscalls) and halting forward
/// progress deterministically.
///
/// # Staged evolution (architecture item A-8)
///
/// 1. Spin after formatting (current step; observable behavior unchanged).
/// 2. Route the sink through the kernel diagnostic channel.
/// 3. Terminate through the process manager after emitting.
#[cfg(all(not(test), not(feature = "std"), feature = "panic-handler"))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Stage 1 (C: panic.c:34-46, message-then-newline): format the location
    // and message into stack memory through the crate's single formatting
    // home (see `diag::format_panic_report`). Stack memory only: no
    // allocator, no syscalls, safe to run from any state.
    let mut buffer = [0u8; 256];
    let length = diag::format_panic_report(
        info.location().map(|location| (location.file(), location.line())),
        info.message(),
        &mut buffer,
    );
    // Stage 2 (A-8 step 2): a registered diagnostic hook takes over ALL
    // rendering. The kernel hook (D-48) prints the C-panic format —
    // "kernel panic: " + message + "kernel on CPU %d: " + backtrace —
    // through the kernel EarlyConsole. The hook registry lives in
    // minix-types, the shared contract crate both sides already depend
    // on: the kernel registers at boot, this handler consults here.
    let message = core::str::from_utf8(&buffer[..length]).unwrap_or("panicked (non-utf8 message)");
    if !minix_types::run_panic_diagnostic_hook(message) {
        // No hook registered (pre-registration panics, or binaries
        // without a kernel): stage-1 emit through the default sink.
        let mut sink = diag::SpinSink;
        diag::DiagnosticSink::emit(&mut sink, &buffer[..length]);
    }
    loop {
        core::hint::spin_loop();
    }
}

// ── Panic diagnostic hook contract (D-48, A-8 step 2) ───────────────────
//
// The hook registry itself lives in minix-types (`minix_types::
// set_panic_diagnostic_hook` / `run_panic_diagnostic_hook`): one registry,
// written by the kernel at boot and read by this crate's panic handler.
// The no-hook fallback path (spin sink) is intentionally untestable
// in-process — spinning is its contract.

#[cfg(test)]
mod panic_diagnostic_tests {
    use core::sync::atomic::{AtomicUsize, Ordering};

    static HOOK_INVOCATIONS: AtomicUsize = AtomicUsize::new(0);

    fn counting_hook(_message: &str) {
        HOOK_INVOCATIONS.fetch_add(1, Ordering::AcqRel);
    }

    /// Consumer-side contract pin: the panic handler in no_std builds
    /// consults the shared minix-types registry — the same one the kernel
    /// registers into at boot. A hook registered through the shared API
    /// must be visible to the shared run helper, and clearing it must
    /// restore the "no hook" answer. Workspace forces RUST_TEST_THREADS=1,
    /// so the shared static slot is serial.
    #[test]
    fn panic_hook_contract_through_shared_registry() {
        minix_types::set_panic_diagnostic_hook(None);
        assert!(!minix_types::run_panic_diagnostic_hook("no hook yet"));
        assert_eq!(HOOK_INVOCATIONS.load(Ordering::Acquire), 0);

        minix_types::set_panic_diagnostic_hook(Some(counting_hook));
        assert!(minix_types::run_panic_diagnostic_hook("panic path message"));
        assert_eq!(HOOK_INVOCATIONS.load(Ordering::Acquire), 1);

        minix_types::set_panic_diagnostic_hook(None);
        assert!(!minix_types::run_panic_diagnostic_hook("cleared again"));
        assert_eq!(HOOK_INVOCATIONS.load(Ordering::Acquire), 1);
    }
}
