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

/// Failure of a kernel call made through the gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GatewayError {
    /// The kernel answered a negative errno (pre-E1 the trap stub answers
    /// `-EIO`). The raw code is preserved for audit output.
    Kernel(i32),
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
    fn sys_fork(&mut self, parent: Endpoint, child_slot: UserSlot) -> Result<Endpoint, GatewayError>;

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

    /// Write a diagnostic string through SYS_DIAGCTL code 1 (C:
    /// do_diagctl.c:28-44 — the kernel data_copy's up to DIAGBUFSIZE=128
    /// bytes from the caller's buffer and kputc's them to the console;
    /// kernel/src/syscall.rs:2281+, m_type = Syscall::Diagctl = 44).
    /// Callers chunk longer text at 128 bytes (audit.rs).
    fn diag_write(&mut self, text: &str) -> Result<(), GatewayError>;

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

/// C: SYS_FORK is kernel call number 0 in minix-rs
/// (kernel/src/syscall.rs:66 `Syscall::Fork = 0`, decoded at :125).
/// C spells it `KERNEL_CALL + 1`; the numeric convention of this codebase
/// is the small non-negative kernel-call space decoded by `Syscall::try_from`.
const SYS_FORK_CALL: i32 = 0;

impl<T: KernelCallTransport> KernelGateway for TrapKernelGateway<T> {
    fn sys_fork(&mut self, parent: Endpoint, child_slot: UserSlot) -> Result<Endpoint, GatewayError> {
        let mut msg = Message::default();
        {
            // SAFETY: M1 fields are the documented SYS_FORK wire layout
            // (kernel/src/syscall_process.rs:139-141 reads m1i1/m1i2/m1i3).
            let m1 = unsafe { &mut msg.m_u.m_m1 };
            m1.m1i1 = parent.0;
            m1.m1i2 = child_slot.get() as i32;
            m1.m1i3 = 0; // C: do_fork flags — none used by VM's fork path
        }
        // C: `_kernel_call(SYS_FORK, &m)` — ENOTREADY retry loop lives in
        // `perform_kernel_call` (kernel_call.c:7-21). The tick-delay
        // callback is a no-op until a user-space tickdelay primitive lands
        // with E1; pre-E1 the transport answers -EIO immediately so the
        // delay path is never exercised.
        let reply = perform_kernel_call(&self.transport, SYS_FORK_CALL, &mut msg, |_| {});
        if reply < 0 {
            return Err(GatewayError::Kernel(reply));
        }
        // C: do_fork.c:111 — the child endpoint comes back in the reply's
        // result slot (m_type), assigned by the kernel to the child slot.
        Ok(Endpoint(reply))
    }

    fn diag_write(&mut self, text: &str) -> Result<(), GatewayError> {
        // m_type is written by perform_kernel_call (the call number).
        let mut msg = Message::default();
        {
            // SAFETY: documented SYS_DIAGCTL wire — the kernel dispatch
            // reads code/len/buf (kernel/src/syscall.rs:2285-2292).
            let d = unsafe { &mut msg.m_u.m_lsys_krn_sys_diagctl };
            d.code = 1; // DIAGCTL_CODE_DIAG
            d.buf = text.as_ptr() as u64;
            d.len = text.len() as u64;
        }
        let reply = perform_kernel_call(&self.transport, SYS_DIAGCTL_CALL, &mut msg, |_| {});
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
        let mut msg = Message::default();
        {
            // SAFETY: documented SYS_EXEC wire — the kernel dispatch reads
            // endpt/ip/stack/name/ps_str (kernel/src/syscall_process.rs:240).
            let e = unsafe { &mut msg.m_u.m_lsys_krn_sys_exec };
            e.endpt = endpt.get();
            e.ip = ip;
            e.stack = stack;
            e.name = name_ptr;
            e.ps_str = ps_str;
        }
        let reply = perform_kernel_call(&self.transport, SYS_EXEC_CALL, &mut msg, |_| {});
        if reply < 0 {
            return Err(GatewayError::Kernel(reply));
        }
        Ok(())
    }
}

/// C: SYS_DIAGCTL is kernel call 44 (kernel/src/syscall.rs:106).
const SYS_DIAGCTL_CALL: i32 = 44;
/// C: SYS_EXEC kernel call number — kernel/src/syscall.rs `Syscall::Exec`
/// (decoded by `Syscall::try_from`; grep the enum for the current value).
const SYS_EXEC_CALL: i32 = 1;

#[cfg(test)]
/// Scripted gateway for unit tests: records `sys_fork` inputs and answers
/// either a configured endpoint (success) or a gateway error.
pub(crate) struct MockGateway {
    /// Endpoint returned on the next `sys_fork`.
    pub fork_reply: Result<Endpoint, GatewayError>,
    /// Last (parent, child_slot) seen, for call-shape assertions.
    pub last_fork: core::cell::Cell<Option<(Endpoint, UserSlot)>>,
    /// Diagnostic text recorded by `diag_write` (V11/T15).
    pub diag_log: RefCell<alloc::string::String>,
    /// Last `sys_exec` seen: (endpt, ip, stack, ps_str) (V11/T14).
    pub last_exec: Cell<Option<(Endpoint, u64, u64, u64)>>,
}

#[cfg(test)]
impl MockGateway {
    pub(crate) fn new() -> Self {
        Self {
            fork_reply: Err(GatewayError::Kernel(-minix_types::EIO)),
            last_fork: core::cell::Cell::new(None),
            diag_log: RefCell::new(alloc::string::String::new()),
            last_exec: Cell::new(None),
        }
    }
}

#[cfg(test)]
impl KernelGateway for MockGateway {
    fn sys_fork(&mut self, parent: Endpoint, child_slot: UserSlot) -> Result<Endpoint, GatewayError> {
        self.last_fork.set(Some((parent, child_slot)));
        self.fork_reply
    }

    fn diag_write(&mut self, text: &str) -> Result<(), GatewayError> {
        self.diag_log.borrow_mut().push_str(text);
        Ok(())
    }

    #[cfg(test)]
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
}


#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::UserSlot;

    /// V11/T9: the SYS_FORK wire — call number 0, reply m_type = child
    /// endpoint (kernel syscall_process.rs:210-215 → reply_code()).
    #[test]
    fn test_trap_gateway_sys_fork_wire() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(77); // kernel answers: child endpoint = 77
        let mut g = TrapKernelGateway { transport: canned };
        let ep = g.sys_fork(Endpoint(10), UserSlot::new(3)).unwrap();
        assert_eq!(ep, Endpoint(77));
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
}
