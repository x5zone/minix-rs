//! rt-birth — the first minix-rt freestanding user binary (edge E1 slice 5
//! acceptance, 14-stage-runtime V1-P1-1 step 2).
//!
//! The birth chain under test (minix-rt `crt0`): the kernel loads this ELF
//! through `load_vm_elf` and hands the CPU over at `_start` with the
//! ps_strings pointer in RBX; the chain then reads the descriptor, queries
//! the kernel information page through the real MINIX_KERNINFO trap, and
//! calls `main`.
//!
//! `main` emits its evidence on the serial log through the SYS_DIAGCTL
//! console channel (kernel `dispatch_diagctl` code 1) and additionally
//! records a compact numeric trail in `M_TRAIL` (a .bss static,
//! GDB-readable through the live CR3): M0 = magic 0xC0FFEE01, M1 = argv
//! count, M2 = diagctl result (0 = emitted, else errno), M3 = kerninfo
//! availability (bit 0 = ready), M4 = channel result for the flags line,
//! M5 = channel result for the user_sp line, M6 = channel result for the
//! MAIN OK line, M7 = 0xC0FFEE05 once the forced panic check begins.
//! After that `main` forces a panic on purpose so the user-space
//! diagnostic hook renders through the same console channel.
//!
//! PASS = the run script (qemu-tests/test-rt-birth.sh) finds the serial
//! markers and the trail values via GDB.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU64, Ordering};

use minix_sys::syscall::{sys_diagctl, DirectKernelCallTransport};

/// Console write through the kernel diagnostic channel.
///
/// C: `printf`-to-console reduced to `sys_diagctl(DIAGCTL_CODE_DIAG)`;
/// the kernel copies from the caller buffer (`dispatch_diagctl` code 1,
/// 128-byte DIAGBUFSIZE cap) and renders through its EarlyConsole.
/// Returns 0 on success, else the errno the kernel returned.
fn emit(message: &[u8]) -> u64 {
    match sys_diagctl(
        &DirectKernelCallTransport,
        1,
        message.as_ptr() as u64,
        message.len() as i32,
    ) {
        Ok(()) => 0,
        Err(code) => code as u64,
    }
}

/// Decimal + fixed-width hex into a stack buffer, no allocator, no fmt.
struct Line {
    buffer: [u8; 128],
    used: usize,
}

impl Line {
    const fn new() -> Self {
        Line {
            buffer: [0; 128],
            used: 0,
        }
    }

    fn push_bytes(&mut self, bytes: &[u8]) {
        let room = self.buffer.len() - self.used;
        let n = bytes.len().min(room);
        self.buffer[self.used..self.used + n].copy_from_slice(&bytes[..n]);
        self.used += n;
    }

    fn push_dec(&mut self, mut value: u64) {
        let mut digits = [0u8; 20];
        let mut len = 0;
        loop {
            digits[len] = b'0' + (value % 10) as u8;
            value /= 10;
            len += 1;
            if value == 0 {
                break;
            }
        }
        while len > 0 {
            len -= 1;
            self.push_bytes(&digits[len..len + 1]);
        }
    }

    fn push_hex16(&mut self, value: u64) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        self.push_bytes(b"0x");
        for shift in (0..64).step_by(4).rev() {
            let digit = ((value >> shift) & 0xF) as usize;
            self.push_bytes(&HEX[digit..digit + 1]);
        }
    }

    fn emit(&self) -> u64 {
        emit(&self.buffer[..self.used])
    }
}

/// BISECT trail (GDB-readable through the live CR3; nm symbol M_TRAIL).
/// Evidence recorded even when the console channel itself misbehaves.
#[unsafe(no_mangle)]
pub static M_TRAIL: [AtomicU64; 8] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];

/// Real `main` — reached only after the whole birth chain ran.
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    // Trail: main entered.
    M_TRAIL[0].store(0xC0FF_EE01, Ordering::Relaxed);

    // 1. descriptor parse result (boot images: empty argv, empty name).
    let mut line = Line::new();
    line.push_bytes(b"rt-birth argv=");
    line.push_dec(minix_rt::crt0::argv_count() as u64);
    line.push_bytes(b" progname='");
    line.push_bytes(minix_rt::crt0::progname());
    line.push_bytes(b"'\n");
    M_TRAIL[1].store(minix_rt::crt0::argv_count() as u64, Ordering::Relaxed);
    M_TRAIL[2].store(line.emit(), Ordering::Relaxed);

    // 2 + 3. kernel information page: the trap query already validated the
    // magic; the user_sp line additionally dereferences the published
    // page's kuserinfo pointer — two real reads of the shared page.
    if let Some(state) = minix_rt::crt0::runtime_state() {
        M_TRAIL[3].store(state.is_ready() as u64, Ordering::Relaxed);
        match state.kerninfo {
            minix_rt::init::KerninfoAvailability::Available(info) => {
                let header = info.header();
                let mut line = Line::new();
                line.push_bytes(b"rt-birth kerninfo=ready flags=");
                line.push_dec(header.flags as u64);
                line.push_bytes(b"\n");
                M_TRAIL[4].store(line.emit(), Ordering::Relaxed);

                if header.flags & minix_rt::handoff::KIF_USER_INFO != 0
                    && header.user_info_address != 0
                {
                    // SAFETY: the kernel published and user-mapped the page
                    // (E-KERNINFO kernel half); kuserinfo points at the
                    // KuserInfo struct inside that page.
                    let user_info = unsafe {
                        &*(header.user_info_address as *const minix_rt::handoff::UserInfo)
                    };
                    let mut line = Line::new();
                    line.push_bytes(b"rt-birth user_sp=");
                    line.push_hex16(user_info.initial_stack_pointer);
                    line.push_bytes(b"\n");
                    M_TRAIL[5].store(line.emit(), Ordering::Relaxed);
                }
            }
            minix_rt::init::KerninfoAvailability::BadMagic { found } => {
                M_TRAIL[4].store(0xBAD0_0000 + found as u64, Ordering::Relaxed);
            }
            minix_rt::init::KerninfoAvailability::QueryFailed(code) => {
                M_TRAIL[4].store(0xFA11_0000 + code as u64, Ordering::Relaxed);
            }
        }
    }

    // 4. full chain alive.
    let mut line = Line::new();
    line.push_bytes(b"RT-BIRTH MAIN OK\n");
    M_TRAIL[6].store(line.emit(), Ordering::Relaxed);

    // 5. panic-ladder render check: register the user-space hook (the
    // minix-types registry is per-address-space — the kernel's hook is not
    // visible here), then panic on purpose. The handler renders through
    // `format_panic_report` and hands the report to the hook, which routes
    // it to the console; the process then spins, which is the handler's
    // contract.
    minix_types::set_panic_diagnostic_hook(Some(|message: &str| {
        emit(message.as_bytes());
        emit(b"\n");
    }));
    M_TRAIL[7].store(0xC0FF_EE05, Ordering::Relaxed);
    panic!("rt-birth panic render check");
}
