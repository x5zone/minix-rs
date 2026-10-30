//! Minix file server entry point.
//!
//! C: `main` + the SEF callbacks — `minix3/minix/fs/mfs/main.c:29-78`
//! (`env_setargs` + `sef_local_startup` + `fsdriver_task`). The common
//! runtime shape (SEF receive loop, RS birth handshake, request decode,
//! replies, grant data plane) lives in `minix-fs-rt`; this entry only
//! contributes the server value and the two hooks.
//!
//! Two deliberate stand-ins, both honest failures rather than pretend
//! successes:
//! - the block source starts fail-closed (`EIO`) when no packaged image is
//!   available (dev/test build); when `xtask image` runs, its build.rs
//!   discovers the generated imgrd and `include_bytes!` fills `BOOT_IMGRD`
//!   with real bytes, switching `BootBlockSource` to the imgrd RAM disk
//!   (`minix_fs_rt::source::ImgrdBlockSource`). (E-IMGPKG has landed.);
//! - the signal hook ignores notifications for now — the C handler
//!   terminates on `SIGTERM` (`main.c:70-78`), but process signals reach
//!   it through the signal-manager pull whose kernel wrappers are not
//!   wired yet; the decision point stays in one hook, ready for that
//!   channel.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The freestanding side holds a link reference to minix-rt: the bin code
// itself uses no symbol from rt, and an unreferenced rlib is dropped whole,
// taking `_start`/`#[panic_handler]`/`#[global_allocator]` with it (the
// keep semantics of `extern crate` in panic-halt mode). The birth chain
// `_start` has already completed rt initialization before main
// (`lib.rs:86` init contract: called automatically and idempotently in
// no_std mode), so there is no need — and no reason — to call it again.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

// `Box::new` is used bare by the hooks: the none side gets it from alloc
// (host/test builds use the std prelude).
#[cfg(all(not(test), target_os = "none"))]
extern crate alloc;

#[cfg(all(not(test), target_os = "none"))]
use alloc::boxed::Box;

// In test builds, use the system allocator (the crate is no_std in
// production; the test harness allocates before main() runs).
#[cfg(test)]
#[global_allocator]
static GLOBAL: std::alloc::System = std::alloc::System;

// The packaged boot image, empty until image assembly lands (E-IMGPKG).
// C carries the same bytes as the memory driver's linked-in
// `_binary_imgrd_mfs_*` blob (`drivers/storage/memory/local.h:5-9`); the
// Rust packaging fills this slot instead — `build.rs` emits
// `BOOT_IMGRD_DATA` via `include_bytes!` when a packaged imgrd is present
// at `target/image/<arch>/imgrd.img`, or falls back to `&[]` for dev/test.
include!(concat!(env!("OUT_DIR"), "/imgrd_data.rs"));
#[cfg(not(test))]
static BOOT_IMGRD: &[u8] = BOOT_IMGRD_DATA;

/// The imgrd filesystem block size (`mkfs_mfs` default, `const.h` 4 KiB
/// era). Must match the superblock's declared `s_block_size` or mount
/// fails with `BlockSizeMismatch` (`mount.rs:387`).
#[cfg(not(test))]
const BOOT_BLOCK_SIZE: usize = 4096;

// The entry point splits by target: the none side satisfies the crt0
// Consumer contract (name + Rust ABI + `-> i32`,
// os/libs/minix-rt/src/crt0.rs:38); host/test sides keep `()`, because since
// rustc 1.94 Termination is no longer implemented for i32, so a hosted i32
// main is E0277 (observed on docker minix-ci:1.94).
#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    // real_main consumes its return value in place and yields (); 0 is the
    // process exit code (crt0 Consumer contract).
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
        let driver = minix_fs_mfs::server::MfsServer::new(
            minix_fs_rt::source::BootBlockSource::from_boot_image(BOOT_IMGRD, BOOT_BLOCK_SIZE),
        );

        // Control reading for the migration (pre-birth pool high-water): read
        // the global allocator diagnostics once before the install point and
        // print the pages the fixed pool consumed from birth to install, plus
        // the pool total, to the serial console. This line is the machine
        // reading that quantifies that high-water mark; the format keeps the
        // probe-family prefix so on-target logs are easy to scrape.
        #[cfg(target_os = "none")]
        {
            let diag = minix_rt::global_alloc_diag();
            let mut line = [0u8; 96];
            let mut at = 0;
            let head = b"nk4a: mfs pre-install pool pages used=";
            line[..head.len()].copy_from_slice(head);
            at += head.len();
            at += minix_rt::diag::format_decimal(diag.pages_consumed as i32, &mut line[at..]);
            let mid = b" total=";
            line[at..at + mid.len()].copy_from_slice(mid);
            at += mid.len();
            at += minix_rt::diag::format_decimal(diag.total_pages as i32, &mut line[at..]);
            line[at] = b'\n';
            at += 1;
            use minix_sys::syscall::{DirectKernelCallTransport, sys_diagctl_write};
            #[allow(clippy::let_underscore_must_use)] // [SD19-gate] when the report channel itself breaks, park is the fallback (panic.c:66)
            let _ = sys_diagctl_write(
                &DirectKernelCallTransport,
                core::str::from_utf8(&line[..at]).unwrap_or(""),
            );
        }

        // Migration (the ninth server, the first file server): MFS is the big
        // cache consumer (block cache and inode table, the Vec/Box working set
        // inside src/) and the reason the x86 build runs the 2048-page
        // profile. Switch the global heap supply from the hardwired fixed
        // pool to the VM_BRK growth heap. The install point sits after server
        // construction (the fresh body has already built tables and caches,
        // still on the fixed pool) and before `serve`'s main loop — by then
        // VM (boot order position 2, ahead of MFS at 11) is already serving
        // requests, so the VM_BRK direct link inside `install_vm_supplier`
        // gets an answer; EINVAL fail-fast; the same method as
        // DS(T3)/SCHED(T3b)/MIB/VFS/PM/PFS/TTY/memory, and the whole
        // #[cfg(target_os="none")] block leaves hosted tests untouched.
        #[cfg(target_os = "none")]
        minix_rt::install_vm_supplier(minix_rt::image_end())
            .expect("mfs: install_vm_supplier(镜像尾) 不得失败");

        // On-target evidence probe: immediately after install, take one page
        // through the VM leg (an 8-byte slab request triggers add_slab →
        // supply_page → VM_BRK); fail fast on failure. On success, print one
        // serial marker line as the end-to-end proof that the VM_BRK growth
        // leg works (the same probe as VFS).
        #[cfg(target_os = "none")]
        {
            let probe = minix_rt::alloc(8);
            assert!(!probe.is_null(), "mfs: vm-brk probe 分配失败");
            minix_rt::free(probe);
            use minix_sys::syscall::{DirectKernelCallTransport, sys_diagctl_write};
            #[allow(clippy::let_underscore_must_use)] // [SD19-gate] when the report channel itself breaks, park is the fallback (panic.c:66)
            let _ = sys_diagctl_write(
                &DirectKernelCallTransport,
                "nk4a: mfs-vmbrk probe ok pages=1 via=VM_BRK\n",
            );
        }

        // C registers a signal handler for this server (`mfs/main.c:38`);
        // its body (`main.c:70-78`) ignores everything but SIGTERM and, on
        // SIGTERM, syncs the file system before asking the framework to stop
        // (`fsdriver_terminate`, `fsdriver.c:68-74`). The decision function
        // lives in the library so C's rule is testable outside this binary.
        // Birth: the C fresh-install sequence (vmcache flag, inode table
        // zeroing, buffer pool — main.c:47-65) is carried by the server
        // value's construction, so the callback only confirms readiness.
        let hooks = minix_fs_rt::transport::ServerHooks {
            on_signal: Box::new(minix_fs_mfs::startup::decide_on_signal),
            init: Box::new(|_| Ok(())),
        };
        match minix_fs_rt::serve(driver, hooks) {
            Ok(()) => {}
            // C panics on the startup path (sef_startup); the boot failure
            // here is the birth report RS refused, same fatal outcome.
            Err(e) => panic!("mfs: startup failed: {e}"),
        }
    }
}
