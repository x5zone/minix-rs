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

    // LAPIC spurious interrupt (C: apic.c apic_spurious_interrupt): no
    // handler chain, no EOI — the spurious vector never enters IRR/ISR.
    if vector == 0xFF {
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
    fn reply_code_none_is_a_wiring_bug_marker() {
        // The B leg panics on NoReply/VmSuspend rather than replying
        // garbage; this pins that those variants exist and are distinct
        // from Ok/BadCall/CallDenied (which carry a reply code).
        assert_eq!(KcallResult::Ok(7).reply_code(), Some(7));
        assert!(KcallResult::NoReply.reply_code().is_none());
        assert!(KcallResult::VmSuspend.reply_code().is_none());
    }
}
