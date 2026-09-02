//! VM Access Control List (ACL).
//!
//! Implements per-process ACL for VM call permissions.
//!
//! # Design
//!
//! Uses `AclState` enum to express three states from Minix3:
//! - `Uninitialized` → Minix3 `NO_ACL (-1)`: process not yet managed by RS
//! - `Default` → Minix3 `USER_ACL (0)`: shared default permissions for user processes
//! - `System(mask)` → Minix3 `vm_acl >= FIRST_SYS_ACL`: per-system-service permissions
//!
//! The `AclMask` bitflags type uses call numbers from `minix_types::ipc::vm`
//! as bit positions, eliminating duplicate definitions.

use minix_types::{Endpoint, VmError};

use minix_types::{
    VM_RQ_BASE, VM_EXIT, VM_FORK, VM_BRK, VM_EXEC_NEWMEM, VM_WILLEXIT,
    VM_MMAP, VM_ADDDMA, VM_DELDMA, VM_GETDMA, VM_MAP_PHYS, VM_UNMAP_PHYS,
    VM_MUNMAP, VM_MAPCACHEPAGE, VM_SETCACHEPAGE, VM_FORGETCACHEPAGE,
    VM_CLEARCACHE, VM_VFS_REPLY, VM_REMAP, VM_SHM_UNMAP, VM_GETPHYS,
    VM_GETREF, VM_RS_SET_PRIV, VM_INFO, VM_RS_UPDATE, VM_RS_MEMCTL,
    VM_REMAP_RO, VM_PROCCTL, VM_VFS_MMAP, VM_GETRUSAGE, VM_RS_PREPARE,
};

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct AclMask: u64 {
        const VM_EXIT = 1 << (VM_EXIT - VM_RQ_BASE);
        const VM_FORK = 1 << (VM_FORK - VM_RQ_BASE);
        const VM_BRK = 1 << (VM_BRK - VM_RQ_BASE);
        const VM_EXEC_NEWMEM = 1 << (VM_EXEC_NEWMEM - VM_RQ_BASE);
        const VM_WILLEXIT = 1 << (VM_WILLEXIT - VM_RQ_BASE);
        const VM_MMAP = 1 << (VM_MMAP - VM_RQ_BASE);
        const VM_ADDDMA = 1 << (VM_ADDDMA - VM_RQ_BASE);
        const VM_DELDMA = 1 << (VM_DELDMA - VM_RQ_BASE);
        const VM_GETDMA = 1 << (VM_GETDMA - VM_RQ_BASE);
        const VM_MAP_PHYS = 1 << (VM_MAP_PHYS - VM_RQ_BASE);
        const VM_UNMAP_PHYS = 1 << (VM_UNMAP_PHYS - VM_RQ_BASE);
        const VM_MUNMAP = 1 << (VM_MUNMAP - VM_RQ_BASE);
        const VM_MAPCACHEPAGE = 1 << (VM_MAPCACHEPAGE - VM_RQ_BASE);
        const VM_SETCACHEPAGE = 1 << (VM_SETCACHEPAGE - VM_RQ_BASE);
        const VM_FORGETCACHEPAGE = 1 << (VM_FORGETCACHEPAGE - VM_RQ_BASE);
        const VM_CLEARCACHE = 1 << (VM_CLEARCACHE - VM_RQ_BASE);
        const VM_VFS_REPLY = 1 << (VM_VFS_REPLY - VM_RQ_BASE);
        const VM_REMAP = 1 << (VM_REMAP - VM_RQ_BASE);
        const VM_SHM_UNMAP = 1 << (VM_SHM_UNMAP - VM_RQ_BASE);
        const VM_GETPHYS = 1 << (VM_GETPHYS - VM_RQ_BASE);
        const VM_GETREF = 1 << (VM_GETREF - VM_RQ_BASE);
        const VM_RS_SET_PRIV = 1 << (VM_RS_SET_PRIV - VM_RQ_BASE);
        const VM_INFO = 1 << (VM_INFO - VM_RQ_BASE);
        const VM_RS_UPDATE = 1 << (VM_RS_UPDATE - VM_RQ_BASE);
        const VM_RS_MEMCTL = 1 << (VM_RS_MEMCTL - VM_RQ_BASE);
        const VM_REMAP_RO = 1 << (VM_REMAP_RO - VM_RQ_BASE);
        const VM_PROCCTL = 1 << (VM_PROCCTL - VM_RQ_BASE);
        const VM_VFS_MMAP = 1 << (VM_VFS_MMAP - VM_RQ_BASE);
        const VM_GETRUSAGE = 1 << (VM_GETRUSAGE - VM_RQ_BASE);
        const VM_RS_PREPARE = 1 << (VM_RS_PREPARE - VM_RQ_BASE);
    }
}

impl AclMask {
    pub(crate) const DEFAULT: Self = Self::from_bits_truncate(
        Self::VM_EXIT.bits() |
        Self::VM_FORK.bits() |
        Self::VM_BRK.bits() |
        Self::VM_EXEC_NEWMEM.bits() |
        Self::VM_WILLEXIT.bits() |
        Self::VM_MMAP.bits() |
        Self::VM_MUNMAP.bits()
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum AclState {
    #[default]
    Uninitialized,
    Default,
    System(AclMask),
}

impl AclState {
    /// Check whether a process is allowed to make a certain (zero-based) call.
    ///
    /// Corresponds to Minix3's `acl_check()`.
    /// Returns `Ok(())` if allowed, `Err(VmError::PermissionDenied)` if not.
    ///
    /// Takes `endpoint` rather than `&ActiveProc` — ACL checking only depends
    /// on the caller's endpoint (to exempt VM itself) and the call number.
    /// This decouples ACL logic from the process typestate hierarchy.
    pub(crate) fn acl_check(&self, endpoint: Endpoint, call: u32) -> Result<(), VmError> {
        if endpoint == Endpoint::VM {
            return Ok(());
        }

        match self {
            AclState::Uninitialized => {
                // SECURITY FIX [ARCH: A-11]: Restrict to DEFAULT calls instead
                // of allowing all.
                // Minix3's NO_ACL allows all calls ("for now" — acl.c:44-53), but
                // this is a known security relaxation. DEFAULT covers
                // VM_EXIT/VM_FORK/VM_BRK/VM_EXEC_NEWMEM/VM_WILLEXIT/VM_MMAP/
                // VM_MUNMAP — sufficient for early boot (RS needs VM_BRK) and
                // user processes. Privileged calls (VM_MAP_PHYS, VM_RS_SET_PRIV,
                // etc.) require explicit System(AclMask) assignment via acl_set.
                let call_flag = AclMask::from_bits_truncate(1u64 << call);
                if AclMask::DEFAULT.contains(call_flag) {
                    Ok(())
                } else {
                    Err(VmError::PermissionDenied)
                }
            }
            AclState::Default => {
                let call_flag = AclMask::from_bits_truncate(1u64 << call);
                if AclMask::DEFAULT.contains(call_flag) {
                    Ok(())
                } else {
                    Err(VmError::PermissionDenied)
                }
            }
            AclState::System(mask) => {
                let call_flag = AclMask::from_bits_truncate(1u64 << call);
                if mask.contains(call_flag) {
                    Ok(())
                } else {
                    Err(VmError::PermissionDenied)
                }
            }
        }
    }

    /// Assign a call mask to a process.
    ///
    /// Corresponds to Minix3's `acl_set()`.
    /// - User processes (`sys_proc == false`) get `Default` (shared user ACL).
    /// - System processes (`sys_proc == true`) get `System(mask)`.
    ///
    /// Unlike Minix3, there is no shared slot table (`acl_mask[][]` + `acl_inuse`).
    /// Each `System(AclMask)` carries its own mask inline, so:
    /// - Slot allocation is unnecessary (no `acl_inuse` bitmap to search)
    /// - Slot exhaustion is impossible (no `NR_SYS_PROCS` limit on ACL entries)
    /// - Minix3's `printf("VM: no ACL entries available!")` cannot occur
    pub(crate) fn acl_set(sys_proc: bool, mask: Option<AclMask>) -> Self {
        if sys_proc {
            match mask {
                Some(m) => AclState::System(m),
                None => {
                    // Minix3: "WARNING: inheriting uninitialized ACL mask"
                    // In our design, no shared slots to inherit from.
                    AclState::System(AclMask::empty())
                }
            }
        } else {
            AclState::Default
        }
    }

    /// A process has forked. User processes inherit their parent's ACL.
    /// System processes do not inherit an ACL.
    ///
    /// Corresponds to Minix3's `acl_fork()`.
    pub(crate) fn acl_fork(&self) -> Self {
        match self {
            AclState::Uninitialized => AclState::Uninitialized,
            AclState::Default => AclState::Default,
            AclState::System(_) => AclState::Uninitialized,
        }
    }

    /// A process has exited. Mark it as having no ACL.
    ///
    /// Corresponds to Minix3's `acl_clear()`.
    /// Unlike Minix3, there is no shared slot table to free,
    /// so simply returning `Uninitialized` is sufficient.
    // V10-P2-1 (DEFERRED): test-only today — exit clears the ACL via
    // `VmProc::clear()` instead of this path.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn acl_clear(&self) -> Self {
        AclState::Uninitialized
    }

    // V10-P2-1 (DEFERRED): test-only; `mask` can converge with the
    // V9-P2-3 Provider-enum work.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn mask(&self) -> Option<AclMask> {
        match self {
            AclState::Uninitialized => None,
            AclState::Default => Some(AclMask::DEFAULT),
            AclState::System(m) => Some(*m),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A non-VM endpoint for testing normal ACL checks.
    const USER_EP: Endpoint = Endpoint(100);

    #[test]
    fn test_acl_state_default() {
        assert_eq!(AclState::default(), AclState::Uninitialized);
    }

    #[test]
    fn test_acl_check_uninitialized() {
        let state = AclState::Uninitialized;

        // DEFAULT calls are allowed
        assert!(state.acl_check(USER_EP, VM_EXIT - VM_RQ_BASE).is_ok());
        assert!(state.acl_check(USER_EP, VM_FORK - VM_RQ_BASE).is_ok());
        assert!(state.acl_check(USER_EP, VM_BRK - VM_RQ_BASE).is_ok());
        assert!(state.acl_check(USER_EP, VM_MMAP - VM_RQ_BASE).is_ok());
        assert!(state.acl_check(USER_EP, VM_MUNMAP - VM_RQ_BASE).is_ok());

        // Non-DEFAULT calls are denied (security fix: default-deny policy)
        assert!(state.acl_check(USER_EP, VM_MAP_PHYS - VM_RQ_BASE).is_err());
        assert!(state.acl_check(USER_EP, VM_RS_SET_PRIV - VM_RQ_BASE).is_err());
        assert!(state.acl_check(USER_EP, VM_RS_PREPARE - VM_RQ_BASE).is_err());
    }

    #[test]
    fn test_acl_check_vm_proc() {
        let state = AclState::Default;

        // VM endpoint always allowed
        assert!(state.acl_check(Endpoint::VM, 0).is_ok());
        assert!(state.acl_check(Endpoint::VM, 100).is_ok());
    }

    #[test]
    fn test_acl_check_default() {
        let state = AclState::Default;

        assert!(state.acl_check(USER_EP, VM_EXIT - VM_RQ_BASE).is_ok());
        assert!(state.acl_check(USER_EP, VM_FORK - VM_RQ_BASE).is_ok());
        assert!(state.acl_check(USER_EP, VM_BRK - VM_RQ_BASE).is_ok());
        assert!(state.acl_check(USER_EP, VM_MMAP - VM_RQ_BASE).is_ok());
        assert!(state.acl_check(USER_EP, VM_MUNMAP - VM_RQ_BASE).is_ok());

        assert!(state.acl_check(USER_EP, VM_MAP_PHYS - VM_RQ_BASE).is_err());
        assert!(state.acl_check(USER_EP, VM_RS_PREPARE - VM_RQ_BASE).is_err());
    }

    #[test]
    fn test_acl_check_system() {
        let mask = AclMask::VM_MMAP | AclMask::VM_MAP_PHYS | AclMask::VM_RS_PREPARE;
        let state = AclState::System(mask);

        assert!(state.acl_check(USER_EP, VM_MMAP - VM_RQ_BASE).is_ok());
        assert!(state.acl_check(USER_EP, VM_MAP_PHYS - VM_RQ_BASE).is_ok());
        assert!(state.acl_check(USER_EP, VM_RS_PREPARE - VM_RQ_BASE).is_ok());

        assert!(state.acl_check(USER_EP, VM_EXIT - VM_RQ_BASE).is_err());
        assert!(state.acl_check(USER_EP, VM_FORK - VM_RQ_BASE).is_err());
    }

    #[test]
    fn test_acl_fork_default() {
        let state = AclState::Default;
        assert_eq!(state.acl_fork(), AclState::Default);
    }

    #[test]
    fn test_acl_fork_uninitialized() {
        let state = AclState::Uninitialized;
        assert_eq!(state.acl_fork(), AclState::Uninitialized);
    }

    #[test]
    fn test_acl_fork_system() {
        let mask = AclMask::VM_MMAP | AclMask::VM_MAP_PHYS;
        let state = AclState::System(mask);
        assert_eq!(state.acl_fork(), AclState::Uninitialized);
    }

    #[test]
    fn test_acl_set_user() {
        let state = AclState::acl_set(false, None);
        assert_eq!(state, AclState::Default);

        let state = AclState::acl_set(false, Some(AclMask::VM_EXIT));
        assert_eq!(state, AclState::Default);
    }

    #[test]
    fn test_acl_set_system() {
        let mask = AclMask::VM_MMAP | AclMask::VM_MAP_PHYS;
        let state = AclState::acl_set(true, Some(mask));
        assert_eq!(state, AclState::System(mask));

        let state = AclState::acl_set(true, None);
        assert_eq!(state, AclState::System(AclMask::empty()));
    }

    #[test]
    fn test_acl_clear() {
        let state = AclState::Default;
        assert_eq!(state.acl_clear(), AclState::Uninitialized);

        let mask = AclMask::VM_MMAP;
        let state = AclState::System(mask);
        assert_eq!(state.acl_clear(), AclState::Uninitialized);

        let state = AclState::Uninitialized;
        assert_eq!(state.acl_clear(), AclState::Uninitialized);
    }

    #[test]
    fn test_acl_mask_default() {
        let mask = AclMask::DEFAULT;
        assert!(mask.contains(AclMask::VM_EXIT));
        assert!(mask.contains(AclMask::VM_FORK));
        assert!(mask.contains(AclMask::VM_BRK));
        assert!(mask.contains(AclMask::VM_MMAP));
        assert!(mask.contains(AclMask::VM_MUNMAP));
        assert!(!mask.contains(AclMask::VM_MAP_PHYS));
        assert!(!mask.contains(AclMask::VM_RS_PREPARE));
    }

    #[test]
    fn test_acl_state_mask() {
        assert_eq!(AclState::Uninitialized.mask(), None);
        assert_eq!(AclState::Default.mask(), Some(AclMask::DEFAULT));
        let custom = AclMask::VM_MMAP | AclMask::VM_BRK;
        assert_eq!(AclState::System(custom).mask(), Some(custom));
    }
}
