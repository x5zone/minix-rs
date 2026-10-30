//! Minix-RS Runtime Library.
//!
//! User-space runtime support for every Minix-RS user program (servers, file
//! systems, drivers, and commands share this layer). The crate covers four
//! documents of the runtime stage:
//!
//! - `01-kernel-handoff`: the kernel information page and the initial stack
//!   ([`handoff`]).
//! - `02-crt0-start`: the program entry sequence — the real birth chain
//!   (entry stub, descriptor check, runtime init, publish, main, exit)
//!   lives in [`crt0`].
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
//! The five modules have complete logic with unit tests. The birth chain
//! (`crt0::_start` → named stages → main → exit) is the no_std entry; the
//! first real-machine verification is the `test-rt-birth` QEMU run (edge
//! E1 slice 5 acceptance).
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
//! Redox's userland runtime (relibc) walks the same arc: its crt0 assembly
//! receives the startup values, a Rust `__libc_init` runs the startup lists
//! and thread setup, then `main` runs and its result goes to `exit`. The
//! Minix variant differs in its input: Linux and Redox place the argument
//! count and pointers directly on the initial stack, while Minix passes a
//! pointer to a process string descriptor plus two loader values in
//! registers, so the entry code must read the descriptor instead of parsing
//! a raw stack image.

#![cfg_attr(not(feature = "std"), no_std)]
// Silent-error-swallowing family mechanical gate (second crate, same
// treatment as minix-sys): this crate's swallowing is concentrated in the
// diagnostic chain — `core::fmt::write` into `ByteWriter` (whose `write_str`
// always returns `Ok(())` and silently truncates when full, discarding an
// `fmt::Result` that provably cannot fail) and `sys_diagctl_write` (when the
// reporting channel itself is broken there is nowhere higher to report; on
// the transport side park is the `panic.c:66` fallback). Both are registered
// under the maintain+comment tier of the disposition table; a new unexplained
// `let _ =` reports an extra hit immediately.
#![warn(clippy::let_underscore_must_use)]

// Instrumentation: downcasting the String payload of a formatted panic needs
// the alloc crate itself (the local `mod alloc` is the slab allocator and
// shadows the name).
extern crate alloc as alloc_crate;

/// Memory allocator: break management plus slab allocation (document 06).
pub mod alloc;
/// Program birth chain: entry stub, named birth stages, and the published
/// process vectors ([`crt0`]; documents 02 + 03 wiring).
pub mod crt0;
/// Diagnostic output: buffering, number formatting, panic ladder (document 07).
pub mod diag;
/// Kernel handoff: kernel information page and initial stack (document 01).
pub mod handoff;
/// Runtime initialization: kernel page query and vector install (document 03).
pub mod init;
/// Signal return trampoline: the handler-return stub whose address goes
/// into the sigaction request ([`signals`]; C libc `__sigreturn`).
pub mod signals;

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
/// pool plays the role of the initial heap: [`alloc::GLOBAL_POOL_BYTES`]
/// of `.bss` owned by the binary itself (it was sixteen pages,
/// grown through the 512- and 1024-page capacity rounds), with virtual
/// memory mapping chained behind it later.
/// Page-aligned so the same memory can later be described to the virtual
/// memory server without copying.
#[repr(align(4096))]
struct PoolStorage(core::cell::UnsafeCell<[u8; alloc::GLOBAL_POOL_BYTES]>);

// SAFETY: only touched through the global allocator functions, which assume
// a single-threaded user process (see the threading contract on `alloc`).
unsafe impl Sync for PoolStorage {}

static POOL_STORAGE: PoolStorage =
    PoolStorage(core::cell::UnsafeCell::new([0u8; alloc::GLOBAL_POOL_BYTES]));

/// Holder for the lazily created global allocator.
///
/// WHY A HAND-ROLLED TICKET: `core` has no `Once` (`std::sync::Once` is
/// std-only, and this crate compiles `no_std`), and `core::cell::OnceCell`
/// cannot serve here — initialization needs a `&mut` through a shared
/// static, which `OnceCell` never hands out. The swap-ticket below is the
/// accepted `no_std` idiom for "run once, then read": the atomic hands out
/// a single first ticket, ordering the construction against every later
/// read.
struct GlobalAllocator {
    inner: core::cell::UnsafeCell<Option<alloc::SlabAllocator<alloc::HeapSupplier<'static>>>>,
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
            let supplier = alloc::HeapSupplier::Fixed(alloc::FixedPoolSupplier::new(pool));
            *GLOBAL_ALLOCATOR.inner.get() = Some(alloc::SlabAllocator::new(supplier));
        }
    }
}

/// Runs `action` against the global allocator, creating it first if needed.
fn with_global_allocator<R>(
    action: impl FnOnce(&mut alloc::SlabAllocator<alloc::HeapSupplier<'static>>) -> R,
) -> R {
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

/// Reader of the global slab allocator's state snapshot (slabs and large
/// blocks in use, pages consumed / total / free). This is the crate's formal
/// allocator diagnostic surface rather than a boot trace: the T1 test
/// of [`install_vm_supplier`] asserts its post-condition through here, i.e.
/// that total and free page counts drop to zero once the VM page supplier is
/// installed.
pub fn global_alloc_diag() -> alloc::AllocDiag {
    with_global_allocator(|allocator| allocator.diag())
}

/// Switch the global allocator's page supply to the VM_BRK growing heap (the
/// T1 install point).
///
/// When to call: after the process has completed its birth handshake with VM
/// and obtained its own data region endpoint, and before any allocation that
/// depends on heap growth (fixed-pool allocations during birth are unaffected
/// — the switch only changes where subsequent
/// [`alloc::PageSupplier::supply_pages`] calls get new pages; see the caller
/// contract of [`alloc::SlabAllocator::replace_supplier`]).
///
/// * `initial_break`: the current endpoint of the VM-side data region
///   (counterpart of C's `_brksize` initial value = the data segment endpoint
///   established by exec; the install point is wired to the linker's
///   `__bss_end`, see the T3 install point). Passing 0 is meaningless and
///   returns `EINVAL`.
/// * Idempotent: a no-op once the VM form is active (a repeated handshake does
///   not reinstall).
/// * After returning, the total/free page counts of the pool diagnostic
///   [`global_alloc_diag`] are 0, which is the "VM form is active" marker
///   ([`alloc::VmBreakSupplier`] has no fixed total).
pub fn install_vm_supplier(initial_break: u64) -> Result<(), minix_types::Errno> {
    if initial_break == 0 {
        return Err(minix_types::Errno::EINVAL);
    }
    with_global_allocator(|allocator| {
        if allocator.is_vm_form() {
            // VM form already active: idempotent no-op (a repeated handshake
            // does not reinstall, avoiding caching initial_break back to an
            // old value).
            return Ok(());
        }
        let vm = alloc::HeapSupplier::Vm(alloc::VmBreakSupplier::new(
            alloc::DirectTrapTransport,
            initial_break,
        ));
        allocator.replace_supplier(vm);
        Ok(())
    })
}

/// The linker image-end symbol (C `_end`) and its reader (freestanding body
/// only).
///
/// C counterpart: `minix3/minix/lib/libc/arch/i386/sys/brksize.S` seeds the
/// linker `_end` as the initial `_brksize` — the first byte after the data
/// segment is the heap start. All three architectures' none targets in this
/// crate use lld, whose ELF executables provide the three symbols
/// `__bss_start`/`_edata`/`_end` built in, so taking their address suffices
/// (no custom linker script needed). The host std build does not expose them
/// (symbol semantics vary with the host libc, there are no consumers, and
/// misuse is avoided).
#[cfg(target_os = "none")]
mod image_end_sym {
    unsafe extern "C" {
        static _end: u8;
    }

    /// Image-end address (the first byte after .bss; the same source as the
    /// initial break of [`install_vm_supplier`], the T3 install point).
    pub fn image_end() -> u64 {
        // Only take the symbol address, never dereference it — `_end` is a
        // linker-builtin symbol with no backing storage object (`addr_of!` is
        // itself safe, no unsafe block needed).
        core::ptr::addr_of!(_end) as u64
    }
}
#[cfg(target_os = "none")]
pub use image_end_sym::image_end;

/// Binding of the runtime slab allocator to the `#[global_allocator]` slot
/// (opt-in: a binary providing its own global allocator must not enable
/// this feature, exactly like the `panic-handler` feature).
///
/// Alignment contract: the slab serves 8-byte-aligned blocks, so requests
/// with a stricter alignment fail honest (null) instead of misbehaving.
#[cfg(all(not(feature = "std"), feature = "alloc-global"))]
mod global_alloc_binding {
    struct RtGlobalAlloc;

    unsafe impl core::alloc::GlobalAlloc for RtGlobalAlloc {
        unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
            if layout.align() > 8 {
                return core::ptr::null_mut();
            }
            crate::alloc(layout.size())
        }

        unsafe fn dealloc(&self, ptr: *mut u8, _layout: core::alloc::Layout) {
            crate::free(ptr);
        }
    }

    #[global_allocator]
    static RT_GLOBAL_ALLOCATOR: RtGlobalAlloc = RtGlobalAlloc;
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

    /// The `install_vm_supplier` install point (T1, host-observable part):
    /// a 0 break is rejected; after installation the pool diagnostic's
    /// total/free counts drop to 0 (VM form marker); repeated installation is
    /// idempotent. Note: this test switches the global allocator to the VM form
    /// irreversibly — libtest runs tests in name order, and `test_global_*`
    /// (prefix g) come before this test (prefix i); keep this ordering when
    /// renaming tests.
    #[test]
    fn test_install_vm_supplier_guards_and_form_switch() {
        let _guard = GLOBAL_TEST_LOCK.lock().unwrap();
        assert_eq!(
            install_vm_supplier(0),
            Err(minix_types::Errno::EINVAL),
            "0 断点无意义，必须拒绝"
        );
        install_vm_supplier(0x10_0000).expect("首次安装");
        let diag = global_alloc_diag();
        assert_eq!(diag.total_pages, 0);
        assert_eq!(diag.free_pages, 0);
        install_vm_supplier(0x20_0000).expect("重复安装必须幂等");
        let diag = global_alloc_diag();
        assert_eq!(diag.total_pages, 0);
    }
}

/// Panic handler (`no_std` mode only).
///
/// In `std` mode, the std panic handler is used.
///
/// # Current behavior
///
/// Formats the panic location and message into a stack buffer, hands the
/// buffer to the diagnostic sink (a registered kernel hook when one exists,
/// the spin sink otherwise), and then terminates the process through the
/// process manager with status 1.
///
/// # Staged evolution (architecture item A-8)
///
/// 1. Spin after formatting (landed; observable behavior unchanged).
/// 2. Route the sink through the kernel diagnostic channel (landed: the
///    shared minix-types hook registry, written by the kernel at boot).
/// 3. Terminate through the process manager after emitting (landed:
///    `minix_sys::exit(1)` — the C ladder's `_exit(1)`).
#[cfg(all(not(test), not(feature = "std"), feature = "panic-handler"))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Stage 1 (C: panic.c:34-46, message-then-newline): format the location
    // and message into stack memory through the crate's single formatting
    // home (see `diag::format_panic_report`). Stack memory only: no
    // allocator, no syscalls, safe to run from any state.
    let mut buffer = [0u8; 256];
    let length = diag::format_panic_report(
        info.location()
            .map(|location| (location.file(), location.line())),
        info.message(),
        &mut buffer,
    );
    // Stage 2: a registered diagnostic hook takes over ALL
    // rendering. The kernel hook prints the C-panic format —
    // "kernel panic: " + message + "kernel on CPU %d: " + backtrace —
    // through the kernel EarlyConsole. The hook registry lives in
    // minix-types, the shared contract crate both sides already depend
    // on: the kernel registers at boot, this handler consults here.
    let message = core::str::from_utf8(&buffer[..length]).unwrap_or("panicked (non-utf8 message)");
    if !minix_types::run_panic_diagnostic_hook(message) {
        // No hook registered in THIS address space. The registry is a
        // per-address-space static: the kernel registers in its own space,
        // so a user process (or any non-kernel binary) never observes that
        // registration here. The previous fallback — the SpinSink — hung
        // the process at exactly this point, which meant no panic message
        // was ever visible from user mode and the stage-3 exit below was
        // unreachable. Emit through the kernel diagnostic channel instead
        // (SYS_DIAGCTL; the init process (pid 1) precedent, `os/commands/sbin/init/
        // src/main.rs`'s handler): the kernel prints on its EarlyConsole,
        // and when even the kernel is absent the transport's park is the
        // C panic.c:66 hang this handler already documents. The SpinSink
        // stays in `diag` for its staging/test consumers.
        let _ = minix_sys::syscall::sys_diagctl_write(
            &minix_sys::syscall::DirectKernelCallTransport,
            message,
        );
    }
    // Stage 3 (C panic.c:54 `_exit(1)`): terminate through the
    // process manager so PM can reap the process — and RS restart it when
    // it is a service. `exit` never returns: when the PM_EXIT send finds
    // no process manager (bare boot images), the transport-side park takes
    // over — the C ladder's final hang (panic.c:66), the same observable
    // fallback the previous spin tail provided.
    minix_sys::exit(1)
}

// ── Panic diagnostic hook contract ──────────────────────────────────────
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
