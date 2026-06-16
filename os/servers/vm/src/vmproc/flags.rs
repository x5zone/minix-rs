//! VM process state flags.

use bitflags::bitflags;

bitflags! {
    /// VM process state flags.
    ///
    /// VM states are "multi-dimensional orthogonal", e.g., `IN_USE | EXITING | VM_INSTANCE` can combine.
    /// Not using enum because enum forces mutual exclusion.
    ///
    /// Corresponds to Minix3's `vm_flags_t` in `vmproc.h`.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use minix_vm::VmFlags;
    ///
    /// let flags = VmFlags::IN_USE | VmFlags::VM_INSTANCE;
    /// assert!(flags.contains(VmFlags::IN_USE));
    /// assert!(flags.contains(VmFlags::VM_INSTANCE));
    /// assert!(!flags.contains(VmFlags::EXITING));
    /// ```
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct VmFlags: u8 {
        /// Slot contains a process.
        const IN_USE = 0x001;
        /// PM is cleaning up this process.
        const EXITING = 0x002;
        /// This is a VM process instance.
        const VM_INSTANCE = 0x010;
    }
}

impl Default for VmFlags {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vmflags_basic() {
        let flags = VmFlags::IN_USE;
        assert!(flags.contains(VmFlags::IN_USE));
        assert!(!flags.contains(VmFlags::EXITING));
    }

    #[test]
    fn test_vmflags_combination() {
        let flags = VmFlags::IN_USE | VmFlags::EXITING;
        assert!(flags.contains(VmFlags::IN_USE));
        assert!(flags.contains(VmFlags::EXITING));
        assert!(!flags.contains(VmFlags::VM_INSTANCE));
    }

    #[test]
    fn test_vmflags_helpers() {
        let flags = VmFlags::IN_USE | VmFlags::VM_INSTANCE;
        assert!(flags.contains(VmFlags::IN_USE));
        assert!(!flags.contains(VmFlags::EXITING));
        assert!(flags.contains(VmFlags::VM_INSTANCE));
    }

    #[test]
    fn test_vmflags_default() {
        let flags: VmFlags = Default::default();
        assert!(flags.is_empty());
    }
}
