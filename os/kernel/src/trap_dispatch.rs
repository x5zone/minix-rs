//! Kernel-side trap/syscall dispatch bodies.
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
//! - **User-origin traps**: the body captures the per-CPU
//!   current process, acquires the BKL (user code runs without it —
//!   `finish_and_restore` released it), and acts on the outcome:
//!   `ForwardToVm` sets RTS_PAGEFAULT and sends the `VM_PAGEFAULT`
//!   message to VM FROM_KERNEL (C: `pagefault()` — exception.c:112-129);
//!   `Signal` calls `cause_signal` (C: `cause_sig` — exception.c:276);
//!   VM's own fault panics (C: "pagefault in VM" — exception.c:101-118).
//!   Both acting arms end in the scheduling loop (C: mpx.S
//!   `jmp switch_to_user`).
//! - **Recovery-class outcomes**: `SpuriousNmi` prints
//!   and returns (C: exception.c:191-194); `ClearTrapFlag` clears the
//!   trace bit in the frame and returns (C: exception.c:243-245, fed by
//!   the saved-PSW/trap-style legitimacy inputs); `RedirectToRecovery` /
//!   `PhysCopyFault` are invariant panics — the kernel copy paths are
//!   validate-first (ipc.rs / vm.rs), so C's RIP-redirect recovery
//!   (klib.S labels) has no producer here. Only `FpuTrap` remains a
//!   registered gap (lazy-FPU restore, X-8 / T5 wave).
//! - **Syscall body**: reads the per-CPU `proc_ptr` anchor before any lock
//!   (C reads `proc_ptr` at trap entry the same way), then runs the
//!   existing `kernel_call` wrapper — which acquires/releases the BKL
//!   around dispatch+finish itself. The user message pointer travels in
//!   RDI (first SysV argument register); x86-64 is a new port, C's 32-bit
//!   register choice does not transfer (doc 13 §6.3 documents the ABI).

use crate::syscall::KcallResult;
use minix_arch::TrapStyle;
use minix_arch::exception::{ExceptionArch, FaultContext};
use minix_arch::exception_dispatcher::{ExceptionDispatcher, ExceptionOutcome, ExceptionSignal};
#[cfg(target_arch = "x86_64")]
use minix_arch::x86_64::exception::X86_64ExceptionFrame;
#[cfg(target_arch = "x86_64")]
use minix_arch::x86_64::trap_stub::TrapFrame;
use minix_types::VirBytes;

/// The int-33 IPC gate (C: IPC_VECTOR_ORIG = 33, interrupt.h:33).
#[cfg(target_arch = "x86_64")]
const IPC_VECTOR_GATE: u8 = 33;

/// RFLAGS trace bit — single-step flag (C: `TRACEBIT` 0x0100,
/// i386 archconst.h:120). Read from the saved process PSW for the
/// nested-debug legitimacy check; cleared in the frame by the
/// ClearTrapFlag acting arm (exception.c:243).
#[cfg(target_arch = "x86_64")]
const TRACEBIT: u64 = 0x0100;

/// Build the CPU-pushed tail of the frame as an `X86_64ExceptionFrame` for
/// the arch-generic dispatcher (which is implemented over that type).
#[cfg(target_arch = "x86_64")]
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

#[cfg(target_arch = "x86_64")]
/// C: the `SAVE_PROCESS_CTX(0, KTS_INT_HARD)` macro body every IRQ/clock
/// stub in mpx.S runs at trap entry (mpx.S:77/137/355/540; sconst.h:75-89)
/// — `SAVE_GP_REGS` mirrors ALL interrupted user registers into
/// `saved_proc->p_reg` (EBX included; on i386 the IPC status register and
/// a callee-saved GP are the same register, ipcconst.h:10) and records
/// `p_kern_trap_style`, BEFORE any scheduling policy runs.
///
/// The fix: the IRQ/tick arms of this port never saved the frame
/// (the old comment at the profile-clock handoff admitted it), so a
/// process moved by quantum expiry or the user scheduler woke through
/// `finish_and_restore` (kernel/lib.rs, "rebuild the frame from the
/// arch-private context") with a stale register file — RBX's live
/// callee-saved value rolled back to whatever the last trap-entry save or
/// the IPC-status/ps_strings semantics left there (on hardware:
/// RS step2 saw `&self.table` = 0x0 after a pf/IPC round trip, crashing
/// in `endpoint_slot`). Kernel-origin interrupts skip: the frame holds
/// kernel registers there, and the process's user context was already
/// saved in full by its own syscall/trap entry.
///
/// # Safety
/// Mirrors the interrupted frame into the shared `PROC_TABLE`, so it must
/// run under the BKL. It acquires the lock itself via
/// [`crate::smp::bkl_lock_or_inherit`] (inheriting only when THIS CPU's
/// interrupted kernel frame already owns it) — the previous "user-origin
/// IRQs run under the interrupted context's lock" premise was false: a CPU
/// in user/idle does NOT own the BKL, so a bare `assume_held` here mutated
/// the table concurrently with a peer CPU.
unsafe fn save_irq_frame_to_context(frame: &TrapFrame) {
    let acquired = crate::smp::bkl_lock_or_inherit();
    let section = unsafe { crate::smp::BklSection::assume_held() };
    let cur_nr = {
        let smp = crate::smp_state_with(&section);
        smp.cpu_local(crate::current_cpu_id())
            .and_then(|l| l.proc_ptr)
    };
    if let Some(nr) = cur_nr {
        let table = crate::proc_table_with(&section);
        if let Some(proc) = table.get_mut(nr) {
            mirror_irq_frame_into_proc(frame, proc);
        }
    }
    if acquired {
        crate::smp::bkl_unlock();
    }
}

/// The testable core of [`save_irq_frame_to_context`]: the CPL gate plus
/// the full-context mirror. Returns whether the frame was mirrored (the
/// host-side test pins both branches without boot-global fixtures).
///
/// x86-64 only: the parameter is the x86 `TrapFrame` (`cs`/`rip`/`rbx`
/// fields); this function and its call sites serve x86_64 evidence work and
/// are compiled under an architecture gate.
#[cfg(target_arch = "x86_64")]
fn mirror_irq_frame_into_proc(
    frame: &TrapFrame,
    proc: &mut crate::proc::KProcess,
) -> bool {
    if frame.cs & 3 != 3 {
        // kernel re-entry: the frame holds kernel registers; the process's
        // user context was already saved in full by its own trap entry.
        return false;
    }
    minix_arch::save_frame_to_context(frame, &mut proc.cpu_context);
    proc.trap_style = TrapStyle::FullContext;
    true
}
#[cfg(target_arch = "x86_64")]
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
    // IPC status rides the saved-context R10 channel (moved from RBX to the
    // caller-saved R10; the MINIX_KERNINFO secondary shares the same lane),
    // and is synchronized into the frame on return
    // (sync_status_register_to_frame, or_ipc_status_reg /
    // set_secondary_ipc_return).
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

    // Scheduler IPI (C SMP_SCHED_IPI_VECTOR) → the target CPU's own
    // sched_handler_full (loads flags, applies STOP/SAVE_CTX/VM_INHIBIT,
    // clears pending — the schedule_sync waiter's completion signal).
    // BKL: `sched_handler_full` mutates the shared proc table (RTS_SET), so
    // it must run under the lock. Use owner-aware inherit-or-acquire
    // — inherit only if THIS CPU's interrupted frame owns the BKL (kernel
    // origin), otherwise spin-acquire it even when a peer holds it. The old
    // bare `bkl_try_lock` mis-inherited a peer's lock, letting two CPUs
    // `&mut PROC_TABLE` at once (the early-SMP corruption).
    if vector == minix_arch::x86_64::smp::SCHED_IPI_VECTOR as u8 {
        let acquired = crate::smp::bkl_lock_or_inherit();
        // C context_stop_idle at interrupt entry: a wake IPI that lands on
        // an idle CPU clears the flag and restarts that CPU's one-shot; the
        // handler below then sees a live tick source.
        crate::context_stop_idle_current();
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

    // LAPIC local timer: the per-CPU one-shot tick never
    // enters the hook chains — its handler re-arms the ICR and services
    // per-CPU work directly (C: `lapic_timer_int_handler`, apic.c:913,
    // registered straight in the IDT, with its own EOI). The local-tick
    // body owns the EOI via the arch `local_timer_eoi`.
    //
    // Quantum enforcement: after the re-arm + EOI, evaluate
    // quantum expiry for this CPU's running process (C: clock_handler
    // per-CPU arm, proc.c:418-424 — if the quantum is exhausted, call
    // sched_proc_no_time to notify the user scheduler or transition to
    // the kernel-scheduled renewal path). The SSI trap entry cleared
    // SIE; the sret after the handler restores it (SPIE=1), so the
    // preempted process's instruction stream resumes without disruption.
    if vector == 0xF1 {
        // C: every clock/IRQ stub in mpx.S runs SAVE_PROCESS_CTX at entry
        // before any policy code (mpx.S:77/137/355/540) — do it before the
        // quantum check so a preempt-and-wake dispatch never reads a stale
        // register file.
        unsafe { save_irq_frame_to_context(frame) };

        // One-shot LAPIC tick liveness proof: if the tick never fires while
        // RS spins bare in user mode, there is no preemption and the spin
        // owns the CPU.
        #[cfg(not(feature = "mock"))]
        {
            use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
            static TICK1: AtomicUsize = AtomicUsize::new(0);
            if TICK1.fetch_add(1, AtomicOrd::Relaxed) == 0 {
                use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
            }
        }
        crate::clock::local_tick(crate::current_cpu_id());
        // Quantum enforcement (C proc.c:418-424): the tick CPU evaluates
        // its own running process — if the quantum is exhausted, the
        // process is preempted (NO_QUANTUM set, scheduler notified).
        // The quantum check mutates the shared proc table, so it
        // must run under the BKL. Inherit only if THIS CPU's interrupted
        // frame owns it, otherwise spin-acquire — a bare `assume_held` here
        // ran lock-free on a user/idle-origin tick and let two CPUs `&mut`
        // the table at once (the early-SMP corruption).
        let tick_acquired = crate::smp::bkl_lock_or_inherit();
        let section = unsafe { crate::smp::BklSection::assume_held() };
        // C `context_stop_idle` (arch_clock.c:351-369): a tick landing on a
        // halted CPU marks it busy again and, if it *was* idle, restarts the
        // local timer. Runs under the BKL acquired just above.
        crate::context_stop_idle_current();
        let cur_nr = {
            let smp = crate::smp_state_with(&section);
            smp.cpu_local(crate::current_cpu_id())
                .and_then(|l| l.proc_ptr)
        };
        if let Some(nr) = cur_nr {
            let table = crate::proc_table_with(&section);
            let priv_table = crate::priv_table_with(&section);
            // Diagnostic probe: PM stays silent for 270s after resuming,
            // with no sign of any pick/tick. After tick > 5000 this probe
            // takes a saved-context stack trace (cap 2) on ticks where
            // current == PM(0): output ⇒ the tick is alive and PM is
            // spinning in user mode (the trace shows the fmt call chain);
            // no output ⇒ the timer interrupt is dead (a separate bug).
            #[cfg(not(feature = "mock"))]
            {
                use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering as AtomicOrd};
                static PMST_TICK: AtomicU64 = AtomicU64::new(0);
                static PMST_N: AtomicUsize = AtomicUsize::new(0);
                static PMST_K: AtomicUsize = AtomicUsize::new(0);
                let tick = PMST_TICK.fetch_add(1, AtomicOrd::Relaxed);
                // Every 1000 ticks, print a marker (count + current ep) — to
                // pin down the exact event of timer death (align logs from
                // before and after the death point).
                if tick % 1000 == 0
                    && PMST_K.fetch_add(1, AtomicOrd::Relaxed) < 20
                {
                    use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
                }
                if tick > 5000
                    && nr.0 == 0
                    && PMST_N.fetch_add(1, AtomicOrd::Relaxed) < 2
                {
                    use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
                    if let Some(p) = table.get(nr) {
                        crate::stacktrace::proc_stacktrace(p);
                    }
                }
            }
            if table.get(nr).is_some_and(|p| p.is_runnable()) {
                table.check_quantum(nr, priv_table, &section);
            }
        }
        if tick_acquired {
            crate::smp::bkl_unlock();
        }
        return;
    }

    if let Some(irq) = minix_arch::x86_64::trap_stub::irq_of_vector(vector) {
        // C parity: like the clock stub, every IRQ stub in mpx.S saves the
        // full interrupted user context at entry (SAVE_PROCESS_CTX,
        // sconst.h:75-89) before irq_int's policy code — the PIT clock line
        // lands here, so the save must happen before anything that can lead
        // to a quantum/preemption decision.
        unsafe { save_irq_frame_to_context(frame) };
        // The PIT (IRQ0 → 0x50) lands on this arm, not on the clock arm —
        // this arm used to advance only the software clock
        // (clock_irq_handler), **lacking the local_tick + check_quantum
        // scheduling half**: the quantum never expired and any user-mode
        // spin could monopolize the CPU forever (observed: PM's fmt spin
        // silent for 270s, the tick arm firing <1000 times/150s, no pick
        // and no preemption). It now carries the same tick semantics as the
        // clock arm: bill + preempt on quantum expiry (C proc.c:418-424).
        // save_irq_frame_to_context has already saved the interrupted
        // context at this arm's entry, so preemption recovery is safe.
        if irq == minix_plat::TIMER_IRQ.get() {
            // Generic-arm tick counter (the clock arm's `tick k=` only covers
            // the dedicated stub path; this arm's ticks were previously
            // invisible).
            #[cfg(not(feature = "mock"))]
            {
                use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering as AtomicOrd};
                static GTICK: AtomicU64 = AtomicU64::new(0);
                static GTICK_K: AtomicUsize = AtomicUsize::new(0);
                let g = GTICK.fetch_add(1, AtomicOrd::Relaxed);
                if g % 1000 == 0
                    && GTICK_K.fetch_add(1, AtomicOrd::Relaxed) < 20
                {
                    use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
                }
            }
            crate::clock::local_tick(crate::current_cpu_id());
            // Same lock discipline as the LAPIC 0xF1 arm — the
            // quantum check below mutates the shared proc table, so acquire
            // the BKL (or inherit it only if this CPU already owns it).
            let tick_acquired = crate::smp::bkl_lock_or_inherit();
            let tick_section = unsafe { crate::smp::BklSection::assume_held() };
            // Same context_stop_idle as the LAPIC 0xF1 arm — the PIT-origin
            // tick also wakes a halted CPU and restarts its timer.
            crate::context_stop_idle_current();
            let tick_cur = {
                let smp = crate::smp_state_with(&tick_section);
                smp.cpu_local(crate::current_cpu_id())
                    .and_then(|l| l.proc_ptr)
            };
            if let Some(nr) = tick_cur {
                let table = crate::proc_table_with(&tick_section);
                let priv_table = crate::priv_table_with(&tick_section);
                // Sample the only call chain in which PM can spin (cap 2
                // after g>5000).
                #[cfg(not(feature = "mock"))]
                {
                    use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering as AtomicOrd};
                    static GST_TICK: AtomicU64 = AtomicU64::new(0);
                    static GST_N: AtomicUsize = AtomicUsize::new(0);
                    let g2 = GST_TICK.fetch_add(1, AtomicOrd::Relaxed);
                    if g2 > 5000
                        && nr.0 == 0
                        && GST_N.fetch_add(1, AtomicOrd::Relaxed) < 2
                    {
                        use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
                        if let Some(p) = table.get(nr) {
                            crate::stacktrace::proc_stacktrace(p);
                        }
                    }
                }
                if table.get(nr).is_some_and(|p| p.is_runnable()) {
                    table.check_quantum(nr, priv_table, &tick_section);
                }
            }
            if tick_acquired {
                crate::smp::bkl_unlock();
            }
            // LAPIC EOI: an edge-delivered tick must be acknowledged once
            // handled, or the LAPIC refuses further ticks (C: lapic_eoi,
            // apic.c).
            minix_plat::lapic_eoi();
        }
        // Profile-clock PC handoff (C reads p->p_reg.pc, which the asm
        // entry saved into the process context — the save above mirrors
        // that; the rip still travels to `profile_clock_hook` through the
        // one-shot slot so the hook needs no table access). The
        // SPROFILING guard keeps a line fire outside an active profiling
        // run from leaving a stale value in the slot.
        if irq == minix_plat::PROFILE_CLOCK_IRQ.get()
            && crate::misc::SPROFILING.load(core::sync::atomic::Ordering::Acquire)
        {
            crate::misc::stash_profile_pc(frame.rip);
        }
        // Hardware half-loop: ack → mask → hook chain → unmask → eoi,
        // under the interrupted context's BKL (C: IRQ handlers run with the
        // BKL owned by the interrupted context; no lock is taken here).
        // Spurious lines are logged-and-dropped (C irq_handle's spurious
        // path); an invalid IRQ line is a trap-entry wiring bug.
        match crate::irq_manager::dispatch_hardware_irq(minix_plat::IrqVector::new(irq)) {
            Ok(()) => {}
            Err(crate::irq_manager::IrqError::Spurious(line)) => {
                use crate::kmess::{EarlyConsole, KernelConsole as Console};
                Console::write_str("spurious irq ");
                Console::write_hex(line.get() as u64);
                Console::write_str("\n");
            }
            Err(e) => panic!("trap_dispatch: IRQ dispatch error {e:?} (vector {vector:#04x})"),
        }
        // 8259 EOI (C i8259.c pic_eoi counterpart) — every serviced
        // IRQ must be acknowledged, or the PIC stops delivering further
        // interrupts.
        minix_plat::pic_eoi(irq);
        return;
    }

    // Exception / user soft-int gate → arch-generic dispatcher.
    //
    // BKL ownership at the exception entry, per origin:
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
    // — exception.c:186). User-origin outcomes act on it; the kernel-origin
    // path needs it too for the nested-debug legitimacy check (C reads
    // saved_proc->p_reg.psw and p_kern_trap_style unconditionally —
    // exception.c:232-234). The read follows the same dispatch-anchor
    // convention as the SYSCALL body (proc_ptr is read as the anchor
    // itself). User origin + None = no scheduler step ever ran on this CPU,
    // so no CPL3 code could have faulted — wiring bug, same alarm as the
    // SYSCALL arm. Kernel origin tolerates None (early-boot faults have no
    // saved process, C: "no saved_proc yet" — exception.c:173).
    let smp = unsafe { crate::smp_state_boot_unchecked() };
    let cpu = crate::current_cpu_id();
    let proc_ptr = smp.cpu_local(cpu).and_then(|l| l.proc_ptr);
    let cur_nr = if is_user {
        Some(proc_ptr.unwrap_or_else(|| {
            panic!(
                "user exception before scheduler bring-up (proc_ptr = \
                 None on cpu {cpu:?}) — wiring bug"
            )
        }))
    } else {
        proc_ptr
    };
    // is_vm feeds the page-fault classification (C: `pr->p_endpoint ==
    // VM_PROC_NR` — exception.c:101; VM's own faults cannot be forwarded
    // to VM itself). Kernel-origin faults never reach that check (the
    // nested path panics first), matching C's is_nested ordering.
    let is_vm = cur_nr == Some(crate::proc::proc_nr::VM_PROC_NR);
    // Nested-debug legitimacy inputs, read from the saved process state the
    // way C does (exception.c:232-234): the trace bit comes from the saved
    // PSW (`p_reg.psw & TRACEBIT`, archconst.h:120), the entry style from
    // the per-process record (`p_seg.p_kern_trap_style`). A kernel-origin
    // fault with no saved process is not a traced-process debug trap.
    let (is_traced, kern_trap_style) = cur_nr
        .and_then(|nr| {
            crate::proc_table_with(&section).get(nr).map(|p| {
                (
                    (minix_arch::x86_64::trap_stub::saved_psw(&p.cpu_context) & TRACEBIT) != 0,
                    p.trap_style,
                )
            })
        })
        .unwrap_or((false, TrapStyle::NoEntry));
    // The kernel copy paths are validate-first (user-buffer range checks in
    // ipc.rs, Direct Map window checks in vm.rs), so no FaultContext slot is
    // maintained: C's `catch_pagefaults` + context-tracking replacement
    // (doc 14 §3.4) has no producer in this kernel — a fault inside a
    // recoverable copy cannot occur by construction. `FaultContext::Normal`
    // is therefore the truthful context, not a placeholder.
    let outcome = ExceptionDispatcher::<X86_64ExceptionFrame>::handle(
        &mut exc,
        /* is_nested = */ !is_user,
        is_vm,
        FaultContext::Normal,
        is_traced,
        kern_trap_style,
    );

    match outcome {
        // Spurious NMI — C prints and returns (exception.c:191-194); the
        // stub's iretq resumes the interrupted context. No state mutated.
        ExceptionOutcome::SpuriousNmi => {
            use crate::kmess::{EarlyConsole, KernelConsole as Console};
            Console::write_str("got spurious NMI\n");
        }
        // Traced process entered the kernel (int-gate legs keep TF set) and
        // single-stepped the first kernel instruction before the entry
        // recorded a style. C clears the flag in the frame and resumes
        // (exception.c:243-245); identical here — the stub's iretq delivers
        // the corrected RFLAGS. (The `syscall` entry masks TF in hardware,
        // so on this port only the int-33 gate leg can produce the case.)
        ExceptionOutcome::ClearTrapFlag => {
            frame.rflags &= !TRACEBIT;
        }
        // C recovers copy faults by redirecting RIP into recovery labels
        // (exception.c:206-230, klib.S) — a mechanism this kernel replaces
        // with validate-first copy paths (ipc.rs `user_copy_range_mapped`,
        // vm.rs `physical_range_in_dm_window`): bad caller buffers produce
        // typed Err values and never fault. Reaching these outcomes means a
        // fault escaped validation — an invariant break (kernel bug), not
        // recoverable state. C's own fallback for unrecognized kernel
        // faults is inkernel_disaster (exception.c:280-282).
        ExceptionOutcome::RedirectToRecovery(rp) => panic!(
            "trap_dispatch: RedirectToRecovery({rp:?}) — kernel copy paths \
             are validate-first; a fault here escaped validation (kernel bug)"
        ),
        ExceptionOutcome::PhysCopyFault { fault_addr } => panic!(
            "trap_dispatch: PhysCopyFault at {:#x} — Direct Map window \
             validation escaped a fault (kernel bug)",
            fault_addr.0
        ),
        ExceptionOutcome::KernelPanic(v) => {
            // Console diagnostics before dying: the panic handler cannot
            // render the formatted message (no fmt on early console).
            use crate::kmess::{EarlyConsole, KernelConsole as Console};
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
            #[cfg(not(feature = "mock"))]
            panic!(
                "kernel exception vector {} at rip {:#x} errcode {:#x} [dispatch_body @ 0x{:x}]",
                v.get(),
                frame.rip,
                frame.errcode,
                x86_trap_dispatch_body as *const () as usize
            );
        }
        ExceptionOutcome::VmPageFault => {
            // VM's own user page fault — forwarding to VM would deadlock
            // on itself. C: pagefault() prints the frame summary and
            // panics ("pagefault in VM") — exception.c:101-118.
            use crate::kmess::{EarlyConsole, KernelConsole as Console};
            Console::write_str("pagefault for VM on cpu ");
            Console::write_hex(crate::current_cpu_id().raw() as u64);
            Console::write_str(" rip ");
            Console::write_hex(frame.rip);
            // CR2 = the real faulting address. rip only shows how far
            // execution got; deciding "which VA is unmapped" requires cr2.
            let cr2: u64;
            unsafe { core::arch::asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack)); }
            Console::write_str(" cr2 ");
            Console::write_hex(cr2);
            Console::write_str(" err ");
            Console::write_hex(frame.errcode);
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
                // C parity: the exception entry records the return style
                // just like the trap entry (mpx.S save half sets
                // p_kern_trap_style=KTS_FULLCONTEXT; restore consumes it).
                // Without it, a process re-enqueued after forwarding hits
                // "no entry trap style known" on its next dispatch (observed
                // on hardware once RS resumed from fault filling).
                proc.trap_style = minix_arch::TrapStyle::FullContext;
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
            // Console diagnostics for the signal-classified exception —
            // same rationale as the KernelPanic arm's frame dump: which
            // vector became which signal is not otherwise observable on
            // real machine (the panic handler cannot render it).
            {
                use crate::kmess::{EarlyConsole, KernelConsole as Console};
                Console::write_str("user exception: vector ");
                Console::write_hex(vector as u64);
                Console::write_str(" err ");
                Console::write_hex(frame.errcode);
                Console::write_str(" rip ");
                Console::write_hex(frame.rip);
                Console::write_str(" rsp ");
                Console::write_hex(frame.rsp);
                Console::write_str(" cs ");
                Console::write_hex(frame.cs);
                Console::write_str(" rflags ");
                Console::write_hex(frame.rflags);
                Console::write_str("\n");
            }
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
                // Same as the ForwardToVm arm: the exception entry records
                // the return style (C mpx.S parity); the next dispatch after
                // the signal park needs it for the restore.
                proc.trap_style = minix_arch::TrapStyle::FullContext;
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
        // User-mode #NM (vector 7): the process touched FP while CR0.TS
        // was set. C: `copr_not_available_handler()` — proc.c:1922-1957.
        // Lazy-FPU restore acting half: hand the FPU to the trapping
        // process and resume it so the faulting FP instruction re-executes.
        // We do NOT schedule (C: "do not schedule!") — the trap stub's
        // iretq resumes this same process directly from `frame`, and #NM is
        // a fault so RIP still points at the FP instruction (no advance).
        // Unlike the riscv64 sibling, x86's enable() (CLTS) is the CARRIER,
        // not inert: CR0 is a live control register that iretq does NOT
        // restore from the frame, so clearing TS here is what lets the
        // re-executed FP instruction and the FXSAVE/FXRSTOR below go
        // through. C runs `disable_fpu_exception()` (= CLTS — exception.c:
        // 382-385) *before* any FP access, so the CLTS must precede the
        // save/restore or those very instructions re-trap.
        ExceptionOutcome::FpuTrap => {
            let cur_fpu = cur_nr.expect("FpuTrap implies a user-origin #NM");
            let table = crate::proc_table_with(&section);
            use minix_arch::{CurrentFpuArch, FpuArch};
            let fpu = CurrentFpuArch::default();
            // C: disable_fpu_exception() — CLTS first, so FP is usable.
            fpu.enable();
            // C proc.c:1936-1942 — save the outgoing owner's FP state, but
            // skip owners that never touched FP (their fpu_state is the
            // zeroed default and saving would fabricate a live state).
            let old_owner = smp.cpu_local(cpu).and_then(|l| l.fpu_owner);
            // C proc.c:1938 — assert(*local_fpu_owner != p).
            debug_assert!(
                old_owner != Some(cur_fpu),
                "x86_64 FPU trap: current process is already the FPU owner"
            );
            if let Some(owner_nr) = old_owner.filter(|&o| o != cur_fpu)
                && table.get(owner_nr).is_some_and(|p| {
                    p.p_misc_flags
                        .is_set(crate::proc::MiscFlagsBits::EXT_REG_INITIALIZED)
                })
                && let Some(p) = table.get_mut(owner_nr)
            {
                // C proc.c:1936-1942 — save the outgoing owner's FP state,
                // skipping owners that never touched FP (their fpu_state is
                // the zeroed default and saving would fabricate a live state).
                fpu.save(&mut p.fpu_state);
            }
            // C proc.c:1946 — restore the trapping process's state (the
            // zeroed default is a valid FP state for a first-touch process).
            if let Some(p) = table.get_mut(cur_fpu) {
                fpu.restore(&p.fpu_state);
                p.p_misc_flags
                    .set(crate::proc::MiscFlagsBits::EXT_REG_INITIALIZED);
            }
            // C proc.c:1955 — ownership passes to the trapping process, so
            // the next switch-out saves THIS process's state and re-arms TS.
            if let Some(local) = smp.cpu_local_mut(cpu) {
                local.fpu_owner = Some(cur_fpu);
            }
            // This arm returns **straight to the user** through the stub's
            // iretq — it does not go through switch_to_user's
            // "hand over / release while holding the lock" convention.
            // BklSection is a witness that does not release on drop (see the
            // must_use note in smp.rs); if the BKL acquired by the
            // user-origin entry's bkl_lock_section is not explicitly
            // released here, it stays held across iretq (BKL=me=0): the next
            // int-33 park/resched branch's fresh `bkl_lock()` spins forever
            // (observed: the sysboot face froze; gdb caught [BKL]=0=BSP
            // self-held, IF=0 spinning with no clock rescue). BKL free
            // during user mode is the documented semantics
            // (finish_and_restore releases before the context restore —
            // arch_clock.c:226-233 parity, the same net effect as the
            // Signal arm releasing via switch_to_user).
            crate::smp::bkl_unlock();
            // Fall through: the stub's iretq re-executes the FP instruction
            // against the now-enabled FPU.
        }
    }
}

#[cfg(target_arch = "x86_64")]
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
    // The enqueue half of C: RTS_UNSET(dst, RTS_RECEIVING) — the engine only
    // clears the RTS flag (a primitive setter, see the ipc.rs delivery arm);
    // the caller must supply the enqueue (the same shape as
    // syscall.rs dispatch_ipc_entry's take_wake_target → enqueue_if_woken).
    // Missing this half means VM is unlocked but never put on a ready queue,
    // so the scheduler has nobody to pick and the system idles to a halt
    // (observed on hardware: everything went silent after fwd Delivered).
    // The engine records every wake slot and they are all enqueued (multiple
    // wakeups from one syscall do not overwrite each other).
    // Collect first (the engine borrows the procs slice), enqueue after (to
    // avoid a second mutable borrow).
    let mut woken = [None; 4];
    for slot in woken.iter_mut() {
        *slot = engine.take_wake_target();
    }
    for woken_nr in woken.into_iter().flatten() {
        proc_table.enqueue_if_woken(woken_nr);
    }
    // Anchor: this function's runtime address (one-shot, for offline #GP
    // rip resolution).
    #[cfg(not(feature = "mock"))]
    {
        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
        static ANCHOR: AtomicUsize = AtomicUsize::new(0);
        if ANCHOR.fetch_add(1, AtomicOrd::Relaxed) == 0 {
        }
    }
    // Birth-server (p_nr 1,3,4,5,6,7,9,10,11) page-fault forwarding outcomes
    // fall into three classes — D = Path A direct delivery, B = Path B into
    // VM's caller_q, E = error. In the observed tail state all 9 processes
    // sit on pure PAGEFAULT(0x400)+to=VM, so this probe decides "the message
    // was never delivered" vs "delivered and then dropped".
    #[cfg(not(feature = "mock"))]
    {
        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
        static PFW_N: AtomicUsize = AtomicUsize::new(0);
        if matches!(cur_nr.0, 1 | 3 | 4 | 5 | 6 | 7 | 9 | 10 | 11)
            && PFW_N.fetch_add(1, AtomicOrd::Relaxed) < 40
        {
            use crate::kmess::{EarlyConsole as _, KernelConsole as C0};
            match &outcome {
                crate::ipc::IpcOutcome::Delivered => C0::write_str("D"),
                crate::ipc::IpcOutcome::Blocked => C0::write_str("B"),
                crate::ipc::IpcOutcome::Error(_) => C0::write_str("E"),
            }
            C0::write_str("\n");
        }
    }
    match outcome {
        crate::ipc::IpcOutcome::Error(e) => Err(e),
        delivered_or_blocked => Ok(delivered_or_blocked),
    }
}

#[cfg(target_arch = "x86_64")]
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

#[cfg(target_arch = "x86_64")]
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
    // The int-33 IPC trap arrives from USER mode, where this CPU
    // does NOT own the BKL. The entry below mutates the shared PROC_TABLE
    // (saves the register file into `caller.cpu_context`, sets `trap_style`,
    // `p_defer.r2/r3`, `p_delivermsg_vir`) before `dispatch_ipc_entry`
    // acquires the lock. Under early SMP that left two CPUs writing the same
    // slot at once — the aliasing that corrupted a per-proc table pointer and
    // crashed `IpcEngine::send`. Acquire here (inherit only if a nested
    // caller already owns it), and hand the lock to `dispatch_ipc_entry` by
    // releasing just before it (which acquires fresh — the BKL is
    // non-reentrant, so we must not hold it across that call).
    let ipc_bkl = crate::smp::bkl_lock_or_inherit();
    // Review guard: int-33 reaches this body only from the DPL=3
    // IPC gate, so this CPU never owns the BKL on entry and `ipc_bkl` must be
    // true (we always freshly acquire). If a future bug let int-33 fire from
    // kernel context, `ipc_bkl` would be false and `dispatch_ipc_entry`'s
    // fresh `bkl_lock()` below would self-deadlock on the non-reentrant
    // spinlock — assert the invariant loudly in debug instead.
    debug_assert!(
        ipc_bkl,
        "int-33 IPC reached from kernel context (BKL inherited) — \
         dispatch_ipc_entry's fresh acquire would self-deadlock"
    );
    let table = unsafe { crate::proc_table_boot_unchecked() };

    // Decision 3: persist the user register file before any dispatch side
    // effect — delivery paths OR IPC status into this saved context.
    {
        let caller = table
            .get_mut(cur_nr)
            .unwrap_or_else(|| panic!("int-33 IPC from invalid proc nr {cur_nr:?}"));
        minix_arch::save_frame_to_context(frame, &mut caller.cpu_context);
        // Record the ENTRY style (C: every mpx.S soft-int entry records
        // `p_kern_trap_style`; arch_system.c:585 consumes it at
        // restore_user_context). A door-parked caller (blocked IPC)
        // resumes through finish_and_restore's entry-style gate — without
        // a fresh record the wake dispatch finds NoEntry and panics
        // ("no entry trap style known", arch_system.c:597-598; observed
        // on real machine waking a parked receiver).
        // The int-33 gate pushes a full frame, so the style is
        // FullContext — same record the carrier seeds for the first run.
        caller.trap_style = TrapStyle::FullContext;
    }

    // (caller-by-nr): no laundering — the caller travels as its nr and
    // every access re-borrows the slot from `table` at the point of use.

    // Pre-decode: unknown call numbers exit before the BKL is taken —
    // dispatch_ipc_entry's own decode-fail return path also skips the BKL
    // (EBADCALL without acquiring), so kernel_call_finish must not run.
    // C: proc.c:602-606 — do_ipc's default branch, same effect.
    let call_nr = frame.rcx as i32;
    // int-33 IPC entry counter (capped at 16).
    #[cfg(not(feature = "mock"))]
    {
        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
        static IPC_ENTRY: AtomicUsize = AtomicUsize::new(0);
        let n = IPC_ENTRY.fetch_add(1, AtomicOrd::Relaxed);
        if n < 16 {
            use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
        }
    }
    if crate::ipc::IpcCall::from_raw(call_nr).is_none() {
        // Release the entry acquisition before this early exit
        // (no dispatch_ipc_entry runs, so nothing else will drop the lock).
        if ipc_bkl {
            crate::smp::bkl_unlock();
        }
        frame.rax = crate::errno::EBADCALL as i64 as u64;
        return;
    }

    // Register extraction (design decision 2).
    let call_nr = frame.rcx as i32;
    let r1 = frame.rax; // src/dst endpoint, or SENDA count
    let r2 = frame.rbx; // message pointer, or SENDA table pointer
    let is_senda = call_nr == (crate::ipc::IpcCall::SendA as i32);
    // Body-less calls: the stubs trap with rbx = 0 (no message buffer) —
    // C `ipc_minix_kerninfo.S:8-10` zeroes eax/ebx before `int`, and the
    // notify stub passes 0 the same way ("notify without a message body").
    // KernInfo's result returns through the secondary R10 channel
    // (the int-33 return arm syncs RAX + R10, and R10
    // doubles as the status lane — the two roles are per-call exclusive,
    // the same sharing as C's i386 %ebx); the
    // service arm documents "No message buffer is read or written"
    // (syscall.rs). Copying 64 bytes from VA 0 was the SIGSEGV this door
    // delivered to every birth-time kerninfo query on real machine
    // (the first real-machine birth through this
    // gate).
    let is_bodyless =
        is_senda
            || call_nr == (crate::ipc::IpcCall::KernInfo as i32)
            || call_nr == (crate::ipc::IpcCall::Notify as i32);
    {
        let caller = table
            .get_mut(cur_nr)
            .expect("int-33 IPC: caller slot must exist");
        caller.p_defer.r2 = r1 as usize;
        caller.p_defer.r3 = if is_senda { r2 as usize } else { 0 };
        // C do_sync_ipc's `(message *) r3` — the caller's message buffer
        // reaches the engine through `p_delivermsg_vir` (C mini_receive
        // stores m_buff_usr there, proc.c:983; the syscall-leg kernel_call
        // stores m_user, system.c:141). The door previously dropped rbx, so
        // a parked receiver's wake delivery copied the message to a stale
        // syscall buffer and the receive buffer kept its pre-call content
        // (rx read Message::default poison, then its
        // sendnb(m_source) failed with EDEADSRCDST on the garbage source).
        // SENDA carries the table pointer in rbx instead (p_defer.r3 above).
        // Print the site live: the overwrite window is pinned to the single
        // int33 kernel-proxy execution segment, and the candidate writer is
        // kernel_call_finish's DM direct store. To tell fresh from stale,
        // print this entry's r2, the call number, whether it is SENDA (SENDA
        // stores no delivermsg, so the fx target is necessarily stale) and
        // the old p_delivermsg_vir (RS only, reconciled offline against the
        // fx record). Moved out of the `!is_senda` block: zero output there
        // was itself evidence (that window's int33 was probably SENDA), and
        // the senda= field decides it directly.
        #[cfg(not(feature = "mock"))]
        if caller.p_endpoint.0 == 2 {
            use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
            static XIN: AtomicUsize = AtomicUsize::new(0);
            // Cap at 512 and add a marker when the cap is hit (the serial
            // port is synchronous PIO; the worst-case volume must not blow
            // the rerun timeout).
            let xn = XIN.fetch_add(1, AtomicOrd::Relaxed);
            if xn == 512 {
                use crate::kmess::{EarlyConsole as _, KernelConsole as C0};
            }
            if xn < 512 {
                use crate::kmess::{EarlyConsole as _, KernelConsole as C0};
            }
        }
        #[cfg(not(feature = "mock"))]
        let pdmv_before = caller.p_delivermsg_vir;
        #[cfg(feature = "mock")]
        let _pdmv_before = caller.p_delivermsg_vir;
        if !is_senda {
            caller.p_delivermsg_vir = VirBytes(r2);
        }
        // INIT (ep 0xb) int33 trap leg in full: a retrospective showed INIT
        // parked in RECEIVING+gf=0 but without rcvblk ep=0xb (the global
        // probe_mark cap of 8 was exhausted early and is untrustworthy).
        // This probe keeps its own counter and prints the call number / RBX /
        // pre-entry pdmv / getfrom / rts at trap time, deciding directly
        // "did INIT ever enter int33, and what was the message pointer when
        // it parked".
        #[cfg(not(feature = "mock"))]
        if caller.p_endpoint.0 == 0xb {
            use crate::kmess::{EarlyConsole as _, KernelConsole as C0};
            use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
            static INIT33: AtomicUsize = AtomicUsize::new(0);
            if INIT33.fetch_add(1, AtomicOrd::Relaxed) < 48 {
            }
        }
    }

    // Copy the user message (kernel-side copy, TOCTOU defense — the same
    // shape as kernel_call's own copy). SENDA carries no message buffer:
    // mini_senda reads entries from the user table directly (proc.c:683).
    let mut msg = minix_types::Message::default();
    if !is_bodyless {
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
                // Release the entry acquisition before this
                // SIGSEGV/EFAULT early exit.
                if ipc_bkl {
                    crate::smp::bkl_unlock();
                }
                frame.rax = crate::errno::EFAULT as i64 as u64;
                return;
            }
        }
    }
    // Compare the RBX captured at int-33 entry against the rbx delivered
    // pre-restore: non-zero at entry but 0 at delivery ⇒ the kernel
    // overwrote it while the process was parked; 0 already at entry ⇒ the
    // value came from the stub / frame / user side.
    #[cfg(not(feature = "mock"))]
    if cur_nr == crate::proc::ProcNr(2) {
        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
        static I33N: AtomicUsize = AtomicUsize::new(0);
        if I33N.fetch_add(1, AtomicOrd::Relaxed) < 48 {
            use crate::kmess::{EarlyConsole as _, KernelConsole as C0};
        }
    }
    msg.m_type = call_nr;
    msg.m_source = table
        .get(cur_nr)
        .map(|c| c.p_endpoint)
        .expect("int-33 IPC: caller slot must exist");

    // BKL: dispatch_ipc_entry acquires and transfers out (held across
    // kernel_call_finish, which releases it on every non-VmSuspend path).
    // Release the entry's acquisition here so `dispatch_ipc_entry`
    // can take a fresh lock (the BKL is non-reentrant — holding ours across
    // its internal acquire would self-deadlock). Mutual exclusion is never
    // broken: the table is unlocked only for the instant between this release
    // and dispatch_ipc_entry's acquire, and we do not touch it in between.
    // Gate discipline: the trap leg uses `kernel_call_finish_ipc_door` — C's
    // proc.c mini_* never performs the kernel_call leg's eager reply direct
    // write (state travels through h_errno/registers, the real reply goes
    // through MF_DELIVERMSG), so a stale p_delivermsg_vir (SENDA does not
    // refresh it) is structurally unreachable.
    if ipc_bkl {
        crate::smp::bkl_unlock();
    }
    let priv_table = unsafe { crate::priv_table_boot_unchecked() };
    let result = crate::syscall::dispatch_ipc_entry(cur_nr, table, &mut msg, priv_table);
    crate::syscall::kernel_call_finish_ipc_door(cur_nr, table, &msg, result, priv_table);

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

#[cfg(target_arch = "x86_64")]
/// Re-acquire the BKL (released by `kernel_call_finish`) and enter the
/// scheduling loop — the unified exit for blocked IPC (design decision 3).
/// x86-only: the int-33 leg diverges here safely because every later trap
/// reloads RSP from TSS.sp0. The aarch64 leg cannot do this on its single
/// EL1h stack and stops with an explicit panic until the switch-after-pop
/// asm lands.
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

#[cfg(target_arch = "x86_64")]
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
    // (guard transfer) and released by `kernel_call_finish`.
    //
    // With no scheduler step ever run, `proc_ptr` is None and no user code
    // could have executed SYSCALL — panic is the wiring-bug alarm.
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
    // (caller-by-nr): no laundering — the caller travels as its nr.
    let result = crate::syscall::kernel_call(
        cur_nr,
        table,
        m_user,
        unsafe { crate::priv_table_boot_unchecked() },
        unsafe { crate::clock_state_boot_unchecked() },
        &crate::ipc::KernelUserCopy,
    );

    // Reply code → RAX for the sysret leg. VmSuspend: the call has already
    // been parked by vm_suspend (RTS_VMREQUEST + memreq enqueued + VM sent
    // SIGKMEM) — save the full context (RDI = message pointer, stashed
    // separately in saved_m_user), record the return style, and re-enter the
    // scheduler; once VM has serviced it, the KCALL_RESUME stage re-dispatches
    // the call and delivers RAX via set_ipc_return_code (this arm used to
    // panic).
    // NoReply does not occur on this leg (receive-type calls go through
    // int-33).
    // On the SYSCALL leg the wire-level negation is handled uniformly by
    // `reply_wire()`: error codes (Ok) are negated, data codes (Data) pass
    // through as-is; the int33 leg does not negate (rax carries IPC status
    // bit semantics, and user space consumes a positive errno).
    match result.reply_wire() {
        Some(wire) => frame.rax = wire as i64 as u64,
        None => {
            // C: system.c:76 `if (result != EDONTREPLY)` — EDONTREPLY is the
            // contract form that delivers no reply and writes no return
            // register (do_exit/SYS_EXIT takes this path; the caller state
            // machine has already been advanced by cause_sig inside
            // dispatch). The old comment assumed "NoReply does not occur on
            // this leg"; the real-machine C test's _exit catch-all leg
            // disproved it (the aarch64 leg panicked and hung on exit with
            // the same shape).
            if matches!(result, KcallResult::NoReply) {
                return;
            }
            if !matches!(result, KcallResult::VmSuspend) {
                panic!("trap_dispatch: syscall returned {result:?} with no reply code");
            }
            {
                let proc = table
                    .get_mut(cur_nr)
                    .expect("syscall suspend: caller slot must exist");
                minix_arch::save_frame_to_context(frame, &mut proc.cpu_context);
                proc.trap_style = minix_arch::TrapStyle::FullContext;
                if let Some(ctx) = proc.p_vm_suspend.as_mut() {
                    ctx.saved_m_user = Some(m_user.0);
                }
                // Print the suspend state and VMREQUEST at park time
                // (one-shot) — to tell a "suspended but never enqueued /
                // never notified" VM starvation wait from other causes.
                #[cfg(not(feature = "mock"))]
                {
                    use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
                    let (vmreq, state_pending) = table
                        .get(cur_nr)
                        .map(|p| {
                            (
                                p.p_rts_flags
                                    .is_set(crate::proc::RtsFlagsBits::VMREQUEST),
                                p.p_vm_suspend
                                    .as_ref()
                                    .is_some_and(|c| {
                                        matches!(
                                            c.state,
                                            crate::vm::VmSuspendState::Pending
                                        )
                                    }),
                            )
                        })
                        .unwrap_or((false, false));
                }
                // The three halves — chain insertion / dequeue / VM
                // notification — are NOT done here. C's vm_suspend
                // (proc.c:241) runs once per suspension (guaranteed by its
                // assert(!RTS_VMREQUEST)); Rust splits it into
                // suspend_for_vm (set the flag + build the ctx) and
                // vm_enqueue_and_notify_vm (insert + dequeue + notify). On
                // this leg (kernel_call / SYSCALL) the single owner of chain
                // insertion is kernel_call_finish (syscall.rs:3446), and
                // kernel_call (syscall.rs:600) always calls finish for
                // VmSuspend, so this SYSCALL fast path has already been
                // through one insertion. (The other leg — a DeliverMsg
                // suspension in process_misc_flags (proc_table.rs) — performs
                // its own insertion and does not go through finish; the two
                // legs are mutually exclusive.) Inserting again here (as an
                // earlier revision did under the stale assumption that "the
                // fast path skips finish") head-inserts the same node twice
                // → p_next[nr] points at itself → after vm_memreq_get pops
                // the head, head falls back onto the self-linked node and
                // never becomes None → was_empty stays false forever → later
                // requesters' inserts no longer wake VM, and VM waits in
                // RECEIVING = a stable deadlock. Parking dequeue, insertion
                // and notification are all owned by finish; this arm only
                // adds the SYSCALL-leg-specific context save
                // (save_frame_to_context / trap_style / saved_m_user) and
                // does not duplicate the bookkeeping.
            }
            crate::scheduler_loop(crate::current_cpu_id());
        }
    }
}

// ── riscv64 / aarch64 production trap bodies ─────────────
//
// The kernel-side policy half of the production trap legs. The arch asm
// legs (frame save + stack strategy) live in `minix_arch::{riscv64,arm64}
// ::trap_stub`; these bodies receive the frame and decide:
// - **timer arm**: SBI re-arm + per-CPU tick (riscv64) / GIC claim-route
//   (aarch64) → the clock hook chain advances uptime → quantum
//   enforcement (C proc.c:418-424, the x86 0xF1 shape).
// - **syscall arm**: the KERNEL_CALL message leg (a7/x8 == 0, message at
//   a0/x0 per `minix_sys::arch_trap`) → `kernel_call`, reply code in
//   a0/x0. The raw IPC legs (a7/x8 1..16): both aarch64 and riscv64 now
//   dispatch through their `*_ipc_dispatch_body` mirrors of the x86 int-33
//   bridge; the -ENOSYS placeholder
//   and its `ENOSYS_CODE` constant are retired with the riscv wiring.
// - **faults**: U-mode page faults (riscv64 scause 12/13/15) and
//   lower-EL aborts (aarch64 EC 0x20/0x24) forward to VM through
//   the shared `forward_pagefault_to_vm` leg; the signal arms
//   (cause_signal/SIGSEGV/SIGILL) remain riscv-side panic-leg registered
//   gaps — riscv causes 1/4/5/6/7 (misalign/access/illegal…) and the
//   non-riscv PF siblings still land on the diagnostic panic.
//
// Registered by `init_protection` before `TrapEntryArch::load()`; slot
// semantics per arch are documented on `register_trap_dispatchers`.

/// KERNEL_CALL message-leg trap number (minix_sys::arch_trap).
#[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
const KERNEL_CALL_TRAP: u64 = 0;
/// riscv64 scause: Supervisor Timer interrupt (privileged spec §5.2.2,
/// interrupt code 5 under the interrupt bit).
#[cfg(target_arch = "riscv64")]
const RISCV64_CAUSE_SUPERVISOR_TIMER: u64 = 5;
/// riscv64 scause: Supervisor Software interrupt (privileged spec §5.2.1,
/// interrupt code 1 under the interrupt bit) — the IPI lane.
#[cfg(target_arch = "riscv64")]
const RISCV64_CAUSE_SUPERVISOR_SOFTWARE: u64 = 1;
/// riscv64 scause: Supervisor External interrupt (privileged spec §5.2.4,
/// interrupt code 9 under the interrupt bit) — the PLIC peripheral interrupt
/// lane.
#[cfg(target_arch = "riscv64")]
const RISCV64_CAUSE_SUPERVISOR_EXTERNAL: u64 = 9;
/// riscv64 scause: Environment Call from U-mode (exception code 8).
#[cfg(target_arch = "riscv64")]
const RISCV64_CAUSE_ECALL_UMODE: u64 = 8;
/// riscv64 scause codes for the three U-mode page faults (privileged
/// spec Table 3.1/3.3, Sv39): 12 = instruction page fault, 13 = load
/// page fault, 15 = store/AMO page fault. `stval` carries the faulting
/// virtual address (the riscv analogue of the x86 CR2 / arm64 FAR).
#[cfg(any(target_arch = "riscv64", test))]
const RISCV64_CAUSE_INST_PF: u64 = 12;
/// Illegal instruction (scause=2): with `sstatus.FS = Off` (the lazy-FPU
/// gate) every FP instruction raises this — the lazy-restore
/// trigger. C i386: #NM → copr_not_available_handler.
#[cfg(any(target_arch = "riscv64", test))]
const RISCV64_CAUSE_ILLEGAL_INSN: u64 = 2;
/// scause=2 discrimination for the FP lazy gate: only an entry
/// `sstatus.FS` (bits 13:14) == Off (0b00) makes it a gate trap; an illegal
/// instruction with FS != Off is a real SIGILL (privileged CSR / bad
/// encoding) and goes to the diagnostic leg. Host-testable (precedent
/// `riscv64_pf_error_code`).
#[cfg(any(target_arch = "riscv64", test))]
pub(crate) fn riscv64_illegal_insn_is_fpu_trap(entry_sstatus: u64) -> bool {
    entry_sstatus & (0b11 << 13) == 0
}
/// Misaligned fetch (scause=0). C i386: "Alignment check" -> SIGBUS.
#[cfg(any(target_arch = "riscv64", test))]
const RISCV64_CAUSE_MISALIGNED_FETCH: u64 = 0;
/// Breakpoint (scause=3). C i386: "Debug exception" -> SIGTRAP.
#[cfg(any(target_arch = "riscv64", test))]
const RISCV64_CAUSE_BREAKPOINT: u64 = 3;
/// Misaligned load (scause=4) / store (scause=6). C: "Alignment check" -> SIGBUS.
#[cfg(any(target_arch = "riscv64", test))]
const RISCV64_CAUSE_MISALIGNED_LOAD: u64 = 4;
#[cfg(any(target_arch = "riscv64", test))]
const RISCV64_CAUSE_MISALIGNED_STORE: u64 = 6;

/// riscv64 user synchronous exception → signal mapping (pure function,
/// host-testable; precedent `riscv64_pf_error_code`). The C ground truth is
/// exception.c's exception table, user-origin parity: a user origin means
/// `cause_sig(proc_nr(saved_proc), ep->signum)` (exception.c:275), not a
/// kernel panic. None = not deliverable to the user (supervisor-state
/// cause / reserved encoding), which stays on the diagnostic panic lane.
/// The three page-fault causes and the FP gate (scause=2 with FS=Off) are
/// handled by earlier arms and do not enter this mapping.
#[cfg(any(target_arch = "riscv64", test))]
fn riscv64_sync_fault_signal(scause: u64) -> Option<u32> {
    use crate::syscall_signal::{SIGBUS, SIGILL, SIGTRAP};
    match scause {
        // Misaligned fetch/load/store → C "Alignment check" → SIGBUS.
        RISCV64_CAUSE_MISALIGNED_FETCH
        | RISCV64_CAUSE_MISALIGNED_LOAD
        | RISCV64_CAUSE_MISALIGNED_STORE => Some(SIGBUS),
        // Real illegal instruction (gated FP with FS!=Off is taken by an earlier arm) → C "Invalid opcode" → SIGILL.
        RISCV64_CAUSE_ILLEGAL_INSN => Some(SIGILL),
        // Breakpoint → C "Debug exception" → SIGTRAP.
        RISCV64_CAUSE_BREAKPOINT => Some(SIGTRAP),
        _ => None,
    }
}
#[cfg(any(target_arch = "riscv64", test))]
const RISCV64_CAUSE_LOAD_PF: u64 = 13;
#[cfg(any(target_arch = "riscv64", test))]
const RISCV64_CAUSE_STORE_PF: u64 = 15;
/// riscv64 scause interrupt bit (bit 63).
#[cfg(target_arch = "riscv64")]
const RISCV64_SCAUSE_INTERRUPT: u64 = 1 << 63;
/// GIC spurious INTID (nothing claimable — ICC_IAR1_EL1).
#[cfg(target_arch = "aarch64")]
const GIC_SPURIOUS_INTID: u32 = 1023;

#[cfg(target_arch = "riscv64")]
fn riscv64_read_scause() -> u64 {
    let v: u64;
    // SAFETY: side-effect-free CSR read in S-mode; nomem/nostack per the
    // usual CSR-read contract.
    unsafe { core::arch::asm!("csrr {}, scause", out(reg) v, options(nomem, nostack)); }
    v
}

#[cfg(target_arch = "riscv64")]
fn riscv64_read_stval() -> u64 {
    let v: u64;
    // SAFETY: side-effect-free CSR read in S-mode.
    unsafe { core::arch::asm!("csrr {}, stval", out(reg) v, options(nomem, nostack)); }
    v
}

/// Kernel-leg body (S-origin): supervisor timer ticks + kernel-fault
/// diagnostics. The SSI (IPI) and SEI (PLIC external) interrupt arms are
/// registered gaps; the riscv64 IPI lane is owned on this leg.
///
/// # Safety
///
/// `frame` points at the live kernel-leg frame (asm contract); the stub
/// resumes via `sret` when this returns.
#[cfg(target_arch = "riscv64")]
pub unsafe extern "C" fn riscv64_kernel_body(frame: &mut minix_arch::riscv64::trap_stub::Riscv64TrapFrame) {
    // PD22S2H diag: body-entry trace via the SBI ecall console (MMIO-free).
    minix_plat::riscv64::early_console::write_str("  [body-diag] enter\n");
    let scause = riscv64_read_scause();
    {
        let sc = scause & !RISCV64_SCAUSE_INTERRUPT;
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut hexb = [0u8; 16];
        let mut v = sc;
        for i in (0..16).rev() {
            hexb[i] = HEX[(v & 0xf) as usize];
            v >>= 4;
        }
        minix_plat::riscv64::early_console::write_str("  [body-diag] scause=");
        minix_plat::riscv64::early_console::write_str(core::str::from_utf8(&hexb).unwrap_or("?"));
        minix_plat::riscv64::early_console::write_str("\n");
    }
    if scause & RISCV64_SCAUSE_INTERRUPT != 0
        && scause & !RISCV64_SCAUSE_INTERRUPT == RISCV64_CAUSE_SUPERVISOR_TIMER
    {
        riscv64_timer_arm();
        return;
    }
    if scause & RISCV64_SCAUSE_INTERRUPT != 0
        && scause & !RISCV64_SCAUSE_INTERRUPT == RISCV64_CAUSE_SUPERVISOR_SOFTWARE
    {
        riscv64_swi_arm();
        return;
    }
    if scause & RISCV64_SCAUSE_INTERRUPT != 0
        && scause & !RISCV64_SCAUSE_INTERRUPT == RISCV64_CAUSE_SUPERVISOR_EXTERNAL
    {
        riscv64_external_arm();
        return;
    }
    riscv64_diag_panic(frame, scause, "kernel-leg trap");
}

/// User-leg body (U-origin): ecall kernel calls + raw IPC bridge +
/// U-mode page-fault forwarding + user-fault diagnostics. Page faults
/// (scause 12/13/15) park the caller and forward to VM
/// ([`riscv64_pagefault_body`]); the remaining synchronous
/// faults (misalign/illegal/breakpoint/access…) still land on the
/// diagnostics panic — the acting signal arms arrive with the next
/// three-arch wave.
///
/// The `u64` return is the park decision (the riscv mirror of the aarch64
/// park contract): `PARK_NONE` → the epilogue restores and `sret`s;
/// `PARK_RESCHEDULE` → the epilogue unwinds the frame and enters the
/// resched thunk (the caller was parked by the IPC engine).
///
/// # Safety
///
/// `frame` points at the live user-leg frame at the kernel-stack top
/// (asm contract); the interrupted user sp travels in `frame.gpr[2]`.
#[cfg(target_arch = "riscv64")]
pub unsafe extern "C" fn riscv64_user_body(
    frame: &mut minix_arch::riscv64::trap_stub::Riscv64TrapFrame,
) -> u64 {
    use minix_arch::riscv64::trap_stub::PARK_NONE;
    let scause = riscv64_read_scause();
    if scause == RISCV64_CAUSE_ECALL_UMODE {
        // `ecall` does not advance sepc — step past the 4-byte
        // instruction or the sret re-executes it and the trap loops
        // forever (minix-sys arch_trap contract).
        frame.sepc = frame.sepc.wrapping_add(4);
        let leg = frame.gpr[17]; // a7 = call number register
        if leg == KERNEL_CALL_TRAP {
            return riscv64_kernel_call_leg(frame);
        }
        // Raw IPC legs (SEND..SENDA) + MINIX_KERNINFO: the riscv
        // mirror of the x86 int-33 / aarch64 EL1 bridge.
        // An unknown call number is rejected inside the body
        // (EBADCALL); the park decision rides a0 back to asm.
        return riscv64_ipc_dispatch_body(frame);
    }
    if scause & RISCV64_SCAUSE_INTERRUPT != 0 {
        // A U-origin interrupt (delivered while the user leg was
        // installed — stvec is the user leg for all U-mode execution).
        // Only the supervisor timer has an arm here — same scause-code
        // gate as the kernel leg; SSI (IPI) / SEI (PLIC) remain
        // registered gaps and must NOT be mis-counted as a tick
        // (treating every interrupt as a timer
        // would pollute uptime and never EOI a PLIC line).
        if scause & !RISCV64_SCAUSE_INTERRUPT == RISCV64_CAUSE_SUPERVISOR_TIMER {
            riscv64_timer_arm();
            return PARK_NONE;
        }
        if scause & !RISCV64_SCAUSE_INTERRUPT == RISCV64_CAUSE_SUPERVISOR_SOFTWARE {
            riscv64_swi_arm();
            return PARK_NONE;
        }
        if scause & !RISCV64_SCAUSE_INTERRUPT == RISCV64_CAUSE_SUPERVISOR_EXTERNAL {
            riscv64_external_arm();
            return PARK_NONE;
        }
        return riscv64_diag_panic(frame, scause, "user-leg trap");
    }
    // U-mode page faults (the riscv twin of the aarch64 lower-EL abort leg):
    // the three Sv39 fault causes route through the forward-to-VM leg
    // instead of dying on the diagnostic panic. RISC-V separates the
    // access type by cause code — no syndrome register needed.
    if matches!(
        scause,
        RISCV64_CAUSE_INST_PF | RISCV64_CAUSE_LOAD_PF | RISCV64_CAUSE_STORE_PF
    ) {
        return riscv64_pagefault_body(frame, scause);
    }
    // FS=Off lazy-gate FP trap (scause=2) — save the old owner / restore
    // the current one / hand over ownership / re-enable FS in the frame,
    // then re-execute at the same epc (riscv illegal-instruction traps do
    // not advance sepc). scause=2 is the generic illegal instruction
    // (privileged CSR / undefined encoding / bad FP encoding under
    // FS=Initial all report 2) — only an entry FS=Off is the lazy gate (the
    // same FS==OFF gate as the old Linux riscv entry.S); a real illegal
    // instruction with FS!=Off falls back to the diagnostic panic (SIGILL
    // surface).
    if scause == RISCV64_CAUSE_ILLEGAL_INSN
        && riscv64_illegal_insn_is_fpu_trap(frame.sstatus)
    {
        return riscv64_fpu_trap_body(frame);
    }
    // The acting arm for the remaining user synchronous exceptions (C:
    // exception.c:275 `cause_sig(proc_nr(saved_proc), ep->signum)`, user
    // origin parity — this closes the former registered gap "acting
    // SIGSEGV/SIGILL arms arrive with the next three-arch wave"). The shape
    // mirrors the aarch64 Segv arm (this file's aarch64 page-fault leg):
    // acquire the BKL → save the frame into cpu_context + FullContext (the
    // signal manager's SYS_SIGSEND builds the handler frame on top) →
    // cause_signal → unlock → PARK_RESCHEDULE (cause_signal has set
    // RTS_SIGNALED and dequeued the process; returning to the user would
    // livelock on the same trap; the x86 twin lives in scheduler_loop).
    // A None mapping (supervisor-state cause / reserved encoding) stays on
    // the diagnostic panic lane.
    if let Some(sig) = riscv64_sync_fault_signal(scause) {
        use minix_arch::riscv64::trap_stub::PARK_RESCHEDULE;
        let ipc_bkl = crate::smp::bkl_lock_or_inherit();
        let smp = unsafe { crate::smp_state_boot_unchecked() };
        let cpu = crate::current_cpu_id();
        let cur_nr = smp.cpu_local(cpu).and_then(|l| l.proc_ptr).unwrap_or_else(|| {
            panic!(
                "riscv64 user sync fault before scheduler bring-up (proc_ptr = \
                 None on cpu {cpu:?}) — wiring bug"
            )
        });
        let table = unsafe { crate::proc_table_boot_unchecked() };
        {
            let proc = table
                .get_mut(cur_nr)
                .unwrap_or_else(|| panic!("riscv64 sync fault from invalid proc nr {cur_nr:?}"));
            minix_arch::save_frame_to_context(frame, &mut proc.cpu_context);
            proc.trap_style = TrapStyle::FullContext;
        }
        crate::syscall_signal::cause_signal(
            cur_nr,
            sig,
            table,
            unsafe { crate::priv_table_boot_unchecked() },
        );
        if ipc_bkl {
            crate::smp::bkl_unlock();
        }
        return PARK_RESCHEDULE;
    }
    riscv64_diag_panic(frame, scause, "user-leg trap")
}

/// PFEC (page-fault error code) lanes the VM's decode expects — the
/// lane convention this project fixed for the aarch64 leg (present or
/// not, the VM side only consumes the W lane; see `ipc/vm.rs` decode),
/// NOT the x86 CR2 bit0=present semantics. Gated to the riscv leg +
/// host tests; the aarch64/x86 legs keep their established spellings
/// (single migration point; the cross-leg equality is pinned by
/// `riscv_pf_error_code_matches_the_aarch64_lane_table`).
#[cfg(any(target_arch = "riscv64", test))]
const PFEC_PRESENT: u32 = 1;
#[cfg(any(target_arch = "riscv64", test))]
const PFEC_WRITE: u32 = 2;
#[cfg(any(target_arch = "riscv64", test))]
const PFEC_INSTR: u32 = 4;
#[cfg(any(target_arch = "riscv64", test))]
const PFEC_ID: u32 = 16;

/// riscv64 scause → PFEC (the riscv twin of `aarch64_pf_error_code`,
/// host-testable under the `cfg(any(riscv64, test))` precedent of
/// §1.116). RISC-V Sv39 encodes the access type in the cause itself:
/// 12 = fetch, 13 = read, 15 = write. A fault reaches here because a
/// leaf PTE failed the R/W/X/U permission check (PTE-absent and
/// permission-denied share the same cause on RISC-V — the VM's own
/// region ledger is what separates them downstream), so the lane set
/// rides the cause. The write lane only ever comes from cause 15: a
/// fetch is architecturally never a write (same masking as the aarch64
/// leg).
#[cfg(any(target_arch = "riscv64", test))]
pub(crate) fn riscv64_pf_error_code(scause: u64) -> u32 {
    let mut err = PFEC_PRESENT; // user-mode context lane
    match scause {
        RISCV64_CAUSE_INST_PF => {
            err |= PFEC_INSTR;
            err |= PFEC_ID;
        }
        RISCV64_CAUSE_STORE_PF => err |= PFEC_WRITE,
        _ => {} // load page fault: read-side data fault, P only
    }
    err
}

/// The riscv64 page-fault leg: the U-mode instruction/load/store
/// page-fault twin of [`aarch64_pagefault_body`] and the x86 vector-14
/// `ForwardToVm` arm. Saves the user register file (`FullContext`),
/// records RTS_PAGEFAULT + p_fault_addr, and `mini_send`s VM_PAGEFAULT
/// FROM_KERNEL — all through the shared [`forward_pagefault_to_vm`]
/// (single source of truth for the C exception.c:112-129 tail). Ends in
/// the switch-after-pop: the faulting process is parked, so
/// `PARK_RESCHEDULE` unwinds the frame and hands off to the resched
/// thunk (the riscv analogue of diverging into `scheduler_loop`).
///
/// # Safety
///
/// `frame` is the live user-leg frame and the scheduler has a current
/// process (same contract as [`riscv64_ipc_dispatch_body`]).
/// Companion of the krewalk (rolled out after use): the ptroot/pte_pa of
/// the last fill-root.
pub static LAST_FILL_PTROOT: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);
pub static LAST_FILL_LEAF_PA: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

#[cfg(target_arch = "riscv64")]
unsafe fn riscv64_pagefault_body(
    frame: &mut minix_arch::riscv64::trap_stub::Riscv64TrapFrame,
    scause: u64,
) -> u64 {
    use minix_arch::riscv64::trap_stub::PARK_RESCHEDULE;
    let fault_addr = riscv64_read_stval();
    let errcode = riscv64_pf_error_code(scause);

    let smp = unsafe { crate::smp_state_boot_unchecked() };
    let cpu = crate::current_cpu_id();
    let cur_nr = smp
        .cpu_local(cpu)
        .and_then(|l| l.proc_ptr)
        .unwrap_or_else(|| {
            panic!(
                "riscv64 page fault before scheduler bring-up (proc_ptr = \
                 None on cpu {cpu:?}) — wiring bug"
            )
        });

    // VM's own fault cannot be forwarded to VM — C: "pagefault in VM"
    // panic (exception.c:101-118), the same stop the x86/aarch64 legs
    // enforce.
    if cur_nr == crate::proc::proc_nr::VM_PROC_NR {
        use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
        Console::write_str("pagefault for VM sepc ");
        Console::write_hex(frame.sepc);
        Console::write_str(" stval ");
        Console::write_hex(fault_addr);
        Console::write_str("\n");
        // One-shot probe: machine derivation of stval — VM_DM_BASE = 1<<36,
        // pa = stval - base, split into page base and slot offset. A slot
        // offset that is not a multiple of 8 ⇒ not a slot read but a data
        // access (separating the two failure shapes: walk_read slot read vs
        // leaf data access).
        {
            let mut pa = fault_addr.wrapping_sub(1u64 << 36);
            Console::write_str("pfvm: pa=");
            Console::write_hex(pa);
            Console::write_str(" off=");
            Console::write_hex(pa & 0xfff);
            Console::write_str("\n");
            // The true walk root — at fault time satp still belongs to the
            // trapped process (VM); the kernel does not switch roots at the
            // trap entry. satp.PPN << 12 = VM's address-space root table.
            let satp: u64;
            unsafe { core::arch::asm!("csrr {}, satp", out(reg) satp) };
            Console::write_str("pfvm: satp=");
            Console::write_hex(satp);
            Console::write_str(" root=");
            Console::write_hex(satp & 0x00000FFFFFFFFFFF << 12);
            Console::write_str("\n");
            // Walk again at fault time (rolled out after use): re-read the
            // page-table chain slot values (raw l2e/l1e/l0e of all three
            // levels) of the **walk target vaddr** (GPR x12 = the query's
            // vaddr argument) through the kernel direct map (KDM). The
            // criterion: content == 0/normal in the child chain slot at this
            // moment ⇒ the poison was never in RAM (a view mismatch);
            // == the poison value ⇒ RAM dirty-at-fault (there is a memory
            // writer). Note: do not walk fault_addr (the DM-window VA) — the
            // DM window not mapping the pseudo frame IS the cause of the
            // fault, so walking it is tautological (an early version did
            // exactly that and was corrected). Cold path; KDM = direct read
            // at the base-subtracted address.
            {
                const KDM: u64 = 0xFFFF_FFC0_4000_0000;
                let root = (satp & 0x00000FFFFFFFFFFF) << 12;
                let walk_va = frame.gpr[12]; // x12 = a2 = vaddr (3rd arg of the query signature)
                Console::write_str("krewalk: va=");
                Console::write_hex(walk_va);
                Console::write_str("\n");
                // v3: the root switched to the ptroot the fill-root just used
                // (the child root, captured from diagctl) — VM's own root
                // does not contain the child vaddr (that tautology was
                // corrected). Also read directly the current leaf slot
                // pte_pa the fill-root just wrote.
                let ptroot = LAST_FILL_PTROOT.load(
                    core::sync::atomic::Ordering::Relaxed,
                );
                let leaf_pa = LAST_FILL_LEAF_PA.load(
                    core::sync::atomic::Ordering::Relaxed,
                );
                Console::write_str("krewalk: ptroot=");
                Console::write_hex(ptroot);
                Console::write_str(" leaf_pa=");
                Console::write_hex(leaf_pa);
                Console::write_str("\n");
                if leaf_pa >= 0x8000_0000 && leaf_pa < 0xA000_0000 {
                    let cur = unsafe {
                        ((0xFFFF_FFC0_4000_0000 + leaf_pa) as *const u64).read_volatile()
                    };
                    Console::write_str("krewalk: leaf_now=");
                    Console::write_hex(cur);
                    Console::write_str("\n");
                }
                if ptroot >= 0x8000_0000 && ptroot < 0xA000_0000 {
                    // The child chain walks ptroot (in v3 the `let mut pa =
                    // root` rebinding overwrote pa = ptroot — that is the
                    // root cause of the tautological misread; the root is now
                    // chosen at the binding point — a valid ptroot means
                    // walking the child chain).
                    pa = ptroot;
                } else {
                    Console::write_str("krewalk: ptroot out-of-range\n");
                }
                let shifts = [30u64, 21, 12];
                for (lvl, sh) in shifts.iter().enumerate() {
                    let idx = (walk_va >> sh) & 511;
                    let slot_pa = pa + idx * 8;
                    if slot_pa < 0x8000_0000 || slot_pa >= 0xA000_0000 {
                        Console::write_str("krewalk: lvl out-of-range\n");
                        break;
                    }
                    let v = unsafe {
                        ((KDM + slot_pa) as *const u64).read_volatile()
                    };
                    Console::write_str("krewalk: l");
                    Console::write_str(["2", "1", "0"][lvl]);
                    Console::write_str(" slot_pa=");
                    Console::write_hex(slot_pa);
                    Console::write_str(" raw=");
                    Console::write_hex(v);
                    Console::write_str("\n");
                    if (v & 1) == 0 {
                        Console::write_str("krewalk: not-present\n");
                        break;
                    }
                    pa = ((v >> 10) & 0xFFFFFFF_F) << 12;
                }
            }
        }
        // One-shot probe: full GPR frame dump — locating partial register
        // restores.
        for (idx, val) in frame.gpr.iter().enumerate() {
            Console::write_str("x");
            Console::write_hex(idx as u64);
            Console::write_str("=");
            Console::write_hex(*val);
            Console::write_str(" ");
            if idx % 4 == 3 {
                Console::write_str("\n");
            }
        }
        Console::write_str("\n");
        panic!("pagefault in VM");
    }
    // One-shot probe: on a non-VM page fault print the full satp
    // (MODE/ASID/PPN split) — the satp reconciliation for "the PTE is valid
    // yet the CPU still faults": was the CPU really using the fill-root
    // root at that moment? What was the ASID? Capped at 32 to prevent a
    // flood.
    #[cfg(all(target_arch = "riscv64", not(feature = "mock")))]
    {
        use crate::kmess::{EarlyConsole as _, KernelConsole as P5Console};
        use core::sync::atomic::{AtomicUsize, Ordering as P5Ord};
        static P5_N: AtomicUsize = AtomicUsize::new(0);
        // Only observe faults from rc(0xc) (the CAP=32 budget was consumed
        // by early demand-fill, leaving rc's faults unobserved).
        if cur_nr.0 == 0xc && P5_N.fetch_add(1, P5Ord::Relaxed) < 32 {
            let satp: u64;
            // SAFETY: CSR read, no side effects.
            unsafe { core::arch::asm!("csrr {}, satp", out(reg) satp) };
            let mode = satp >> 60;
            let asid = (satp >> 44) & 0xFFFF;
            let ppn = satp & 0xF_FFFF_FFFF;
            // Fault-origin probe: sepc landing point +
            // sstatus.SPP(bit8)/SUM(bit18) + sp at interrupt time
            // (frame.gpr[2]). sstatus comes from the in-frame entry snapshot
            // (trap_stub contract: the value is captured at entry), not a
            // live CSR read. Decision tree: SPP=0 and sepc inside the user
            // code region ⇒ triggered by a user-mode store (this is what was
            // observed; the address-normativity check then settled it: the
            // faulting VA 0x7fff_ffff_f000 lies above the canonical Sv39
            // range); SPP=1 ⇒ the kernel accessing user memory on the user's
            // behalf (an S-mode store to a U page with SUM=0 is an
            // architecturally legal fault and a guest-side defect).
            let sst = frame.sstatus;
            P5Console::write_str(" sepc=");
            P5Console::write_hex(frame.sepc);
            P5Console::write_str(" sst=");
            P5Console::write_hex(sst);
            P5Console::write_str(" sp=");
            P5Console::write_hex(frame.gpr[2]);
            P5Console::write_str("\n");
            // Core criterion (closing on the same fault): walk the three
            // levels from this fault's satp root and read the leaf PTE — the
            // leaf value sequence over 26 refaults decides "what the leaf PTE
            // in RAM actually is during the loop".
            if ppn << 12 >= 0x8000_0000 {
                const KDM: u64 = 0xFFFF_FFC0_4000_0000;
                let kdm = |pa: u64| unsafe {
                    ((KDM + pa) as *const u64).read_volatile()
                };
                let mut pa = ppn << 12;
                let mut ok = true;
                for sh in [30u64, 21, 12] {
                    let idx = (fault_addr >> sh) & 511;
                    let slot = pa + idx * 8;
                    if slot < 0x8000_0000 || slot >= 0xA000_0000 {
                        P5Console::write_str(" p5walk oor slot=");
                        P5Console::write_hex(slot);
                        ok = false;
                        break;
                    }
                    let v = kdm(slot);
                    P5Console::write_str(" l");
                    P5Console::write_hex(if sh == 30 { 2 } else if sh == 21 { 1 } else { 0 });
                    P5Console::write_str("=");
                    P5Console::write_hex(v);
                    if v & 1 == 0 { ok = false; break; }
                    pa = ((v >> 10) & 0xF_FFFF_F) << 12;
                }
                if ok {
                    let leaf = kdm(pa);
                    P5Console::write_str(" leaf=");
                    P5Console::write_hex(leaf);
                }
                P5Console::write_str("\n");
            }
        }
    }
    // User code runs without the BKL (resched-thunk convention: unlock
    // before PARK_RESCHEDULE, the thunk re-acquires).
    let ipc_bkl = crate::smp::bkl_lock_or_inherit();
    let table = unsafe { crate::proc_table_boot_unchecked() };
    {
        let proc = table
            .get_mut(cur_nr)
            .unwrap_or_else(|| panic!("riscv64 pagefault from invalid proc nr {cur_nr:?}"));
        minix_arch::save_frame_to_context(frame, &mut proc.cpu_context);
        proc.trap_style = TrapStyle::FullContext;
    }
    let priv_table = unsafe { crate::priv_table_boot_unchecked() };
    // Two diagnostic probes were rolled out this round — both had already
    // been hollowed out into pure counter increments plus empty `if` bodies
    // (their readers were removed earlier, leaving unread `LETHAL_PF`/`FA0_N`
    // static counters), the same "empty-body remnant" family; removal
    // carries zero behavior risk and is unrelated to the main probe still
    // under investigation. The probes that still genuinely depend on it
    // closing (krewalk, sfence-exec, etc.) stay where they are, untouched.
    if let Err(e) = forward_pagefault_to_vm(table, priv_table, cur_nr, fault_addr, errcode) {
        // C: panic("WARNING: pagefault: mini_send returned %d")
        panic!("pagefault: mini_send returned {e:?}");
    }
    if ipc_bkl {
        crate::smp::bkl_unlock();
    }
    PARK_RESCHEDULE
}

/// The riscv64 timer arm — one S-mode timer tick, three jobs in the x86
/// local-tick + PIT-hook order:
/// 1. `local_tick` (per-CPU observability counter + the SBI re-arm via
///    `ClockArch::local_timer_eoi` — the one-shot stops dead unless the
///    body re-arms, apic.c:578 parity),
/// 2. the clock hook chain under `minix_plat::TIMER_IRQ` (advances
///    uptime, expires alarms; the CPU-local timer bypasses the
///    controller claim — see the `dispatch_claimed_hardware_irq` note below),
/// 3. quantum enforcement for this CPU's running process
///    (C proc.c:418-424).
/// riscv64 SWI arm (scause=1, Supervisor Software interrupt): clear this
/// hart's SSIP, then apply this CPU's scheduling-IPI policy under the BKL.
/// Same shape as the aarch64 SGI arm: `sched_handler_full` applies and
/// clears STOP/SAVE_CTX/VM_INHIBIT, and `preempt_current_on_ipi` marks the
/// interrupted process preempted (no-op for IDLE). `context_stop_idle_current`
/// clears the idle flag and restarts the one-shot first.
///
/// SSI delivery itself is still firmware-blocked (the forensic note in
/// `riscv64::smp`); this is the receive half, exercised when a delegated or
/// SSWI platform delivers, or by the boot self-loop.
#[cfg(target_arch = "riscv64")]
fn riscv64_swi_arm() {
    // SAFETY: CSR write that clears only this hart's SSIP pending bit (an
    // S-mode self-clear the privileged spec allows; this platform's
    // aclint-mswi has no SSWI device, and CLINT msip cannot be relied on
    // from S-mode — see the contract note on the arch side's
    // clear_self_ssip).
    unsafe {
        minix_arch::riscv64::smp::clear_self_ssip();
    }
    let acquired = crate::smp::bkl_lock_or_inherit();
    // C context_stop_idle at interrupt entry: clear the idle flag and
    // restart the one-shot before the scheduler-IPI policy runs.
    crate::context_stop_idle_current();
    {
        let section = unsafe { crate::smp::BklSection::assume_held() };
        let table = crate::proc_table_with(&section);
        let smp = crate::smp_state_with(&section);
        let cpu = crate::current_cpu_id();
        // Apply the pending scheduler flags (STOP/SAVE_CTX/VM_INHIBIT) and
        // preempt the interrupted process — the aarch64 SGI arm's policy.
        smp.sched_handler_full(table, cpu);
        smp.preempt_current_on_ipi(table, cpu);
    }
    if acquired {
        crate::smp::bkl_unlock();
    }
}

/// riscv64 EI arm (scause=9, Supervisor External interrupt): PLIC claim read
/// (the source number is the IRQ number) → dispatch by vector through the
/// full `dispatch_claimed` handshake (mask→chain→unmask→complete). The
/// PLIC window is already in the user roots, so the claim/complete MMIO
/// accesses are reachable in both legs' address spaces. A source without a
/// chain → Spurious counter (the PLIC claim read has already cleared the
/// source pending bit, so there is no storm risk; same Spurious print
/// convention as the timer arm); source enabling is chosen by the first
/// device driver through the IRQ manager's unmask (this arm is the channel,
/// it does not enable sources itself).
#[cfg(target_arch = "riscv64")]
fn riscv64_external_arm() {
    use core::sync::atomic::{AtomicU64, Ordering as AtomicOrd};
    static ACKS: AtomicU64 = AtomicU64::new(0);
    let Some(id) = crate::irq_manager::claim_hardware_irq() else {
        return; // claim=0: spurious interrupt (no source), swallowed silently per the timer convention
    };
    let irq = minix_plat::IrqVector::new(id as u8);
    match crate::irq_manager::dispatch_claimed_hardware_irq(irq, Some(id)) {
        Ok(()) => {
            let n = ACKS.fetch_add(1, AtomicOrd::AcqRel);
            if n < 4 {
                use crate::kmess::{EarlyConsole, KernelConsole as Console};
                Console::write_str("nk4c: sei-ack src=");
                let mut hb = [0u8; 8];
                let mut v = id as u64;
                for byte in hb.iter_mut().rev() {
                    *byte = b"0123456789abcdef"[(v & 0xf) as usize];
                    v >>= 4;
                }
                Console::write_str(core::str::from_utf8(&hb).unwrap_or("?"));
                Console::write_str("\n");
            }
        }
        Err(crate::irq_manager::IrqError::Spurious(irq)) => {
            use crate::kmess::{EarlyConsole, KernelConsole as Console};
            Console::write_str("spurious irq ");
            Console::write_hex(irq.get() as u64);
            Console::write_str("\n");
        }
        Err(e) => panic!("riscv64 external arm: IRQ dispatch error {e:?}"),
    }
}

fn riscv64_timer_arm() {
    // Quiet-point drain instrumentation (a walk and slot re-read outside
    // the write window; see the minix_arch::ptwb module docs and
    // riscv64::paging::ptwb_drain_budgeted).
    #[cfg(target_arch = "riscv64")]
    minix_arch::riscv64::paging::ptwb_drain_budgeted();
    crate::clock::local_tick(crate::clock::current_cpuid());
    // The riscv S-mode timer is a CPU-local interrupt (`sip.STIP`) that never
    // crosses the PLIC — its dispatch identity is the reserved pseudo-vector
    // 0, which the controller's `mask`/`unmask` already special-case as "no
    // controller line" (`os/plat/src/riscv64/interrupt.rs` timer/vector-0).
    // A bare `dispatch_hardware_irq` self-claims first (`IrqManager::dispatch`
    // → `controller.claim()`), and the riscv `claim()` reads the PLIC claim
    // MMIO register `read_volatile(plic_base + 0x200004 + context*0x1000)` —
    // an address the port does not map, so the very first tick that interrupts
    // a process whose page table omits the PLIC window load-faults (scause 13).
    // Confirmed: `sepc` lands in `dispatch_hardware_irq` (the `claim` arm of
    // `dispatch()` is inlined), `stval=0xc201004` = PLIC base `0xc000000` +
    // `PLIC_CLAIM 0x200004` + `context(1)×0x1000` (three-arch comparison:
    // x86's `claim()` returns `None` and aarch64 reads the system register
    // `icc_iar1_el1`; neither needs a page-table mapping, so the same
    // segment does not fault there).
    // Mirror the x86 shape (`X86_64InterruptController::claim` → `None`): route
    // through the already-claimed entry with `claimed = None`, which skips the
    // controller claim and leaves `complete(None)` a no-op (early return) — the
    // SBI re-arm in `local_tick` already retires the timer. The future PLIC
    // external (SEI) lane will use the split `claim → route → dispatch_claimed`
    // shape once wired (this leg currently handles only the supervisor timer).
    match crate::irq_manager::dispatch_claimed_hardware_irq(minix_plat::TIMER_IRQ, None) {
        Ok(()) => {}
        Err(crate::irq_manager::IrqError::Spurious(irq)) => {
            use crate::kmess::{EarlyConsole, KernelConsole as Console};
            Console::write_str("spurious irq ");
            Console::write_hex(irq.get() as u64);
            Console::write_str("\n");
        }
        Err(e) => panic!("riscv64 timer arm: IRQ dispatch error {e:?}"),
    }
    // Quantum check under owner-aware inherit-or-acquire BKL (the SCHED_IPI
    // shape): inherit only if THIS CPU's interrupted kernel frame
    // owns the lock, else spin-acquire it even when a peer holds it.
    let acquired = crate::smp::bkl_lock_or_inherit();
    // C context_stop_idle: a tick landing on a halted hart clears the idle
    // flag and restarts its local timer (the riscv arm previously only ran
    // the quantum check, leaving a woken AP's one-shot masked).
    crate::context_stop_idle_current();
    {
        let section = unsafe { crate::smp::BklSection::assume_held() };
        let smp = crate::smp_state_with(&section);
        let cur_nr = {
            let cpu = crate::current_cpu_id();
            smp.cpu_local(cpu).and_then(|l| l.proc_ptr)
        };
        if let Some(nr) = cur_nr {
            let table = crate::proc_table_with(&section);
            let priv_table = crate::priv_table_with(&section);
            if table.get(nr).is_some_and(|p| p.is_runnable()) {
                table.check_quantum(nr, priv_table, &section);
            }
        }
    }
    if acquired {
        crate::smp::bkl_unlock();
    }
}

/// The KERNEL_CALL leg: message pointer at a0, call number in
/// `m_type` — `kernel_call` parity with the x86 SYSCALL body.
///
/// Returns the park decision (the aarch64 park contract mirror): a reply
/// code rides a0 out and returns `PARK_NONE`; a `VmSuspend` outcome
/// saves the caller's context (the KCALL_RESUME redispatch delivers the
/// reply into it later) and parks through the switch-after-pop path.
/// Other no-reply outcomes are a wiring bug here — panic rather than
/// reply garbage (x86 B-body parity).
///
/// # Safety
///
/// Caller guarantees the registered-dispatcher invariants (scheduler
/// brought up, tables initialized); a blocked-call outcome is a wiring
/// bug here (blocking IPC arrives through the IPC bridge, x86 parity).
#[cfg(target_arch = "riscv64")]
unsafe fn riscv64_kernel_call_leg(
    frame: &mut minix_arch::riscv64::trap_stub::Riscv64TrapFrame,
) -> u64 {
    use minix_arch::riscv64::trap_stub::{PARK_NONE, PARK_RESCHEDULE};
    let smp = unsafe { crate::smp_state_boot_unchecked() };
    let cpu = crate::current_cpu_id();
    let cur_nr = smp
        .cpu_local(cpu)
        .and_then(|l| l.proc_ptr)
        .unwrap_or_else(|| {
            panic!(
                "riscv64 kernel call before scheduler bring-up (proc_ptr = \
                 None on cpu {cpu:?}) — wiring bug"
            )
        });
    // User message pointer ABI: a0 (minix_sys::arch_trap kernel_call_trap).
    let m_user = VirBytes::new(frame.gpr[10]);
    let table = unsafe { crate::proc_table_boot_unchecked() };
    let result = crate::syscall::kernel_call(
        cur_nr,
        table,
        m_user,
        unsafe { crate::priv_table_boot_unchecked() },
        unsafe { crate::clock_state_boot_unchecked() },
        &crate::ipc::KernelUserCopy,
    );
    // Reply → a0 on the wire form (negative errno lanes go out through
    // `reply_wire`, not the raw positive code — the same gap this leg
    // carried before, closed together with the aarch64 mirror).
    match result.reply_wire() {
        Some(wire) => {
            frame.gpr[10] = wire as i64 as u64;
            PARK_NONE
        }
        None => {
            // C: system.c:76 `if (result != EDONTREPLY)` — NoReply delivers
            // no reply and writes no a0 (all three legs synchronized; the old
            // panic assumption was disproved by the real-machine C test's
            // _exit catch-all leg).
            if matches!(result, crate::syscall::KcallResult::NoReply) {
                return PARK_NONE;
            }
            // VmSuspend (aarch64 mirror): the call parked the
            // caller on RTS_VMREQUEST — enqueue/dequeue/notify VM were
            // already done by kernel_call_finish (the single owner of
            // that bookkeeping on this leg, x86 SYSCALL parity). This
            // arm only adds the leg-specific context save so the
            // KCALL_RESUME redispatch can deliver the reply into the
            // saved registers.
            if !matches!(result, crate::syscall::KcallResult::VmSuspend) {
                panic!("riscv64 kernel call returned {result:?} with no reply code");
            }
            {
                let proc = table
                    .get_mut(cur_nr)
                    .expect("riscv64 kernel call suspend: caller slot must exist");
                minix_arch::save_frame_to_context(frame, &mut proc.cpu_context);
                proc.trap_style = TrapStyle::FullContext;
                if let Some(ctx) = proc.p_vm_suspend.as_mut() {
                    ctx.saved_m_user = Some(m_user.0);
                }
            }
            // The caller is non-runnable; hand off through the switch-
            // after-pop (the resched thunk re-acquires the BKL).
            PARK_RESCHEDULE
        }
    }
}

/// The diverging park-and-reschedule thunk (the riscv mirror of the aarch64
/// park contract / x86 `reenter_scheduler`), reached via
/// `riscv64_resched_entry` after the user-leg asm unwound a blocked-
/// receiver frame back to the clean kernel-stack base. Re-acquires the
/// BKL that `kernel_call_finish_ipc_door` released (`scheduler_loop`
/// consumes the held-BKL convention) and enters the scheduling loop.
///
/// # Safety
///
/// Called only from the user leg's park branch (single, well-defined
/// kernel-stack base; BKL not held; current proc already parked and
/// dequeued by the IPC engine).
#[cfg(target_arch = "riscv64")]
pub unsafe extern "C" fn riscv64_resched_thunk() -> ! {
    let guard = crate::smp::bkl_lock();
    core::mem::forget(guard);
    let cpu = crate::current_cpu_id();
    crate::scheduler_loop(cpu)
}

/// The raw-IPC leg (ecall with a7 = 1..16): the riscv mirror of
/// [`aarch64_ipc_dispatch_body`] and the x86 int-33
/// bridge. Register ABI per `minix-sys::arch_trap::ipc_trap` (riscv64):
/// a7 = call number, a0 = src/dst endpoint (SENDA: count), a1 = message
/// pointer (SENDA: table pointer); errno returns in a0 and the IPC
/// status rides a1 (the A1 status lane, `or_ipc_status_reg`). The
/// `ecall` sepc step already happened in [`riscv64_user_body`].
///
/// # Safety
///
/// Same invariants as `riscv64_kernel_call_leg`: `frame` is the live
/// user-leg frame and the current proc is scheduled.
#[cfg(target_arch = "riscv64")]
unsafe fn riscv64_ipc_dispatch_body(
    frame: &mut minix_arch::riscv64::trap_stub::Riscv64TrapFrame,
) -> u64 {
    use minix_arch::riscv64::trap_stub::{PARK_NONE, PARK_RESCHEDULE};
    let smp = unsafe { crate::smp_state_boot_unchecked() };
    let cpu = crate::current_cpu_id();
    let cur_nr = smp
        .cpu_local(cpu)
        .and_then(|l| l.proc_ptr)
        .unwrap_or_else(|| {
            panic!(
                "riscv64 IPC trap before scheduler bring-up (proc_ptr = \
                 None on cpu {cpu:?}) — wiring bug"
            )
        });

    // The ecall arrives from U-mode where this CPU does NOT own the BKL.
    // The entry below mutates the shared PROC_TABLE before
    // dispatch_ipc_entry acquires it fresh — acquire now (the x86 shape),
    // release just before the dispatch hand-off (the BKL is
    // non-reentrant).
    let ipc_bkl = crate::smp::bkl_lock_or_inherit();
    debug_assert!(
        ipc_bkl,
        "riscv64 ecall IPC reached from kernel context (BKL inherited) — \
         dispatch_ipc_entry's fresh acquire would self-deadlock"
    );
    let table = unsafe { crate::proc_table_boot_unchecked() };

    // Persist the user register file before any dispatch side effect —
    // delivery paths OR IPC status into this saved context (the resume
    // asm reads it back before `sret`). Record the ENTRY style so a
    // door-parked caller resumes through the entry-style gate.
    {
        let caller = table
            .get_mut(cur_nr)
            .unwrap_or_else(|| panic!("riscv64 IPC from invalid proc nr {cur_nr:?}"));
        minix_arch::save_frame_to_context(frame, &mut caller.cpu_context);
        caller.trap_style = TrapStyle::FullContext;
    }

    // Pre-decode: unknown call numbers exit before any dispatch (C
    // proc.c:602-606 do_ipc default — EBADCALL without entering the
    // engine).
    let call_nr = frame.gpr[17] as i32; // a7
    if crate::ipc::IpcCall::from_raw(call_nr).is_none() {
        if ipc_bkl {
            crate::smp::bkl_unlock();
        }
        frame.gpr[10] = crate::errno::EBADCALL as i64 as u64;
        return PARK_NONE;
    }

    let r1 = frame.gpr[10]; // a0: src/dst endpoint, or SENDA count
    let r2 = frame.gpr[11]; // a1: message pointer, or SENDA table pointer
    let is_senda = call_nr == (crate::ipc::IpcCall::SendA as i32);
    // Body-less calls (kerninfo / notify / senda) trap with the message
    // register unused — no user buffer is copied (C ipc_minix_kerninfo.S
    // zeroes the operand regs; notify passes no body).
    let is_bodyless = is_senda
        || call_nr == (crate::ipc::IpcCall::KernInfo as i32)
        || call_nr == (crate::ipc::IpcCall::Notify as i32);
    {
        let caller = table
            .get_mut(cur_nr)
            .expect("riscv64 IPC: caller slot must exist");
        caller.p_defer.r2 = r1 as usize;
        caller.p_defer.r3 = if is_senda { r2 as usize } else { 0 };
        if !is_senda {
            caller.p_delivermsg_vir = VirBytes(r2);
        }
    }

    // Copy the user message (kernel-side copy, TOCTOU defense — the same
    // shape as the x86/aarch64 legs). SENDA carries no message buffer.
    let mut msg = minix_types::Message::default();
    if !is_bodyless {
        use crate::ipc::UserCopy as _;
        match crate::ipc::KernelUserCopy.copy_msg_from_user(VirBytes(r2)) {
            Ok(m) => msg = m,
            Err(_) => {
                // IPC-door-EFAULT probe (riscv64; candidate source of VM's
                // own SIGSEGV): the kernel_call leg already has the kc-efault
                // probe (syscall.rs:596, ruled out with zero hits), but the
                // IPC trap door (mini_send/receive through this
                // riscv64_ipc_dispatch_body) had no probe on its
                // copy_msg_from_user failure leg — the conclusion was
                // "delivery copy EFAULT → cause_sig(8,11)" but kc-efault did
                // not cover this leg, and the resume-Fault family was proven
                // clear (zero rvflt/mrr-Fault/sf-*). Print the caller ep +
                // the message's user VA r2 + current_root here to settle once
                // why the VM IPC copy returns EFAULT. Do not roll out before
                // it is closed.
                #[cfg(not(feature = "mock"))]
                {
                    use crate::kmess::{EarlyConsole as _, KernelConsole as C0};
                    use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                    static IPE: AtomicUsize = AtomicUsize::new(0);
                    if IPE.fetch_add(1, AtomicOrd::Relaxed) < 16 {
                        let ep = table.get(cur_nr).map_or(0i32, |p| p.p_endpoint.0);
                        let root = crate::current_root_phys().map(|r| r.0).unwrap_or(0);
                        // Dump the whole a0..a7 argument window: distinguishes
                        // "VM really passed a null pointer" from "the trap
                        // frame GPR slots are misplaced" (a1 should be
                        // gpr[11] = msg ptr).
                        C0::write_str(" a0=0x");
                        C0::write_hex(frame.gpr[10]);
                        C0::write_str(" a2=0x");
                        C0::write_hex(frame.gpr[12]);
                        C0::write_str(" a3=0x");
                        C0::write_hex(frame.gpr[13]);
                        C0::write_str(" a4=0x");
                        C0::write_hex(frame.gpr[14]);
                        C0::write_str(" a5=0x");
                        C0::write_hex(frame.gpr[15]);
                        C0::write_str("\n");
                    }
                }
                // C system.c:152-155 parity: SIGSEGV + EFAULT, without
                // entering IPC dispatch.
                crate::syscall_signal::cause_signal(
                    cur_nr,
                    crate::syscall_signal::SIGSEGV,
                    table,
                    unsafe { crate::priv_table_boot_unchecked() },
                );
                if ipc_bkl {
                    crate::smp::bkl_unlock();
                }
                frame.gpr[10] = crate::errno::EFAULT as i64 as u64;
                return PARK_NONE;
            }
        }
    }
    msg.m_type = call_nr;
    msg.m_source = table
        .get(cur_nr)
        .map(|c| c.p_endpoint)
        .expect("riscv64 IPC: caller slot must exist");

    // Release the entry acquisition so dispatch_ipc_entry takes a fresh
    // lock (non-reentrant BKL); mutual exclusion is unbroken for the
    // instant we hold nothing (we do not touch the table in between).
    if ipc_bkl {
        crate::smp::bkl_unlock();
    }
    let priv_table = unsafe { crate::priv_table_boot_unchecked() };
    let result = crate::syscall::dispatch_ipc_entry(cur_nr, table, &mut msg, priv_table);
    crate::syscall::kernel_call_finish_ipc_door(cur_nr, table, &msg, result, priv_table);

    // Delivered: errno rides a0 out via `sret`; the IPC status bits were
    // ORed into the saved context's A1 by the delivery path — pull that
    // lane back into the frame (sync_status_register_to_frame), then let
    // the epilogue restore normally (PARK_NONE).
    //
    // Blocked (NoReply): the caller was parked by
    // `kernel_call_finish_ipc_door` (dequeued, context already saved
    // above). Request the switch-after-pop: the user leg unwinds this
    // frame back to the clean kernel-stack base and branches to the
    // registered resched thunk, which re-acquires the BKL and runs
    // `scheduler_loop` — picking another runnable process. The parked
    // process resumes later through `finish_and_restore` when a
    // delivery makes it runnable again.
    if let Some(code) = result.reply_code() {
        frame.gpr[10] = code as i64 as u64;
        let ctx = &table
            .get(cur_nr)
            .expect("riscv64 IPC: caller slot must exist")
            .cpu_context;
        minix_arch::sync_status_register_to_frame(ctx, frame);
        PARK_NONE
    } else {
        PARK_RESCHEDULE
    }
}

/// Console diagnostics + panic for unreached causes — the production
/// replacement of the removed print-and-halt diag stub: the panic routes
/// through the kernel diagnostic path instead of halting in the entry
/// leg, so a scheduler can at least be observed around the corpse.
#[cfg(target_arch = "riscv64")]
/// The lazy-FPU restore stage (scause=2: user FP instruction trapped
/// because a context switch left `sstatus.FS = Off` via
/// [`FpuArch::disable`]).
///
/// C: `copr_not_available_handler()` — proc.c:1922-1962 — the i386 #NM
/// handler's acting half: save the outgoing owner's FP state
/// (retain=FALSE), restore the trapping process's state, hand ownership
/// to the trapping process, re-enable FP, and return to re-execute the
/// faulting instruction. RISC-V illegal-instruction traps do not
/// advance sepc, so returning with `PARK_NONE` re-executes it — but the
/// riscv FS bit lives in sstatus, which the trap stub's epilogue restores
/// wholesale from the entry-captured frame: the CARRIED change is the
/// frame patch (`frame.sstatus` FS=Dirty) at the body tail, and the
/// live `enable()` only serves the dispatch path. The BKL guards the two
/// cross-process `fpu_state` accesses — same discipline as the other
/// trap bodies.
#[cfg(target_arch = "riscv64")]
unsafe fn riscv64_fpu_trap_body(frame: &mut minix_arch::riscv64::trap_stub::Riscv64TrapFrame) -> u64 {
    use minix_arch::riscv64::trap_stub::PARK_NONE;
    let smp = unsafe { crate::smp_state_boot_unchecked() };
    let cpu = crate::current_cpu_id();
    let cur_nr = smp
        .cpu_local(cpu)
        .and_then(|l| l.proc_ptr)
        .unwrap_or_else(|| {
            panic!(
                "riscv64 FPU trap before scheduler bring-up (proc_ptr = \
                 None on cpu {cpu:?}) — wiring bug"
            )
        });
    let fpu_bkl = crate::smp::bkl_lock_or_inherit();
    debug_assert!(
        fpu_bkl,
        "riscv64 FPU trap reached from kernel context (BKL inherited) — \
         FS=Off must not trap S-mode with an FP-free kernel, so this is a \
         wiring bug"
    );
    let table = unsafe { crate::proc_table_boot_unchecked() };
    use minix_arch::{CurrentFpuArch, FpuArch};
    let fpu = CurrentFpuArch::default();
    // C proc.c:1936-1942 — save the outgoing owner (EXT_REG_INITIALIZED
    // gates processes that never touched FP: their fpu_state is the
    // zeroed default and saving would fabricate a state).
    let old_owner = smp.cpu_local(cpu).and_then(|l| l.fpu_owner);
    // C proc.c:1938 — assert(*local_fpu_owner != p).
    debug_assert!(
        old_owner != Some(cur_nr),
        "riscv64 FPU trap: current process is already the FPU owner"
    );
    if let Some(owner_nr) = old_owner.filter(|&o| o != cur_nr) {
        let used_fpu = table.get(owner_nr).is_some_and(|p| {
            p.p_misc_flags
                .is_set(crate::proc::MiscFlagsBits::EXT_REG_INITIALIZED)
        });
        if used_fpu {
            if let Some(p) = table.get_mut(owner_nr) {
                fpu.save(&mut p.fpu_state);
            }
        }
    }
    // C proc.c:1946 — restore the trapping process's state (the zeroed
    // default for a first-touch process is a valid FP state).
    let cur_slot = table.get_mut(cur_nr);
    debug_assert!(
        cur_slot.is_some(),
        "riscv64 FPU trap: current proc slot missing — wiring bug"
    );
    if let Some(p) = cur_slot {
        fpu.restore(&p.fpu_state);
        p.p_misc_flags
            .set(crate::proc::MiscFlagsBits::EXT_REG_INITIALIZED);
    }
    // C proc.c:1954 — ownership passes to the trapping process.
    if let Some(local) = smp.cpu_local_mut(cpu) {
        local.fpu_owner = Some(cur_nr);
    }
    fpu.enable(); // live write: inert past this trap's sret (frame patch carries), kept for symmetry.
    // The riscv FS bits live in sstatus, and the trap stub's exit does a
    // whole `csrw sstatus` write-back of the frame values captured at entry
    // — a live enable() write does not survive to sret. The carrier is the
    // patched frame with FS=Dirty (a conservative value: with Initial the
    // hardware already permits execution and moves Initial→Dirty
    // automatically, so there is no repeated trap; Dirty makes the state
    // semantics equal to "FP just executed", and the next switch-out
    // capture reports it truthfully).
    frame.sstatus = (frame.sstatus & !(0b11 << 13)) | (0b11 << 13); // FS=Dirty (0b10 would be Clean, mislabeled)
    if fpu_bkl {
        crate::smp::bkl_unlock();
    }
    PARK_NONE
}

/// Console diagnostics + panic for unreached causes (aarch64 sibling
/// carries the rationale).
#[cfg(target_arch = "riscv64")]
fn riscv64_diag_panic(
    frame: &minix_arch::riscv64::trap_stub::Riscv64TrapFrame,
    scause: u64,
    origin: &str,
) -> ! {
    use crate::kmess::{EarlyConsole, KernelConsole as Console};
    let stval = riscv64_read_stval();
    Console::write_str(origin);
    Console::write_str(" scause=");
    Console::write_hex(scause);
    Console::write_str(" stval=");
    Console::write_hex(stval);
    Console::write_str(" sepc=");
    Console::write_hex(frame.sepc);
    Console::write_str(" sstatus=");
    Console::write_hex(frame.sstatus);
    // Identity + stack pointer make an AP-side fault distinguishable from a
    // BSP-side one (the AP identity is stack-based; a wrong id here is the
    // first thing to check).
    let cpu = crate::current_cpu_id().raw();
    let sp: usize;
    // SAFETY: reading sp has no side effects.
    unsafe { core::arch::asm!("mv {}, sp", out(reg) sp) };
    Console::write_str(" cpu=");
    Console::write_hex(cpu as u64);
    Console::write_str(" sp=");
    Console::write_hex(sp as u64);
    // User-origin forensics: the caller chain lives in the saved GPRs —
    // ra is the first thing to resolve against the module's own symbols
    // (a wild store inside a library loop, e.g. memset, points back at the
    // caller's argument computation, not at the store itself).
    Console::write_str(" ura=");
    Console::write_hex(frame.gpr[1]);
    Console::write_str(" usp=");
    Console::write_hex(frame.sp());
    Console::write_str(" ua0=");
    Console::write_hex(frame.gpr[10]);
    Console::write_str(" ua2=");
    Console::write_hex(frame.gpr[12]);
    Console::write_str("\n");
    panic!(
        "riscv64 {origin}: scause {scause:#x} stval {stval:#x} sepc {:#x} cpu {cpu} sp {sp:#x}",
        frame.sepc
    );
}

/// Current-EL (kernel) body: GIC claim → route → dispatch, kernel-fault
/// diagnostics. IRQ: the claim read IS the identification register on
/// GIC, so the INTID routes the chain and travels to
/// `dispatch_claimed_hardware_irq` for the completion half. Sync: a
/// kernel-origin synchronous exception is a kernel bug at this stage —
/// panic with the syndrome (the EL0 page-fault arm lands with the
/// three-arch wave).
///
/// # Safety
///
/// `frame` points at the live kernel-leg frame (asm contract); the stub
/// resumes via `eret` when this returns.
#[cfg(target_arch = "aarch64")]
pub unsafe extern "C" fn aarch64_kernel_body(
    frame: &mut minix_arch::arm64::trap_stub::AArch64TrapFrame,
    class: u64,
) -> u64 {
    // Kernel-origin legs never park (a blocked IPC is a lower-EL event);
    // every exit restores/returns normally, so the park decision is NONE.
    use minix_arch::arm64::trap_stub::PARK_NONE;
    if class == minix_arch::arm64::trap_stub::TRAP_CLASS_IRQ {
        let claimed = crate::irq_manager::claim_hardware_irq();
        match claimed {
            // Spurious: nothing claimable, nothing to complete.
            None => return PARK_NONE,
            Some(id) if id == GIC_SPURIOUS_INTID => return PARK_NONE,
            // Scheduler SGI: no hook chain. Run C's smp_sched_handler +
            // smp_ipi_sched_handler policy under the BKL, then complete the
            // claim exactly once (the generic path would mask the SGI and
            // return Spurious without completing it).
            Some(id) if id == minix_arch::arm64::smp::SCHED_SGI_INTID => {
                let acquired = crate::smp::bkl_lock_or_inherit();
                // C context_stop_idle at interrupt entry: clear the idle
                // flag and restart this CPU's one-shot before the handler.
                crate::context_stop_idle_current();
                {
                    let section = unsafe { crate::smp::BklSection::assume_held() };
                    let table = crate::proc_table_with(&section);
                    let smp = crate::smp_state_with(&section);
                    let cpu = crate::current_cpu_id();
                    smp.sched_handler_full(table, cpu);
                    smp.preempt_current_on_ipi(table, cpu);
                }
                if acquired {
                    crate::smp::bkl_unlock();
                }
                crate::irq_manager::complete_hardware_irq(Some(id));
                return PARK_NONE;
            }
            Some(id) => {
                // INTIDs ≥ NR_IRQ_VECTORS cannot be enabled (the
                // controller clamps its line count at the manager bound)
                // — routing one would alias through the u8 conversion.
                if id as usize >= minix_plat::NR_IRQ_VECTORS {
                    panic!(
                        "aarch64 IRQ route: intid {id} beyond NR_IRQ_VECTORS — \
                         controller wiring bug"
                    );
                }
                // The boot clock PPI: per-CPU re-arm + observability tick
                // BEFORE the hook chain (the riscv64 timer-arm shape;
                // CNTP is a one-shot — without the re-arm the tick train
                // stops dead after the first interrupt).
                let is_timer = id as u8 == minix_plat::TIMER_IRQ.get();
                if is_timer {
                    crate::clock::local_tick(crate::clock::current_cpuid());
                }
                let irq = minix_plat::IrqVector::new(id as u8);
                match crate::irq_manager::dispatch_claimed_hardware_irq(irq, claimed) {
                    Ok(()) => {}
                    Err(crate::irq_manager::IrqError::Spurious(line)) => {
                        use crate::kmess::{EarlyConsole, KernelConsole as Console};
                        Console::write_str("spurious irq ");
                        Console::write_hex(line.get() as u64);
                        Console::write_str("\n");
                    }
                    Err(e) => panic!("aarch64 IRQ route: dispatch error {e:?} (intid {id})"),
                }
                // Quantum enforcement for a timer tick (C proc.c:418-424;
                // try-or-inherit BKL — the SCHED_IPI shape).
                if is_timer {
                    let acquired = crate::smp::bkl_lock_or_inherit();
                    // C context_stop_idle: a tick landing on a halted CPU
                    // clears the idle flag and restarts its timer.
                    crate::context_stop_idle_current();
                    {
                        let section = unsafe { crate::smp::BklSection::assume_held() };
                        let smp = crate::smp_state_with(&section);
                        let cur_nr = {
                            let cpu = crate::current_cpu_id();
                            smp.cpu_local(cpu).and_then(|l| l.proc_ptr)
                        };
                        if let Some(nr) = cur_nr {
                            let table = crate::proc_table_with(&section);
                            let priv_table = crate::priv_table_with(&section);
                            if table.get(nr).is_some_and(|p| p.is_runnable()) {
                                table.check_quantum(nr, priv_table, &section);
                            }
                        }
                    }
                    if acquired {
                        crate::smp::bkl_unlock();
                    }
                }
            }
        }
        return PARK_NONE;
    }
    aarch64_diag_panic(frame, "kernel sync exception");
}

/// Lower-EL (user) body: SVC kernel calls + user-origin diagnostics.
///
/// # Safety
///
/// `frame` points at the live user-leg frame on the EL1 entry stack
/// (asm contract); the interrupted EL0 sp travels in `frame.sp`.
#[cfg(target_arch = "aarch64")]
pub unsafe extern "C" fn aarch64_user_body(
    frame: &mut minix_arch::arm64::trap_stub::AArch64TrapFrame,
    class: u64,
) -> u64 {
    if class == minix_arch::arm64::trap_stub::TRAP_CLASS_IRQ {
        // Interrupts arriving from EL0 route through the same GIC
        // claim/route as kernel-origin ones. Timer-driven preemption does
        // not yet switch here (returns PARK_NONE); quantum flags are set
        // and the process is rescheduled at its next voluntary IPC.
        return aarch64_kernel_body(frame, class);
    }
    // Synchronous: discriminate by ESR_EL1 EC (bits [31:26]).
    // EC 0x15 = SVC from AArch64; `svc` DOES advance ELR — no PC step
    // (minix-sys arch_trap contract).
    const ESR_EC_SHIFT: u64 = 26;
    const ESR_EC_MASK: u64 = 0x3F;
    const EC_SVC_AARCH64: u64 = 0x15;
    // EC 0x07 = Trapped SIMD/FP access from lower EL — the lazy-FPU gate
    // (CPACR_EL1.FPEN=0b01 set by `FpuArch::disable` on a context switch
    // to a non-owner) firing on the process's first FP instruction. C:
    // i386 #NM → `copr_not_available_handler` (proc.c:1922).
    const EC_SIMD_FP_LOWER_EL: u64 = 0x07;
    // EC 0x20 = instruction abort from a lower EL; EC 0x24 = data abort
    // from a lower EL — aarch64's #PF (§1.116): route them through the
    // page-fault leg instead of dying on the diagnostic panic. The same-EL
    // legs (0x21 / 0x25) are dispatched by the entry stub to
    // `aarch64_kernel_body` on the SPSR mode, so they never reach here.
    // (Still on the panic leg, C SIGSEGV/SIGILL parity — §1.116 gap: EC
    // 0x22 PC-align at EL0, EC 0x00 UNDEF.)
    const EC_INST_ABORT_LOWER_EL: u64 = 0x20;
    const EC_DATA_ABORT_LOWER_EL: u64 = 0x24;
    let esr = aarch64_read_esr();
    let ec = (esr >> ESR_EC_SHIFT) & ESR_EC_MASK;
    if ec == EC_SVC_AARCH64 {
        let leg = frame.gpr[8] as u64; // x8 = call number register
        if leg == KERNEL_CALL_TRAP {
            // The message leg answers with a reply code, or suspends the
            // caller on RTS_VMREQUEST (VmSuspend → park, §1.116 — the
            // aarch64 mirror of the x86 SYSCALL leg's suspend arm).
            return aarch64_kernel_call_leg(frame);
        }
        // Raw IPC legs + MINIX_KERNINFO (x8 = 1..16): the EL1 mirror
        // of the x86 int-33 bridge. An unknown call
        // number is rejected inside the body (EBADCALL). The body's
        // park decision (§1.113) rides x0 back to `EL0BODY`.
        return aarch64_ipc_dispatch_body(frame);
    }
    if matches!(ec, EC_INST_ABORT_LOWER_EL | EC_DATA_ABORT_LOWER_EL) {
        return aarch64_pagefault_body(frame, esr, ec);
    }
    if ec == EC_SIMD_FP_LOWER_EL {
        return aarch64_fpu_trap_body();
    }
    // The acting arm for the remaining user synchronous ECs (C:
    // exception.c:275 user-origin cause_sig parity). The shape mirrors this
    // leg's Segv arm. EC=0x2f (SError) and other asynchronous/hardware-level
    // ECs stay on the diagnostic panic lane (the i386 semantics of
    // "Machine check" → SIGBUS were classified as a kernel stop in this
    // project's earlier evidence work).
    if let Some(sig) = aarch64_sync_ec_signal(ec) {
        use minix_arch::arm64::trap_stub::PARK_RESCHEDULE;
        let ipc_bkl = crate::smp::bkl_lock_or_inherit();
        let smp = unsafe { crate::smp_state_boot_unchecked() };
        let cpu = crate::current_cpu_id();
        let cur_nr = smp.cpu_local(cpu).and_then(|l| l.proc_ptr).unwrap_or_else(|| {
            panic!(
                "aarch64 user sync fault before scheduler bring-up (proc_ptr = \
                 None on cpu {cpu:?}) — wiring bug"
            )
        });
        let table = unsafe { crate::proc_table_boot_unchecked() };
        {
            let proc = table
                .get_mut(cur_nr)
                .unwrap_or_else(|| panic!("aarch64 sync fault from invalid proc nr {cur_nr:?}"));
            minix_arch::save_frame_to_context(frame, &mut proc.cpu_context);
            proc.trap_style = TrapStyle::FullContext;
        }
        crate::syscall_signal::cause_signal(
            cur_nr,
            sig,
            table,
            unsafe { crate::priv_table_boot_unchecked() },
        );
        if ipc_bkl {
            crate::smp::bkl_unlock();
        }
        return PARK_RESCHEDULE;
    }
    aarch64_diag_panic(frame, "user sync exception");
}

/// aarch64 user synchronous EC → signal mapping (pure function, host-
/// testable). The C ground truth is exception.c's exception table,
/// user-origin parity: EC 0x00 (Unknown) / 0x0e (Illegal execution state)
/// → "Invalid opcode" → SIGILL; 0x26 (SP alignment) / 0x27 (PC alignment)
/// → "Alignment check" → SIGBUS; 0x3c (BRK) → "Debug exception" → SIGTRAP.
/// None = stay on the diagnostic panic lane (SError 0x2f, supervisor-state
/// / reserved EC). Page faults (0x20/0x24), the FP gate (0x07) and SVC
/// (0x15) are handled by earlier arms and do not enter this mapping.
#[cfg(any(target_arch = "aarch64", test))]
fn aarch64_sync_ec_signal(ec: u64) -> Option<u32> {
    use crate::syscall_signal::{SIGBUS, SIGILL, SIGTRAP};
    match ec {
        0x00 | 0x0e => Some(SIGILL),
        0x26 | 0x27 => Some(SIGBUS),
        0x3c => Some(SIGTRAP),
        _ => None,
    }
}

/// How a lower-EL synchronous abort resolved through
/// [`classify_abort_disposition`] should be acted on. Mirrors the
/// x86 `ExceptionOutcome` arms that the vector-14/#PF dispatcher
/// produces (exception.c:49-130 parity).
#[cfg(any(target_arch = "aarch64", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AbortDisposition {
    /// A resolvable fault (translation / access flag / permission) —
    /// forward to VM as a PAGEFAULT, C: pagefault() exception.c:112-129.
    Pagefault,
    /// An unresolvable fault (address size, speculative unhandled,
    /// implementation-defined) — deliver SIGSEGV, C: cause_sig
    /// exception.c:276.
    Segv,
    /// Synchronous External Abort (DFSC 0b01xxxx, incl. SEA-on-translation
    /// 0b010000): hardware error, no software recovery — kernel panic,
    /// the x86 machine-check analogue.
    Sea,
    /// Reserved DFSC encoding or an access-flag fault on stage-2 — not a
    /// fault this port produces; panic rather than mis-route.
    Unknown,
}

/// DFSC (ISS[5:0]) → disposition. Architecture-neutral pure helper so it
/// is host-testable from the mock lane (§1.116: aarch64 had no #PF leg at
/// all — every EL0 abort fell into the diagnostic panic; the x86 #PF arm
/// is the ground truth this maps onto).
///
/// Armv8 DDI0487 ESR_EL1 fault-status grouping, stage-1 (type in
/// ISS[5:2], translation level in ISS[1:0]):
/// - 0b0000xx address size (levels 0-3) → Segv — the VA does not fit the
///   translation geometry; no mapping VM installs can fix it. A user
///   access must not panic the kernel, hence SIGSEGV (never Unknown).
/// - 0b0001xx translation, 0b0010xx access flag, 0b0011xx permission
///   (levels 0-3) → Pagefault — all resolvable by VM's own page-table ops
///   (this port installs AF=1 so access-flag is not expected, but it is
///   still VM-resolvable; C maps pages read-only and promotes on the write
///   permission fault).
/// - 0b0100xx synchronous external abort (incl. SEA-on-translation) →
///   Sea — hardware error, no software recovery.
/// - alignment / TLB-conflict / IMPLEMENTATION-DEFINED / reserved
///   encodings → Unknown.
#[cfg(any(target_arch = "aarch64", test))]
pub(crate) fn classify_abort_disposition(esr: u64) -> AbortDisposition {
    match esr & 0x3F {
        0x00..=0x03 => AbortDisposition::Segv, // address size
        0x04..=0x0F => AbortDisposition::Pagefault, // translation / access flag / permission
        0x10..=0x13 => AbortDisposition::Sea, // synchronous external abort
        _ => AbortDisposition::Unknown,
    }
}

/// EC + WnR → the x86-style #PF error code that rides VPF_FLAGS (C:
/// exception.c:122 stores the raw frame error code; VM's fault handlers
/// test bit1 for write — pgtable.c's CoW / `PF_DISABLEP`-style decoding —
/// so that lane must be truthful or a COW write is mis-served as a read).
///
/// Bit layout (x86 #PF): P=bit0 (constant 1 — every leg routed here is
/// lower-EL/user), W=bit1, user-instruction-fetch=bit2, I/D=bit4.
/// `is_instr` comes from the EC (0x20 instruction abort); the read/write
/// direction is ESR ISS[6] = WnR, NOT the DFSC — DFSC bits[1:0] encode the
/// translation level, so permission faults 0x0C..0x0F are level tags,
/// orthogonal to write direction (instruction aborts are always a fetch).
/// The §1.116 INIT fetch fault (EC=0x20) converts to `0x15`
/// (P | I/D | user-IF, not write). The W lane is masked for instruction
/// faults so a caller mis-setting WnR on a fetch cannot corrupt VM's CoW
/// decoding (an instruction abort is architecturally always a read).
#[cfg(any(target_arch = "aarch64", test))]
pub(crate) fn aarch64_pf_error_code(is_instr: bool, is_write: bool) -> u32 {
    let mut err = 1u32; // P: user mode
    if is_write && !is_instr {
        err |= 2; // W lane: ISS[6] WnR — the only lane VM consumes for CoW
    }
    if is_instr {
        err |= 4; // user-mode instruction fetch (bit2)
        err |= 16; // I/D lane: instruction-side fault (bit4)
    }
    err
}

#[cfg(test)]
mod abort_classification_tests {
    use super::*;

    #[test]
    fn dfsc_groups_route_to_the_right_disposition() {
        // translation levels 0-3 → Pagefault (INIT's §1.116 fault: 0x05 =
        // 0b000101, type=translation, level=1)
        for dfsc in [0x04u64, 0x05, 0x06, 0x07] {
            assert_eq!(classify_abort_disposition(dfsc), AbortDisposition::Pagefault, "{dfsc:#x}");
        }
        // access flag (VM-resolvable: set AF) → Pagefault
        for dfsc in [0x08u64, 0x09, 0x0A, 0x0B] {
            assert_eq!(classify_abort_disposition(dfsc), AbortDisposition::Pagefault, "{dfsc:#x}");
        }
        // permission levels 0-3 → Pagefault (VM read-only → write promote)
        for dfsc in [0x0Cu64, 0x0D, 0x0E, 0x0F] {
            assert_eq!(classify_abort_disposition(dfsc), AbortDisposition::Pagefault, "{dfsc:#x}");
        }
        // address size levels 0-3 → Segv (geometry, not resolvable; a user
        // access must never panic the kernel)
        for dfsc in [0x00u64, 0x01, 0x02, 0x03] {
            assert_eq!(classify_abort_disposition(dfsc), AbortDisposition::Segv, "{dfsc:#x}");
        }
        // synchronous external abort → Sea
        for dfsc in [0x10u64, 0x11, 0x12, 0x13] {
            assert_eq!(classify_abort_disposition(dfsc), AbortDisposition::Sea, "{dfsc:#x}");
        }
    }

    #[test]
    fn illegal_insn_fpu_trap_predicate_gates_on_fs_off() {
        // FS=00 (Off) → lazy gate trap; the other three arms
        // (Initial/Clean/Dirty) → a real SIGILL falling to the diagnostic
        // leg — matching the aarch64 semantics where EC=0x07 is only
        // reported when FPEN gates it (riscv scause does not separate the
        // lanes; the gate is the discriminator).
        assert!(riscv64_illegal_insn_is_fpu_trap(0b00 << 13));
        assert!(!riscv64_illegal_insn_is_fpu_trap(0b01 << 13));
        assert!(!riscv64_illegal_insn_is_fpu_trap(0b10 << 13));
        assert!(!riscv64_illegal_insn_is_fpu_trap(0b11 << 13));
        // The remaining sstatus bits do not affect the discrimination.
        assert!(riscv64_illegal_insn_is_fpu_trap(
            (0b00 << 13) | 0x8000000000000000 | 0x100
        ));
        assert!(!riscv64_illegal_insn_is_fpu_trap(
            (0b11 << 13) | 0x8000000000000000
        ));
    }

    #[test]
    fn riscv64_sync_fault_signal_maps_c_exception_table() {
        // The mapping follows C exception.c's exception table, user-origin parity.
        use crate::syscall_signal::{SIGBUS, SIGILL, SIGTRAP};
        // Misaligned fetch/load/store → SIGBUS (C "Alignment check").
        assert_eq!(riscv64_sync_fault_signal(0), Some(SIGBUS));
        assert_eq!(riscv64_sync_fault_signal(4), Some(SIGBUS));
        assert_eq!(riscv64_sync_fault_signal(6), Some(SIGBUS));
        // Real illegal instruction → SIGILL (C "Invalid opcode"); the
        // FS=Off gated FP trap is taken by an earlier arm and does not go
        // through this mapping.
        assert_eq!(riscv64_sync_fault_signal(2), Some(SIGILL));
        // Breakpoint → SIGTRAP (C "Debug exception").
        assert_eq!(riscv64_sync_fault_signal(3), Some(SIGTRAP));
        // The three page-fault causes are forwarded to VM by earlier arms;
        // supervisor-state causes / reserved encodings → None (diagnostic
        // panic lane).
        for handled in [12u64, 13, 15, 8] {
            assert_eq!(riscv64_sync_fault_signal(handled), None, "scause {handled}");
        }
        assert_eq!(riscv64_sync_fault_signal(16), None);
        assert_eq!(riscv64_sync_fault_signal(u64::MAX), None);
    }

    #[test]
    fn aarch64_sync_ec_signal_maps_c_exception_table() {
        use crate::syscall_signal::{SIGBUS, SIGILL, SIGTRAP};
        // EC 0x00 (Unknown) / 0x0e (Illegal execution state) → SIGILL.
        assert_eq!(aarch64_sync_ec_signal(0x00), Some(SIGILL));
        assert_eq!(aarch64_sync_ec_signal(0x0e), Some(SIGILL));
        // EC 0x26 (SP alignment) / 0x27 (PC alignment) → SIGBUS.
        assert_eq!(aarch64_sync_ec_signal(0x26), Some(SIGBUS));
        assert_eq!(aarch64_sync_ec_signal(0x27), Some(SIGBUS));
        // EC 0x3c (BRK) → SIGTRAP.
        assert_eq!(aarch64_sync_ec_signal(0x3c), Some(SIGTRAP));
        // ECs with dedicated arms and SError/reserved encodings → None.
        for handled in [0x15u64, 0x07, 0x20, 0x24, 0x2f, 0x01] {
            assert_eq!(aarch64_sync_ec_signal(handled), None, "ec {handled:#x}");
        }
    }

    #[test]
    fn init_text_fetch_fault_converts_to_pf_read_code() {
        // §1.116 real-machine signature: EC=0x20 (instruction abort),
        // DFSC=0x05 (level-1 translation) → P | user-IF | I/D = 0x15,
        // NOT a write (instruction fetch is always a read).
        assert_eq!(aarch64_pf_error_code(true, false), 0x15);
        // data-side read fault: P only
        assert_eq!(aarch64_pf_error_code(false, false), 0x01);
        // data-side WnR=1 write (the CoW-first-write lane): P | W
        assert_eq!(aarch64_pf_error_code(false, true), 0x03);
        // a write on the instruction side never happens (EC=0x20 is fetch)
        assert_eq!(aarch64_pf_error_code(true, true), 0x15);
    }

    #[test]
    fn riscv_pf_error_code_matches_the_aarch64_lane_table() {
        // Cross-arch consistency, pinned two ways: (1) the bare
        // privileged-spec cause codes are the
        // architecture contract — a constant typo must turn this red
        // even though the dispatch arm consumes the same constant;
        // (2) the lane table is compared against the aarch64 converter
        // itself (cfg(test)-visible), not a hand-copied literal, so any
        // single-leg drift fails.
        assert_eq!(RISCV64_CAUSE_INST_PF, 12);
        assert_eq!(RISCV64_CAUSE_LOAD_PF, 13);
        assert_eq!(RISCV64_CAUSE_STORE_PF, 15);
        assert_eq!(
            riscv64_pf_error_code(RISCV64_CAUSE_INST_PF),
            aarch64_pf_error_code(true, false)
        );
        assert_eq!(
            riscv64_pf_error_code(RISCV64_CAUSE_LOAD_PF),
            aarch64_pf_error_code(false, false)
        );
        assert_eq!(
            riscv64_pf_error_code(RISCV64_CAUSE_STORE_PF),
            aarch64_pf_error_code(false, true)
        );
        // Spell out the expected encodings once more for the reader:
        // fetch = 0x15, read = 0x01, write = 0x03.
        assert_eq!(riscv64_pf_error_code(RISCV64_CAUSE_INST_PF), 0x15);
        assert_eq!(riscv64_pf_error_code(RISCV64_CAUSE_LOAD_PF), 0x01);
        assert_eq!(riscv64_pf_error_code(RISCV64_CAUSE_STORE_PF), 0x03);
    }

    #[test]
    fn classification_matches_the_expected_dfsc_table() {
        // Explicit 64-encoding expectation table: a group written in the
        // wrong order turns this test red on the host lane.
        use AbortDisposition::*;
        const EXPECT: [AbortDisposition; 0x40] = {
            let mut t = [Unknown; 0x40];
            let mut i = 0usize;
            while i < 0x40 {
                t[i] = match i as u64 {
                    0x00..=0x03 => Segv,
                    0x04..=0x0F => Pagefault,
                    0x10..=0x13 => Sea,
                    _ => Unknown,
                };
                i += 1;
            }
            t
        };
        for dfsc in 0u64..0x40 {
            assert_eq!(
                classify_abort_disposition(dfsc),
                EXPECT[dfsc as usize],
                "dfsc {dfsc:#04x}"
            );
        }
    }
}

/// The aarch64 page-fault leg (§1.116): the EL0 instruction/data abort
/// analogue of the x86 vector-14 `ForwardToVm` acting arm.
///
/// Saves the user register file (`FullContext`), records
/// RTS_PAGEFAULT + p_fault_addr, and `mini_send`s the VM_PAGEFAULT
/// message FROM_KERNEL — all through the shared
/// [`forward_pagefault_to_vm`] (single source of truth for the C
/// exception.c:112-129 tail). Ends in the §1.113 switch-after-pop:
/// the faulting process is parked, so `PARK_RESCHEDULE` unwinds the
/// EL0 frame and hands off to the resched thunk (the EL1h analogue of
/// x86's `scheduler_loop` divergence from this body).
///
/// # Safety
///
/// `frame` is the live lower-EL sync frame on the EL1 entry stack and
/// the scheduler has a current process (same contract as
/// [`aarch64_ipc_dispatch_body`]).
#[cfg(target_arch = "aarch64")]
unsafe fn aarch64_pagefault_body(
    frame: &mut minix_arch::arm64::trap_stub::AArch64TrapFrame,
    esr: u64,
    ec: u64,
) -> u64 {
    use minix_arch::arm64::trap_stub::PARK_RESCHEDULE;
    // EC 0x20 = instruction abort from a lower EL — the I/D lane rides on
    // the EC (DFSC parity is a 4K/16K granule hint, not I/D). The same-EL
    // leg (0x21) never reaches this body (dispatched to the kernel body).
    let is_instr = ec == 0x20;
    let dfsc = esr & 0x3F;
    let disposition = classify_abort_disposition(esr);
    let far = aarch64_read_far();
    // ISS[6] = WnR carries the read/write direction (instruction aborts
    // are always a fetch, so their ISS[6] is not a write indicator).
    let is_write = !is_instr && ((esr >> 6) & 1) != 0;
    let errcode = aarch64_pf_error_code(is_instr, is_write);

    let smp = unsafe { crate::smp_state_boot_unchecked() };
    let cpu = crate::current_cpu_id();
    let cur_nr = smp
        .cpu_local(cpu)
        .and_then(|l| l.proc_ptr)
        .unwrap_or_else(|| {
            panic!(
                "aarch64 abort trap before scheduler bring-up (proc_ptr = \
                 None on cpu {cpu:?}) — wiring bug"
            )
        });

    match disposition {
        AbortDisposition::Sea | AbortDisposition::Unknown => panic!(
            "aarch64 {:?} at far {:#x} (DFSC {:#x}, elr {:#x}) — not a \
             recoverable fault",
            disposition, far, dfsc, frame.elr
        ),
        AbortDisposition::Segv => {
            let ipc_bkl = crate::smp::bkl_lock_or_inherit();
            let table = unsafe { crate::proc_table_boot_unchecked() };
            // Persist the fault-time register file + entry style BEFORE
            // signalling (x86 Signal arm parity, trap_dispatch.rs:1049-
            // 1058): the signal manager's later SYS_SIGSEND builds the
            // handler frame on top of this context and rejects a `NoEntry`
            // target, so the EL0 frame must be saved and FullContext
            // recorded here (finish_and_restore consumes the style back to
            // NoEntry on every return to user).
            {
                let proc = table
                    .get_mut(cur_nr)
                    .unwrap_or_else(|| panic!("aarch64 segv from invalid proc nr {cur_nr:?}"));
                minix_arch::save_frame_to_context(frame, &mut proc.cpu_context);
                proc.trap_style = TrapStyle::FullContext;
            }
            crate::syscall_signal::cause_signal(
                cur_nr,
                crate::syscall_signal::SIGSEGV,
                table,
                unsafe { crate::priv_table_boot_unchecked() },
            );
            if ipc_bkl {
                crate::smp::bkl_unlock();
            }
            // `cause_signal` sets RTS_SIGNALED (rts_set dequeues the
            // process), so it is no longer runnable — `eret`-ing back would
            // resume a descheduled context and livelock the same fault on
            // the single core. Park instead: the scheduler runs the signal
            // manager, which resolves delivery (x86 twin ends in
            // `scheduler_loop`).
            PARK_RESCHEDULE
        }
        AbortDisposition::Pagefault => {
            // VM's own fault cannot be forwarded to VM — C: "pagefault
            // in VM" panic (exception.c:101-118).
            if cur_nr == crate::proc::proc_nr::VM_PROC_NR {
                use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
                Console::write_str("pagefault for VM elr ");
                Console::write_hex(frame.elr);
                Console::write_str(" far ");
                Console::write_hex(far);
                Console::write_str("\n");
                panic!("pagefault in VM");
            }
            // User code runs without the BKL (§1.113 resched-thunk
            // convention: unlock before PARK_RESCHEDULE, the thunk
            // re-acquires).
            let ipc_bkl = crate::smp::bkl_lock_or_inherit();
            let table = unsafe { crate::proc_table_boot_unchecked() };
            {
                let proc = table
                    .get_mut(cur_nr)
                    .unwrap_or_else(|| panic!("aarch64 pagefault from invalid proc nr {cur_nr:?}"));
                minix_arch::save_frame_to_context(frame, &mut proc.cpu_context);
                proc.trap_style = TrapStyle::FullContext;
            }
            let priv_table = unsafe { crate::priv_table_boot_unchecked() };
            // Temporary diagnostic (rolled out once the stray-address case
            // closed): print (proc, elr, far, lr) before forwarding a
            // non-VM U-mode abort — pf-exit logs only cr2, no PC.
            // Capped at 40 (the other probes in this file all have bounds;
            // an unbounded flood busy-waits on the serial port character by
            // character while the BKL is held, inflating boot time).
            #[cfg(not(feature = "mock"))]
            {
                static PFA_N: core::sync::atomic::AtomicUsize =
                    core::sync::atomic::AtomicUsize::new(0);
                if PFA_N.fetch_add(1, core::sync::atomic::Ordering::Relaxed) < 40 {
                    use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
                }
            }
            if let Err(e) = forward_pagefault_to_vm(table, priv_table, cur_nr, far, errcode) {
                // C: panic("WARNING: pagefault: mini_send returned %d")
                panic!("pagefault: mini_send returned {e:?}");
            }
            if ipc_bkl {
                crate::smp::bkl_unlock();
            }
            PARK_RESCHEDULE
        }
    }
}

/// The diverging park-and-reschedule thunk, registered with
/// the aarch64 `EL0BODY` entry stub. Reached via `aarch64_resched_entry`
/// after asm has unwound a blocked-receiver's EL0 frame back to the clean
/// entry base, so the rest of the switch runs on a non-drifting EL1h
/// stack. Re-acquires the BKL that `kernel_call_finish_ipc_door` released
/// (`scheduler_loop` consumes the held-BKL convention) and enters the
/// scheduling loop — the aarch64 twin of the x86 `reenter_scheduler` body.
///
/// # Safety
///
/// Called only from the `EL0BODY` park branch (single, well-defined SP_EL1
/// base; BKL not held; current proc already parked/dequeued by the IPC
/// engine).
#[cfg(target_arch = "aarch64")]
pub unsafe extern "C" fn aarch64_resched_thunk() -> ! {
    let guard = crate::smp::bkl_lock();
    core::mem::forget(guard);
    let cpu = crate::current_cpu_id();
    crate::scheduler_loop(cpu)
}

#[cfg(target_arch = "aarch64")]
fn aarch64_read_esr() -> u64 {
    let v: u64;
    // SAFETY: side-effect-free system-register read at EL1.
    unsafe { core::arch::asm!("mrs {}, esr_el1", out(reg) v, options(nomem, nostack)); }
    v
}

/// The KERNEL_CALL leg: message pointer at x0 — `kernel_call` parity
/// with the x86 SYSCALL body (riscv64 sibling carries the full contract).
///
/// # Safety
///
/// Same invariants as `riscv64_kernel_call_leg`.
#[cfg(target_arch = "aarch64")]
unsafe fn aarch64_kernel_call_leg(
    frame: &mut minix_arch::arm64::trap_stub::AArch64TrapFrame,
) -> u64 {
    use minix_arch::arm64::trap_stub::{PARK_NONE, PARK_RESCHEDULE};
    let smp = unsafe { crate::smp_state_boot_unchecked() };
    let cpu = crate::current_cpu_id();
    let cur_nr = smp
        .cpu_local(cpu)
        .and_then(|l| l.proc_ptr)
        .unwrap_or_else(|| {
            panic!(
                "aarch64 kernel call before scheduler bring-up (proc_ptr = \
                 None on cpu {cpu:?}) — wiring bug"
            )
        });
    let m_user = VirBytes::new(frame.gpr[0]);
    let table = unsafe { crate::proc_table_boot_unchecked() };
    let result = crate::syscall::kernel_call(
        cur_nr,
        table,
        m_user,
        unsafe { crate::priv_table_boot_unchecked() },
        unsafe { crate::clock_state_boot_unchecked() },
        &crate::ipc::KernelUserCopy,
    );
    match result.reply_wire() {
        Some(wire) => {
            frame.gpr[0] = wire as i64 as u64;
            PARK_NONE
        }
        None => {
            // C: system.c:76 `if (result != EDONTREPLY)` — NoReply delivers
            // no reply and writes no x0 (all three legs synchronized; this
            // arm's old panic assumption was disproved by the real-machine C
            // test's _exit).
            if matches!(result, crate::syscall::KcallResult::NoReply) {
                return PARK_NONE;
            }
            // VmSuspend (§1.116): the call parked the caller on
            // RTS_VMREQUEST — enqueue/dequeue/notify VM were already done
            // by kernel_call_finish (the single owner of that bookkeeping
            // on this leg, x86 SYSCALL parity; re-chaining here would
            // head-insert twice and self-link the memreq list). This arm
            // only adds the leg-specific context save so the KCALL_RESUME
            // redispatch can deliver the reply into the saved registers.
            if !matches!(result, crate::syscall::KcallResult::VmSuspend) {
                panic!("aarch64 kernel call returned {result:?} with no reply code");
            }
            {
                let proc = table
                    .get_mut(cur_nr)
                    .expect("aarch64 kernel call suspend: caller slot must exist");
                minix_arch::save_frame_to_context(frame, &mut proc.cpu_context);
                proc.trap_style = TrapStyle::FullContext;
                if let Some(ctx) = proc.p_vm_suspend.as_mut() {
                    ctx.saved_m_user = Some(m_user.0);
                }
            }
            // The caller is non-runnable; hand off through the §1.113
            // switch-after-pop (the resched thunk re-acquires the BKL).
            PARK_RESCHEDULE
        }
    }
}

/// The raw-IPC leg (SVC with x8 = 1..16): the EL1 mirror of
/// [`x86_ipc_dispatch_body`]. Register ABI per
/// `minix-sys::arch_trap::ipc_trap` (aarch64): x8 = call number, x0 =
/// src/dst endpoint (SENDA: count), x1 = message pointer (SENDA: table
/// pointer); errno returns in x0 and IPC status rides x1 (the X1 status
/// lane). `svc` already advanced ELR, so — unlike the riscv64 `ecall`
/// leg — the PC must NOT be stepped here.
///
/// # Safety
///
/// Same invariants as `aarch64_kernel_call_leg`: `frame` is the live
/// lower-EL frame and the current proc is scheduled.
#[cfg(target_arch = "aarch64")]
unsafe fn aarch64_ipc_dispatch_body(
    frame: &mut minix_arch::arm64::trap_stub::AArch64TrapFrame,
) -> u64 {
    use minix_arch::arm64::trap_stub::{PARK_NONE, PARK_RESCHEDULE};
    let smp = unsafe { crate::smp_state_boot_unchecked() };
    let cpu = crate::current_cpu_id();
    let cur_nr = smp
        .cpu_local(cpu)
        .and_then(|l| l.proc_ptr)
        .unwrap_or_else(|| {
            panic!(
                "aarch64 IPC trap before scheduler bring-up (proc_ptr = \
                 None on cpu {cpu:?}) — wiring bug"
            )
        });

    // The SVC arrives from EL0 where this CPU does NOT own the BKL. The
    // entry below mutates the shared PROC_TABLE before dispatch_ipc_entry
    // acquires it fresh — acquire now (same shape as x86), release just before
    // the dispatch hand-off (the BKL is non-reentrant).
    let ipc_bkl = crate::smp::bkl_lock_or_inherit();
    debug_assert!(
        ipc_bkl,
        "aarch64 SVC IPC reached from kernel context (BKL inherited) — \
         dispatch_ipc_entry's fresh acquire would self-deadlock"
    );
    let table = unsafe { crate::proc_table_boot_unchecked() };

    // Persist the user register file before any dispatch side effect —
    // delivery paths OR IPC status into this saved context (the resume asm
    // reads it back before `eret`). Record the ENTRY style so a
    // door-parked caller resumes through the entry-style gate.
    {
        let caller = table
            .get_mut(cur_nr)
            .unwrap_or_else(|| panic!("aarch64 IPC from invalid proc nr {cur_nr:?}"));
        minix_arch::save_frame_to_context(frame, &mut caller.cpu_context);
        caller.trap_style = TrapStyle::FullContext;
    }

    // Pre-decode: unknown call numbers exit before any dispatch (C
    // proc.c:602-606 do_ipc default — EBADCALL without entering the engine).
    let call_nr = frame.gpr[8] as i32;
    if crate::ipc::IpcCall::from_raw(call_nr).is_none() {
        if ipc_bkl {
            crate::smp::bkl_unlock();
        }
        frame.gpr[0] = crate::errno::EBADCALL as i64 as u64;
        return PARK_NONE;
    }

    let r1 = frame.gpr[0]; // src/dst endpoint, or SENDA count
    let r2 = frame.gpr[1]; // message pointer, or SENDA table pointer
    let is_senda = call_nr == (crate::ipc::IpcCall::SendA as i32);
    // Body-less calls (kerninfo / notify / senda) trap with the message
    // register unused — no user buffer is copied (C ipc_minix_kerninfo.S
    // zeroes the operand regs; notify passes no body).
    let is_bodyless = is_senda
        || call_nr == (crate::ipc::IpcCall::KernInfo as i32)
        || call_nr == (crate::ipc::IpcCall::Notify as i32);
    {
        let caller = table
            .get_mut(cur_nr)
            .expect("aarch64 IPC: caller slot must exist");
        caller.p_defer.r2 = r1 as usize;
        caller.p_defer.r3 = if is_senda { r2 as usize } else { 0 };
        if !is_senda {
            caller.p_delivermsg_vir = VirBytes(r2);
        }
    }

    // Copy the user message (kernel-side copy, TOCTOU defense — the same
    // shape as the x86 leg). SENDA carries no message buffer.
    let mut msg = minix_types::Message::default();
    if !is_bodyless {
        use crate::ipc::UserCopy as _;
        match crate::ipc::KernelUserCopy.copy_msg_from_user(VirBytes(r2)) {
            Ok(m) => msg = m,
            Err(_) => {
                // C system.c:152-155 parity: SIGSEGV + EFAULT, without
                // entering IPC dispatch.
                crate::syscall_signal::cause_signal(
                    cur_nr,
                    crate::syscall_signal::SIGSEGV,
                    table,
                    unsafe { crate::priv_table_boot_unchecked() },
                );
                if ipc_bkl {
                    crate::smp::bkl_unlock();
                }
                frame.gpr[0] = crate::errno::EFAULT as i64 as u64;
                return PARK_NONE;
            }
        }
    }
    msg.m_type = call_nr;
    msg.m_source = table
        .get(cur_nr)
        .map(|c| c.p_endpoint)
        .expect("aarch64 IPC: caller slot must exist");

    // Release the entry acquisition so dispatch_ipc_entry takes a fresh
    // lock (non-reentrant BKL); mutual exclusion is unbroken for the
    // instant we hold nothing (we do not touch the table in between).
    if ipc_bkl {
        crate::smp::bkl_unlock();
    }
    let priv_table = unsafe { crate::priv_table_boot_unchecked() };
    let result = crate::syscall::dispatch_ipc_entry(cur_nr, table, &mut msg, priv_table);
    crate::syscall::kernel_call_finish_ipc_door(cur_nr, table, &msg, result, priv_table);

    // Delivered: errno rides x0 out via `eret`; the IPC status bits were
    // ORed into the saved context's X1 by the delivery path — pull that
    // lane back into the frame (sync_status_register_to_frame), then let
    // `EL0BODY` restore normally (PARK_NONE).
    //
    // Blocked (NoReply): the caller was parked by
    // `kernel_call_finish_ipc_door` (dequeued, context already saved above
    // via `save_frame_to_context`). Request the §1.113 switch-after-pop:
    // `EL0BODY` unwinds this EL0 frame back to the clean entry base and
    // branches to the registered resched thunk, which re-acquires the BKL
    // and runs `scheduler_loop` — picking another runnable process. This
    // is the aarch64 EL1h analogue of the x86 leg diverging into
    // `reenter_scheduler`; the difference is only that the single EL1h
    // stack (unlike x86's TSS.sp0-reloading entry) must have the frame
    // popped before the switch, so the decision rides x0 back to asm
    // instead of diverging here. The parked process resumes later through
    // `finish_and_restore` when a delivery makes it runnable again.
    if let Some(code) = result.reply_code() {
        frame.gpr[0] = code as i64 as u64;
        let ctx = &table
            .get(cur_nr)
            .expect("aarch64 IPC: caller slot must exist")
            .cpu_context;
        minix_arch::sync_status_register_to_frame(ctx, frame);
        PARK_NONE
    } else {
        PARK_RESCHEDULE
    }
}

/// The lazy-FPU restore stage (EC=0x07: user FPSIMD access trapped because
/// a context switch left `CPACR_EL1.FPEN=0b01` via [`FpuArch::disable`]).
///
/// C: `copr_not_available_handler()` — proc.c:1922-1962 — the i386 #NM
/// handler's acting half: save the outgoing owner's FPSIMD state
/// (retain=FALSE — its registers become undefined; it re-traps on its
/// next FP use), restore the trapping process's state, hand ownership to
/// the trapping process, re-enable FP (`clts` / FPEN=0b11), and return to
/// re-execute the faulting instruction. ELR reports the FP instruction
/// itself (FP traps do not advance), so returning with `PARK_NONE` (frame
/// untouched) re-executes it.
///
/// The BKL guards the two cross-process accesses (`owner`'s and the
/// trapper's `fpu_state` fields) — same discipline as
/// [`aarch64_ipc_dispatch_body`].
#[cfg(target_arch = "aarch64")]
unsafe fn aarch64_fpu_trap_body() -> u64 {
    use minix_arch::arm64::trap_stub::PARK_NONE;
    let smp = unsafe { crate::smp_state_boot_unchecked() };
    let cpu = crate::current_cpu_id();
    let cur_nr = smp
        .cpu_local(cpu)
        .and_then(|l| l.proc_ptr)
        .unwrap_or_else(|| {
            panic!(
                "aarch64 FPU trap before scheduler bring-up (proc_ptr = \
                 None on cpu {cpu:?}) — wiring bug"
            )
        });
    let fpu_bkl = crate::smp::bkl_lock_or_inherit();
    debug_assert!(
        fpu_bkl,
        "aarch64 FPU trap reached from kernel context (BKL inherited) — \
         FPEN=0b01 must not trap EL1, so this is a wiring bug"
    );
    let table = unsafe { crate::proc_table_boot_unchecked() };
    use minix_arch::{CurrentFpuArch, FpuArch};
    let fpu = CurrentFpuArch::default();
    // C proc.c:1936-1942 — save the outgoing owner (EXT_REG_INITIALIZED
    // gates processes that never touched FP: their fpu_state is the
    // zeroed default and saving would fabricate a state).
    let old_owner = smp.cpu_local(cpu).and_then(|l| l.fpu_owner);
    // C proc.c:1938 — assert(*local_fpu_owner != p): the trapper must not
    // be the owner (FPEN=0b01 after `disable` implies the switch handed
    // ownership away). A hit here means FPEN and the owner bookkeeping
    // desynchronized.
    debug_assert!(
        old_owner != Some(cur_nr),
        "aarch64 FPU trap: current process is already the FPU owner"
    );
    if let Some(owner_nr) = old_owner.filter(|&o| o != cur_nr) {
        let used_fpu = table.get(owner_nr).is_some_and(|p| {
            p.p_misc_flags
                .is_set(crate::proc::MiscFlagsBits::EXT_REG_INITIALIZED)
        });
        if used_fpu {
            if let Some(p) = table.get_mut(owner_nr) {
                fpu.save(&mut p.fpu_state);
            }
        }
    }
    // C proc.c:1946 — restore the trapping process's state. A process
    // that never used FP restores the zeroed default, which is always a
    // valid FPSIMD state (C validates the state header and SIGFPEs on
    // garbage; the rewrite's zero-default cannot be invalid — deviation
    // registered on the fpu wiring unit).
    let cur_slot = table.get_mut(cur_nr);
    debug_assert!(
        cur_slot.is_some(),
        "aarch64 FPU trap: current proc slot missing — wiring bug"
    );
    if let Some(p) = cur_slot {
        fpu.restore(&p.fpu_state);
        p.p_misc_flags
            .set(crate::proc::MiscFlagsBits::EXT_REG_INITIALIZED);
    }
    // C proc.c:1954 — ownership passes to the trapping process.
    if let Some(local) = smp.cpu_local_mut(cpu) {
        local.fpu_owner = Some(cur_nr);
    }
    fpu.enable(); // C: clts — FPEN=0b11
    if fpu_bkl {
        crate::smp::bkl_unlock();
    }
    PARK_NONE
}

/// Console diagnostics + panic for unreached causes (riscv64 sibling
/// carries the rationale).
#[cfg(target_arch = "aarch64")]
fn aarch64_diag_panic(
    frame: &minix_arch::arm64::trap_stub::AArch64TrapFrame,
    origin: &str,
) -> ! {
    use crate::kmess::{EarlyConsole, KernelConsole as Console};
    let far = aarch64_read_far();
    Console::write_str(origin);
    Console::write_str(" esr=");
    Console::write_hex(aarch64_read_esr());
    Console::write_str(" far=");
    Console::write_hex(far);
    Console::write_str(" elr=");
    Console::write_hex(frame.elr);
    Console::write_str(" spsr=");
    Console::write_hex(frame.spsr);
    Console::write_str("\n");
    panic!(
        "aarch64 {origin}: elr {:#x} spsr {:#x} far {far:#x}",
        frame.elr, frame.spsr
    );
}

#[cfg(target_arch = "aarch64")]
fn aarch64_read_far() -> u64 {
    let v: u64;
    // SAFETY: side-effect-free system-register read at EL1.
    unsafe { core::arch::asm!("mrs {}, far_el1", out(reg) v, options(nomem, nostack)); }
    v
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
        // The LAPIC spurious vector (0xFF) returns WITHOUT
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
    fn irq_entry_mirrors_user_frame_but_skips_kernel_origin() {
        // Regression guard (C: every IRQ/clock stub entry in mpx.S runs
        // SAVE_PROCESS_CTX — sconst.h:75-89): a user-origin tick/IRQ must
        // mirror the full register file (including RBX live values) into the
        // process ctx before any scheduling decision, or a preempted/moved
        // process is restored from a stale ctx by finish_and_restore (on
        // hardware: RS step2's &self.table = 0x0); a kernel-origin interrupt
        // must not mirror (the frame holds kernel registers). Discriminating:
        // removing the user mirror fails the first assertion group; removing
        // the CPL gate fails the second group (stale rip retained).
        let mut table = crate::test_helpers::test_proc_table();
        let proc = table.get_mut(crate::proc::ProcNr(0)).unwrap();
        // Pre-seed the context through the write seam (save with a
        // sentinel frame): the kernel-visible read seams are
        // ipc_status_register (the R10 status lane) /
        // ipc_return_code (RAX) / callee_saved_rbx (RBX)。
        let seed = TrapFrame {
            rax: 7,
            rbx: 0xABCD_0000,
            rcx: 0,
            rdx: 0,
            rsi: 0,
            rdi: 0,
            rbp: 0,
            r8: 0,
            r9: 0,
            r10: 0x1111_0000,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            vector: 0x50,
            errcode: 0,
            rip: 0x203bf0,
            cs: 0x1B, // user CS → RPL3, must mirror
            rflags: 0x202,
            rsp: 0x7FFF_0000,
            ss: 0x23,
        };
        assert!(
            mirror_irq_frame_into_proc(&seed, proc),
            "user-origin IRQ must mirror"
        );
        assert_eq!(
            minix_arch::x86_64::trap_stub::callee_saved_rbx(&proc.cpu_context),
            0xABCD_0000,
            "RBX live value must survive into ctx (callee-saved完整性)"
        );
        assert_eq!(
            minix_arch::x86_64::trap_stub::ipc_status_register(&proc.cpu_context),
            0x1111_0000,
            "R10 must be mirrored (A 案：R10 = IPC 状态车道)"
        );
        assert_eq!(
            minix_arch::x86_64::trap_stub::ipc_return_code(&proc.cpu_context),
            7,
            "RAX must be mirrored"
        );
        assert_eq!(proc.trap_style, TrapStyle::FullContext);
        // Kernel CS + different register values → must NOT touch the ctx.
        let mut kern = seed;
        kern.cs = 0x08;
        kern.rbx = 0;
        kern.rax = 99;
        assert!(
            !mirror_irq_frame_into_proc(&kern, proc),
            "kernel-origin IRQ must skip the mirror"
        );
        assert_eq!(
            minix_arch::x86_64::trap_stub::callee_saved_rbx(&proc.cpu_context),
            0xABCD_0000,
            "kernel-origin must not overwrite RBX"
        );
        assert_eq!(
            minix_arch::x86_64::trap_stub::ipc_return_code(&proc.cpu_context),
            7,
            "kernel-origin must not overwrite RAX"
        );
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
