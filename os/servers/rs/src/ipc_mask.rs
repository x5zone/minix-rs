//! IPC send-mask computation.
//!
//! Mirrors `minix3/minix/servers/rs/manager.c:2112-2331` — `get_next_name`
//! (2115-2152), `add_forward_ipc` (2157-2224), `add_backward_ipc`
//! (2230-2294), and `init_privs` (2300-2331). 05-rs-ipc-sendmask.md.
//!
//! Boot Step 1 does **not** use this machinery: it fills the whole mask via
//! `fill_send_mask(&rp->r_priv.s_ipc_to, ipc_to == ALL_M)` (main.c:272-273),
//! since `SRV_M`/`USR_M` are both `ALL_M` (priv.h:67-69). The list-based
//! computation here serves dynamic services (`RS_UP` → `init_slot` →
//! `edit_slot` → `init_privs`, manager.c:1700, 1794) and runtime edits
//! (`do_edit` → `edit_slot`, request.c:348).

use crate::boot::KernelApi;
use crate::privilege::SysMap;
use crate::process_table::RProcTable;
use crate::service_slot::{Label, RS_MAX_LABEL_LEN, ServiceSlot};
use minix_types::Endpoint;

/// C: `RSS_IPC_ALL` — rs.h:29.
pub const RSS_IPC_ALL: &str = "IPC_ALL";
/// C: `RSS_IPC_ALL_SYS` — rs.h:30.
pub const RSS_IPC_ALL_SYS: &str = "IPC_ALL_SYS";

/// Shared user-process priv id. C: `USER_PRIV_ID = static_priv_id(ROOT_USR_PROC_NR)`
/// = `NR_TASKS + INIT_PROC_NR` — priv.h:18, with `ROOT_USR_PROC_NR = INIT_PROC_NR`
/// = 11 (com.h:78, com.h:72). Excluded from `IPC_ALL_SYS` (manager.c:2325-2328).
pub const USER_PRIV_ID: i32 = minix_types::NR_TASKS as i32 + Endpoint::INIT.slot();

/// Compares a NUL-terminated `ipc_list` buffer with a string literal.
/// C: `strcmp(rrp->r_ipc_list, RSS_IPC_ALL) == 0` — manager.c:2261-2262.
fn ipc_list_eq(list: &[u8], s: &str) -> bool {
    let n = list.iter().position(|&b| b == 0).unwrap_or(list.len());
    &list[..n] == s.as_bytes()
}

/// Tokenizer over an IPC target-process-name list.
///
/// C: `get_next_name` — manager.c:2115-2152. Whitespace-separated names;
/// entries longer than `RS_MAX_LABEL_LEN` are **skipped** (C prints a
/// diagnostic, manager.c:2138-2143) rather than truncated. The C diagnostic
/// is dropped in no_std (A-11 family); the skip semantics are preserved.
pub struct IpcListIterator<'a> {
    rest: &'a [u8],
}

impl<'a> IpcListIterator<'a> {
    /// Builds an iterator over `list` (a NUL-terminated buffer).
    pub fn new(list: &'a [u8]) -> Self {
        Self { rest: list }
    }
}

impl<'a> Iterator for IpcListIterator<'a> {
    type Item = Label;

    fn next(&mut self) -> Option<Label> {
        // Skip leading whitespace (C: manager.c:2128-2129).
        while let Some(&b) = self.rest.first() {
            if b.is_ascii_whitespace() {
                self.rest = &self.rest[1..];
            } else {
                break;
            }
        }
        // The NUL terminator ends the list (C: the for-loop condition
        // `p[0] != '\0'`, manager.c:2125-2126).
        match self.rest.first() {
            None | Some(0) => return None,
            _ => {}
        }
        // Find the end of the next word — whitespace *or* NUL
        // (C: `while (q[0] != '\0' && !isspace(...))`, manager.c:2133-2134).
        let end = self
            .rest
            .iter()
            .position(|&b| b == 0 || b.is_ascii_whitespace())
            .unwrap_or(self.rest.len());
        let word = &self.rest[..end];
        self.rest = &self.rest[end..];
        if word.is_empty() {
            return None; // end of list (C returns NULL, manager.c:2151)
        }
        // Oversized entries are skipped, not truncated (C: manager.c:2138-2143).
        if word.len() > RS_MAX_LABEL_LEN {
            return self.next();
        }
        Some(Label::from_bytes(word))
    }
}

/// Adds send bits for the targets named in `rp`'s own IPC list.
///
/// C: `add_forward_ipc` — manager.c:2157-2224. The pseudo-names `SYSTEM`
/// (kernel tasks, manager.c:2177-2178) and `USER` (all user processes,
/// `INIT_PROC_NR`, manager.c:2179-2180) resolve their priv id via
/// `sys_getpriv` (manager.c:2208-2215). Any other name matches in-use table
/// rows by `proc_name`; a missing match is **not** an error — the target may
/// not have been started yet (manager.c:2183-2188, see `add_backward_ipc`).
pub fn add_forward_ipc(rp: &ServiceSlot, table: &RProcTable, sys: &mut dyn KernelApi) -> SysMap {
    let mut map = SysMap::empty();
    for name in IpcListIterator::new(&rp.ipc_list) {
        let endpoint = if name == "SYSTEM" {
            Some(Endpoint::SYSTEM)
        } else if name == "USER" {
            Some(Endpoint::INIT)
        } else {
            None
        };
        if let Some(ep) = endpoint {
            // C: sys_getpriv(&priv, endpoint); use priv.s_id (manager.c:2208-2215).
            if let Ok(priv_) = sys.getpriv(ep) {
                map = map.set(priv_.id.0 as usize);
            }
            continue;
        }
        // Match by process name (C: manager.c:2181-2205).
        for (_, rrp) in table.iter_in_use() {
            if rrp.pub_.proc_name == name {
                map = map.set(rrp.priv_.id.0 as usize);
            }
        }
    }
    map
}

/// Adds send bits from *other* services' IPC lists that name `target`.
///
/// C: `add_backward_ipc` — manager.c:2230-2294. Covers the case where the
/// target did not exist when the other service's mask was computed
/// (manager.c:2234-2239). The C comment "as the kernel guarantees send mask
/// symmetry" (manager.c:2236) is preserved as a behavioral note — the
/// kernel side of the symmetry check lives in 01-stage-kernel/23-ipc-filter.md.
pub fn add_backward_ipc(target: &ServiceSlot, table: &RProcTable) -> SysMap {
    let mut map = SysMap::empty();
    let target_name = &target.pub_.proc_name;
    for (_, rrp) in table.iter_in_use() {
        // Skip rows with no IPC list (C: manager.c:2252).
        if rrp.ipc_list[0] == 0 {
            continue;
        }
        let is_ipc_all = ipc_list_eq(&rrp.ipc_list, RSS_IPC_ALL);
        let is_ipc_all_sys = ipc_list_eq(&rrp.ipc_list, RSS_IPC_ALL_SYS);
        if is_ipc_all || (is_ipc_all_sys && target.priv_.is_sys_proc()) {
            // C: manager.c:2261-2270.
            map = map.set(rrp.priv_.id.0 as usize);
            continue;
        }
        // Scan the other service's list for the target's process name
        // (C: manager.c:2280-2290).
        for name in IpcListIterator::new(&rrp.ipc_list) {
            if name == *target_name {
                map = map.set(rrp.priv_.id.0 as usize);
            }
        }
    }
    map
}

/// Computes the full IPC send mask for a service from its IPC list.
///
/// C: `init_privs` — manager.c:2300-2331. Three cases:
///
/// 1. `IPC_ALL` — every priv id (manager.c:2325-2328).
/// 2. `IPC_ALL_SYS` — every priv id except the shared user-process
///    `USER_PRIV_ID` (manager.c:2325-2328).
/// 3. Any other list — union of `add_forward_ipc` + `add_backward_ipc`
///    (manager.c:2319-2320).
pub fn init_privs(rp: &ServiceSlot, table: &RProcTable, sys: &mut dyn KernelApi) -> SysMap {
    let is_ipc_all = ipc_list_eq(&rp.ipc_list, RSS_IPC_ALL);
    let is_ipc_all_sys = ipc_list_eq(&rp.ipc_list, RSS_IPC_ALL_SYS);

    if is_ipc_all || is_ipc_all_sys {
        let mut map = SysMap::all();
        if is_ipc_all_sys {
            // All bits except USER_PRIV_ID (C: manager.c:2325-2328).
            map = SysMap(map.0 & !(1u64 << USER_PRIV_ID));
        }
        return map;
    }

    let forward = add_forward_ipc(rp, table, sys);
    let backward = add_backward_ipc(rp, table);
    SysMap(forward.0 | backward.0)
}

/// Shorthand used by callers that own the table: `init_privs` + write-back.
///
/// C: `edit_slot` calls `init_privs(rp, &rp->r_priv)` (manager.c:1700) and the
/// updated structure is submitted with `SYS_PRIV_UPDATE_SYS` (03).
pub fn update_ipc_mask(rp: &mut ServiceSlot, table: &RProcTable, sys: &mut dyn KernelApi) {
    rp.priv_.ipc_to = init_privs(rp, table, sys);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot::Machine;
    use crate::privilege::{PrivCtlOp, PrivFlags, PrivId, Privilege};
    use crate::service_slot::{RFlags, SlotId};
    use alloc::vec::Vec;
    use minix_types::Endpoint;

    /// KernelApi stub: `getpriv` returns a fixed priv id for SYSTEM/INIT.
    struct MockSys {
        ids: Vec<(Endpoint, i32)>,
    }

    impl MockSys {
        fn new() -> Self {
            Self {
                ids: vec![
                    (Endpoint::SYSTEM, 4), // arbitrary, C queries the kernel
                    (Endpoint::INIT, USER_PRIV_ID),
                ],
            }
        }
    }

    impl KernelApi for MockSys {
        fn get_machine(&mut self) -> Result<Machine, i32> {
            unimplemented!()
        }
        fn get_hz(&mut self) -> Result<u32, i32> {
            unimplemented!()
        }
        fn privctl(
            &mut self,
            _proc: Endpoint,
            _op: PrivCtlOp,
            _priv_: Option<&Privilege>,
        ) -> Result<(), i32> {
            unimplemented!()
        }
        fn getpriv(&mut self, proc: Endpoint) -> Result<Privilege, i32> {
            self.ids
                .iter()
                .find(|(ep, _)| *ep == proc)
                .map(|(_, id)| {
                    let mut p = Privilege::boot_priv(PrivFlags::SYS_PROC, 0);
                    p.id = PrivId(*id);
                    p
                })
                .ok_or(-1)
        }
        fn getnuid(&mut self, _proc: Endpoint) -> Result<u32, i32> {
            unimplemented!()
        }
        fn sched_init_proc(&mut self, _proc: Endpoint) -> Result<(), i32> {
            unimplemented!()
        }
        fn getnpid(&mut self, _proc: Endpoint) -> Result<i32, i32> {
            unimplemented!()
        }
        fn setalarm(&mut self, _delay_ticks: u32) -> Result<(), i32> {
            unimplemented!()
        }
    }

    /// A table with two in-use services: `tty` (priv id 10) and `vm` (priv id 13).
    fn table() -> (RProcTable, SlotId, SlotId) {
        let mut t = RProcTable::new();
        let a = t.alloc_slot().unwrap();
        t.get_mut(a).flags |= RFlags::IN_USE;
        let b = t.alloc_slot().unwrap();
        t.get_mut(b).flags |= RFlags::IN_USE;
        t.get_mut(a).pub_.proc_name = Label::from_bytes(b"tty");
        t.get_mut(a).priv_.id = PrivId(10);
        t.get_mut(b).pub_.proc_name = Label::from_bytes(b"vm");
        t.get_mut(b).priv_.id = PrivId(13);
        (t, a, b)
    }

    fn slot(ipc_list: &[u8]) -> ServiceSlot {
        let mut s = ServiceSlot::vacant();
        s.pub_.proc_name = Label::from_bytes(b"tty");
        s.pub_.label = Label::from_bytes(b"tty");
        s.ipc_list[..ipc_list.len()].copy_from_slice(ipc_list);
        s
    }

    #[test]
    fn test_get_next_name_basic() {
        let list = b"vm pm  tty  \0";
        let names: Vec<Label> = IpcListIterator::new(list).collect();
        let strs: Vec<&str> = names.iter().filter_map(|l| l.as_str()).collect();
        assert_eq!(strs, vec!["vm", "pm", "tty"]);
    }

    #[test]
    fn test_get_next_name_empty_and_whitespace() {
        assert_eq!(IpcListIterator::new(b"\0").count(), 0);
        assert_eq!(IpcListIterator::new(b"   \0").count(), 0);
        assert_eq!(IpcListIterator::new(b"").count(), 0);
    }

    #[test]
    fn test_get_next_name_oversized_skipped() {
        // 17-letter word (> RS_MAX_LABEL_LEN) followed by " ok\0".
        let mut full = vec![0u8; RS_MAX_LABEL_LEN + 1];
        for (i, b) in full.iter_mut().enumerate() {
            *b = b'a' + (i % 26) as u8;
        }
        full.extend_from_slice(b" ok\0");
        let names: Vec<Label> = IpcListIterator::new(&full).collect();
        let strs: Vec<&str> = names.iter().filter_map(|l| l.as_str()).collect();
        assert_eq!(strs, vec!["ok"]);
    }

    #[test]
    fn test_init_privs_ipc_all() {
        let (t, a, _) = table();
        let sys = &mut MockSys::new();
        let s = slot(b"IPC_ALL\0");
        let map = init_privs(&s, &t, sys);
        // IPC_ALL includes the shared user priv id (manager.c:2325-2328).
        assert!(map.test(USER_PRIV_ID as usize));
        assert!(map.test(10));
        let _ = a;
    }

    #[test]
    fn test_init_privs_ipc_all_sys() {
        let (t, a, _) = table();
        let sys = &mut MockSys::new();
        let s = slot(b"IPC_ALL_SYS\0");
        let map = init_privs(&s, &t, sys);
        // IPC_ALL_SYS excludes USER_PRIV_ID but includes system privs.
        assert!(!map.test(USER_PRIV_ID as usize));
        assert!(map.test(10));
        let _ = a;
    }

    #[test]
    fn test_init_privs_list_forward_backward() {
        let (t, _, b) = table();
        let sys = &mut MockSys::new();
        // tty's list names vm → forward sets bit for vm's priv id (13).
        let s = slot(b"vm\0");
        let map = init_privs(&s, &t, sys);
        assert!(map.test(13));
        assert!(!map.test(10));
        let _ = b;
    }

    #[test]
    fn test_forward_system_user() {
        let (t, _, _) = table();
        let sys = &mut MockSys::new();
        let s = slot(b"SYSTEM USER\0");
        let map = init_privs(&s, &t, sys);
        assert!(map.test(4)); // SYSTEM priv id from mock
        assert!(map.test(USER_PRIV_ID as usize)); // USER → INIT's priv id
    }

    #[test]
    fn test_forward_unmatched_name_tolerated() {
        let (t, _, _) = table();
        let sys = &mut MockSys::new();
        // A name with no matching slot is fine — target may start later
        // (manager.c:2183-2188); backward IPC covers it when it does.
        let s = slot(b"not-yet-started\0");
        let map = init_privs(&s, &t, sys);
        assert_eq!(map, SysMap::empty());
    }

    #[test]
    fn test_backward_ipc_all_others() {
        let (t, _, _) = table();
        let sys = &mut MockSys::new();
        // vm's list is IPC_ALL → backward sets vm's bit on tty's mask.
        let mut vm = t.get(SlotId::new(1)).clone();
        vm.ipc_list[..8].copy_from_slice(b"IPC_ALL\0");
        // build a fresh table where vm has IPC_ALL
        let mut t2 = RProcTable::new();
        let a = t2.alloc_slot().unwrap();
        t2.get_mut(a).flags |= RFlags::IN_USE;
        let b2 = t2.alloc_slot().unwrap();
        t2.get_mut(b2).flags |= RFlags::IN_USE;
        t2.get_mut(a).pub_.proc_name = Label::from_bytes(b"tty");
        t2.get_mut(a).priv_.id = PrivId(10);
        t2.get_mut(b2).pub_.proc_name = Label::from_bytes(b"vm");
        t2.get_mut(b2).priv_.id = PrivId(13);
        t2.get_mut(b2).ipc_list[..8].copy_from_slice(b"IPC_ALL\0");
        let tty = t2.get(a).clone();
        let map = init_privs(&tty, &t2, sys);
        assert!(map.test(13)); // from vm's IPC_ALL via backward
        let _ = vm;
    }

    #[test]
    fn test_update_ipc_mask_writes_back() {
        let (mut t, _, _) = table();
        let sys = &mut MockSys::new();
        let mut s = t.get(SlotId::new(0)).clone();
        s.ipc_list[..3].copy_from_slice(b"vm\0");
        update_ipc_mask(&mut s, &t, sys);
        assert!(s.priv_.ipc_to.test(13));
    }

    /// Placeholder asserting the public constants match C (rs.h:29-30).
    #[test]
    fn test_constants() {
        assert_eq!(RSS_IPC_ALL, "IPC_ALL");
        assert_eq!(RSS_IPC_ALL_SYS, "IPC_ALL_SYS");
        assert_eq!(USER_PRIV_ID, PrivId::static_priv_id(11).0); // priv.h:18
    }
}
