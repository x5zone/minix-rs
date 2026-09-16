//! Kernel-side trap/syscall dispatch bodies — S-8 (smp_todo.md §3.7).
//!
//! The x86-64 asm stubs (`minix_arch::x86_64::trap_stub`) save context and
//! call the two bodies registered here at `init_protection` time. This
//! module owns the *policy* (exception → `ExceptionDispatcher`, IRQ →
//! `IrqManager::dispatch`, syscall → `kernel_call`) while the arch module
//! owns the *mechanics* (frame save, iretq/sysretq).
//!
//! # C parity (mpx.S / trap.c / system.c)
//!
//! - **Kernel-origin traps** (C `TEST_INT_IN_KERNEL` light path): the
//!   interrupted context already owns the BKL, so handlers run under it via
//!   `BklSection::assume_held()` — the same root contract
//!   `dispatch_hardware_irq` and `clock_irq_handler` already witness. C's
//!   interrupt handlers take no lock for the same reason.
//! - **User-origin traps**: every acting outcome (signals, VM page-fault
//!   forwarding, FPU restore, recovery redirects) needs the per-CPU current
//!   process and the VM IPC link. Those arrive with the per-CPU scheduler
//!   steps (S-6/S-7); until then this body treats them as unreachable and
//!   panics with the outcome attached — no CPL3 code can exist before the
//!   scheduler hands out user contexts, so the panic is a wiring bug alarm,
//!   not reachable behavior. The S-9 step threads the explicit BKL witness
//!   for the exception entry (todo D-38①).
//! - **Syscall body**: reads the per-CPU `proc_ptr` anchor before any lock
//!   (C reads `proc_ptr` at trap entry the same way), then runs the
//!   existing `kernel_call` wrapper — which acquires/releases the BKL
//!   around dispatch+finish itself. The user message pointer travels in
//!   RDI (first SysV argument register); x86-64 is a new port, C's 32-bit
//!   register choice does not transfer (doc 13 §6.3 documents the ABI).

use crate::syscall::KcallResult;
use minix_arch::exception::{ExceptionArch, FaultContext};
use minix_arch::exception_dispatcher::{ExceptionDispatcher, ExceptionOutcome};
use minix_arch::TrapStyle;
use minix_arch::x86_64::exception::X86_64ExceptionFrame;
use minix_arch::x86_64::trap_stub::TrapFrame;
use minix_types::VirBytes;

/// The int-33 IPC gate (C: IPC_VECTOR_ORIG = 33, interrupt.h:33).
const IPC_VECTOR_GATE: u8 = 33;

/// Build the CPU-pushed tail of the frame as an `X86_64ExceptionFrame` for
/// the arch-generic dispatcher (which is implemented over that type).
fn exception_frame_of(frame: &TrapFrame) -> X86_64ExceptionFrame {
    X86_64ExceptionFrame {
        vector: frame.vector,
        errcode: frame.errcode,
        rip: frame.rip,
        cs: frame.cs,
        rflags: frame.rflags,
        rsp: frame.rsp,
        ss: frame.ss,
    }
}

/// A-path body: exceptions, external IRQs, IPIs, soft-int gates, spurious.
///
/// Registered via `minix_arch::register_trap_dispatchers` before
/// `TrapEntryArch::load()`; reached from `x86_trap_common`.
///
/// # Safety
///
/// `frame` points at the TrapFrame built by the asm stub on the interrupted
/// stack; the stub resumes via iretq when this returns.
pub unsafe extern "C" fn x86_trap_dispatch_body(frame: &mut TrapFrame) {
    let vector = frame.vector as u8;

    // E1 trap bridge: vector 33 (IPC_VECTOR) from user mode is the IPC
    // soft-int leg (C: IPC_VECTOR_ORIG, interrupt.h:33; gate DPL=3,
    // trap_entry.rs configure_ipc_entry). The register ABI is the C i386
    // soft-int convention widened to 64 bits — RAX = src/dst endpoint
    // (SENDA: count), RBX = message pointer (SENDA: table pointer),
    // RCX = IPC call number (design doc 18, decision 2; C:
    // usermapped_glo_ipc.S IPCARGS/SENDA_ARGS). errno returns in RAX;
    // IPC status rides the saved-context RBX channel as already wired
    // (or_ipc_status_reg / set_secondary_ipc_return).
    if vector == IPC_VECTOR_GATE {
        let cur_nr = current_ipc_proc_nr();
        x86_ipc_dispatch_body(frame, cur_nr);
        return;
    }

    // LAPIC spurious interrupt (C: apic.c apic_spurious_interrupt): no
    // handler chain, no EOI — the spurious vector never enters IRR/ISR.
    if vector == 0xFF {
        return;
    }

    // S-10: scheduler IPI (C SMP_SCHED_IPI_VECTOR) → the target CPU's own
    // sched_handler_full (loads flags, applies STOP/SAVE_CTX/VM_INHIBIT,
    // clears pending — the schedule_sync waiter's completion signal).
    // BKL: try-or-inherit (depth-1 emulation of C's counting lock — the AP
    // may be in its idle window with the BKL released, or interrupted while
    // the kernel held it).
    if vector == minix_arch::x86_64::smp::SCHED_IPI_VECTOR as u8 {
        let acquired = crate::smp::bkl_try_lock();
        {
            let section = unsafe { crate::smp::BklSection::assume_held() };
            let table = crate::proc_table_with(&section);
            let smp = crate::smp_state_with(&section);
            let cpu = crate::current_cpu_id();
            smp.sched_handler_full(table, cpu);
        }
        if acquired {
            crate::smp::bkl_unlock();
        }
        return;
    }

    if let Some(irq) = minix_arch::x86_64::trap_stub::irq_of_vector(vector) {
        // D-46 hardware half-loop: ack → mask → hook chain → unmask → eoi,
        // under the interrupted context's BKL (C: IRQ handlers run with the
        // BKL owned by the interrupted context; no lock is taken here).
        // Spurious lines are logged-and-dropped (C irq_handle's spurious
        // path); an invalid IRQ line is a trap-entry wiring bug.
        match crate::irq_manager::dispatch_hardware_irq(minix_plat::IrqVector::new(irq)) {
            Ok(()) => {}
            Err(crate::irq_manager::IrqError::Spurious(line)) => {
                use minix_plat::{EarlyConsole, CurrentEarlyConsole as Console};
                Console::write_str("spurious irq ");
                Console::write_hex(line.get() as u64);
                Console::write_str("\n");
            }
            Err(e) => panic!("trap_dispatch: IRQ dispatch error {e:?} (vector {vector:#04x})"),
        }
        return;
    }

    // Exception / user soft-int gate → arch-generic dispatcher.
    //
    // S-9 (D-38①) — BKL ownership at the exception entry: the trap path
    // INHERITS the interrupted context's BKL ownership (C parity: trap
    // handlers run with the BKL owned by the interrupted context; trap.c
    // takes no lock — acquiring here would deadlock on the non-reentrant
    // CAS lock for the normal kernel-origin case). `assume_held` turns that
    // invariant into a debug-asserted witness; the user-origin acquire
    // (C: trap entry BKL_LOCK from ring 3) becomes reachable with S-6/S-7
    // user frames and threads its own witness then.
    let _section = unsafe { crate::smp::BklSection::assume_held() };
    let mut exc = exception_frame_of(frame);
    let is_user = X86_64ExceptionFrame::is_user_mode(&exc);
    let outcome = ExceptionDispatcher::<X86_64ExceptionFrame>::handle(
        &mut exc,
        /* is_nested = */ !is_user,
        /* is_vm = */ false, // current-process identity arrives with S-6
        FaultContext::Normal,
        /* is_traced = */ false,
        TrapStyle::NoEntry,
    );

    match outcome {
        ExceptionOutcome::KernelPanic(v) => {
            // Console diagnostics before dying: the panic handler cannot
            // render the formatted message (no fmt on early console).
            use minix_plat::{EarlyConsole, CurrentEarlyConsole as Console};
            Console::write_str("trap: vector ");
            Console::write_hex(v.get() as u64);
            Console::write_str(" err ");
            Console::write_hex(frame.errcode);
            Console::write_str(" rip ");
            Console::write_hex(frame.rip);
            Console::write_str(" cs ");
            Console::write_hex(frame.cs);
            Console::write_str(" rflags ");
            Console::write_hex(frame.rflags);
            Console::write_str(" rsp ");
            Console::write_hex(frame.rsp);
            Console::write_str(" ss ");
            Console::write_hex(frame.ss);
            // TEMP-DEBUG (E8): remove after bootstrap bring-up.
            Console::write_str(" cr2 ");
            Console::write_hex(<X86_64ExceptionFrame as ExceptionArch>::page_fault_address().get());
            for (name, val) in [
                ("rdi", frame.rdi),
                ("rsi", frame.rsi),
                ("rdx", frame.rdx),
                ("rcx", frame.rcx),
                ("r8", frame.r8),
                ("r9", frame.r9),
                ("rax", frame.rax),
                ("rbx", frame.rbx),
            ] {
                Console::write_str(" ");
                Console::write_str(name);
                Console::write_str(" ");
                Console::write_hex(val);
            }
            Console::write_str("\n");
            panic!(
                "kernel exception vector {} at rip {:#x} errcode {:#x}",
                v.get(),
                frame.rip,
                frame.errcode
            );
        }
        other => panic!(
            "trap_dispatch: user-origin outcome {other:?} needs per-CPU process \
             context (S-6/S-7); reached at vector {vector:#04x} rip {:#x} — \
             wiring bug, no CPL3 code should exist yet",
            frame.rip
        ),
    }
}

/// E1 trap bridge — locate the current user process for the vector-33 arm.
/// Mirrors the SYSCALL body's per-CPU anchor read (proc_ptr before any lock).
fn current_ipc_proc_nr() -> crate::proc::ProcNr {
    let smp = unsafe { crate::smp_state_boot_unchecked() };
    let cpu = crate::current_cpu_id();
    smp.cpu_local(cpu)
        .and_then(|l| l.proc_ptr)
        .unwrap_or_else(|| {
            panic!(
                "int-33 IPC before scheduler bring-up (proc_ptr = None on                  cpu {cpu:?}) — wiring bug"
            )
        })
}

/// E1 trap bridge body: vector-33 IPC leg.
///
/// Contract (design doc 18, decisions 1-3):
/// - registers: RAX = src/dst endpoint (SENDA: count), RBX = message
///   pointer (SENDA: table pointer), RCX = IPC call number;
/// - the interrupted register file is persisted into the caller's saved
///   context BEFORE dispatch, so delivery-side IPC-status ORs land in the
///   same state the scheduler restores;
/// - a reply-code outcome returns through the stub (errno in RAX, status
///   RBX synced back from the context); a NoReply (Blocked) outcome never
///   returns — the BKL released by `kernel_call_finish` is re-acquired and
///   the CPU enters the scheduling loop, which restores whoever is
///   runnable (the blocked caller resumes only when its IPC completes).
///
/// # Safety
///
/// Same contract as `x86_trap_dispatch_body`: `frame` points at the
/// TrapFrame built by the asm stub on this CPU's kernel stack, and the
/// per-CPU `proc_ptr` anchor names the interrupted process.
unsafe fn x86_ipc_dispatch_body(frame: &mut TrapFrame, cur_nr: crate::proc::ProcNr) {
    // TEMP-DEBUG (E8): remove after bootstrap bring-up.
    {
        use minix_plat::{EarlyConsole, CurrentEarlyConsole as Console};
        Console::write_str("[ipc] call=");
        Console::write_hex(frame.rcx);
        Console::write_str(" frame.cs=");
        Console::write_hex(frame.cs);
        Console::write_str(" frame.ss=");
        Console::write_hex(frame.ss);
        Console::write_str(" frame.rflags=");
        Console::write_hex(frame.rflags);
        Console::write_str("\n");
    }
    let table = unsafe { crate::proc_table_boot_unchecked() };
    let cur_idx = crate::proc_table::nr_to_idx(cur_nr)
        .unwrap_or_else(|| panic!("int-33 IPC from invalid proc nr {cur_nr:?}"));

    // Decision 3: persist the user register file before any dispatch side
    // effect — delivery paths OR IPC status into this saved context.
    {
        let procs = table.procs_slice_mut();
        let caller = &mut procs[cur_idx];
        minix_arch::save_frame_to_context(frame, &mut caller.cpu_context);
    }

    // SAFETY (aliasing): same ground truth as the SYSCALL body — the caller
    // lives inside the table; the two &mut never touch the same bytes at
    // the same time (kernel_call_finish / dispatch_ipc_entry consume them
    // in disjoint parameter roles).
    let caller = unsafe {
        &mut *(core::ptr::addr_of_mut!(table.procs_slice_mut()[cur_idx]))
    };

    // Pre-decode: unknown call numbers exit before the BKL is taken —
    // dispatch_ipc_entry's own decode-fail return path also skips the BKL
    // (EBADCALL without acquiring), so kernel_call_finish must not run.
    // C: proc.c:602-606 — do_ipc's default branch, same effect.
    let call_nr = frame.rcx as i32;
    if crate::ipc::IpcCall::from_raw(call_nr).is_none() {
        frame.rax = crate::errno::EBADCALL as i64 as u64;
        return;
    }

    // Register extraction (design decision 2).
    let call_nr = frame.rcx as i32;
    let r1 = frame.rax; // src/dst endpoint, or SENDA count
    let r2 = frame.rbx; // message pointer, or SENDA table pointer
    let is_senda = call_nr == (crate::ipc::IpcCall::SendA as i32);
    caller.p_defer.r2 = r1 as usize;
    caller.p_defer.r3 = if is_senda { r2 as usize } else { 0 };

    // Copy the user message (kernel-side copy, TOCTOU defense — the same
    // shape as kernel_call's own copy). SENDA carries no message buffer:
    // mini_senda reads entries from the user table directly (proc.c:683).
    let mut msg = minix_types::Message::default();
    if !is_senda {
        use crate::ipc::UserCopy as _;
        match crate::ipc::KernelUserCopy.copy_msg_from_user(VirBytes(r2)) {
            Ok(m) => msg = m,
            Err(_) => {
                // C system.c:152-155 parity (kernel_call's copy arm):
                // SIGSEGV + EFAULT, without entering IPC dispatch.
                crate::syscall_signal::cause_signal(
                    caller.p_nr,
                    crate::syscall_signal::SIGSEGV,
                    table,
                    unsafe { crate::priv_table_boot_unchecked() },
                );
                frame.rax = crate::errno::EFAULT as i64 as u64;
                return;
            }
        }
    }
    msg.m_type = call_nr;
    msg.m_source = caller.p_endpoint;

    // BKL: dispatch_ipc_entry acquires and transfers out (held across
    // kernel_call_finish, which releases it on every non-VmSuspend path).
    let priv_table = unsafe { crate::priv_table_boot_unchecked() };
    let result = crate::syscall::dispatch_ipc_entry(caller, &mut msg, priv_table, table);
    crate::syscall::kernel_call_finish(caller, &msg, result, table, priv_table);

    // Delivered (reply code): errno rides RAX out through the stub's
    // iretq; the IPC status bits were ORed into the saved context's RBX by
    // the delivery path — pull that register back into the frame so the
    // stub restores the up-to-date value (C: status lives in p_reg.bx).
    // Blocked (NoReply): leave RAX untouched — the caller must not observe
    // a return value; it stays unrunnable until its IPC completes.
    if let Some(code) = result.reply_code() {
        frame.rax = code as i64 as u64;
        minix_arch::sync_status_register_to_frame(&caller.cpu_context, frame);
    } else {
        // Enter the scheduling loop; never returns to this frame.
        reenter_scheduler();
    }
}

/// Re-acquire the BKL (released by `kernel_call_finish`) and enter the
/// scheduling loop — the unified exit for blocked IPC (design decision 3).
fn reenter_scheduler() -> ! {
    // scheduler_loop consumes the held-BKL convention (assume_held witness
    // inside); the guard is forgotten deliberately — ownership passes to
    // the ambient held-BKL scope the same way dispatch_ipc_entry's
    // transfer() hands the lock off.
    let guard = crate::smp::bkl_lock();
    core::mem::forget(guard);
    let cpu = crate::current_cpu_id();
    crate::scheduler_loop(cpu)
}

/// B-path body: SYSCALL entry → `kernel_call` (trap-1).
///
/// Registered via `minix_arch::register_trap_dispatchers`; reached from
/// `x86_syscall_entry`. Returns the reply code in the frame's `rax`; the
/// stub then restores and executes `sysretq`.
///
/// # Safety
///
/// `frame` points at the TrapFrame built by the LSTAR stub on the per-CPU
/// kernel stack.
pub unsafe extern "C" fn x86_syscall_dispatch_body(frame: &mut TrapFrame) {
    // Per-CPU anchor read before any lock — C parity: `proc_ptr` is read at
    // trap entry without the BKL (it is the dispatch anchor itself). The
    // BKL is acquired inside `kernel_call` → `kernel_call_dispatch`
    // (R-03/R-05 guard transfer) and released by `kernel_call_finish`.
    //
    // Pre-S-6 reality: no scheduler step ever ran, so `proc_ptr` is None and
    // no user code could have executed SYSCALL — panic = wiring bug alarm.
    let smp = unsafe { crate::smp_state_boot_unchecked() };
    let bsp = smp.bsp_cpu_id();
    let cur_nr = smp
        .cpu_local(bsp)
        .and_then(|l| l.proc_ptr)
        .unwrap_or_else(|| {
            panic!(
                "SYSCALL before scheduler bring-up (proc_ptr = None) — the \
                 syscall data path activates with S-6/S-7"
            )
        });
    let cur_idx = crate::proc_table::nr_to_idx(cur_nr)
        .unwrap_or_else(|| panic!("SYSCALL from invalid proc nr {cur_nr:?}"));

    // User message pointer ABI: RDI (first SysV argument register).
    let m_user = VirBytes::new(frame.rdi);

    let table = unsafe { crate::proc_table_boot_unchecked() };
    // SAFETY (aliasing, C-exact semantics): `caller` lives INSIDE the table —
    // that is the ground truth the C syscall path runs on (`caller` is
    // `proc_addr(CPU->proc_nr)`; every C pointer here aliases). Rust's
    // `kernel_call`/`kernel_call_dispatch` signature models the test shape
    // (caller as a handle disjoint from the table) and cannot express this
    // without the split below. The two `&mut` derive from the same
    // `SyncUnsafeCell` static escape that every `*_boot_unchecked` accessor
    // already provides; `kernel_call` itself only ever reaches the caller
    // through `caller` and the table through its own parameter — the two
    // references never use the same bytes for the same field at the same
    // time, matching C. Disjoint-API refactor of the chain (caller by nr)
    // is registered as the S-6 blocker in smp_todo.md S-8.
    let caller = unsafe {
        &mut *(core::ptr::addr_of_mut!(table.procs_slice_mut()[cur_idx]))
    };
    let result = crate::syscall::kernel_call(
        caller,
        m_user,
        table,
        unsafe { crate::priv_table_boot_unchecked() },
        unsafe { crate::clock_state_boot_unchecked() },
        &crate::ipc::KernelUserCopy,
    );

    // Reply code → RAX for the sysret leg (NoReply/VmSuspend must not occur
    // for the SYSCALL fast path — VMSUSPEND arrives via the IPC trap path;
    // treat them as a wiring bug rather than replying garbage).
    frame.rax = match result.reply_code() {
        Some(code) => code as i64 as u64,
        None => panic!("trap_dispatch: syscall returned {result:?} with no reply code"),
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exception_frame_carries_cpu_tail() {
        // The dispatcher's view is exactly the CPU-pushed fields; GPRs stay
        // in the TrapFrame and are restored by the asm leg.
        let frame = TrapFrame {
            rax: 1,
            rbx: 2,
            rcx: 3,
            rdx: 4,
            rsi: 5,
            rdi: 6,
            rbp: 7,
            r8: 8,
            r9: 9,
            r10: 10,
            r11: 11,
            r12: 12,
            r13: 13,
            r14: 14,
            r15: 15,
            vector: 14,
            errcode: 2,
            rip: 0x1000,
            cs: 0x1B,
            rflags: 0x202,
            rsp: 0x7FFF_0000,
            ss: 0x23,
        };
        let exc = exception_frame_of(&frame);
        assert_eq!(X86_64ExceptionFrame::vector(&exc).get(), 14);
        assert_eq!(X86_64ExceptionFrame::error_code(&exc), 2);
        assert_eq!(
            X86_64ExceptionFrame::instruction_pointer(&exc),
            VirBytes::new(0x1000)
        );
        assert!(X86_64ExceptionFrame::is_user_mode(&exc));
        assert!(X86_64ExceptionFrame::is_write_fault(&exc));
    }

    #[test]
    fn spurious_vector_dispatch_completes() {
        // S-9 (D-38①): the LAPIC spurious vector (0xFF) returns WITHOUT
        // dispatching and without panicking — exercises the full entry path
        // (asm→thunk contract aside, the body's early-return arm) hosted,
        // including the BKL witness acquisition.
        let mut frame = TrapFrame {
            rax: 0, rbx: 0, rcx: 0, rdx: 0, rsi: 0, rdi: 0, rbp: 0,
            r8: 0, r9: 0, r10: 0, r11: 0, r12: 0, r13: 0, r14: 0, r15: 0,
            vector: 0xFF,
            errcode: 0,
            rip: 0, cs: 0x08, rflags: 0, rsp: 0, ss: 0,
        };
        // SAFETY: hosted test body; the frame is a local.
        unsafe { x86_trap_dispatch_body(&mut frame) };
    }

    #[test]
    fn reply_code_none_is_a_wiring_bug_marker() {
        // The B leg panics on NoReply/VmSuspend rather than replying
        // garbage; this pins that those variants exist and are distinct
        // from Ok/BadCall/CallDenied (which carry a reply code).
        assert_eq!(KcallResult::Ok(7).reply_code(), Some(7));
        assert!(KcallResult::NoReply.reply_code().is_none());
        assert!(KcallResult::VmSuspend.reply_code().is_none());
    }
}
