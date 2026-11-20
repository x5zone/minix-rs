//! `e5-spawned` — the service body for the V20-P1-1 batch-1 stepping stone:
//! RS dynamic spawn on the real machine.
//!
//! The carrier is launched by RS (via `minix-service up /bin/e5-spawned
//! -label e5spawn` in `/etc/rc`), i.e. through the real dynamic-service
//! surface: RS allocates a slot, forks itself, execs this image, installs
//! privileges and publishes the label. This binary's only jobs are:
//!
//! 1. prove it ran: print `E5SPAWN: running` on stdout (the VFS leg the
//!    other user commands use); and
//! 2. stay alive as a service: park in `receive(ANY)` and answer every
//!    ordinary message, so the slot stays IN_USE and a later `down`/update
//!    has something to act on.
//!
//! No new protocol is invented here: the marker and the reply loop are the
//! minimum observable body, and the gate (`test-e5-ds.sh`) checks the
//! marker while RS's own bookkeeping is what the spawn exercised.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

/// The link needs a `#[global_allocator]` (minix-sys references `alloc`),
/// but this body never allocates: `alloc-global` would add a 4 MiB static
/// pool to `.bss`, and the dynamic-exec leg fails on a memsz-sized buffer
/// (batch-1 forensics). A failing allocator keeps the image small and turns
/// any accidental allocation into an honest abort.
#[cfg(all(not(test), target_os = "none"))]
struct NeverAlloc;

#[cfg(all(not(test), target_os = "none"))]
unsafe impl core::alloc::GlobalAlloc for NeverAlloc {
    unsafe fn alloc(&self, _layout: core::alloc::Layout) -> *mut u8 {
        core::ptr::null_mut()
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {}
}

#[cfg(all(not(test), target_os = "none"))]
#[global_allocator]
static NEVER_ALLOC: NeverAlloc = NeverAlloc;

use minix_sys::ipc::{DirectTrapTransport, IpcTransport};
use minix_sys::syscall::{DirectKernelCallTransport, sys_diagctl_write};
use minix_types::{Endpoint, Message};

/// Marker the gate greps. Kept whole so the line is unambiguous even when
/// the serial stream is interleaved with other boot output.
///
/// The kernel diagnostic channel (SYS_DIAGCTL, console write) is used
/// instead of fd 1: a dynamically spawned service inherits no VFS file
/// table, and the marker must not depend on the fd/stdout wiring this
/// stepping stone is not testing.
const MARKER: &str = "E5SPAWN: running\n";

fn run() -> i32 {
    // A lost marker makes the gate FAIL; do not swallow the write error.
    let _ = sys_diagctl_write(&DirectKernelCallTransport, MARKER);

    // Service body: park in receive(ANY), reply to everything that is not
    // a kernel notification (a reply to SYSTEM would be a protocol error;
    // this body never sees one in the stepping stone).
    loop {
        let mut msg = Message::default();
        if DirectTrapTransport.receive(Endpoint::ANY, &mut msg).is_ok() {
            if msg.m_source != Endpoint::SYSTEM {
                msg.m_type = 0;
                let _ = DirectTrapTransport.send(msg.m_source, &msg);
            }
        }
    }
}

#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}
