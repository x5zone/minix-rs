//! Kernel-call gateway — the VM's single exit point for kernel syscalls.
//!
//! # Motivation
//!
//! Before this module, kernel calls were either `unimplemented!()`-adjacent
//! stubs (fork.rs `sys_fork` returned a *fabricated* endpoint — a fake
//! success that would corrupt real state on hardware) or scattered plans.
//! The gateway makes the dependency explicit and testable: handlers program
//! against the [`KernelGateway`] trait, tests inject a scripted mock, and
//! the production impl speaks the real kernel-call wire.
//!
//! # Wire convention (kernel calls)
//!
//! C: `_kernel_call(SYS_x, &m)` writes the call number into `m_type`, traps,
//! and on success the kernel places a non-negative result in the reply's
//! `m_type` (negative = errno). minix-rs is identical:
//! `perform_kernel_call` (minix-sys syscall.rs:201) runs the ENOTREADY
//! retry loop of `kernel_call.c:7-21`, and the kernel's kcall dispatch
//! returns the result through `KcallResult::Ok(ret)` → `reply_code()`
//! (kernel/src/syscall.rs:219-222).
//!
//! # Status (V11/T9 step 2)
//!
//! - The message construction and reply decoding here are **final**.
//! - The trap instruction sequences themselves are edge **E1/E2**
//!   (minix-sys `DirectKernelCallTransport` answers `-EIO` until then);
//!   pre-E1 every call fails with `GatewayError::Kernel(-EIO)`, which
//!   handlers map to honest error replies — never fake successes.

use minix_sys::syscall::{perform_kernel_call, KernelCallTransport};
#[cfg(test)]
use minix_sys::syscall::{CannedKernelCallTransport, DirectKernelCallTransport};
#[cfg(test)]
use core::cell::{Cell, RefCell};
use minix_types::{Endpoint, Message, UserSlot};
use minix_sys::syscall::sys_kill as minix_sys_kill;

/// Failure of a kernel call made through the gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GatewayError {
    /// The kernel answered a negative errno (pre-E1 the trap stub answers
    /// `-EIO`). The raw code is preserved for audit output.
    Kernel(i32),
}

/// One fetched kernel memory request.
///
/// Reply payload of SYS_VMCTL `VMCTL_MEMREQ_GET` (kernel syscall.rs fills
/// `SVMCTL_MRG_*` M1 fields; C: `sys_vmctl_get_memreq` out-params
/// `(&who, &mem, &len, &wrflag, &who_s, &mem_s, &requestor)` — minix-rs
/// narrows to the fields the CHECK path consumes, kernel vm.rs §3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))] // consumed via handle_signal (wired at E1)
pub(crate) struct KernelMemReq {
    /// Process whose address space the kernel asks about (`SVMCTL_MRG_TARGET`).
    pub target: Endpoint,
    /// Range start (page-aligned virtual address, `SVMCTL_MRG_ADDR`).
    pub start: u64,
    /// Range length in bytes (`SVMCTL_MRG_LENGTH`).
    pub length: u64,
    /// Write access requested (`SVMCTL_MRG_FLAG`).
    pub write: bool,
    /// Originating process (kernel/driver; `SVMCTL_MRG_REQUESTOR`).
    pub requestor: Endpoint,
}

/// The VM's view of the kernel syscalls it consumes. Grows per consumer:
/// `sys_fork` (fork.rs) today; safecopy (rs handshake), `sys_update`
/// (RS live update), `sys_exec` (boot proc) and diag output (audit) join
/// in their own iterations — each backed by edge E2's SYS_* wrappers.
pub(crate) trait KernelGateway {
    /// Notify the kernel to create the child scheduling entity.
    ///
    /// C: `sys_fork()` (libsys) → kernel `do_fork()`
    /// (kernel/src/syscall_process.rs:122-215). Wire (M1): `m1i1` = parent
    /// endpoint, `m1i2` = child slot, `m1i3` = flags (0). Reply: `m_type` =
    /// child endpoint on success, negative errno on failure
    /// (KcallResult::Ok(child_endpoint.0) → reply_code()).
    ///
    /// The second tuple element is C's fifth `sys_fork` output — the
    /// deliver-message buffer address (`msgaddr`, fork.c:90, sourced from
    /// `p_delivermsg_vir`) that do_fork eager-CoWs for parent and child
    /// (fork.c:101-108). E-FORKMSG: the kernel writes it in place into
    /// the reply (`m_krn_lsys_sys_fork.msgaddr`, do_fork.c:112), so the
    /// trap implementation returns `Some` from the reply arm; tests may
    /// still script `None` to drive the skip path.
    fn sys_fork(&mut self, parent: Endpoint, child_slot: UserSlot)
        -> Result<(Endpoint, Option<u64>), GatewayError>;

    /// Notify the kernel of a process's new image (entry point + stack).
    ///
    /// C: `sys_exec` (libsys) → kernel `do_exec` (kernel/src/syscall_process.rs:231):
    /// wire is `m_lsys_krn_sys_exec { endpt, ip, stack, name, ps_str }`
    /// (m_type = Syscall::Exec); `name` is a pointer into the *caller's*
    /// address space (the kernel data_copy's it, do_exec.c:37-43).
    fn sys_exec(
        &mut self,
        endpt: Endpoint,
        ip: u64,
        stack: u64,
        name_ptr: u64,
        ps_str: u64,
    ) -> Result<(), GatewayError>;

    /// Ask the kernel to swap the scheduling identity of two processes
    /// (live-update step 4; VM-side bookkeeping follows).
    ///
    /// C: `sys_update(src_e, dst_e, flags)` (libsys) → kernel `do_update`
    /// (kernel/src/misc.rs dispatch_update — 12-step swap, all implemented).
    /// Wire (M1): `m1i1` = src endpoint, `m1i2` = dst endpoint, `m1i3` =
    /// flags (SYS_UPD_ROLLBACK bit). Reply: OK(0) or negative errno.
    fn sys_update(&mut self, src: Endpoint, dst: Endpoint, flags: u32)
        -> Result<(), GatewayError>;

    /// Write a diagnostic string through SYS_DIAGCTL code 1 (C:
    /// do_diagctl.c:28-44 — the kernel data_copy's up to DIAGBUFSIZE=128
    /// bytes from the caller's buffer and kputc's them to the console;
    /// kernel/src/syscall.rs:2281+, m_type = Syscall::Diagctl = 44).
    /// Callers chunk longer text at 128 bytes (audit.rs).
    fn diag_write(&mut self, text: &str) -> Result<(), GatewayError>;

    /// Fetch the next pending kernel memory request.
    ///
    /// C: `sys_vmctl_get_memreq()` → kernel `VMCTL_MEMREQ_GET` (= 14,
    /// kernel/src/vm.rs VmCtlParam; reply fills `SVMCTL_MRG_*`). `Ok(None)`
    /// = queue empty (kernel answers ENOENT = 2); `Ok(Some)` = a
    /// VMPTYPE_CHECK request (the only type minix-rs queues).
    fn sys_vmctl_memreq_get(&mut self)
        -> Result<Option<KernelMemReq>, GatewayError>;

    /// Reply to the previously fetched memory request.
    ///
    /// C: `sys_vmctl_memreq_reply(target, result)` → kernel
    /// `VMCTL_MEMREQ_REPLY` (= 15); `ok == false` reports EFAULT — the
    /// kernel's `check_resumed_caller` only distinguishes vmresult != OK
    /// (kernel vm.rs VmCheckResult).
    fn sys_vmctl_memreq_reply(&mut self, target: Endpoint, ok: bool)
        -> Result<(), GatewayError>;

    /// Deliver a signal to a process.
    ///
    /// C: `sys_kill(endpoint, sig)` (libsys) → kernel `do_kill`
    /// (kernel/src/syscall_signal.rs). VM's use: SIGSEGV delivery when a
    /// page fault is not servable (G-V12-6; C pagefaults.c:109-119).
    fn sys_kill(&mut self, endpoint: Endpoint, signal: i32) -> Result<(), GatewayError>;

    /// Clear the kernel's pagefault suspension (RTS_PAGEFAULT) on a process.
    ///
    /// C: `sys_vmctl(ep, VMCTL_CLEAR_PAGEFAULT)` — after a pagefault is
    /// disposed of (served or SIGSEGV'd), the process must be un-suspended.
    fn sys_vmctl_clear_pagefault(&mut self, endpoint: Endpoint) -> Result<(), GatewayError>;

    /// Test/diagnostic accessor: concatenated diag text (default empty;
    /// `MockGateway` returns what `diag_write` recorded).
    #[cfg(test)]
    fn diag_log(&self) -> alloc::string::String {
        alloc::string::String::new()
    }
}

/// Production gateway: kernel calls through the minix-sys kernel-call
/// transport (`DirectKernelCallTransport`; pre-E1 it answers `-EIO`).
/// Generic over the transport so tests can script replies with
/// `CannedKernelCallTransport`.
pub(crate) struct TrapKernelGateway<T: KernelCallTransport> {
    pub(crate) transport: T,
}

impl<T: KernelCallTransport> KernelGateway for TrapKernelGateway<T> {
    fn sys_fork(&mut self, parent: Endpoint, child_slot: UserSlot) -> Result<(Endpoint, Option<u64>), GatewayError> {
        // E2: consume the minix-sys wrapper — the request arm construction
        // and the E-FORKMSG reply-arm parsing (endpt + msgaddr, C
        // do_fork.c:111-112) live in one place now.
        minix_sys::syscall::sys_fork(&self.transport, parent.0, child_slot.get() as i32, 0)
            .map(|(endpt, msgaddr)| (Endpoint(endpt), Some(msgaddr)))
            .map_err(GatewayError::Kernel)
    }

    fn diag_write(&mut self, text: &str) -> Result<(), GatewayError> {
        // E2: consume the minix-sys wrapper (DIAGCTL_CODE_DIAG = 1; the
        // kernel copies the buffer out of this process's address space).
        minix_sys::syscall::sys_diagctl(
            &self.transport,
            1, // DIAGCTL_CODE_DIAG
            text.as_ptr() as u64,
            text.len() as u64,
        )
        .map_err(GatewayError::Kernel)
    }

    fn sys_vmctl_memreq_get(&mut self)
        -> Result<Option<KernelMemReq>, GatewayError>
    {
        let mut msg = Message::default();
        {
            // SAFETY: SYS_VMCTL wire — kernel dispatch_vmctl reads
            // SVMCTL_WHO (m1i1), SVMCTL_PARAM (m1i2), SVMCTL_VALUE (m1i3)
            // (kernel/src/syscall.rs:1936-1941).
            let m1 = unsafe { &mut msg.m_u.m_m1 };
            m1.m1i1 = Endpoint::VM.0;
            m1.m1i2 = VMCTL_MEMREQ_GET;
            m1.m1i3 = 0;
        }
        let reply = perform_kernel_call(&self.transport, SYS_VMCTL_CALL, &mut msg, |_| {});
        if reply < 0 {
            return Err(GatewayError::Kernel(reply));
        }
        // Kernel signals "no pending request" by returning ENOENT = 2 as
        // the result value (kernel/src/syscall.rs: VMCTL_MEMREQ_GET arm).
        if reply == minix_types::ENOENT {
            return Ok(None);
        }
        // Reply fields land in the same message (kernel fills SVMCTL_MRG_*:
        // m1i1 = target, m1p1 = addr, m1p2 = length, m1i3 = write flag,
        // m1p3 = requestor; kernel/src/syscall.rs:2010-2023).
        let m1 = unsafe { &msg.m_u.m_m1 };
        Ok(Some(KernelMemReq {
            target: Endpoint(m1.m1i1),
            start: m1.m1p1,
            length: m1.m1p2,
            write: m1.m1i3 != 0,
            requestor: Endpoint(m1.m1p3 as i32),
        }))
    }

    fn sys_vmctl_memreq_reply(&mut self, target: Endpoint, ok: bool)
        -> Result<(), GatewayError>
    {
        let mut msg = Message::default();
        {
            // SAFETY: SYS_VMCTL wire — SVMCTL_WHO carries the target the
            // request was fetched for, SVMCTL_VALUE the check result
            // (0 = OK, anything else = fault; kernel vm.rs VmCheckResult).
            let m1 = unsafe { &mut msg.m_u.m_m1 };
            m1.m1i1 = target.0;
            m1.m1i2 = VMCTL_MEMREQ_REPLY;
            m1.m1i3 = if ok { 0 } else { 1 };
        }
        let reply = perform_kernel_call(&self.transport, SYS_VMCTL_CALL, &mut msg, |_| {});
        if reply < 0 {
            return Err(GatewayError::Kernel(reply));
        }
        Ok(())
    }

    fn sys_kill(&mut self, endpoint: Endpoint, signal: i32) -> Result<(), GatewayError> {
        // V11/T35+: reuse minix-sys's SYS_KILL wrapper (same wire the PM
        // side uses — kernel/src/syscall_signal.rs:141-147 m_sigcalls).
        let reply = minix_sys_kill(&self.transport, endpoint.0, signal);
        if reply < 0 {
            return Err(GatewayError::Kernel(reply));
        }
        Ok(())
    }

    fn sys_vmctl_clear_pagefault(&mut self, endpoint: Endpoint) -> Result<(), GatewayError> {
        let mut msg = Message::default();
        {
            // SAFETY: SYS_VMCTL wire — SVMCTL_WHO (m1i1) target,
            // SVMCTL_PARAM (m1i2) = VMCTL_CLEAR_PAGEFAULT (12).
            let m1 = unsafe { &mut msg.m_u.m_m1 };
            m1.m1i1 = endpoint.0;
            m1.m1i2 = VMCTL_CLEAR_PAGEFAULT;
            m1.m1i3 = 0;
        }
        let reply = perform_kernel_call(&self.transport, SYS_VMCTL_CALL, &mut msg, |_| {});
        if reply < 0 {
            return Err(GatewayError::Kernel(reply));
        }
        Ok(())
    }

    fn sys_exec(
        &mut self,
        endpt: Endpoint,
        ip: u64,
        stack: u64,
        name_ptr: u64,
        ps_str: u64,
    ) -> Result<(), GatewayError> {
        // E2: consume the minix-sys wrapper (documented SYS_EXEC wire —
        // kernel dispatch_exec reads endpt/ip/stack/name/ps_str).
        minix_sys::syscall::sys_exec(&self.transport, endpt.get(), ip, stack, name_ptr, ps_str)
            .map_err(GatewayError::Kernel)
    }

    fn sys_update(&mut self, src: Endpoint, dst: Endpoint, flags: u32)
        -> Result<(), GatewayError>
    {
        // E2: consume the minix-sys wrapper (SYS_UPDATE is M1 by C design —
        // do_update.c:9-11; kernel dispatch_update reads m1i1/m1i2/m1i3).
        minix_sys::syscall::sys_update(&self.transport, src.get(), dst.get(), flags as i32)
            .map_err(GatewayError::Kernel)
    }
}

/// SYS_VMCTL kernel-call number (kernel/src/syscall.rs:105 `Syscall::Vmctl = 43`).
#[cfg(test)]
pub(crate) const SYS_VMCTL_CALL: i32 = 43;
#[cfg(not(test))]
const SYS_VMCTL_CALL: i32 = 43;
/// SVMCTL_PARAM sub-commands (kernel/src/vm.rs VmCtlParam TryFrom: 14/15;
/// C com.h VMCTL_MEMREQ_GET / VMCTL_MEMREQ_REPLY).
const VMCTL_MEMREQ_GET: i32 = 14;
const VMCTL_MEMREQ_REPLY: i32 = 15;
/// SVMCTL_PARAM sub-command: clear RTS_PAGEFAULT on the target
/// (kernel/src/vm.rs VmCtlParam::ClearPageFault = 12; C VMCTL_CLEAR_PAGEFAULT).
const VMCTL_CLEAR_PAGEFAULT: i32 = 12;
#[cfg(test)]
/// Scripted gateway for unit tests: records `sys_fork` inputs and answers
/// either a configured endpoint (success) or a gateway error.
pub(crate) struct MockGateway {
    /// Endpoint returned on the next `sys_fork`.
    pub fork_reply: Result<Endpoint, GatewayError>,
    /// Last (parent, child_slot) seen, for call-shape assertions.
    pub last_fork: core::cell::Cell<Option<(Endpoint, UserSlot)>>,
    /// msgaddr handed back by `sys_fork` (V11/T33; `None` = pre-E-FORKMSG).
    pub fork_msgaddr: core::cell::Cell<Option<u64>>,
    /// Diagnostic text recorded by `diag_write` (V11/T15).
    pub diag_log: RefCell<alloc::string::String>,
    /// Last `sys_exec` seen: (endpt, ip, stack, ps_str) (V11/T14).
    pub last_exec: Cell<Option<(Endpoint, u64, u64, u64)>>,
    /// Reply m_type for the next `sys_update` (0 = OK) (V11/T13).
    pub update_reply: Cell<i32>,
    /// Last (src, dst, flags) seen by `sys_update` (V11/T13).
    pub last_update: Cell<Option<(Endpoint, Endpoint, u32)>>,
    /// Pending kernel memory requests for `sys_vmctl_memreq_get` (V11/T29).
    pub pending_memreqs: RefCell<alloc::collections::VecDeque<KernelMemReq>>,
    /// (target, ok) pairs recorded by `sys_vmctl_memreq_reply` (V11/T29).
    pub memreq_replies: RefCell<alloc::vec::Vec<(Endpoint, bool)>>,
    /// When non-zero, `sys_vmctl_memreq_get` answers this errno (V11/T29).
    pub memreq_error: Cell<i32>,
    /// (endpoint, signal) pairs recorded by `sys_kill` (V11/T35).
    pub kills: RefCell<alloc::vec::Vec<(Endpoint, i32)>>,
    /// Endpoints recorded by `sys_vmctl_clear_pagefault` (V11/T35).
    pub clear_pagefaults: RefCell<alloc::vec::Vec<Endpoint>>,
}

#[cfg(test)]
impl MockGateway {
    pub(crate) fn new() -> Self {
        Self {
            fork_reply: Err(GatewayError::Kernel(-minix_types::EIO)),
            last_fork: core::cell::Cell::new(None),
            fork_msgaddr: core::cell::Cell::new(None),
            diag_log: RefCell::new(alloc::string::String::new()),
            last_exec: Cell::new(None),
            update_reply: Cell::new(0),
            last_update: Cell::new(None),
            pending_memreqs: RefCell::new(alloc::collections::VecDeque::new()),
            memreq_replies: RefCell::new(alloc::vec::Vec::new()),
            memreq_error: Cell::new(0),
            kills: RefCell::new(alloc::vec::Vec::new()),
            clear_pagefaults: RefCell::new(alloc::vec::Vec::new()),
        }
    }
}

#[cfg(test)]
impl KernelGateway for MockGateway {
    fn sys_fork(&mut self, parent: Endpoint, child_slot: UserSlot)
        -> Result<(Endpoint, Option<u64>), GatewayError>
    {
        self.last_fork.set(Some((parent, child_slot)));
        // V11/T33: hand back the scripted msgaddr so tests can drive the
        // eager-CoW phase end to end (None = pre-E-FORKMSG shape).
        self.fork_reply.clone().map(|ep| (ep, self.fork_msgaddr.get()))
    }

    fn diag_write(&mut self, text: &str) -> Result<(), GatewayError> {
        self.diag_log.borrow_mut().push_str(text);
        Ok(())
    }

    #[cfg(test)]
    fn sys_vmctl_memreq_get(&mut self)
        -> Result<Option<KernelMemReq>, GatewayError>
    {
        let err = self.memreq_error.get();
        if err != 0 {
            return Err(GatewayError::Kernel(err));
        }
        Ok(self.pending_memreqs.borrow_mut().pop_front())
    }

    fn sys_vmctl_memreq_reply(&mut self, target: Endpoint, ok: bool)
        -> Result<(), GatewayError>
    {
        self.memreq_replies.borrow_mut().push((target, ok));
        Ok(())
    }
    fn sys_kill(&mut self, endpoint: Endpoint, signal: i32) -> Result<(), GatewayError> {
        self.kills.borrow_mut().push((endpoint, signal));
        Ok(())
    }

    fn sys_vmctl_clear_pagefault(&mut self, endpoint: Endpoint) -> Result<(), GatewayError> {
        self.clear_pagefaults.borrow_mut().push(endpoint);
        Ok(())
    }

    fn diag_log(&self) -> alloc::string::String {
        self.diag_log.borrow().clone()
    }

    fn sys_exec(
        &mut self,
        endpt: Endpoint,
        ip: u64,
        stack: u64,
        _name_ptr: u64,
        ps_str: u64,
    ) -> Result<(), GatewayError> {
        self.last_exec.set(Some((endpt, ip, stack, ps_str)));
        Ok(())
    }

    fn sys_update(&mut self, src: Endpoint, dst: Endpoint, flags: u32)
        -> Result<(), GatewayError>
    {
        self.last_update.set(Some((src, dst, flags)));
        let r = self.update_reply.get();
        if r < 0 {
            Err(GatewayError::Kernel(r))
        } else {
            Ok(())
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::UserSlot;

    /// E-FORKMSG: the SYS_FORK wire — the request rides the dedicated
    /// `m_lsys_krn_sys_fork` arm (endpt/slot/flags), the kernel answers
    /// OK(0) with the reply fields written in place
    /// (`m_krn_lsys_sys_fork.{endpt,msgaddr}`, C do_fork.c:111-112), and
    /// the gateway returns both — the msgaddr enables do_fork's
    /// eager-CoW phase (fork.c:100-108).
    #[test]
    fn test_trap_gateway_sys_fork_wire() {
        let mut canned = CannedKernelCallTransport::new();
        // Kernel-shaped reply: OK(0) + in-place out-params.
        let mut reply = Message::default();
        reply.m_type = 0;
        // SAFETY(test): constructing the reply arm directly.
        let arm = unsafe { &mut reply.m_u.m_krn_lsys_sys_fork };
        arm.endpt = 77;
        arm.msgaddr = 0x7000;
        canned.reply_message(reply);
        let mut g = TrapKernelGateway { transport: canned };
        let (ep, msgaddr) = g.sys_fork(Endpoint(10), UserSlot::new(3)).unwrap();
        assert_eq!(ep, Endpoint(77));
        assert_eq!(msgaddr, Some(0x7000));
    }

    /// V11/T14: the SYS_EXEC wire — call number 1, `MessLsysKrnSysExec`
    /// {endpt, ip, stack, name, ps_str}; kernel replies OK(0).
    #[test]
    fn test_trap_gateway_sys_exec_wire() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0); // kernel answers OK
        let mut g = TrapKernelGateway { transport: canned };
        g.sys_exec(Endpoint(10), 0x40_1000, 0x7FFF_F000, 0xdead_beef, 0x7FFF_E000)
            .unwrap();
    }

    /// Pre-E1 the direct trap transport answers -EIO: the gateway surfaces
    /// `GatewayError::Kernel(-EIO)` — honest failure, no fabricated endpoint.
    #[test]
    fn test_trap_gateway_sys_fork_error_pre_e1() {
        let mut g = TrapKernelGateway {
            transport: DirectKernelCallTransport,
        };
        assert_eq!(
            g.sys_fork(Endpoint(10), UserSlot::new(3)),
            Err(GatewayError::Kernel(-minix_types::EIO))
        );
    }
    #[test]
    fn test_trap_gateway_memreq_get_wire() {
        // SYS_VMCTL VMCTL_MEMREQ_GET: request wire (m1i1 = SVMCTL_WHO,
        // m1i2 = 14) + reply payload parse (SVMCTL_MRG_* fields).
        let mut canned = CannedKernelCallTransport::new();
        let mut check = Message::default();
        check.m_type = 1; // VMPTYPE_CHECK
        {
            // SAFETY: reply fields land in M1 (kernel syscall.rs:2010-2023).
            let m1 = unsafe { &mut check.m_u.m_m1 };
            m1.m1i1 = 70;                 // SVMCTL_MRG_TARGET
            m1.m1p1 = 0x3000_0000;        // SVMCTL_MRG_ADDR
            m1.m1p2 = 0x2000;             // SVMCTL_MRG_LENGTH
            m1.m1i3 = 1;                  // SVMCTL_MRG_FLAG (write)
            m1.m1p3 = 0;                  // SVMCTL_MRG_REQUESTOR (kernel)
        }
        canned.reply_message(check);
        canned.reply(minix_types::ENOENT); // second GET: queue empty

        let mut gw = crate::kernel_gateway::TrapKernelGateway { transport: canned };
        let first = gw.sys_vmctl_memreq_get().unwrap();
        assert_eq!(first, Some(KernelMemReq {
            target: Endpoint(70),
            start: 0x3000_0000,
            length: 0x2000,
            write: true,
            requestor: Endpoint(0),
        }));
        assert_eq!(gw.sys_vmctl_memreq_get().unwrap(), None, "ENOENT = empty queue");

        let sent = gw.transport.sent.borrow();
        assert_eq!(sent[0].m_type, SYS_VMCTL_CALL);
        let req = unsafe { &sent[0].m_u.m_m1 };
        assert_eq!(req.m1i2, VMCTL_MEMREQ_GET);
        assert_eq!(req.m1i1, Endpoint::VM.0);
    }

    #[test]
    fn test_trap_gateway_memreq_reply_wire() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0); // kernel answers OK

        let mut gw = crate::kernel_gateway::TrapKernelGateway { transport: canned };
        gw.sys_vmctl_memreq_reply(Endpoint(70), false).unwrap();

        let sent = gw.transport.sent.borrow();
        assert_eq!(sent[0].m_type, SYS_VMCTL_CALL);
        let req = unsafe { &sent[0].m_u.m_m1 };
        assert_eq!(req.m1i1, 70, "SVMCTL_WHO = target endpoint");
        assert_eq!(req.m1i2, VMCTL_MEMREQ_REPLY);
        assert_eq!(req.m1i3, 1, "false verdict = fault (VmCheckResult::Fault)");
    }
}
