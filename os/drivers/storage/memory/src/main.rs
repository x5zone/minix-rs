//! The memory driver binary.
//!
//! C: `main()` (`memory.c:99-108`) — SEF local startup (the birth handshake
//! with the restart server), then the receive loop that routes every message
//! to the block or character framework. Both halves of the shell come from
//! the shared driver runtime; this binary names the two data-store labels
//! (the memory driver announces on both families, `memory.c:160-161`) and
//! builds the device.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code
// itself uses no rt symbol, and an unreferenced rlib would be dropped whole,
// taking `_start`/`#[panic_handler]`/`#[global_allocator]` with it (the
// `extern crate` retention semantics of panic-halt mode). The birth chain
// `_start` has already completed rt initialization before main (`lib.rs:86`
// init contract: called automatically in no_std mode, idempotent), so it must
// not and need not be called again here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_driver_rt::kernel::KernelTransport;
use minix_driver_rt::runtime::DriverRuntime;

// The entry point splits into two forms by target: the none side satisfies the
// crt0 Consumer contract (name + Rust ABI + `-> i32`,
// os/libs/minix-rt/src/crt0.rs:38); the host/test side keeps `()` — since
// rustc 1.94, Termination is no longer implemented for i32, so an i32 main on
// the host is E0277 (measured with docker minix-ci:1.94).
#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    // real_main consumes its return value in place, returning (); 0 is the
    // process exit code (crt0 Consumer contract).
    real_main();
    0
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    real_main()
}

fn real_main() {
    // Birth self-identification: C's sef_startup first calls sys_whoami to fill
    // sef_self_endpoint (sef.c:76-87, failure panics at :81). The endpoint is
    // granted by the kernel boot table (not by RS during boot — an earlier note
    // was wrong); the query goes through the source-less
    // DirectKernelCallTransport (the reply comes from the kernel slot of the
    // caller's process context, independent of m_source), and the answer is
    // then fed to the source-carrying IPC transport (same as tty NL6-A).
    let who = minix_sys::syscall::sys_whoami(&minix_sys::syscall::DirectKernelCallTransport)
        .unwrap_or_else(|r| panic!("memory: sys_whoami failed: {r}"));
    let transport = KernelTransport::new(who.endpoint);
    // A dual-framework driver publishes both family keys at announce: the
    // character label first, then the block label (C runs
    // `chardriver_announce` and `blockdriver_announce` back to back in
    // `sef_cb_init_fresh`).
    let mut runtime =
        DriverRuntime::new(transport, "drv.chr.memory").also_announce("drv.blk.memory");
    let mut service = minix_driver_memory::init();

    // Migration (eighth host, second driver): memory is a ramdisk
    // block/character driver (it supplies the root-disk block source for
    // /bin/sh, /etc/rc and imgrd at boot), has 6 Vec/Box uses in src/, and
    // links alloc-global. Switch the global heap supply from the hardwired
    // fixed pool to the VM_BRK growing heap. The install point is after
    // service construction (both faces and their tables are built, still on
    // the fixed pool) and before the `serve` main loop — by now VM (second in
    // boot order, ahead of memory at fourth) is already receiving requests in
    // its main loop; EINVAL fails fast; same approach as
    // DS/SCHED/MIB/VFS/PFS/PM/TTY, and the whole block is
    // #[cfg(target_os="none")] so host tests are unaffected.
    #[cfg(target_os = "none")]
    minix_rt::install_vm_supplier(minix_rt::image_end())
        .expect("memory: install_vm_supplier(镜像尾) 不得失败");

    #[cfg(target_os = "none")]
    {
        let probe = minix_rt::alloc(8);
        assert!(!probe.is_null(), "memory: vm-brk probe 分配失败");
        minix_rt::free(probe);
        use minix_sys::syscall::{DirectKernelCallTransport, sys_diagctl_write};
        #[allow(clippy::let_underscore_must_use)]
        // [SD19-gate] park is the fallback when the reporting channel itself is
        // broken (panic.c:66)
        let _ = sys_diagctl_write(
            &DirectKernelCallTransport,
            "nk4a: memory-vmbrk probe ok pages=1 via=VM_BRK\n",
        );
    }

    // A fresh start needs no extra preparation: both faces and their tables
    // are built at service construction. The runtime refuses the stateful
    // kinds (a restart must preserve RAM-disk contents, which the birth
    // handshake does not yet model) itself, so this callback only ever sees
    // the fresh kind.
    //
    // `serve` runs the birth handshake then the receive loop forever; it
    // returns only when startup is refused or the transport dies. There is
    // nothing a driver that cannot serve can do next, so main returns and
    // the process exits (C turns the same receive failure into a panic).
    // serve returns only when startup is refused or the transport dies (at
    // which point C gets the same receive failure and panics); the none entry
    // supplies exit code 0 (crt0 Consumer contract).
    let _birth = runtime.serve(&mut service, |_| Ok(()));
}
