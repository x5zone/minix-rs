//! Minix-RS PM entry point.
//!
//! C counterpart: `minix3/minix/servers/pm/main.c:49` (main) — boot-chain
//! details are in 01-pm-init-main.md, main-loop dispatch in 04-ipc-dispatch.md.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code itself
// touches no symbol from rt, and an unreferenced rlib is dropped wholesale,
// taking `_start`, `#[panic_handler]` and `#[global_allocator]` with it (the
// `extern crate` retention semantics of panic-halt).  The birth chain's `_start`
// has already completed rt initialization before main (`lib.rs:86` init
// contract: in no_std mode `_start` calls it automatically and idempotently), so
// it must not and need not be called again here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_pm::init::{BootParams, PmServer};

// The entry point is split by target: the none side satisfies the crt0 Consumer
// contract (name + Rust ABI + `-> i32`, os/libs/minix-rt/src/crt0.rs:40); the
// host/test side stays `()` — since rustc 1.94 nothing implements Termination
// for i32, so an i32 main on the host is E0277 (observed with docker
// minix-ci:1.94).
#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    real_main()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    real_main()
}

fn real_main() -> ! {
    // C: main.c:49-56 — main() -> sef_local_startup() -> sef_startup()
    // -> block waiting for RS's RS_INIT (sef.c:127-141) -> sef_cb_init_fresh
    // (main.c:130-243, which includes the VFS_PM_INIT handshake with VFS at
    // main.c:220-236) -> process_init's tail replying RS_INIT+result
    // (sef_init.c:113-117).
    // This tree puts the fresh body in server.init() after construction; receipt
    // of and reply to the birth request are handled by the birth arm inside the
    // run() loop.
    //
    // Real boot acquisition: the sys_getmonparams + sys_getimage + sys_hz calls
    // at C main.c:167-176/238.  Any failure halts — the C counterpart is a panic
    // ("get monitor params failed"/"couldn't get image table"), and half-true
    // data must never enter the boot contract.
    let params = BootParams::acquire_from(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|e| panic!("PM boot: kernel getinfo failed: errno {e}"));

    let mut server = PmServer::new(params);

    // C: sef_cb_init_fresh — main.c:130-243.
    server.init();

    // Migration of the global heap supply from a welded fixed pool to a growing
    // VM_BRK heap: PM is the service with the largest process-table/fork/exec
    // working set (82 Vec/Box sites inside src/, linked with alloc-global).  The
    // install point is after init() (the fresh body has built its tables, still
    // on the fixed pool) and before the `run()` main loop — by then VM (second in
    // boot order, ahead of PM in third) already has its main loop receiving
    // requests, so the direct VM_BRK inside `install_vm_supplier` gets an answer;
    // EINVAL fails fast.  Same scheme as DS/SCHED/MIB/VFS/PFS, and the whole
    // block sits behind #[cfg(target_os="none")] so host tests are unaffected.
    #[cfg(target_os = "none")]
    minix_rt::install_vm_supplier(minix_rt::image_end())
        .expect("pm: install_vm_supplier(镜像尾) 不得失败");

    #[cfg(target_os = "none")]
    {
        let probe = minix_rt::alloc(8);
        assert!(!probe.is_null(), "pm: vm-brk probe 分配失败");
        minix_rt::free(probe);
        use minix_sys::syscall::{DirectKernelCallTransport, sys_diagctl_write};
        #[allow(clippy::let_underscore_must_use)] // [SD19-gate] park is the fallback when the report channel itself is broken (panic.c:66)
        let _ = sys_diagctl_write(
            &DirectKernelCallTransport,
            "nk4a: pm-vmbrk probe ok pages=1 via=VM_BRK\n",
        );
    }

    // C: main.c:59-110 — the main loop (dispatch details belong to 04).  run()
    // diverges (init.rs:398 `-> !`), so the tail expression is real_main's
    // function tail.
    server.run()
}
