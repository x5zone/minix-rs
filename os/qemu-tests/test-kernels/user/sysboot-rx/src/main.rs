//! `sysboot-rx` — the receive half of the S42 ④ multi-process boot carrier
//! (C-27). Loaded into the VM boot slot by the carrier kernel, so its boot
//! endpoint is 8 (generation 0: endpoint == proc_nr).
//!
//! It parks in `receive(ANY)` — the first real blocking user receive — and
//! on the peer's message answers with a non-blocking send, emitting the
//! serial markers the run script greps:
//!   SYSBOOT RX UP    — main entered, diagctl console channel works
//!   SYSBOOT RX GOT   — the user↔user message arrived through the kernel
//!   SYSBOOT RX DONE  — the reply left through sendnb
//!
//! After the exchange it parks again in `receive(ANY)`: the carrier has no
//! exit primitive to exercise, and a parked process keeps the scheduler
//! honest without burning the CPU.

#![no_std]
#![no_main]

use minix_sys::ipc::{DirectTrapTransport, IpcTransport};
use minix_sys::syscall::{sys_diagctl, DirectKernelCallTransport};
use minix_types::{Endpoint, Message};

/// Console write through the kernel diagnostic channel (rt-birth's
/// `emit` shape: `sys_diagctl` code 1, kernel renders on its console).
fn emit(message: &[u8]) {
    let _ = sys_diagctl(
        &DirectKernelCallTransport,
        1,
        message.as_ptr() as u64,
        message.len() as i32,
    );
}

#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    // Birth-chain evidence: argv parse through minix-rt's crt0 statics
    // (rt-birth marker convention; also keeps the runtime crate linked).
    let argv = minix_rt::crt0::argv_count();
    let mut up: [u8; 21] = *b"SYSBOOT RX UP argv=?\n";
    up[19] = b'0' + (argv as u8).min(9);
    emit(&up);

    let ipc = DirectTrapTransport;
    loop {
        let mut msg = Message::default();
        match ipc.receive(Endpoint::ANY, &mut msg) {
            Ok(_status) => {
                emit(b"SYSBOOT RX GOT\n");
                // The peer sits in sendrec's receive half: a non-blocking
                // full-message send delivers the reply and wakes it.
                let mut reply = Message::default();
                reply.m_type = 0x43;
                match ipc.sendnb(msg.m_source, &reply) {
                    Ok(()) => emit(b"SYSBOOT RX DONE\n"),
                    Err(code) => {
                        // Report the errno (two decimal digits, < 100 for
                        // classic errnos; Minix-extended ones print their
                        // low two digits — enough to distinguish lanes).
                        let v = code.0 as u32 & 0xff;
                        let mut m: [u8; 21] = *b"SYSBOOT RX RERR ??? \n";
                        m[15] = b'0' + ((v / 100) % 10) as u8;
                        m[16] = b'0' + ((v / 10) % 10) as u8;
                        m[17] = b'0' + (v % 10) as u8;
                        emit(&m);
                    }
                }
            }
            Err(code) => {
                // The receive trap failed: report the errno once and park
                // in a spin — there is no user-space exit primitive in
                // this carrier, and the run script judges by markers, not
                // by process exit. Two decimal digits suffice: every
                // Minix3 errno is < 100 (signal.h errno values).
                let v = code.0 as u32 & 0xff;
                let mut m: [u8; 20] = *b"SYSBOOT RX ERR ??  \n";
                m[14] = b'0' + ((v / 10) % 10) as u8;
                m[15] = b'0' + (v % 10) as u8;
                emit(&m);
                loop {
                    core::hint::spin_loop();
                }
            }
        }
    }
}
