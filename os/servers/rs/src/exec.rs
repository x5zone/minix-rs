//! Executable-image loading and sharing.
//!
//! Mirrors `minix3/minix/servers/rs/exec.c` (`srv_execve` — 21,
//! `do_exec` — 62, `exec_restart` — 121, `read_seg` — 143) and
//! `manager.c:1354-1455` (`share_exec` — 1357, `read_exec` — 1372,
//! `free_exec` — 1424). 09-rs-exec.md.
//!
//! The image arrival face (`read_exec`'s `stat`/`open`/`read`/`close`
//! sequence) goes through the VFS client wrappers in `minix-sys`; the
//! remaining kernel/IPC-coupled steps (the `exec_loaders` loop with
//! `libexec_*`, `srv_execve`, the `PM_EXEC_RESTART` message, `sys_datacopy`)
//! are wired through 19-rs-external-interfaces.md (DEFERRED). This module
//! owns the *in-memory image* semantics (ARCH A-5: C `malloc` + pointer
//! sharing → `Arc<[u8]>`) and the pure validation.
//!
//! C reads the image from `rp->r_argv[0]`; that face lives behind
//! [`ExecImageIo`] so the gate/short-read policy is host-testable
//! ([`read_exec`] is the production entry the effect bundles carry).

use crate::process_table::RProcTable;
use crate::service_slot::{ServiceSlot, SlotId};
use crate::slot::build_cmd_dep;
use alloc::sync::Arc;
use minix_sys::Fd;
use minix_types::Errno;

/// Validates that an image starts with a loadable 64-bit ELF header.
///
/// C: `read_exec` — manager.c:1388-1389: `sb.st_size < sizeof(Elf_Ehdr)` →
/// `ENOEXEC`. The minimum-size gate (64 bytes for `Elf64_Ehdr`) plus the
/// magic/class/data checks mirror `libexec_load_elf`'s first validation
/// (lib/libexec/exec_elf.c). ARCH A-8: full parsing (`parse_ehdr`,
/// `segment_iter`) lives in the `minix-elf` crate and is wired when
/// `srv_execve`/`do_exec` land (19).
///
/// awaiting-wiring: the kernel exec face (`srv_execve`/`exec_loaders`) is
/// still 19-rs-external-interfaces.md — until then this validator has no
/// production caller. The image arrival face itself is wired: [`read_exec`].
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
///
/// awaiting-wiring: 13-rs-control-requests.md — the C consumer checks the
/// donor's image sharing in the `RSS_REUSE` path (`edit_slot`/`up` refresh);
/// until that arm goes live this predicate has no production caller.
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

// ── Image arrival face (NS9; C: read_exec — manager.c:1372-1420) ────────

/// `open(2)` read-only flag. C: `O_RDONLY` (`minix3/sys/sys/fcntl.h:64`,
/// `0x00000000`).
const O_RDONLY: i32 = 0x0000;

/// File-system face behind [`read_exec_with`] — the four syscalls C
/// `read_exec` makes, kept as separate methods so the policy (the size
/// gate, the short-read `EIO` verdict, the unconditional `close`) lives in
/// one place. Two behaviorally different impls: the production
/// [`VfsImageIo`] and the test double in this module's tests.
pub trait ExecImageIo {
    /// C: `stat(e_name, &sb)` — manager.c:1385; only `st_size` is consulted
    /// afterwards (manager.c:1388).
    fn stat_size(&mut self, path: &str) -> Result<i64, Errno>;
    /// C: `open(e_name, O_RDONLY)` — manager.c:1392.
    fn open(&mut self, path: &str) -> Result<Fd, Errno>;
    /// C: `read(fd, rp->r_exec, rp->r_exec_len)` — manager.c:1410.
    fn read(&mut self, fd: Fd, buf: &mut [u8]) -> Result<usize, Errno>;
    /// C: `close(fd)` — manager.c:1412; the C code ignores the result.
    fn close(&mut self, fd: Fd);
}

/// Production face: the `minix-sys` VFS client wrappers over the direct-trap
/// transport (the same shape as the DS publish face in `shell_request.rs`).
/// A hosted transport answers `EIO`, so tests substitute their own impl.
pub struct VfsImageIo;

impl ExecImageIo for VfsImageIo {
    fn stat_size(&mut self, path: &str) -> Result<i64, Errno> {
        // C memsets its `struct stat`; this is an all-integer repr(C) type,
        // so zeroed is a valid bit pattern for every field (the init host.rs
        // `path_exists` precedent).
        let mut buf: minix_sys::Stat = unsafe { core::mem::zeroed() };
        minix_sys::stat(path, &mut buf)?;
        Ok(buf.st_size)
    }
    fn open(&mut self, path: &str) -> Result<Fd, Errno> {
        minix_sys::open(path, O_RDONLY, 0)
    }
    fn read(&mut self, fd: Fd, buf: &mut [u8]) -> Result<usize, Errno> {
        minix_sys::read(fd, buf)
    }
    fn close(&mut self, fd: Fd) {
        let _ = minix_sys::close(fd);
    }
}

/// Reads the slot's binary image from its command path into `slot.exec`.
///
/// C: `read_exec` — manager.c:1372-1420. The path is `r_argv[0]`, the first
/// command token (manager.c:1380; `build_cmd_dep`'s N11 empty case yields
/// `""`, whose `stat` fails exactly like C's would). Failure faces:
/// `stat`/`open` propagate the errno; `st_size < 64` → `ENOEXEC`
/// (manager.c:1388-1389, `sizeof(Elf_Ehdr)`); a short read → `EIO`
/// (manager.c:1414-1418, the `r >= 0` branch); a failed read propagates the
/// errno (the `r < 0` branch). `close` runs on both read outcomes with its
/// result ignored, as in C. ARCH: C allocates `rp->r_exec` up front and
/// `free_exec`s it on failure; Rust builds the buffer locally and installs
/// it into `slot.exec` only on success — externally identical (every
/// failure leaves `slot.exec` `None`). C's malloc-failure `ENOMEM` has no
/// Rust counterpart (allocation failure aborts); the `try_from` guard keeps
/// a negative `st_size` on the same errno face.
pub fn read_exec(slot: &mut ServiceSlot) -> Result<(), Errno> {
    read_exec_with(&mut VfsImageIo, slot)
}

/// [`read_exec`] with the file-system face injected (the test seam).
pub fn read_exec_with(io: &mut dyn ExecImageIo, slot: &mut ServiceSlot) -> Result<(), Errno> {
    // C: e_name = rp->r_argv[0] — manager.c:1380. The token is copied out so
    // the borrow of `slot.args` ends before `slot.exec` is written.
    let name = build_cmd_dep(&slot.args).swap_remove(0).to_vec();
    // The minix-sys client face is `&str`-shaped while C stats arbitrary
    // bytes; an invalid-UTF-8 name fails resolution like any other
    // unresolvable name.
    let path = core::str::from_utf8(&name).map_err(|_| Errno::ENOENT)?;

    let len = usize::try_from(io.stat_size(path)?).map_err(|_| Errno::ENOMEM)?;
    // C: sb.st_size < sizeof(Elf_Ehdr) → ENOEXEC — manager.c:1388-1389.
    // Elf64_Ehdr is 64 bytes; shorter files cannot hold a valid header.
    if len < 64 {
        return Err(Errno::ENOEXEC);
    }
    // C: open(e_name, O_RDONLY) — manager.c:1392-1394 (failure → -errno).
    let fd = io.open(path)?;
    // C mallocs exactly st_size (manager.c:1395-1403) — the Vec is
    // zero-initialized where C's malloc bytes are uninitialized; the
    // full-count read below makes the difference unobservable.
    let mut image = alloc::vec![0u8; len];
    // C: one read(2) over the whole buffer with an exact-count comparison
    // (manager.c:1410-1419) — a short read is EIO, not a retry.
    let r = io.read(fd, &mut image);
    io.close(fd); // C: manager.c:1412 — always closed, result ignored.
    match r {
        Ok(n) if n == len => {}
        Ok(_) => return Err(Errno::EIO),
        Err(e) => return Err(e),
    }
    slot.exec = Some(Arc::from(image));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service_slot::{Label, RFlags};

    #[allow(dead_code)] // RSS_COPY share/scan tests' fixture (the kernel exec face is still 19)
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

    // ── read_exec_with (NS9) ────────────────────────────────────────────

    /// In-memory [`ExecImageIo`] double: one file at a fixed size with a
    /// configurable short-read truncation and error injection. The call log
    /// lets the gate-order tests assert `open` is never reached.
    struct FakeImageIo {
        size: i64,
        content: alloc::vec::Vec<u8>,
        /// When set, `read` returns this many bytes (short-read face).
        read_count: Option<usize>,
        /// When set, `read` fails with this errno (the `r < 0` face).
        read_err: Option<Errno>,
        open_err: Option<Errno>,
        stat_err: Option<Errno>,
        opened: bool,
        closed: bool,
    }

    impl FakeImageIo {
        fn file(content: &[u8]) -> Self {
            Self {
                size: content.len() as i64,
                content: content.to_vec(),
                read_count: None,
                read_err: None,
                open_err: None,
                stat_err: None,
                opened: false,
                closed: false,
            }
        }

        fn slot_with(path: &[u8]) -> ServiceSlot {
            let mut s = ServiceSlot::vacant();
            s.args[..path.len()].copy_from_slice(path);
            s
        }
    }

    impl ExecImageIo for FakeImageIo {
        fn stat_size(&mut self, _path: &str) -> Result<i64, Errno> {
            match self.stat_err {
                Some(e) => Err(e),
                None => Ok(self.size),
            }
        }
        fn open(&mut self, _path: &str) -> Result<Fd, Errno> {
            match self.open_err {
                Some(e) => Err(e),
                None => {
                    self.opened = true;
                    Ok(3)
                }
            }
        }
        fn read(&mut self, _fd: Fd, buf: &mut [u8]) -> Result<usize, Errno> {
            if let Some(e) = self.read_err {
                return Err(e);
            }
            let n = self.read_count.unwrap_or(buf.len()).min(buf.len());
            buf[..n].copy_from_slice(&self.content[..n]);
            Ok(n)
        }
        fn close(&mut self, _fd: Fd) {
            self.closed = true;
        }
    }

    /// One Ehdr-sized file (64 bytes — exactly the gate minimum, manager.c
    /// :1388) with the ELF magic/class/data bytes set.
    fn minimal_elf() -> alloc::vec::Vec<u8> {
        let mut v = alloc::vec![0u8; 64];
        v[0..4].copy_from_slice(b"\x7fELF");
        v[4] = 2; // ELFCLASS64
        v[5] = 1; // ELFDATA2LSB
        v
    }

    #[test]
    fn test_read_exec_installs_image_from_argv0() {
        // C: manager.c:1372-1420 happy path — the image comes from argv[0],
        // not the whole command buffer.
        let elf = minimal_elf();
        let mut io = FakeImageIo::file(&elf);
        let mut slot = FakeImageIo::slot_with(b"/sbin/pm --verbose\0");
        assert_eq!(read_exec_with(&mut io, &mut slot), Ok(()));
        assert!(io.opened && io.closed);
        let img = slot.exec.expect("image installed");
        assert_eq!(&img[..], &elf[..]);
    }

    #[test]
    fn test_read_exec_small_file_enoexec_before_open() {
        // C: manager.c:1388-1389 — st_size < sizeof(Elf_Ehdr) → ENOEXEC,
        // with no open attempted (the gate precedes the open).
        let mut io = FakeImageIo::file(&[0u8; 63]);
        let mut slot = FakeImageIo::slot_with(b"/sbin/pm\0");
        assert_eq!(read_exec_with(&mut io, &mut slot), Err(Errno::ENOEXEC));
        assert!(!io.opened);
        assert!(slot.exec.is_none());
    }

    #[test]
    fn test_read_exec_stat_and_open_errors_propagate() {
        let mut io = FakeImageIo::file(&minimal_elf());
        io.stat_err = Some(Errno::ENOENT);
        let mut slot = FakeImageIo::slot_with(b"/sbin/gone\0");
        assert_eq!(read_exec_with(&mut io, &mut slot), Err(Errno::ENOENT));

        let mut io = FakeImageIo::file(&minimal_elf());
        io.open_err = Some(Errno::EPERM);
        let mut slot = FakeImageIo::slot_with(b"/sbin/pm\0");
        assert_eq!(read_exec_with(&mut io, &mut slot), Err(Errno::EPERM));
    }

    #[test]
    fn test_read_exec_empty_command_stats_empty_path() {
        // N11 empty case: argv[0] is "" (build_cmd_dep yields [""], never an
        // empty argv) — the lookup of that empty name fails exactly as C's
        // stat("") would.
        let mut io = FakeImageIo::file(&minimal_elf());
        io.stat_err = Some(Errno::ENOENT);
        let mut slot = FakeImageIo::slot_with(b"\0");
        assert_eq!(read_exec_with(&mut io, &mut slot), Err(Errno::ENOENT));
    }

    #[test]
    fn test_read_exec_short_read_is_eio_and_closes() {
        // C: manager.c:1414-1418 — one read(2), exact-count comparison; a
        // short count is EIO (no retry), and the fd is still closed.
        let elf = minimal_elf();
        let mut io = FakeImageIo::file(&elf);
        io.read_count = Some(elf.len() - 1);
        let mut slot = FakeImageIo::slot_with(b"/sbin/pm\0");
        assert_eq!(read_exec_with(&mut io, &mut slot), Err(Errno::EIO));
        assert!(io.closed);
        assert!(slot.exec.is_none());
    }

    #[test]
    fn test_read_exec_read_error_propagates_errno() {
        // C: the `r < 0` branch returns -errno (manager.c:1419).
        let mut io = FakeImageIo::file(&minimal_elf());
        io.read_err = Some(Errno::EINVAL);
        let mut slot = FakeImageIo::slot_with(b"/sbin/pm\0");
        assert_eq!(read_exec_with(&mut io, &mut slot), Err(Errno::EINVAL));
        assert!(io.closed);
    }

    #[test]
    fn test_read_exec_failure_leaves_prior_exec_alone() {
        // ARCH note's external face: on failure slot.exec keeps whatever the
        // caller had (C frees the fresh buffer; a USE_COPY slot's shared
        // image is never touched by this path).
        let mut io = FakeImageIo::file(&minimal_elf());
        io.read_count = Some(0);
        let mut slot = FakeImageIo::slot_with(b"/sbin/pm\0");
        slot.exec = Some(Arc::from(&b"shared"[..]));
        assert_eq!(read_exec_with(&mut io, &mut slot), Err(Errno::EIO));
        assert_eq!(&*slot.exec.unwrap(), b"shared");
    }
}
