//! Panic diagnostic hook — the kernel-side panic renderer registration
//! point (D-48, A-8 step 2).
//!
//! # Why this lives in minix-types
//!
//! The `#[panic_handler]` lives in `minix-rt`, which cannot reach the
//! kernel's diagnostic context (EarlyConsole, CPU id, stack walker) —
//! the dependency direction is `minix-kernel → minix-types ← minix-rt`.
//! Both sides see this module: the kernel registers its renderer at
//! boot; the minix-rt handler (feature `panic-handler`) delegates
//! rendering to it when one is registered.
//!
//! # Contract
//!
//! The hook receives the formatted stage-1 panic message (location +
//! payload) and takes over ALL rendering — the kernel hook prints the
//! C-panic format (`kernel panic: ` + message + `kernel on CPU %d: ` +
//! backtrace, C utility.c:30-39; `minix_shutdown(0)` stays deferred).
//! With no hook registered the handler falls back to its stage-1 sink.

/// Kernel-side panic renderer, registered at boot (see
/// `minix_kernel::register_panic_diagnostic`). Receives the formatted
/// panic message and takes over rendering.
pub type PanicDiagnosticHook = fn(&str);

static PANIC_DIAGNOSTIC_HOOK: crate::AssumeSyncCell<Option<PanicDiagnosticHook>> =
    crate::AssumeSyncCell::new(None);

/// Register (or clear) the panic diagnostic hook.
///
/// Called once from the kernel boot path, as early as the kernel's
/// diagnostic context (EarlyConsole, CPU id, stack walker) is usable.
/// Panics before registration fall back to the handler's stage-1 sink
/// path.
pub fn set_panic_diagnostic_hook(hook: Option<PanicDiagnosticHook>) {
    // SAFETY: panic handling is effectively single-threaded — the first
    // panic halts forward progress; registration happens before any
    // concurrency exists (single-CPU boot) or under the BKL.
    unsafe { *PANIC_DIAGNOSTIC_HOOK.get() = hook };
}

/// Current hook, if any.
pub fn panic_diagnostic_hook() -> Option<PanicDiagnosticHook> {
    // SAFETY: as `set_panic_diagnostic_hook`.
    unsafe { *PANIC_DIAGNOSTIC_HOOK.get() }
}

/// Invoke the registered hook with the formatted panic message.
/// Returns `true` if a hook ran, `false` when none is registered.
pub fn run_panic_diagnostic_hook(message: &str) -> bool {
    match panic_diagnostic_hook() {
        Some(hook) => {
            hook(message);
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicUsize, Ordering};

    static HOOK_INVOCATIONS: AtomicUsize = AtomicUsize::new(0);

    fn counting_hook(_message: &str) {
        HOOK_INVOCATIONS.fetch_add(1, Ordering::AcqRel);
    }

    /// Hook set → run invokes it with the message and reports true;
    /// cleared → run reports false. Workspace forces
    /// RUST_TEST_THREADS=1, so the shared static slot is serial.
    #[test]
    fn hook_set_run_clear_round_trip() {
        set_panic_diagnostic_hook(None);
        assert!(!run_panic_diagnostic_hook("stage-1 fallback"));
        assert_eq!(HOOK_INVOCATIONS.load(Ordering::Acquire), 0);

        set_panic_diagnostic_hook(Some(counting_hook));
        assert!(run_panic_diagnostic_hook("kernel message"));
        assert_eq!(HOOK_INVOCATIONS.load(Ordering::Acquire), 1);

        set_panic_diagnostic_hook(None);
        assert!(!run_panic_diagnostic_hook("fallback again"));
        assert_eq!(HOOK_INVOCATIONS.load(Ordering::Acquire), 1);
    }
}
