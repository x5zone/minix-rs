//! Minix-RS data store — entry point.
//!
//! C: `main()` — `minix3/minix/servers/ds/main.c:28-88`.
//! See `rewrite-notes/07-stage-ds/01-ds-init-main.md`.

// The form splits by target (following init, `os/commands/sbin/init/src/main.rs:27-35`):
// host and test builds keep the std form — rustc always passes Scrt1.o for a
// hosted target (its `_start` needs `main` + `__libc_start_main`), so
// no_std + no_main always breaks the host link; the `target_os = "none"` side
// is the real freestanding body (a boot-installed module).
// `#[unsafe(no_mangle)]` satisfies the minix-rt crt0 birth chain, which
// resolves `main` by name through the Consumer contract
// (`os/libs/minix-rt/src/crt0.rs:40`: name + Rust ABI + `-> i32`, the three
// requirements; the return value becomes the process exit status verbatim
// through the stage-6 exit).
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code
// itself uses no rt symbol (init differs — its main calls `minix_rt::init()`
// directly), so an unreferenced rlib would be dropped whole and
// `_start`/`#[panic_handler]`/`#[global_allocator]` would vanish with it (the
// `extern crate` retention semantics of the panic-halt pattern). The birth
// chain's `_start` has already completed rt initialization before `main` (the
// `lib.rs:86` init contract: in no_std form `_start` calls it automatically,
// idempotently), so it is neither needed nor proper to call it again here.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

// In test builds, use the system allocator (the crate is no_std in
// production; the test harness allocates before main() runs).
#[cfg(test)]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;

// The entry splits two ways by target: the none side satisfies the crt0
// Consumer contract (name + Rust ABI + `-> i32`,
// os/libs/minix-rt/src/crt0.rs:40); the host/test side keeps `()` — since
// rustc 1.94, Termination is no longer implemented for i32, so a host i32
// main is an immediate E0277 (measured with docker minix-ci:1.94).
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
    // In test builds, skip the binary entirely (the test harness drives the
    // library directly).
    #[cfg(not(test))]
    {
        use minix_ds::server::{DsServer, SysIpc, SysKernel};
        use minix_sys::ipc::DirectTrapTransport;

        // C: env_setargs + sef_local_startup() — main.c:38-39. The fresh
        // anchor (`sef_cb_init_fresh`, store.c:254-282) resets both tables
        // and shadows the RS boot table; the rproctab *decode* (raw
        // `rprocpub` bytes → `BootService` list) is pinned by tests, so the
        // fresh run starts from the reset state — the exact state
        // `sef_cb_init_fresh` itself occupies after its reset half, before the
        // shadow loop.
        let mut server = DsServer::new();
        let mut ipc = SysIpc::new(DirectTrapTransport);
        let mut kernel = SysKernel;

        // The first install point of the migration: switch the global heap
        // supply to the VM_BRK growing heap, with the initial break at the
        // linker image end (C `brksize.S` broadcasts the same symbol `_end`
        // into `_brksize`; here `minix_rt::image_end`). The install itself
        // performs no IPC (it only switches the allocator form); only a first
        // allocation beyond the fixed pool really sends VM_BRK. In the birth
        // order DS comes after VM (`rs/boot.rs` install table), so the VM main
        // loop is already taking requests at install time; EINVAL (image end
        // of 0 = link defect) fails fast.
        #[cfg(target_os = "none")]
        minix_rt::install_vm_supplier(minix_rt::image_end())
            .expect("ds: install_vm_supplier(镜像尾) 不得失败");

        // Real-machine probe for the install: immediately after it, take one
        // page through the VM leg (an 8-byte slab request triggers add_slab →
        // supply_page → VM_BRK); any failure fails fast (OOM tag + panic). On
        // success it prints one serial marker — evidence that the VM_BRK
        // growth leg is connected end to end; the probe block returns to the
        // slab free list on free (the VM-side page is not reclaimed: shrinking
        // the break never unmaps, as in C). The probe sends a blocking sendrec
        // to VM; DS comes after VM in the birth order, so there is no cyclic
        // dependency and it is safe.
        #[cfg(target_os = "none")]
        {
            let probe = minix_rt::alloc(8);
            assert!(!probe.is_null(), "ds: vm-brk probe 分配失败");
            minix_rt::free(probe);
            use minix_sys::syscall::{DirectKernelCallTransport, sys_diagctl_write};
            #[allow(clippy::let_underscore_must_use)] // [SD19-gate] park is the fallback when the report channel itself breaks (panic.c:66)
            let _ = sys_diagctl_write(
                &DirectKernelCallTransport,
                "nk4a: ds-vmbrk probe ok pages=1 via=VM_BRK\n",
            );
        }

        // C: main loop — main.c:42-86, owned by server.rs: receive →
        // triage → dispatch → reply, forever; a broken transport dies at
        // the documented 64-turn bound (server.rs, the SCHED-shared
        // deviation from C's panic-on-first-failure).
        server.run(&mut ipc, &mut kernel);
    }
    // With the test arm compiled out this body is empty; an unreachable keeps
    // the `-> !` shape (the test shell drives the library directly and never
    // enters this entry point).
    #[cfg(test)]
    unreachable!("bin entry unused in test builds")
}
