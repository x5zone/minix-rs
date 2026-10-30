//! Pipe file server entry point.
//!
//! C: `main` — `minix3/minix/fs/pfs/pfs.c:422-451` (`env_setargs` +
//! `sef_local_startup` + `fsdriver_task(&pfs_table)`). The common runtime
//! shape lives in `minix-fs-rt`; this entry contributes the server value
//! (in-memory table, no block device — PFS never touches a disk) and the
//! two hooks.
//!
//! Deliberate stand-in, honest rather than fake: the signal hook ignores
//! notifications for now. The C handler's termination path needs the
//! signal-manager pull channel whose kernel wrappers are not wired yet;
//! the decision point stays in one hook. PFS has no block-device face at
//! all, so the E-FSBDEV stand-in does not apply here.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// In test builds, use the system allocator (the crate is no_std in
// production; the test harness allocates before main() runs).
#[cfg(test)]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;


// The freestanding side holds the minix-rt link reference: the bin code
// itself uses no symbol from rt, so an unreferenced rlib is dropped
// wholesale, and `_start`/`#[panic_handler]`/`#[global_allocator]`
// disappear with it (the `extern crate` retention semantics of the
// panic-halt pattern). The birth chain's `_start` has already completed
// rt initialization before main (the `lib.rs:86` init contract: in no_std
// mode `_start` calls it automatically and is idempotent), so calling it
// here is neither needed nor appropriate.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

// `Box::new` is used bare in the hooks: the none side gets it from alloc
// (host/test builds go through the std prelude).
#[cfg(all(not(test), target_os = "none"))]
extern crate alloc;

#[cfg(all(not(test), target_os = "none"))]
use alloc::boxed::Box;

// The entry splits by target: the none side satisfies the crt0 Consumer
// contract (name + Rust ABI + `-> i32`, `os/libs/minix-rt/src/crt0.rs:38`);
// the host/test side keeps `()` — since rustc 1.94 `Termination` is no
// longer implemented for i32, and a host i32 main is E0277 (measured in
// docker minix-ci:1.94).
#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    // real_main consumes the return in place; 0 is the process exit code
    // (the crt0 Consumer contract).
    real_main();
    0
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    real_main()
}

fn real_main() {
    // In test builds, skip the binary entirely (the test harness drives the
    // library directly).
    #[cfg(not(test))]
    {
        let server = minix_fs_pfs::init();

        // Migration: switch the global heap supply to the VM_BRK growth
        // heap (the same method DS, SCHED, MIB and VFS use). init has
        // already built the inode table (still in the fixed pool); growth
        // allocations from here on go through VM_BRK. The install point is
        // before serve, by which time the VM server (boot order position
        // 2, ahead of PFS at position 8) is already receiving requests in
        // its main loop; EINVAL fails fast; the whole block is
        // #[cfg(target_os="none")] and does not affect host tests.
        #[cfg(target_os = "none")]
        minix_rt::install_vm_supplier(minix_rt::image_end())
            .expect("pfs: install_vm_supplier(镜像尾) 不得失败");

        #[cfg(target_os = "none")]
        {
            let probe = minix_rt::alloc(8);
            assert!(!probe.is_null(), "pfs: vm-brk probe 分配失败");
            minix_rt::free(probe);
            use minix_sys::syscall::{DirectKernelCallTransport, sys_diagctl_write};
            #[allow(clippy::let_underscore_must_use)] // [SD19-gate] park as the fallback when the report channel itself is broken (panic.c:66)
            let _ = sys_diagctl_write(
                &DirectKernelCallTransport,
                "nk4a: pfs-vmbrk probe ok pages=1 via=VM_BRK\n",
            );
        }

        // Birth: the C fresh-install sequence builds the inode table inside
        // the server value (pfs.c init half); the callback confirms ready.
        let hooks = minix_fs_rt::transport::ServerHooks {
            // C `pfs_signal` (`pfs.c:381-388`, registered `pfs.c:416`):
            // SIGTERM terminates without a sync, anything else is ignored.
            on_signal: Box::new(minix_fs_pfs::signal_action),
            init: Box::new(|_| Ok(())),
        };
        match minix_fs_rt::serve(server, hooks) {
            Ok(()) => {}
            Err(e) => panic!("pfs: startup failed: {e}"),
        }
    }
}
