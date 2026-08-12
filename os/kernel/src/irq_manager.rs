//! Architecture-independent IRQ hook chain manager
//!
//! Manages registration, removal, and dispatch of IRQ handlers.
//! Uses a fixed-size hook pool and per-vector chain heads.
//!
//! # Design decisions (see 14-exception-interrupt.md §3.6, §3.7)
//!
//! - **Not a trait** (§3.7): Logic is identical across all architectures.
//!   The only architecture dependency (mask/unmask/eoi) is injected via
//!   `IC: InterruptController`.
//! - **Index-based linked list** (§3.6): Replaces C's pointer-based list
//!   with `Option<usize>` indices into a fixed-size pool.
//! - **IrqAction enum** (§3.6): Replaces C's int return convention.
//! - **Single-threaded** (§3.1): No synchronization needed under BKL.
//!
//! Exception dispatch lives in `arch::ExceptionDispatcher`, not here —
//! see `os/arch/src/arch/exception_dispatcher.rs`.
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

/// Trait for delivering hardware interrupt notifications to user-space processes.
///
/// Implemented by `KernelNotifier` (production) and `MockNotifier` (tests).
/// The IRQ dispatch path injects an `&mut dyn IrqNotify` into each handler's
/// [`IrqHookContext`]; `generic_notify_handler` calls `notify_hardware` to
/// reproduce the C side-effect `mini_notify(proc_addr(HARDWARE), hook->proc_nr_e)`
/// (do_irqctl.c:170).
///
/// # Design rationale (D9 / 14-exception-interrupt.md §4.4)
///
/// The handler signature carries the notifier as a trait object rather than
/// reaching into global state directly. This keeps handlers testable (the
/// trait is mockable) and avoids `unsafe` proliferation — the `unsafe` access
/// to global `PROC_TABLE` / `PRIV_TABLE` lives in the single `KernelNotifier`
/// impl, not scattered across handlers.
pub trait IrqNotify {
    /// Record the pending interrupt bit and deliver a HARDWARE notification.
    ///
    /// C: `priv(rp)->s_int_pending |= (1 << hook->notify_id);`
    ///    `mini_notify(proc_addr(HARDWARE), hook->proc_nr_e);` — do_irqctl.c:167-170.
    fn notify_hardware(&mut self, dst: Endpoint, notify_id: IrqNotifyId);
}

/// Context passed to IRQ handlers.
///
/// Replaces C's `irq_hook_t *hook` parameter to `generic_handler`. Carries
/// all slot information needed by the handler, plus a `&mut dyn IrqNotify`
/// for delivering notifications to user space.
///
/// # Lifetime
///
/// `'a` is tied to the borrow of the `dyn IrqNotify` passed into [`dispatch`].
/// The handler receives `&mut IrqHookContext<'a>` and may reborrow the
/// notifier for the duration of the call.
pub struct IrqHookContext<'a> {
    /// The IRQ vector being dispatched. C: `hook->irq`.
    pub irq: IrqVector,
    /// The hook's ID within this IRQ vector. C: `hook->id`.
    pub id: IrqId,
    /// The owning process's endpoint (for notification). C: `hook->proc_nr_e`.
    pub proc_endpoint: Endpoint,
    /// The notification ID (bit position in `s_int_pending`). C: `hook->notify_id`.
    pub notify_id: IrqNotifyId,
    /// The hook's policy flags (REENABLE, etc.). C: `hook->policy`.
    pub policy: IrqPolicy,
    /// Notifier for delivering hardware notifications to user space.
    /// C: implicit — C's `generic_handler` calls the global `mini_notify` directly.
    pub notifier: &'a mut dyn IrqNotify,
}

/// IRQ handler function pointer type.
///
/// Receives an [`IrqHookContext`] containing slot information and a notifier.
/// Returns [`IrqAction::Completed`] if the IRQ is fully handled (the dispatch
/// loop will clear the hook's active bit), or [`IrqAction::NotCompleted`] if
/// the handler needs to defer completion (the active bit stays set and the
/// IRQ remains masked until `enable_irq` is called).
///
/// C: `int (*handler)(irq_hook_t *)` — glo.h:46.
pub type IrqHandler = for<'a> fn(ctx: &'a mut IrqHookContext<'a>) -> IrqAction;

/// Production [`IrqNotify`] implementation that delivers hardware interrupt
/// notifications to user-space processes via the global IPC engine.
///
/// Used by `IrqManager::dispatch` when called from the trap entry path.
/// Tests use `MockNotifier` instead (see `tests` module).
///
/// # Safety contract
///
/// All methods access global `PROC_TABLE` / `PRIV_TABLE` via `unsafe`.
/// Callers must hold the BKL — the trap entry path acquires BKL before
/// dispatching IRQs.
///
/// # C alignment
///
/// C: `generic_handler` — do_irqctl.c:167-170:
/// ```c
/// priv(proc_addr(proc_nr))->s_int_pending |= (1 << hook->notify_id);
/// mini_notify(proc_addr(HARDWARE), hook->proc_nr_e);
/// ```
/// Rust splits this into two steps (set bit + deliver notification) inside
/// a single `notify_hardware` call, preserving the C ordering.
pub struct KernelNotifier;

impl IrqNotify for KernelNotifier {
    fn notify_hardware(&mut self, dst: Endpoint, notify_id: IrqNotifyId) {
        // C: do_irqctl.c:154 — `get_randomness(&krandom, hook->irq)` is
        // called from `IrqManager::dispatch` (not here) before invoking
        // the handler. See krandom.rs for the implementation.

        // C: do_irqctl.c:160-161 — `if(!isokendpt(hook->proc_nr_e, &proc_nr))
        //                              panic("invalid interrupt handler: %d", hook->proc_nr_e)`
        //
        // C invariant (do_irqctl.c:156-159): "processes that die automatically
        // get their interrupt hooks unhooked." If the endpoint doesn't resolve
        // to a live process (or the process lacks a priv entry), a hook was
        // not cleaned up — this is a kernel bug. We panic with the same
        // diagnostic as C so the bug surfaces immediately rather than silently
        // dropping the notification (which would cause the hook to fire
        // repeatedly with no effect, making debugging very hard).
        //
        // SAFETY: `IrqManager::dispatch` is invoked from the trap entry
        // path, which holds the BKL. Both `proc_table()` and `priv_table()`
        // return `&'static mut` to global BSS — we borrow each once, and
        // the borrows do not overlap (sequential reads/writes).
        unsafe {
            let proc_table = crate::proc_table();
            let priv_table = crate::priv_table();

            let proc = proc_table
                .iter()
                .find(|p| p.p_endpoint == dst)
                .unwrap_or_else(|| {
                    panic!("invalid interrupt handler: endpoint={:?}", dst)
                });

            let priv_id = proc.priv_id.unwrap_or_else(|| {
                panic!(
                    "invalid interrupt handler: no priv_id for endpoint={:?}",
                    dst
                )
            });

            let priv_ = priv_table.get_mut(priv_id).unwrap_or_else(|| {
                panic!(
                    "invalid interrupt handler: no priv entry for endpoint={:?} priv_id={:?}",
                    dst, priv_id
                )
            });

            // C: do_irqctl.c:167 — `priv(proc_addr(proc_nr))->s_int_pending
            //                        |= (1 << hook->notify_id)`
            priv_.signals.s_int_pending |= 1u32 << notify_id.get();
        }

        // C: do_irqctl.c:170 — `mini_notify(proc_addr(HARDWARE), hook->proc_nr_e)`
        //
        // Deliver the notification. `HARDWARE = KERNEL = -1` (proc.rs:80).
        // C's `mini_notify` returns `void` (not a status code); `generic_handler`
        // does not check any return value because there is none to check.
        // The notification is best-effort: if the destination is not currently
        // RECEIVE-ing, the `s_int_pending` bit set above will be delivered on
        // its next RECEIVE. We ignore the Rust `IpcOutcome` to match C's
        // void-return semantics.
        let _ = crate::ipc::kernel_mini_notify(crate::proc::proc_nr::KERNEL, dst);
    }
}

/// Dispatch a hardware interrupt via the global `IRQ_MANAGER`.
///
/// This is the Rust entry point called by the architecture-specific trap
/// entry path when the CPU receives a hardware interrupt (as opposed to an
/// exception or syscall). The trap entry assembly is responsible for:
///
/// 1. Saving registers (frame construction)
/// 2. Acquiring the BKL (Big Kernel Lock)
/// 3. Extracting the IRQ vector from the trap frame
/// 4. Calling this function
/// 5. Releasing the BKL
/// 6. Restoring registers
///
/// C: `hwint_master` / `hwint_slave` (assembly) → `irq_handle(irq)` —
/// interrupt.c:116-140.
///
/// # Safety
///
/// Caller must hold the BKL. The trap entry path acquires BKL before
/// calling this function; the syscall path holds BKL throughout.
///
/// # Errors
///
/// - `IrqError::Spurious(irq)` — no handler registered for this IRQ.
///   The caller should log this (for diagnostics) but not panic —
///   spurious IRQs can occur during normal operation (e.g., race
///   between mask and EOI).
/// - `IrqError::InvalidIrq` — IRQ vector out of range. Indicates a
///   trap-entry bug (extracted an invalid vector from the frame).
///   The caller should panic.
pub fn dispatch_hardware_irq(irq: IrqVector) -> Result<(), IrqError> {
    let mut notifier = KernelNotifier;
    // SAFETY: Caller (trap entry path) holds the BKL.
    let mgr = unsafe { crate::irq_manager() };
    mgr.dispatch(irq, &mut notifier)
}

/// An IRQ hook slot in the global hook pool.
struct IrqHookSlot {
    next: Option<usize>,
    handler: IrqHandler,
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
        handler: IrqHandler,
        proc_endpoint: Endpoint,
        notify_id: IrqNotifyId,
        policy: IrqPolicy,
    ) -> Result<IrqId, IrqError> {
        let slot_idx = self.find_free_slot().ok_or(IrqError::NoFreeSlots)?;
        self.register_hook_at_slot(slot_idx, irq, handler, proc_endpoint, notify_id, policy)
    }

    /// Register an IRQ handler at a specific slot.
    ///
    /// Core installation logic shared by [`register_hook`] (which auto-finds
    /// a free slot) and [`irqctl_set_policy`] (which finds the slot first,
    /// mirroring C's `do_irqctl.c:91-119` pattern: find slot → set fields →
    /// `put_irq_handler(hook_ptr, ...)` → return `hook_ptr - hook_tab + 1`).
    ///
    /// The slot must be empty. Allocates the lowest unused bit ID for the
    /// new hook, appends it to the chain for the given IRQ, and unmasks
    /// the IRQ if this is the first handler.
    ///
    /// C: `put_irq_handler(hook_ptr, ...)` — interrupt.c:29-73.
    fn register_hook_at_slot(
        &mut self,
        slot_idx: usize,
        irq: IrqVector,
        handler: IrqHandler,
        proc_endpoint: Endpoint,
        notify_id: IrqNotifyId,
        policy: IrqPolicy,
    ) -> Result<IrqId, IrqError> {
        let irq_idx = irq.get() as usize;
        if irq_idx >= NR_IRQ_VECTORS {
            panic!("invalid IRQ vector: {}", irq.get());
        }

        // The slot must be free. This catches double-installation bugs
        // and ensures `irqctl_set_policy`'s slot_idx is the actual
        // installation site.
        if self.hooks[slot_idx].is_some() {
            return Err(IrqError::NoFreeSlots);
        }

        // Allocate the lowest unused bit ID for this IRQ chain.
        let mut bitmap: IrqIdBitmap = 0;
        let mut cur = self.handlers[irq_idx];
        while let Some(idx) = cur {
            let slot = self.hooks[idx].as_ref().unwrap();
            bitmap |= slot.id.0;
            cur = slot.next;
        }

        let mut id = 1u32;
        while id != 0 && (bitmap & id) != 0 {
            id <<= 1;
        }
        if id == 0 {
            panic!("too many handlers for IRQ {}", irq.get());
        }

        self.hooks[slot_idx] = Some(IrqHookSlot {
            next: None,
            handler,
            irq,
            id: IrqId(id),
            proc_endpoint,
            notify_id,
            policy,
        });

        self.append_to_chain(irq_idx, slot_idx);

        self.irq_use |= 1u64 << irq_idx;

        // C: interrupt.c:65 — `(irq_actids[irq] &= ~hook->id) == 0`.
        // Clear this hook's active bit (no-op for a brand-new hook, but
        // faithful to C and safe if a slot is ever reused), then unmask
        // the IRQ only if NO handler on this vector is still active.
        self.actids[irq_idx] &= !id;
        if self.actids[irq_idx] == 0 {
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
    /// The `notifier` is injected into each handler's [`IrqHookContext`]
    /// so handlers can deliver hardware notifications to user space without
    /// touching global state directly.
    ///
    /// C: `irq_handle()` — interrupt.c:116-140. C passes `hook` directly
    /// to the handler; Rust passes an [`IrqHookContext`] that also carries
    /// the notifier (D9 / §4.4).
    pub fn dispatch(
        &mut self,
        irq: IrqVector,
        notifier: &mut dyn IrqNotify,
    ) -> Result<(), IrqError> {
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
            // Copy the slot fields by value (all Copy) so we don't hold a
            // borrow on `self.hooks` while calling the handler. The handler
            // receives `&mut IrqHookContext` which only borrows `notifier`.
            let (handler, slot_irq, slot_id, slot_proc_ep, slot_notify_id, slot_policy, next) = {
                let slot = self.hooks[idx].as_ref().unwrap();
                (
                    slot.handler,
                    slot.irq,
                    slot.id,
                    slot.proc_endpoint,
                    slot.notify_id,
                    slot.policy,
                    slot.next,
                )
            };

            self.actids[irq_idx] |= slot_id.0;

            // C: do_irqctl.c:154 — `get_randomness(&krandom, hook->irq)`.
            // Called once per hook invocation (matching C's generic_handler).
            // Currently a no-op stub (see krandom.rs §Design Decisions D3).
            crate::krandom::get_randomness(slot_irq.get() as i32);

            let mut ctx = IrqHookContext {
                irq: slot_irq,
                id: slot_id,
                proc_endpoint: slot_proc_ep,
                notify_id: slot_notify_id,
                policy: slot_policy,
                notifier,
            };

            let action = handler(&mut ctx);
            if action == IrqAction::Completed {
                self.actids[irq_idx] &= !slot_id.0;
            }

            slot_idx = next;
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

    /// Get the notify ID of a hook slot.
    ///
    /// C: irq_hooks[hook_id].notify_id — type.h:25
    pub fn hook_notify_id(&self, slot_idx: usize) -> Option<IrqNotifyId> {
        self.hooks.get(slot_idx).and_then(|s| s.as_ref().map(|h| h.notify_id))
    }

    /// Get the policy flags of a hook slot.
    ///
    /// C: irq_hooks[hook_id].policy — type.h:26
    pub fn hook_policy(&self, slot_idx: usize) -> Option<IrqPolicy> {
        self.hooks.get(slot_idx).and_then(|s| s.as_ref().map(|h| h.policy))
    }

    /// Get a read-only slice of the `irq_actids[]` bitmap array.
    ///
    /// C: `irq_actids[NR_IRQ_VECTORS]` — glo.h:49. Each entry is a bitmap
    /// of `IrqId` bits currently in "active" state for that IRQ vector.
    ///
    /// Used by `SYS_GETINFO` `GET_IRQACTIDS` (do_getinfo.c:179-183) to
    /// export the kernel's active-IRQ bitmap to user-space diagnostic
    /// tools (e.g. `is` server's kernel dump).
    ///
    /// # Safety contract
    ///
    /// Read-only access; caller must hold the BKL to observe a consistent
    /// snapshot (otherwise another CPU may concurrently update `actids`).
    pub fn irq_actids(&self) -> &[IrqIdBitmap] {
        &self.actids
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
/// C: `generic_handler(irq_hook_t *hook)` — do_irqctl.c:148-174.
///
/// Reproduces the C side-effects in order:
/// 1. (C: `get_randomness`) — called from [`IrqManager::dispatch`] before
///    invoking the handler. See [`crate::krandom::get_randomness`].
/// 2. Sets the `s_int_pending` bit for `notify_id` on the owning process.
/// 3. Delivers `mini_notify(HARDWARE, proc_endpoint)` via the injected
///    notifier (see [`IrqNotify`]).
/// 4. Returns `Completed` iff `policy & IRQ_REENABLE` (C: `return(hook->policy & IRQ_REENABLE)`).
///
/// The handler receives [`IrqHookContext`] which carries the slot's
/// `proc_endpoint`, `notify_id`, `policy`, and a `&mut dyn IrqNotify`.
/// This replaces C's direct access to the global `mini_notify` and
/// `priv(proc_addr(...))` (D9 / 14-exception-interrupt.md §4.4).
fn generic_notify_handler(ctx: &mut IrqHookContext) -> IrqAction {
    // C: do_irqctl.c:167-170 — set pending bit + deliver notification.
    ctx.notifier.notify_hardware(ctx.proc_endpoint, ctx.notify_id);

    // C: do_irqctl.c:171 — `return(hook->policy & IRQ_REENABLE)`.
    if ctx.policy.contains(IrqPolicy::REENABLE) {
        IrqAction::Completed
    } else {
        IrqAction::NotCompleted
    }
}

// Note: Exception dispatch (exception_handler / pagefault / ex_data[]) lives
// in `arch::ExceptionDispatcher` — see `os/arch/src/arch/exception_dispatcher.rs`
// and `os/arch/src/arch/exception.rs`. The earlier in-tree duplicate
// (EXCEPTION_TABLE / PageFaultInfo / handle_exception / ExceptionAction /
// TimerAction) was removed during the 14-exception-interrupt rewrite — it
// duplicated the arch path with weaker typing (raw `u32` signal numbers
// instead of `ExceptionSignal` enum) and described non-existent traits.

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
        fn new(_desc: &dyn minix_platform::InterruptControllerDesc) -> Self {
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

    fn completed_handler(_ctx: &mut IrqHookContext) -> IrqAction {
        IrqAction::Completed
    }

    fn not_completed_handler(_ctx: &mut IrqHookContext) -> IrqAction {
        IrqAction::NotCompleted
    }

    /// Mock `IrqNotify` that records all `notify_hardware` calls.
    #[derive(Default)]
    struct MockNotifier {
        notify_log: alloc::vec::Vec<(Endpoint, IrqNotifyId)>,
    }

    impl IrqNotify for MockNotifier {
        fn notify_hardware(&mut self, dst: Endpoint, notify_id: IrqNotifyId) {
            self.notify_log.push((dst, notify_id));
        }
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

        let mut notifier = MockNotifier::default();
        let result = mgr.dispatch(IrqVector::new(0), &mut notifier);
        assert!(result.is_ok());
        assert_eq!(mgr.controller.eoi_log.len(), 1);
    }

    #[test]
    fn spurious_irq() {
        let ctrl = MockController::new_mock();
        let mut mgr = IrqManager::new(ctrl);
        mgr.init();

        let mut notifier = MockNotifier::default();
        let result = mgr.dispatch(IrqVector::new(5), &mut notifier);
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

        let mut notifier = MockNotifier::default();
        let result = mgr.dispatch(IrqVector::new(0), &mut notifier);
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

        let mut notifier = MockNotifier::default();
        let result = mgr.dispatch(IrqVector::new(0), &mut notifier);
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

        let mut notifier = MockNotifier::default();
        let dispatch_result = mgr.dispatch(IrqVector::new(0), &mut notifier);
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

        let mut notifier = MockNotifier::default();
        mgr.dispatch(IrqVector::new(0), &mut notifier).unwrap();
        assert_ne!(mgr.actids[0], 0);

        mgr.enable_irq(id, IrqVector::new(0));
        assert_eq!(mgr.actids[0], 0);

        let disabled = mgr.disable_irq(id, IrqVector::new(0));
        assert!(disabled);
        assert_ne!(mgr.actids[0], 0);
    }

    /// Verify that `generic_notify_handler` actually delivers the
    /// `mini_notify(HARDWARE, proc_endpoint)` side-effect via the injected
    /// notifier. This is the regression test for the P1 TODO that previously
    /// lived at this site.
    #[test]
    fn generic_notify_handler_sends_notification() {
        let ctrl = MockController::new_mock();
        let mut mgr = IrqManager::new(ctrl);
        mgr.init();

        // Register via `irqctl_set_policy`, which installs `generic_notify_handler`.
        let proc_ep = Endpoint::KERNEL;
        let notify_id = IrqNotifyId(3);
        mgr.irqctl_set_policy(
            IrqVector::new(0),
            proc_ep,
            notify_id,
            IrqPolicy::REENABLE,
        ).expect("irqctl_set_policy should succeed");

        let mut notifier = MockNotifier::default();
        mgr.dispatch(IrqVector::new(0), &mut notifier).expect("dispatch should succeed");

        // The handler should have called notify_hardware exactly once with
        // the registered proc_endpoint and notify_id.
        assert_eq!(notifier.notify_log.len(), 1, "expected exactly one notify call");
        assert_eq!(notifier.notify_log[0].0, proc_ep, "notify dst endpoint mismatch");
        assert_eq!(notifier.notify_log[0].1, notify_id, "notify_id mismatch");

        // With REENABLE policy, the handler returns Completed → active bit cleared.
        assert_eq!(mgr.actids[0], 0, "REENABLE policy should clear active bit");
    }

    /// Verify that `generic_notify_handler` returns `NotCompleted` when
    /// `IRQ_REENABLE` is NOT set, leaving the active bit set.
    #[test]
    fn generic_notify_handler_no_reenable_keeps_active() {
        let ctrl = MockController::new_mock();
        let mut mgr = IrqManager::new(ctrl);
        mgr.init();

        let proc_ep = Endpoint::PM;
        let notify_id = IrqNotifyId(5);
        mgr.irqctl_set_policy(
            IrqVector::new(2),
            proc_ep,
            notify_id,
            IrqPolicy::empty(),
        ).expect("irqctl_set_policy should succeed");

        let mut notifier = MockNotifier::default();
        mgr.dispatch(IrqVector::new(2), &mut notifier).expect("dispatch should succeed");

        // Notification still delivered even without REENABLE.
        assert_eq!(notifier.notify_log.len(), 1);
        assert_eq!(notifier.notify_log[0].0, proc_ep);

        // But the active bit stays set (handler returned NotCompleted).
        assert_ne!(mgr.actids[2], 0, "no-REENABLE policy should keep active bit");
    }
}
