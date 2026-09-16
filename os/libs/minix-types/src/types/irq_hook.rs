//! `GET_IRQHOOKS` IRQ-hook-table snapshot — the single Rust authority for
//! the wire layout of the kernel `GET_IRQHOOKS` payload.
//!
//! E-ISPROD: lifted verbatim from `os/kernel/src/misc.rs` (the producer);
//! the IS `irqhooks` dump previously kept an `IrqHookSnap` subset with
//! narrowed widths (u32 notify_id/policy vs the C `unsigned long`).
//!
//! Unlike `ProcInfoStruct` (a semantic snapshot), this struct mirrors C
//! `struct irq_hook` (kernel/type.h:18-26) field-by-field because the dump
//! interprets the raw bytes via the C layout. The `next`/`handler` pointer
//! fields export raw addresses (0 for NULL); user-space tools only read
//! the non-pointer fields.

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IrqHookStruct {
    /// C: `next` — pointer to next hook in chain. Exported as 0 (NULL):
    /// Rust uses index-based lists, not pointers.
    pub next: u64,
    /// C: `handler` — handler function pointer. Exported as 0 (NULL):
    /// Rust stores `fn` pointers in the IrqManager, not in the struct.
    pub handler: u64,
    /// C: `irq` — IRQ vector number.
    pub irq: i32,
    /// C: `id` — id of this hook (bit position in irq_actids).
    pub id: i32,
    /// C: `proc_nr_e` — owning process endpoint (NONE if not in use).
    pub proc_nr_e: i32,
    /// Padding aligning `notify_id` to 8 bytes (C ABI on 64-bit).
    pub _pad0: [u8; 4],
    /// C: `notify_id` — id to return on interrupt (irq_id_t = unsigned).
    pub notify_id: u64,
    /// C: `policy` — bit mask for policy (irq_policy_t = unsigned).
    pub policy: u64,
}

impl Default for IrqHookStruct {
    /// Zeroed except `irq = -1` and `proc_nr_e = NONE` — the kernel's
    /// unused-hook sentinels (kernel misc.rs `impl Default`).
    fn default() -> Self {
        Self {
            next: 0,
            handler: 0,
            irq: -1,
            id: 0,
            proc_nr_e: crate::Endpoint::NONE.0,
            _pad0: [0; 4],
            notify_id: 0,
            policy: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::offset_of;

    /// Wire layout frozen: field order mirrors C struct irq_hook.
    #[test]
    fn test_irq_hook_layout_frozen() {
        assert_eq!(core::mem::size_of::<IrqHookStruct>(), 48);
        assert_eq!(offset_of!(IrqHookStruct, next), 0);
        assert_eq!(offset_of!(IrqHookStruct, handler), 8);
        assert_eq!(offset_of!(IrqHookStruct, irq), 16);
        assert_eq!(offset_of!(IrqHookStruct, id), 20);
        assert_eq!(offset_of!(IrqHookStruct, proc_nr_e), 24);
        assert_eq!(offset_of!(IrqHookStruct, notify_id), 32);
        assert_eq!(offset_of!(IrqHookStruct, policy), 40);
    }

    /// The unused-hook sentinels match the kernel's manual Default.
    #[test]
    fn test_irq_hook_default_sentinels() {
        let h = IrqHookStruct::default();
        assert_eq!(h.irq, -1);
        assert_eq!(h.proc_nr_e, crate::Endpoint::NONE.0);
    }
}
