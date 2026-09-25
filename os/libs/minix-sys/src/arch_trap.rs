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
//! The trap bodies compile only under `cfg(kernel_trap)` — set by
//! `build.rs` for freestanding targets or the `real-trap` feature
//! (NK4-A fix22: the per-package feature opt-in used to be the only
//! gate, and every image module that forgot it silently got `-EIO`).
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
/// through the secondary return register (x86-64: R10 status-lane 兼任,
/// C ipcconst.h:12; NK4-C 1.54 — 与 `ipc_trap` 第二返回值同车道).
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
/// operands; the returned `usize` is the post-trap IPC status (R10).
///
/// # Safety
///
/// Traps into the kernel. Requires a live kernel behind the vector-33
/// gate and a valid `a2` pointer for message-carrying calls.
#[cfg(all(target_arch = "x86_64", kernel_trap))]
pub unsafe fn ipc_trap(call_nr: i32, a1: usize, a2: usize) -> (i32, usize) {
    // ABI（NK4-C Task C A 案，[ARCH]）：R10 是 IPC 状态寄存器——入口由本
    // 序列装载消息指针，内核完成时把状态字 OR 进同一寄存器并随恢复交付。
    // R10 是 caller-saved：用户代码不能指望它跨调用存活（与 C earm 的
    // IPC_STATUS_REG=r1 同思路），因此内核在任意恢复点写入它不破坏用户
    // 活值——这正是把状态从 callee-saved 的 RBX 迁出的原因（RBX 迁移前
    // 被内核状态写摧毁用户 callee-saved 活值：真机 RS `&self.table`=0，
    // NK4A Task C 六轮取证）。消息指针仍走 RBX 车道（入口参数），本序列
    // 继续 push/pop 保护调用者的 RBX。
    //
    // RBX is reserved by LLVM on x86-64 and cannot be a declared operand
    // (same constraint as cpu_identity.rs) — the manual push/pop around
    // the int keeps LLVM's value intact.
    let ret: usize;
    let status: usize;
    unsafe {
        core::arch::asm!(
            "push rbx",
            "mov rbx, {a2}",
            "int 0x21",
            "mov {status}, r10",
            "pop rbx",
            a2 = in(reg) a2,
            status = out(reg) status,
            inlateout("rax") a1 => ret,
            in("rcx") call_nr as usize,
            out("r10") _,
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
#[cfg(all(target_arch = "x86_64", kernel_trap))]
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

// ── riscv64 ecall boundary (K12b riscv64 leg) ───────────────────────────
//
// The riscv64 semantic mapping of the same C i386 soft-int convention: the
// RISC-V ABI already reserves a7 for the syscall number (the Linux rv64
// convention), so the C `ecx` slot (call number) lands there, and the two
// operand slots take the first two argument registers. One instruction —
// `ecall` — serves both x86 legs (the int-0x21 IPC gate and the LSTAR
// SYSCALL message leg): the KERNEL_CALL message leg reserves call number 0
// and carries the call number inside the message (`m_type`), exactly like
// the x86 SYSCALL leg.
//
// ```text
// a7 = call number           (0 = KERNEL_CALL message leg; 1..16 = raw IPC)
// a0 = operand 1             (IPC: endpoint / KERNEL_CALL: message pointer)
// a1 = operand 2             (IPC: message pointer)
// ecall                      (U-mode, cause 8: environment-call-from-U)
// return: a0 = errno (0 = OK), a1 = secondary return (status / page VA)
// ```
//
// Unlike x86 (where `int 0x21` leaves the interrupted RIP on the frame),
// `ecall` does NOT advance sepc — the kernel's trap handler must step the
// saved PC past the ecall before sret, or the instruction re-traps forever.

/// Call number reserving the KERNEL_CALL message leg on the riscv64 trap
/// ABI (the message's `m_type` carries the real call number, as on the x86
/// SYSCALL leg).
pub const KERNEL_CALL_TRAP_NR: i32 = 0;

/// Execute the riscv64 IPC trap. `a1`/`a2` carry the two call-specific
/// operands; the returned pair is the errno and the post-trap secondary
/// return register (a1).
///
/// # Safety
///
/// Traps into the kernel. Requires a live kernel behind the ecall boundary
/// and a valid `a2` pointer for message-carrying calls.
#[cfg(all(target_arch = "riscv64", kernel_trap))]
pub unsafe fn ipc_trap(call_nr: i32, a1: usize, a2: usize) -> (i32, usize) {
    let ret: usize;
    let status: usize;
    unsafe {
        core::arch::asm!(
            "ecall",
            inlateout("a0") a1 => ret,
            inlateout("a1") a2 => status,
            in("a7") call_nr as usize,
        );
    }
    (ret as i32, status)
}

/// Execute a kernel call through the KERNEL_CALL message leg (trap number
/// 0): a0 carries the user message pointer, the call number rides in the
/// message's `m_type`, and the reply code comes back in a0. The kernel
/// copies reply fields into the same user message buffer, so `msg`
/// reflects out-params after the call.
///
/// # Safety
///
/// Traps into the kernel; `msg` must be a valid, writable user message.
#[cfg(all(target_arch = "riscv64", kernel_trap))]
pub unsafe fn kernel_call_trap(msg: &mut minix_types::Message) -> i32 {
    let ret: usize;
    unsafe {
        core::arch::asm!(
            "ecall",
            inlateout("a0") msg as *mut minix_types::Message as usize => ret,
            lateout("a1") _,
            in("a7") KERNEL_CALL_TRAP_NR as usize,
        );
    }
    ret as i32
}

// ── aarch64 SVC boundary (K12b aarch64 leg) ─────────────────────────────
//
// The aarch64 semantic mapping of the same C i386 soft-int convention,
// following the riscv64 shape (one boundary instruction serving both the
// IPC and KERNEL_CALL legs): `svc #0` from EL0 raises a synchronous
// exception to EL1 (ESR EC 0x15, SVC from AArch64), and x8 is the
// syscall-number register per the arm64 Linux convention.
//
// ```text
// x8  = call number          (0 = KERNEL_CALL message leg; 1..16 = raw IPC)
// x0  = operand 1            (IPC: endpoint / KERNEL_CALL: message pointer)
// x1  = operand 2            (IPC: message pointer)
// svc #0
// return: x0 = errno (0 = OK), x1 = secondary return (status / page VA)
// ```
//
// `svc` DOES advance the return PC past the instruction (ELR_EL1 = the
// next instruction, per the A-arm exception semantics), so — unlike
// riscv64 ecall — the trap handler must NOT step the saved PC.

/// Execute the aarch64 IPC trap (mirror of the riscv64 `ipc_trap`).
///
/// # Safety
///
/// Traps into the kernel. Requires a live kernel behind the svc boundary
/// and a valid `a2` pointer for message-carrying calls.
#[cfg(all(target_arch = "aarch64", kernel_trap))]
pub unsafe fn ipc_trap(call_nr: i32, a1: usize, a2: usize) -> (i32, usize) {
    let ret: usize;
    let status: usize;
    unsafe {
        core::arch::asm!(
            "svc #0",
            inlateout("x0") a1 => ret,
            inlateout("x1") a2 => status,
            in("x8") call_nr as usize,
        );
    }
    (ret as i32, status)
}

/// Execute a kernel call through the KERNEL_CALL message leg (mirror of
/// the riscv64 `kernel_call_trap`).
///
/// # Safety
///
/// Traps into the kernel; `msg` must be a valid, writable user message.
#[cfg(all(target_arch = "aarch64", kernel_trap))]
pub unsafe fn kernel_call_trap(msg: &mut minix_types::Message) -> i32 {
    let ret: usize;
    unsafe {
        core::arch::asm!(
            "svc #0",
            inlateout("x0") msg as *mut minix_types::Message as usize => ret,
            lateout("x1") _,
            in("x8") KERNEL_CALL_TRAP_NR as usize,
        );
    }
    ret as i32
}
