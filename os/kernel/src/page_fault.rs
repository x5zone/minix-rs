//! Page fault helpers.
//!
//! C: `pagefault()` — exception.c:49-131 (x86) / exception.c:34-115 (earm)
//!
//! This module contains the *portable* parts of the page fault path:
//! - **RTS_PAGEFAULT flag set/clear** (the proc-side state machine that
//!   marks "this process has an unhandled page fault").
//! - **VM notify helper** (builds the `m_pagefault` message that the
//!   assembly trap handler sends to VM).
//!
//! What is NOT here:
//! - The actual delivery of `m_pagefault` to VM (`mini_send`/`async_send`)
//!   is left to the assembly trap handler (arch-abstractions).
//! - PTE walk / `vm_lookup` is in VM (Direct Map).
//!
//! # C source path
//!
//! ```c
//! static void pagefault(struct proc *pr, struct exception_frame *frame,
//!                       int is_nested) {
//!     ...
//!     /* Don't schedule this process until pagefault is handled. */
//!     RTS_SET(pr, RTS_PAGEFAULT);
//!
//!     /* tell Vm about the pagefault */
//!     m_pagefault.m_source = pr->p_endpoint;
//!     m_pagefault.m_type   = VM_PAGEFAULT;
//!     m_pagefault.VPF_ADDR = pagefaultcr2;
//!     m_pagefault.VPF_FLAGS = frame->errcode;
//!     if ((err = mini_send(pr, VM_PROC_NR, &m_pagefault, FROM_KERNEL))) {
//!         panic("WARNING: pagefault: mini_send returned %d\n", err);
//!     }
//! }
//! ```
//!
//! # Rust design
//!
//! The C code mixes "set a flag on a process" with "send a message to VM".
//! In Rust we split these into independent, testable helpers:
//!
//! - [`set_pagefault_pending`] — sets `RTS_PAGEFAULT` and records the
//!   faulting address (in the proc struct) for later inspection.
//! - [`build_vm_pagefault_msg`] — builds the message to send to VM
//!   (returns a `Message` value; the caller does the actual `mini_send`).
//! - [`is_pagefault_pending`] — predicate for use by the scheduler.
//!
//! Each helper has a single responsibility, no side effects beyond what
//! the name implies, and is fully unit-testable.

use minix_types::{Endpoint, Message, VM_PAGEFAULT};

use crate::proc::{KProcess, RtsFlagsBits};

/// Set the `RTS_PAGEFAULT` flag on a process and record the faulting
/// address.
///
/// C: `RTS_SET(pr, RTS_PAGEFAULT)` — exception.c:115.
///
/// After this call, the scheduler must NOT pick this process until the
/// VM (via SYS_VMCTL with param `ClearPageFault`) clears the flag.
///
/// # Arguments
///
/// * `proc` — the process that took the page fault (may be the
///   currently running kernel process, in which case the caller should
///   panic instead — see [`is_kernel_proc_pagefault`]).
/// * `fault_addr` — the faulting virtual address (CR2 on x86, FAR on
///   ARM, stval on RISC-V). Stored for diagnostics; not consulted by
///   the scheduler.
pub fn set_pagefault_pending(proc: &mut KProcess, fault_addr: u64) {
    proc.p_rts_flags.set(RtsFlagsBits::PAGEFAULT);
    proc.p_fault_addr = Some(fault_addr);
}

/// Clear the `RTS_PAGEFAULT` flag on a process.
///
/// C: `RTS_UNSET(p, RTS_PAGEFAULT)` — do_vmctl.c:32-35.
///
/// Called by VM after it has handled the page fault (delivered a fresh
/// PTE, refreshed the TLB, etc.). Returns `true` if the flag was set
/// before the call, `false` if it was already clear (matching C's
/// `assert(RTS_ISSET(p, RTS_PAGEFAULT))` semantics: the assertion
/// translates to "the caller was confused" → error return).
pub fn clear_pagefault_pending(proc: &mut KProcess) -> bool {
    let was_set = proc.p_rts_flags.is_set(RtsFlagsBits::PAGEFAULT);
    proc.p_rts_flags.clear(RtsFlagsBits::PAGEFAULT);
    proc.p_fault_addr = None;
    was_set
}

/// Check whether a process has an unhandled page fault.
///
/// Used by the scheduler (per-CPU scheduling queue) to skip
/// faulting processes until VM clears the flag.
pub fn is_pagefault_pending(proc: &KProcess) -> bool {
    proc.p_rts_flags.is_set(RtsFlagsBits::PAGEFAULT)
}

/// Returns the last-recorded fault address, if any.
pub fn last_fault_addr(proc: &KProcess) -> Option<u64> {
    proc.p_fault_addr
}

/// Determine whether a kernel-mode page fault should panic the kernel.
///
/// C: `if (is_nested) { ... inkernel_disaster(...); }` — exception.c:91.
///
/// A page fault while the kernel is already holding the BKL (nested
/// exception, BKL already acquired) is a kernel bug or unrecoverable
/// hardware fault. The C code prints diagnostics and panics.
///
/// We return a human-readable panic message suitable for
/// `panic!("{}", msg)`. The actual panic call is left to the trap
/// handler so it can include additional context (saved IP, registers).
pub fn kernel_mode_pagefault_panic_msg(
    proc_endpoint: Endpoint,
    fault_addr: u64,
) -> alloc::string::String {
    use alloc::format;
    format!(
        "kernel-mode page fault: endpoint={}, fault_addr=0x{:x} \
         (nested exception while BKL held; \
         this is a kernel bug or unrecoverable hardware fault)",
        proc_endpoint.0, fault_addr
    )
}

/// Build the message to send to VM for a page fault.
///
/// C: `m_pagefault.m_source = ...; m_pagefault.m_type = VM_PAGEFAULT;`
/// — exception.c:119-122.
///
/// Returns a fresh `Message` value with:
/// - `m_source` = the faulting process endpoint
/// - `m_type`   = `VM_PAGEFAULT` (0xCFF = `VM_RQ_BASE + 0xFF`, defined in
///   `minix-types/src/ipc/vm.rs:146`; VM's main loop dispatches on this
///   exact value — see `os/servers/vm/src/vm_server.rs:436`).
/// - `VPF_ADDR` and `VPF_FLAGS` fields populated from the trap frame.
///
/// The caller is responsible for delivering the message (via the
/// architecture-specific trap handler, `mini_send`/`async_send`).
pub fn build_vm_pagefault_msg(
    proc_endpoint: Endpoint,
    fault_addr: u64,
    error_code: u32,
) -> Message {
    // SAFETY: `Message` is a `#[repr(C)]` union; zeroing is the
    // documented "fresh message" pattern in Minix's kernel.
    let mut msg = Message {
        // C: exception.c:119 — `m_pagefault.m_source = pr->p_endpoint`
        m_source: proc_endpoint,
        // C: exception.c:120 — `m_pagefault.m_type = VM_PAGEFAULT`
        //
        // VM_PAGEFAULT is a u32 in minix-types; m_type is i32. The cast is
        // safe because VM_PAGEFAULT (0xCFF = 3327) fits in i32's positive
        // range. VM's main loop checks `m_type == VM_PAGEFAULT` (vm_server.rs:436)
        // — without this assignment, m_type stays 0 (from Message::default)
        // and VM cannot dispatch the page-fault request.
        m_type: VM_PAGEFAULT as i32,
        ..Default::default()
    };
    // C: exception.c:121-122 — `VPF_ADDR = pagefaultcr2; VPF_FLAGS = frame->errcode`
    msg.m_u.m_vm_pagefault.vpf_addr = fault_addr;
    msg.m_u.m_vm_pagefault.vpf_flags = error_code;
    msg
}

extern crate alloc;

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proc::proc_nr::KERNEL;
    use crate::proc::RtsFlagsBits;
    use crate::proc::ProcNr;
    use crate::proc_table::ProcessTable;

    #[test]
    fn test_set_pagefault_pending_sets_flag_and_addr() {
        // C: RTS_SET(pr, RTS_PAGEFAULT) at exception.c:115.
        let mut proc_table = ProcessTable::new();
        let proc = proc_table.get_mut(ProcNr(0)).unwrap();
        proc.p_endpoint = Endpoint(100);
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);

        set_pagefault_pending(proc, 0xdead_beef);
        assert!(is_pagefault_pending(proc));
        assert_eq!(last_fault_addr(proc), Some(0xdead_beef));
    }

    #[test]
    fn test_clear_pagefault_pending_clears_flag_and_addr() {
        let mut proc_table = ProcessTable::new();
        let proc = proc_table.get_mut(ProcNr(0)).unwrap();
        proc.p_endpoint = Endpoint(100);
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        set_pagefault_pending(proc, 0x1234_5678);

        // was_set = true on first clear
        assert!(clear_pagefault_pending(proc));
        assert!(!is_pagefault_pending(proc));
        assert_eq!(last_fault_addr(proc), None);

        // was_set = false on second clear (matches C assert)
        assert!(!clear_pagefault_pending(proc));
    }

    #[test]
    fn test_is_pagefault_pending_default_false() {
        // A freshly-spawned process must not have RTS_PAGEFAULT set.
        let mut proc_table = ProcessTable::new();
        let proc = proc_table.get_mut(ProcNr(0)).unwrap();
        proc.p_endpoint = Endpoint(100);
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        assert!(!is_pagefault_pending(proc));
        assert_eq!(last_fault_addr(proc), None);
    }

    #[test]
    fn test_build_vm_pagefault_msg_populates_fields() {
        // C: exception.c:119-122 — m_source / m_type / VPF_ADDR / VPF_FLAGS.
        let msg = build_vm_pagefault_msg(Endpoint(42), 0xcafe_f00d, 0x07);
        assert_eq!(msg.m_source, Endpoint(42));
        // m_type must be VM_PAGEFAULT — VM's main loop dispatches on this
        // exact value (vm_server.rs:436). A default m_type=0 would cause
        // VM to silently drop the page-fault request.
        assert_eq!(msg.m_type, VM_PAGEFAULT as i32);
        // SAFETY: `m_vm_pagefault` is the active union arm; we just wrote
        // it via `build_vm_pagefault_msg`.
        let pf = unsafe { msg.m_u.m_vm_pagefault };
        assert_eq!(pf.vpf_addr, 0xcafe_f00d);
        assert_eq!(pf.vpf_flags, 0x07);
    }

    #[test]
    fn test_kernel_mode_pagefault_panic_msg_includes_endpoint_and_addr() {
        // The panic message should include the faulting endpoint and
        // address so post-mortem analysis can identify the cause.
        let msg = kernel_mode_pagefault_panic_msg(Endpoint(7), 0xffff_8000_0000_0000);
        assert!(msg.contains("7"));
        assert!(msg.contains("ffff800000000000") || msg.contains("0xffff"));
    }

    #[test]
    fn test_pagefault_pending_roundtrip_with_clear() {
        // End-to-end: set → query → clear → query (false).
        let mut proc_table = ProcessTable::new();
        let proc = proc_table.get_mut(ProcNr(0)).unwrap();
        proc.p_endpoint = Endpoint(100);
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);

        set_pagefault_pending(proc, 0x4000_0000);
        assert!(is_pagefault_pending(proc));
        clear_pagefault_pending(proc);
        assert!(!is_pagefault_pending(proc));
    }

    #[test]
    fn test_pagefault_pending_idempotent_set() {
        // Calling set twice should be idempotent (RTS_PAGEFAULT is a
        // single bit; setting it again is a no-op).
        let mut proc_table = ProcessTable::new();
        let proc = proc_table.get_mut(ProcNr(0)).unwrap();
        proc.p_endpoint = Endpoint(100);
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        set_pagefault_pending(proc, 0x1000);
        set_pagefault_pending(proc, 0x2000);
        // The addr field is overwritten (last writer wins).
        assert_eq!(last_fault_addr(proc), Some(0x2000));
        assert!(is_pagefault_pending(proc));
    }

    #[test]
    fn test_pagefault_skips_kernel_proc_for_set() {
        // The trap handler should never call set_pagefault_pending on
        // a kernel process — kernel page faults panic via
        // kernel_mode_pagefault_panic_msg. This test documents the
        // expected caller-side contract: callers must dispatch on
        // is_kernelp first.
        let mut proc_table = ProcessTable::new();
        let kernel_proc = proc_table.get_mut(KERNEL).unwrap();
        kernel_proc.p_endpoint = Endpoint(0); // KERNEL's endpoint
        kernel_proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);

        // We do NOT call set_pagefault_pending here; instead we verify
        // that kernel_mode_pagefault_panic_msg produces a useful panic
        // message that the trap handler can use.
        let panic_msg = kernel_mode_pagefault_panic_msg(
            kernel_proc.p_endpoint,
            0xdead_beef,
        );
        assert!(panic_msg.contains("kernel-mode"));
        assert!(panic_msg.contains("0xdeadbeef"));
    }
}