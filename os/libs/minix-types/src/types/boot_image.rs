//! `GET_IMAGE` boot-image-table snapshot — the single Rust authority for
//! the wire layout of the kernel `GET_IMAGE` payload.
//!
//! E-ISPROD: lifted verbatim from `os/kernel/src/misc.rs` (the producer);
//! the IS `image` dump previously kept a `BootImageSnap` subset. Field
//! order mirrors C `struct boot_image` (include/minix/type.h:148-154):
//! proc_nr, proc_name, endpoint, start_addr, len — with `phys_bytes`
//! widened to `u64` (LP64 rewrite precedent, proc_info.rs).

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BootImageStruct {
    /// C: `proc_nr` — process number to use.
    pub proc_nr: i32,
    /// C: `proc_name[PROC_NAME_LEN]` — name in process table.
    /// (C: `PROC_NAME_LEN` = 16 — type.h:145; re-exports via `boot`.)
    pub proc_name: [u8; crate::types::boot::PROC_NAME_LEN],
    /// C: `endpoint` — endpoint number when started.
    pub endpoint: i32,
    /// C: `start_addr` — physical address of the process image.
    pub start_addr: u64,
    /// C: `len` — length of the process image in bytes.
    pub len: u64,
}

impl Default for BootImageStruct {
    fn default() -> Self {
        Self {
            proc_nr: 0,
            proc_name: [0; crate::types::boot::PROC_NAME_LEN],
            endpoint: 0,
            start_addr: 0,
            len: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::offset_of;

    /// Wire layout frozen: field order mirrors C struct boot_image.
    #[test]
    fn test_boot_image_layout_frozen() {
        assert_eq!(core::mem::size_of::<BootImageStruct>(), 40);
        assert_eq!(offset_of!(BootImageStruct, proc_nr), 0);
        assert_eq!(offset_of!(BootImageStruct, proc_name), 4);
        assert_eq!(offset_of!(BootImageStruct, endpoint), 20);
        assert_eq!(offset_of!(BootImageStruct, start_addr), 24);
        assert_eq!(offset_of!(BootImageStruct, len), 32);
    }
}
