//! `GET_PRIVTAB` privilege-table snapshot — the single Rust authority for
//! the wire layout of kernel `GET_PRIVTAB`/`GET_PRIV` payloads.
//!
//! E-ISPROD: this struct was lifted verbatim from `os/kernel/src/misc.rs`
//! (the producer). The kernel emits `NR_SYS_PROCS` of these per
//! `GET_PRIVTAB` (chunked per-entry copies, do_getinfo.c:132-150), and the
//! Information Server's `privileges` dump interprets the bytes — previously
//! the IS side kept a separate `KPrivSnap` whose widths diverged (i16
//! flags, `[u32; 2]` ipc map vs the producer's u32/u64).
//!
//! Not a mirror of C `struct priv` (kernel/priv.h:21-61): it is the
//! minix-rs wire contract — a used-fields subset. Widths follow the kernel
//! producer (LP64 rewrite precedent, proc_info.rs): C's `short` flag
//! fields widen to `u32` and C's `bitchunk_t s_ipc_to[2]` collapses to one
//! `u64` word (the IS dump splits it back into two `%08x` chunks,
//! dmp_kernel.c:284-286).

/// C: `SYS_CALL_MASK_SIZE = BITMAP_CHUNKS(NR_SYS_CALLS) = 2` — com.h:272.
pub const SYS_CALL_MASK_SIZE: usize = 2;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PrivInfoStruct {
    /// C: `s_proc_nr` — process number this privilege belongs to.
    pub s_proc_nr: i32,
    /// C: `s_id` — privilege ID.
    pub s_id: i32,
    /// C: `s_flags` — privilege flags.
    pub s_flags: u32,
    /// C: `s_trap_mask` — allowed traps.
    pub s_trap_mask: u32,
    /// C: `s_grant_entries` — number of grant entries.
    pub s_grant_entries: i32,
    /// C: `s_sig_mgr` — signal manager endpoint.
    pub s_sig_mgr: i32,
    /// C: `s_notify_pending` — pending notifications.
    pub s_notify_pending: u64,
    /// C: `s_sig_pending` — pending signals.
    pub s_sig_pending: u64,
    /// C: `s_ipc_to` — bitmap of endpoints allowed to send to.
    pub s_ipc_to: u64,
    /// C: `s_k_call_mask` — allowed kernel calls ([`SYS_CALL_MASK_SIZE`]).
    pub s_k_call_mask: [u32; SYS_CALL_MASK_SIZE],
}

impl Default for PrivInfoStruct {
    /// Zeroed except `s_proc_nr = -1` — the kernel's unset-process
    /// sentinel (kernel misc.rs `impl Default for PrivInfoStruct`).
    fn default() -> Self {
        Self {
            s_proc_nr: -1,
            s_id: 0,
            s_flags: 0,
            s_trap_mask: 0,
            s_grant_entries: 0,
            s_sig_mgr: 0,
            s_notify_pending: 0,
            s_sig_pending: 0,
            s_ipc_to: 0,
            s_k_call_mask: [0; SYS_CALL_MASK_SIZE],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::offset_of;

    /// Wire layout frozen: field order follows the kernel producer's
    /// declaration order (E-ISPROD single authority).
    #[test]
    fn test_priv_info_layout_frozen() {
        assert_eq!(core::mem::size_of::<PrivInfoStruct>(), 56);
        assert_eq!(offset_of!(PrivInfoStruct, s_proc_nr), 0);
        assert_eq!(offset_of!(PrivInfoStruct, s_id), 4);
        assert_eq!(offset_of!(PrivInfoStruct, s_flags), 8);
        assert_eq!(offset_of!(PrivInfoStruct, s_trap_mask), 12);
        assert_eq!(offset_of!(PrivInfoStruct, s_grant_entries), 16);
        assert_eq!(offset_of!(PrivInfoStruct, s_sig_mgr), 20);
        assert_eq!(offset_of!(PrivInfoStruct, s_notify_pending), 24);
        assert_eq!(offset_of!(PrivInfoStruct, s_sig_pending), 32);
        assert_eq!(offset_of!(PrivInfoStruct, s_ipc_to), 40);
        assert_eq!(offset_of!(PrivInfoStruct, s_k_call_mask), 48);
    }

    /// The unset-process sentinel matches the kernel's manual Default.
    #[test]
    fn test_priv_info_default_sentinel() {
        let p = PrivInfoStruct::default();
        assert_eq!(p.s_proc_nr, -1);
        assert_eq!(p.s_k_call_mask, [0; SYS_CALL_MASK_SIZE]);
    }
}
