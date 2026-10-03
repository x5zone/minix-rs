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

// NK4-C 1.10g 仪器化：格式化 panic 的 String 载荷 downcast 需要 alloc
// crate 本体（本地 `mod alloc` 是 slab 分配器，名字被遮蔽）。
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
    let ptr = with_global_allocator(|allocator| allocator.alloc(size));
    // NK4-C 1.5c 取证（task1-close 裁决删除）：slab 分配失败锁定是否
    // 落在用 minix-rt slab 的服务器（RS 等）。无分配：定长标签 + hex size。
    #[cfg(all(not(feature = "mock"), not(test)))]
    if ptr.is_null() {
        // NK4-C 1.5c 取证：OOM 后上层若有重试会刷屏，与同 crate
        // `nk4a_supply_log` 一致地封顶（task1-close 裁决删除）。
        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
        static N: AtomicUsize = AtomicUsize::new(0);
        if N.fetch_add(1, AtomicOrd::Relaxed) < 32 {
            nk4c_oom_tag(size);
        }
    }
    ptr
}

/// NK4-C 1.5c 取证（task1-close 裁决删除）：无分配的 slab OOM 行打印。
#[cfg(all(not(feature = "mock"), not(test)))]
fn nk4c_oom_tag(size: usize) {
    use minix_sys::syscall::{DirectKernelCallTransport, sys_diagctl_write};
    const HEX: &[u8; 16] = b"0123456789abcdef";
    // 取当前分配器快照（diag 不分配，安全）。
    let d = crate::global_alloc_diag();
    let mut buf = [0u8; 96];
    let mut n = 0usize;
    macro_rules! lit {
        ($s:expr) => {{
            for &b in $s {
                buf[n] = b;
                n += 1;
            }
        }};
    }
    macro_rules! hex {
        ($v:expr, $digits:expr) => {{
            let v = $v;
            for i in (0..$digits).rev() {
                buf[n] = HEX[(v >> (i * 4)) & 0xf];
                n += 1;
            }
        }};
    }
    lit!(b"nk4c: OOM-RT ");
    // proc 归因（NK4C 续-279a）：本函数只能走裸 kernel call 诊断腿（minix-rt
    // 不依赖 IPC 发送通道，服务器/命令 transport 形态不同，拿不到 getpid）——
    // 归因改由内核侧完成：dispatch_diagctl 对 "nk4c: OOM-RT" 前缀行补打 caller
    // proc 号（内核腿自带 cur_nr，单点零新通道），见 §续-279a 内核侧改动。
    lit!(b" size=");
    hex!(size, 6);
    // 分母取派生常量真值（NK4-C 1.55：池 512→1024 后写死的 `/200`
    // 会打出 px=400/200 的非法形态，签名对账判据即被污染）。
    lit!(b" slabs=");
    hex!(d.slabs_in_use, 3);
    lit!(b"/");
    hex!(alloc::MAX_SLABS, 3);
    lit!(b" big=");
    hex!(d.big_in_use, 3);
    lit!(b"/");
    // 分母宽度 2→3：MAX_BIG_BLOCKS 已绑 GLOBAL_POOL_PAGES（非 x86 档 2048=0x800，
    // x86_64 档 1024=0x400，§续-279d 分档后两值并存；279c 前全架构 0x400），
    // 两位只打得出截断的 "00"——真机 NK4C 续-278 gh140 的 `big=b6/00` 实为
    // 182/1024（分母溢出使读数歧义，诊断面缺陷本笔坐实并修）。
    hex!(alloc::MAX_BIG_BLOCKS, 3);
    lit!(b" px=");
    hex!(d.pages_consumed, 3);
    lit!(b"/");
    hex!(d.total_pages, 3);
    lit!(b" fp=");
    hex!(d.free_pages, 3);
    lit!(b"/");
    hex!(alloc::GLOBAL_POOL_PAGES, 3);
    lit!(b"\n");
    let _ = sys_diagctl_write(
        &DirectKernelCallTransport,
        core::str::from_utf8(&buf[..n]).unwrap_or("nk4c: OOM-RT\n"),
    );
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
/// of `.bss` owned by the binary itself (NK4-C 1.55: was sixteen pages,
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
fn with_global_allocator<R>(
    action: impl FnOnce(&mut alloc::SlabAllocator<alloc::FixedPoolSupplier<'static>>) -> R,
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

/// NK4-C 1.5c 取证（task1-close 裁决删除）：读全局 slab 分配器状态快照，
/// 供用户态服务器（VM/RS 等）在真机采样打印 free-bytes/slab-in-use。
pub fn global_alloc_diag() -> alloc::AllocDiag {
    with_global_allocator(|allocator| allocator.diag())
}

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
    // NK4-C 1.10g 仪器化（task1-close 裁决删除）：入口零依赖直打——不经
    // Stage 1 的 format_panic_report（其 fmt 已观测到自旋）、失败可观测。
    // file 用原串、line 手工十六进制、message 原文（as_str 可得时）。
    {
        use minix_sys::syscall::{sys_diagctl_write, DirectKernelCallTransport};
        let w = |s: &str| {
            let r = sys_diagctl_write(&DirectKernelCallTransport, s);
            if r.is_err() {
                let _ = sys_diagctl_write(&DirectKernelCallTransport, "nk4a: diag-err\n");
            }
            r.is_ok()
        };
        w("nk4a: panic-enter\n");
        // 长串分块：diagctl 对 >~16B 的写会静默丢弃（s14r/w 实测
        // 12B 成功、file 路径失败），16 字节分块逐段写。
        fn w_chunked(s: &str) {
            for chunk in s.as_bytes().chunks(16) {
                let _ = core::str::from_utf8(chunk);
            }
        }
        let _ = w_chunked;
        match info.location() {
            Some(l) => {
                for chunk in l.file().as_bytes().chunks(16) {
                    // 定长栈缓冲拼前缀（panic 路径零分配）。
                    let mut line = [0u8; 32];
                    line[..9].copy_from_slice(b"nk4a: PF ");
                    line[9..9 + chunk.len()].copy_from_slice(chunk);
                    let k = 9 + chunk.len();
                    line[k] = b'\n';
                    if let Ok(cs2) = core::str::from_utf8(&line[..k + 1]) {
                        let _ = w(cs2);
                    }
                }
                let mut lb = [0u8; 12];
                let mut n = l.line();
                let mut i = lb.len();
                loop {
                    lb[i - 1] = b"0123456789abcdef"[(n & 0xf) as usize];
                    i -= 1;
                    n >>= 4;
                    if n == 0 {
                        break;
                    }
                }
                if let Ok(s) = core::str::from_utf8(&lb[i..]) {
                    let _ = w(s);
                }
                let _ = w("\n");
            }
            None => {
                let _ = w("nk4a: panic-no-loc\n");
            }
        }
        if let Some(m) = info.message().as_str() {
            for chunk in m.as_bytes().chunks(16) {
                let cs = unsafe { core::str::from_utf8_unchecked(chunk) };
                let _ = w(cs);
            }
            let _ = w("\n");
        } else {
            let _ = w("nk4a: panic-msg-nonstr\n");
            // 格式化 panic（expect/assert 带参数）的载荷是 alloc String：
            // downcast 后分块前缀打印——errno/m_type 数值即在其中。
            if let Some(p) = info.payload().downcast_ref::<alloc_crate::string::String>() {
                for chunk in p.as_bytes().chunks(16) {
                    let mut line = [0u8; 32];
                    line[..9].copy_from_slice(b"nk4a: PY ");
                    line[9..9 + chunk.len()].copy_from_slice(chunk);
                    let k = 9 + chunk.len();
                    line[k] = b'\n';
                    if let Ok(cs2) = core::str::from_utf8(&line[..k + 1]) {
                        let _ = w(cs2);
                    }
                }
            }
        }
        w("nk4a: panic-loc-done\n");
    }
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
    // Stage 2 (A-8 step 2): a registered diagnostic hook takes over ALL
    // rendering. The kernel hook (D-48) prints the C-panic format —
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
        // (SYS_DIAGCTL; the init PID-1 precedent, `os/commands/sbin/init/
        // src/main.rs`'s handler): the kernel prints on its EarlyConsole,
        // and when even the kernel is absent the transport's park is the
        // C panic.c:66 hang this handler already documents. The SpinSink
        // stays in `diag` for its staging/test consumers.
        let _ = minix_sys::syscall::sys_diagctl_write(
            &minix_sys::syscall::DirectKernelCallTransport,
            message,
        );
    }
    // Stage 3 (A-8 step 3; C panic.c:54 `_exit(1)`): terminate through the
    // process manager so PM can reap the process — and RS restart it when
    // it is a service. `exit` never returns: when the PM_EXIT send finds
    // no process manager (bare boot images), the transport-side park takes
    // over — the C ladder's final hang (panic.c:66), the same observable
    // fallback the previous spin tail provided.
    minix_sys::exit(1)
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
