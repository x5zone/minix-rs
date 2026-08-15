//! The system-service registration table (`rproc`/`rprocpub`/`rproc_ptr`).
//!
//! Mirrors `minix3/minix/servers/rs/glo.h:33-35` and the slot-management
//! primitives in `manager.c:1935-2109` + `utility.c:352-359`:
//!
//! - `RProcTable` — 64 [`ServiceSlot`] rows + the endpoint→slot index
//!   (ARCH A-4: C `rproc_ptr[NR_PROCS]` → `[Option<SlotId>; NR_PROCS]`).
//! - lookup / alloc / free primitives (C: `lookup_slot_by_*` / `alloc_slot` /
//!   `free_slot`).
//! - `isokendpt` (C: `rs_isokendpt` — utility.c:352-359).
//! - `instances_of` (C: `get_service_instances` — manager.c:1334-1352,
//!   ARCH A-3: static 5-slot array → [`ServiceInstances`] iterator).
//!
//! See 02-rs-process-table.md §4.2.

use alloc::vec::Vec;
use minix_types::{EINVAL, ENOMEM, ENOSYS, Endpoint, NR_PROCS, NR_TASKS, Pid};

use crate::privilege::Privilege;
use crate::service_slot::{Label, RFlags, ServiceSlot, SlotId};
use crate::table::{BootImageDev, BootImagePriv, BootImageSys};

// ── Global update descriptor (C: type.h:43-54) ─────────────────────────────

bitflags::bitflags! {
    /// Flags of the global update descriptor.
    ///
    /// C: `rupdate.flags` — type.h:44. Bit values reuse the `r_flags` macros
    /// (const.h:35/34: `RS_UPDATING`/`RS_INITIALIZING`); the update state
    /// machine that writes them is 16-rs-live-update.md.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct RupdateFlags: u16 {
        /// Update in progress. C: `RS_UPDATING` — const.h:35.
        const UPDATING = 0x080;
        /// Init after update in progress. C: `RS_INITIALIZING` — const.h:34.
        const INITIALIZING = 0x040;
    }
}

/// Global live-update descriptor.
///
/// C: `struct rupdate` — `minix3/minix/servers/rs/type.h:43-54` (global:
/// glo.h:45). Data shape only — the state machine (prepare/update/init/end)
/// is 16-rs-live-update.md.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RupdateDescriptor {
    /// Status flags. C: `rupdate.flags` — type.h:44.
    pub flags: RupdateFlags,
    /// Number of descriptors scheduled for the update. C: `num_rpupds` — type.h:45.
    pub num_rpupds: usize,
    /// Number of pending init ready messages. C: `num_init_ready_pending` — type.h:46.
    pub num_init_ready_pending: usize,
    /// Current descriptor under update. C: `curr_rpupd` — type.h:47 (ARCH A-3).
    pub curr_rpupd: Option<SlotId>,
    /// First descriptor scheduled. C: `first_rpupd` — type.h:48 (ARCH A-3).
    pub first_rpupd: Option<SlotId>,
    /// Last descriptor scheduled. C: `last_rpupd` — type.h:49 (ARCH A-3).
    pub last_rpupd: Option<SlotId>,
    /// VM descriptor scheduled. C: `vm_rpupd` — type.h:50 (ARCH A-3).
    pub vm_rpupd: Option<SlotId>,
    /// RS descriptor scheduled. C: `rs_rpupd` — type.h:51 (ARCH A-3).
    pub rs_rpupd: Option<SlotId>,
}

impl RupdateDescriptor {
    /// Fresh descriptor — C: `RUPDATE_INIT()` (memset 0, const.h:87).
    pub fn new() -> Self {
        Self {
            flags: RupdateFlags::empty(),
            num_rpupds: 0,
            num_init_ready_pending: 0,
            curr_rpupd: None,
            first_rpupd: None,
            last_rpupd: None,
            vm_rpupd: None,
            rs_rpupd: None,
        }
    }
}

impl Default for RupdateDescriptor {
    fn default() -> Self {
        Self::new()
    }
}

// ── ServiceInstances (C: manager.c:1334-1352, ARCH A-3) ─────────────────────

/// Iterator over the instances of a service (itself + prev/next/old/new).
///
/// C: `get_service_instances` — manager.c:1334-1352 uses a `static` 5-slot
/// array; Rust yields the same fixed order (`rp → prev → next → old → new`)
/// without shared mutable state.
pub struct ServiceInstances {
    current: Option<SlotId>,
    prev: Option<SlotId>,
    next: Option<SlotId>,
    old: Option<SlotId>,
    new: Option<SlotId>,
}

impl Iterator for ServiceInstances {
    type Item = SlotId;

    /// C order: rp, r_prev_rp, r_next_rp, r_old_rp, r_new_rp — manager.c:1344-1348.
    fn next(&mut self) -> Option<SlotId> {
        if let Some(id) = self.current.take() {
            return Some(id);
        }
        if let Some(id) = self.prev.take() {
            return Some(id);
        }
        if let Some(id) = self.next.take() {
            return Some(id);
        }
        if let Some(id) = self.old.take() {
            return Some(id);
        }
        if let Some(id) = self.new.take() {
            return Some(id);
        }
        None
    }
}

// ── RProcTable (C: glo.h:33-35) ─────────────────────────────────────────────

/// The system-service registration table.
///
/// C: `rproc[NR_SYS_PROCS]` + `rprocpub[NR_SYS_PROCS]` (glo.h:33-34, merged
/// per row in [`ServiceSlot`]) + `rproc_ptr[NR_PROCS]` (glo.h:35, the
/// endpoint→slot fast index, ARCH A-4).
#[derive(Debug, Clone)]
pub struct RProcTable {
    /// Service rows. C: `rproc[]`/`rprocpub[]` — glo.h:33-34.
    slots: Vec<ServiceSlot>,
    /// Endpoint slot → row index. C: `rproc_ptr[NR_PROCS]` — glo.h:35 (ARCH A-4).
    ///
    /// Indexed by `endpoint.slot()` (`_ENDPOINT_P`). Negative slots (kernel
    /// tasks) are never services and are not indexed.
    by_endpoint: [Option<SlotId>; NR_PROCS],
}

impl RProcTable {
    /// A fresh table with 64 vacant rows.
    ///
    /// C: the table reset loop — main.c:230-237 (`r_flags=0`, `in_use=FALSE`,
    /// `old/new_endpoint=NONE`).
    pub fn new() -> Self {
        Self {
            slots: (0..minix_types::NR_SYS_PROCS)
                .map(|_| ServiceSlot::vacant())
                .collect(),
            by_endpoint: [None; NR_PROCS],
        }
    }

    /// Number of rows (`NR_SYS_PROCS`).
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether the table has no rows (always false; kept for the `len` idiom).
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Borrows a row. Panics on an out-of-range id (defensive: ids come from
    /// this table).
    pub fn get(&self, id: SlotId) -> &ServiceSlot {
        &self.slots[id.0]
    }

    /// Mutably borrows a row. Panics on an out-of-range id (defensive).
    pub fn get_mut(&mut self, id: SlotId) -> &mut ServiceSlot {
        &mut self.slots[id.0]
    }

    /// Endpoint → row index (O(1)). C: `rproc_ptr[_ENDPOINT_P(ep)]` — glo.h:35.
    ///
    /// Kernel tasks (negative slot) and unregistered endpoints yield `None`.
    pub fn endpoint_slot(&self, endpoint: Endpoint) -> Option<SlotId> {
        let slot = endpoint.slot();
        if slot < 0 {
            return None; // kernel tasks are never system services
        }
        self.by_endpoint[slot as usize]
    }

    /// Writes the endpoint → row index.
    ///
    /// C: `rproc_ptr[_ENDPOINT_P(ep)] = rp` — glo.h:35 (ARCH A-4). Used by
    /// `mark_child_created` (manager.c:596) and `swap_slot` (manager.c:1922-1925,
    /// 10-rs-service-create.md §2.7). Kernel-task endpoints are never indexed.
    pub fn set_endpoint_index(&mut self, endpoint: Endpoint, id: Option<SlotId>) {
        let slot = endpoint.slot();
        if slot < 0 {
            return; // kernel tasks are never system services
        }
        self.by_endpoint[slot as usize] = id;
    }

    /// Validates an endpoint and returns its slot number.
    ///
    /// C: `rs_isokendpt` — utility.c:352-359: `_ENDPOINT_P(e)` must be in
    /// `[-NR_TASKS, NR_PROCS)`; otherwise `EINVAL`. Kernel-task slots
    /// (negative) are valid here — the main loop compares `who_p` against
    /// `CLOCK` before touching the table.
    pub fn isokendpt(endpoint: Endpoint) -> Result<i32, i32> {
        let slot = endpoint.slot();
        if slot < -(NR_TASKS as i32) || slot >= NR_PROCS as i32 {
            return Err(EINVAL);
        }
        Ok(slot)
    }

    /// Looks up the active service instance with the given label.
    ///
    /// C: `lookup_slot_by_label` — manager.c:1935-1954. **Filters on
    /// `RS_ACTIVE`** (only the active instance of a service is findable by
    /// label; replicas/old instances are not).
    pub fn lookup_by_label(&self, label: &str) -> Option<SlotId> {
        self.slots
            .iter()
            .enumerate()
            .find(|(_, rp)| rp.flags.contains(RFlags::ACTIVE) && rp.pub_.label == label)
            .map(|(i, _)| SlotId::new(i))
    }

    /// Iterates over in-use rows with their slot ids.
    ///
    /// C: the `for (rp = BEG_RPROC_ADDR; rp < END_RPROC_ADDR; rp++)` scans —
    /// e.g. `add_forward_ipc` (manager.c:2181-2182) and `add_backward_ipc`
    /// (manager.c:2249-2250), 05-rs-ipc-sendmask.md.
    pub fn iter_in_use(&self) -> impl Iterator<Item = (SlotId, &ServiceSlot)> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, rp)| rp.flags.contains(RFlags::IN_USE))
            .map(|(i, rp)| (SlotId::new(i), rp))
    }

    /// Sets the endpoint → slot mapping (ARCH A-4).
    ///
    /// C: `rproc_ptr[_ENDPOINT_P(endpoint)] = rp` — manager.c:599
    /// (create_service's child bookkeeping). Panics on kernel-task endpoints
    /// (negative slots are never services) and out-of-range slots.
    pub fn set_endpoint_mapping(&mut self, endpoint: Endpoint, id: SlotId) {
        let idx = endpoint.slot();
        assert!(
            idx >= 0 && (idx as usize) < NR_PROCS,
            "endpoint out of range"
        );
        assert!(id.0 < self.slots.len(), "slot id out of range");
        self.by_endpoint[idx as usize] = Some(id);
    }

    /// Looks up a service slot by pid.
    ///
    /// C: `lookup_slot_by_pid` — manager.c:1959-1980. `pid < 0` → `None`
    /// (C returns NULL early, manager.c:1965-1967); filters on `RS_IN_USE`.
    pub fn lookup_by_pid(&self, pid: Pid) -> Option<SlotId> {
        if pid < 0 {
            return None;
        }
        self.slots
            .iter()
            .enumerate()
            .find(|(_, rp)| rp.flags.contains(RFlags::IN_USE) && rp.pid == Some(pid))
            .map(|(i, _)| SlotId::new(i))
    }

    /// Looks up a service slot by major device number.
    ///
    /// C: `lookup_slot_by_dev_nr` — manager.c:1985-2008. `dev_nr == 0` →
    /// `None` (C tests `dev_nr <= 0`; the unsigned `u32` has no negative
    /// values, manager.c:1992-1993); filters on `RS_IN_USE`.
    pub fn lookup_by_dev_nr(&self, dev_nr: u32) -> Option<SlotId> {
        if dev_nr == 0 {
            return None;
        }
        self.slots
            .iter()
            .enumerate()
            .find(|(_, rp)| rp.flags.contains(RFlags::IN_USE) && rp.pub_.dev_nr == dev_nr)
            .map(|(i, _)| SlotId::new(i))
    }

    /// Looks up a service slot by socket-driver domain.
    ///
    /// C: `lookup_slot_by_domain` — manager.c:2013-2036. `domain <= 0` →
    /// `None` (manager.c:2020-2021); any match in `domain[..nr_domain]` hits.
    pub fn lookup_by_domain(&self, domain: i32) -> Option<SlotId> {
        if domain <= 0 {
            return None;
        }
        self.slots
            .iter()
            .enumerate()
            .find(|(_, rp)| {
                if !rp.flags.contains(RFlags::IN_USE) {
                    return false;
                }
                let pub_ = &rp.pub_;
                (0..pub_.nr_domain as usize).any(|i| pub_.domain[i] == domain)
            })
            .map(|(i, _)| SlotId::new(i))
    }

    /// Looks up a service slot whose flags have **any** of the given bits set.
    ///
    /// C: `lookup_slot_by_flags` — manager.c:2041-2062. Empty flags → `None`
    /// (manager.c:2047-2048); `rp->r_flags & flags` nonzero matches.
    pub fn lookup_by_flags(&self, flags: RFlags) -> Option<SlotId> {
        if flags.is_empty() {
            return None;
        }
        self.slots
            .iter()
            .enumerate()
            .find(|(_, rp)| rp.flags.contains(RFlags::IN_USE) && rp.flags.intersects(flags))
            .map(|(i, _)| SlotId::new(i))
    }

    /// Allocates the first free row.
    ///
    /// C: `alloc_slot` — manager.c:2067-2083: first row without `RS_IN_USE`;
    /// `ENOMEM` when the table is full.
    pub fn alloc_slot(&mut self) -> Result<SlotId, i32> {
        self.slots
            .iter()
            .position(|rp| !rp.flags.contains(RFlags::IN_USE))
            .map(SlotId::new)
            .ok_or(ENOMEM)
    }

    /// Frees a row (table-level invariants only).
    ///
    /// C: `free_slot` — manager.c:2088-2109:
    /// - `late_reply(rp, OK)` (manager.c:2097) is the 06-rs-main-loop.md
    ///   mechanism (RS_LATEREPLY); this table has no reply channel — callers
    ///   must ensure no pending late reply before freeing (invariant, doc §2.9).
    /// - `free_exec(rp)` when `SF_USE_COPY` (manager.c:2100-2102) is the
    ///   09-rs-exec.md mechanism; exec images are released there when it lands.
    ///
    /// The C clear steps (manager.c:2105-2108) are all applied: flags cleared,
    /// pid reset, `in_use` false, endpoint index removed.
    pub fn free_slot(&mut self, id: SlotId) {
        let endpoint = self.slots[id.0].pub_.endpoint;
        let slot = &mut self.slots[id.0];
        slot.pub_.in_use = false; // manager.c:2107
        slot.pub_.endpoint = Endpoint::NONE;
        slot.flags = RFlags::empty(); // manager.c:2105
        slot.pid = None; // manager.c:2106 (r_pid = -1)
        // C: rproc_ptr[_ENDPOINT_P(rpub->endpoint)] = NULL — manager.c:2108.
        if !endpoint.is_none() && endpoint.slot() >= 0 {
            self.by_endpoint[endpoint.slot() as usize] = None;
        }
    }

    /// Activates a boot slot (Step 1 of `sef_cb_init_fresh`).
    ///
    /// C: main.c:255-345 — `rp = &rproc[boot_image_priv - boot_image_priv_table]`
    /// (255); label/sys/dev/endpoint population (262/301/306/326); activation
    /// `r_flags = RS_IN_USE|RS_ACTIVE` (343); `rproc_ptr[...] = rp` (344);
    /// `in_use = TRUE` (345). The priv-structure construction (main.c:264-296)
    /// belongs to 03; only the slot-level facts are applied here.
    /// # Arity note
    ///
    /// Eight arguments mirror the C loop body (main.c:255-345), which
    /// populates the slot from the three boot tables + the assembled
    /// privilege in one iteration; bundling would hide the per-field C
    /// mapping (03-rs-privilege.md §4.2).
    #[allow(clippy::too_many_arguments)]
    pub fn activate_boot_slot(
        &mut self,
        id: SlotId,
        endpoint: Endpoint,
        proc_name: Label,
        priv_: &BootImagePriv,
        sys: &BootImageSys,
        dev: &BootImageDev,
        privilege: Privilege,
    ) -> Result<(), i32> {
        let slot = self.slots.get_mut(id.0).ok_or(ENOSYS)?;
        if slot.flags.contains(RFlags::IN_USE) {
            // Defensive: C writes over the row unconditionally (main.c:255).
            return Err(ENOSYS);
        }
        slot.pub_.label = Label::from_bytes(priv_.label.as_bytes()); // main.c:262
        slot.pub_.proc_name = proc_name; // main.c:313 (strlcpy from ip->proc_name)
        slot.pub_.sys_flags = crate::service_slot::SysFlags::from_bits_truncate(sys.flags as u16); // main.c:301
        slot.pub_.dev_nr = dev.dev_nr; // main.c:306
        slot.pub_.endpoint = endpoint; // main.c:325
        slot.pub_.in_use = true; // main.c:345
        slot.flags = RFlags::IN_USE | RFlags::ACTIVE; // main.c:343
        slot.priv_ = privilege; // main.c:264-296 (03-rs-privilege.md)
        let idx = endpoint.slot();
        debug_assert!(idx >= 0 && (idx as usize) < NR_PROCS);
        self.by_endpoint[idx as usize] = Some(id); // main.c:344 (ARCH A-4)
        Ok(())
    }

    /// Swaps the full contents of two rows.
    ///
    /// C: `swap_slot`'s pair move `*src_rp = orig_dst_rproc; *src_rpub =
    /// orig_dst_rprocpub; ...` — manager.c:1887-1896. With the public half
    /// embedded per row (ARCH A-3), the C "swap both tables + restore the
    /// `r_pub` self-pointer" collapses to one row swap (each row keeps its
    /// own index).
    pub fn swap_rows(&mut self, a: SlotId, b: SlotId) {
        self.slots.swap(a.0, b.0);
    }

    /// All instances of the service at `id`.
    ///
    /// C: `get_service_instances` — manager.c:1334-1352 (ARCH A-3).
    pub fn instances_of(&self, id: SlotId) -> ServiceInstances {
        let slot = &self.slots[id.0];
        ServiceInstances {
            current: Some(id),
            prev: slot.prev_rp,
            next: slot.next_rp,
            old: slot.old_rp,
            new: slot.new_rp,
        }
    }
}

impl Default for RProcTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privilege::PrivFlags;
    use crate::service_slot::{SRVR_SF, SysFlags};

    fn boot_priv(endpoint: Endpoint, label: &'static str) -> BootImagePriv {
        BootImagePriv {
            endpoint,
            label,
            flags: 0,
        }
    }

    fn boot_sys(flags: u32) -> BootImageSys {
        BootImageSys {
            endpoint: Endpoint::NONE,
            flags,
        }
    }

    fn boot_dev(dev_nr: u32) -> BootImageDev {
        BootImageDev {
            endpoint: Endpoint::NONE,
            dev_nr,
        }
    }

    /// A boot-step privilege for slot tests (flags not asserted here).
    fn boot_privilege(endpoint: Endpoint) -> Privilege {
        Privilege::boot_priv(PrivFlags::empty(), endpoint.slot())
    }

    /// A table with one activated boot slot at index 0 (label "vfs", pid 100).
    fn table_with_one_slot() -> RProcTable {
        let mut table = RProcTable::new();
        table
            .activate_boot_slot(
                SlotId::new(0),
                Endpoint::VFS,
                Label::from_bytes(b"vfs"),
                &boot_priv(Endpoint::VFS, "vfs"),
                &boot_sys(SRVR_SF.bits() as u32),
                &boot_dev(0),
                boot_privilege(Endpoint::VFS),
            )
            .expect("activate");
        table.get_mut(SlotId::new(0)).pid = Some(100);
        table
    }

    #[test]
    fn test_lookup_by_label_requires_active() {
        let mut table = RProcTable::new();
        // IN_USE but not ACTIVE → must NOT be found by label (manager.c:1944).
        let slot = table.get_mut(SlotId::new(0));
        slot.flags = RFlags::IN_USE;
        slot.pub_.label = Label::from_bytes(b"vfs");
        slot.pub_.in_use = true;
        assert_eq!(table.lookup_by_label("vfs"), None);

        // ACTIVE → found.
        let slot = table.get_mut(SlotId::new(0));
        slot.flags |= RFlags::ACTIVE;
        assert_eq!(table.lookup_by_label("vfs"), Some(SlotId::new(0)));
        assert_eq!(table.lookup_by_label("pm"), None);
    }

    #[test]
    fn test_lookup_by_pid_negative() {
        let table = table_with_one_slot();
        // pid < 0 → None (manager.c:1965-1967).
        assert_eq!(table.lookup_by_pid(-1), None);
        assert_eq!(table.lookup_by_pid(100), Some(SlotId::new(0)));
        assert_eq!(table.lookup_by_pid(101), None);
    }

    #[test]
    fn test_lookup_by_dev_nr_zero() {
        let mut table = RProcTable::new();
        table
            .activate_boot_slot(
                SlotId::new(0),
                Endpoint::TTY,
                Label::from_bytes(b"tty"),
                &boot_priv(Endpoint::TTY, "tty"),
                &boot_sys(0),
                &boot_dev(5),
                boot_privilege(Endpoint::TTY),
            )
            .expect("activate");
        assert_eq!(table.lookup_by_dev_nr(5), Some(SlotId::new(0)));
        assert_eq!(table.lookup_by_dev_nr(0), None); // NO_DEV (manager.c:1992-1993)
        assert_eq!(table.lookup_by_dev_nr(6), None);
    }

    #[test]
    fn test_lookup_by_domain() {
        let mut table = RProcTable::new();
        table
            .activate_boot_slot(
                SlotId::new(0),
                Endpoint::DS,
                Label::from_bytes(b"ds"),
                &boot_priv(Endpoint::DS, "ds"),
                &boot_sys(0),
                &boot_dev(0),
                boot_privilege(Endpoint::DS),
            )
            .expect("activate");
        let slot = table.get_mut(SlotId::new(0));
        slot.pub_.nr_domain = 2;
        slot.pub_.domain[0] = 1;
        slot.pub_.domain[1] = 3;

        assert_eq!(table.lookup_by_domain(3), Some(SlotId::new(0)));
        assert_eq!(table.lookup_by_domain(1), Some(SlotId::new(0)));
        assert_eq!(table.lookup_by_domain(2), None);
        assert_eq!(table.lookup_by_domain(0), None); // domain <= 0 (manager.c:2020-2021)
        assert_eq!(table.lookup_by_domain(-4), None);
    }

    #[test]
    fn test_lookup_by_flags_any_bit() {
        let mut table = RProcTable::new();
        let slot = table.get_mut(SlotId::new(0));
        slot.flags = RFlags::IN_USE | RFlags::INITIALIZING;
        slot.pub_.in_use = true;

        // Any-bit match (manager.c:2056): INITIALIZING hits.
        assert_eq!(
            table.lookup_by_flags(RFlags::INITIALIZING),
            Some(SlotId::new(0))
        );
        assert_eq!(
            table.lookup_by_flags(RFlags::UPDATING | RFlags::INITIALIZING),
            Some(SlotId::new(0))
        );
        assert_eq!(table.lookup_by_flags(RFlags::UPDATING), None);
        assert_eq!(table.lookup_by_flags(RFlags::empty()), None); // flags == 0 (manager.c:2047-2048)
    }

    #[test]
    fn test_alloc_slot_roundtrip() {
        let mut table = RProcTable::new();
        let id = table.alloc_slot().expect("first free slot");
        assert_eq!(id, SlotId::new(0));

        table.free_slot(id);
        // Reused after free (manager.c:2105-2108 clear the row).
        assert_eq!(table.alloc_slot().expect("reuse"), SlotId::new(0));
    }

    #[test]
    fn test_alloc_slot_full_returns_enomem() {
        let mut table = RProcTable::new();
        for _ in 0..table.len() {
            let id = table.alloc_slot().expect("slot");
            table.get_mut(id).flags = RFlags::IN_USE;
        }
        assert_eq!(table.alloc_slot(), Err(ENOMEM)); // manager.c:2076-2079
    }

    #[test]
    fn test_free_slot_clears_table_state() {
        let mut table = table_with_one_slot();
        assert_eq!(table.endpoint_slot(Endpoint::VFS), Some(SlotId::new(0)));

        table.free_slot(SlotId::new(0));

        let slot = table.get(SlotId::new(0));
        assert!(slot.flags.is_empty()); // manager.c:2105
        assert_eq!(slot.pid, None); // manager.c:2106
        assert!(!slot.pub_.in_use); // manager.c:2107
        assert_eq!(slot.pub_.endpoint, Endpoint::NONE);
        assert_eq!(table.endpoint_slot(Endpoint::VFS), None); // manager.c:2108
    }

    #[test]
    fn test_activate_boot_slot_indexes_endpoint() {
        let mut table = RProcTable::new();
        table
            .activate_boot_slot(
                SlotId::new(3),
                Endpoint::SCHED,
                Label::from_bytes(b"sched"),
                &boot_priv(Endpoint::SCHED, "sched"),
                &boot_sys(0),
                &boot_dev(0),
                boot_privilege(Endpoint::SCHED),
            )
            .expect("activate");

        // A-4: O(1) reverse lookup hits (main.c:344).
        assert_eq!(table.endpoint_slot(Endpoint::SCHED), Some(SlotId::new(3)));
        let slot = table.get(SlotId::new(3));
        assert_eq!(slot.pub_.label.as_str(), Some("sched"));
        assert_eq!(slot.flags, RFlags::IN_USE | RFlags::ACTIVE); // main.c:343
        assert!(slot.pub_.in_use); // main.c:345
    }

    #[test]
    fn test_activate_boot_slot_rejects_reuse() {
        let mut table = table_with_one_slot();
        assert_eq!(
            table.activate_boot_slot(
                SlotId::new(0),
                Endpoint::PM,
                Label::from_bytes(b"pm"),
                &boot_priv(Endpoint::PM, "pm"),
                &boot_sys(0),
                &boot_dev(0),
                boot_privilege(Endpoint::PM),
            ),
            Err(ENOSYS) // defensive: row already IN_USE
        );
    }

    #[test]
    fn test_activate_boot_slot_rejects_out_of_range() {
        let mut table = RProcTable::new();
        assert_eq!(
            table.activate_boot_slot(
                SlotId::new(64), // out of NR_SYS_PROCS
                Endpoint::PM,
                Label::from_bytes(b"pm"),
                &boot_priv(Endpoint::PM, "pm"),
                &boot_sys(0),
                &boot_dev(0),
                boot_privilege(Endpoint::PM),
            ),
            Err(ENOSYS)
        );
    }

    #[test]
    fn test_isokendpt_bounds() {
        // utility.c:356 — [-NR_TASKS, NR_PROCS).
        assert_eq!(RProcTable::isokendpt(Endpoint::CLOCK), Ok(-3));
        assert_eq!(RProcTable::isokendpt(Endpoint::SYSTEM), Ok(-2));
        assert_eq!(RProcTable::isokendpt(Endpoint::PM), Ok(0));
        assert_eq!(RProcTable::isokendpt(Endpoint::INIT), Ok(11));
        // Endpoint::NONE/ANY sit above the top of the user range.
        assert_eq!(RProcTable::isokendpt(Endpoint::NONE), Err(EINVAL));
        assert_eq!(RProcTable::isokendpt(Endpoint::ANY), Err(EINVAL));
        // Generation-wrapped slot still validates by slot (endpoint.h:68-69).
        let gen_endpoint = Endpoint::from_generation_slot(1, 5);
        assert_eq!(RProcTable::isokendpt(gen_endpoint), Ok(5));
    }

    #[test]
    fn test_endpoint_slot_ignores_kernel_tasks() {
        let table = RProcTable::new();
        // Negative slots are kernel tasks — never services (A-4: no index).
        assert_eq!(table.endpoint_slot(Endpoint::CLOCK), None);
    }

    #[test]
    fn test_instances_of_order() {
        let mut table = RProcTable::new();
        // Link: 0 (prev=1, next=2, old=3, new=4).
        let links = [(
            SlotId::new(1),
            SlotId::new(2),
            SlotId::new(3),
            SlotId::new(4),
        )];
        {
            let slot = table.get_mut(SlotId::new(0));
            slot.prev_rp = Some(links[0].0);
            slot.next_rp = Some(links[0].1);
            slot.old_rp = Some(links[0].2);
            slot.new_rp = Some(links[0].3);
        }
        // C order: rp, prev, next, old, new — manager.c:1344-1348.
        let got: Vec<SlotId> = table.instances_of(SlotId::new(0)).collect();
        assert_eq!(
            got,
            vec![
                SlotId::new(0),
                SlotId::new(1),
                SlotId::new(2),
                SlotId::new(3),
                SlotId::new(4),
            ]
        );
    }

    #[test]
    fn test_instances_of_unlinked() {
        let table = table_with_one_slot();
        let got: Vec<SlotId> = table.instances_of(SlotId::new(0)).collect();
        assert_eq!(got, vec![SlotId::new(0)]);
    }

    #[test]
    fn test_rupdate_descriptor_new() {
        // C: RUPDATE_INIT() — memset 0 (const.h:87).
        let upd = RupdateDescriptor::new();
        assert!(upd.flags.is_empty());
        assert_eq!(upd.num_rpupds, 0);
        assert_eq!(upd.num_init_ready_pending, 0);
        assert_eq!(upd.curr_rpupd, None);
        assert_eq!(upd.first_rpupd, None);
        assert_eq!(upd.last_rpupd, None);
        assert_eq!(upd.vm_rpupd, None);
        assert_eq!(upd.rs_rpupd, None);
    }
}
