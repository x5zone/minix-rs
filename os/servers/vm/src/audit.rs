//! Feature-gated audit sink for the VM server.
//!
//! `vm_acl_audit` builds the audit call sites into non-test `no_std`
//! builds. Routing (V11/T15): `emit` forwards the formatted record to the
//! registered [`KernelGateway`] via `SYS_DIAGCTL` code 1 (`diag_write`) —
//! the kernel data_copy's up to 128 bytes per call (DIAGBUFSIZE,
//! kernel/src/syscall.rs:2281+) and writes them to the early console.
//! Longer records are chunked at char boundaries; failures are swallowed
//! (audit loss is never fatal — same argument as C fdref.c's close).
//!
//! No registered gateway (production without `vm_acl_audit`, or before
//! `VmServer::new`) → the record is formatted and dropped at this single
//! point — the call sites stay live (no dead-code drift) and the wiring
//! point stays explicit.
//!
//! Test builds bypass the feature path entirely: `audit_log!` expands to
//! `std::eprintln!` under `#[cfg(test)]`, and `emit_to_gateway` is
//! directly testable against a `MockGateway`.

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::String;
use core::cell::RefCell;

use crate::kernel_gateway::KernelGateway;

/// C: DIAGBUFSIZE (kernel/const.h) — per-call payload limit.
const DIAG_CHUNK: usize = 128;

type GatewayHandle = Rc<RefCell<Box<dyn KernelGateway>>>;

#[cfg(all(not(test), feature = "vm_acl_audit"))]
static AUDIT_SINK: minix_types::AssumeSyncCell<Option<GatewayHandle>> =
    minix_types::AssumeSyncCell::new(None);

/// Register the audit gateway sink (V11/T15). Called from
/// `VmServer::new`; mirrors the `register_page_alloc` construction
/// pattern in global.rs. `AssumeSyncCell` (not `RefCell`) because
/// statics require `Sync`: single-threaded VM (lib.rs) makes unsynchronized
/// access sound — the same argument as `global.rs`.
///
/// SAFETY (set/get raw access): the VM event loop is the only mutator;
/// no aliasing references outlive a call.
#[cfg(all(not(test), feature = "vm_acl_audit"))]
pub(crate) fn register_gateway(gw: GatewayHandle) {
    // SAFETY: single-threaded event loop; written once per server
    // construction before any audit call site can run.
    unsafe {
        *(AUDIT_SINK.as_ptr() as *mut Option<GatewayHandle>) = Some(gw);
    }
}

/// Clear the sink (V11/T15). Called from `VmServer::drop` so a subsequent
/// server (the next unit test) registers its own without interference.
///
/// SAFETY: see `register_gateway`.
#[cfg(all(not(test), feature = "vm_acl_audit"))]
pub(crate) fn clear_gateway() {
    // SAFETY: single-threaded event loop; no concurrent access.
    unsafe {
        *(AUDIT_SINK.as_ptr() as *mut Option<GatewayHandle>) = None;
    }
}

/// Emit an audit record to an explicit gateway. Unconditional so tests
/// can drive it directly; the feature-gated [`emit`] is a thin sink
/// lookup around this.
///
/// C: `do_diagctl` code 1 (do_diagctl.c:28-44) — each call carries up to
/// 128 bytes; the kernel copies them from the caller and writes the early
/// console. Send failures are swallowed: an audit loss is never fatal.
#[cfg_attr(
    all(not(test), not(feature = "vm_acl_audit")),
    allow(dead_code)
)]
pub(crate) fn emit_to_gateway(gw: &GatewayHandle, args: core::fmt::Arguments<'_>) {
    let text: String = alloc::format!("{}", args);
    let bytes = text.as_bytes();
    let mut start = 0usize;
    while start < bytes.len() {
        let mut end = (start + DIAG_CHUNK).min(bytes.len());
        while end > start && !text.is_char_boundary(end) {
            end -= 1;
        }
        let chunk = &text[start..end];
        let _ = gw.borrow_mut().diag_write(chunk);
        start = end;
    }
}

/// Emit an audit record through the registered gateway (feature builds).
#[cfg(all(not(test), feature = "vm_acl_audit"))]
pub(crate) fn emit(args: core::fmt::Arguments<'_>) {
    // SAFETY: single-threaded event loop; the sink is set in `new` before
    // any audit call site can run and cleared only in `drop`.
    let gw = unsafe { (*AUDIT_SINK.as_ptr()).clone() };
    if let Some(gw) = gw {
        emit_to_gateway(&gw, args);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel_gateway::MockGateway;

    fn gateway() -> GatewayHandle {
        Rc::new(RefCell::new(Box::new(MockGateway::new())))
    }

    #[test]
    fn test_emit_to_gateway_forwards_text() {
        let gw = gateway();
        emit_to_gateway(&gw, format_args!("[VM IPC] ipc_receive() failed — message dropped"));
        let logged = gw.borrow().diag_log();
        assert!(logged.contains("ipc_receive() failed"), "logged: {logged}");
        assert!(logged.starts_with("[VM IPC] "), "logged: {logged}");
    }

    #[test]
    fn test_emit_to_gateway_chunks_beyond_128_bytes() {
        let gw = gateway();
        // 300 ASCII bytes → 128 + 128 + 44 chunks, concatenated exactly.
        let long = "x".repeat(300);
        emit_to_gateway(&gw, format_args!("{}", long));
        assert_eq!(gw.borrow().diag_log(), long);
    }

    #[test]
    fn test_emit_to_gateway_chunks_at_char_boundaries() {
        let gw = gateway();
        // A 2-byte char straddling the 128-byte boundary must not be split.
        // 127 ASCII + 'é'(2B) + padding → chunk 1 ends at 127, chunk 2 owns 'é'.
        let mut text = String::from("a").repeat(127);
        text.push('\u{00e9}');
        text.push_str("bc");
        emit_to_gateway(&gw, format_args!("{}", text));
        assert_eq!(gw.borrow().diag_log(), text);
    }
}
