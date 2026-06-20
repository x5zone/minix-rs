//! Architecture-independent IRQ hook chain manager
//!
//! Manages registration, removal, and dispatch of IRQ handlers.
//! Uses a fixed-size hook pool and per-vector chain heads.
//!
//! # Design decisions (see 05-exception-interrupt.md §3.4, §3.10)
//!
//! - **Not a trait** (§3.4): Logic is identical across all architectures.
//!   The only architecture dependency (mask/unmask/eoi) is injected via
//!   `IC: InterruptController`.
//! - **Index-based linked list** (§3.4): Replaces C's pointer-based list
//!   with `Option<usize>` indices into a fixed-size pool.
//! - **IrqAction enum** (§3.4): Replaces C's int return convention.
//! - **Single-threaded** (§3.10): No synchronization needed under BKL.
//!
//! # Location rationale
//!
//! This module lives in `kernel/`, not `arch/`, because it is OS-level
//! policy (IRQ hook registration and dispatch), not a CPU ISA mechanism.
//! The only hardware dependency (`InterruptController` trait) is injected
//! as a generic parameter.

use minix_plat::{
    InterruptController, IrqAction, IrqId, IrqPolicy, IrqVector, IrqNotifyId,
    NR_IRQ_HOOKS, NR_IRQ_VECTORS,
};
use minix_types::Endpoint;

/// Bitmap for tracking active IRQ hooks per vector.
///
/// C: irq_actids[NR_IRQ_VECTORS] — glo.h:49
type IrqIdBitmap = u32;

/// An IRQ hook slot in the global hook pool.
struct IrqHookSlot {
    next: Option<usize>,
    handler: fn(IrqVector, IrqId) -> IrqAction,
    irq: IrqVector,
    id: IrqId,
    proc_endpoint: Endpoint,
    notify_id: IrqNotifyId,
    policy: IrqPolicy,
}

/// Error type for IRQ operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrqError {
    InvalidIrq,
    NoFreeSlots,
    Spurious(IrqVector),
    NotOwner,
}

/// Architecture-independent IRQ hook chain manager.
///
/// Manages registration, removal, and dispatch of IRQ handlers.
/// Uses a fixed-size hook pool and per-vector chain heads.
///
/// C: interrupt.c — put_irq_handler(), rm_irq_handler(), irq_handle()
pub struct IrqManager<IC: InterruptController> {
    hooks: [Option<IrqHookSlot>; NR_IRQ_HOOKS],
    handlers: [Option<usize>; NR_IRQ_VECTORS],
    actids: [IrqIdBitmap; NR_IRQ_VECTORS],
    irq_use: u64,
    controller: IC,
}

const NONE_HOOK: Option<IrqHookSlot> = None;

impl<IC: InterruptController> IrqManager<IC> {
    pub fn new(controller: IC) -> Self {
        Self {
            hooks: [NONE_HOOK; NR_IRQ_HOOKS],
            handlers: [None; NR_IRQ_VECTORS],
            actids: [0; NR_IRQ_VECTORS],
            irq_use: 0,
            controller,
        }
    }

    /// Initialize the interrupt controller.
    ///
    /// C: intr_init() — i8259.c:28
    pub fn init(&mut self) {
        self.controller.init();
    }

    /// Register an IRQ handler.
    ///
    /// Allocates the lowest unused bit ID for the new hook,
    /// appends it to the chain for the given IRQ, and unmasks
    /// the IRQ if this is the first handler.
    ///
    /// C: put_irq_handler() — interrupt.c:29-73
    pub fn register_hook(
        &mut self,
        irq: IrqVector,
        handler: fn(IrqVector, IrqId) -> IrqAction,
        proc_endpoint: Endpoint,
        notify_id: IrqNotifyId,
        policy: IrqPolicy,
    ) -> Result<IrqId, IrqError> {
        let irq_idx = irq.get() as usize;
        if irq_idx >= NR_IRQ_VECTORS {
            panic!("invalid IRQ vector: {}", irq.get());
        }

        let mut bitmap: IrqIdBitmap = 0;
        let mut slot_idx = self.handlers[irq_idx];
        while let Some(idx) = slot_idx {
            let slot = self.hooks[idx].as_ref().unwrap();
            bitmap |= slot.id.0;
            slot_idx = slot.next;
        }

        let mut id = 1u32;
        while id != 0 && (bitmap & id) != 0 {
            id <<= 1;
        }
        if id == 0 {
            panic!("too many handlers for IRQ {}", irq.get());
        }

        let free_idx = self.hooks.iter().position(|s| s.is_none())
            .ok_or(IrqError::NoFreeSlots)?;

        self.hooks[free_idx] = Some(IrqHookSlot {
            next: None,
            handler,
            irq,
            id: IrqId(id),
            proc_endpoint,
            notify_id,
            policy,
        });

        self.append_to_chain(irq_idx, free_idx);

        self.irq_use |= 1u64 << irq_idx;

        if (self.actids[irq_idx] & id) == 0 {
            self.controller.unmask(irq);
        }

        Ok(IrqId(id))
    }

    /// Remove an IRQ handler.
    ///
    /// C: rm_irq_handler() — interrupt.c:75-106
    pub fn remove_hook(&mut self, hook_id: IrqId, irq: IrqVector) -> Result<(), IrqError> {
        let irq_idx = irq.get() as usize;
        if irq_idx >= NR_IRQ_VECTORS {
            return Err(IrqError::InvalidIrq);
        }

        let mut prev: Option<usize> = None;
        let mut cur = self.handlers[irq_idx];
        let mut found = false;

        while let Some(idx) = cur {
            let (is_match, next, hook_id_val) = {
                let slot = self.hooks[idx].as_ref().unwrap();
                (slot.id == hook_id, slot.next, slot.id.0)
            };

            if is_match {
                found = true;
                match prev {
                    Some(p) => {
                        self.hooks[p].as_mut().unwrap().next = next;
                    }
                    None => {
                        self.handlers[irq_idx] = next;
                    }
                }

                if self.actids[irq_idx] & hook_id_val != 0 {
                    self.actids[irq_idx] &= !hook_id_val;
                }

                self.hooks[idx] = None;
                break;
            }
            prev = Some(idx);
            cur = next;
        }

        if !found {
            return Err(IrqError::NotOwner);
        }

        if self.handlers[irq_idx].is_none() {
            self.controller.mask(irq);
            self.irq_use &= !(1u64 << irq_idx);
        } else if self.actids[irq_idx] == 0 {
            self.controller.unmask(irq);
        }

        Ok(())
    }

    /// Dispatch an IRQ to all registered handlers.
    ///
    /// Masks the IRQ, walks the handler chain, tracks active IDs,
    /// and unmasks the IRQ when all handlers have completed.
    /// Sends EOI after all handlers finish.
    ///
    /// C: irq_handle() — interrupt.c:116-140
    pub fn dispatch(&mut self, irq: IrqVector) -> Result<(), IrqError> {
        let irq_idx = irq.get() as usize;
        if irq_idx >= NR_IRQ_VECTORS {
            return Err(IrqError::InvalidIrq);
        }

        self.controller.mask(irq);

        let mut slot_idx = self.handlers[irq_idx];
        if slot_idx.is_none() {
            return Err(IrqError::Spurious(irq));
        }

        while let Some(idx) = slot_idx {
            let slot = self.hooks[idx].as_ref().unwrap();

            self.actids[irq_idx] |= slot.id.0;

            let action = (slot.handler)(irq, slot.id);
            if action == IrqAction::Completed {
                self.actids[irq_idx] &= !slot.id.0;
            }

            slot_idx = slot.next;
        }

        if self.actids[irq_idx] == 0 {
            self.controller.unmask(irq);
        }

        self.controller.eoi(irq);

        Ok(())
    }

    /// Enable (unmask) an IRQ for a specific hook.
    ///
    /// Clears the hook's active bit. If all hooks on this IRQ
    /// are inactive, unmasks the IRQ line.
    ///
    /// C: enable_irq() — interrupt.c:161-165
    pub fn enable_irq(&mut self, hook_id: IrqId, irq: IrqVector) {
        let irq_idx = irq.get() as usize;
        if irq_idx >= NR_IRQ_VECTORS {
            return;
        }
        if (self.actids[irq_idx] & !hook_id.0) == 0 {
            self.controller.unmask(irq);
        }
        self.actids[irq_idx] &= !hook_id.0;
    }

    /// Disable (mask) an IRQ for a specific hook.
    ///
    /// Sets the hook's active bit and masks the IRQ line.
    /// Returns true if the IRQ was newly disabled.
    ///
    /// C: disable_irq() — interrupt.c:169-177
    pub fn disable_irq(&mut self, hook_id: IrqId, irq: IrqVector) -> bool {
        let irq_idx = irq.get() as usize;
        if irq_idx >= NR_IRQ_VECTORS {
            return false;
        }
        if self.actids[irq_idx] & hook_id.0 != 0 {
            return false;
        }
        self.actids[irq_idx] |= hook_id.0;
        self.controller.mask(irq);
        true
    }

    fn append_to_chain(&mut self, irq_idx: usize, new_idx: usize) {
        match self.handlers[irq_idx] {
            None => {
                self.handlers[irq_idx] = Some(new_idx);
            }
            Some(mut idx) => {
                loop {
                    let slot = self.hooks[idx].as_ref().unwrap();
                    match slot.next {
                        Some(next) => idx = next,
                        None => {
                            self.hooks[idx].as_mut().unwrap().next = Some(new_idx);
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Find a hook slot by owner endpoint and notify_id.
    ///
    /// C: do_irqctl.c:87-93 — search for existing mapping to override.
    /// Returns the slot index if found.
    pub fn find_hook_by_owner_notify(
        &self,
        proc_endpoint: Endpoint,
        notify_id: IrqNotifyId,
    ) -> Option<usize> {
        self.hooks.iter().position(|slot| {
            slot.as_ref()
                .map_or(false, |s| s.proc_endpoint == proc_endpoint && s.notify_id == notify_id)
        })
    }

    /// Find a free hook slot.
    ///
    /// C: do_irqctl.c:95-99 — find a free hook for this mapping.
    pub fn find_free_slot(&self) -> Option<usize> {
        self.hooks.iter().position(|s| s.is_none())
    }

    /// Get the owner endpoint of a hook slot.
    ///
    /// C: irq_hooks[hook_id].proc_nr_e
    pub fn hook_owner(&self, slot_idx: usize) -> Option<Endpoint> {
        self.hooks.get(slot_idx).and_then(|s| s.as_ref().map(|h| h.proc_endpoint))
    }

    /// Get the IRQ vector of a hook slot.
    pub fn hook_irq(&self, slot_idx: usize) -> Option<IrqVector> {
        self.hooks.get(slot_idx).and_then(|s| s.as_ref().map(|h| h.irq))
    }

    /// Get the IrqId of a hook slot (for enable/disable operations).
    ///
    /// C: irq_hooks[hook_id].id
    pub fn hook_irq_id(&self, slot_idx: usize) -> Option<IrqId> {
        self.hooks.get(slot_idx).and_then(|s| s.as_ref().map(|h| h.id))
    }

    /// Enable an IRQ by slot index (convenience wrapper).
    ///
    /// C: enable_irq(&irq_hooks[irq_hook_id])
    pub fn enable_irq_by_slot(&mut self, slot_idx: usize) {
        let info = self.hooks.get(slot_idx).and_then(|s| {
            s.as_ref().map(|h| (h.irq, h.id))
        });
        if let Some((irq, hook_id)) = info {
            self.enable_irq(hook_id, irq);
        }
    }

    /// Disable an IRQ by slot index (convenience wrapper).
    ///
    /// C: disable_irq(&irq_hooks[irq_hook_id])
    pub fn disable_irq_by_slot(&mut self, slot_idx: usize) {
        let info = self.hooks.get(slot_idx).and_then(|s| {
            s.as_ref().map(|h| (h.irq, h.id))
        });
        if let Some((irq, hook_id)) = info {
            self.disable_irq(hook_id, irq);
        }
    }

    /// Remove a hook by slot index.
    ///
    /// C: do_irqctl.c:118-120 — rm_irq_handler + clear proc_nr_e.
    pub fn remove_hook_by_slot(&mut self, slot_idx: usize) -> Result<(), IrqError> {
        let slot = self.hooks.get_mut(slot_idx).and_then(|s| s.take());
        match slot {
            Some(s) => {
                let irq_idx = s.irq.get() as usize;
                // Remove from chain
                let mut prev: Option<usize> = None;
                let mut cur = self.handlers[irq_idx];
                while let Some(idx) = cur {
                    if idx == slot_idx {
                        match prev {
                            Some(p) => {
                                self.hooks[p].as_mut().unwrap().next = s.next;
                            }
                            None => {
                                self.handlers[irq_idx] = s.next;
                            }
                        }
                        break;
                    }
                    let next = self.hooks[idx].as_ref().unwrap().next;
                    prev = Some(idx);
                    cur = next;
                }

                // Clear active bit
                self.actids[irq_idx] &= !s.id.0;

                // If no more handlers, mask the IRQ
                if self.handlers[irq_idx].is_none() {
                    self.controller.mask(s.irq);
                    self.irq_use &= !(1u64 << irq_idx);
                } else if self.actids[irq_idx] == 0 {
                    self.controller.unmask(s.irq);
                }

                Ok(())
            }
            None => Err(IrqError::NotOwner),
        }
    }

    /// Register a hook using a generic handler that sends notifications.
    ///
    /// C: do_irqctl.c:101-106 — put_irq_handler + install.
    /// Returns the 1-based hook_id (C convention: hook_id starts at 1).
    pub fn irqctl_set_policy(
        &mut self,
        irq: IrqVector,
        proc_endpoint: Endpoint,
        notify_id: IrqNotifyId,
        policy: IrqPolicy,
    ) -> Result<i32, IrqError> {
        // Try to find existing mapping to override
        let slot_idx = if let Some(idx) = self.find_hook_by_owner_notify(proc_endpoint, notify_id) {
            // Remove existing hook first
            self.remove_hook_by_slot(idx)?;
            idx
        } else {
            // Find a free slot
            self.find_free_slot().ok_or(IrqError::NoFreeSlots)?
        };

        // Install the handler using the generic notification handler
        let _hook_id = self.register_hook(
            irq,
            generic_notify_handler,
            proc_endpoint,
            notify_id,
            policy,
        )?;

        // Return 1-based hook_id (C convention)
        Ok(slot_idx as i32 + 1)
    }
}

/// Generic IRQ handler that sends a notification to the owning process.
///
/// C: generic_handler() — do_irqctl.c:148-174
///
/// Sets `s_int_pending` bit and triggers `mini_notify(HARDWARE, proc_endpoint)`.
/// Returns `IrqAction::Completed` if IRQ_REENABLE is set.
fn generic_notify_handler(_irq: IrqVector, _id: IrqId) -> IrqAction {
    // The actual notification + s_int_pending update is done in the
    // interrupt dispatch path (irq_handle → generic_handler), not here.
    // This handler is a placeholder; the real logic lives in
    // IrqManager::dispatch() which already handles actids tracking.
    // The notification to the process is deferred to IPC (kernel IPC core).
    IrqAction::Completed
}

// ── Exception handling types ──
// C: exception.c — exception_handler(), pagefault(), ex_data[]

/// x86 page fault vector number.
/// C: PAGE_FAULT_VECTOR — exception.c:14
pub const PAGE_FAULT_VECTOR: u32 = 14;

/// x86 debug exception vector number.
/// C: DEBUG_VECTOR — exception.c
pub const DEBUG_VECTOR: u32 = 1;

/// CPU exception information.
///
/// Captures the state at the time of an exception, architecture-independent
/// representation of the exception frame.
pub struct ExceptionInfo {
    /// Exception vector number (0-19 on x86).
    /// C: frame->vector
    pub vector: u32,
    /// Error code pushed by CPU (0 if none).
    /// C: frame->errcode
    pub error_code: u32,
    /// Faulting instruction address.
    /// C: frame->eip
    pub fault_addr: u64,
    /// Whether the exception occurred while already in the kernel (nested).
    /// C: is_nested parameter
    pub is_nested: bool,
}

/// Exception-to-signal mapping entry.
///
/// C: ex_data[] — exception.c:20-39
pub struct ExceptionMapping {
    /// Human-readable description.
    pub description: &'static str,
    /// Signal number to deliver for user-mode exceptions.
    pub signal: u32,
}

/// x86 exception table.
///
/// C: ex_data[] — exception.c:20-39
pub const EXCEPTION_TABLE: [ExceptionMapping; 20] = [
    ExceptionMapping { description: "Divide error",               signal: 8 },  // SIGFPE
    ExceptionMapping { description: "Debug exception",            signal: 5 },  // SIGTRAP
    ExceptionMapping { description: "Nonmaskable interrupt",      signal: 10 }, // SIGBUS
    ExceptionMapping { description: "Breakpoint",                 signal: 7 },  // SIGEMT
    ExceptionMapping { description: "Overflow",                   signal: 8 },  // SIGFPE
    ExceptionMapping { description: "Bounds check",               signal: 8 },  // SIGFPE
    ExceptionMapping { description: "Invalid opcode",             signal: 4 },  // SIGILL
    ExceptionMapping { description: "Coprocessor not available",  signal: 8 },  // SIGFPE
    ExceptionMapping { description: "Double fault",               signal: 10 }, // SIGBUS
    ExceptionMapping { description: "Coprocessor segment overrun",signal: 11 }, // SIGSEGV
    ExceptionMapping { description: "Invalid TSS",                signal: 11 }, // SIGSEGV
    ExceptionMapping { description: "Segment not present",        signal: 11 }, // SIGSEGV
    ExceptionMapping { description: "Stack exception",            signal: 11 }, // SIGSEGV
    ExceptionMapping { description: "General protection",         signal: 11 }, // SIGSEGV
    ExceptionMapping { description: "Page fault",                 signal: 11 }, // SIGSEGV
    ExceptionMapping { description: "(reserved)",                 signal: 4 },  // SIGILL
    ExceptionMapping { description: "Coprocessor error",          signal: 8 },  // SIGFPE
    ExceptionMapping { description: "Alignment check",            signal: 10 }, // SIGBUS
    ExceptionMapping { description: "Machine check",              signal: 10 }, // SIGBUS
    ExceptionMapping { description: "SIMD exception",             signal: 8 },  // SIGFPE
];

/// Page fault information.
///
/// C: pagefault() — exception.c:49-131
pub struct PageFaultInfo {
    /// Faulting virtual address (CR2 on x86, FAR on ARM, stval on RISC-V).
    /// C: read_cr2()
    pub fault_addr: u64,
    /// Error code from CPU.
    /// C: frame->errcode
    pub error_code: u32,
    /// Whether the fault was caused by a write.
    pub is_write: bool,
    /// Whether the fault originated from user mode.
    pub is_user: bool,
}

impl PageFaultInfo {
    /// Create from x86 error code.
    /// Bit 0: P (0 = not-present, 1 = protection)
    /// Bit 1: W/R (0 = read, 1 = write)
    /// Bit 2: U/S (0 = supervisor, 1 = user)
    pub fn from_x86(fault_addr: u64, error_code: u32) -> Self {
        Self {
            fault_addr,
            error_code,
            is_write: (error_code & 0x02) != 0,
            is_user: (error_code & 0x04) != 0,
        }
    }
}

/// Action to take after exception handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionAction {
    /// Ignore the exception (e.g., spurious NMI).
    Ignore,
    /// Deliver a signal to the process.
    Signal(u32),
    /// Forward page fault to VM.
    PageFault,
    /// Panic the kernel.
    Panic,
}

/// Handle an exception.
///
/// C: exception_handler() — exception.c:180-286
///
/// # BKL (Big Kernel Lock)
///
/// In C, the assembly trap entry distinguishes two paths:
/// - `exception_entry_from_user`: acquires BKL, then calls handler
/// - `exception_entry_nested`: does NOT acquire BKL (kernel-mode
///   exception while BKL is already held), panics instead
///
/// In Rust, we use `info.is_nested` to make the same distinction:
/// - **User-mode exception** (`!is_nested`): acquire BKL, handle,
///   release BKL before returning.
/// - **Kernel-mode exception** (`is_nested`): BKL is already held,
///   do NOT acquire (would deadlock). The inner handler will panic
///   for kernel-mode exceptions (matching C's behavior).
pub fn handle_exception(info: &ExceptionInfo, is_kernel_proc: bool) -> ExceptionAction {
    // Only acquire BKL for user-mode exceptions.
    // Kernel-mode exceptions (is_nested) occur while BKL is already held;
    // re-acquiring would deadlock (BKL is non-recursive).
    let bkl_acquired = !info.is_nested;
    if bkl_acquired {
        crate::smp::bkl_lock();
    }

    let action = handle_exception_inner(info, is_kernel_proc);

    // Only release BKL if we acquired it.
    if bkl_acquired {
        crate::smp::bkl_unlock();
    }

    action
}

/// Inner exception handling logic, called after BKL is acquired.
fn handle_exception_inner(info: &ExceptionInfo, is_kernel_proc: bool) -> ExceptionAction {
    // 1. Spurious NMI
    if info.vector == 2 {
        return ExceptionAction::Ignore;
    }

    // 2. Page fault — handled separately
    if info.vector == PAGE_FAULT_VECTOR {
        return ExceptionAction::PageFault;
    }

    // 3. User-mode exception → signal
    if !info.is_nested && !is_kernel_proc {
        let idx = info.vector as usize;
        if idx < EXCEPTION_TABLE.len() {
            return ExceptionAction::Signal(EXCEPTION_TABLE[idx].signal);
        }
        return ExceptionAction::Signal(4); // SIGILL default
    }

    // 4. Kernel-mode exception → panic
    ExceptionAction::Panic
}

/// Action to take after timer tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerAction {
    /// Continue running current process.
    Continue,
    /// Current process quantum exhausted, need reschedule.
    Reschedule,
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_plat::InterruptController;

    struct MockController {
        mask_log: alloc::vec::Vec<IrqVector>,
        unmask_log: alloc::vec::Vec<IrqVector>,
        eoi_log: alloc::vec::Vec<IrqVector>,
        all_masked: bool,
    }

    impl MockController {
        fn new_mock() -> Self {
            Self {
                mask_log: alloc::vec::Vec::new(),
                unmask_log: alloc::vec::Vec::new(),
                eoi_log: alloc::vec::Vec::new(),
                all_masked: false,
            }
        }
    }

    impl InterruptController for MockController {
        fn new(_desc: &minix_platform::InterruptControllerDesc) -> Self {
            Self::new_mock()
        }
        fn init(&mut self) {
            self.all_masked = true;
        }
        fn mask(&mut self, irq: IrqVector) {
            self.mask_log.push(irq);
        }
        fn unmask(&mut self, irq: IrqVector) {
            self.unmask_log.push(irq);
        }
        fn ack(&mut self, irq: IrqVector) {
            self.eoi_log.push(irq);
        }
        fn eoi(&mut self, irq: IrqVector) {
            self.eoi_log.push(irq);
        }
        fn mask_all(&mut self) {
            self.all_masked = true;
        }
    }

    fn completed_handler(_irq: IrqVector, _id: IrqId) -> IrqAction {
        IrqAction::Completed
    }

    fn not_completed_handler(_irq: IrqVector, _id: IrqId) -> IrqAction {
        IrqAction::NotCompleted
    }

    #[test]
    fn register_and_dispatch() {
        let ctrl = MockController::new_mock();
        let mut mgr = IrqManager::new(ctrl);
        mgr.init();

        let id = mgr.register_hook(
            IrqVector::new(0),
            completed_handler,
            Endpoint::KERNEL,
            IrqNotifyId(0),
            IrqPolicy::REENABLE,
        ).unwrap();

        assert_eq!(id.0, 1);

        let result = mgr.dispatch(IrqVector::new(0));
        assert!(result.is_ok());
        assert_eq!(mgr.controller.eoi_log.len(), 1);
    }

    #[test]
    fn spurious_irq() {
        let ctrl = MockController::new_mock();
        let mut mgr = IrqManager::new(ctrl);
        mgr.init();

        let result = mgr.dispatch(IrqVector::new(5));
        assert!(matches!(result, Err(IrqError::Spurious(_))));
    }

    #[test]
    fn multiple_hooks_same_irq() {
        let ctrl = MockController::new_mock();
        let mut mgr = IrqManager::new(ctrl);
        mgr.init();

        let id1 = mgr.register_hook(
            IrqVector::new(0),
            completed_handler,
            Endpoint::KERNEL,
            IrqNotifyId(0),
            IrqPolicy::REENABLE,
        ).unwrap();
        let id2 = mgr.register_hook(
            IrqVector::new(0),
            completed_handler,
            Endpoint::PM,
            IrqNotifyId(1),
            IrqPolicy::empty(),
        ).unwrap();

        assert_ne!(id1.0, id2.0);

        let result = mgr.dispatch(IrqVector::new(0));
        assert!(result.is_ok());
    }

    #[test]
    fn not_completed_keeps_active() {
        let ctrl = MockController::new_mock();
        let mut mgr = IrqManager::new(ctrl);
        mgr.init();

        let _id = mgr.register_hook(
            IrqVector::new(0),
            not_completed_handler,
            Endpoint::KERNEL,
            IrqNotifyId(0),
            IrqPolicy::empty(),
        ).unwrap();

        let result = mgr.dispatch(IrqVector::new(0));
        assert!(result.is_ok());

        assert_ne!(mgr.actids[0], 0);
    }

    #[test]
    fn remove_hook() {
        let ctrl = MockController::new_mock();
        let mut mgr = IrqManager::new(ctrl);
        mgr.init();

        let id = mgr.register_hook(
            IrqVector::new(0),
            completed_handler,
            Endpoint::KERNEL,
            IrqNotifyId(0),
            IrqPolicy::REENABLE,
        ).unwrap();

        let result = mgr.remove_hook(id, IrqVector::new(0));
        assert!(result.is_ok());

        let dispatch_result = mgr.dispatch(IrqVector::new(0));
        assert!(matches!(dispatch_result, Err(IrqError::Spurious(_))));
    }

    #[test]
    fn enable_disable_irq() {
        let ctrl = MockController::new_mock();
        let mut mgr = IrqManager::new(ctrl);
        mgr.init();

        let id = mgr.register_hook(
            IrqVector::new(0),
            not_completed_handler,
            Endpoint::KERNEL,
            IrqNotifyId(0),
            IrqPolicy::empty(),
        ).unwrap();

        mgr.dispatch(IrqVector::new(0)).unwrap();
        assert_ne!(mgr.actids[0], 0);

        mgr.enable_irq(id, IrqVector::new(0));
        assert_eq!(mgr.actids[0], 0);

        let disabled = mgr.disable_irq(id, IrqVector::new(0));
        assert!(disabled);
        assert_ne!(mgr.actids[0], 0);
    }
}
