//! User-space grant table — the Rust rewrite of C libsys `safecopies.c`.
//!
//! C'scpf mechanism keeps the grant table in the **granter's own address
//! space**: libc manages slots (free list + sequence numbers), grows the
//! table with malloc, and `sys_setgrant` only registers the table
//! address/size with the kernel. `cpf_grant_direct` and `cpf_revoke`
//! never trap — they are pure in-memory table edits. `verify_grant` on
//! the kernel side reads the slots cross-space, so the slot layout is
//! the shared contract in [`minix_types`] (types::grant).
//!
//! E-DSWIRE: this table is the transport half every granting client
//! needs (DS first, then devman/RS/VM clients) — one authority instead
//! of per-crate copies.

use alloc::vec::Vec;

use minix_types::{
    CpFlags, CpGrant, CpGrantDirect, CpGrantFree, CpGrantUnion, GRANT_INVALID, GRANT_MAX_SEQ,
    grant_id, grant_idx, grant_seq, grant_valid,
};

use crate::syscall::{KernelCallTransport, perform_kernel_call};

/// C: `GRANT_FAULTED` — safecopies.h:77. `cpf_revoke` returns this when a
/// CPF_TRY grant saw a soft fault during its lifetime.
pub const GRANT_FAULTED: i32 = 1;

/// Access mask accepted by [`GrantTable::grant_direct`]
/// (C: `ACCESS_CHECK` — safecopies.c:13-17: any bit outside
/// READ|WRITE|TRY is EINVAL).
const fn access_check(access: CpFlags) -> bool {
    let all = CpFlags::READ.bits() | CpFlags::WRITE.bits() | CpFlags::TRY.bits();
    (access.bits() & !all) == 0
}

/// A user-space grant table plus its registration state.
///
/// Mirrors C libsys `safecopies.c` state (`grants`/`ngrants`/`freelist`)
/// with one explicit addition: `registered` tracks whether the current
/// table address/size has been told to the kernel (C does it eagerly
/// inside `cpf_prealloc`; the explicit flag lets the caller own the
/// kernel-call timing — one `sys_setgrant` per grow, not per slot).
/// `GrantTable::probe` 的窗口读数:授权的宿主地址窗与访问形状。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrantProbe {
    /// 授权内存窗:`(start, len)`(granter 地址空间的真实地址)。
    pub granter_window: (u64, u64),
    /// MAGIC 臂时为 `(who_to, who_from)`;DIRECT 臂为 `None`。
    pub magic: Option<(i32, i32)>,
    /// 授权含写(`CPF_WRITE`)。
    pub writable: bool,
}

pub struct GrantTable {
    slots: Vec<CpGrant>,
    /// Next free slot index, or -1 when the table is full (C: `freelist`).
    freelist: i32,
    /// True after the table grew (address/size changed) and the kernel
    /// has not been told yet.
    unregistered: bool,
}

impl Default for GrantTable {
    fn default() -> Self {
        Self::new()
    }
}

impl GrantTable {
    /// An empty table. The first `grant_direct` grows it (C starts from
    /// `NR_STATIC_GRANTS = 3` static slots; a `Vec` makes the static
    /// block unnecessary — growth is one realloc).
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            freelist: -1,
            unregistered: false,
        }
    }

    /// Current slot count (C: `ngrants`).
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// True when no slots exist yet.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Register (or re-register after growth) the table with the kernel.
    ///
    /// C: `sys_setgrant(new_grants, new_size)` — `m_lsys_krn_sys_setgrant`
    /// carries the table address and entry count; the kernel stores them
    /// in the caller's privilege structure (`do_setgrant.c`).
    pub fn register(&self, transport: &impl KernelCallTransport) -> Result<(), i32> {
        let mut msg = minix_types::Message::default();
        {
            // SAFETY: m_lsys_krn_sys_setgrant is the documented SYS_SETGRANT
            // payload (kernel/src/syscall.rs dispatch_setgrant reads
            // addr/size; C libsys sys_setgrant.c:8-15).
            let g = unsafe { &mut msg.m_u.m_lsys_krn_sys_setgrant };
            g.addr = self.slots.as_ptr() as u64;
            g.size = self.slots.len() as i32;
        }
        let reply = perform_kernel_call(transport, minix_types::SYS_SETGRANT, &mut msg, |_| {});
        if reply < 0 {
            return Err(-reply);
        }
        Ok(())
    }

    /// Grow the table (C: `cpf_prealloc(0)` — double the size, minimum one
    /// new slot) and register the new address/size with the kernel.
    fn grow_and_register(&mut self, transport: &impl KernelCallTransport) -> Result<(), i32> {
        let old_len = self.slots.len();
        // C caps growth at GRANT_MAX_IDX slots; a Vec cannot lie about
        // memory but the ID space can — keep the C ceiling.
        if old_len >= minix_types::GRANT_MAX_IDX as usize {
            return Err(minix_types::ENOSPC);
        }
        let new_size = (1 + old_len) * 2;

        for g in old_len..new_size {
            // Fresh slots join the free list in ascending order so the
            // lowest IDs are allocated first (safecopies.c:61-69) — this is
            // what makes the first allocated grant have ID 0 (live update
            // relies on it: SEF_STATE_TRANSFER_GID).
            let slot = CpGrant {
                flags: 0,
                seq: 0,
                u: CpGrantUnion {
                    free: CpGrantFree {
                        next: if g < new_size - 1 {
                            g as i32 + 1
                        } else {
                            self.freelist
                        },
                    },
                },
                faulted: 0,
            };
            self.slots.push(slot);
        }
        self.freelist = old_len as i32;
        self.unregistered = true;

        self.register(transport)?;
        self.unregistered = false;
        Ok(())
    }

    /// Allocate a free slot (C: `cpf_new_grantslot` — pop the free list,
    /// growing the table when empty).
    fn new_grantslot(&mut self, transport: &impl KernelCallTransport) -> Result<usize, i32> {
        if self.freelist == -1 {
            self.grow_and_register(transport)?;
        }
        if self.freelist == -1 {
            // C: growth failed — ENOSPC (safecopies.c:131-134).
            return Err(minix_types::ENOSPC);
        }
        let g = self.freelist as usize;
        // SAFETY(test-shape): freelist indexes a live slot by construction
        // (every path that frees a slot pushes its index back).
        let slot_union = unsafe { self.slots.get_unchecked(g) }.u;
        let next = unsafe { slot_union.free.next };
        self.freelist = next;
        Ok(g)
    }

    /// Grant another process read/write access to this process's memory.
    ///
    /// C: `cpf_grant_direct` — safecopies.c:148-172. Pure table edit: fill
    /// the slot (who_to/start/len/faulted), then a barrier and the flags
    /// word last (the kernel treats a set USED|VALID|DIRECT word as the
    /// commit point).
    pub fn grant_direct(
        &mut self,
        transport: &impl KernelCallTransport,
        who_to: i32,
        addr: u64,
        bytes: u64,
        access: CpFlags,
    ) -> Result<i32, i32> {
        if !access_check(access) {
            return Err(minix_types::EINVAL);
        }
        let g = self.new_grantslot(transport)?;
        let seq = self.slots[g].seq;
        // SAFETY(test-shape): g < slots.len() by new_grantslot's contract.
        let slot = unsafe { self.slots.get_unchecked_mut(g) };
        slot.u.direct = CpGrantDirect {
            who_to,
            start: addr,
            len: bytes,
        };
        slot.faulted = GRANT_INVALID;
        // Commit point: flags word last (C's __insn_barrier + flags store).
        slot.flags = (access | CpFlags::DIRECT | CpFlags::USED | CpFlags::VALID).bits() as i32;
        Ok(grant_id(g as u32, seq as u32))
    }

    /// Grant one process access into another's memory (magic grant).
    ///
    /// C: `cpf_grant_magic` — safecopies.c:198-220. 与
    /// [`Self::grant_direct`] 的唯一差别是**转账方与收方分离**：
    /// `who_from` 是缓冲的所有者、`who_to` 是被授权读写它的进程。VFS 用它
    /// 把**用户进程**的缓冲直接交给 FS 写（`cpf_grant_magic(fs_e, user_e,
    /// user_addr, len, CPF_WRITE|cpflag)` — request.c:844/1087）。
    ///
    /// 提交纪律同 direct：先填槽，最后写 `flags`（USED|MAGIC|VALID|access）
    /// 作为内核认可的提交点。
    pub fn grant_magic(
        &mut self,
        transport: &impl KernelCallTransport,
        who_to: i32,
        who_from: i32,
        start: u64,
        len: u64,
        access: CpFlags,
    ) -> Result<i32, i32> {
        if !access_check(access) {
            return Err(minix_types::EINVAL);
        }
        let g = self.new_grantslot(transport)?;
        let seq = self.slots[g].seq;
        // SAFETY(test-shape): g < slots.len() by new_grantslot's contract.
        let slot = unsafe { self.slots.get_unchecked_mut(g) };
        slot.u.magic = minix_types::CpGrantMagic {
            who_from,
            who_to,
            start,
            len,
        };
        slot.faulted = GRANT_INVALID;
        // Commit point: flags word last (C's __insn_barrier + flags store).
        slot.flags = (access | CpFlags::MAGIC | CpFlags::USED | CpFlags::VALID).bits() as i32;
        Ok(grant_id(g as u32, seq as u32))
    }

    /// 内核视角的 grant 读取:按 ID 解析授权窗口。C 里这是内核从特权结构
    /// 读槽(`do_safecopy` 前的校验);宿主桥接层(VFS↔FS 的内存回放)以
    /// 同一语义取窗后在真地址上直读直写。未用/未知 ID 返回 `None`。
    pub fn probe(&self, grant: i32) -> Option<GrantProbe> {
        if !grant_valid(grant) {
            return None;
        }
        let g = grant_idx(grant) as usize;
        let slot = self.slots.get(g)?;
        if slot.seq as u32 != grant_seq(grant) || !slot.cp_flags().contains(CpFlags::USED) {
            return None;
        }
        let flags = slot.cp_flags();
        // SAFETY: flags 的 USED 提交点保证 direct/magic 联合体臂已定型;
        // INDIRECT 臂在宿主桥接面未使用,按未授权回答。
        unsafe {
            if flags.contains(CpFlags::DIRECT) {
                let d = &slot.u.direct;
                Some(GrantProbe {
                    granter_window: (d.start, d.len),
                    magic: None,
                    writable: flags.contains(CpFlags::WRITE),
                })
            } else if flags.contains(CpFlags::MAGIC) {
                let m = &slot.u.magic;
                Some(GrantProbe {
                    granter_window: (m.start, m.len),
                    magic: Some((m.who_to, m.who_from)),
                    writable: flags.contains(CpFlags::WRITE),
                })
            } else {
                None
            }
        }
    }

    /// Revoke a grant (C: `cpf_revoke` — safecopies.c:218-263). Returns
    /// `Ok(GRANT_FAULTED)` when a CPF_TRY grant saw a soft fault, `Ok(0)`
    /// on success, `Err(EINVAL)` for an unknown/unused ID.
    pub fn revoke(&mut self, grant: i32) -> Result<i32, i32> {
        if !grant_valid(grant) {
            return Err(minix_types::EINVAL);
        }
        let g = grant_idx(grant) as usize;
        let seq = grant_seq(grant);
        // C GID_CHECK_USED: index in range AND the slot's sequence matches
        // the ID (ABA guard) AND the slot is marked used.
        let slot = match self.slots.get(g) {
            Some(s) if s.seq as u32 == seq && (s.cp_flags().contains(CpFlags::USED)) => s,
            _ => return Err(minix_types::EINVAL),
        };
        // C: a TRY grant that faulted reports GRANT_FAULTED on revoke.
        let result = if slot.cp_flags().contains(CpFlags::TRY) && slot.faulted == grant {
            GRANT_FAULTED
        } else {
            0
        };

        // SAFETY: g < slots.len() (checked by `get` above).
        let slot = unsafe { self.slots.get_unchecked_mut(g) };
        // Invalidate: flags word to 0 (clears USED), sequence advances
        // (safecopies.c:245-254 — on revoke, not on allocation, because
        // live update relies on the first allocated grant having ID 0).
        slot.flags = 0;
        slot.seq = if (slot.seq as u32) < GRANT_MAX_SEQ - 1 {
            slot.seq + 1
        } else {
            0
        };
        // Back on the free list (single-headed: last freed, first reused).
        slot.u = CpGrantUnion {
            free: CpGrantFree {
                next: self.freelist,
            },
        };
        self.freelist = g as i32;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syscall::CannedKernelCallTransport;

    #[test]
    fn test_grant_direct_lays_out_c_slot() {
        // C: cpf_grant_direct fills who_to/start/len, then commits via the
        // flags word (access | DIRECT | USED | VALID).
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0); // sys_setgrant OK
        canned.reply(0); // (growth registers once; extra replies harmless)

        let mut table = GrantTable::new();
        let gid = table
            .grant_direct(&canned, 7, 0x4000, 128, CpFlags::READ)
            .expect("first grant");

        assert_eq!(grant_idx(gid), 0, "first allocated grant has index 0");
        assert_eq!(grant_seq(gid), 0, "first allocated grant has sequence 0");
        // SAFETY(test): DIRECT was just set on this slot.
        let slot = unsafe { table.slots[0].cp_direct() };
        assert_eq!((slot.who_to, slot.start, slot.len), (7, 0x4000, 128));
        let flags = table.slots[0].cp_flags();
        assert!(flags.contains(CpFlags::READ | CpFlags::DIRECT | CpFlags::USED | CpFlags::VALID));
    }

    #[test]
    fn test_grant_magic_lays_out_c_slot() {
        // C: cpf_grant_magic fills who_from/who_to/start/len, then commits
        // via the flags word (access | MAGIC | USED | VALID).
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0); // sys_setgrant OK
        canned.reply(0);

        let mut table = GrantTable::new();
        let gid = table
            .grant_magic(
                &canned,
                1, /* FS */
                5, /* user */
                0x4000,
                128,
                CpFlags::WRITE,
            )
            .expect("first grant");
        assert_eq!(grant_idx(gid), 0);
        // SAFETY(test): MAGIC was just set on this slot.
        let slot = unsafe { table.slots[0].cp_magic() };
        assert_eq!(
            (slot.who_to, slot.who_from, slot.start, slot.len),
            (1, 5, 0x4000, 128)
        );
        let flags = table.slots[0].cp_flags();
        assert!(flags.contains(CpFlags::WRITE | CpFlags::MAGIC | CpFlags::USED | CpFlags::VALID));
    }

    fn test_grant_revoke_recycles_slot_with_sequence_bump() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        canned.reply(0);

        let mut table = GrantTable::new();
        let g1 = table
            .grant_direct(&canned, 7, 0x4000, 128, CpFlags::READ)
            .unwrap();
        assert_eq!(table.revoke(g1), Ok(0));

        // Slot 0 is back on the free list (single-headed free list), so the
        // next grant reuses index 0 with sequence +1 (ABA guard, C:245-254).
        let g2 = table
            .grant_direct(&canned, 9, 0x8000, 64, CpFlags::WRITE)
            .unwrap();
        assert_eq!(grant_idx(g2), 0);
        assert_eq!(grant_seq(g2), 1);

        // The stale ID no longer passes the sequence check (EINVAL).
        assert_eq!(table.revoke(g1), Err(minix_types::EINVAL));
        assert_eq!(table.revoke(g2), Ok(0));
    }

    #[test]
    fn test_grant_direct_rejects_unknown_access_bits() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let mut table = GrantTable::new();
        let bad = CpFlags::from_bits_truncate(0x1000).union(CpFlags::READ);
        assert_eq!(
            table.grant_direct(&canned, 7, 0, 0, bad),
            Err(minix_types::EINVAL)
        );
        assert!(table.is_empty(), "被拒的 grant 不得消耗槽位");
    }

    #[test]
    fn test_setgrant_registered_with_kernel() {
        let mut canned = CannedKernelCallTransport::new();
        canned.reply(0);
        let mut table = GrantTable::new();
        table
            .grant_direct(&canned, 7, 0x4000, 8, CpFlags::READ)
            .unwrap();
        // The growth path registered the table (addr = slots base, size).
        let sent = canned.sent.borrow();
        assert_eq!(sent[0].m_type, minix_types::SYS_SETGRANT);
        // SAFETY(test): reading back the recorded setgrant payload.
        let g = unsafe { sent[0].m_u.m_lsys_krn_sys_setgrant };
        assert_eq!(g.size, 2); // first growth doubles 0 → 2 slots (C prealloc(0))
        assert_ne!(g.addr, 0);
    }
}
