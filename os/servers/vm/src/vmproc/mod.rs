//! VM process table module.
//!
//! Provides:
//! - `VmFlags`: Process state flags
//! - `VmProcTable`: Process table (AssumeSyncCell-backed)
//! - `EmptySlot<'a>`: Free slot typestate view
//! - `ActiveProc<'a>`: Active process typestate view
//! - `ExitingProc<'a>`: Exiting process typestate view
//!
//! # Visibility Design
//!
//! VM is an independent user-space process. All types are `pub(crate)` —
//! no external crate should depend on VM internals. External processes
//! (PM, VFS, RS) interact with VM via IPC messages.
//!
//! Within the vmproc module tree:
//! - `VmProc` is NOT exported — external code must use typestate views
//! - `get_slot_mut()` is `pub(super)` — only vmproc module tree can access raw `&mut VmProc`
//! - Typestate views (`EmptySlot`, `ActiveProc`, `ExitingProc`) are `pub(crate)`
//! - Query methods on `VmProcTable` are `pub(crate)`
//!
//! State transitions:
//! ```text
//! EmptySlot ──[activate]──> ActiveProc ──[mark_exiting]──> ExitingProc ──[reap]──> EmptySlot
//!                           ActiveProc ──[force_clear]──────────────────────────> EmptySlot
//! ```

mod flags;
mod vmproc;
mod vmproc_handle;
mod table;

pub(crate) use flags::VmFlags;
pub(crate) use vmproc_handle::{EmptySlot, ActiveProc, ExitingProc};
pub(crate) use table::VmProcTable;

#[cfg(test)]
pub(super) mod test_utils {
    use minix_types::{Endpoint, UserSlot};
    use crate::vmproc::VmProcTable;
    use super::ActiveProc;

    pub fn get_active_vmproc(slot: UserSlot) -> ActiveProc<'static> {
        let table = VmProcTable::get_global();
        unsafe { table.reset_slot(slot); }
        let empty = table.get_empty(slot).unwrap();
        let ep = Endpoint::from_generation_slot(1, slot.get() as i32);
        let mut active = empty.activate(ep);
        active.init_page_table().unwrap();
        active.init_regions();
        unsafe { core::mem::transmute::<ActiveProc<'_>, ActiveProc<'static>>(active) }
    }
}
