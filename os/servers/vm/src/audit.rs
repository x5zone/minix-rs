//! Feature-gated audit sink for the VM server.
//!
//! `vm_acl_audit` builds the audit call sites into non-test `no_std`
//! builds. There is no stdout/serial backend yet (VM ↔ syslog IPC is
//! pending, see 15-ipc-dispatch.md / 25-rs-services.md), so the sink
//! formats the message and drops it at this single point — the call
//! sites stay live (no dead-code drift) and the wiring point for the
//! future log channel is explicit.
//!
//! Test builds bypass this module entirely: `audit_log!` expands to
//! `std::eprintln!` under `#[cfg(test)]`.

/// Emit an audit record. Formatting the arguments here keeps the audit
/// expression alive; output routing is pending the kernel IPC core.
#[cfg(all(not(test), feature = "vm_acl_audit"))]
pub(crate) fn emit(_args: core::fmt::Arguments<'_>) {
    // TODO(P1-3): forward to syslog IPC / serial console once the kernel
    // IPC core lands; until then the audit channel is a no-op sink so
    // `vm_acl_audit` builds are valid `no_std` (V10-P0-1).
}
