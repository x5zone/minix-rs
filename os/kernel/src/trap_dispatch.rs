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
//! - **User-origin traps** (S-6/S-7 wired): the body captures the per-CPU
//!   current process, acquires the BKL (user code runs without it —
//!   `finish_and_restore` released it), and acts on the outcome:
//!   `ForwardToVm` sets RTS_PAGEFAULT and sends the `VM_PAGEFAULT`
//!   message to VM FROM_KERNEL (C: `pagefault()` — exception.c:112-129);
//!   `Signal` calls `cause_signal` (C: `cause_sig` — exception.c:276);
//!   VM's own fault panics (C: "pagefault in VM" — exception.c:101-118).
//!   Both acting arms end in the scheduling loop (C: mpx.S
//!   `jmp switch_to_user`). Outcomes with no acting stage yet (FpuTrap —
//!   lazy-FPU restore, recovery redirects, traced-debug, spurious-NMI
//!   resume) still panic with the outcome attached — registered gaps,
//!   not reachable from the current carriers.
//! - **Syscall body**: reads the per-CPU `proc_ptr` anchor before any lock
//!   (C reads `proc_ptr` at trap entry the same way), then runs the
//!   existing `kernel_call` wrapper — which acquires/releases the BKL
//!   around dispatch+finish itself. The user message pointer travels in
//!   RDI (first SysV argument register); x86-64 is a new port, C's 32-bit
//!   register choice does not transfer (doc 13 §6.3 documents the ABI).

use crate::syscall::KcallResult;
use minix_arch::exception::{ExceptionArch, FaultContext};
use minix_arch::exception_dispatcher::{ExceptionDispatcher, ExceptionOutcome, ExceptionSignal};
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

    // LAPIC local timer (edge1 K6): the per-CPU one-shot tick never
    // enters the hook chains — its handler re-arms the ICR and services
    // per-CPU work directly (C: `lapic_timer_int_handler`, apic.c:913,
    // registered straight in the IDT, with its own EOI). The local-tick
    // body owns the EOI via the arch `local_timer_eoi`.
    //
    // K6-v2 quantum enforcement: after the re-arm + EOI, evaluate
    // quantum expiry for this CPU's running process (C: clock_handler
    // per-CPU arm, proc.c:418-424 — if the quantum is exhausted, call
    // sched_proc_no_time to notify the user scheduler or transition to
    // the kernel-scheduled renewal path). The SSI trap entry cleared
    // SIE; the sret after the handler restores it (SPIE=1), so the
    // preempted process's instruction stream resumes without disruption.
    if vector == 0xF1 {
        crate::clock::local_tick(crate::current_cpu_id());
        // Quantum enforcement (C proc.c:418-424): the tick CPU evaluates
        // its own running process — if the quantum is exhausted, the
        // process is preempted (NO_QUANTUM set, scheduler notified).
        let section = unsafe { crate::smp::BklSection::assume_held() };
        let cur_nr = {
            let smp = crate::smp_state_with(&section);
            smp.cpu_local(crate::current_cpu_id())
                .and_then(|l| l.proc_ptr)
        };
        if let Some(nr) = cur_nr {
            let table = crate::proc_table_with(&section);
            let priv_table = crate::priv_table_with(&section);
            if table.get(nr).is_some_and(|p| p.is_runnable()) {
                table.check_quantum(nr, priv_table, &section);
            }
        }
        return;
    }

    if let Some(irq) = minix_arch::x86_64::trap_stub::irq_of_vector(vector) {
        // Profile-clock PC handoff (C reads p->p_reg.pc, which the asm
        // entry already saved into the process context; the Rust IRQ path
        // never saves the frame, so the interrupted rip travels to
        // `profile_clock_hook` through the one-shot slot instead). The
        // SPROFILING guard keeps a line fire outside an active profiling
        // run from leaving a stale value in the slot.
        if irq == minix_plat::PROFILE_CLOCK_IRQ.get()
            && crate::misc::SPROFILING.load(core::sync::atomic::Ordering::Acquire)
        {
            crate::misc::stash_profile_pc(frame.rip);
        }
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
    // S-9 (D-38①) — BKL ownership at the exception entry, per origin:
    // - **Kernel-origin** (is_user = false): the interrupted context owns
    //   the BKL, so the body INHERITS it via `assume_held` (C parity: trap
    //   handlers run with the BKL owned by the interrupted context; trap.c
    //   takes no lock — acquiring here would deadlock on the non-reentrant
    //   CAS lock for the normal kernel-origin case).
    // - **User-origin** (is_user = true): user code runs WITHOUT the BKL
    //   (`finish_and_restore` releases it before the context restore —
    //   C: context_stop must_bkl_unlock, arch_clock.c:226-233), so the
    //   entry ACQUIRES it (C: trap entry BKL_LOCK from ring 3). Every
    //   acting arm below (page-fault forwarding, signal) mutates shared
    //   tables and must run under the lock; the scheduling loop reuses the
    //   held-BKL convention on exit.
    let mut exc = exception_frame_of(frame);
    let is_user = X86_64ExceptionFrame::is_user_mode(&exc);
    let section = if is_user {
        crate::smp::bkl_lock_section()
    } else {
        unsafe { crate::smp::BklSection::assume_held() }
    };
    // Per-CPU current process (C: `saved_proc = get_cpulocal_var(proc_ptr)`
    // — exception.c:186). Only user-origin outcomes act on it; the read
    // follows the same dispatch-anchor convention as the SYSCALL body
    // (proc_ptr is read as the anchor itself). None = no scheduler step
    // ever ran on this CPU, so no CPL3 code could have faulted — wiring
    // bug, same alarm as the SYSCALL arm.
    let cur_nr = if is_user {
        let smp = unsafe { crate::smp_state_boot_unchecked() };
        let cpu = crate::current_cpu_id();
        Some(
            smp.cpu_local(cpu)
                .and_then(|l| l.proc_ptr)
                .unwrap_or_else(|| {
                    panic!(
                        "user exception before scheduler bring-up (proc_ptr = \
                         None on cpu {cpu:?}) — wiring bug"
                    )
                }),
        )
    } else {
        None
    };
    // is_vm feeds the page-fault classification (C: `pr->p_endpoint ==
    // VM_PROC_NR` — exception.c:101; VM's own faults cannot be forwarded
    // to VM itself). Kernel-origin faults never reach that check (the
    // nested path panics first), matching C's is_nested ordering.
    let is_vm = cur_nr == Some(crate::proc::proc_nr::VM_PROC_NR);
    let outcome = ExceptionDispatcher::<X86_64ExceptionFrame>::handle(
        &mut exc,
        /* is_nested = */ !is_user,
        is_vm,
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
        ExceptionOutcome::VmPageFault => {
            // VM's own user page fault — forwarding to VM would deadlock
            // on itself. C: pagefault() prints the frame summary and
            // panics ("pagefault in VM") — exception.c:101-118.
            use minix_plat::{EarlyConsole, CurrentEarlyConsole as Console};
            Console::write_str("pagefault for VM on cpu ");
            Console::write_hex(crate::current_cpu_id().raw() as u64);
            Console::write_str(" rip ");
            Console::write_hex(frame.rip);
            Console::write_str("\n");
            panic!("pagefault in VM");
        }
        ExceptionOutcome::ForwardToVm(pf) => {
            let cur_nr = cur_nr.expect("ForwardToVm implies a user-origin fault");
            // Persist the user register file BEFORE any dispatch side
            // effect (design decision 3, same shape as the int-33 IPC
            // arm) — VM resolves the fault while the process is parked on
            // RTS_PAGEFAULT, and the resume must re-execute the faulting
            // instruction against this saved state (C: mpx.S
            // SAVE_PROCESS_CTX already ran by the time exception_handler
            // dispatches).
            {
                let table = crate::proc_table_with(&section);
                let proc = table
                    .get_mut(cur_nr)
                    .unwrap_or_else(|| panic!("pagefault from invalid proc nr {cur_nr:?}"));
                minix_arch::save_frame_to_context(frame, &mut proc.cpu_context);
            }
            // C: pagefault() tail — RTS_PAGEFAULT, VM_PAGEFAULT message,
            // FROM_KERNEL mini_send (exception.c:112-129). Send errors
            // panic here (C: panic "WARNING: pagefault: mini_send
            // returned %d").
            if let Err(e) = forward_pagefault_to_vm(
                crate::proc_table_with(&section),
                crate::priv_table_with(&section),
                cur_nr,
                pf.vaddr.0,
                frame.errcode as u32,
            ) {
                panic!("pagefault: mini_send returned {e:?}");
            }
            // C: pagefault() returns → mpx.S `jmp switch_to_user`. The
            // process stays non-runnable (RTS_PAGEFAULT) until VM's
            // SYS_VMCTL ClearPageFault re-enqueues it (do_vmctl.c:35);
            // the scheduling loop picks whoever is runnable next.
            crate::scheduler_loop(crate::current_cpu_id())
        }
        ExceptionOutcome::Signal(sig) => {
            let cur_nr = cur_nr.expect("Signal implies a user-origin exception");
            // Persist first: the signal manager's later SIGSEND delivery
            // builds the handler trampoline on top of the fault-time
            // register file (C: sig_proc reshapes p_reg, which holds the
            // trap-saved state).
            {
                let table = crate::proc_table_with(&section);
                let proc = table
                    .get_mut(cur_nr)
                    .unwrap_or_else(|| panic!("exception from invalid proc nr {cur_nr:?}"));
                minix_arch::save_frame_to_context(frame, &mut proc.cpu_context);
            }
            // C: cause_sig(proc_nr(saved_proc), ep->signum) —
            // exception.c:276. RTS_SIGNALED parks the process until its
            // signal manager resolves the delivery (kernel-side behavior
            // is complete at cause_sig).
            let sig_nr = exception_signal_to_nr(sig);
            crate::syscall_signal::cause_signal(
                cur_nr,
                sig_nr,
                crate::proc_table_with(&section),
                crate::priv_table_with(&section),
            );
            // C: cause_sig returns → mpx.S `jmp switch_to_user`.
            crate::scheduler_loop(crate::current_cpu_id())
        }
        other => {
            // Console diagnostics before dying: these outcomes are typed
            // but their acting stages are not wired into this trap path
            // yet, so reaching them is a wiring gap, not recoverable
            // state. Each names its registered owner:
            // - FpuTrap: lazy-FPU restore stage (save owner / restore
            //   self / clts) — FPU ownership wiring, X-8 / T5 wave.
            // - RedirectToRecovery / PhysCopyFault: the kernel copy paths
            //   do not thread FaultContext into this entry yet.
            // - ClearTrapFlag: is_traced / kern_trap_style are not
            //   threaded from the saved PSW yet.
            // - SpuriousNmi: C prints-and-returns (exception.c:191-195);
            //   the vector-2 resume leg is unwired.
            use minix_plat::{EarlyConsole, CurrentEarlyConsole as Console};
            Console::write_str("trap: vector ");
            Console::write_hex(vector as u64);
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
                "trap_dispatch: outcome {other:?} has no acting stage wired \
                 (registered gap, not recoverable) — vector {vector:#04x} rip {:#x}",
                frame.rip
            )
        }
    }
}

/// Map the arch-generic exception classification to the kernel signal
/// number (C: the `ex_data[].signum` column — exception.c:19-39; constants
/// signal.h:55-63).
fn exception_signal_to_nr(sig: ExceptionSignal) -> u32 {
    match sig {
        ExceptionSignal::Fpe => crate::syscall_signal::SIGFPE,
        ExceptionSignal::Ill => crate::syscall_signal::SIGILL,
        ExceptionSignal::Segv => crate::syscall_signal::SIGSEGV,
        ExceptionSignal::Bus => crate::syscall_signal::SIGBUS,
        ExceptionSignal::Emt => crate::syscall_signal::SIGEMT,
        ExceptionSignal::Trap => crate::syscall_signal::SIGTRAP,
    }
}

/// The page-fault forwarding arm's acting half — C: `pagefault()` tail,
/// exception.c:112-129.
///
/// 1. `RTS_SET(pr, RTS_PAGEFAULT)` (exception.c:115) — via the
///    scheduler-aware `rts_set`, which is the RTS_SET macro's dequeue
///    half; the primitive flag set in `page_fault::set_pagefault_pending`
///    alone would leave the process queued while non-runnable
///    (pick_proc returns queue heads without a runnable re-check).
/// 2. Build the `VM_PAGEFAULT` message (m_source = faulting endpoint,
///    VPF_ADDR = fault address, VPF_FLAGS = raw error code —
///    exception.c:118-122) and `mini_send` it to VM FROM_KERNEL
///    (exception.c:123-125).
///
/// Returns `Err(errno)` when the send fails; the caller panics (C:
/// `panic("WARNING: pagefault: mini_send returned %d")`).
fn forward_pagefault_to_vm(
    proc_table: &mut crate::proc_table::ProcessTable,
    priv_table: &mut crate::kpriv::PrivTable,
    cur_nr: crate::proc::ProcNr,
    fault_addr: u64,
    error_code: u32,
) -> Result<crate::ipc::IpcOutcome, crate::ipc::IpcError> {
    use crate::ipc::{IpcEngine, KernelUserCopy, SendFlags};
    use crate::proc::RtsFlagsBits;

    proc_table.rts_set(cur_nr, RtsFlagsBits::PAGEFAULT);
    // Rust-side diagnostic record (the field `set_pagefault_pending`
    // maintains; C keeps no per-process fault address — the address
    // travels in the message alone).
    if let Some(p) = proc_table.get_mut(cur_nr) {
        p.p_fault_addr = Some(fault_addr);
    }
    let src_endpoint = proc_table
        .get(cur_nr)
        .map(|p| p.p_endpoint)
        .unwrap_or(minix_types::Endpoint::NONE);
    let msg = crate::page_fault::build_vm_pagefault_msg(src_endpoint, fault_addr, error_code);
    // Send target: the VM process's own endpoint (C: mini_send(pr,
    // VM_PROC_NR, ...) resolves the slot through the table; a dead VM
    // slot is the EDEADSRCDST path).
    let dst_endpoint = proc_table
        .get(crate::proc::proc_nr::VM_PROC_NR)
        .map(|p| p.p_endpoint)
        .ok_or(crate::ipc::IpcError::DeadSrcDst)?;
    let mut engine = IpcEngine::new(proc_table.procs_slice_mut(), priv_table, &KernelUserCopy);
    let outcome = engine.send(cur_nr, dst_endpoint, &msg, SendFlags::FROM_KERNEL);
    match outcome {
        crate::ipc::IpcOutcome::Error(e) => Err(e),
        delivered_or_blocked => Ok(delivered_or_blocked),
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
    let table = unsafe { crate::proc_table_boot_unchecked() };

    // Decision 3: persist the user register file before any dispatch side
    // effect — delivery paths OR IPC status into this saved context.
    {
        let caller = table
            .get_mut(cur_nr)
            .unwrap_or_else(|| panic!("int-33 IPC from invalid proc nr {cur_nr:?}"));
        minix_arch::save_frame_to_context(frame, &mut caller.cpu_context);
    }

    // K20 (caller-by-nr): no laundering — the caller travels as its nr and
    // every access re-borrows the slot from `table` at the point of use.

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
    {
        let caller = table
            .get_mut(cur_nr)
            .expect("int-33 IPC: caller slot must exist");
        caller.p_defer.r2 = r1 as usize;
        caller.p_defer.r3 = if is_senda { r2 as usize } else { 0 };
    }

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
                    cur_nr,
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
    msg.m_source = table
        .get(cur_nr)
        .map(|c| c.p_endpoint)
        .expect("int-33 IPC: caller slot must exist");

    // BKL: dispatch_ipc_entry acquires and transfers out (held across
    // kernel_call_finish, which releases it on every non-VmSuspend path).
    let priv_table = unsafe { crate::priv_table_boot_unchecked() };
    let result = crate::syscall::dispatch_ipc_entry(cur_nr, table, &mut msg, priv_table);
    crate::syscall::kernel_call_finish(cur_nr, table, &msg, result, priv_table);

    // Delivered (reply code): errno rides RAX out through the stub's
    // iretq; the IPC status bits were ORed into the saved context's RBX by
    // the delivery path — pull that register back into the frame so the
    // stub restores the up-to-date value (C: status lives in p_reg.bx).
    // Blocked (NoReply): leave RAX untouched — the caller must not observe
    // a return value; it stays unrunnable until its IPC completes.
    if let Some(code) = result.reply_code() {
        frame.rax = code as i64 as u64;
        let ctx = &table
            .get(cur_nr)
            .expect("int-33 IPC: caller slot must exist")
            .cpu_context;
        minix_arch::sync_status_register_to_frame(ctx, frame);
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

    // User message pointer ABI: RDI (first SysV argument register).
    let m_user = VirBytes::new(frame.rdi);

    let table = unsafe { crate::proc_table_boot_unchecked() };
    // K20 (caller-by-nr): no laundering — the caller travels as its nr.
    let result = crate::syscall::kernel_call(
        cur_nr,
        table,
        m_user,
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

    #[test]
    fn exception_signal_maps_to_c_signum() {
        // C: the ex_data[].signum column (exception.c:19-39) — the
        // classification lives in the arch dispatcher, the numeric signal
        // in this kernel signal module; the mapping must hit the C
        // constants exactly (signal.h:55-63).
        use minix_arch::exception_dispatcher::ExceptionSignal;
        assert_eq!(exception_signal_to_nr(ExceptionSignal::Fpe), crate::syscall_signal::SIGFPE);
        assert_eq!(exception_signal_to_nr(ExceptionSignal::Ill), crate::syscall_signal::SIGILL);
        assert_eq!(exception_signal_to_nr(ExceptionSignal::Segv), crate::syscall_signal::SIGSEGV);
        assert_eq!(exception_signal_to_nr(ExceptionSignal::Bus), crate::syscall_signal::SIGBUS);
        assert_eq!(exception_signal_to_nr(ExceptionSignal::Emt), crate::syscall_signal::SIGEMT);
        assert_eq!(exception_signal_to_nr(ExceptionSignal::Trap), crate::syscall_signal::SIGTRAP);
    }

    /// Shape a test table with a faulting user proc (nr 0) and the VM
    /// slot (VM_PROC_NR), both occupied with distinct endpoints.
    fn pagefault_test_table() -> (crate::test_helpers::TestProcTable, crate::proc::ProcNr) {
        use crate::proc::RtsFlagsBits;
        let mut table = crate::test_helpers::test_proc_table();
        let caller_nr = crate::proc::ProcNr(0);
        {
            let p = table.get_mut(caller_nr).unwrap();
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            p.p_endpoint = minix_types::Endpoint(100);
        }
        {
            let p = table.get_mut(crate::proc::proc_nr::VM_PROC_NR).unwrap();
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            p.p_endpoint = minix_types::Endpoint(8);
        }
        (table, caller_nr)
    }

    #[test]
    fn forward_pagefault_blocks_when_vm_not_receiving() {
        // C: mini_send(pr, VM_PROC_NR, &m_pagefault, FROM_KERNEL) with VM
        // busy → the faulting process parks on the VM caller queue,
        // carrying RTS_PAGEFAULT (the scheduler must skip it until VM
        // resolves — exception.c:115 + proc.c:938-960).
        use crate::ipc::IpcOutcome;
        use crate::proc::{proc_nr, RtsFlagsBits};
        let (mut table, caller_nr) = pagefault_test_table();
        let mut priv_table = crate::test_helpers::test_priv_table();

        let outcome = forward_pagefault_to_vm(&mut table, &mut priv_table, caller_nr, 0x4000, 0x4);
        assert!(matches!(outcome, Ok(IpcOutcome::Blocked)));
        {
            let p = table.get(caller_nr).unwrap();
            assert!(p.p_rts_flags.is_set(RtsFlagsBits::PAGEFAULT));
            assert!(!p.is_runnable(), "faulting process must not stay schedulable");
            assert!(p.p_rts_flags.is_set(RtsFlagsBits::SENDING));
            assert_eq!(p.p_sendto_e, minix_types::Endpoint(8));
            assert_eq!(p.p_fault_addr, Some(0x4000));
        }
        assert_eq!(
            table.get(proc_nr::VM_PROC_NR).unwrap().caller_q_head,
            Some(caller_nr),
            "caller must be enqueued on VM's caller queue"
        );
    }

    #[test]
    fn forward_pagefault_delivers_to_receiving_vm() {
        // Path A: VM in RECEIVE for the faulting endpoint → direct
        // delivery; the delivered message is the VM_PAGEFAULT wire shape
        // (m_source = faulting endpoint, VPF_ADDR, VPF_FLAGS —
        // exception.c:118-122), and the faulting process is parked on
        // RTS_PAGEFAULT alone (delivery cleared nothing for the sender).
        use crate::ipc::IpcOutcome;
        use crate::proc::{proc_nr, RtsFlagsBits};
        let (mut table, caller_nr) = pagefault_test_table();
        {
            let vm = table.get_mut(proc_nr::VM_PROC_NR).unwrap();
            vm.p_rts_flags.set(RtsFlagsBits::RECEIVING);
            vm.p_getfrom_e = minix_types::Endpoint(100);
        }
        let mut priv_table = crate::test_helpers::test_priv_table();

        let outcome = forward_pagefault_to_vm(&mut table, &mut priv_table, caller_nr, 0xCAFE, 0x6);
        assert!(matches!(outcome, Ok(IpcOutcome::Delivered)));
        {
            let vm = table.get(proc_nr::VM_PROC_NR).unwrap();
            assert!(vm.p_misc_flags.is_set(crate::proc::MiscFlagsBits::DELIVERMSG));
            assert_eq!(vm.p_delivermsg.m_type, minix_types::VM_PAGEFAULT as i32);
            assert_eq!(vm.p_delivermsg.m_source, minix_types::Endpoint(100));
            // SAFETY: m_vm_pagefault is the active arm — the kernel just
            // wrote it via build_vm_pagefault_msg.
            let pf = unsafe { vm.p_delivermsg.m_u.m_vm_pagefault };
            assert_eq!(pf.vpf_addr, 0xCAFE);
            assert_eq!(pf.vpf_flags, 0x6);
        }
        {
            let p = table.get(caller_nr).unwrap();
            assert!(p.p_rts_flags.is_set(RtsFlagsBits::PAGEFAULT));
            assert!(
                !p.is_runnable(),
                "delivered != resolved: parked until SYS_VMCTL ClearPageFault"
            );
        }
    }

    #[test]
    fn forward_pagefault_errors_on_dead_vm_slot() {
        // C: mini_send error → the caller panics ("WARNING: pagefault:
        // mini_send returned %d"). The helper surfaces the errno as Err
        // for the trap body's panic; RTS_PAGEFAULT was still set first
        // (exception.c:115 precedes the send).
        use crate::ipc::IpcError;
        use crate::proc::RtsFlagsBits;
        let (mut table, caller_nr) = pagefault_test_table();
        {
            let vm = table.get_mut(crate::proc::proc_nr::VM_PROC_NR).unwrap();
            vm.p_rts_flags.set(RtsFlagsBits::NO_ENDPOINT);
        }
        let mut priv_table = crate::test_helpers::test_priv_table();

        let outcome = forward_pagefault_to_vm(&mut table, &mut priv_table, caller_nr, 0x4000, 0x4);
        assert_eq!(outcome, Err(IpcError::DeadSrcDst));
        let p = table.get(caller_nr).unwrap();
        assert!(p.p_rts_flags.is_set(RtsFlagsBits::PAGEFAULT));
    }
}
