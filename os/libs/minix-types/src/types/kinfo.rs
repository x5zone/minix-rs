//! `GET_KINFO` kernel-info snapshot — the single Rust authority for the
//! wire layout of the kernel `GET_KINFO` payload.
//!
//! E-ISPROD: replaces the kernel's former M4-message-field encoding
//! (nr_procs/nr_tasks/user_sp/freepde_start/vir_kern_start packed into
//! `MessageM4` registers) with a struct copy — C parity: do_getinfo.c
//! GET_KINFO copies `sizeof(kinfo)` from the kernel's `kinfo` struct, and
//! the IS `kernel` dump reads `release`/`version`/`nr_procs`/`nr_tasks`
//! (dmp_kernel.c) which the register form could not carry.
//!
//! Not a mirror of C `struct kinfo` (include/minix/param.h:14-54): it is
//! the used-fields subset — the multiboot blobs, `param_buf` and the
//! `kmessages` pointer stay kernel-private. Field order preserves C's
//! relative order (freepde_start < user_sp < vir_kern_start < nr_procs <
//! nr_tasks < release < version). `vir_bytes` fields widen to `u64`
//! (LP64 rewrite precedent, proc_info.rs).

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct KinfoStruct {
    /// C: `freepde_start` — lowest PDE unused by the kernel mapping
    /// (pre_init.c:233).
    pub freepde_start: i64,
    /// C: `user_sp` — where the kernel wants the user stack set
    /// (pre_init.c:156 USR_STACKTOP).
    pub user_sp: i64,
    /// C: `vir_kern_start` — kernel address space start (pre_init.c:113).
    pub vir_kern_start: i64,
    /// C: `nr_procs` — number of user processes.
    pub nr_procs: i32,
    /// C: `nr_tasks` — number of kernel tasks.
    pub nr_tasks: i32,
    /// C: `release[6]` — kernel release number (config.h OS_RELEASE).
    pub release: [u8; 6],
    /// C: `version[6]` — kernel version number.
    pub version: [u8; 6],
    /// Padding to align the struct to 8 bytes.
    pub _padding: [u8; 4],
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::offset_of;

    /// Wire layout frozen: field order preserves C kinfo relative order.
    #[test]
    fn test_kinfo_layout_frozen() {
        assert_eq!(core::mem::size_of::<KinfoStruct>(), 48);
        assert_eq!(offset_of!(KinfoStruct, freepde_start), 0);
        assert_eq!(offset_of!(KinfoStruct, user_sp), 8);
        assert_eq!(offset_of!(KinfoStruct, vir_kern_start), 16);
        assert_eq!(offset_of!(KinfoStruct, nr_procs), 24);
        assert_eq!(offset_of!(KinfoStruct, nr_tasks), 28);
        assert_eq!(offset_of!(KinfoStruct, release), 32);
        assert_eq!(offset_of!(KinfoStruct, version), 38);
    }
}
