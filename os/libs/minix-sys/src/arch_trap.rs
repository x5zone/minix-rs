//! User-side trap bodies — the real `int 0x21` / `syscall` instructions.
//!
//! E1 slice 3 (design doc `01-stage-kernel/18-trap-bridge-design.md`,
//! decisions 2 + 4). The register ABI is the C i386 soft-int convention
//! widened to 64 bits — the only trap ABI a C source in this tree defines
//! (anti-guess discipline):
//!
//! ```text
//! RCX = IPC call number            (C: `movl $opcode, %ecx`)
//! RAX = src/dst endpoint           (SENDA: table entry count)
//! RBX = message pointer            (SENDA: table pointer)
//! int 0x21                         (vector 33, gate DPL=3)
//! return: RAX = errno (0 = OK), RBX = entry value possibly OR-merged
//!         with IPC status by the kernel (the status register family)
//! ```
//!
//! Every other general-purpose register is restored by the kernel's trap
//! stub, so callers only observe RAX/RBX changes — the same contract the
//! C soft-int bodies rely on (usermapped_glo_ipc.S: "all message passing
//! routines save ebx, but destroy eax and ecx").
//!
//! # Gating
//!
//! The trap bodies compile only under the `real-trap` feature on x86-64.
//! Hosted test builds keep the `-EIO` transports: a stray `syscall`/`int`
//! under the HOST kernel is indistinguishable from a real boundary at
//! compile time (observed live: the host answered -ENOSYS), so the switch
//! is an explicit feature only real boot-image builds turn on. The
//! instruction sequences themselves are validated by the slice-5 bring-up
//! boot, not by unit tests.

/// IPC call numbers on the trap ABI — C: `ipcconst.h` SEND=1 … SENDA=16.
/// The kernel's `IpcCall::from_raw` accepts exactly these.
pub const SEND_NR: i32 = 1;
pub const RECEIVE_NR: i32 = 2;
pub const SENDREC_NR: i32 = 3;
pub const NOTIFY_NR: i32 = 4;
pub const SENDNB_NR: i32 = 5;
pub const SENDA_NR: i32 = 16;
/// MINIX_KERNINFO — kernel info page query; the page address comes back
/// through the secondary return register (RBX), C ipcconst.h:12.
pub const KERNINFO_NR: i32 = 6;

/// Map a trap errno to the transport result shape.
pub fn errno_result(ret: i32) -> Result<(), super::ipc::TrapStatus> {
    if ret == 0 {
        Ok(())
    } else {
        Err(super::ipc::TrapStatus(ret))
    }
}

/// Execute the vector-33 IPC trap. `a1`/`a2` carry the two call-specific
/// operands; the returned `usize` is the post-trap status register (RBX).
///
/// # Safety
///
/// Traps into the kernel. Requires a live kernel behind the vector-33
/// gate and a valid `a2` pointer for message-carrying calls.
#[cfg(all(target_arch = "x86_64", feature = "real-trap"))]
pub unsafe fn ipc_trap(call_nr: i32, a1: usize, a2: usize) -> (i32, usize) {
    // RBX is reserved by LLVM on x86-64 and cannot be a declared operand
    // (same constraint as cpu_identity.rs) — yet the ABI requires it as
    // the message-pointer in / status-out register. The sequence saves
    // LLVM's RBX, loads the operand, traps, captures the post-trap RBX
    // (entry value OR-merged with IPC status by the kernel), and restores.
    let ret: usize;
    let status: usize;
    unsafe {
        core::arch::asm!(
            "push rbx",
            "mov rbx, {a2}",
            "int 0x21",
            "mov {status}, rbx",
            "pop rbx",
            a2 = in(reg) a2,
            status = out(reg) status,
            inlateout("rax") a1 => ret,
            in("rcx") call_nr as usize,
        );
    }
    (ret as i32, status)
}

/// Execute a kernel call through the SYSCALL leg (LSTAR): RDI carries the
/// user message pointer, RAX comes back as the reply code. The kernel
/// copies reply fields into the same user message buffer
/// (`kernel_call_finish` → `copy_msg_to_user`), so `msg` reflects
/// out-params after the call.
///
/// # Safety
///
/// Traps into the kernel; `msg` must be a valid, writable user message.
#[cfg(all(target_arch = "x86_64", feature = "real-trap"))]
pub unsafe fn kernel_call_trap(msg: &mut minix_types::Message) -> i32 {
    let ret: i32;
    unsafe {
        core::arch::asm!(
            "syscall",
            lateout("rax") ret,
            in("rdi") msg as *mut minix_types::Message as usize,
            lateout("rcx") _,
            lateout("r11") _,
        );
    }
    ret
}

// Hosted / non-x86-64 builds: the trap boundary does not exist here, so
// the instruction bodies are absent by construction — the transports keep
// their `-EIO` behavior and the Canned transports remain the test seam.
