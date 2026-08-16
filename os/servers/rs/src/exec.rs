//! Executable-image loading and sharing.
//!
//! Mirrors `minix3/minix/servers/rs/exec.c` (`srv_execve` — 21,
//! `do_exec` — 62, `exec_restart` — 121, `read_seg` — 143) and
//! `manager.c:1354-1455` (`share_exec` — 1357, `read_exec` — 1372,
//! `free_exec` — 1424). 09-rs-exec.md.
//!
//! The kernel/IPC-coupled steps (`stat`/`open`/`read`, the `exec_loaders`
//! loop with `libexec_*`, the `PM_EXEC_RESTART` message, `sys_datacopy`)
//! are wired through 19-rs-external-interfaces.md (DEFERRED). This module
//! owns the *in-memory image* semantics (ARCH A-5: C `malloc` + pointer
//! sharing → `Arc<[u8]>`) and the pure validation.

use crate::process_table::RProcTable;
use crate::service_slot::{ServiceSlot, SlotId};
use alloc::sync::Arc;
use minix_types::Errno;

/// Validates that an image starts with a loadable 64-bit ELF header.
///
/// C: `read_exec` — manager.c:1388-1389: `sb.st_size < sizeof(Elf_Ehdr)` →
/// `ENOEXEC`. The minimum-size gate (64 bytes for `Elf64_Ehdr`) plus the
/// magic/class/data checks mirror `libexec_load_elf`'s first validation
/// (lib/libexec/exec_elf.c). ARCH A-8: full parsing (`parse_ehdr`,
/// `segment_iter`) lives in the `minix-elf` crate and is wired when
/// `srv_execve`/`do_exec` land (19).
pub fn validate_image(image: &[u8]) -> Result<(), Errno> {
    // Elf64_Ehdr is 64 bytes; shorter images cannot hold a valid header.
    if image.len() < 64 {
        return Err(Errno::ENOEXEC);
    }
    if image.len() >= 4 && &image[0..4] == b"\x7fELF" {
        // ELFCLASS64 + ELFDATA2LSB (exec_elf.c magic checks).
        if image.len() >= 6 && image[4] == 2 && image[5] == 1 {
            return Ok(());
        }
    }
    Err(Errno::ENOEXEC)
}

/// Shares the exec image from `src` to `dst`.
///
/// C: `share_exec` — manager.c:1357-1367: `rp_dst->r_exec = rp_src->r_exec`
/// (pointer sharing, `RSS_REUSE` path). Rust: `Arc::clone` — both slots
/// strong-count the same buffer (ARCH A-5). The image length is inherent in
/// the `[u8]` slice, replacing the duplicated `r_exec_len` (manager.c:1365).
pub fn share_exec(dst: &mut ServiceSlot, src: &ServiceSlot) {
    dst.exec = src.exec.clone();
}

/// Whether another in-use slot shares `rp`'s exec image.
///
/// C: `free_exec`'s full-table scan — manager.c:1433-1440
/// (`other_rp->r_exec == rp->r_exec`). Rust uses `Arc::ptr_eq` on the same
/// scan (ARCH A-5); an O(1) `Arc::strong_count > 1` alternative exists but
/// counts clones outside the table too, so the scan is the faithful default.
pub fn has_shared_exec(rp: &ServiceSlot, table: &RProcTable) -> bool {
    let Some(img) = &rp.exec else {
        return false;
    };
    table.iter_in_use().any(|(_, other)| {
        !core::ptr::eq(other as *const ServiceSlot, rp as *const ServiceSlot)
            && other.exec.as_ref().is_some_and(|o| Arc::ptr_eq(o, img))
    })
}

/// Frees an exec image unless another slot shares it.
///
/// C: `free_exec` — manager.c:1424-1455: scan for a sharer; if none, `free`
/// the buffer (manager.c:1443-1446); then `r_exec = NULL; r_exec_len = 0`
/// (manager.c:1453-1454). Rust: dropping the last `Arc` frees the buffer;
/// no explicit scan is needed here — `has_shared_exec` (the C manager.c:1433-1440
/// scan) is kept as an explicit API for the RSS_REUSE/restart paths and tests.
pub fn free_exec(table: &mut RProcTable, rp_id: SlotId) {
    if table.get(rp_id).exec.is_none() {
        return;
    }
    // Dropping the slot's `Arc` frees the buffer iff it is the last holder
    // (C: scan-then-free, manager.c:1433-1446).
    table.get_mut(rp_id).exec = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service_slot::{Label, RFlags};

    fn slot_with_image(image: &'static [u8]) -> ServiceSlot {
        let mut s = ServiceSlot::vacant();
        s.flags = RFlags::IN_USE;
        s.pub_.proc_name = Label::from_bytes(b"tty");
        s.exec = Some(Arc::from(image));
        s
    }

    #[test]
    fn test_validate_image_rejects_tiny() {
        assert_eq!(validate_image(&[0u8; 4]), Err(Errno::ENOEXEC));
    }

    #[test]
    fn test_validate_image_accepts_minimum_ehdr() {
        let mut img = vec![0u8; 64];
        img[0..4].copy_from_slice(b"\x7fELF");
        img[4] = 2; // ELFCLASS64
        img[5] = 1; // ELFDATA2LSB
        assert!(validate_image(&img).is_ok());
    }

    #[test]
    fn test_share_exec_clones() {
        let mut t = RProcTable::new();
        // alloc_slot does not set IN_USE (C: manager.c:2067-2083) — the
        // caller marks the slot; marking between allocs yields distinct ids.
        let a = t.alloc_slot().unwrap();
        t.get_mut(a).flags |= RFlags::IN_USE;
        let b = t.alloc_slot().unwrap();
        t.get_mut(b).flags |= RFlags::IN_USE;
        let img: &'static [u8] = &[1, 2, 3, 4];
        t.get_mut(a).exec = Some(Arc::from(img));
        let src = t.get(a).clone();
        share_exec(t.get_mut(b), &src);
        assert!(has_shared_exec(t.get(a), &t));
        assert!(has_shared_exec(t.get(b), &t));
    }

    #[test]
    fn test_free_exec_exclusive() {
        let mut t = RProcTable::new();
        let a = t.alloc_slot().unwrap();
        t.get_mut(a).exec = Some(Arc::from(&[1u8, 2][..]));
        // Single holder → freed (None).
        let img = t.get(a).exec.clone();
        free_exec(&mut t, a);
        assert!(t.get(a).exec.is_none());
        // The Arc outlives the slot only in this test variable.
        let _ = img;
    }

    #[test]
    fn test_free_exec_shared_keeps_other() {
        let mut t = RProcTable::new();
        let a = t.alloc_slot().unwrap();
        t.get_mut(a).flags |= RFlags::IN_USE;
        let b = t.alloc_slot().unwrap();
        t.get_mut(b).flags |= RFlags::IN_USE;
        let img: &'static [u8] = &[9, 8, 7];
        t.get_mut(a).exec = Some(Arc::from(img));
        t.get_mut(b).pub_.proc_name = Label::from_bytes(b"vm");
        let src = t.get(a).clone();
        share_exec(t.get_mut(b), &src);
        // Freeing a keeps b's reference alive.
        free_exec(&mut t, a);
        assert!(t.get(a).exec.is_none());
        assert!(t.get(b).exec.is_some());
    }
}
