//! `sysboot-tx` — the send half of the S42 ④ multi-process boot carrier
//! (C-27). Loaded into the RS slot by the carrier kernel (endpoint 2,
//! generation 0). It fires the first user↔user `sendrec` at the receive
//! half parked in the VM boot slot (endpoint 8).
//!
//! Serial markers the run script greps:
//!   SYSBOOT TX UP    — main entered
//!   SYSBOOT TX GOT   — the sendrec came back (the peer's reply arrived)
//!   SYSBOOT TX DONE  — full round trip complete
//!
//! If the sendrec fails, the errno is printed (hex) and main returns 1 —
//! the run script treats the missing GOT/DONE markers as FAIL.

#![no_std]
#![no_main]

use minix_sys::ipc::{DirectTrapTransport, IpcTransport};
use minix_sys::syscall::{sys_diagctl, DirectKernelCallTransport};
use minix_types::{Endpoint, Message};

/// Console write through the kernel diagnostic channel (rt-birth's
/// `emit` shape).
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
    let mut up: [u8; 21] = *b"SYSBOOT TX UP argv=?\n";
    up[19] = b'0' + (argv as u8).min(9);
    emit(&up);

    let ipc = DirectTrapTransport;
    // The receive half's boot endpoint: VM boot slot, generation 0
    // (endpoint == proc_nr == 8 — the kernel derives boot endpoints as
    // `_ENDPOINT(0, proc_nr)`).
    let peer = Endpoint(8);

    let mut msg = Message::default();
    msg.m_type = 0x42;
    match ipc.sendrec(peer, &mut msg) {
        Ok(()) => {
            emit(b"SYSBOOT TX GOT\n");
            emit(b"SYSBOOT TX DONE\n");
            0
        }
        Err(status) => {
            // Render the failure status so the serial tail explains the
            // miss instead of going silent.
            let mut err: [u8; 33] = *b"SYSBOOT TX ERR status=0x00000000\n";
            let mut code = status.0 as u32;
            for i in 0..8 {
                let nibble = (code >> 28) & 0xF;
                err[21 + i] = if nibble < 10 {
                    b'0' + nibble as u8
                } else {
                    b'a' + nibble as u8 - 10
                };
                code <<= 4;
            }
            emit(&err);
            1
        }
    }
}
