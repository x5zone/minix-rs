//! The tty driver binary.
//!
//! C: `main()` (`tty.c:146-238`) — SEF local startup (the birth handshake
//! with the restart server), then the receive-classify-dispatch loop. Both
//! halves come from the shared driver runtime; this binary only names the
//! data-store label and builds the device.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

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

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

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
    // Birth self-identification: C's `sef_startup` calls `sys_whoami`
    // first and stores the result in `sef_self_endpoint` (`sef.c:76-87`,
    // panics on failure at :81). The kernel's boot procedure grants the
    // endpoint according to `BOOT_MODULE_PROC_NRS` — RS does not allocate
    // it during boot. The answer is read from the caller's own kernel slot
    // (`misc.rs` `getinfo_whoami`) and is independent of the message's
    // `m_source`, so the query goes through the sourceless
    // `DirectKernelCallTransport` (the same one driver-rt's safecopy
    // uses), and the answer then feeds the source-carrying IPC transport.
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("tty: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    let mut runtime = DriverRuntime::new(transport, "drv.chr.tty");
    let mut service = minix_driver_tty::init();

    // Migration (the seventh server, the first driver): tty is a line
    // terminal driver (26 Vec/Box uses under src/, linked against
    // alloc-global). Switch the global heap supply from the hard-wired
    // fixed pool to the VM_BRK growth heap. The install point is after
    // service construction (the line table is already built, still in the
    // fixed pool) and before `serve`'s main loop — by then the VM server
    // (boot order position 2, ahead of tty at position 7) is already
    // receiving requests in its main loop, so the direct VM_BRK call
    // inside `install_vm_supplier` gets an answer; EINVAL fails fast; the
    // same method as DS/SCHED/MIB/VFS/PFS/PM, and the whole block is
    // #[cfg(target_os="none")], so host tests are unaffected.
    #[cfg(target_os = "none")]
    minix_rt::install_vm_supplier(minix_rt::image_end())
        .expect("tty: install_vm_supplier(镜像尾) 不得失败");

    #[cfg(target_os = "none")]
    {
        let probe = minix_rt::alloc(8);
        assert!(!probe.is_null(), "tty: vm-brk probe 分配失败");
        minix_rt::free(probe);
        use minix_sys::syscall::{DirectKernelCallTransport, sys_diagctl_write};
        #[allow(clippy::let_underscore_must_use)]
        // [SD19-gate] park as the fallback when the report channel itself
        // is broken (panic.c:66)
        let _ = sys_diagctl_write(
            &DirectKernelCallTransport,
            "nk4a: tty-vmbrk probe ok pages=1 via=VM_BRK\n",
        );
    }

    // A fresh start needs no extra preparation: the line table is built at
    // service construction. The runtime refuses the stateful kinds itself,
    // so this callback only ever sees the fresh kind.
    //
    // `serve` runs the birth handshake then the receive loop forever; it
    // returns only when startup is refused or the transport dies. There is
    // nothing a driver that cannot serve can do next, so main returns and
    // the process exits (C turns the same receive failure into a panic).
    // `serve` returns only when startup is refused or the transport dies
    // (at which point C turns the same receive failure into a panic); the
    // 0 exit code is supplied by the none entry (the crt0 Consumer
    // contract).
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
