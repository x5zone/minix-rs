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

use crate::interrupt::{
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interrupt::InterruptController;

    struct MockController {
        mask_log: alloc::vec::Vec<IrqVector>,
        unmask_log: alloc::vec::Vec<IrqVector>,
        eoi_log: alloc::vec::Vec<IrqVector>,
        all_masked: bool,
    }

    impl MockController {
        fn new() -> Self {
            Self {
                mask_log: alloc::vec::Vec::new(),
                unmask_log: alloc::vec::Vec::new(),
                eoi_log: alloc::vec::Vec::new(),
                all_masked: false,
            }
        }
    }

    impl InterruptController for MockController {
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
        let ctrl = MockController::new();
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
        let ctrl = MockController::new();
        let mut mgr = IrqManager::new(ctrl);
        mgr.init();

        let result = mgr.dispatch(IrqVector::new(5));
        assert!(matches!(result, Err(IrqError::Spurious(_))));
    }

    #[test]
    fn multiple_hooks_same_irq() {
        let ctrl = MockController::new();
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
        let ctrl = MockController::new();
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
        let ctrl = MockController::new();
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
        let ctrl = MockController::new();
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
