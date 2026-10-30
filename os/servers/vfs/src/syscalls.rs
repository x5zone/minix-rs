//! Syscall binding layer — the exhaustive 64-arm match on `VfsCallNum`.
//!
//! C's `do_work` (main.c) reaches each `do_xxx` handler directly through the
//! `call_vec[call_index]()` function pointer. The Rust counterpart is this
//! module's exhaustive [`dispatch_syscall`]: each arm decodes the fields of
//! `current_message` (the `lc_vfs_*` layout follows the `ipc.h` offsets, read
//! through the `m_m7` union view), calls the owning module's decision
//! function, and produces a [`SyscallResult`]. There is no wildcard arm —
//! a new call number must state its own position explicitly.
//!
//! Faithful-port boundary: calls that need an FS/driver conversation are
//! explicitly refused with `SyscallResult::Nosys` (fail-closed) until the W1
//! transport is live; each arm names its dependency. Once the transport is
//! powered, the conversation is wired in at the same arm (honest contract).
extern crate alloc;


use crate::call_table::{SyscallResult, VfsCallNum};
use crate::path::PathFetcher as _;
use crate::filedes::{close_fd, Fd};
use crate::open::{seek_pos, S_IFMT, S_IFIFO, Whence};
use crate::main_loop::VfsState;

use crate::vnode::VnodeId;

/// The binding layer for `do_work` (`main.c:283-294`) — all 64 arms are
/// exhaustive, with no wildcard.
///
/// Decoding convention: the `lc_vfs_*` message layout follows the field
/// offsets of C's `ipc.h`, read through the `m_m7` union view (`off_t` is
/// reassembled from two adjacent `i32`s, low 32 bits first).
/// Transfer start position: with `O_APPEND` it is the file size (C
/// read.c:234), otherwise the filp position (`position = f->filp_pos` at
/// :145).
///
/// Kept as a pure function because it takes effect before this round's two
/// gates (grant/mount); under the host build the later registration is
/// unreachable, so this is the only testable spot — and it keeps C's two
/// sources side by side.
pub(crate) const fn transfer_position(flags: i32, filp_pos: i64, vnode_size: u64) -> i64 {
    if (flags as u32) & crate::fcntl::O_APPEND != 0 {
        vnode_size as i64
    } else {
        filp_pos
    }
}

/// Character-device routing for the read/write arms (C `read_write`'s
/// `S_ISCHR` branch, read.c:162-217 → `cdev_io`, cdev.c:277-341): `/dev/tty`
/// redirect (`cdev_get` → `cdev_map`) → dmap driver lookup by major (none →
/// **EIO**; the lookup gate on read/write is a runtime fault surface, unlike
/// open/close's `ENXIO`) → crossed magic grant (read pairs `CPF_WRITE`,
/// write pairs `CPF_READ`, via `grant_dir`) → `CDEV_READ`/`CDEV_WRITE`
/// message → driver suspension, with the position advanced optimistically
/// and written back to the filp (same as the FIXME at read.c:216: on
/// suspension the position moves by the requested length, the driver's reply
/// carries the actual count, and VFS does not rewrite the position).
///
/// `filp_idx` is the filp-table index used for position advancement; `sdev`
/// is the vnode's special device number; `write` selects the direction.
/// Under the host build driver sends are unreachable, so the success path is
/// only reachable on real hardware (the same honest note as the ioctl arm).
pub(crate) fn cdev_data_route(
    state: &mut VfsState,
    fp_slot: minix_types::UserSlot,
    filp_idx: usize,
    sdev: u64,
    buf: u64,
    len: u64,
    write: bool,
) -> SyscallResult {
    // C `major(x)`/`minor(x)` (sys/types.h:290-292) — the same bit folding as
    // the ioctl arm; recomputed after a redirect with the new device number.
    let is_ctty = ((sdev & 0x000fff00) >> 8) as u32 == crate::device_map::CTTY_MAJOR;
    let major_valid = (((sdev & 0x000fff00) >> 8) as usize) < crate::device_map::NR_DEVICES;
    let fp_tty = state
        .fproc_table
        .get(fp_slot)
        .map(|fp| fp.tty)
        .filter(|t| *t != minix_types::NO_DEV);
    let dev = match crate::cdev::tty_redirect(sdev, is_ctty, fp_tty, major_valid) {
        crate::cdev::RedirectVerdict::Keep(d) | crate::cdev::RedirectVerdict::Substitute(d) => d,
        // C `cdev_map` returns `NO_DEV` → `cdev_get` gives NULL → `cdev_io`
        // returns EIO (cdev.c:292-293; not the ENXIO of the open/close path).
        crate::cdev::RedirectVerdict::NoDev => {
            return SyscallResult::Error(minix_types::EIO);
        }
    };
    let major = ((dev & 0x000fff00) >> 8) as u32;
    let minor = (((dev & 0xfff0_0000) >> 12) | (dev & 0xff)) as u32;
    let drv_e = match crate::device_map::get_by_major(&state.dmap_table, major)
        .and_then(|row| row.driver)
    {
        Some(e) => e,
        None => return SyscallResult::Error(minix_types::EIO),
    };
    let Some(worker) = state.current_worker else {
        return SyscallResult::Error(minix_types::EAGAIN);
    };
    let user_e = state
        .fproc_table
        .get(fp_slot)
        .map(|fp| fp.endpoint)
        .unwrap_or(minix_types::Endpoint::NONE);
    // Data magic grant: read = the driver **writes** into the user buffer,
    // write = the driver **reads** from the user buffer (the crossed
    // direction pairing at cdev.c:306-308).
    let access = minix_types::CpFlags::from_bits_truncate(crate::cdev::grant_dir(!write))
        | minix_types::CpFlags::TRY;
    let grant = match state.grant_user_buffer(drv_e, user_e, buf, len, access) {
        Ok(g) => g,
        Err(_) => return SyscallResult::Error(minix_types::EIO),
    };
    let mut m = minix_types::Message {
        // The message number is base value + ordinal (0x402/0x403) — `as i32`
        // only yields the enum ordinal, so `message_type()` must be used.
        m_type: if write {
            minix_chardriver::protocol::CdevRequest::Write.message_type()
        } else {
            minix_chardriver::protocol::CdevRequest::Read.message_type()
        },
        ..minix_types::Message::default()
    };
    // SAFETY: `mess_vfs_lchardriver_readwrite { off_t pos@0; cp_grant_id_t
    // grant@8; size_t count@16; unsigned long request@24; int flags@32;
    // endpoint_t id@36; endpoint_t user@40; devminor_t minor@44 }`
    // (ipc.h:2238-2249). `id = proc_e` (cdev.c:326) — the driver's reply
    // carries it back verbatim and VFS's reply routing identifies the process by it.
    unsafe {
        let raw = &mut m.m_u.raw;
        let pos = state
            .filp_table
            .get(crate::filp::FilpId(filp_idx))
            .map(|f| f.pos)
            .unwrap_or(0);
        raw[0..8].copy_from_slice(&pos.to_le_bytes());
        raw[8..12].copy_from_slice(&grant.to_le_bytes());
        raw[16..24].copy_from_slice(&len.to_le_bytes());
        let mut flags = 0i32;
        if state
            .filp_table
            .get(crate::filp::FilpId(filp_idx))
            .is_some_and(|f| f.flags & (crate::fcntl::O_NONBLOCK as i32) != 0)
        {
            flags |= minix_chardriver::protocol::CDEV_NONBLOCK;
        }
        raw[32..36].copy_from_slice(&flags.to_le_bytes());
        raw[36..40].copy_from_slice(&user_e.0.to_le_bytes());
        raw[40..44].copy_from_slice(&user_e.0.to_le_bytes());
        raw[44..46].copy_from_slice(&(minor as u16).to_le_bytes());
    }
    if let Some(wp) = state.worker_pool.get_mut(worker) {
        wp.cont = Some(crate::worker::WorkerCont::CdevIo { grant });
    }
    match state.send_drv_for_slot(worker, Some(fp_slot), drv_e, &m) {
        Ok(()) => {
            // Optimistic advancement from C read.c:216 (FIXME text preserved):
            // on suspension the position moves forward by the requested length,
            // and `f->filp_pos` is committed accordingly at the end of
            // read_write (:261); the actual count is returned directly to the
            // user by the driver's reply and never rewrites the position.
            if let Some(f) = state.filp_table.get_mut(crate::filp::FilpId(filp_idx)) {
                f.pos = f.pos.saturating_add(len as i64);
            }
            // C cdev.c:336-339: `fp_cdev { dev, endpt, grant }` +
            // `FP_BLOCKED_ON_CDEV` — driver-death cleanup (`ReviveEio` side of
            // `driver_vanish_plan`) and MIB's `fpl_task` both rely on it.
            if let Some(fp) = state.fproc_table.get_mut(fp_slot) {
                fp.blocked_on = crate::fproc::BlockedOn::Cdev(crate::fproc::CdevBlock {
                    dev,
                    endpt: drv_e,
                    grant: Some(grant),
                });
            }
            SyscallResult::Suspend
        }
        Err(e) => {
            // [SD19-gate] Best-effort grant revocation: the primary operation has already succeeded or entered the error path, so a failed revocation does not change its result
            #[allow(clippy::let_underscore_must_use)]
            let _ = state.revoke_grant(grant);
            SyscallResult::Error(e)
        }
    }
}

pub fn dispatch_syscall(state: &mut VfsState, call: VfsCallNum) -> SyscallResult {
    let msg = state.current_message;
    // Reading the union matches C's union semantics (same convention as VfsPmInit decode).
    let m7 = unsafe { &msg.m_u.m_m7 };

    match call {
        // ── Table-local semantics: decision complete, execute directly ──
        VfsCallNum::Close => {
            // lc_vfs_close: fd @0 (ipc.h:620-627).
            let fd = match Fd::new(m7.m7i1.max(0) as usize) {
                Some(fd) => fd,
                None => return SyscallResult::Error(minix_types::EBADF),
            };
            let fp_slot = match state.current_fp_slot {
                Some(slot) => slot,
                None => return SyscallResult::Error(minix_types::EINVAL),
            };

            // Prefetch the identity needed for lock release (C `close_fd:702`'s `vp = rfilp->filp_vno`).
            let (filp_vid, fp_pid) = {
                let fp = match state.fproc_table.get(fp_slot) {
                    Some(fp) => fp,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                let filp_idx = match fp.filps[fd.get()] {
                    Some(idx) => idx,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                let pid = fp.pid;
                let vid = state
                    .filp_table
                    .get(crate::filp::FilpId(filp_idx))
                    .and_then(|f| f.vnode)
                    .map(crate::vnode::VnodeId);
                (vid, pid)
            };

            // C `close_fd:496-505`: the last close (`--filp_count == 0`) must
            // `put_vnode` the filp's vnode (the slow path sends REQ_PUTNODE).
            let last_close = {
                let filp_idx = state
                    .fproc_table
                    .get(fp_slot)
                    .and_then(|fp| fp.filps[fd.get()]);
                match filp_idx {
                    Some(idx) => state
                        .filp_table
                        .get(crate::filp::FilpId(idx))
                        .map(|f| f.count == 1)
                        .unwrap_or(false),
                    None => false,
                }
            };
            let fp = match state.fproc_table.get_mut(fp_slot) {
                Some(fp) => fp,
                None => return SyscallResult::Error(minix_types::EINVAL),
            };
            match close_fd(fp, fd, &mut state.filp_table) {
                Ok(()) => {}
                Err(e) => return SyscallResult::Error(e.to_errno()),
            }
            if last_close
                && let Some(vid) = filp_vid
            {
                // [SD19-gate] Best-effort vnode put/reference-count teardown (C put_vnode is void and cannot report an error)
                #[allow(clippy::let_underscore_must_use)]
                let _ = state.put_vnode_deferred(vid);
            }

            // POSIX record-lock release (C `close_fd:700-713`): all locks on
            // the closed file's vnode that belong to this process are released
            // and lock_revive fires.
            if let Some(vid) = filp_vid
                && let Some(v) = state.vnode_table.get(vid)
            {
                let key = crate::fcntl::VnodeKey { fs: v.fs, ino: v.ino };
                let pid = fp_pid as u32;
                let _released = state.lock_table.release_for(key, pid);
                // The lock_revive wakeup broadcast to select/lock waiters is not modeled here.
            }

            SyscallResult::Ok(0)
        }
        VfsCallNum::Lseek => {
            // lc_vfs_lseek (ipc.h:725-731): off_t offset @0, int fd @8,
            // int whence @12. The pure arithmetic decision is in `actual_lseek` (open.rs seek_pos).
            let offset = (i64::from(m7.m7i2) << 32) | i64::from(m7.m7i1);
            let fd = match Fd::new(m7.m7i3.max(0) as usize) {
                Some(fd) => fd,
                None => return SyscallResult::Error(minix_types::EBADF),
            };
            let whence = match m7.m7i4 {
                0 => Whence::Set,
                1 => Whence::Cur,
                2 => Whence::End,
                _ => return SyscallResult::Error(minix_types::EINVAL),
            };
            let fp_slot = match state.current_fp_slot {
                Some(slot) => slot,
                None => return SyscallResult::Error(minix_types::EINVAL),
            };
            let fp = match state.fproc_table.get_mut(fp_slot) {
                Some(fp) => fp,
                None => return SyscallResult::Error(minix_types::EINVAL),
            };
            let filp_idx = match fp.filps[fd.get()] {
                Some(idx) => idx,
                None => return SyscallResult::Error(minix_types::EBADF),
            };
            let filp = match state.filp_table.get_mut(crate::filp::FilpId(filp_idx)) {
                Some(f) => f,
                None => return SyscallResult::Error(minix_types::EBADF),
            };
            // SEEK_END needs the vnode size; only allowed for non-FIFO (open.rs seek_pos).
            let (cur, size, is_fifo) = match filp.vnode
                .and_then(|vid| state.vnode_table.get(VnodeId(vid)))
            {
                Some(v) => (f_pos_of(filp), v.size as i64, (v.mode & S_IFMT) == S_IFIFO),
                None => return SyscallResult::Error(minix_types::EBADF),
            };
            match seek_pos(whence, cur, size, offset, is_fifo) {
                Ok((new_pos, inhibit)) => {
                    filp.pos = new_pos;
                    // C `do_lseek` writes the new position into the reply payload
                    // (`m_vfs_lc_lseek.offset`, open.c:665-666) — both exits
                    // (with or without the read-ahead inhibit) must carry it.
                    let mut reply = minix_types::Message {
                        m_type: minix_types::OK,
                        ..minix_types::Message::default()
                    };
                    // SAFETY: `mess_vfs_lc_lseek { off_t offset; }`
                    // (ipc.h:2206-2210) starts the payload area.
                    unsafe {
                        reply.m_u.raw[0..8].copy_from_slice(&new_pos.to_le_bytes());
                    }
                    let target = state
                        .fproc_table
                        .get(fp_slot)
                        .map(|fp| fp.endpoint)
                        .unwrap_or(minix_types::Endpoint::NONE);
                    if !inhibit {
                        // Position unchanged: do not disturb the FS (C open.c:639).
                        state.queue_reply_msg(target, reply);
                        return SyscallResult::Ok(0);
                    }
                    // Position changed: send `REQ_INHIBREAD` (inhibit read-ahead)
                    // then reply — the `InhibRead` continuation carries the new position (C :642-645).
                    let fs_e = match state
                        .vnode_table
                        .get(VnodeId(
                            state.filp_table.get(crate::filp::FilpId(filp_idx)).unwrap().vnode.unwrap(),
                        ))
                        .map(|v| (v.fs, v.ino))
                    {
                        Some(pair) => pair,
                        None => return SyscallResult::Error(minix_types::EBADF),
                    };
                    let vmnt_id = match state.vmnt_table.find_by_fs(fs_e.0) {
                        Some(v) => v,
                        None => return SyscallResult::Error(minix_types::EIO),
                    };
                    let Some(worker) = state.current_worker else {
                        return SyscallResult::Error(minix_types::EAGAIN);
                    };
                    if let Some(wp) = state.worker_pool.get_mut(worker) {
                        wp.cont = Some(crate::worker::WorkerCont::InhibRead { offset: new_pos });
                    }
                    state.pending_fs = Some(crate::main_loop::PendingFs {
                        vmnt: vmnt_id.0,
                        fs_e: fs_e.0,
                        worker,
                        grant: 0, // no data plane
                        user: target,
                        req: crate::request::encode_inhibread(fs_e.1),
                    });
                    SyscallResult::Suspend
                }
                Err(e) => SyscallResult::Error(e.to_errno()),
            }
        }
        VfsCallNum::Umask => {
            // `do_umask` (protect.c:186-190): `mask &= 0777`, then swap in the old value.
            let fp_slot = match state.current_fp_slot {
                Some(slot) => slot,
                None => return SyscallResult::Error(minix_types::EINVAL),
            };
            let fp = match state.fproc_table.get_mut(fp_slot) {
                Some(fp) => fp,
                None => return SyscallResult::Error(minix_types::EINVAL),
            };
            let old = fp.umask;
            fp.umask = (m7.m7i1 as u32) & 0o777;
            SyscallResult::Ok(old as i32)
        }
        VfsCallNum::Getrusage => SyscallResult::Ok(0), // Obsolete call, always OK (misc.c:1005)

        // ── FS conversation family: req_* submitted through the fs_comm window, enabled once the W1 transport is live ──
        | VfsCallNum::Mount
        | VfsCallNum::Umount
        | VfsCallNum::Svrctl
        | VfsCallNum::Vmcall
        | VfsCallNum::Socketpath
        | VfsCallNum::GcovFlush => {
            // FS/driver conversation — wired in through the fs_comm window once
            // the W1 transport is live.
            SyscallResult::Nosys
        }

        // ── Conversation arm template: read (regular files; pipe/char/block have their own arms) ──
        VfsCallNum::Read => {
            // The **regular file** branch of C `do_read` → `read_write(READING)`
            // (read.c:141-265): position comes from the filp
            // (`position = f->filp_pos`, :145) → `req_readwrite` (the grant is a
            // magic grant letting the FS write into the user buffer,
            // `CPF_WRITE|CPF_TRY`) → the reply carries the new position and the
            // actual byte count. This round wires only regular files; the
            // `S_ISFIFO`/`S_ISCHR`/`S_ISBLK` branches each have their own arm
            // (pipe.c/cdev.c/bdev.c) and stay ENOSYS until wired.
            let (fd, buf, len) = {
                // User payload (minix-sys `read_via`: fd@0, buf@8, len@16, the
                // reserved accumulator field@24 is always zero — C
                // `mess_lc_vfs_readwrite`).
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let fd = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[8..16]);
                let buf = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[16..24]);
                (fd, buf, u64::from_le_bytes(b8))
            };
            if fd < 0 {
                return SyscallResult::Error(minix_types::EBADF);
            }
            // C read.c:150 — `if (size > SSIZE_MAX) return EINVAL;`
            if len > i64::MAX as u64 {
                return SyscallResult::Error(minix_types::EINVAL);
            }
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let (filp_idx, fs_e, ino, mode, orig_pos, vnode_idx) = {
                let fp = match state.fproc_table.get(fp_slot) {
                    Some(fp) => fp,
                    None => return SyscallResult::Error(minix_types::EINVAL),
                };
                let filp_idx = match fp.filps.get(fd as usize).copied().flatten() {
                    Some(idx) => idx,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                let filp = match state.filp_table.get(crate::filp::FilpId(filp_idx)) {
                    Some(f) => f,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                let vnode_idx = match filp.vnode {
                    Some(idx) => idx,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                let vnode = match state.vnode_table.get(crate::vnode::VnodeId(vnode_idx)) {
                    Some(v) => v,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                (
                    filp_idx,
                    vnode.fs,
                    vnode.ino,
                    vnode.mode,
                    filp.pos,
                    vnode_idx,
                )
            };
            // C dispatches on `vp->v_mode` (read.c:154-231): CHR → `cdev_io`
            // (wired through the cdev route below), REG → `req_read`;
            // FIFO/SOCK/BLK remain Nosys.
            match mode & crate::open::S_IFMT {
                crate::open::S_IFCHR => {
                    let sdev = state
                        .vnode_table
                        .get(VnodeId(vnode_idx))
                        .map(|v| v.sdev)
                        .unwrap_or(minix_types::NO_DEV);
                    return cdev_data_route(state, fp_slot, filp_idx, sdev, buf, len, false);
                }
                crate::open::S_IFREG => {}
                _ => return SyscallResult::Nosys,
            }
            let vmnt_id = match state.vmnt_table.find_by_fs(fs_e) {
                Some(v) => v,
                None => return SyscallResult::Error(minix_types::EIO),
            };
            // C request.c:860-862 — if the FS did not declare 64-bit capability
            // and the position exceeds INT_MAX, return EINVAL
            // (`!(vmp->m_fs_flags & RES_64BIT) && pos > INT_MAX`).
            let fs_flags = state
                .vmnt_table
                .get(vmnt_id)
                .map(|v| v.fs_flags)
                .unwrap_or(0);
            if fs_flags & crate::request::FsFlags::IS64BIT.bits() == 0 && orig_pos > i32::MAX as i64 {
                return SyscallResult::Error(minix_types::EINVAL);
            }
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            // magic grant: read direction = the FS writes into the user buffer.
            let grant = match state.grant_user_buffer(
                fs_e,
                user_e,
                buf,
                len,
                minix_types::CpFlags::WRITE | minix_types::CpFlags::TRY,
            ) {
                Ok(g) => g,
                Err(_) => return SyscallResult::Error(minix_types::EIO),
            };
            let req = crate::request::encode_read(ino, grant, orig_pos, len as usize);
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Transfer {
                    grant,
                    filp: filp_idx,
                    vnode: vnode_idx,
                    orig_pos,
                    write: false,
                });
            }
            state.pending_fs = Some(crate::main_loop::PendingFs {
                vmnt: vmnt_id.0,
                fs_e,
                worker,
                grant,
                user: user_e,
                req,
            });
            SyscallResult::Suspend
        }

        // ── Conversation arm template: write (regular files; pipe/char/block have their own arms) ──
        VfsCallNum::Write => {
            // C `do_write` → `read_write(WRITING)` (read.c:141-265): position
            // comes from the filp (:145); with **`O_APPEND` the position instead
            // becomes the vnode size** (:234) → `req_readwrite` (the grant is a
            // magic grant letting the FS read from the user buffer,
            // `CPF_READ|CPF_TRY`) → the reply carries the new position and actual
            // byte count, and the write direction must raise the vnode size to
            // the new position (:255-259, done in the continuation).
            let (fd, buf, len) = {
                // Payload has the same shape as read (minix-sys `write_via` uses
                // the same ReadWritePayload: fd@0, buf@8, len@16).
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let fd = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[8..16]);
                let buf = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[16..24]);
                (fd, buf, u64::from_le_bytes(b8))
            };
            if fd < 0 {
                return SyscallResult::Error(minix_types::EBADF);
            }
            // The same gate as C read.c:150 (read and write share one function in C).
            if len > i64::MAX as u64 {
                return SyscallResult::Error(minix_types::EINVAL);
            }
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let (filp_idx, fs_e, ino, mode, orig_pos, vnode_idx) = {
                let fp = match state.fproc_table.get(fp_slot) {
                    Some(fp) => fp,
                    None => return SyscallResult::Error(minix_types::EINVAL),
                };
                let filp_idx = match fp.filps.get(fd as usize).copied().flatten() {
                    Some(idx) => idx,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                let filp = match state.filp_table.get(crate::filp::FilpId(filp_idx)) {
                    Some(f) => f,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                let vnode_idx = match filp.vnode {
                    Some(idx) => idx,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                let vnode = match state.vnode_table.get(crate::vnode::VnodeId(vnode_idx)) {
                    Some(v) => v,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                // C read.c:234 — with `O_APPEND` the position becomes the current file size.
                let pos = transfer_position(filp.flags, filp.pos, vnode.size);
                (filp_idx, vnode.fs, vnode.ino, vnode.mode, pos, vnode_idx)
            };
            // Type dispatch mirrors read (the WRITING side of read.c:154-231): CHR → `cdev_io`
            // (wired through the cdev route below), REG → `req_readwrite`; everything else stays Nosys.
            match mode & crate::open::S_IFMT {
                crate::open::S_IFCHR => {
                    let sdev = state
                        .vnode_table
                        .get(VnodeId(vnode_idx))
                        .map(|v| v.sdev)
                        .unwrap_or(minix_types::NO_DEV);
                    return cdev_data_route(state, fp_slot, filp_idx, sdev, buf, len, true);
                }
                crate::open::S_IFREG => {}
                _ => return SyscallResult::Nosys,
            }
            let vmnt_id = match state.vmnt_table.find_by_fs(fs_e) {
                Some(v) => v,
                None => return SyscallResult::Error(minix_types::EIO),
            };
            let fs_flags = state
                .vmnt_table
                .get(vmnt_id)
                .map(|v| v.fs_flags)
                .unwrap_or(0);
            if fs_flags & crate::request::FsFlags::IS64BIT.bits() == 0 && orig_pos > i32::MAX as i64 {
                return SyscallResult::Error(minix_types::EINVAL);
            }
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            // magic grant: write direction = the FS reads from the user buffer (`CPF_READ`).
            let grant = match state.grant_user_buffer(
                fs_e,
                user_e,
                buf,
                len,
                minix_types::CpFlags::READ | minix_types::CpFlags::TRY,
            ) {
                Ok(g) => g,
                Err(_) => return SyscallResult::Error(minix_types::EIO),
            };
            let req = crate::request::encode_write(ino, grant, orig_pos, len as usize);
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Transfer {
                    grant,
                    filp: filp_idx,
                    vnode: vnode_idx,
                    orig_pos,
                    write: true,
                });
            }
            state.pending_fs = Some(crate::main_loop::PendingFs {
                vmnt: vmnt_id.0,
                fs_e,
                worker,
                grant,
                user: user_e,
                req,
            });
            SyscallResult::Suspend
        }

        // ── Conversation arm template: ftruncate (fd-based truncation; no data plane) ──
        VfsCallNum::Ftruncate => {
            // C `do_ftruncate` (link.c:331-359): fd + new length → `get_filp(fd,
            // VNODE_WRITE)` → write-bit gate (`filp_mode & W_BIT`) → if the size is
            // unchanged, **send no request** (POSIX: preserve file times) →
            // `truncate_vnode` → type gate (REG/FIFO) → `req_ftrunc(fs_e, ino, newsize, 0)`.
            let (length, fd) = {
                // User payload (C `mess_lc_vfs_truncate`: offset@0, fd@8;
                // minix-sys has no wrapper yet, so this follows the C struct
                // order with LP64 scaling).
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[0..8]);
                let len = i64::from_le_bytes(b8);
                let fd = i32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]);
                (len, fd)
            };
            // C link.c:341-342 — a negative length is EINVAL.
            if length < 0 {
                return SyscallResult::Error(minix_types::EINVAL);
            }
            if fd < 0 {
                return SyscallResult::Error(minix_types::EBADF);
            }
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let (filp_idx, fs_e, ino, mode, size, vnode_idx) = {
                let fp = match state.fproc_table.get(fp_slot) {
                    Some(fp) => fp,
                    None => return SyscallResult::Error(minix_types::EINVAL),
                };
                let filp_idx = match fp.filps.get(fd as usize).copied().flatten() {
                    Some(idx) => idx,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                let filp = match state.filp_table.get(crate::filp::FilpId(filp_idx)) {
                    Some(f) => f,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                // C link.c:346-347 — write-bit gate (`!(filp_mode & W_BIT)`; the
                // bit value is authoritative in `open::W_BIT`, same `Mode` type as `filp.mode`).
                if filp.mode & crate::open::W_BIT == 0 {
                    return SyscallResult::Error(minix_types::EBADF);
                }
                let vnode_idx = match filp.vnode {
                    Some(idx) => idx,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                let vnode = match state.vnode_table.get(crate::vnode::VnodeId(vnode_idx)) {
                    Some(v) => v,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                (filp_idx, vnode.fs, vnode.ino, vnode.mode, vnode.size, vnode_idx)
            };
            let _ = filp_idx;
            match state.send_ftrunc_for_vnode(
                state.current_worker,
                Some(fp_slot),
                fs_e,
                ino,
                mode,
                size,
                length,
                vnode_idx,
            ) {
                Ok(true) => SyscallResult::Suspend,
                // Size unchanged: C returns `r = OK` (not the new length).
                Ok(false) => SyscallResult::Ok(0),
                Err(e) => SyscallResult::Error(e),
            }
        }

        // ── Path-family arm: creat (O_CREAT; three-stage continuation chain) ──
        VfsCallNum::Creat => {
            // C `do_creat` (open.c:58-78) → the O_CREAT branch of `common_open`:
            // 1) `O_CREAT` must be present (otherwise EINVAL) 2) fetch the path
            // 3) narrow the mode bits by umask 4) run the whole path walk — success
            // means "file already exists"; ENOENT switches to the parent directory
            // and sends `REQ_CREATE` (stages 2/3 live in the continuation).
            let (name_addr, name_len, oflags, mode) = {
                // Payload (C `mess_lc_vfs_creat`: name@0/len@8/flags@16/mode@20
                // + padding, ipc.h:628-637 — **no buf field**; the client's
                // `open_via` CreatePayload has the same shape with zero padding).
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[0..8]);
                let name = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[8..16]);
                let len = u64::from_le_bytes(b8);
                let flags = u32::from_le_bytes([raw[16], raw[17], raw[18], raw[19]]);
                let mode = u32::from_le_bytes([raw[20], raw[21], raw[22], raw[23]]);
                (name, len, flags, mode)
            };
            // C `do_creat:71-72` — `O_CREAT` must be present.
            let args = crate::open::OpenArgs {
                oflags: crate::open::OpenFlags::from_bits_truncate(oflags),
                mode,
            };
            if args.validate_for_creat().is_err() {
                return SyscallResult::Error(minix_types::EINVAL);
            }
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            // Node-creation mode in C `common_open`: `omode & ALLPERMS & fp_umask`
            // (open.c:109). **Storage convention differs from C**: C's `fp_umask`
            // is `~mask` (protect.c:190), Rust's `fproc.umask` is the raw mask
            // (default 0 = keep everything), hence `& !umask` here.
            let umask = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.umask)
                .unwrap_or(0o022);
            let create_bits = (mode & 0o777 & !umask) | crate::open::S_IFREG;
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            // Fetch the path: C `do_creat:72-74` always calls `fetch_name` — the
            // creat wire format has no inline field, so the path is delivered only
            // through the caller's address space (under the host build a failed
            // fetch is EINVAL; the PATH_MAX limit and NUL-termination check live
            // inside fetch).
            let path = match (crate::path::SysPathFetcher { who: user_e })
                .fetch(name_addr, name_len as usize)
            {
                Ok(p) => p,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let resolve = match crate::path::Lookup::new(path.clone(), crate::path::LookupFlags::NOFLAGS)
            {
                Ok(l) => l,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let rd = state.root_dir_of(Some(fp_slot));
            let start = if resolve.path.starts_with('/') {
                crate::path::LookupStart { fs: rd.fs, ino: rd.ino, dev: rd.dev }
            } else {
                let wd = state.work_dir_of(fp_slot);
                crate::path::LookupStart { fs: wd.fs, ino: wd.ino, dev: wd.dev }
            };
            let uid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_uid).unwrap_or(0);
            let gid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_gid).unwrap_or(0);
            let (walk, step) = match crate::path::LookupWalk::begin(start, resolve, rd, uid, gid) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let crate::path::WalkStep::Send { fs_e, dir_ino, root_ino } = step else {
                return SyscallResult::Error(minix_types::EIO);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Path);
                wp.path = Some(crate::worker::PathPending {
                    walk,
                    grant: 0,
                    follow: crate::worker::PathFollow::Creat {
                        user: user_e,
                        oflags,
                        mode: create_bits,
                        path,
                    },
                });
            }
            if state
                .send_lookup_for_slot(worker, Some(fp_slot), fs_e, dir_ino, root_ino)
                .is_err()
            {
                if let Some(wp) = state.worker_pool.get_mut(worker) {
                    wp.cont = None;
                    wp.path = None;
                }
                return SyscallResult::Error(minix_types::EIO);
            }
            SyscallResult::Suspend
        }

        // ── Path-family arm: mkdir (walk to parent → REQ_MKDIR) ──
        VfsCallNum::Mkdir => {
            // C `do_mkdir` (open.c:564-598): fetch the path → `last_dir` (**walk
            // to the parent**) → directory gate + permission gate → `req_mkdir(parent
            // ino, lastc, uid, gid, bits)`. The path is split into "parent
            // directory + last component" by `last_dir_split` (the same thing as
            // C `last_dir`, including the NAME_MAX gate).
            let (name_addr, name_len, mode, inline) = {
                // Payload has the same shape as open (`mess_lc_vfs_path`: name@0,
                // len@8, flags@16, mode@20, buf@24).
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[0..8]);
                let name = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[8..16]);
                let len = u64::from_le_bytes(b8);
                let mode = u32::from_le_bytes([raw[20], raw[21], raw[22], raw[23]]);
                (name, len, mode, raw[24..].to_vec())
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            // Fetch the path: both branches of C `copy_path` (utility.c:17-43) —
            // at or below the inline limit it comes inline from the payload;
            // longer paths are fetched through the caller's address space (under
            // the host build the failure surface is EINVAL).
            let path = match crate::path::copy_path(&inline, name_addr, name_len as usize, user_e) {
                Ok(p) => p,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let split = match crate::path::last_dir_split(&path) {
                Ok(sp) => sp,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            // C `do_mkdir:583` — permission bits = `I_DIRECTORY | (dirmode & RWX_MODES
            // & fp_umask)`; as with Creat, the Rust side stores `fp_umask` as the
            // raw mask, hence `& !umask`.
            let umask = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.umask)
                .unwrap_or(0o022);
            let bits = crate::open::S_IFDIR | (mode & 0o777 & !umask);
            let resolve = match crate::path::Lookup::new(
                split.dir_path.clone(),
                crate::path::LookupFlags::NOFLAGS,
            ) {
                Ok(l) => l,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let rd = state.root_dir_of(Some(fp_slot));
            let start = if resolve.path.starts_with('/') {
                crate::path::LookupStart { fs: rd.fs, ino: rd.ino, dev: rd.dev }
            } else {
                let wd = state.work_dir_of(fp_slot);
                crate::path::LookupStart { fs: wd.fs, ino: wd.ino, dev: wd.dev }
            };
            let uid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_uid).unwrap_or(0);
            let gid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_gid).unwrap_or(0);
            let (walk, step) = match crate::path::LookupWalk::begin(start, resolve, rd, uid, gid) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let crate::path::WalkStep::Send { fs_e, dir_ino, root_ino } = step else {
                return SyscallResult::Error(minix_types::EIO);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Path);
                wp.path = Some(crate::worker::PathPending {
                    walk,
                    grant: 0,
                    follow: crate::worker::PathFollow::Mkdir {
                        user: user_e,
                        entry: split.entry,
                        mode: bits,
                    },
                });
            }
            if state
                .send_lookup_for_slot(worker, Some(fp_slot), fs_e, dir_ino, root_ino)
                .is_err()
            {
                if let Some(wp) = state.worker_pool.get_mut(worker) {
                    wp.cont = None;
                    wp.path = None;
                }
                return SyscallResult::Error(minix_types::EIO);
            }
            SyscallResult::Suspend
        }

        // ── Path-family arm: open (open-existing; O_CREAT belongs to the creat arm) ──
        VfsCallNum::Open => {
            // C `do_open` (open.c:38-53): `O_CREAT` must be absent (libc splits
            // open() into the OPEN and CREAT calls) → `copy_path` fetches the path
            // → `common_open` runs the walk + local assembly. This arm fetches the
            // path and starts the walk; the local half after the walk is done by
            // the `PathFollow::Open` continuation.
            let (name_addr, name_len, flags, inline) = {
                // User payload (minix-sys `OpenPathPayload`: name@0, len@8,
                // flags@16, mode@20, buf@24 — the LP64 rendering of C
                // `mess_lc_vfs_path`; paths ≤ OPEN_PATH_INLINE_MAX are **inline in buf**).
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[0..8]);
                let name = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[8..16]);
                let len = u64::from_le_bytes(b8);
                let flags = u32::from_le_bytes([raw[16], raw[17], raw[18], raw[19]]);
                let inline = raw[24..].to_vec();
                (name, len, flags, inline)
            };
            // The O_CREAT gate of `do_open` (C open.c:46-47).
            let args = crate::open::OpenArgs {
                oflags: crate::open::OpenFlags::from_bits_truncate(flags),
                mode: 0,
            };
            if args.validate_for_open().is_err() {
                return SyscallResult::Error(minix_types::EINVAL);
            }
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            // Path retrieval: both branches of C `copy_path` (utility.c:17-43) —
            // at or below the inline limit it comes inline from the payload
            // (minix-sys `OPEN_PATH_INLINE_MAX`); longer paths are fetched through
            // the caller's address space (C `fetch_name`, EINVAL on failure under the host build).
            let path = match crate::path::copy_path(&inline, name_addr, name_len as usize, user_e) {
                Ok(p) => p,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let resolve = match crate::path::Lookup::new(
                path,
                crate::path::LookupFlags::NOFLAGS,
            ) {
                Ok(l) => l,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            // Start point: a leading `/` selects the process root, otherwise the working directory (C `eat_path`).
            let rd = state.root_dir_of(Some(fp_slot));
            let start = if resolve.path.starts_with('/') {
                crate::path::LookupStart { fs: rd.fs, ino: rd.ino, dev: rd.dev }
            } else {
                let wd = state.work_dir_of(fp_slot);
                crate::path::LookupStart { fs: wd.fs, ino: wd.ino, dev: wd.dev }
            };
            let uid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_uid).unwrap_or(0);
            let gid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_gid).unwrap_or(0);
            let (walk, step) = match crate::path::LookupWalk::begin(start, resolve, rd, uid, gid) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let crate::path::WalkStep::Send { fs_e, dir_ino, root_ino } = step else {
                return SyscallResult::Error(minix_types::EIO);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Path);
                wp.path = Some(crate::worker::PathPending {
                    walk,
                    grant: 0, // overwritten by send_lookup_for_slot
                    follow: crate::worker::PathFollow::Open { user: user_e, oflags: flags },
                });
            }
            if state
                .send_lookup_for_slot(worker, Some(fp_slot), fs_e, dir_ino, root_ino)
                .is_err()
            {
                if let Some(wp) = state.worker_pool.get_mut(worker) {
                    wp.cont = None;
                    wp.path = None;
                }
                return SyscallResult::Error(minix_types::EIO);
            }
            SyscallResult::Suspend
        }

        // ── Path-family arm: stat (path form; the first arm to drive the path walk) ──
        VfsCallNum::Stat | VfsCallNum::Lstat => {
            // C `do_stat` (stadir.c:140-165) and `do_lstat` (stadir.c:405-434)
            // differ **only** in the walk flags: stat uses `PATH_NOFLAGS`, lstat
            // uses `PATH_RET_SYMLINK` (if the last component is a symbolic link,
            // the link itself is returned, not followed). Everything else (the
            // `req_stat` target, permissions, locks) is identical, so the two call
            // numbers share one arm.
            //
            // Host-testability boundary: `mess_lc_vfs_stat` has no inline path
            // field, so the path must be fetched across address spaces via
            // `SysPathFetcher` (C `fetch_name`), which fails under the host build
            // — therefore "lstat's flags are RET_SYMLINK" cannot be observed on
            // the host; it is deferred to real-hardware E5/T4. The host assertion
            // for the same flags pipeline lives on the sticky-bit sub-walk
            // REQ_LOOKUP of Unlink (there the flags really land in the message).
            let retain_symlink = matches!(call, VfsCallNum::Lstat);
            // C `do_stat` (stadir.c:140-165): fetch the path → `eat_path` runs the
            // walk (one REQ_LOOKUP per step) → `req_stat(fs_e, ino, who_e, buf)`.
            // In the single-threaded model every walk step must suspend and
            // resume, so this arm does only three things: 1) fetch the path
            // (`PathFetcher`) 2) start the state machine (`LookupWalk::begin`)
            // 3) record the site and continuation and hand the first lookup to the
            // loop; subsequent progress is driven by the `WorkerCont::Path`
            // continuation (including phase 2's REQ_STAT).
            let (name_addr, name_len, statbuf) = {
                // User payload (minix-sys `stat_via_path`: len@0, name@8,
                // buf@16 — the LP64 rendering of C `mess_lc_vfs_stat`).
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[0..8]);
                let len = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[8..16]);
                let name = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[16..24]);
                (name, len, u64::from_le_bytes(b8))
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            // Fetch the path: both branches of C `copy_path` (inline in the
            // message vs. cross-space); the production fetcher on the Rust side is `path::SysPathFetcher`.
            let fetcher = crate::path::SysPathFetcher { who: user_e };
            let path = match fetcher.fetch(name_addr, name_len as usize) {
                Ok(p) => p,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let resolve = match crate::path::Lookup::new(
                path,
                if retain_symlink {
                    crate::path::LookupFlags::RET_SYMLINK
                } else {
                    crate::path::LookupFlags::NOFLAGS
                },
            )
            {
                Ok(l) => l,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            // Start point: a leading `/` selects the process root, otherwise the
            // working directory (the same choice as C `eat_path`).
            let rd = state.root_dir_of(Some(fp_slot));
            let start = if resolve.path.starts_with('/') {
                crate::path::LookupStart { fs: rd.fs, ino: rd.ino, dev: rd.dev }
            } else {
                let wd = state.work_dir_of(fp_slot);
                crate::path::LookupStart { fs: wd.fs, ino: wd.ino, dev: wd.dev }
            };
            let uid = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.eff_uid)
                .unwrap_or(0);
            let gid = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.eff_gid)
                .unwrap_or(0);
            let (walk, step) = match crate::path::LookupWalk::begin(start, resolve, rd, uid, gid) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let crate::path::WalkStep::Send { fs_e, dir_ino, root_ino } = step else {
                // `begin` never completes directly (the first step is always Send); defensive return.
                return SyscallResult::Error(minix_types::EIO);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            // Record the site: state machine + follow (phase 2) + continuation
            // tag; the path grant is opened by `send_lookup_for_slot` on the spot
            // (slot scratch holds the path + NUL).
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Path);
                wp.path = Some(crate::worker::PathPending {
                    walk,
                    grant: 0, // overwritten by send_lookup_for_slot
                    follow: crate::worker::PathFollow::Stat { user: user_e, buf: statbuf },
                });
            }
            if state
                .send_lookup_for_slot(worker, Some(fp_slot), fs_e, dir_ino, root_ino)
                .is_err()
            {
                if let Some(wp) = state.worker_pool.get_mut(worker) {
                    wp.cont = None;
                    wp.path = None;
                }
                return SyscallResult::Error(minix_types::EIO);
            }
            SyscallResult::Suspend
        }

        // ── Conversation arm template: fstat (fd-based stat) ──
        VfsCallNum::Fstat => {
            // C `do_fstat` (stadir.c:173-192): `fd`/`buf` from the user payload →
            // `get_filp(fd, VNODE_READ)` (fd valid + readable gate) →
            // `req_stat(v_fs_e, v_inode_nr, who_e, buf)`: a magic grant authorizes
            // the user's `struct stat` buffer for direct FS writes
            // (`cpf_grant_magic(fs_e, user_e, buf, sizeof(struct stat),
            // CPF_WRITE|CPF_TRY)` — request.c:1087) → `fs_sendrec` suspends.
            // User payload (the encoding side of minix-sys `fstat_via`: fd@0,
            // buf@8 — the LP64 rendering of C `mess_lc_vfs_fstat`).
            let (fd, statbuf) = {
                // SAFETY: this call number's payload is laid out in the above two fields.
                let raw = unsafe { &msg.m_u.raw };
                let fd = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[8..16]);
                (fd, u64::from_le_bytes(b8))
            };
            // When there is no fd in the user payload (`fd < 0`), C's `get_filp` returns EBADF.
            if fd < 0 {
                return SyscallResult::Error(minix_types::EBADF);
            }
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            // filp → vnode → (fs_e, ino); this also passes the fd-validity and
            // readability gates (C `get_filp(..., VNODE_READ)`).
            let target = {
                let fp = match state.fproc_table.get(fp_slot) {
                    Some(fp) => fp,
                    None => return SyscallResult::Error(minix_types::EINVAL),
                };
                let filp_idx = match fp.filps.get(fd as usize).copied().flatten() {
                    Some(idx) => idx,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                let filp = match state.filp_table.get(crate::filp::FilpId(filp_idx)) {
                    Some(f) => f,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                let vnode_idx = match filp.vnode {
                    Some(idx) => idx,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                let vnode = match state.vnode_table.get(crate::vnode::VnodeId(vnode_idx)) {
                    Some(v) => v,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                (filp_idx, vnode.fs, vnode.ino)
            };
            let (filp_idx, fs_e, ino) = target;
            let _ = filp_idx;
            let vmnt = match state.vmnt_table.find_by_fs(fs_e) {
                Some(v) => v.0,
                // A failed C `find_vmnt` is EIO (comm.c:137-140).
                None => return SyscallResult::Error(minix_types::EIO),
            };
            let Some(worker) = state.current_worker else {
                // No slot (not a run_once path, or already released) — return
                // EAGAIN on the same surface as C's `worker_available()==0`.
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            // magic grant: the FS writes into the user's stat buffer (`CPF_WRITE | CPF_TRY`).
            // Grant window = the full size of the user `struct stat` (the FS side
            // `write_user_stat` stores that many bytes in C layout; the old value
            // 88 was the fs_driver serialization SIZE, not C `struct stat` — that
            // would truncate fields after st_size@112).
            const STRUCT_STAT_SIZE: u64 = minix_types::Stat::USER_STAT_SIZE as u64;
            let grant = match state.grant_user_buffer(
                fs_e,
                user_e,
                statbuf,
                STRUCT_STAT_SIZE,
                minix_types::CpFlags::WRITE | minix_types::CpFlags::TRY,
            ) {
                Ok(g) => g,
                // C panics here ("cpf_grant_* failed", request.c:1090); Rust
                // returns EIO to the user (the external face of an internal error).
                Err(_) => return SyscallResult::Error(minix_types::EIO),
            };
            let req = crate::request::encode_stat(ino, grant);
            // Register the continuation + pending send (the send is done by the main loop's flush_pending_fs).
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Fstat { grant });
            }
            state.pending_fs = Some(crate::main_loop::PendingFs {
                vmnt,
                fs_e,
                worker,
                grant,
                user: user_e,
                req,
            });
            // C's `worker_wait()`: in the single-threaded model this means the arm
            // returns Suspend and the slot stays in WaitingForFs (set by
            // flush_pending_fs through fs_sendrec).
            SyscallResult::Suspend
        }

        // ── Server-internal arm: getsysinfo (VFS_GETSYSINFO = +48) ──
        // ── Conversation arm template: getdents (directory read; only directories pass the type gate) ──
        // ── Path arm template: access (decide once the walk completes, no FS round trip) ──
        // ── Path arm template: readlink (walk the last-component symlink itself, don't follow it) ──
        // ── Path arm template: chmod (walk, then permission gate; reply carries the actual mode) ──
        // ── Path arm template: unlink / rmdir (parent walk + sticky-bit sub-walk) ──
        // ── chown / fchown (C `do_chown`, protect.c:24-110; both call numbers
        // share one function body: the path half walks, the fd half takes the
        // vnode from the filp) ──
        // ── fchmod (the fd half of C `do_chmod`, protect.c:62-133: take the vnode
        // from the filp, then share the same body as the path half) ──
        VfsCallNum::Fchmod => {
            // Payload `mess_lc_vfs_fchmod` (ipc.h:647-652): fd@0, mode@4
            // (`mode_t` is 4 bytes, so the two fields sit adjacent).
            let (fd, mode) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let fd = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let mode = u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]);
                (fd, mode)
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            if fd < 0 {
                return SyscallResult::Error(minix_types::EBADF);
            }
            // C `do_chmod:88-93`: `get_filp(rfd, VNODE_WRITE)` — only fd validity
            // is checked, not the open mode (unlike ftruncate's W-bit gate).
            let vnode_idx = {
                let fp = match state.fproc_table.get(fp_slot) {
                    Some(fp) => fp,
                    None => return SyscallResult::Error(minix_types::EINVAL),
                };
                let filp_idx = match fp.filps.get(fd as usize).copied().flatten() {
                    Some(idx) => idx,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                match state
                    .filp_table
                    .get(crate::filp::FilpId(filp_idx))
                    .and_then(|f| f.vnode)
                {
                    Some(v) => v,
                    None => return SyscallResult::Error(minix_types::EBADF),
                }
            };
            let (fs_e, ino, node_uid, node_gid) =
                match state.vnode_table.get(crate::vnode::VnodeId(vnode_idx)) {
                    Some(v) => (v.fs, v.ino, v.uid, v.gid),
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
            state.finish_chmod(worker, Some(fp_slot), fs_e, ino, node_uid, node_gid, mode, vnode_idx);
            // The shared body decides whether to suspend or finish; the arm always reports suspension (same as Fchown).
            SyscallResult::Suspend
        }

        // ── Path arm template: truncate (shares the send half with ftruncate) ──
        VfsCallNum::Truncate => {
            // C `do_truncate` (link.c:277-326): payload `mess_lc_vfs_truncate`
            // (offset@0, fd@8, name@16, len@24) → negative length is EINVAL → fetch
            // the path (cross-space) → run the walk (NOFLAGS) → after the walk passes
            // the W-bit gate, send `REQ_FTRUNC`.
            let (length, name_len, name_addr) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[0..8]);
                let length = i64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[16..24]);
                let name_addr = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[24..32]);
                (length, u64::from_le_bytes(b8), name_addr)
            };
            // C link.c:299-300 — a negative length is EINVAL (before any path fetch).
            if length < 0 {
                return SyscallResult::Error(minix_types::EINVAL);
            }
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            let fetcher = crate::path::SysPathFetcher { who: user_e };
            let path = match fetcher.fetch(name_addr, name_len as usize) {
                Ok(p) => p,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let resolve = match crate::path::Lookup::new(
                path,
                crate::path::LookupFlags::NOFLAGS,
            ) {
                Ok(l) => l,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let rd = state.root_dir_of(Some(fp_slot));
            let start = if resolve.path.starts_with('/') {
                crate::path::LookupStart { fs: rd.fs, ino: rd.ino, dev: rd.dev }
            } else {
                let wd = state.work_dir_of(fp_slot);
                crate::path::LookupStart { fs: wd.fs, ino: wd.ino, dev: wd.dev }
            };
            let uid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_uid).unwrap_or(0);
            let gid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_gid).unwrap_or(0);
            let (walk, step) = match crate::path::LookupWalk::begin(start, resolve, rd, uid, gid) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let crate::path::WalkStep::Send { fs_e, dir_ino, root_ino } = step else {
                return SyscallResult::Error(minix_types::EIO);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Path);
                wp.path = Some(crate::worker::PathPending {
                    walk,
                    grant: 0, // overwritten by send_lookup_for_slot
                    follow: crate::worker::PathFollow::Truncate { length },
                });
            }
            if state
                .send_lookup_for_slot(worker, Some(fp_slot), fs_e, dir_ino, root_ino)
                .is_err()
            {
                if let Some(wp) = state.worker_pool.get_mut(worker) {
                    wp.cont = None;
                    wp.path = None;
                }
                return SyscallResult::Error(minix_types::EIO);
            }
            SyscallResult::Suspend
        }

        // ── Path arm template: mknod (parent walk + direct name grant) ──
        // ── Path arm template: utimens (path/fd halves share one body) ──
        // ── Path arm template: symlink (parent walk + direct name grant + target magic grant) ──
        // ── Path arm template: link (**two walks**: source file first, then the new name's parent) ──
        // ── sync / fsync (multi-mount sequence: one REQ_SYNC per matching mount) ──
        // ── Path arm template: chdir / chroot / fchdir (local directory change after the walk) ──
        VfsCallNum::Chdir | VfsCallNum::Chroot | VfsCallNum::Fchdir => {
            // C `do_chdir`/`do_chroot` (stadir.c:48-107) and `do_fchdir`
            // (stadir.c:32-46): all reduce to `change_into` (decide and change as
            // soon as the walk ends, **no FS round trip**). chroot adds a
            // superuser-only gate.
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let into_root = matches!(call, VfsCallNum::Chroot);
            if into_root {
                let eff_uid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_uid).unwrap_or(0);
                if eff_uid != crate::link::SU_UID {
                    return SyscallResult::Error(minix_types::EPERM);
                }
            }
            if matches!(call, VfsCallNum::Fchdir) {
                // C stadir.c:38-42: fd → filp → vnode, then the same `change_into`.
                let fd = {
                    // SAFETY: `mess_lc_vfs_fchdir { int fd; }` (ipc.h:640-644).
                    let raw = unsafe { &msg.m_u.raw };
                    i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]])
                };
                if fd < 0 {
                    return SyscallResult::Error(minix_types::EBADF);
                }
                let vnode_idx = {
                    let fp = match state.fproc_table.get(fp_slot) {
                        Some(fp) => fp,
                        None => return SyscallResult::Error(minix_types::EINVAL),
                    };
                    let filp_idx = match fp.filps.get(fd as usize).copied().flatten() {
                        Some(idx) => idx,
                        None => return SyscallResult::Error(minix_types::EBADF),
                    };
                    match state
                        .filp_table
                        .get(crate::filp::FilpId(filp_idx))
                        .and_then(|f| f.vnode)
                    {
                        Some(v) => v,
                        None => return SyscallResult::Error(minix_types::EBADF),
                    }
                };
                let status = state.change_into(Some(fp_slot), vnode_idx, false);
                return SyscallResult::Ok(status);
            }
            // Path half: same shape as access/chmod (`mess_lc_vfs_path`); after the
            // walk, change the directory locally. Path retrieval uses both branches
            // of C `copy_path` (utility.c:17-43).
            let (name_addr, name_len, inline) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[0..8]);
                let name = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[8..16]);
                (name, u64::from_le_bytes(b8), raw[24..].to_vec())
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            let path = match crate::path::copy_path(&inline, name_addr, name_len as usize, user_e) {
                Ok(p) => p,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            // `lookup_init(..., PATH_NOFLAGS, ...)` at C stadir.c:65.
            let resolve = match crate::path::Lookup::new(path, crate::path::LookupFlags::NOFLAGS)
            {
                Ok(l) => l,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let rd = state.root_dir_of(Some(fp_slot));
            let start = if resolve.path.starts_with('/') {
                crate::path::LookupStart { fs: rd.fs, ino: rd.ino, dev: rd.dev }
            } else {
                let wd = state.work_dir_of(fp_slot);
                crate::path::LookupStart { fs: wd.fs, ino: wd.ino, dev: wd.dev }
            };
            let uid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_uid).unwrap_or(0);
            let gid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_gid).unwrap_or(0);
            let (walk, step) = match crate::path::LookupWalk::begin(start, resolve, rd, uid, gid) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let crate::path::WalkStep::Send { fs_e, dir_ino, root_ino } = step else {
                return SyscallResult::Error(minix_types::EIO);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Path);
                wp.path = Some(crate::worker::PathPending {
                    walk,
                    grant: 0, // overwritten by send_lookup_for_slot
                    follow: crate::worker::PathFollow::Chdir { into_root },
                });
            }
            if state
                .send_lookup_for_slot(worker, Some(fp_slot), fs_e, dir_ino, root_ino)
                .is_err()
            {
                if let Some(wp) = state.worker_pool.get_mut(worker) {
                    wp.cont = None;
                    wp.path = None;
                }
                return SyscallResult::Error(minix_types::EIO);
            }
            SyscallResult::Suspend
        }

        VfsCallNum::Sync | VfsCallNum::Fsync => {
            // C `do_sync` (misc.c:276-296, all mounts) and `do_fsync`
            // (misc.c:229-267, filtered by the file's device `v_dev`).
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            let dev = if matches!(call, VfsCallNum::Fsync) {
                // C misc.c:236-243: `get_filp(fd, VNODE_READ)` takes the vnode's
                // `v_dev` — fsync only syncs the mount that device belongs to.
                let fd = {
                    // SAFETY: `mess_lc_vfs_fsync { int fd; }` (ipc.h:673-677).
                    let raw = unsafe { &msg.m_u.raw };
                    i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]])
                };
                if fd < 0 {
                    return SyscallResult::Error(minix_types::EBADF);
                }
                let fp = match state.fproc_table.get(fp_slot) {
                    Some(fp) => fp,
                    None => return SyscallResult::Error(minix_types::EINVAL),
                };
                let filp_idx = match fp.filps.get(fd as usize).copied().flatten() {
                    Some(idx) => idx,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                let vnode_idx = match state
                    .filp_table
                    .get(crate::filp::FilpId(filp_idx))
                    .and_then(|f| f.vnode)
                {
                    Some(v) => v,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                match state.vnode_table.get(crate::vnode::VnodeId(vnode_idx)) {
                    Some(v) => Some(v.dev),
                    None => return SyscallResult::Error(minix_types::EBADF),
                }
            } else {
                None
            };
            match state.begin_sync_sequence(worker, Some(fp_slot), dev) {
                Ok(()) => {
                    // No mounts to sync: C's loop sent nothing → succeed directly.
                    if state.pending_fs.is_none() {
                        return SyscallResult::Ok(0);
                    }
                    SyscallResult::Suspend
                }
                Err(e) => SyscallResult::Error(e),
            }
        }

        // ── Path arm template: rename (**three-stage chain**: old parent → [sticky-bit sub-walk] → new parent) ──
        // ── statvfs family (Statvfs1 / Fstatvfs1): take the mount row → fill_statvfs ──
        // ── getvfsstat (multi-mount sequence + user buffer offset by i*sizeof + returned count) ──
        // ── mapdriver (only RS may call; label → endpoint → dmap/smap registration) ──
        // ── fcntl (mostly local fd-table operations; locks and F_FREESP copy flock across spaces) ──
        // ── ioctl (dispatch by file type; the device conversation pipeline is not wired, see the note below) ──
        // ── copyfd (driver callback: move/close one fd between the caller and the remote end) ──
        // ── pipe2 (ask PFS for a new inode, then assemble the fd pair) ──
        // ── socket family (common local gates: domain → driver table, fd → socket, resources available) ──
        VfsCallNum::Socket | VfsCallNum::Socketpair => {
            // C `do_socket` (socket.c:176-216) / `do_socketpair`
            // (socket.c:224-273): both payloads are `mess_lc_vfs_socket`
            // (domain@0, type@4, protocol@8).
            let (domain, ty, _protocol) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let domain = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let ty = i32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]);
                let protocol = i32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]);
                (domain, ty, protocol)
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            // C socket.c:185-187 — does this domain have a socket driver? If not,
            // `EAFNOSUPPORT` (decided locally by looking up the smap domain table).
            if crate::device_map::smap_by_domain(&state.smap_table, domain).is_none() {
                return SyscallResult::Error(minix_types::EAFNOSUPPORT);
            }
            // C socket.c:204-205 / :242-243 — first confirm the process has enough
            // fd slots (socket needs 1, socketpair needs 2).
            let want = if matches!(call, VfsCallNum::Socketpair) { 2 } else { 1 };
            let enough = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| crate::filedes::check_fds(fp, want).is_ok())
                .unwrap_or(false);
            if !enough {
                return SyscallResult::Error(minix_types::EMFILE);
            }
            // C's `sock_type = type & ~SOCK_FLAGS_MASK` and `get_sock_flags`
            // (decision function `socket::strip_sock_type`): type bits and open flags split.
            let (_sock_type, _flags) = crate::socket::strip_sock_type(ty as u32);
            // C socket.c:207-214 — `sdev_socket(domain, sock_type, protocol,
            // &dev, pair)`: send `SDEV_SOCKET` to that domain's socket driver; the
            // reply carries the new socket number. The driver conversation
            // pipeline (smap endpoint lookup + send + reply landing in the slot) is
            // in place, so this really sends; when the reply arrives,
            // `WorkerCont::SdevSocket` continues with `make_sock_fd` (PFS creates
            // the node + assembles the fd).
            let Some(row) = crate::device_map::smap_by_domain(&state.smap_table, domain) else {
                return SyscallResult::Error(minix_types::EAFNOSUPPORT);
            };
            let drv_e = match state
                .smap_table
                .entries
                .get(row as usize)
                .and_then(|r| r.endpt)
            {
                Some(e) => e,
                None => return SyscallResult::Error(minix_types::EAFNOSUPPORT),
            };
            let smap_num = state.smap_table.entries[row as usize].num;
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            // `mess_vfs_lsockdriver_socket { req_id, domain, type, protocol,
            // user_endpt }` (ipc.h:2329-2337); `m_type` is `SDEV_SOCKET` /
            // `SDEV_SOCKETPAIR` (chosen by `pair`).
            let mut req = minix_types::Message {
                m_type: if matches!(call, VfsCallNum::Socketpair) {
                    minix_sockdriver::sdev::SdevRequest::SocketPair as i32
                } else {
                    minix_sockdriver::sdev::SdevRequest::Socket as i32
                },
                ..minix_types::Message::default()
            };
            // SAFETY: the request payload is laid out in the above field order.
            unsafe {
                let raw = &mut req.m_u.raw;
                raw[0..4].copy_from_slice(&user_e.0.to_le_bytes()); // req_id = who_e
                raw[4..8].copy_from_slice(&domain.to_le_bytes());
                raw[8..12].copy_from_slice(&(ty & !(crate::socket::SOCK_FLAGS_MASK as i32)).to_le_bytes());
                raw[12..16].copy_from_slice(&_protocol.to_le_bytes());
                raw[16..20].copy_from_slice(&user_e.0.to_le_bytes()); // user_endpt
            }
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::SdevSocket {
                    pair: matches!(call, VfsCallNum::Socketpair),
                    flags: _flags,
                    smap_num,
                });
            }
            match state.send_drv_for_slot(worker, Some(fp_slot), drv_e, &req) {
                Ok(()) => SyscallResult::Suspend,
                Err(e) => SyscallResult::Error(e),
            }
        }

        VfsCallNum::Shutdown => {
            // C `do_shutdown` (socket.c:744-762): the two gates of `get_sock(fd)`
            // (not an fd → EBADF; not a socket → ENOTSOCK) → the `how` value gate
            // (EINVAL) → `sdev_shutdown` (driver conversation).
            let (fd, how) = {
                // SAFETY: `mess_lc_vfs_shutdown { int fd; int how; }`
                // (ipc.h:828-833).
                let raw = unsafe { &msg.m_u.raw };
                (
                    i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]),
                    i32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]),
                )
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            if let Err(e) = state.get_sock(fp_slot, fd) {
                return SyscallResult::Error(e);
            }
            if let Err(e) = crate::socket::check_shutdown_how(how) {
                return SyscallResult::Error(e.to_errno());
            }
            // C socket.c:761 — `sdev_shutdown(dev, how)`: goes through `sdev_simple`
            // (`SDEV_SHUTDOWN` + `param = how`); the reply type is `SDEV_REPLY` and
            // the status is in the payload. The pipeline is in place, so this really sends.
            let (dev, _flags) = match state.get_sock(fp_slot, fd) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e),
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            match state.send_sdev_simple(
                worker,
                Some(fp_slot),
                dev,
                minix_sockdriver::sdev::SdevRequest::Shutdown as i32,
                how,
            ) {
                Ok(()) => SyscallResult::Suspend,
                Err(e) => SyscallResult::Error(e),
            }
        }

        // ── getset family (Setsockopt/Getsockopt/Getsockname/Getpeername) ──
        VfsCallNum::Setsockopt | VfsCallNum::Getsockopt => {
            // C `do_setsockopt`/`do_getsockopt` (socket.c:657-698): payload
            // `mess_lc_vfs_sockopt` (fd@0, level@4, name@8, buf@16, len@24).
            let (fd, level, name, buf, len) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let fd = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let level = i32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]);
                let name = i32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]);
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[16..24]);
                let buf = u64::from_le_bytes(b8);
                let len = u32::from_le_bytes([raw[24], raw[25], raw[26], raw[27]]);
                (fd, level, name, buf, len)
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let (dev, _flags) = match state.get_sock(fp_slot, fd) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e),
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            let write_dir = matches!(call, VfsCallNum::Getsockopt);
            let req_type = if write_dir {
                minix_sockdriver::sdev::SdevRequest::GetSockOpt
            } else {
                minix_sockdriver::sdev::SdevRequest::SetSockOpt
            };
            match state.send_sdev_getset(
                worker,
                Some(fp_slot),
                dev,
                req_type as i32,
                level,
                name,
                buf,
                len,
                write_dir,
            ) {
                Ok(()) => SyscallResult::Suspend,
                Err(e) => SyscallResult::Error(e),
            }
        }

        VfsCallNum::Getsockname | VfsCallNum::Getpeername => {
            // C `do_getsockname`/`do_getpeername` (socket.c:701-733): payload
            // `mess_lc_vfs_sockaddr` (fd@0, addr@8, addr_len@16); `level`/`name`
            // are passed as 0 (in C this is `sdev_get(dev, type, 0, 0, addr, &len)`).
            let (fd, addr, addr_len) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let fd = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[8..16]);
                let addr = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[16..24]);
                let addr_len = u64::from_le_bytes(b8) as u32;
                (fd, addr, addr_len)
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let (dev, _flags) = match state.get_sock(fp_slot, fd) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e),
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            let req_type = if matches!(call, VfsCallNum::Getpeername) {
                minix_sockdriver::sdev::SdevRequest::GetPeerName
            } else {
                minix_sockdriver::sdev::SdevRequest::GetSockName
            };
            match state.send_sdev_getset(
                worker,
                Some(fp_slot),
                dev,
                req_type as i32,
                0,
                0,
                addr,
                addr_len,
                true,
            ) {
                Ok(()) => SyscallResult::Suspend,
                Err(e) => SyscallResult::Error(e),
            }
        }

        // ── bind / connect (**process-level suspension**: the shape of C's sdev_suspend) ──
        // ── sendto / recvfrom (two grants: data + address; process-level suspension) ──
        // ── sendmsg / recvmsg (msghdr + a single iovec; the same sdev_readwrite as
        //     sendto/recvfrom, plus a control buffer and msg_buf) ──
        // ── select (local half: gates, fd validation, readiness decision and immediate return) ──
        VfsCallNum::Select => {
            // C `do_select` (select.c:94-260): payload `mess_lc_vfs_select`
            // (nfds@0, readfds@8, writefds@16, errorfds@24, timeout@32).
            let (nfds, readfds, writefds, errorfds, timeout) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let nfds = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[8..16]);
                let readfds = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[16..24]);
                let writefds = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[24..32]);
                let errorfds = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[32..40]);
                (nfds, readfds, writefds, errorfds, u64::from_le_bytes(b8))
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            let io = crate::select::SysFdSetIo { who: user_e };
            do_select(
                state,
                fp_slot,
                nfds as usize,
                readfds,
                writefds,
                errorfds,
                timeout,
                &io,
            )
        }

        VfsCallNum::Sendmsg | VfsCallNum::Recvmsg => {
            // C `do_sockmsg` (socket.c:538-595): payload `mess_lc_vfs_sockmsg`
            // (fd@0, msgbuf@8, flags@16). Fetch the user `struct msghdr` (cross-space) →
            // iov gate (multi-element vectors unsupported, > 1 is EMSGSIZE; libc
            // coalesces them) → a single `iovec` → `sdev_readwrite(dev, data, ctl, addr, flags, ...)`.
            let (fd, msgbuf, flags) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let fd = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[8..16]);
                let msgbuf = u64::from_le_bytes(b8);
                let flags = i32::from_le_bytes([raw[16], raw[17], raw[18], raw[19]]);
                (fd, msgbuf, flags)
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let (dev, filp_flags) = match state.get_sock(fp_slot, fd) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e),
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            // Fetch the user msghdr (the production path copies across spaces; under the host build → EINVAL).
            let fetcher = crate::socket::SysMsgHdrFetcher { who: user_e };
            match do_sockmsg(
                state,
                worker,
                Some(fp_slot),
                dev,
                filp_flags,
                msgbuf,
                flags,
                matches!(call, VfsCallNum::Recvmsg),
                &fetcher,
            ) {
                Ok(()) => SyscallResult::Suspend,
                Err(e) => SyscallResult::Error(e),
            }
        }

        VfsCallNum::Sendto | VfsCallNum::Recvfrom => {
            // C `do_sendto`/`do_recvfrom` (socket.c:483-518) + `sdev_readwrite`
            // (sdev.c:336-410): payload `mess_lc_vfs_sendrecv`
            // (fd@0, buf@8, len@16, flags@24, addr@32, addr_len@40).
            let (fd, buf, len, flags, addr, addr_len) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let fd = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[8..16]);
                let buf = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[16..24]);
                let len = u64::from_le_bytes(b8);
                let flags = i32::from_le_bytes([raw[24], raw[25], raw[26], raw[27]]);
                b8.copy_from_slice(&raw[32..40]);
                let addr = u64::from_le_bytes(b8);
                let addr_len = u32::from_le_bytes([raw[40], raw[41], raw[42], raw[43]]);
                (fd, buf, len, flags, addr, addr_len)
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let (dev, filp_flags) = match state.get_sock(fp_slot, fd) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e),
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let writing = matches!(call, VfsCallNum::Sendto);
            // C sdev.c:396-400 — open flags become message flags (non-blocking plus
            // NOSIGPIPE in the write direction), OR-ed bitwise with the user's flags.
            let extra = minix_sockdriver::sdev::sock_msg_flags(
                filp_flags & (crate::fcntl::O_NONBLOCK as i32) != 0,
                writing && filp_flags & (crate::fcntl::O_NOSIGPIPE as i32) != 0,
            ) as i32;
            let call_kind = if writing {
                crate::fproc::SdevCall::Sendto
            } else {
                crate::fproc::SdevCall::Recvfrom
            };
            match state.send_sdev_readwrite(
                worker,
                Some(fp_slot),
                dev,
                Some((buf, len)),
                None,
                Some((addr, addr_len as u64)),
                flags | extra,
                writing,
                call_kind,
            ) {
                Ok(()) => SyscallResult::Suspend,
                Err(e) => SyscallResult::Error(e),
            }
        }

        // ── accept (process-level suspension + three-state finish; see `finish_sdev_blocked`) ──
        VfsCallNum::Accept => {
            // C `do_accept` (socket.c:363-378): `get_sock` → `check_sock_fds(1)`
            // → `sdev_accept(dev, addr, addr_len, flags, fd)` (socket.c's
            // `sdev_accept` uses the same `mess_vfs_lsockdriver_addr` payload as
            // bind/connect, but the address grant is `CPF_WRITE` — the driver writes
            // the peer address back).
            let (fd, addr, addr_len) = {
                // SAFETY: mess_lc_vfs_sockaddr: fd@0, addr@8, addr_len@16.
                let raw = unsafe { &msg.m_u.raw };
                let fd = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[8..16]);
                let addr = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[16..24]);
                (fd, addr, u64::from_le_bytes(b8) as u32)
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let (dev, filp_flags) = match state.get_sock(fp_slot, fd) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e),
            };
            // C socket.c:373-374 — the new socket needs an fd slot, so confirm one is available first.
            let enough = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| crate::filedes::check_fds(fp, 1).is_ok())
                .unwrap_or(false);
            if !enough {
                return SyscallResult::Error(minix_types::EMFILE);
            }
            let drv_e = match crate::device_map::smap_endpt_by_dev(&state.smap_table, dev) {
                Some(e) => e,
                None => return SyscallResult::Error(minix_types::EIO),
            };
            let (_, sock_id) = match crate::device_map::split_smap_dev(dev) {
                Some(p) => p,
                None => return SyscallResult::Error(minix_types::EIO),
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            // C `sdev_accept`: only build a grant for the address buffer when it is **non-zero** (`CPF_WRITE`).
            let grant = if addr != 0 {
                match state.grant_user_buffer(
                    drv_e,
                    user_e,
                    addr,
                    addr_len as u64,
                    minix_types::CpFlags::WRITE,
                ) {
                    Ok(g) => g,
                    Err(_) => return SyscallResult::Error(minix_types::EIO),
                }
            } else {
                minix_types::GRANT_INVALID
            };
            let mut req = minix_types::Message {
                m_type: minix_sockdriver::sdev::SdevRequest::Accept as i32,
                ..minix_types::Message::default()
            };
            // SAFETY: `mess_vfs_lsockdriver_addr` (the same one as bind/connect).
            unsafe {
                let raw = &mut req.m_u.raw;
                raw[0..4].copy_from_slice(&user_e.0.to_le_bytes());
                raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
                raw[8..12].copy_from_slice(&grant.to_le_bytes());
                raw[12..16].copy_from_slice(&addr_len.to_le_bytes());
                raw[16..20].copy_from_slice(&user_e.0.to_le_bytes());
                let sflags = if filp_flags & (crate::fcntl::O_NONBLOCK as i32) != 0 {
                    minix_sockdriver::sdev::SDEV_NONBLOCK as i32
                } else {
                    0
                };
                raw[20..24].copy_from_slice(&sflags.to_le_bytes());
            }
            if minix_sys::ipc::IpcTransport::send(&minix_sys::ipc::DirectTrapTransport, drv_e, &req)
                .is_err()
            {
                if grant != minix_types::GRANT_INVALID {
                    // [SD19-gate] Best-effort grant revocation: the primary operation has already succeeded or entered the error path, so a failed revocation does not change its result
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = state.revoke_grant(grant);
                }
                return SyscallResult::Error(minix_types::EIO);
            }
            let block = crate::fproc::SdevBlock {
                dev,
                call: crate::fproc::SdevCall::Accept,
                grants: [
                    if grant != minix_types::GRANT_INVALID {
                        Some(grant)
                    } else {
                        None
                    },
                    None,
                    None,
                ],
                // C `sdev_suspend(dev, grant, GRANT_INVALID, GRANT_INVALID,
                // listen_fd, 0)`: the listening fd is carried in the saved state (the finish reads its flags).
                aux: crate::fproc::SdevAux::Fd(fd as usize),
            };
            match state.suspend_on_sdev(Some(fp_slot), worker, block) {
                Ok(()) => SyscallResult::Suspend,
                Err(e) => SyscallResult::Error(e),
            }
        }

        VfsCallNum::Bind | VfsCallNum::Connect => {
            // C `do_bind`/`do_connect` (socket.c:308-333) + `sdev_bindconn`
            // (sdev.c:179-215): payload `mess_lc_vfs_sockaddr`
            // (fd@0, addr@8, addr_len@16).
            let (fd, addr, addr_len) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let fd = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[8..16]);
                let addr = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[16..24]);
                (fd, addr, u64::from_le_bytes(b8) as u32)
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let (dev, filp_flags) = match state.get_sock(fp_slot, fd) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e),
            };
            let drv_e = match crate::device_map::smap_endpt_by_dev(&state.smap_table, dev) {
                Some(e) => e,
                None => return SyscallResult::Error(minix_types::EIO),
            };
            let (_, sock_id) = match crate::device_map::split_smap_dev(dev) {
                Some(p) => p,
                None => return SyscallResult::Error(minix_types::EIO),
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            // C sdev.c:192-194 — magic grant for the address buffer (the driver only reads it).
            let grant = match state.grant_user_buffer(
                drv_e,
                user_e,
                addr,
                addr_len as u64,
                minix_types::CpFlags::READ,
            ) {
                Ok(g) => g,
                Err(_) => return SyscallResult::Error(minix_types::EIO),
            };
            let req_type = if matches!(call, VfsCallNum::Connect) {
                minix_sockdriver::sdev::SdevRequest::Connect
            } else {
                minix_sockdriver::sdev::SdevRequest::Bind
            };
            let mut req = minix_types::Message {
                m_type: req_type as i32,
                ..minix_types::Message::default()
            };
            // SAFETY: `mess_vfs_lsockdriver_addr { int32_t req_id; int32_t
            // sock_id; cp_grant_id_t grant; unsigned int len; endpoint_t
            // user_endpt; int sflags; }` (of the `minix/ipc.h:2261-2269` kind).
            unsafe {
                let raw = &mut req.m_u.raw;
                raw[0..4].copy_from_slice(&user_e.0.to_le_bytes());
                raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
                raw[8..12].copy_from_slice(&grant.to_le_bytes());
                raw[12..16].copy_from_slice(&addr_len.to_le_bytes());
                raw[16..20].copy_from_slice(&user_e.0.to_le_bytes());
                let sflags = if filp_flags & (crate::fcntl::O_NONBLOCK as i32) != 0 {
                    minix_sockdriver::sdev::SDEV_NONBLOCK as i32
                } else {
                    0
                };
                raw[20..24].copy_from_slice(&sflags.to_le_bytes());
            }
            // C `asynsend3`: send and **do not wait** (these calls may take a long time).
            if minix_sys::ipc::IpcTransport::send(&minix_sys::ipc::DirectTrapTransport, drv_e, &req)
                .is_err()
            {
                // [SD19-gate] Best-effort grant revocation: the primary operation has already succeeded or entered the error path, so a failed revocation does not change its result
                #[allow(clippy::let_underscore_must_use)]
                let _ = state.revoke_grant(grant);
                return SyscallResult::Error(minix_types::EIO);
            }
            // Process-level suspension: record the site + release the worker slot (C `sdev_suspend`).
            let block = crate::fproc::SdevBlock {
                dev,
                call: if matches!(call, VfsCallNum::Connect) {
                    crate::fproc::SdevCall::Connect
                } else {
                    crate::fproc::SdevCall::Bind
                },
                grants: [Some(grant), None, None],
                aux: crate::fproc::SdevAux::None,
            };
            match state.suspend_on_sdev(Some(fp_slot), worker, block) {
                Ok(()) => SyscallResult::Suspend,
                Err(e) => SyscallResult::Error(e),
            }
        }

        VfsCallNum::Listen => {
            // C `do_listen` (socket.c:334-357): `get_sock` → clamp a negative backlog
            // to zero (`socket::clamp_backlog`) → `sdev_listen(dev, backlog)`
            // (`SDEV_LISTEN` + `param = backlog`, through `sdev_simple`).
            let (fd, backlog) = {
                // SAFETY: `mess_lc_vfs_listen { int fd; int backlog; }`
                // (of the ipc.h:851-856 kind).
                let raw = unsafe { &msg.m_u.raw };
                (
                    i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]),
                    i32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]),
                )
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let (dev, _flags) = match state.get_sock(fp_slot, fd) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e),
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            match state.send_sdev_simple(
                worker,
                Some(fp_slot),
                dev,
                minix_sockdriver::sdev::SdevRequest::Listen as i32,
                crate::socket::clamp_backlog(backlog) as i32,
            ) {
                Ok(()) => SyscallResult::Suspend,
                Err(e) => SyscallResult::Error(e),
            }
        }

        VfsCallNum::Pipe2 => {
            // C `do_pipe2` (pipe.c:39-55) + `create_pipe` (pipe.c:58-135):
            // payload `mess_lc_vfs_pipe2` (flags@0, _unused@4, oflags@8) — the two
            // flag fields are OR-ed together (backward compatibility); the reply payload is `m_vfs_lc_fdpair`.
            let (flags, oflags) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let flags = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let oflags = i32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]);
                (flags, oflags)
            };
            let flags = flags | oflags;
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            // C pipe.c:70-71 — `find_vmnt(PFS_PROC_NR)`; failing that is a panic
            // ("PFS gone"). The Rust side **does not panic**: PFS is a server in a
            // separate work stream, so when it is not mounted this fails closed with EIO
            // (it does not pretend the pipe was created).
            let Some(vmnt_id) = state.vmnt_table.find_by_fs(minix_types::Endpoint::PFS) else {
                return SyscallResult::Error(minix_types::EIO);
            };
            // C pipe.c:75-78 — reserve a vnode (`get_free_vnode`).
            let vnode = match state.vnode_table.alloc() {
                Ok(v) => v,
                Err(_) => return SyscallResult::Error(minix_types::ENFILE),
            };
            // C pipe.c:81-110 — two fds + two filps (read end R, write end W); if
            // the second step fails, `rollback_for` must undo the first step.
            let (fd0, filp0) = {
                let Some(fp) = state.fproc_table.get_mut(fp_slot) else {
                    return SyscallResult::Error(minix_types::EINVAL);
                };
                use crate::filedes::FdAllocPolicy;
                let idx = match crate::filedes::LowestFree.allocate(&fp.filps, 0) {
                    Some(i) => i,
                    None => return SyscallResult::Error(minix_types::EMFILE),
                };
                let fd = match crate::filedes::Fd::new(idx) {
                    Some(f) => f,
                    None => return SyscallResult::Error(minix_types::EMFILE),
                };
                let filp = match state.filp_table.alloc_filp(crate::open::R_BIT) {
                    Ok(f) => f,
                    Err(_) => return SyscallResult::Error(minix_types::ENFILE),
                };
                state.filp_table.inc_count(filp);
                if let Some(fp) = state.fproc_table.get_mut(fp_slot) {
                    fp.filps[idx] = Some(filp.get());
                }
                (fd, filp)
            };
            let (fd1, filp1) = {
                let Some(fp) = state.fproc_table.get_mut(fp_slot) else {
                    return SyscallResult::Error(minix_types::EINVAL);
                };
                use crate::filedes::FdAllocPolicy;
                let idx = match crate::filedes::LowestFree.allocate(&fp.filps, 0) {
                    Some(i) => i,
                    None => {
                        // `rollback_for(FdWrite)`: undo the read end and the vnode.
                        let plan = crate::pipe::rollback_for(crate::pipe::CreateStage::FdWrite);
                        if plan.free_read {
                            fp.filps[fd0.get()] = None;
                            state.filp_table.dec_count(filp0);
                        }
                        if plan.free_vnode
                            && let Some(v) = state.vnode_table.get_mut(vnode)
                        {
                            v.ref_count = 0;
                        }
                        return SyscallResult::Error(minix_types::EMFILE);
                    }
                };
                let fd = match crate::filedes::Fd::new(idx) {
                    Some(f) => f,
                    None => return SyscallResult::Error(minix_types::EMFILE),
                };
                let filp = match state.filp_table.alloc_filp(crate::open::W_BIT) {
                    Ok(f) => f,
                    Err(_) => return SyscallResult::Error(minix_types::ENFILE),
                };
                state.filp_table.inc_count(filp);
                if let Some(fp) = state.fproc_table.get_mut(fp_slot) {
                    fp.filps[idx] = Some(filp.get());
                }
                (fd, filp)
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            let (uid, gid) = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| (fp.eff_uid, fp.eff_gid))
                .unwrap_or((0, 0));
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Pipe2 {
                    filp0: filp0.get(),
                    filp1: filp1.get(),
                    fd0: fd0.get() as u32,
                    fd1: fd1.get() as u32,
                    flags,
                    vnode: vnode.get(),
                });
            }
            state.pending_fs = Some(crate::main_loop::PendingFs {
                vmnt: vmnt_id.0,
                fs_e: minix_types::Endpoint::PFS,
                worker,
                grant: 0, // no data plane
                user: user_e,
                // C pipe.c:112-113: `req_newnode(PFS, effuid, effgid,
                // I_NAMED_PIPE, NO_DEV, &res)`.
                req: crate::request::encode_newnode(
                    minix_types::NO_DEV,
                    crate::open::S_IFIFO | 0o600,
                    uid,
                    gid,
                ),
            });
            SyscallResult::Suspend
        }

        VfsCallNum::Copyfd => {
            // C `do_copyfd` (filedes.c:524-650): payload `mess_lsys_vfs_copyfd`
            // (endpt@0, fd@4, what@8). **Entirely local table operations** (no
            // FS/driver round trip); both decision and execution live in
            // `filedes::copy_fd` (prepared in the earlier fd-copy work).
            let (endpt_raw, fd, what) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let endpt_raw = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let fd = i32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]);
                let what = i32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]);
                (endpt_raw, fd, what)
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            // C filedes.c:540-541 — `flags = what & COPYFD_FLAGS; what &=
            // ~COPYFD_FLAGS`: the low four bits are the operation kind, `0x8000` is the CLOEXEC bit.
            let flags = what & 0xF000;
            let op = what & !0xF000;
            let kind = match op {
                0 => crate::filedes::CopyKind::From,
                1 => crate::filedes::CopyKind::To,
                2 => crate::filedes::CopyKind::Close,
                _ => return SyscallResult::Error(minix_types::EINVAL),
            };
            let cloexec = flags & 0x8000 != 0;
            // C filedes.c:543-545 — `isokendpt(endpt)`: the remote end must be a
            // **live** process. Note that checking "the slot is in range" is not
            // enough: `FProcTable` slots are preallocated, so even an empty slot
            // yields a value from `get()` — use `is_ok_endpoint` (the endpoint
            // matches the one recorded in the slot, which is exactly C's generation check).
            let remote_ep = minix_types::Endpoint(endpt_raw);
            let remote_slot = match state.fproc_table.is_ok_endpoint(remote_ep) {
                Ok(s) => s,
                Err(_) => return SyscallResult::Error(minix_types::EINVAL),
            };
            if fd < 0 {
                return SyscallResult::Error(minix_types::EBADF);
            }
            let fd = match crate::filedes::Fd::new(fd as usize) {
                Some(f) => f,
                None => return SyscallResult::Error(minix_types::EBADF),
            };
            let is_super = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.eff_uid == crate::link::SU_UID)
                .unwrap_or(false);
            // Borrow both slots at once (`get_two_mut`) — `copy_fd` needs the
            // caller's and the remote end's fd tables simultaneously; the other
            // tables are borrowed through `CopyFdCtx`.
            let caller_ep = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            let (caller_fp, remote_fp) = match state
                .fproc_table
                .get_two_mut(fp_slot, remote_slot)
            {
                Some(pair) => pair,
                None => return SyscallResult::Error(minix_types::EINVAL),
            };
            let result = {
                let mut ctx = crate::filedes::CopyFdCtx {
                    filp_table: &mut state.filp_table,
                    vnode_table: &state.vnode_table,
                    smap_table: &state.smap_table,
                    policy: &crate::filedes::LowestFree,
                    caller_endpoint: caller_ep,
                    remote_slot,
                    is_super,
                    cloexec,
                };
                crate::filedes::copy_fd(caller_fp, remote_fp, fd, kind, ctx)
            };
            match result {
                Ok(new_fd) => SyscallResult::Ok(new_fd.get() as i32),
                Err(e) => SyscallResult::Error(e.to_errno()),
            }
        }

        VfsCallNum::Ioctl => {
            // C `do_ioctl` (device.c:18-58): payload `mess_lc_vfs_ioctl`
            // (fd@0, req@8, arg@16). After taking the filp/vnode it dispatches by
            // **file type**: block devices go to `bdev_ioctl`, character devices
            // to `cdev_io(CDEV_IOCTL, ...)`, sockets to `sdev_ioctl`, everything
            // else is `ENOTTY` (C's `default`).
            let (fd, req, arg) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let fd = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[8..16]);
                let req = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[16..24]);
                (fd, req, u64::from_le_bytes(b8))
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            if fd < 0 {
                return SyscallResult::Error(minix_types::EBADF);
            }
            let (filp_idx, vnode_idx) = {
                let fp = match state.fproc_table.get(fp_slot) {
                    Some(fp) => fp,
                    None => return SyscallResult::Error(minix_types::EINVAL),
                };
                let filp_idx = match fp.filps.get(fd as usize).copied().flatten() {
                    Some(idx) => idx,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                match state
                    .filp_table
                    .get(crate::filp::FilpId(filp_idx))
                    .and_then(|f| f.vnode)
                {
                    Some(v) => (filp_idx, v),
                    None => return SyscallResult::Error(minix_types::EBADF),
                }
            };
            let (mode, vnode_sdev) = match state.vnode_table.get(crate::vnode::VnodeId(vnode_idx)) {
                Some(v) => (v.mode, v.sdev),
                None => return SyscallResult::Error(minix_types::EBADF),
            };
            // `filp_flags`: C `cdev_io(..., f->filp_flags)` uses it to derive
            // `CDEV_NONBLOCK`.
            let filp_flags = {
                let fp = match state.fproc_table.get(fp_slot) {
                    Some(fp) => fp,
                    None => return SyscallResult::Error(minix_types::EINVAL),
                };
                match fp.filps.get(fd as usize).copied().flatten() {
                    Some(idx) => state
                        .filp_table
                        .get(crate::filp::FilpId(idx))
                        .map(|f| f.flags)
                        .unwrap_or(0),
                    None => return SyscallResult::Error(minix_types::EBADF),
                }
            };
            // Type dispatch (decision function `device_map::ioctl_route`): a
            // non-device file is `ENOTTY` — that is a **local decision**, unrelated
            // to any device conversation, so it is wired first.
            let target = match crate::device_map::ioctl_route(crate::open::FileType::from(mode)) {
                Ok(t) => t,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            match target {
                crate::device_map::IoctlTarget::Char => {
                    // The `S_IFCHR` branch of C `do_ioctl` (device.c:44-46) →
                    // `cdev_io(CDEV_IOCTL, vp->v_sdev, who_e, arg, 0, request,
                    // filp_flags)` (cdev.c:277-340): dmap driver lookup by major →
                    // `make_ioctl_grant` (access/size decoded from the request bits) →
                    // `CDEV_IOCTL` message → wait for the reply.
                    // C `major(x)`/`minor(x)` (sys/types.h:290-292):
                    // `major = (dev & 0x000fff00) >> 8`;
                    // `minor = ((dev & 0xfff00000) >> 12) | (dev & 0xff)`.
                    let major = ((vnode_sdev & 0x000fff00) >> 8) as u32;
                    let minor = (((vnode_sdev & 0xfff0_0000) >> 12) | (vnode_sdev & 0xff)) as u32;
                    // C `cdev_io` (cdev.c:296-303): VFS handles TIOCSCTTY
                    // locally for TTY/PTY majors — sets fp->fp_tty and
                    // (C also forwards, but our TTY driver returns ENOTTY
                    // on unhandled ioctls; the semantic effect — recording
                    // the controlling terminal — is the only thing INIT
                    // needs from this call, so short-circuit with OK).
                    if req == minix_sys::tty::TIOCSCTTY
                        && (major == crate::device_map::TTY_MAJOR
                            || major == crate::device_map::PTY_MAJOR)
                    {
                        if let Some(fp) = state.fproc_table.get_mut(fp_slot) {
                            fp.tty = vnode_sdev;
                        }
                        return SyscallResult::Ok(minix_types::OK);
                    }
                    let drv_e = match crate::device_map::get_by_major(&state.dmap_table, major)
                        .and_then(|row| row.driver)
                    {
                        Some(e) => e,
                        None => return SyscallResult::Error(minix_types::ENXIO),
                    };
                    let Some(worker) = state.current_worker else {
                        return SyscallResult::Error(minix_types::EAGAIN);
                    };
                    let user_e = state
                        .fproc_table
                        .get(fp_slot)
                        .map(|fp| fp.endpoint)
                        .unwrap_or(minix_types::Endpoint::NONE);
                    // C `make_ioctl_grant`: the request bits decode to access and
                    // size, then a magic grant is made (`CPF_WRITE`/`CPF_READ` from the direction bits).
                    let access = minix_types::CpFlags::from_bits_truncate(
                        crate::device_map::ioctl_access(req),
                    );
                    let size = crate::device_map::ioctl_size(req);
                    let grant = if size > 0 && arg != 0 {
                        match state.grant_user_buffer(drv_e, user_e, arg, size, access) {
                            Ok(g) => g,
                            Err(_) => return SyscallResult::Error(minix_types::EIO),
                        }
                    } else {
                        minix_types::GRANT_INVALID
                    };
                    let mut m = minix_types::Message {
                        // Message number = base value + ordinal (0x404, inside the
                        // CDEV_RQ_BASE gate at `com.h:923`) — `as i32` only yields the
                        // enum ordinal, which the driver-side `is_char_request` mask
                        // test does not recognize; `message_type()` must be used (same
                        // as the character data arm).
                        m_type: minix_chardriver::protocol::CdevRequest::Ioctl.message_type(),
                        ..minix_types::Message::default()
                    };
                    // SAFETY: `mess_vfs_lchardriver_readwrite { off_t pos@0;
                    // cp_grant_id_t grant@8; size_t count@16; unsigned long
                    // request@24; int flags@32; endpoint_t id@36; endpoint_t
                    // user@40; devminor_t minor@44 }` (ipc.h:2238-2249).
                    unsafe {
                        let raw = &mut m.m_u.raw;
                        raw[8..12].copy_from_slice(&grant.to_le_bytes());
                        raw[24..32].copy_from_slice(&req.to_le_bytes());
                        let mut flags = 0i32;
                        if filp_flags & (crate::fcntl::O_NONBLOCK as i32) != 0 {
                            flags |= minix_chardriver::protocol::CDEV_NONBLOCK as i32;
                        }
                        raw[32..36].copy_from_slice(&flags.to_le_bytes());
                        raw[36..40].copy_from_slice(&user_e.0.to_le_bytes());
                        raw[40..44].copy_from_slice(&user_e.0.to_le_bytes());
                        raw[44..46].copy_from_slice(&(minor as u16).to_le_bytes());
                    }
                    if let Some(wp) = state.worker_pool.get_mut(worker) {
                        wp.cont = Some(crate::worker::WorkerCont::CdevIoctl { grant });
                    }
                    return match state.send_drv_for_slot(worker, Some(fp_slot), drv_e, &m) {
                        Ok(()) => SyscallResult::Suspend,
                        Err(e) => {
                            if grant != minix_types::GRANT_INVALID {
                                // [SD19-gate] Best-effort grant revocation: the primary operation has already succeeded or entered the error path, so a failed revocation does not change its result
                                #[allow(clippy::let_underscore_must_use)]
                                let _ = state.revoke_grant(grant);
                            }
                            SyscallResult::Error(e)
                        }
                    };
                }
                crate::device_map::IoctlTarget::Sock => {
                    // The `S_IFSOCK` branch of C `do_ioctl` (device.c:48-50) →
                    // `sdev_ioctl(dev, request, buf, filp_flags)`
                    // (sdev.c:415-448): smap driver lookup by device number →
                    // `make_ioctl_grant` → `SDEV_IOCTL` message → **process-level
                    // suspension** (socket calls may wait).
                    let drv_e = match crate::device_map::smap_endpt_by_dev(&state.smap_table, vnode_sdev)
                    {
                        Some(e) => e,
                        None => return SyscallResult::Error(minix_types::EIO),
                    };
                    let (_, sock_id) = match crate::device_map::split_smap_dev(vnode_sdev) {
                        Some(p) => p,
                        None => return SyscallResult::Error(minix_types::EIO),
                    };
                    let Some(worker) = state.current_worker else {
                        return SyscallResult::Error(minix_types::EAGAIN);
                    };
                    let user_e = state
                        .fproc_table
                        .get(fp_slot)
                        .map(|fp| fp.endpoint)
                        .unwrap_or(minix_types::Endpoint::NONE);
                    // C `make_ioctl_grant` (the same decoding as the character-device branch).
                    let access = minix_types::CpFlags::from_bits_truncate(
                        crate::device_map::ioctl_access(req),
                    );
                    let size = crate::device_map::ioctl_size(req);
                    let grant = if size > 0 && arg != 0 {
                        match state.grant_user_buffer(drv_e, user_e, arg, size, access) {
                            Ok(g) => g,
                            Err(_) => return SyscallResult::Error(minix_types::EIO),
                        }
                    } else {
                        minix_types::GRANT_INVALID
                    };
                    let mut m = minix_types::Message {
                        m_type: minix_sockdriver::sdev::SdevRequest::Ioctl as i32,
                        ..minix_types::Message::default()
                    };
                    // SAFETY: `mess_vfs_lsockdriver_ioctl { int32_t req_id@0;
                    // int32_t sock_id@4; unsigned long request@8;
                    // cp_grant_id_t grant@16; endpoint_t user_endpt@20;
                    // int sflags@24 }` (ipc.h:2284-2293).
                    unsafe {
                        let raw = &mut m.m_u.raw;
                        raw[0..4].copy_from_slice(&user_e.0.to_le_bytes());
                        raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
                        raw[8..16].copy_from_slice(&req.to_le_bytes());
                        raw[16..20].copy_from_slice(&grant.to_le_bytes());
                        raw[20..24].copy_from_slice(&user_e.0.to_le_bytes());
                        let sflags = if filp_flags & (crate::fcntl::O_NONBLOCK as i32) != 0 {
                            minix_sockdriver::sdev::SDEV_NONBLOCK as i32
                        } else {
                            0
                        };
                        raw[24..28].copy_from_slice(&sflags.to_le_bytes());
                    }
                    if minix_sys::ipc::IpcTransport::send(
                        &minix_sys::ipc::DirectTrapTransport,
                        drv_e,
                        &m,
                    )
                    .is_err()
                    {
                        if grant != minix_types::GRANT_INVALID {
                            // [SD19-gate] Best-effort grant revocation: the primary operation has already succeeded or entered the error path, so a failed revocation does not change its result
                            #[allow(clippy::let_underscore_must_use)]
                            let _ = state.revoke_grant(grant);
                        }
                        return SyscallResult::Error(minix_types::EIO);
                    }
                    let block = crate::fproc::SdevBlock {
                        dev: vnode_sdev,
                        call: crate::fproc::SdevCall::Ioctl,
                        grants: [
                            if grant != minix_types::GRANT_INVALID {
                                Some(grant)
                            } else {
                                None
                            },
                            None,
                            None,
                        ],
                        aux: crate::fproc::SdevAux::None,
                    };
                    return match state.suspend_on_sdev(Some(fp_slot), worker, block) {
                        Ok(()) => SyscallResult::Suspend,
                        Err(e) => SyscallResult::Error(e),
                    };
                }
                crate::device_map::IoctlTarget::Block => {
                    // The `S_IFBLK` branch of C `do_ioctl` (device.c:36-42) →
                    // `bdev_ioctl(dev, who_e, request, arg)` (bdev.c:144-186):
                    // dmap driver lookup by major (none → ENXIO) → `make_ioctl_grant`
                    // → `BDEV_IOCTL` → **worker wait** (block-driver replies are
                    // usually fast). C also sets/clears `filp_ioctl_fp` around the
                    // call (so that changing the same device node during an ioctl
                    // cannot deadlock) — in this model that is `Filp::ioctl_holder`.
                    let major = ((vnode_sdev & 0x000fff00) >> 8) as u32;
                    let minor = (((vnode_sdev & 0xfff0_0000) >> 12) | (vnode_sdev & 0xff)) as u32;
                    let drv_e = match crate::device_map::get_by_major(&state.dmap_table, major)
                        .and_then(|row| row.driver)
                    {
                        Some(e) => e,
                        None => return SyscallResult::Error(minix_types::ENXIO),
                    };
                    let Some(worker) = state.current_worker else {
                        return SyscallResult::Error(minix_types::EAGAIN);
                    };
                    let user_e = state
                        .fproc_table
                        .get(fp_slot)
                        .map(|fp| fp.endpoint)
                        .unwrap_or(minix_types::Endpoint::NONE);
                    let access = minix_types::CpFlags::from_bits_truncate(
                        crate::device_map::ioctl_access(req),
                    );
                    let size = crate::device_map::ioctl_size(req);
                    let grant = if size > 0 && arg != 0 {
                        match state.grant_user_buffer(drv_e, user_e, arg, size, access) {
                            Ok(g) => g,
                            Err(_) => return SyscallResult::Error(minix_types::EIO),
                        }
                    } else {
                        minix_types::GRANT_INVALID
                    };
                    // C `f->filp_ioctl_fp = fp`: record whose ioctl currently holds
                    // this filp (`copyfd` uses it to block deadlocks).
                    if let Some(f) = state.filp_table.get_mut(crate::filp::FilpId(filp_idx)) {
                        f.ioctl_holder = Some(fp_slot);
                    }
                    let mut m = minix_types::Message {
                        m_type: crate::bdev::BdevOp::Ioctl.msg_type() as i32,
                        ..minix_types::Message::default()
                    };
                    // SAFETY: `mess_lbdev_lblockdriver_msg { off_t pos@0;
                    // int minor@8; int id@12; int access@16; int count@20;
                    // cp_grant_id_t grant@24; int flags@28; endpoint_t user@32;
                    // unsigned long request@40 }` (ipc.h:338-353; the offset table
                    // lives in `minix_types::lblockdriver_msg_off`). **The first slot
                    // is `pos`**: every field of the block family is shifted 8 bytes
                    // later than the character family, so sending a block message by
                    // character-family offsets makes the driver see "minor 0, invalid grant".
                    unsafe {
                        let raw = &mut m.m_u.raw;
                        raw[minix_types::lblockdriver_msg_off::MINOR
                            ..minix_types::lblockdriver_msg_off::MINOR + 4]
                            .copy_from_slice(&(minor as i32).to_le_bytes());
                        raw[minix_types::lblockdriver_msg_off::GRANT
                            ..minix_types::lblockdriver_msg_off::GRANT + 4]
                            .copy_from_slice(&grant.to_le_bytes());
                        raw[minix_types::lblockdriver_msg_off::USER
                            ..minix_types::lblockdriver_msg_off::USER + 4]
                            .copy_from_slice(&user_e.0.to_le_bytes());
                        raw[minix_types::lblockdriver_msg_off::REQUEST
                            ..minix_types::lblockdriver_msg_off::REQUEST + 8]
                            .copy_from_slice(&req.to_le_bytes());
                    }
                    if let Some(wp) = state.worker_pool.get_mut(worker) {
                        wp.cont = Some(crate::worker::WorkerCont::BdevIoctl {
                            grant,
                            filp: filp_idx,
                        });
                    }
                    return match state.send_drv_for_slot(worker, Some(fp_slot), drv_e, &m) {
                        Ok(()) => SyscallResult::Suspend,
                        Err(e) => {
                            if grant != minix_types::GRANT_INVALID {
                                // [SD19-gate] Best-effort grant revocation: the primary operation has already succeeded or entered the error path, so a failed revocation does not change its result
                                #[allow(clippy::let_underscore_must_use)]
                                let _ = state.revoke_grant(grant);
                            }
                            if let Some(f) =
                                state.filp_table.get_mut(crate::filp::FilpId(filp_idx))
                            {
                                f.ioctl_holder = None;
                            }
                            SyscallResult::Error(e)
                        }
                    };
                }
            }
        }

        VfsCallNum::Fcntl => {
            // C `do_fcntl` (misc.c:127-300): payload `mess_lc_vfs_fcntl`
            // (fd@0, cmd@4, arg_int@8, arg_ptr@16). Unknown cmd → EINVAL
            // (C's `default: r = EINVAL`).
            // `arg_ptr` is the `struct flock *` for lock commands and `F_FREESP`.
            let (fd, cmd_raw, arg_int, arg_ptr) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let fd = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let cmd = u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]);
                let arg_int = i32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]);
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[16..24]);
                (fd, cmd, arg_int, u64::from_le_bytes(b8))
            };
            let Some(cmd) = crate::fcntl::FcntlCmd::from_raw(cmd_raw) else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            // C misc.c:141-143 — fd gate (lock commands need a write lock, the rest a read lock).
            if fd < 0 {
                return SyscallResult::Error(minix_types::EBADF);
            }
            let filp_idx = {
                let fp = match state.fproc_table.get(fp_slot) {
                    Some(fp) => fp,
                    None => return SyscallResult::Error(minix_types::EINVAL),
                };
                match fp.filps.get(fd as usize).copied().flatten() {
                    Some(idx) => idx,
                    None => return SyscallResult::Error(minix_types::EBADF),
                }
            };
            use crate::fcntl::FcntlCmd as C;
            match cmd {
                // C misc.c:147-163 — replacements for the `dup` family: floor gate →
                // take a new fd → `filp_count++` → write the fd table (the CLOEXEC variant sets it too).
                C::DupFd | C::DupFdCloexec => {
                    if let Err(e) = crate::fcntl::dupfd_arg_check(arg_int) {
                        return SyscallResult::Error(e.to_errno());
                    }
                    // Unlike open, the dup family **shares** the same filp (C's
                    // `f->filp_count++` + `fp_filp[new_fd] = f`), so it only takes an
                    // fd slot and does not allocate a filp — ask the allocation
                    // policy for the lowest free slot directly.
                    let (new_fd, _) = {
                        let Some(fp) = state.fproc_table.get_mut(fp_slot) else {
                            return SyscallResult::Error(minix_types::EINVAL);
                        };
                        use crate::filedes::FdAllocPolicy;
                        let idx = match crate::filedes::LowestFree
                            .allocate(&fp.filps, arg_int as usize)
                        {
                            Some(i) => i,
                            None => return SyscallResult::Error(minix_types::EMFILE),
                        };
                        let fd = match crate::filedes::Fd::new(idx) {
                            Some(f) => f,
                            None => return SyscallResult::Error(minix_types::EMFILE),
                        };
                        fp.filps[idx] = Some(filp_idx);
                        if matches!(cmd, C::DupFdCloexec) {
                            fp.cloexec_set.set(idx, true);
                        }
                        (fd, idx)
                    };
                    state.filp_table.inc_count(crate::filp::FilpId(filp_idx));
                    let _ = new_fd;
                    return SyscallResult::Ok(new_fd.get() as i32);
                }
                // C misc.c:165-172 — read/write the close-on-exec bit.
                C::GetFd => {
                    let set = state
                        .fproc_table
                        .get(fp_slot)
                        .map(|fp| fp.cloexec_set.get(fd as usize))
                        .unwrap_or(false);
                    return SyscallResult::Ok(crate::fcntl::cloexec_get(set) as i32);
                }
                C::SetFd => {
                    if let Some(fp) = state.fproc_table.get_mut(fp_slot) {
                        fp.cloexec_set
                            .set(fd as usize, crate::fcntl::cloexec_apply(arg_int as u32));
                    }
                    return SyscallResult::Ok(0);
                }
                // C misc.c:174-187 — status word (only bits such as
                // `O_NONBLOCK|O_APPEND|O_ACCMODE` pass the gate).
                C::GetFl => {
                    let flags = state
                        .filp_table
                        .get(crate::filp::FilpId(filp_idx))
                        .map(|f| f.flags as u32)
                        .unwrap_or(0);
                    return SyscallResult::Ok(crate::fcntl::status_get(flags) as i32);
                }
                C::SetFl => {
                    if let Some(f) = state.filp_table.get_mut(crate::filp::FilpId(filp_idx)) {
                        f.flags = crate::fcntl::status_set(f.flags as u32, arg_int as u32) as i32;
                    }
                    return SyscallResult::Ok(0);
                }
                // C misc.c:250-256 — the `O_NOSIGPIPE` sentinel.
                C::GetNoSigPipe => {
                    let flags = state
                        .filp_table
                        .get(crate::filp::FilpId(filp_idx))
                        .map(|f| f.flags as u32)
                        .unwrap_or(0);
                    return SyscallResult::Ok(crate::fcntl::nosigpipe_get(flags) as i32);
                }
                C::SetNoSigPipe => {
                    if let Some(f) = state.filp_table.get_mut(crate::filp::FilpId(filp_idx)) {
                        f.flags = crate::fcntl::nosigpipe_set(f.flags as u32, arg_int as u32) as i32;
                    }
                    return SyscallResult::Ok(0);
                }
                // C misc.c:258-275 — only the superuser may flush the cache; the
                // target depends on the file type (block devices flush their own
                // device blocks, regular files/directories flush the hosting FS).
                C::FlushFsCache => {
                    let (is_root, mode) = {
                        let fp = match state.fproc_table.get(fp_slot) {
                            Some(fp) => fp,
                            None => return SyscallResult::Error(minix_types::EINVAL),
                        };
                        let vnode_idx = match state
                            .filp_table
                            .get(crate::filp::FilpId(filp_idx))
                            .and_then(|f| f.vnode)
                        {
                            Some(v) => v,
                            None => return SyscallResult::Error(minix_types::EBADF),
                        };
                        let mode = match state.vnode_table.get(crate::vnode::VnodeId(vnode_idx)) {
                            Some(v) => v.mode,
                            None => return SyscallResult::Error(minix_types::EBADF),
                        };
                        (fp.eff_uid == crate::link::SU_UID, mode)
                    };
                    let ft = crate::open::FileType::from(mode);
                    let target = match crate::fcntl::flush_target(is_root, ft) {
                        Ok(t) => t,
                        Err(e) => return SyscallResult::Error(e.to_errno()),
                    };
                    // Target endpoint and device number: block devices use
                    // `v_bfs_e`/`v_sdev` (the block driver's FS and device), regular
                    // files/directories use `v_fs_e`/`v_dev` (the hosting FS).
                    let (fs_e, dev) = {
                        let vnode_idx = match state
                            .filp_table
                            .get(crate::filp::FilpId(filp_idx))
                            .and_then(|f| f.vnode)
                        {
                            Some(v) => v,
                            None => return SyscallResult::Error(minix_types::EBADF),
                        };
                        let v = match state.vnode_table.get(crate::vnode::VnodeId(vnode_idx)) {
                            Some(v) => v,
                            None => return SyscallResult::Error(minix_types::EBADF),
                        };
                        match target {
                            crate::fcntl::FlushTarget::BlockDev => (v.bfs, v.sdev),
                            crate::fcntl::FlushTarget::HostingFs => (v.fs, v.dev),
                        }
                    };
                    let Some(vmnt_id) = state.vmnt_table.find_by_fs(fs_e) else {
                        return SyscallResult::Error(minix_types::EIO);
                    };
                    let Some(worker) = state.current_worker else {
                        return SyscallResult::Error(minix_types::EAGAIN);
                    };
                    let user_e = state
                        .fproc_table
                        .get(fp_slot)
                        .map(|fp| fp.endpoint)
                        .unwrap_or(minix_types::Endpoint::NONE);
                    if let Some(wp) = state.worker_pool.get_mut(worker) {
                        wp.cont = Some(crate::worker::WorkerCont::Status);
                    }
                    state.pending_fs = Some(crate::main_loop::PendingFs {
                        vmnt: vmnt_id.0,
                        fs_e,
                        worker,
                        grant: 0, // no data plane
                        user: user_e,
                        req: crate::request::encode_flush(dev),
                    });
                    return SyscallResult::Suspend;
                }
                // ── Lock family (C `lock_op` lock.c:21-154) ──
                C::GetLk | C::SetLk | C::SetLkw => {
                    let flock_io = crate::fcntl::SysFlockIo {
                        who: state
                            .fproc_table
                            .get(fp_slot)
                            .map(|fp| fp.endpoint)
                            .unwrap_or(minix_types::Endpoint::NONE),
                    };
                    do_fcntl_lock(state, fp_slot, fd as usize, cmd, arg_ptr, &flock_io)
                }
                // ── `F_FREESP` (C misc.c:184-239) ──
                C::FreeSp => {
                    let who = state
                        .fproc_table
                        .get(fp_slot)
                        .map(|fp| fp.endpoint)
                        .unwrap_or(minix_types::Endpoint::NONE);
                    let flock_io = crate::fcntl::SysFlockIo { who };
                    do_fcntl_freesp(state, fp_slot, fd as usize, arg_ptr, &flock_io)
                }
            }
        }

        VfsCallNum::Mapdriver => {
            // C `do_mapdriver` (dmap.c:106-177): payload
            // `mess_lsys_vfs_mapdriver` (major@0, labellen@8, label@16,
            // ndomains@24, domains@28.., NR_DOMAIN = 8). Three gates come before
            // any table lookup: only RS may call (EPERM), the label must fit
            // (EINVAL), and the label must be NUL-terminated (EINVAL).
            let caller = msg.m_source;
            if crate::device_map::check_mapper(caller).is_err() {
                return SyscallResult::Error(minix_types::EPERM);
            }
            let (major, label_len, label_addr, ndomains, domains) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let major = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[8..16]);
                let label_len = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[16..24]);
                let label_addr = u64::from_le_bytes(b8);
                let ndomains = i32::from_le_bytes([raw[24], raw[25], raw[26], raw[27]]);
                // The domain array starts at 28; this rewrite's payload area is 56
                // bytes (`MESSAGE_PAYLOAD_SIZE`, aligned with 32-bit Minix's
                // message) — it can hold only 7 `int`s, while C's `NR_DOMAIN` is 8.
                // The extra domain cannot be read, so `ndomains > 7` is always
                // EINVAL (**never silently drop a domain**: registering one fewer
                // leaves the driver half-crippled).
                let mut doms = [0i32; 7];
                for (i, d) in doms.iter_mut().enumerate() {
                    let at = 28 + i * 4;
                    *d = i32::from_le_bytes([raw[at], raw[at + 1], raw[at + 2], raw[at + 3]]);
                }
                (major, label_len, label_addr, ndomains, doms)
            };
            // C dmap.c:129-133 — a label that does not fit is EINVAL (`LABEL_MAX` = 16).
            if label_len as usize > crate::device_map::LABEL_MAX {
                return SyscallResult::Error(minix_types::EINVAL);
            }
            // Payload-area capacity gate (see the note on the domain array above):
            // C allows up to `NR_DOMAIN` = 8, but this rewrite's 56-byte payload
            // area holds only 7 domains — reject the excess instead of dropping it.
            if ndomains > 7 {
                return SyscallResult::Error(minix_types::EINVAL);
            }
            // This arm **does not need a worker slot**: registration is entirely
            // local table operations (dmap/smap), with no FS/driver round trip, so
            // unlike the other conversation arms it does not wait for a reply.
            // C dmap.c:134-139 — `sys_vircopy(who_e, label_vir, SELF, label,
            // label_len, CP_FLAG_TRY)`: the label lives in **the caller's memory**
            // (cross-space fetch; unreachable on the host → EINVAL, the same value
            // as C's `r != OK → EINVAL`).
            let mut label = [0u8; crate::device_map::LABEL_MAX];
            let n = (label_len as usize).min(crate::device_map::LABEL_MAX);
            match minix_sys::syscall::sys_datacopy(
                &minix_sys::syscall::DirectKernelCallTransport,
                caller.0,
                label_addr,
                minix_types::Endpoint::SELF.0,
                label.as_mut_ptr() as u64,
                n as u64,
            ) {
                Ok(()) => {}
                Err(_) => return SyscallResult::Error(minix_types::EINVAL),
            }
            // C dmap.c:140-144 — must be NUL-terminated.
            if n == 0 || label[n - 1] != 0 {
                return SyscallResult::Error(minix_types::EINVAL);
            }
            let end = label.iter().position(|&b| b == 0).unwrap_or(n);
            let label_str = match core::str::from_utf8(&label[..end]) {
                Ok(s) => s,
                Err(_) => return SyscallResult::Error(minix_types::EINVAL),
            };
            let doms = if ndomains > 0 {
                &domains[..(ndomains as usize).min(7)]
            } else {
                &domains[..0]
            };
            let status = state.finish_mapdriver(caller, label_str, major, doms);
            if status == 0 {
                SyscallResult::Ok(0)
            } else {
                SyscallResult::Error(status)
            }
        }

        VfsCallNum::Getvfsstat => {
            // C `do_getvfsstat` (stadir.c:330-403): payload
            // `mess_lc_vfs_getvfsstat { buf@0, len@8, flags@16 }`. `buf == 0`
            // only counts; otherwise the mount list is truncated to
            // `bufsize / sizeof(struct statvfs)`, filled one by one at
            // `buf + i*sizeof`, and the **count** is returned (not 0).
            let (buf_addr, bufsize, flags) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[0..8]);
                let buf_addr = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[8..16]);
                let bufsize = u64::from_le_bytes(b8);
                let flags = i32::from_le_bytes([raw[16], raw[17], raw[18], raw[19]]);
                (buf_addr, bufsize, flags)
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            // Both outcomes (suspend / finish in place) enter through one arm that always reports suspension — same as Fstatvfs1.
            match state.begin_getvfsstat(worker, Some(fp_slot), buf_addr, bufsize, flags) {
                Ok(()) => SyscallResult::Suspend,
                Err(e) => SyscallResult::Error(e),
            }
        }

        VfsCallNum::Statvfs1 | VfsCallNum::Fstatvfs1 => {
            // C `do_statvfs` (stadir.c:294-326, path half) and `do_fstatvfs`
            // (stadir.c:419-442, fd half): both reduce to `fill_statvfs(vp->v_vmnt,
            // who_e, statbuf, flags)` — payload `mess_lc_vfs_statvfs1`
            // (fd@0, flags@4, len@8, name@16, buf@24).
            let (fd, flags, name_len, name_addr, statbuf) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let fd = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let flags = i32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]);
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[8..16]);
                let name_len = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[16..24]);
                let name_addr = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[24..32]);
                let statbuf = u64::from_le_bytes(b8);
                (fd, flags, name_len, name_addr, statbuf)
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            if matches!(call, VfsCallNum::Fstatvfs1) {
                // C stadir.c:428-431: fd → filp → vnode's `v_vmnt`.
                if fd < 0 {
                    return SyscallResult::Error(minix_types::EBADF);
                }
                let vnode_idx = {
                    let fp = match state.fproc_table.get(fp_slot) {
                        Some(fp) => fp,
                        None => return SyscallResult::Error(minix_types::EINVAL),
                    };
                    let filp_idx = match fp.filps.get(fd as usize).copied().flatten() {
                        Some(idx) => idx,
                        None => return SyscallResult::Error(minix_types::EBADF),
                    };
                    match state
                        .filp_table
                        .get(crate::filp::FilpId(filp_idx))
                        .and_then(|f| f.vnode)
                    {
                        Some(v) => v,
                        None => return SyscallResult::Error(minix_types::EBADF),
                    }
                };
                let fs_e = match state.vnode_table.get(crate::vnode::VnodeId(vnode_idx)) {
                    Some(v) => v.fs,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                let Some(vmnt_idx) = state.vmnt_table.find_by_fs(fs_e) else {
                    return SyscallResult::Error(minix_types::EIO);
                };
                // `begin_statvfs` may have "suspended" (fresh path) or "finished"
                // (`ST_NOWAIT` uses the cache, fills the fields in place and
                // replies) — both outcomes report suspension: on finish the slot is
                // already released and the reply queued, so `run_once` sees Suspend
                // and does not release it again, and `queue_reply(Suspend)` does not
                // overwrite the already-queued reply.
                return match state.begin_statvfs(
                    worker,
                    Some(fp_slot),
                    vmnt_idx.0,
                    statbuf,
                    flags,
                ) {
                    Ok(()) => SyscallResult::Suspend,
                    Err(e) => SyscallResult::Error(e),
                };
            }
            // Path half: `eat_path` (NOFLAGS) at C stadir.c:305-313.
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            let fetcher = crate::path::SysPathFetcher { who: user_e };
            let path = match fetcher.fetch(name_addr, name_len as usize) {
                Ok(p) => p,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let resolve = match crate::path::Lookup::new(
                path,
                crate::path::LookupFlags::NOFLAGS,
            ) {
                Ok(l) => l,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let rd = state.root_dir_of(Some(fp_slot));
            let start = if resolve.path.starts_with('/') {
                crate::path::LookupStart { fs: rd.fs, ino: rd.ino, dev: rd.dev }
            } else {
                let wd = state.work_dir_of(fp_slot);
                crate::path::LookupStart { fs: wd.fs, ino: wd.ino, dev: wd.dev }
            };
            let uid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_uid).unwrap_or(0);
            let gid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_gid).unwrap_or(0);
            let (walk, step) = match crate::path::LookupWalk::begin(start, resolve, rd, uid, gid) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let crate::path::WalkStep::Send { fs_e, dir_ino, root_ino } = step else {
                return SyscallResult::Error(minix_types::EIO);
            };
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Path);
                wp.path = Some(crate::worker::PathPending {
                    walk,
                    grant: 0, // overwritten by send_lookup_for_slot
                    follow: crate::worker::PathFollow::Statvfs {
                        user_buf: statbuf,
                        flags,
                    },
                });
            }
            if state
                .send_lookup_for_slot(worker, Some(fp_slot), fs_e, dir_ino, root_ino)
                .is_err()
            {
                if let Some(wp) = state.worker_pool.get_mut(worker) {
                    wp.cont = None;
                    wp.path = None;
                }
                return SyscallResult::Error(minix_types::EIO);
            }
            SyscallResult::Suspend
        }

        VfsCallNum::Rename => {
            // C `do_rename` (link.c:166-280): payload has the same shape as link
            // (name1@0, name2@8, len1@16, len2@24). Stage 1 walks name1's parent
            // directory (`last_dir`), stage 2 walks name2's parent directory; a
            // sticky-bit sub-walk may be inserted in between (when the old parent
            // directory has `S_ISVTX`).
            let (old_addr, new_addr, old_len, new_len) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[0..8]);
                let old_addr = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[8..16]);
                let new_addr = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[16..24]);
                let old_len = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[24..32]);
                let new_len = u64::from_le_bytes(b8);
                (old_addr, new_addr, old_len, new_len)
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            let fetcher = crate::path::SysPathFetcher { who: user_e };
            let old_path = match fetcher.fetch(old_addr, old_len as usize) {
                Ok(p) => p,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let new_path = match fetcher.fetch(new_addr, new_len as usize) {
                Ok(p) => p,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            // C `do_rename:196`: `last_dir` splits off the old name's last
            // component — it must be **saved** (used when stage 2 sends the
            // request), so it is carried along in the follow state.
            let split = match crate::path::last_dir_split(&old_path) {
                Ok(sp) => sp,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            // `lookup_init(..., PATH_RET_SYMLINK, ...)` at C `do_rename:188`;
            // symlinks in the directory prefix are followed (`path.c:231-235`
            // clears RET_SYMLINK).
            let resolve = match crate::path::Lookup::new(
                split.dir_path.clone(),
                crate::path::LookupFlags::NOFLAGS,
            ) {
                Ok(l) => l,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let rd = state.root_dir_of(Some(fp_slot));
            let start = if resolve.path.starts_with('/') {
                crate::path::LookupStart { fs: rd.fs, ino: rd.ino, dev: rd.dev }
            } else {
                let wd = state.work_dir_of(fp_slot);
                crate::path::LookupStart { fs: wd.fs, ino: wd.ino, dev: wd.dev }
            };
            let uid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_uid).unwrap_or(0);
            let gid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_gid).unwrap_or(0);
            let (walk, step) = match crate::path::LookupWalk::begin(start, resolve, rd, uid, gid) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let crate::path::WalkStep::Send { fs_e, dir_ino, root_ino } = step else {
                return SyscallResult::Error(minix_types::EIO);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Path);
                wp.path = Some(crate::worker::PathPending {
                    walk,
                    grant: 0, // overwritten by send_lookup_for_slot
                    follow: crate::worker::PathFollow::RenameOld {
                        entry: split.entry,
                        new_path,
                    },
                });
            }
            if state
                .send_lookup_for_slot(worker, Some(fp_slot), fs_e, dir_ino, root_ino)
                .is_err()
            {
                if let Some(wp) = state.worker_pool.get_mut(worker) {
                    wp.cont = None;
                    wp.path = None;
                }
                return SyscallResult::Error(minix_types::EIO);
            }
            SyscallResult::Suspend
        }

        VfsCallNum::Link => {
            // C `do_link` (link.c:170-230): payload `mess_lc_vfs_link`
            // (name1@0 = source file path, name2@8 = new link path, len1@16, len2@24).
            // Stage 1 walks the whole of name1 (the source file must exist), stage 2
            // walks name2's parent directory.
            let (src_addr, dst_addr, src_len, dst_len) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[0..8]);
                let src_addr = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[8..16]);
                let dst_addr = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[16..24]);
                let src_len = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[24..32]);
                let dst_len = u64::from_le_bytes(b8);
                (src_addr, dst_addr, src_len, dst_len)
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            let fetcher = crate::path::SysPathFetcher { who: user_e };
            // Both paths live in user memory (cross-space fetch, unavailable on the host).
            let src_path = match fetcher.fetch(src_addr, src_len as usize) {
                Ok(p) => p,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let dst_path = match fetcher.fetch(dst_addr, dst_len as usize) {
                Ok(p) => p,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            // `lookup_init(..., PATH_NOFLAGS, ...)` at C link.c:186.
            let resolve = match crate::path::Lookup::new(
                src_path,
                crate::path::LookupFlags::NOFLAGS,
            ) {
                Ok(l) => l,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let rd = state.root_dir_of(Some(fp_slot));
            let start = if resolve.path.starts_with('/') {
                crate::path::LookupStart { fs: rd.fs, ino: rd.ino, dev: rd.dev }
            } else {
                let wd = state.work_dir_of(fp_slot);
                crate::path::LookupStart { fs: wd.fs, ino: wd.ino, dev: wd.dev }
            };
            let uid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_uid).unwrap_or(0);
            let gid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_gid).unwrap_or(0);
            let (walk, step) = match crate::path::LookupWalk::begin(start, resolve, rd, uid, gid) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let crate::path::WalkStep::Send { fs_e, dir_ino, root_ino } = step else {
                return SyscallResult::Error(minix_types::EIO);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Path);
                wp.path = Some(crate::worker::PathPending {
                    walk,
                    grant: 0, // overwritten by send_lookup_for_slot
                    follow: crate::worker::PathFollow::LinkSrc { dst_path },
                });
            }
            if state
                .send_lookup_for_slot(worker, Some(fp_slot), fs_e, dir_ino, root_ino)
                .is_err()
            {
                if let Some(wp) = state.worker_pool.get_mut(worker) {
                    wp.cont = None;
                    wp.path = None;
                }
                return SyscallResult::Error(minix_types::EIO);
            }
            SyscallResult::Suspend
        }

        VfsCallNum::Symlink => {
            // C `do_slink` (link.c:386-424): payload `mess_lc_vfs_link`
            // (name1@0 = target string, name2@8 = link path, len1@16, len2@24).
            // Two length gates (`<= 1` → ENOENT, `>= _POSIX_SYMLINK_MAX` →
            // ENAMETOOLONG) come before any path fetch.
            let (target_addr, link_addr, target_len, link_len) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[0..8]);
                let target_addr = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[8..16]);
                let link_addr = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[16..24]);
                let target_len = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[24..32]);
                let link_len = u64::from_le_bytes(b8);
                (target_addr, link_addr, target_len, link_len)
            };
            // The two gates at C link.c:400-401 (the decision function
            // `link::check_slink_len` also computes the NUL-free length).
            let mem_size = match crate::link::check_slink_len(target_len as usize) {
                Ok(n) => n as u64,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            let fetcher = crate::path::SysPathFetcher { who: user_e };
            let path = match fetcher.fetch(link_addr, link_len as usize) {
                Ok(p) => p,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let split = match crate::path::last_dir_split(&path) {
                Ok(sp) => sp,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            // `lookup_init(..., PATH_NOFLAGS, ...)` at C link.c:396.
            let resolve = match crate::path::Lookup::new(
                split.dir_path.clone(),
                crate::path::LookupFlags::NOFLAGS,
            ) {
                Ok(l) => l,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let rd = state.root_dir_of(Some(fp_slot));
            let start = if resolve.path.starts_with('/') {
                crate::path::LookupStart { fs: rd.fs, ino: rd.ino, dev: rd.dev }
            } else {
                let wd = state.work_dir_of(fp_slot);
                crate::path::LookupStart { fs: wd.fs, ino: wd.ino, dev: wd.dev }
            };
            let uid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_uid).unwrap_or(0);
            let gid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_gid).unwrap_or(0);
            let (walk, step) = match crate::path::LookupWalk::begin(start, resolve, rd, uid, gid) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let crate::path::WalkStep::Send { fs_e, dir_ino, root_ino } = step else {
                return SyscallResult::Error(minix_types::EIO);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Path);
                wp.path = Some(crate::worker::PathPending {
                    walk,
                    grant: 0, // overwritten by send_lookup_for_slot
                    follow: crate::worker::PathFollow::Slink {
                        entry: split.entry,
                        target_addr,
                        target_len: mem_size,
                    },
                });
            }
            if state
                .send_lookup_for_slot(worker, Some(fp_slot), fs_e, dir_ino, root_ino)
                .is_err()
            {
                if let Some(wp) = state.worker_pool.get_mut(worker) {
                    wp.cont = None;
                    wp.path = None;
                }
                return SyscallResult::Error(minix_types::EIO);
            }
            SyscallResult::Suspend
        }

        VfsCallNum::Utimens => {
            // C `do_utimens` (time.c:44-160): payload `mess_vfs_utimens`
            // (atime@0, mtime@8, ansec@16, mnsec@24, len@32, name@40,
            // fd@48, flags@52). `name != NULL` takes the path half (where flags
            // may only be `AT_SYMLINK_NOFOLLOW`), otherwise the fd half (flags must be 0).
            let (atime, mtime, ansec, mnsec, name_len, name_addr, fd, flags) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[0..8]);
                let atime = i64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[8..16]);
                let mtime = i64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[16..24]);
                let ansec = i64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[24..32]);
                let mnsec = i64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[32..40]);
                let name_len = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[40..48]);
                let name_addr = u64::from_le_bytes(b8);
                let fd = i32::from_le_bytes([raw[48], raw[49], raw[50], raw[51]]);
                let flags = u32::from_le_bytes([raw[52], raw[53], raw[54], raw[55]]);
                (atime, mtime, ansec, mnsec, name_len, name_addr, fd, flags)
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            if name_addr == 0 {
                // fd half (C time.c:98-108): flags must be 0.
                if flags != 0 {
                    return SyscallResult::Error(minix_types::EINVAL);
                }
                if fd < 0 {
                    return SyscallResult::Error(minix_types::EBADF);
                }
                let vnode_idx = {
                    let fp = match state.fproc_table.get(fp_slot) {
                        Some(fp) => fp,
                        None => return SyscallResult::Error(minix_types::EINVAL),
                    };
                    let filp_idx = match fp.filps.get(fd as usize).copied().flatten() {
                        Some(idx) => idx,
                        None => return SyscallResult::Error(minix_types::EBADF),
                    };
                    match state
                        .filp_table
                        .get(crate::filp::FilpId(filp_idx))
                        .and_then(|f| f.vnode)
                    {
                        Some(v) => v,
                        None => return SyscallResult::Error(minix_types::EBADF),
                    }
                };
                let (fs_e, ino, node_uid, node_gid, node_mode) =
                    match state.vnode_table.get(crate::vnode::VnodeId(vnode_idx)) {
                        Some(v) => (v.fs, v.ino, v.uid, v.gid, v.mode),
                        None => return SyscallResult::Error(minix_types::EBADF),
                    };
                state.finish_utimens(
                    worker,
                    Some(fp_slot),
                    fs_e,
                    ino,
                    node_uid,
                    node_gid,
                    node_mode,
                    (atime, ansec),
                    (mtime, mnsec),
                );
                return SyscallResult::Suspend;
            }
            // Path half (C time.c:76-96): an unknown flag is EINVAL;
            // `AT_SYMLINK_NOFOLLOW` selects the walk flags (do not follow the last-component symlink).
            if flags & !crate::open::AT_SYMLINK_NOFOLLOW != 0 {
                return SyscallResult::Error(minix_types::EINVAL);
            }
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            let fetcher = crate::path::SysPathFetcher { who: user_e };
            let path = match fetcher.fetch(name_addr, name_len as usize) {
                Ok(p) => p,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let lookup_flags = if flags & crate::open::AT_SYMLINK_NOFOLLOW != 0 {
                crate::path::LookupFlags::RET_SYMLINK
            } else {
                crate::path::LookupFlags::NOFLAGS
            };
            let resolve = match crate::path::Lookup::new(path, lookup_flags) {
                Ok(l) => l,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let rd = state.root_dir_of(Some(fp_slot));
            let start = if resolve.path.starts_with('/') {
                crate::path::LookupStart { fs: rd.fs, ino: rd.ino, dev: rd.dev }
            } else {
                let wd = state.work_dir_of(fp_slot);
                crate::path::LookupStart { fs: wd.fs, ino: wd.ino, dev: wd.dev }
            };
            let uid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_uid).unwrap_or(0);
            let gid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_gid).unwrap_or(0);
            let (walk, step) = match crate::path::LookupWalk::begin(start, resolve, rd, uid, gid) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let crate::path::WalkStep::Send { fs_e, dir_ino, root_ino } = step else {
                return SyscallResult::Error(minix_types::EIO);
            };
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Path);
                wp.path = Some(crate::worker::PathPending {
                    walk,
                    grant: 0, // overwritten by send_lookup_for_slot
                    follow: crate::worker::PathFollow::Utimens {
                        atime: (atime, ansec),
                        mtime: (mtime, mnsec),
                        flags,
                    },
                });
            }
            if state
                .send_lookup_for_slot(worker, Some(fp_slot), fs_e, dir_ino, root_ino)
                .is_err()
            {
                if let Some(wp) = state.worker_pool.get_mut(worker) {
                    wp.cont = None;
                    wp.path = None;
                }
                return SyscallResult::Error(minix_types::EIO);
            }
            SyscallResult::Suspend
        }

        VfsCallNum::Mknod => {
            // C `do_mknod` (open.c:514-556): payload `mess_lc_vfs_mknod`
            // (device@0, name@8, len@16, mode@24) → only the superuser may create
            // non-FIFO nodes (otherwise EPERM) → narrow the mode by umask →
            // `last_dir` walks to the parent directory.
            let (dev, name_len, name_addr, mode_bits) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[0..8]);
                let dev = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[8..16]);
                let name_addr = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[16..24]);
                let name_len = u64::from_le_bytes(b8);
                let mode_bits = u32::from_le_bytes([raw[24], raw[25], raw[26], raw[27]]);
                (dev, name_len, name_addr, mode_bits)
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let (eff_uid, umask) = match state.fproc_table.get(fp_slot) {
                Some(fp) => (fp.eff_uid, fp.umask),
                None => return SyscallResult::Error(minix_types::EINVAL),
            };
            // C open.c:530-531 — only the superuser may create non-FIFO nodes.
            if eff_uid != crate::link::SU_UID
                && mode_bits & crate::open::S_IFMT != crate::open::S_IFIFO
            {
                return SyscallResult::Error(minix_types::EPERM);
            }
            // C open.c:541 — `(mode & S_IFMT) | (mode & ACCESSPERMS & fp_umask)`;
            // the Rust side stores umask as the raw mask, hence `& !umask` (see `fproc::FProc`).
            let bits = (mode_bits & crate::open::S_IFMT) | (mode_bits & 0o777 & !umask);
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            let fetcher = crate::path::SysPathFetcher { who: user_e };
            let path = match fetcher.fetch(name_addr, name_len as usize) {
                Ok(p) => p,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let split = match crate::path::last_dir_split(&path) {
                Ok(sp) => sp,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            // Symlinks in the directory prefix are followed (`path.c:231-235` clears RET_SYMLINK).
            let resolve = match crate::path::Lookup::new(
                split.dir_path.clone(),
                crate::path::LookupFlags::NOFLAGS,
            ) {
                Ok(l) => l,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let rd = state.root_dir_of(Some(fp_slot));
            let start = if resolve.path.starts_with('/') {
                crate::path::LookupStart { fs: rd.fs, ino: rd.ino, dev: rd.dev }
            } else {
                let wd = state.work_dir_of(fp_slot);
                crate::path::LookupStart { fs: wd.fs, ino: wd.ino, dev: wd.dev }
            };
            let gid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_gid).unwrap_or(0);
            let (walk, step) = match crate::path::LookupWalk::begin(start, resolve, rd, eff_uid, gid)
            {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let crate::path::WalkStep::Send { fs_e, dir_ino, root_ino } = step else {
                return SyscallResult::Error(minix_types::EIO);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Path);
                wp.path = Some(crate::worker::PathPending {
                    walk,
                    grant: 0, // overwritten by send_lookup_for_slot
                    follow: crate::worker::PathFollow::Mknod {
                        entry: split.entry,
                        mode_bits: bits,
                        dev,
                    },
                });
            }
            if state
                .send_lookup_for_slot(worker, Some(fp_slot), fs_e, dir_ino, root_ino)
                .is_err()
            {
                if let Some(wp) = state.worker_pool.get_mut(worker) {
                    wp.cont = None;
                    wp.path = None;
                }
                return SyscallResult::Error(minix_types::EIO);
            }
            SyscallResult::Suspend
        }

        VfsCallNum::Chown | VfsCallNum::Fchown => {
            // Payload `mess_lc_vfs_chown` (ipc.h:611-619): name@0, len@8, fd@16,
            // owner@20, group@24 — the path half uses name/len, the fd half uses fd.
            let (name_len, name_addr, fd, uid, gid) = {
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[0..8]);
                let name_addr = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[8..16]);
                let name_len = u64::from_le_bytes(b8);
                let fd = i32::from_le_bytes([raw[16], raw[17], raw[18], raw[19]]);
                let uid = u32::from_le_bytes([raw[20], raw[21], raw[22], raw[23]]);
                let gid = u32::from_le_bytes([raw[24], raw[25], raw[26], raw[27]]);
                (name_len, name_addr, fd, uid, gid)
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            if matches!(call, VfsCallNum::Fchown) {
                // The fd half of C `do_chown` (protect.c:53-63): take the vnode
                // from the filp; the rest shares the same body as the path half.
                if fd < 0 {
                    return SyscallResult::Error(minix_types::EBADF);
                }
                let (filp_idx, vnode_idx) = {
                    let fp = match state.fproc_table.get(fp_slot) {
                        Some(fp) => fp,
                        None => return SyscallResult::Error(minix_types::EINVAL),
                    };
                    let filp_idx = match fp.filps.get(fd as usize).copied().flatten() {
                        Some(idx) => idx,
                        None => return SyscallResult::Error(minix_types::EBADF),
                    };
                    let filp = match state.filp_table.get(crate::filp::FilpId(filp_idx)) {
                        Some(f) => f,
                        None => return SyscallResult::Error(minix_types::EBADF),
                    };
                    match filp.vnode {
                        Some(v) => (filp_idx, v),
                        None => return SyscallResult::Error(minix_types::EBADF),
                    }
                };
                let _ = filp_idx;
                let (fs_e, ino, node_uid, node_gid) =
                    match state.vnode_table.get(crate::vnode::VnodeId(vnode_idx)) {
                        Some(v) => (v.fs, v.ino, v.uid, v.gid),
                        None => return SyscallResult::Error(minix_types::EBADF),
                    };
                state.finish_chown(
                    worker,
                    Some(fp_slot),
                    fs_e,
                    ino,
                    node_uid,
                    node_gid,
                    uid,
                    gid,
                    vnode_idx,
                );
                // The shared body decides whether it "suspended" or "finished":
                // neither case should release the slot here (on finish
                // `finish_worker_job` has already released it and cleared
                // `current_worker`), so this always reports suspension.
                return SyscallResult::Suspend;
            }
            // The path half of C `do_chown`: `fetch_name` fetches the path
            // (cross-space, unavailable on the host) → `eat_path` (NOFLAGS) → the shared body.
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            let fetcher = crate::path::SysPathFetcher { who: user_e };
            let path = match fetcher.fetch(name_addr, name_len as usize) {
                Ok(p) => p,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let resolve = match crate::path::Lookup::new(
                path,
                crate::path::LookupFlags::NOFLAGS,
            ) {
                Ok(l) => l,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let rd = state.root_dir_of(Some(fp_slot));
            let start = if resolve.path.starts_with('/') {
                crate::path::LookupStart { fs: rd.fs, ino: rd.ino, dev: rd.dev }
            } else {
                let wd = state.work_dir_of(fp_slot);
                crate::path::LookupStart { fs: wd.fs, ino: wd.ino, dev: wd.dev }
            };
            let wuid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_uid).unwrap_or(0);
            let wgid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_gid).unwrap_or(0);
            let (walk, step) = match crate::path::LookupWalk::begin(start, resolve, rd, wuid, wgid) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let crate::path::WalkStep::Send { fs_e, dir_ino, root_ino } = step else {
                return SyscallResult::Error(minix_types::EIO);
            };
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Path);
                wp.path = Some(crate::worker::PathPending {
                    walk,
                    grant: 0, // overwritten by send_lookup_for_slot
                    follow: crate::worker::PathFollow::Chown { uid, gid },
                });
            }
            if state
                .send_lookup_for_slot(worker, Some(fp_slot), fs_e, dir_ino, root_ino)
                .is_err()
            {
                if let Some(wp) = state.worker_pool.get_mut(worker) {
                    wp.cont = None;
                    wp.path = None;
                }
                return SyscallResult::Error(minix_types::EIO);
            }
            SyscallResult::Suspend
        }

        VfsCallNum::Unlink | VfsCallNum::Rmdir => {
            // C `do_unlink` (link.c:94-163) serves both unlink and rmdir (table.c:25
            // and :36 both point at it); the only difference is the final request
            // number (link.c:156-159).
            let rmdir = matches!(call, VfsCallNum::Rmdir);
            let (name_addr, name_len, inline) = {
                // Payload has the same shape as access/chmod (`mess_lc_vfs_path`:
                // name@0, len@8, flags@16, mode@20, buf@24 inline path) — unlink does not use mode.
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[0..8]);
                let name = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[8..16]);
                (name, u64::from_le_bytes(b8), raw[24..].to_vec())
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            // Fetch the path: both branches of C `copy_path` (utility.c:17-43,
            // which do_unlink uses — link.c's function-level mapping); overly long
            // paths are fetched through the caller's address space.
            let path = match crate::path::copy_path(&inline, name_addr, name_len as usize, user_e) {
                Ok(p) => p,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            // C `last_dir` (path.c:146-380): split into directory prefix and last
            // component and **walk only the directory prefix** (symlinks in the
            // prefix are followed — `path.c:231-235` clearing RET_SYMLINK means
            // exactly that, hence NOFLAGS here).
            let split = match crate::path::last_dir_split(&path) {
                Ok(sp) => sp,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let resolve = match crate::path::Lookup::new(
                split.dir_path.clone(),
                crate::path::LookupFlags::NOFLAGS,
            ) {
                Ok(l) => l,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let rd = state.root_dir_of(Some(fp_slot));
            let start = if resolve.path.starts_with('/') {
                crate::path::LookupStart { fs: rd.fs, ino: rd.ino, dev: rd.dev }
            } else {
                let wd = state.work_dir_of(fp_slot);
                crate::path::LookupStart { fs: wd.fs, ino: wd.ino, dev: wd.dev }
            };
            let uid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_uid).unwrap_or(0);
            let gid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_gid).unwrap_or(0);
            let (walk, step) = match crate::path::LookupWalk::begin(start, resolve, rd, uid, gid) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let crate::path::WalkStep::Send { fs_e, dir_ino, root_ino } = step else {
                return SyscallResult::Error(minix_types::EIO);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Path);
                wp.path = Some(crate::worker::PathPending {
                    walk,
                    grant: 0, // overwritten by send_lookup_for_slot
                    follow: crate::worker::PathFollow::Unlink {
                        entry: split.entry,
                        rmdir,
                    },
                });
            }
            if state
                .send_lookup_for_slot(worker, Some(fp_slot), fs_e, dir_ino, root_ino)
                .is_err()
            {
                if let Some(wp) = state.worker_pool.get_mut(worker) {
                    wp.cont = None;
                    wp.path = None;
                }
                return SyscallResult::Error(minix_types::EIO);
            }
            SyscallResult::Suspend
        }

        VfsCallNum::Chmod => {
            // The path half of C `do_chmod` (protect.c:62-133): `m_lc_vfs_path.mode`
            // → walk the path (VNODE_WRITE lock, not modeled here) → owner/superuser
            // gate + read-only mount gate + clear setgid → `REQ_CHMOD`.
            let (name_addr, name_len, mode, inline) = {
                // Payload has the same shape as access (`mess_lc_vfs_path`: name@0,
                // len@8, flags@16, mode@20, buf@24 inline path).
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[0..8]);
                let name = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[8..16]);
                let len = u64::from_le_bytes(b8);
                let mode = u32::from_le_bytes([raw[20], raw[21], raw[22], raw[23]]);
                (name, len, mode, raw[24..].to_vec())
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            // Fetch the path: both branches of C `copy_path` (utility.c:17-43,
            // which do_chmod uses — stadir.c's function-level mapping); overly long
            // paths are fetched through the caller's address space.
            let path = match crate::path::copy_path(&inline, name_addr, name_len as usize, user_e) {
                Ok(p) => p,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            // C `lookup_init(..., PATH_NOFLAGS, ...)` (protect.c:76).
            let resolve = match crate::path::Lookup::new(path, crate::path::LookupFlags::NOFLAGS)
            {
                Ok(l) => l,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let rd = state.root_dir_of(Some(fp_slot));
            let start = if resolve.path.starts_with('/') {
                crate::path::LookupStart { fs: rd.fs, ino: rd.ino, dev: rd.dev }
            } else {
                let wd = state.work_dir_of(fp_slot);
                crate::path::LookupStart { fs: wd.fs, ino: wd.ino, dev: wd.dev }
            };
            let uid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_uid).unwrap_or(0);
            let gid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_gid).unwrap_or(0);
            let (walk, step) = match crate::path::LookupWalk::begin(start, resolve, rd, uid, gid) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let crate::path::WalkStep::Send { fs_e, dir_ino, root_ino } = step else {
                return SyscallResult::Error(minix_types::EIO);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Path);
                wp.path = Some(crate::worker::PathPending {
                    walk,
                    grant: 0, // overwritten by send_lookup_for_slot
                    follow: crate::worker::PathFollow::Chmod { user: user_e, mode },
                });
            }
            if state
                .send_lookup_for_slot(worker, Some(fp_slot), fs_e, dir_ino, root_ino)
                .is_err()
            {
                if let Some(wp) = state.worker_pool.get_mut(worker) {
                    wp.cont = None;
                    wp.path = None;
                }
                return SyscallResult::Error(minix_types::EIO);
            }
            SyscallResult::Suspend
        }

        VfsCallNum::Readlink => {
            // C `do_rdlink` (link.c:473-507): payload
            // `mess_lc_vfs_readlink { name, namelen, buf, bufsize }` →
            // `bufsize > SSIZE_MAX` is EINVAL → `fetch_name` fetches the path
            // (**always fetch** — the wire has no inline field, link.c:494) →
            // walk with `PATH_RET_SYMLINK` → not a symlink is EINVAL →
            // `REQ_RDLINK` (the grant writes into the user buffer).
            let (name_addr, name_len, buf, buf_size) = {
                // User payload: name@0, namelen@8, buf@16, bufsize@24 (ipc.h:785-792;
                // under LP64 the rest of the payload area is padding, with no inline
                // window from @32).
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[0..8]);
                let name = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[8..16]);
                let namelen = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[16..24]);
                let buf = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[24..32]);
                let buf_size = u64::from_le_bytes(b8);
                (name, namelen, buf, buf_size)
            };
            // C link.c:486 — a window larger than SSIZE_MAX is EINVAL.
            if buf_size > i64::MAX as u64 {
                return SyscallResult::Error(minix_types::EINVAL);
            }
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            // Fetch the path: C `do_rdlink:494` always calls `fetch_name` (under
            // the host build a failed fetch is EINVAL; the PATH_MAX limit and
            // NUL-termination check live inside fetch).
            let path =
                match (crate::path::SysPathFetcher { who: user_e }).fetch(name_addr, name_len as usize)
            {
                Ok(p) => p,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            // C link.c:489 — `PATH_RET_SYMLINK`: if the last component is a
            // symbolic link, the link itself is returned (the bit's semantics are on
            // the FS side, see `encode_lookup`).
            let resolve = match crate::path::Lookup::new(
                path,
                crate::path::LookupFlags::RET_SYMLINK,
            ) {
                Ok(l) => l,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let rd = state.root_dir_of(Some(fp_slot));
            let start = if resolve.path.starts_with('/') {
                crate::path::LookupStart { fs: rd.fs, ino: rd.ino, dev: rd.dev }
            } else {
                let wd = state.work_dir_of(fp_slot);
                crate::path::LookupStart { fs: wd.fs, ino: wd.ino, dev: wd.dev }
            };
            let uid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_uid).unwrap_or(0);
            let gid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_gid).unwrap_or(0);
            let (walk, step) = match crate::path::LookupWalk::begin(start, resolve, rd, uid, gid) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let crate::path::WalkStep::Send { fs_e, dir_ino, root_ino } = step else {
                return SyscallResult::Error(minix_types::EIO);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Path);
                wp.path = Some(crate::worker::PathPending {
                    walk,
                    grant: 0, // overwritten by send_lookup_for_slot
                    follow: crate::worker::PathFollow::Rdlink {
                        user: user_e,
                        buf,
                        buf_size,
                    },
                });
            }
            if state
                .send_lookup_for_slot(worker, Some(fp_slot), fs_e, dir_ino, root_ino)
                .is_err()
            {
                if let Some(wp) = state.worker_pool.get_mut(worker) {
                    wp.cont = None;
                    wp.path = None;
                }
                return SyscallResult::Error(minix_types::EIO);
            }
            SyscallResult::Suspend
        }

        VfsCallNum::Access => {
            // C `do_access` (protect.c:199-233): validate mode first (only
            // combinations of `R_OK|W_OK|X_OK` or `F_OK`) → `copy_path` → `eat_path`
            // → `forbidden(fp, vp, access)` (**real** uid/gid).
            let (name_addr, name_len, mode, inline) = {
                // User payload has the same shape as open (`mess_lc_vfs_path`:
                // name@0, len@8, flags@16, mode@20, buf@24 inline path) — access uses `mode`.
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[0..8]);
                let name = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[8..16]);
                let len = u64::from_le_bytes(b8);
                let mode = u32::from_le_bytes([raw[20], raw[21], raw[22], raw[23]]);
                (name, len, mode, raw[24..].to_vec())
            };
            // C protect.c:216-217 — bit-mask validation first: apart from `F_OK`(0)
            // only `R_OK|W_OK|X_OK` (4|2|1) is accepted; any other bit is EINVAL,
            // with no permission check.
            if mode & !0o7 != 0 && mode != 0 {
                return SyscallResult::Error(minix_types::EINVAL);
            }
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            // Fetch the path: both branches of C `copy_path` (utility.c:17-43,
            // which do_access uses — protect.c:221); overly long paths are fetched
            // through the caller's address space.
            let path = match crate::path::copy_path(&inline, name_addr, name_len as usize, user_e) {
                Ok(p) => p,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let resolve = match crate::path::Lookup::new(path, crate::path::LookupFlags::NOFLAGS)
            {
                Ok(l) => l,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let rd = state.root_dir_of(Some(fp_slot));
            let start = if resolve.path.starts_with('/') {
                crate::path::LookupStart { fs: rd.fs, ino: rd.ino, dev: rd.dev }
            } else {
                let wd = state.work_dir_of(fp_slot);
                crate::path::LookupStart { fs: wd.fs, ino: wd.ino, dev: wd.dev }
            };
            // The uid/gid used during the walk only serve `advance`'s search
            // permission check (C `advance` uses the effective ids internally);
            // access's own check is redone after the walk with the real ids
            // (protect.c:255-256).
            let uid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_uid).unwrap_or(0);
            let gid = state.fproc_table.get(fp_slot).map(|fp| fp.eff_gid).unwrap_or(0);
            let (walk, step) = match crate::path::LookupWalk::begin(start, resolve, rd, uid, gid) {
                Ok(pair) => pair,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let crate::path::WalkStep::Send { fs_e, dir_ino, root_ino } = step else {
                return SyscallResult::Error(minix_types::EIO);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Path);
                wp.path = Some(crate::worker::PathPending {
                    walk,
                    grant: 0, // overwritten by send_lookup_for_slot
                    follow: crate::worker::PathFollow::Access { user: user_e, access: mode },
                });
            }
            if state
                .send_lookup_for_slot(worker, Some(fp_slot), fs_e, dir_ino, root_ino)
                .is_err()
            {
                if let Some(wp) = state.worker_pool.get_mut(worker) {
                    wp.cont = None;
                    wp.path = None;
                }
                return SyscallResult::Error(minix_types::EIO);
            }
            SyscallResult::Suspend
        }

        VfsCallNum::Getdents => {
            // C `do_getdents` (read.c:282-317): payload has the same shape as
            // read/write (`mess_lc_vfs_readwrite`, but the `cum_io` slot **must be 0**)
            // → fd gate (R_BIT + must be a directory) → `req_getdents` (the grant is
            // a magic grant letting the FS write directory entries into the user
            // buffer) → the reply carries the next position and the actual byte count,
            // **the position advances only when `nbytes > 0`**.
            let (fd, buf, len, cum_io) = {
                // User payload: fd@0, buf@8, len@16, cum_io@24
                // (`mess_lc_vfs_readwrite` at ipc.h:795-802).
                // SAFETY: this call number's payload is laid out in the above field order.
                let raw = unsafe { &msg.m_u.raw };
                let fd = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[8..16]);
                let buf = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[16..24]);
                let len = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[24..32]);
                (fd, buf, len, u64::from_le_bytes(b8))
            };
            // C read.c:290-292 — `cum_io` is an internal reserved slot; non-zero is
            // EINVAL (user space should not fill it; doing so means the caller used the wrong entry).
            if cum_io != 0 {
                return SyscallResult::Error(minix_types::EINVAL);
            }
            if fd < 0 {
                return SyscallResult::Error(minix_types::EBADF);
            }
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let (filp_idx, fs_e, ino, mode, orig_pos) = {
                let fp = match state.fproc_table.get(fp_slot) {
                    Some(fp) => fp,
                    None => return SyscallResult::Error(minix_types::EINVAL),
                };
                let filp_idx = match fp.filps.get(fd as usize).copied().flatten() {
                    Some(idx) => idx,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                let filp = match state.filp_table.get(crate::filp::FilpId(filp_idx)) {
                    Some(f) => f,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                let vnode_idx = match filp.vnode {
                    Some(idx) => idx,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                let vnode = match state.vnode_table.get(crate::vnode::VnodeId(vnode_idx)) {
                    Some(v) => v,
                    None => return SyscallResult::Error(minix_types::EBADF),
                };
                (filp_idx, vnode.fs, vnode.ino, vnode.mode, filp.pos)
            };
            // C read.c:294-297 — two EBADF gates: the filp's open mode must have
            // the read bit, and the node must be a directory (`getdents` is not the
            // generic "read file" entry).
            let filp_mode = state
                .filp_table
                .get(crate::filp::FilpId(filp_idx))
                .map(|f| f.mode)
                .unwrap_or(0);
            if filp_mode & crate::open::R_BIT == 0 {
                return SyscallResult::Error(minix_types::EBADF);
            }
            if mode & crate::open::S_IFMT != crate::open::S_IFDIR {
                return SyscallResult::Error(minix_types::EBADF);
            }
            let vmnt_id = match state.vmnt_table.find_by_fs(fs_e) {
                Some(v) => v,
                None => return SyscallResult::Error(minix_types::EIO),
            };
            // C request.c:316-319 — if the FS did not declare 64-bit capability
            // and the position exceeds INT_MAX, EINVAL. **The order intentionally
            // differs from C**: C leaks the grant it just built on this early exit
            // (`cpf_revoke` is only called on the normal path), whereas this arm
            // hoists the capability gate before building the grant — the normal
            // path returns the same value as C, and the early exit leaves no
            // dangling grant.
            let fs_flags = state
                .vmnt_table
                .get(vmnt_id)
                .map(|v| v.fs_flags)
                .unwrap_or(0);
            if fs_flags & crate::request::FsFlags::IS64BIT.bits() == 0 && orig_pos > i32::MAX as i64 {
                return SyscallResult::Error(minix_types::EINVAL);
            }
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            // magic grant: the FS writes directory entries into the user buffer.
            // C's `req_getdents` wrapper (request.c:341-357) carries `CPF_TRY` on
            // the first trip (it does not panic when a user page is not resident;
            // it returns ERESTART, then `vm_vfs_procctl_handlemem` pages in and
            // retries) — the page-in retry is still to be wired here, as in the
            // read/write arms; for now this follows C's first-trip shape with `CPF_TRY`.
            let grant = match state.grant_user_buffer(
                fs_e,
                user_e,
                buf,
                len,
                minix_types::CpFlags::WRITE | minix_types::CpFlags::TRY,
            ) {
                Ok(g) => g,
                Err(_) => return SyscallResult::Error(minix_types::EIO),
            };
            let req = crate::request::encode_getdents(ino, orig_pos, grant, len as usize);
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Getdents {
                    grant,
                    filp: filp_idx,
                });
            }
            state.pending_fs = Some(crate::main_loop::PendingFs {
                vmnt: vmnt_id.0,
                fs_e,
                worker,
                grant,
                user: user_e,
                req,
            });
            SyscallResult::Suspend
        }

        VfsCallNum::Getsysinfo => {
            // C `do_getsysinfo` (misc.c:59-113) in three parts: root gate → `what`
            // classification → exact-size gate + `sys_datacopy(SELF → caller)`.
            // The root criterion is C's `super_user` macro (glo.h:33) — the current
            // fproc's `fp_effuid == SU_UID (0)`.
            let what_raw = unsafe { msg.m_u.m_lsys_getsysinfo.what };
            let where_ = unsafe { msg.m_u.m_lsys_getsysinfo.where_ };
            let size = unsafe { msg.m_u.m_lsys_getsysinfo.size };
            let is_root = state
                .current_fp_slot
                .and_then(|slot| state.fproc_table.get(slot))
                .map(|fp| fp.eff_uid == 0)
                .unwrap_or(false);
            let what = match crate::misc::SysinfoWhat::from_raw(what_raw as u32) {
                Some(w) => w,
                // C `default: return(EINVAL)` (misc.c:104-105).
                None => return SyscallResult::Error(minix_types::EINVAL),
            };
            let target = msg.m_source;
            let mut cpy = crate::misc::SysCopyToUser { target };
            match crate::misc::do_getsysinfo(
                &state.fproc_table,
                &state.dmap_table,
                &state.smap_table,
                is_root,
                what,
                size,
                minix_types::VirBytes(where_),
                &mut cpy,
            ) {
                Ok(()) => SyscallResult::Ok(0),
                Err(e) => SyscallResult::Error(e.to_errno()),
            }
        }
    }
}

/// The filp's current read position (`filp_pos`, `file.h:7`).
fn f_pos_of(f: &crate::filp::Filp) -> i64 {
    f.pos
}

// `OPEN_MAX` is defined by `fproc.rs`; the fd bounds of table-local arms are checked through `Fd::new`.
#[allow(unused_imports)]
/// The shared body of `do_sockmsg` (C `do_sockmsg` socket.c:538-595): fetch the
/// user `struct msghdr` (through the `MsgHdrFetcher` seam) → iov gate (`> 1` is
/// `EMSGSIZE`) → length gate on the single `iovec` (`> SSIZE_MAX` → `EINVAL`)
/// → the same `send_sdev_readwrite` as sendto/recvfrom (`recvmsg` must also
/// carry `msg_buf` down — the finish writes several msghdr fields back).
///
/// Factored out as a free function to make the **fetcher injectable**: under the
/// host build the production fetcher cannot reach an msghdr, but the iov gate and
/// request shape "after fetching" can be tested with a scripted stand-in.
#[allow(clippy::too_many_arguments)]
fn do_sockmsg(
    state: &mut VfsState,
    worker: usize,
    fp_slot: Option<minix_types::UserSlot>,
    dev: u64,
    filp_flags: i32,
    msgbuf: u64,
    flags: i32,
    recv: bool,
    fetcher: &impl crate::socket::MsgHdrFetcher,
) -> Result<(), i32> {
    let mh = fetcher.fetch_msghdr(msgbuf).map_err(|e| e.to_errno())?;
    let (data_buf, data_len) = match crate::socket::iov_gate(mh.iovlen) {
        Ok(crate::socket::IovPlan::Empty) => (0, 0),
        Ok(crate::socket::IovPlan::One) => {
            let iov = fetcher.fetch_iov(mh.iov).map_err(|e| e.to_errno())?;
            crate::socket::check_iov_len(iov.len).map_err(|e| e.to_errno())?;
            if iov.len > 0 {
                (iov.base, iov.len)
            } else {
                (0, 0)
            }
        }
        Err(e) => return Err(e.to_errno()),
    };
    let extra = minix_sockdriver::sdev::sock_msg_flags(
        filp_flags & (crate::fcntl::O_NONBLOCK as i32) != 0,
        !recv && filp_flags & (crate::fcntl::O_NOSIGPIPE as i32) != 0,
    ) as i32;
    let call_kind = if recv {
        crate::fproc::SdevCall::Recvmsg
    } else {
        crate::fproc::SdevCall::Sendmsg
    };
    // `recvmsg`'s finish writes the msghdr back, so `msg_buf` is carried down in
    // the site (the last argument of C `sdev_readwrite(..., user_buf)`).
    let aux = if recv {
        crate::fproc::SdevAux::Buf(minix_types::VirBytes(msgbuf))
    } else {
        crate::fproc::SdevAux::None
    };
    let result = state.send_sdev_readwrite_aux(
        worker,
        fp_slot,
        dev,
        if data_buf != 0 { Some((data_buf, data_len)) } else { None },
        if mh.control != 0 {
            Some((mh.control, mh.controllen as u64))
        } else {
            None
        },
        if mh.name != 0 {
            Some((mh.name, mh.namelen as u64))
        } else {
            None
        },
        flags | extra,
        !recv,
        call_kind,
        aux,
    );
    result
}

/// The **local half** of `do_select` (C `do_select` select.c:94-260): `nfds`
/// gate → take a select slot (`ENOSPC` when full) → copy the three fd sets →
/// validate each fd (skip empty `tab2ops` slots, `EBADF` on `get_filp` failure,
/// `EBADF` on an unknown type) → readiness decision (open-mode mismatches and
/// regular files count as ready immediately) → return immediately
/// (`should_return`).
///
/// **The half not yet wired**: the character/socket drivers' `select_request`
/// and pipe probing, plus the "no one ready, so suspend and wait for the driver
/// or the timeout" path — those need driver conversations (the pipeline is in
/// place) and timers, and are the next step. Until then they honestly return
/// `ENOSYS` and give the slot back.
///
/// Factored out as a free function so the store can be injected (under the host
/// build the production fetcher cannot reach a user fd_set).
#[allow(clippy::too_many_arguments)]
fn do_select(
    state: &mut VfsState,
    fp_slot: minix_types::UserSlot,
    nfds: usize,
    readfds: u64,
    writefds: u64,
    errorfds: u64,
    timeout: u64,
    io: &impl crate::select::FdSetIo,
) -> crate::call_table::SyscallResult {
    use crate::call_table::SyscallResult;
    // C select.c:110-111 — `nfds < 0 || nfds > OPEN_MAX` is EINVAL.
    if nfds > crate::fproc::OPEN_MAX {
        return SyscallResult::Error(minix_types::EINVAL);
    }
    // C select.c:118-124 — find an empty slot; full means ENOSPC.
    let Some(slot_idx) = state.select_table.alloc() else {
        return SyscallResult::Error(minix_types::ENOSPC);
    };
    let bytes = match crate::select::fdset_bytes(nfds) {
        Some(b) => b,
        None => return SyscallResult::Error(minix_types::EINVAL),
    };
    // C `copy_fdsets(se, nfds, FROM_PROC)`: the three sets are copied in
    // separately (sets with a null pointer are skipped — `select` may care about
    // only some of them).
    let mut read_set = alloc::vec::Vec::new();
    let mut write_set = alloc::vec::Vec::new();
    let mut error_set = alloc::vec::Vec::new();
    if readfds != 0 {
        read_set = match io.fetch(readfds, bytes) {
            Ok(b) => b,
            Err(e) => return SyscallResult::Error(e),
        };
    }
    if writefds != 0 {
        write_set = match io.fetch(writefds, bytes) {
            Ok(b) => b,
            Err(e) => return SyscallResult::Error(e),
        };
    }
    if errorfds != 0 {
        error_set = match io.fetch(errorfds, bytes) {
            Ok(b) => b,
            Err(e) => return SyscallResult::Error(e),
        };
    }
    // Timeout: C `plan_timeout` (no timeval = wait forever; (0,0) = poll).
    let plan = if timeout == 0 {
        crate::select::TimeoutPlan::Forever
    } else {
        // The user's `struct timeval { time_t tv_sec; suseconds_t tv_usec; }` (two
        // 8-byte fields under LP64) — reading it is also cross-space (not covered
        // by the same seam; temporarily a direct read that is unreachable on the
        // host, failure is EINVAL).
        let mut raw = [0u8; 16];
        match minix_sys::syscall::sys_datacopy(
            &minix_sys::syscall::DirectKernelCallTransport,
            io_who(state, fp_slot),
            timeout,
            minix_types::Endpoint::SELF.0,
            raw.as_mut_ptr() as u64,
            16,
        ) {
            Ok(()) => {}
            Err(e) => return SyscallResult::Error(-e),
        }
        let sec = i64::from_le_bytes(raw[0..8].try_into().unwrap());
        let usec = i64::from_le_bytes(raw[8..16].try_into().unwrap());
        match crate::select::plan_timeout(true, sec, usec, 60) {
            Ok(p) => p,
            Err(e) => return SyscallResult::Error(e.to_errno()),
        }
    };
    let block = crate::select::block_of(plan);
    {
        let Some(se) = state.select_table.get_mut(slot_idx) else {
            return SyscallResult::Error(minix_types::EIO);
        };
        se.requestor = Some(fp_slot);
        se.block = block;
        se.nfds = nfds;
        se.vir_readfds = readfds;
        se.vir_writefds = writefds;
        se.vir_errorfds = errorfds;
        se.readfds = read_set;
        se.writefds = write_set;
        se.errorfds = error_set;
        se.nready = 0;
        se.error = 0;
    }
    // ── Initialization phase (C `se->starting = TRUE`, select.c:180-314): the fd
    // loop must not return directly — errors are recorded in `se->error` and the
    // loop stops, with a unified cancel at the end (earlier fds may already be
    // attached to `filp_selectors` and must be undone).
    {
        let Some(se) = state.select_table.get_mut(slot_idx) else {
            return SyscallResult::Error(minix_types::EIO);
        };
        se.starting = true;
        se.filps = alloc::vec![None; nfds];
        se.ready_readfds = alloc::vec![0u8; bytes];
        se.ready_writefds = alloc::vec![0u8; bytes];
        se.ready_errorfds = alloc::vec![0u8; bytes];
    }
    for fd in 0..nfds {
        let (rd, wr, er) = {
            let Some(se) = state.select_table.get(slot_idx) else {
                return SyscallResult::Error(minix_types::EIO);
            };
            (
                crate::select::bit_of(&se.readfds, fd),
                crate::select::bit_of(&se.writefds, fd),
                crate::select::bit_of(&se.errorfds, fd),
            )
        };
        let ops = crate::select::tab2ops(rd, wr, er);
        if ops.is_empty() {
            continue; // C: this bit is not set, skip
        }
        // C `get_filp(fd, VNODE_READ)` (select.c:203-212): no entry in the fd
        // table = EBADF (record the error and stop the loop); a filp that is
        // FILP_CLOSED (mode == 0, the serving process crashed) takes the f==NULL
        // path: don't check the type, just mark read and write ready (subsequent
        // I/O will certainly fail).
        let filp_idx = {
            let Some(fp) = state.fproc_table.get(fp_slot) else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            fp.filps.get(fd).copied().flatten()
        };
        let Some(filp_idx) = filp_idx else {
            let se = state.select_table.get_mut(slot_idx).unwrap();
            se.error = minix_types::EBADF;
            break;
        };
        let mode = match state.filp_table.get(crate::filp::FilpId(filp_idx)) {
            Some(f) => f.mode,
            None => {
                let se = state.select_table.get_mut(slot_idx).unwrap();
                se.error = minix_types::EBADF;
                break;
            }
        };
        let closed = mode == 0;
        if closed {
            let se = state.select_table.get_mut(slot_idx).unwrap();
            let (a, b, c, mut n) = (
                &mut se.ready_readfds,
                &mut se.ready_writefds,
                &mut se.ready_errorfds,
                se.nready,
            );
            crate::select::ops2tab_store(
                crate::select::SelOps::RD | crate::select::SelOps::WR,
                fd,
                (rd, wr, er),
                (a, b, c),
                &mut n,
            );
            se.nready = n;
            continue;
        }
        let kind = {
            let vnode_idx = state
                .filp_table
                .get(crate::filp::FilpId(filp_idx))
                .and_then(|f| f.vnode);
            let ft = vnode_idx
                .and_then(|vi| state.vnode_table.get(crate::vnode::VnodeId(vi)))
                .map(|v| crate::open::FileType::from(v.mode));
            let ft = match ft {
                Some(t) => t,
                None => {
                    let se = state.select_table.get_mut(slot_idx).unwrap();
                    se.error = minix_types::EBADF;
                    break;
                }
            };
            crate::select::classify(
                ft == crate::open::FileType::Char,
                ft == crate::open::FileType::Socket,
                ft == crate::open::FileType::Regular,
                ft == crate::open::FileType::Fifo,
            )
        };
        let Some(kind) = kind else {
            // C select.c:233-235: type mismatch → EBADF.
            let se = state.select_table.get_mut(slot_idx).unwrap();
            se.error = minix_types::EBADF;
            break;
        };
        // Associate fd → filp + bump the selector count (C `filp_selectors++`).
        {
            let se = state.select_table.get_mut(slot_idx).unwrap();
            se.filps[fd] = Some(crate::select::FdEntry { filp: filp_idx, kind });
            se.nfds = fd + 1;
        }
        if let Some(f) = state.filp_table.get_mut(crate::filp::FilpId(filp_idx)) {
            f.selectors = f.selectors.saturating_add(1);
        }
        // An open mode that does not match the requested direction counts as **ready immediately** (select.c:243-249).
        let mut want = ops;
        if rd && mode & crate::open::R_BIT == 0 {
            let se = state.select_table.get_mut(slot_idx).unwrap();
            let (a, b, c, mut n) = (
                &mut se.ready_readfds,
                &mut se.ready_writefds,
                &mut se.ready_errorfds,
                se.nready,
            );
            crate::select::ops2tab_store(crate::select::SelOps::RD, fd, (rd, wr, er), (a, b, c), &mut n);
            se.nready = n;
            want -= crate::select::SelOps::RD;
        }
        if wr && mode & crate::open::W_BIT == 0 {
            let se = state.select_table.get_mut(slot_idx).unwrap();
            let (a, b, c, mut n) = (
                &mut se.ready_readfds,
                &mut se.ready_writefds,
                &mut se.ready_errorfds,
                se.nready,
            );
            crate::select::ops2tab_store(crate::select::SelOps::WR, fd, (rd, wr, er), (a, b, c), &mut n);
            se.nready = n;
            want -= crate::select::SelOps::WR;
        }
        if want.is_empty_real() {
            continue;
        }
        // C select.c:256-260: bits another process already reported need not be asked about again.
        let held = state
            .filp_table
            .get(crate::filp::FilpId(filp_idx))
            .map(|f| crate::select::SelOps::from_bits_truncate(f.select_ops))
            .unwrap_or(crate::select::SelOps::empty());
        let want = want & !held;
        if want.is_empty_real() {
            continue;
        }
        match kind {
            crate::select::FdKind::File => {
                // `select_request_file`: regular files are always ready (`*ops` unchanged).
                let se = state.select_table.get_mut(slot_idx).unwrap();
                let (a, b, c, mut n) = (
                    &mut se.ready_readfds,
                    &mut se.ready_writefds,
                    &mut se.ready_errorfds,
                    se.nready,
                );
                crate::select::ops2tab_store(want, fd, (rd, wr, er), (a, b, c), &mut n);
                se.nready = n;
            }
            crate::select::FdKind::Pipe => {
                // `select_request_pipe` (select.c:577-616): probe one byte.
                // Production probe: the VFS-side pipe state = the vnode's `v_size`
                // (PFS maintains it through size updates; capacity = PIPE_BUF, the
                // write-space decision at pipe.c:243-266). **Approximation**: the
                // reader/writer liveness surface is not in VFS state, so the EPIPE
                // case is not decided (the following read/write reports it itself)
                // — one SEL_ERR source fewer than C's `pipe_check`, conservative
                // in direction.
                let (size, capacity) = {
                    let vnode_idx = state
                        .filp_table
                        .get(crate::filp::FilpId(filp_idx))
                        .and_then(|f| f.vnode);
                    let size = vnode_idx
                        .and_then(|vi| state.vnode_table.get(crate::vnode::VnodeId(vi)))
                        .map(|v| v.size)
                        .unwrap_or(0);
                    (size, crate::pipe::PIPE_BUF)
                };
                let probe = VfsPipeProbe { size, capacity };
                let out = crate::select::pipe_request(
                    &probe,
                    want,
                    state
                        .select_table
                        .get(slot_idx)
                        .map(|se| se.block)
                        .unwrap_or(false),
                );
                if !out.ready.is_empty_real() {
                    let se = state.select_table.get_mut(slot_idx).unwrap();
                    let (a, b, c, mut n) = (
                        &mut se.ready_readfds,
                        &mut se.ready_writefds,
                        &mut se.ready_errorfds,
                        se.nready,
                    );
                    crate::select::ops2tab_store(out.ready, fd, (rd, wr, er), (a, b, c), &mut n);
                    se.nready = n;
                }
                if !out.park.is_empty_real()
                    && let Some(f) = state.filp_table.get_mut(crate::filp::FilpId(filp_idx))
                {
                    f.pipe_select_ops |= out.park.bits();
                }
            }
            crate::select::FdKind::Char | crate::select::FdKind::Sock => {
                // `select_request_char`/`select_request_sock` (the shared body is in
                // main_loop). Errors go into the slot (C's `se->error = r; break`).
                let is_char = kind == crate::select::FdKind::Char;
                match state.select_request_driver(Some(fp_slot), filp_idx, is_char, want, block) {
                    Ok(ready) => {
                        if !ready.is_empty_real() {
                            let se = state.select_table.get_mut(slot_idx).unwrap();
                            let (a, b, c, mut n) = (
                                &mut se.ready_readfds,
                                &mut se.ready_writefds,
                                &mut se.ready_errorfds,
                                se.nready,
                            );
                            crate::select::ops2tab_store(
                                ready,
                                fd,
                                (rd, wr, er),
                                (a, b, c),
                                &mut n,
                            );
                            se.nready = n;
                        }
                    }
                    Err(e) => {
                        let se = state.select_table.get_mut(slot_idx).unwrap();
                        se.error = e;
                        break;
                    }
                }
            }
        }
    }
    {
        let Some(se) = state.select_table.get_mut(slot_idx) else {
            return SyscallResult::Error(minix_types::EIO);
        };
        se.starting = false;
    }
    // ── Exit decision (select.c:299-313): an error / something ready / poll with
    // no in-flight query → finish immediately; otherwise set the timeout and
    // suspend the process.
    let deferred = {
        // C `is_deferred`: any participating filp carrying UPDATE|BUSY means not done yet.
        let se = state.select_table.get(slot_idx);
        match se {
            Some(se) => se.filps.iter().flatten().any(|e| {
                state
                    .filp_table
                    .get(crate::filp::FilpId(e.filp))
                    .map(|f| {
                        crate::filp::FsfFlags::from_bits_truncate(f.select_flags as u32)
                            .intersects(
                                crate::filp::FsfFlags::UPDATE | crate::filp::FsfFlags::BUSY,
                            )
                    })
                    .unwrap_or(false)
            }),
            None => true,
        }
    };
    let (error, nready, block) = {
        let Some(se) = state.select_table.get(slot_idx) else {
            return SyscallResult::Error(minix_types::EIO);
        };
        (se.error, se.nready, se.block)
    };
    if crate::select::should_return(nready, error != 0, block, deferred) {
        return match state.select_finish(slot_idx, io) {
            Ok(v) => SyscallResult::Ok(v),
            Err(e) => SyscallResult::Error(e),
        };
    }
    // Real wait: arm + process-level suspension (C `return SUSPEND`; the reply or
    // timeout is finished by the select layer, and the worker slot is already released).
    match state.select_arm_and_suspend(slot_idx, fp_slot, plan) {
        Ok(()) => SyscallResult::Suspend,
        Err(e) => SyscallResult::Error(e),
    }
}

/// Production implementation of the pipe select probe (the vnode's `v_size` = buffered byte count).
struct VfsPipeProbe {
    size: u64,
    capacity: u64,
}

impl crate::select::PipeProbe for VfsPipeProbe {
    fn probe_read(&self) -> crate::select::ProbeOut {
        if self.size > 0 {
            crate::select::ProbeOut::Ready
        } else {
            crate::select::ProbeOut::WouldBlock
        }
    }
    fn probe_write(&self) -> crate::select::ProbeOut {
        if self.size < self.capacity {
            crate::select::ProbeOut::Ready
        } else {
            crate::select::ProbeOut::WouldBlock
        }
    }
}

/// The Fcntl **lock family** arm (C `lock_op` lock.c:21-154): fetch `struct flock`
/// → gates → region arithmetic → [`LockTable::lock_op_decision`] → four outcomes.
fn do_fcntl_lock(
    state: &mut VfsState,
    fp_slot: minix_types::UserSlot,
    fd: usize,
    cmd: crate::fcntl::FcntlCmd,
    arg_ptr: u64,
    io: &impl crate::fcntl::FlockIo,
) -> crate::call_table::SyscallResult {
    use crate::call_table::SyscallResult;
    let raw = match io.fetch(arg_ptr) {
        Ok(b) => b,
        Err(_) => return SyscallResult::Error(minix_types::EINVAL),
    };
    let (l_start, l_len, _l_pid, l_type_raw, l_whence_raw) = crate::fcntl::decode_flock(&raw);
    let Some(ltype) = crate::fcntl::LockType::from_raw(l_type_raw as u32) else {
        return SyscallResult::Error(minix_types::EINVAL);
    };
    if cmd == crate::fcntl::FcntlCmd::GetLk && ltype == crate::fcntl::LockType::Unlock {
        return SyscallResult::Error(minix_types::EINVAL);
    }
    let filp_idx = {
        let Some(fp) = state.fproc_table.get(fp_slot) else {
            return SyscallResult::Error(minix_types::EINVAL);
        };
        match fp.filps.get(fd).copied().flatten() {
            Some(idx) => idx,
            None => return SyscallResult::Error(minix_types::EBADF),
        }
    };
    let (filp_mode, filp_pos, vnode_idx) = {
        let Some(f) = state.filp_table.get(crate::filp::FilpId(filp_idx)) else {
            return SyscallResult::Error(minix_types::EBADF);
        };
        match f.vnode {
            Some(vi) => (f.mode, f.pos, vi),
            None => return SyscallResult::Error(minix_types::EBADF),
        }
    };
    let (v_size, v_fs, v_ino) = {
        let Some(v) = state.vnode_table.get(crate::vnode::VnodeId(vnode_idx)) else {
            return SyscallResult::Error(minix_types::EBADF);
        };
        (v.size as i64, v.fs, v.ino)
    };
    let ft = crate::open::FileType::from(state
        .vnode_table
        .get(crate::vnode::VnodeId(vnode_idx))
        .map(|v| v.mode)
        .unwrap_or(0));
    if ft != crate::open::FileType::Regular && ft != crate::open::FileType::Block {
        return SyscallResult::Error(minix_types::EINVAL);
    }
    if cmd != crate::fcntl::FcntlCmd::GetLk {
        if ltype == crate::fcntl::LockType::Read
            && filp_mode & crate::open::R_BIT == 0
        {
            return SyscallResult::Error(minix_types::EBADF);
        }
        if ltype == crate::fcntl::LockType::Write
            && filp_mode & crate::open::W_BIT == 0
        {
            return SyscallResult::Error(minix_types::EBADF);
        }
    }
    let Some(whence) = crate::fcntl::Whence::from_raw(l_whence_raw as u32) else {
        return SyscallResult::Error(minix_types::EINVAL);
    };
    let base = whence.base(filp_pos, v_size);
    let region = match crate::fcntl::compute_region(base, l_start, l_len) {
        Ok(r) => r,
        Err(e) => return SyscallResult::Error(e.to_errno()),
    };
    let pid = state
        .fproc_table
        .get(fp_slot)
        .map(|fp| fp.pid as u32)
        .unwrap_or(0);
    let vnode_key = crate::fcntl::VnodeKey { fs: v_fs, ino: v_ino };
    let Some(op) = crate::fcntl::LockOp::from_req(cmd, ltype) else {
        return SyscallResult::Error(minix_types::EINVAL);
    };
    let outcome = match state.lock_table.lock_op_decision(op, pid, vnode_key, region, fd) {
        Ok(o) => o,
        Err(e) => return SyscallResult::Error(e.to_errno()),
    };
    match outcome {
        crate::fcntl::LockOutcome::Granted => SyscallResult::Ok(0),
        crate::fcntl::LockOutcome::QueryMiss => {
            let mut raw = [0u8; minix_types::FLOCK_SIZE];
            crate::fcntl::encode_flock(&mut raw, 0, 0, 0, minix_types::F_UNLCK, 0);
            match io.store(arg_ptr, &raw) {
                Ok(()) => SyscallResult::Ok(0),
                Err(e) => SyscallResult::Error(e),
            }
        }
        crate::fcntl::LockOutcome::QueryHit(ans) => {
            let mut raw = [0u8; minix_types::FLOCK_SIZE];
            let ty = match ans.lock_type {
                crate::fcntl::LockType::Read => minix_types::F_RDLCK,
                crate::fcntl::LockType::Write => minix_types::F_WRLCK,
                crate::fcntl::LockType::Unlock => minix_types::F_UNLCK,
            };
            crate::fcntl::encode_flock(&mut raw, ans.first, ans.len, ans.pid as i32, ty, 0);
            match io.store(arg_ptr, &raw) {
                Ok(()) => SyscallResult::Ok(0),
                Err(e) => SyscallResult::Error(e),
            }
        }
        crate::fcntl::LockOutcome::Wait(w) => {
            // C lock.c:93-97: `fp_u.u_flock = {fd, cmd, arg}` + suspend; in this
            // model the wait site is carried by `BlockedOn::Flock` with a
            // process-level suspension (worker released).
            if let Some(fp) = state.fproc_table.get_mut(fp_slot) {
                fp.blocked_on = crate::fproc::BlockedOn::Flock(crate::fproc::FlockBlock {
                    fd: w.fd,
                    cmd: crate::fproc::FlockCmd::SetLkw,
                    arg: minix_types::VirBytes::new(arg_ptr),
                });
            }
            if let Some(idx) = state.current_worker {
                state.worker_pool.release(idx);
                state.current_worker = None;
            }
            SyscallResult::Suspend
        }
        crate::fcntl::LockOutcome::Unlocked { revive } => {
            // C lock.c:133: once the table changed, call `lock_revive()` — every
            // process waiting for a lock is revived (the wakeup surface is the
            // waiters' rerun path, not modeled here).
            let _ = revive;
            SyscallResult::Ok(0)
        }
    }
}

/// The `F_FREESP` arm of Fcntl (C misc.c:184-239): REG gate → W_BIT gate → fetch
/// flock → [`crate::fcntl::freesp_span`] → `REQ_FTRUNC`; a zero length sets
/// `v_size = start` after the reply lands (misc.c:236-237, in the continuation
/// [`crate::worker::WorkerCont::Freesp`]).
fn do_fcntl_freesp(
    state: &mut VfsState,
    fp_slot: minix_types::UserSlot,
    fd: usize,
    arg_ptr: u64,
    io: &impl crate::fcntl::FlockIo,
) -> crate::call_table::SyscallResult {
    use crate::call_table::SyscallResult;
    let raw = match io.fetch(arg_ptr) {
        Ok(b) => b,
        Err(_) => return SyscallResult::Error(minix_types::EINVAL),
    };
    let (l_start, l_len, _, _, l_whence_raw) = crate::fcntl::decode_flock(&raw);
    let filp_idx = {
        let Some(fp) = state.fproc_table.get(fp_slot) else {
            return SyscallResult::Error(minix_types::EINVAL);
        };
        match fp.filps.get(fd).copied().flatten() {
            Some(idx) => idx,
            None => return SyscallResult::Error(minix_types::EBADF),
        }
    };
    let (filp_mode, filp_pos, vnode_idx) = {
        let Some(f) = state.filp_table.get(crate::filp::FilpId(filp_idx)) else {
            return SyscallResult::Error(minix_types::EBADF);
        };
        match f.vnode {
            Some(vi) => (f.mode, f.pos, vi),
            None => return SyscallResult::Error(minix_types::EBADF),
        }
    };
    let (v_mode, v_size, fs_e, ino) = {
        let Some(v) = state.vnode_table.get(crate::vnode::VnodeId(vnode_idx)) else {
            return SyscallResult::Error(minix_types::EBADF);
        };
        (v.mode, v.size as i64, v.fs, v.ino)
    };
    if crate::open::FileType::from(v_mode) != crate::open::FileType::Regular {
        return SyscallResult::Error(minix_types::EINVAL);
    }
    if filp_mode & crate::open::W_BIT == 0 {
        return SyscallResult::Error(minix_types::EBADF);
    }
    let Some(whence) = crate::fcntl::Whence::from_raw(l_whence_raw as u32) else {
        return SyscallResult::Error(minix_types::EINVAL);
    };
    let base = whence.base(filp_pos, v_size);
    let (start, end) = match crate::fcntl::freesp_span(v_size, base, l_start, l_len) {
        Ok(crate::fcntl::FreespSpan::TruncateTo { start, end }) => (start, end),
        Ok(crate::fcntl::FreespSpan::TruncateSize(start)) => (start, 0),
        Err(e) => return SyscallResult::Error(e.to_errno()),
    };
    let Some(worker) = state.current_worker else {
        return SyscallResult::Error(minix_types::EAGAIN);
    };
    let user = state
        .fproc_table
        .get(fp_slot)
        .map(|fp| fp.endpoint)
        .unwrap_or(minix_types::Endpoint::NONE);
    let vmnt = match state.vmnt_table.find_by_fs(fs_e) {
        Some(v) => v.0,
        None => return SyscallResult::Error(minix_types::EIO),
    };
    if let Some(wp) = state.worker_pool.get_mut(worker) {
        wp.cont = Some(crate::worker::WorkerCont::Freesp {
            vnode: vnode_idx,
            zero_len: l_len == 0,
            start,
        });
    }
    state.pending_fs = Some(crate::main_loop::PendingFs {
        vmnt,
        fs_e,
        worker,
        grant: 0,
        user,
        req: crate::request::encode_ftrunc(ino, start, end),
    });
    SyscallResult::Suspend
}

/// The caller's endpoint (needed for the cross-space fetch of the timeout struct).
fn io_who(state: &VfsState, fp_slot: minix_types::UserSlot) -> i32 {
    state
        .fproc_table
        .get(fp_slot)
        .map(|fp| fp.endpoint.0)
        .unwrap_or(0)
}

use crate::fproc::OPEN_MAX;


#[cfg(test)]
mod tests {
    use super::*;
    use crate::call_table::VFS_BASE;
    use minix_types::{Endpoint, Message, MessageM7, MessageUnion};

    fn msg_of(call: VfsCallNum, i1: i32, i2: i32, i3: i32, i4: i32) -> Message {
        Message {
            m_source: Endpoint::from_generation_slot(1, 0),
            m_type: call as i32,
            m_u: MessageUnion {
                m_m7: MessageM7 {
                    m7i1: i1,
                    m7i2: i2,
                    m7i3: i3,
                    m7i4: i4,
                    ..Default::default()
                },
            },
        }
    }

    fn seeded(slot_pid: i32) -> VfsState {
        let mut state = VfsState::new();
        let slot = minix_types::UserSlot::new(0);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.pid = slot_pid;
            fp.endpoint = Endpoint::from_generation_slot(1, 0);
        }
        state.current_fp_slot = Some(slot);
        state.initialized = true;
        state
    }

    /// `transfer_position`: C's two position sources (read.c:145/:234).
    #[test]
    fn test_transfer_position_sources() {
        // No O_APPEND: take the filp position.
        assert_eq!(transfer_position(0, 0x1000, 0x9000), 0x1000);
        // With O_APPEND: take the file size (append write).
        assert_eq!(
            transfer_position(
                crate::fcntl::O_APPEND as i32,
                0x1000,
                0x9000
            ),
            0x9000
        );
        // Empty file + O_APPEND: position 0.
        assert_eq!(transfer_position(crate::fcntl::O_APPEND as i32, 0x1000, 0), 0);
    }

    /// The `Write` arm (the third template): gate order and type branch match Read;
    /// for the append position see `transfer_position`, and the file-size update
    /// after the write lives in the continuation (see the continuation tests in main_loop).
    #[test]
    fn test_dispatch_write_gates_and_type_branch() {
        use minix_types::Endpoint;

        let write_msg = |fd: i32, buf: u64, len: u64| {
            let mut m = Message::default();
            m.m_source = Endpoint::from_generation_slot(1, 0);
            m.m_type = VfsCallNum::Write as i32;
            // SAFETY: write payload fd@0, buf@8, len@16 (same layout as minix-sys write_via).
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[8..16].copy_from_slice(&buf.to_le_bytes());
                raw[16..24].copy_from_slice(&len.to_le_bytes());
            }
            m
        };

        let mut state = seeded(100);
        state.current_message = write_msg(-1, 0x5000, 16);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Write),
            SyscallResult::Error(minix_types::EBADF)
        );
        state.current_message = write_msg(3, 0x5000, 16);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Write),
            SyscallResult::Error(minix_types::EBADF)
        );

        let fid = state.filp_table.alloc_filp(0o644).unwrap();
        state
            .fproc_table
            .get_mut(minix_types::UserSlot::new(0))
            .unwrap()
            .filps[3] = Some(fid.get());
        let vid = state.vnode_table.alloc().unwrap();
        {
            let v = state.vnode_table.get_mut(vid).unwrap();
            v.fs = Endpoint::MFS;
            v.ino = 9;
            v.mode = crate::open::S_IFCHR | 0o644;
        }
        state
            .filp_table
            .get_mut(crate::filp::FilpId(fid.get()))
            .unwrap()
            .vnode = Some(vid.get());
        state.current_message = write_msg(3, 0x5000, 16);
        // After the cdev route is wired, CHR goes through it — the default sdev is 0
        // (major 0), there is no dmap row, and C `cdev_io`'s lookup gate
        // (cdev.c:292-293) returns EIO.
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Write),
            SyscallResult::Error(minix_types::EIO),
            "字符设备走 cdev 路由（无驱动行 → EIO）"
        );

        state.vnode_table.get_mut(vid).unwrap().mode = crate::open::S_IFREG | 0o644;
        state.current_message = write_msg(3, 0x5000, 16);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Write),
            SyscallResult::Error(minix_types::EIO),
            "常规文件：无 vmnt → EIO"
        );
    }

    /// The character routing of the `Write` arm (C `read_write`'s CHR branch →
    /// `cdev_io`): when dmap has a driver it really builds a `CDEV_WRITE` request
    /// (sending is unreachable on the host → EIO), but the request is already
    /// parked in the slot in wire format — m_type/minor/id/count/grant fields
    /// align with `mess_vfs_lchardriver_readwrite` (ipc.h:2238-2249); the position
    /// does not advance (C read.c:216's optimistic advancement happens only after
    /// a successful send).
    #[test]
    fn test_dispatch_write_char_routes_to_driver() {
        let mut state = seeded(100);
        // On the host the grant table's first growth cannot pass `sys_setgrant`:
        // warm it up so the data magic grant is really created (otherwise EIO comes
        // from the grant gate and the send is never reached).
        crate::main_loop::warm_grants(&mut state);
        let user = state
            .fproc_table
            .get(minix_types::UserSlot::new(0))
            .unwrap()
            .endpoint;
        let fid = state.filp_table.alloc_filp(0o644).unwrap();
        state
            .fproc_table
            .get_mut(minix_types::UserSlot::new(0))
            .unwrap()
            .filps[3] = Some(fid.get());
        let vid = state.vnode_table.alloc().unwrap();
        {
            let v = state.vnode_table.get_mut(vid).unwrap();
            v.fs = Endpoint::MFS;
            v.ino = 9;
            v.mode = crate::open::S_IFCHR | 0o644;
            // /dev/console: major 4 (TTY_MAJOR, dmap.h:25), minor 0.
            v.sdev = 4 << 8;
        }
        state
            .filp_table
            .get_mut(crate::filp::FilpId(fid.get()))
            .unwrap()
            .vnode = Some(vid.get());
        let mut row = crate::device_map::DmapEntry::empty();
        row.driver = Some(Endpoint::from_generation_slot(0, 12));
        state.dmap_table.set(4, row);
        let worker = state
            .worker_pool
            .assign_first_fit(
                minix_types::UserSlot::new(0),
                crate::worker::WorkerFunc::DoWork,
                &Message::default(),
            )
            .unwrap();
        state.current_worker = Some(worker);

        let mut m = Message {
            m_source: Endpoint::from_generation_slot(1, 0),
            m_type: VfsCallNum::Write as i32,
            ..Message::default()
        };
        // SAFETY: write payload fd@0, buf@8, len@16.
        unsafe {
            let raw = &mut m.m_u.raw;
            raw[0..4].copy_from_slice(&3i32.to_le_bytes());
            raw[8..16].copy_from_slice(&0x5000u64.to_le_bytes());
            raw[16..24].copy_from_slice(&16u64.to_le_bytes());
        }
        state.current_message = m;
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Write),
            SyscallResult::Error(minix_types::EIO),
            "宿主构建下驱动发送不可达 → EIO"
        );
        let wp = state.worker_pool.get(worker).unwrap();
        let req = wp.sendrec.expect("请求已停在槽上");
        assert_eq!(
            req.m_type,
            minix_chardriver::protocol::CdevRequest::Write.message_type()
        );
        // SAFETY(test): read back using the readwrite payload offsets (ipc.h:2238-2249).
        unsafe {
            let raw = &req.m_u.raw;
            let grant = i32::from_le_bytes(raw[8..12].try_into().unwrap());
            assert_ne!(grant, minix_types::GRANT_INVALID, "数据 magic grant 已发");
            let count = u64::from_le_bytes(raw[16..24].try_into().unwrap());
            assert_eq!(count, 16, "count = 请求长度");
            let id = i32::from_le_bytes(raw[36..40].try_into().unwrap());
            assert_eq!(id, user.get(), "id = 发请求进程（cdev.c:326）");
            let minor = u16::from_le_bytes(raw[44..46].try_into().unwrap());
            assert_eq!(minor, 0, "minor = /dev/console 的 0");
        }
        assert!(wp.cont.is_some(), "续接标识（CdevIo）已挂——等驱动回复收尾");
        assert_eq!(
            state
                .filp_table
                .get(crate::filp::FilpId(fid.get()))
                .unwrap()
                .pos,
            0,
            "发送失败：位置不前移"
        );
    }

    /// The `/dev/tty` redirect of the character route (C `cdev_get` → `cdev_map`,
    /// cdev.c:64-97): with a controlling terminal it is replaced by the real
    /// device and minor is sent for the new device; without one → `NO_DEV` →
    /// `cdev_io`'s EIO (the read/write lookup surface, not the open/close path's ENXIO).
    #[test]
    fn test_dispatch_write_char_ctty_redirect() {
        let mut state = seeded(100);
        crate::main_loop::warm_grants(&mut state);
        let fid = state.filp_table.alloc_filp(0o644).unwrap();
        state
            .fproc_table
            .get_mut(minix_types::UserSlot::new(0))
            .unwrap()
            .filps[3] = Some(fid.get());
        let vid = state.vnode_table.alloc().unwrap();
        {
            let v = state.vnode_table.get_mut(vid).unwrap();
            v.fs = Endpoint::MFS;
            v.ino = 9;
            v.mode = crate::open::S_IFCHR | 0o644;
            // /dev/tty: CTTY_MAJOR 5 (dmap.h:26).
            v.sdev = 5 << 8;
        }
        state
            .filp_table
            .get_mut(crate::filp::FilpId(fid.get()))
            .unwrap()
            .vnode = Some(vid.get());
        let mut row = crate::device_map::DmapEntry::empty();
        row.driver = Some(Endpoint::from_generation_slot(0, 12));
        state.dmap_table.set(4, row);
        let worker = state
            .worker_pool
            .assign_first_fit(
                minix_types::UserSlot::new(0),
                crate::worker::WorkerFunc::DoWork,
                &Message::default(),
            )
            .unwrap();
        state.current_worker = Some(worker);

        let msg = |fd: i32| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Write as i32,
                ..Message::default()
            };
            // SAFETY: write payload fd@0, buf@8, len@16.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[8..16].copy_from_slice(&0x5000u64.to_le_bytes());
                raw[16..24].copy_from_slice(&16u64.to_le_bytes());
            }
            m
        };

        // No controlling terminal: `cdev_map` gives `NO_DEV` → EIO, and no request is parked.
        state.current_message = msg(3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Write),
            SyscallResult::Error(minix_types::EIO),
            "/dev/tty 无控制终端 → EIO"
        );
        assert!(
            state.worker_pool.get(worker).unwrap().sendrec.is_none(),
            "重定向失败：不发请求"
        );

        // With a controlling terminal (real device major 4 minor 7): after substitution minor = 7.
        state
            .fproc_table
            .get_mut(minix_types::UserSlot::new(0))
            .unwrap()
            .tty = (4 << 8) | 7;
        state.current_message = msg(3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Write),
            SyscallResult::Error(minix_types::EIO),
            "宿主发送不可达 → EIO"
        );
        let req = state
            .worker_pool
            .get(worker)
            .unwrap()
            .sendrec
            .expect("重定向后请求已停槽");
        // SAFETY(test): minor is at readwrite payload @44.
        let minor = unsafe { u16::from_le_bytes(req.m_u.raw[44..46].try_into().unwrap()) };
        assert_eq!(minor, 7, "minor 按控制终端的真实设备发");
    }

    /// Gates of the `Creat` arm: absence of `O_CREAT` is EINVAL (C `do_creat:71-72`); a legal
    /// path always goes through fetch_name (no inline field on the wire, so the host
    /// cannot fetch it → EINVAL); a length over PATH_MAX hits fetch's first gate,
    /// ENAMETOOLONG (matching utility.c:62-65).
    #[test]
    fn test_dispatch_creat_gates() {
        use minix_types::Endpoint;

        let creat_msg = |flags: u32, mode: u32, path: &[u8]| {
            let mut m = Message::default();
            m.m_source = Endpoint::from_generation_slot(1, 0);
            m.m_type = VfsCallNum::Creat as i32;
            // SAFETY: creat payload name@0/len@8/flags@16/mode@20 + padding
            // (ipc.h:628-637 — **no buf field**; the path is delivered only through
            // the caller's address space, and C do_creat:72-74 always calls fetch_name).
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..8].copy_from_slice(&(0x7000u64).to_le_bytes());
                raw[8..16].copy_from_slice(&(path.len() as u64).to_le_bytes());
                raw[16..20].copy_from_slice(&flags.to_le_bytes());
                raw[20..24].copy_from_slice(&mode.to_le_bytes());
            }
            m
        };

        let mut state = seeded(100);
        // O_CREAT absent → EINVAL.
        state.current_message = creat_msg(0, 0o644, b"/x\0");
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Creat),
            SyscallResult::Error(minix_types::EINVAL)
        );
        // Legal: always goes through fetch to get the path — under the host build
        // the caller's memory is unreachable → EINVAL (`SysPathFetcher`'s failure
        // surface); the shape of the walk continuation is pinned by the same-kind
        // continuation tests for open/mkdir, and the real-hardware half of creat is
        // deferred to T2.
        state.current_message = creat_msg(0x200, 0o644, b"/x\0");
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Creat),
            SyscallResult::Error(minix_types::EINVAL)
        );
        // Length > PATH_MAX: ENAMETOOLONG right at fetch's first gate (matching
        // utility.c:62-65), deterministically observable on the host.
        let long = [b'a'; crate::path::PATH_MAX + 1];
        state.current_message = creat_msg(0x200, 0o644, &long);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Creat),
            SyscallResult::Error(minix_types::ENAMETOOLONG)
        );
    }

    /// Gates of the `Open` arm: `O_CREAT` present is EINVAL (C `do_open:46-47`, libc
    /// splits open() into the OPEN/CREAT calls); a path over the inline limit goes
    /// through fetch (unreachable on the host → EINVAL, >PATH_MAX is ENAMETOOLONG at
    /// the gate); an inline path reaches the walk (under the host build the first
    /// lookup's grant is unreachable → EIO).
    #[test]
    fn test_dispatch_open_gates() {
        use minix_types::Endpoint;

        let open_msg = |flags: u32, path: &[u8]| {
            let mut m = Message::default();
            m.m_source = Endpoint::from_generation_slot(1, 0);
            m.m_type = VfsCallNum::Open as i32;
            // SAFETY: open payload name@0/len@8/flags@16/buf@24 (OpenPathPayload).
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..8].copy_from_slice(&(0x7000u64).to_le_bytes());
                raw[8..16].copy_from_slice(&(path.len() as u64).to_le_bytes());
                raw[16..20].copy_from_slice(&flags.to_le_bytes());
                let n = path.len().min(raw.len() - 24);
                raw[24..24 + n].copy_from_slice(&path[..n]);
            }
            m
        };

        // O_CREAT present → EINVAL (the bit is `OpenFlags::CREAT` = 0x200).
        let mut state = seeded(100);
        state.current_message = open_msg(0x0000_0200, b"/x\0");
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Open),
            SyscallResult::Error(minix_types::EINVAL),
            "O_CREAT 归 creat 臂"
        );

        // Over the inline limit (33 bytes including NUL) → the fetch half: under
        // the host build the caller's memory is unreachable → EINVAL
        // (`SysPathFetcher`'s failure surface).
        let long = [b'a'; 33];
        state.current_message = open_msg(0, &long);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Open),
            SyscallResult::Error(minix_types::EINVAL)
        );

        // Length > PATH_MAX: fetch's first gate ENAMETOOLONG (utility.c:62-65).
        let over = [b'a'; crate::path::PATH_MAX + 1];
        state.current_message = open_msg(0, &over);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Open),
            SyscallResult::Error(minix_types::ENAMETOOLONG)
        );

        // Legal inline path: reaches the walk (under the host build the grant is
        // unreachable → EIO). Calling dispatch directly has no run_once slot-binding
        // layer, so bind a slot by hand (the arm's suspension phase needs it).
        let worker = state
            .worker_pool
            .assign_first_fit(
                minix_types::UserSlot::new(0),
                crate::worker::WorkerFunc::DoWork,
                &Message::default(),
            )
            .unwrap();
        state.current_worker = Some(worker);
        state.current_message = open_msg(0, b"/x\0");
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Open),
            SyscallResult::Error(minix_types::EIO)
        );
    }

    /// The `Ftruncate` arm (fd-based truncation): gate order follows C `do_ftruncate` —
    /// negative length → EINVAL; no filp → EBADF; **write-bit gate**
    /// (`filp_mode & W_BIT`) → EBADF; size unchanged → plain Ok (don't disturb the FS);
    /// non-REG/FIFO → EINVAL; the rest reaches the mount window and request encoding.
    #[test]
    fn test_dispatch_ftruncate_gates() {
        use minix_types::Endpoint;

        let ftrunc_msg = |fd: i32, length: i64| {
            let mut m = Message::default();
            m.m_source = Endpoint::from_generation_slot(1, 0);
            m.m_type = VfsCallNum::Ftruncate as i32;
            // SAFETY: ftruncate payload offset@0, fd@8 (C mess_lc_vfs_truncate).
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..8].copy_from_slice(&length.to_le_bytes());
                raw[8..12].copy_from_slice(&fd.to_le_bytes());
            }
            m
        };

        let mut state = seeded(100);
        state.current_message = ftrunc_msg(3, -1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Ftruncate),
            SyscallResult::Error(minix_types::EINVAL),
            "负长度"
        );
        state.current_message = ftrunc_msg(3, 100);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Ftruncate),
            SyscallResult::Error(minix_types::EBADF),
            "无 filp"
        );

        // Has a filp but read-only (no write bit in mode) → EBADF (C link.c:346-347).
        let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
        state
            .fproc_table
            .get_mut(minix_types::UserSlot::new(0))
            .unwrap()
            .filps[3] = Some(fid.get());
        let vid = state.vnode_table.alloc().unwrap();
        {
            let v = state.vnode_table.get_mut(vid).unwrap();
            v.fs = Endpoint::MFS;
            v.ino = 11;
            v.mode = crate::open::S_IFREG | 0o644;
            v.size = 50;
        }
        state.filp_table.get_mut(crate::filp::FilpId(fid.get())).unwrap().vnode = Some(vid.get());
        state.current_message = ftrunc_msg(3, 100);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Ftruncate),
            SyscallResult::Error(minix_types::EBADF),
            "只读 fd"
        );

        // Write bit granted: **size unchanged → plain Ok(0) (don't disturb the FS)**
        // (C link.c:349-354's `r = OK`; this was once wrongly written as "return the
        // new length" and the test repeated the mistake — when fixing, check against
        // the C line, not against the implementation).
        state.filp_table.get_mut(crate::filp::FilpId(fid.get())).unwrap().mode =
            crate::open::R_BIT | crate::open::W_BIT;
        state.current_message = ftrunc_msg(3, 50);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Ftruncate),
            SyscallResult::Ok(0)
        );

        // Type gate: directory → EINVAL (C truncate_vnode:375-376).
        state.vnode_table.get_mut(vid).unwrap().mode = crate::open::S_IFDIR | 0o755;
        state.current_message = ftrunc_msg(3, 10);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Ftruncate),
            SyscallResult::Error(minix_types::EINVAL)
        );

        // Regular file + different new length: reaches the mount window (no vmnt → EIO).
        state.vnode_table.get_mut(vid).unwrap().mode = crate::open::S_IFREG | 0o644;
        state.current_message = ftrunc_msg(3, 10);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Ftruncate),
            SyscallResult::Error(minix_types::EIO)
        );
    }

    /// `Ftrunc` continuation: on success update the vnode size per C `truncate_vnode:382`;
    /// on failure leave it alone; both return the status to the user and release the slot.
    #[test]
    fn test_worker_continuation_ftrunc_updates_size() {
        use crate::worker::WorkerCont;
        let user = Endpoint::from_generation_slot(1, 0);
        let mut state = VfsState::new();
        let slot = minix_types::UserSlot::new(0);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
        }
        let vid = state.vnode_table.alloc().unwrap();
        state.vnode_table.get_mut(vid).unwrap().size = 50;

        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::Ftrunc { vnode: vid.get(), newsize: 10 });
            wp.sendrec = Some(Message { m_type: 0, ..Message::default() });
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(state.vnode_table.get(vid).unwrap().size, 10, "成功即更新大小");
        assert_eq!(
            state.take_reply().map(|(t, msg)| (t, msg.m_type)),
            Some((user, 0))
        );

        // Failure: size unchanged.
        let idx2 = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        {
            let wp = state.worker_pool.get_mut(idx2).unwrap();
            wp.cont = Some(WorkerCont::Ftrunc { vnode: vid.get(), newsize: 999 });
            wp.sendrec = Some(Message {
                m_type: minix_types::EACCES,
                ..Message::default()
            });
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(state.vnode_table.get(vid).unwrap().size, 10, "失败不动大小");
        assert_eq!(
            state.take_reply().map(|(t, msg)| (t, msg.m_type)),
            Some((user, -minix_types::EACCES))
        );
    }

    /// The `Read` arm (the second template): gate order follows C `read_write` —
    /// negative fd / no filp → EBADF; `size > SSIZE_MAX` → EINVAL; **character**
    /// devices take the cdev route (no driver row is EIO), pipes still return Nosys
    /// (not wired); regular files reach the mount window and the grant.
    #[test]
    fn test_dispatch_read_gates_and_type_branch() {
        use minix_types::Endpoint;

        let read_msg = |fd: i32, buf: u64, len: u64| {
            let mut m = Message::default();
            m.m_source = Endpoint::from_generation_slot(1, 0);
            m.m_type = VfsCallNum::Read as i32;
            // SAFETY: read payload fd@0, buf@8, len@16 (same layout as minix-sys read_via).
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[8..16].copy_from_slice(&buf.to_le_bytes());
                raw[16..24].copy_from_slice(&len.to_le_bytes());
            }
            m
        };

        let mut state = seeded(100);
        state.current_message = read_msg(-1, 0x5000, 16);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Read),
            SyscallResult::Error(minix_types::EBADF)
        );

        state.current_message = read_msg(3, 0x5000, 16);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Read),
            SyscallResult::Error(minix_types::EBADF),
            "无 filp → EBADF"
        );

        // filp + non-regular vnode (pipe) → the type branch is not wired: Nosys.
        let fid = state.filp_table.alloc_filp(0o644).unwrap();
        state
            .fproc_table
            .get_mut(minix_types::UserSlot::new(0))
            .unwrap()
            .filps[3] = Some(fid.get());
        let vid = state.vnode_table.alloc().unwrap();
        {
            let v = state.vnode_table.get_mut(vid).unwrap();
            v.fs = Endpoint::MFS;
            v.ino = 7;
            v.mode = crate::open::S_IFIFO | 0o644;
        }
        state.filp_table.get_mut(crate::filp::FilpId(fid.get())).unwrap().vnode = Some(vid.get());
        state.current_message = read_msg(3, 0x5000, 16);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Read),
            SyscallResult::Nosys,
            "管道分支未接线"
        );

        // With the cdev route wired: character device → cdev route; the default sdev
        // is 0 (major 0), there is no dmap row → C `cdev_io`'s lookup gate EIO
        // (cdev.c:292-293).
        state.vnode_table.get_mut(vid).unwrap().mode = crate::open::S_IFCHR | 0o644;
        state.current_message = read_msg(3, 0x5000, 16);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Read),
            SyscallResult::Error(minix_types::EIO),
            "字符设备走 cdev 路由（无驱动行 → EIO）"
        );

        // Regular file: reaches the mount window (no vmnt → EIO).
        state.vnode_table.get_mut(vid).unwrap().mode = crate::open::S_IFREG | 0o644;
        state.current_message = read_msg(3, 0x5000, 16);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Read),
            SyscallResult::Error(minix_types::EIO)
        );

        // The over-SSIZE_MAX length gate (C read.c:150) takes effect before parsing.
        state.current_message = read_msg(3, 0x5000, u64::MAX);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Read),
            SyscallResult::Error(minix_types::EINVAL)
        );
    }

    /// Gates and order of the `Getdents` arm: `cum_io != 0` is rejected first (C
    /// read.c:290-292; it is an internal reserved slot); the fd gate and the type
    /// gate are two **different** EBADF cases (C read.c:294-297's `filp_mode & R_BIT`
    /// and `S_ISDIR`); the capability gate (FS did not declare 64-bit and the
    /// position exceeds `INT_MAX`) takes effect before the grant is built.
    #[test]
    fn test_dispatch_getdents_gates_and_type_branch() {
        use minix_types::Endpoint;

        let getdents_msg = |fd: i32, buf: u64, len: u64, cum_io: u64| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Getdents as i32,
                ..Message::default()
            };
            // SAFETY: getdents payload fd@0, buf@8, len@16, cum_io@24
            // (`mess_lc_vfs_readwrite` at ipc.h:795-802).
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[8..16].copy_from_slice(&buf.to_le_bytes());
                raw[16..24].copy_from_slice(&len.to_le_bytes());
                raw[24..32].copy_from_slice(&cum_io.to_le_bytes());
            }
            m
        };

        // `cum_io` non-zero → EINVAL (before every fd check).
        let mut state = seeded(100);
        state.current_message = getdents_msg(-1, 0x5000, 16, 1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getdents),
            SyscallResult::Error(minix_types::EINVAL)
        );

        // Negative fd and empty slot → EBADF.
        state.current_message = getdents_msg(-1, 0x5000, 16, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getdents),
            SyscallResult::Error(minix_types::EBADF)
        );
        state.current_message = getdents_msg(3, 0x5000, 16, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getdents),
            SyscallResult::Error(minix_types::EBADF)
        );

        // Attach a filp + directory vnode, but the open mode is write-only → the first EBADF gate.
        let fid = state.filp_table.alloc_filp(0o644).unwrap();
        state
            .fproc_table
            .get_mut(minix_types::UserSlot::new(0))
            .unwrap()
            .filps[3] = Some(fid.get());
        let vid = state.vnode_table.alloc().unwrap();
        {
            let v = state.vnode_table.get_mut(vid).unwrap();
            v.fs = Endpoint::MFS;
            v.ino = 7;
            v.mode = crate::open::S_IFDIR | 0o755;
            v.ref_count = 1;
        }
        {
            let f = state.filp_table.get_mut(crate::filp::FilpId(fid.get())).unwrap();
            f.vnode = Some(vid.get());
            f.mode = crate::open::W_BIT; // opened write-only
        }
        state.current_message = getdents_msg(3, 0x5000, 16, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getdents),
            SyscallResult::Error(minix_types::EBADF),
            "只写 fd 不能 getdents"
        );

        // Read bit present but the node is a regular file → the second EBADF gate.
        state
            .filp_table
            .get_mut(crate::filp::FilpId(fid.get()))
            .unwrap()
            .mode = crate::open::R_BIT;
        state.vnode_table.get_mut(vid).unwrap().mode = crate::open::S_IFREG | 0o644;
        state.current_message = getdents_msg(3, 0x5000, 16, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getdents),
            SyscallResult::Error(minix_types::EBADF),
            "getdents 只服务目录"
        );

        // Now a directory: no mount window → EIO (C `find_vmnt`'s failure surface).
        state.vnode_table.get_mut(vid).unwrap().mode = crate::open::S_IFDIR | 0o755;
        state.current_message = getdents_msg(3, 0x5000, 16, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getdents),
            SyscallResult::Error(minix_types::EIO)
        );

        // Mount window present but the FS did not declare 64-bit capability and the
        // position exceeds INT_MAX → EINVAL (C request.c:316-319).
        let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(0)).unwrap();
        v.fs = Endpoint::MFS;
        v.dev = 1;
        v.fs_flags = 0;
        state
            .filp_table
            .get_mut(crate::filp::FilpId(fid.get()))
            .unwrap()
            .pos = i32::MAX as i64 + 1;
        state.current_message = getdents_msg(3, 0x5000, 16, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getdents),
            SyscallResult::Error(minix_types::EINVAL)
        );

        // 64-bit capability declared: the capability gate passes and execution
        // reaches the worker slot and the grant (under the host build the grant is
        // unreachable → honest EIO, the same value as C's panic-on-grant-failure
        // "internal error to the outside"). Bind a worker slot first — with no slot
        // the arm returns EAGAIN (C `handle_work:150-151`), which is a different
        // gate and not part of this assertion.
        state.vmnt_table.get_mut(crate::vmnt::VmntId(0)).unwrap().fs_flags =
            crate::request::FsFlags::IS64BIT.bits();
        let idx = state
            .worker_pool
            .assign_first_fit(
                minix_types::UserSlot::new(0),
                crate::worker::WorkerFunc::DoWork,
                &Message::default(),
            )
            .expect("空闲槽");
        state.current_worker = Some(idx);
        state.current_message = getdents_msg(3, 0x5000, 16, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getdents),
            SyscallResult::Error(minix_types::EIO)
        );
    }

    /// The `Fchown` arm (fd half, C `do_chown` protect.c:53-63 + shared body):
    /// fd gate → read-only gate → three ownership rules (non-superuser: must be the
    /// owner, may not give it away, the new group must be one of its own groups) →
    /// `-1` folding → bound check → `REQ_CHOWN`; the reply carries the **new mode**,
    /// and the continuation writes uid/gid back to the vnode cache.
    #[test]
    fn test_dispatch_fchown_gates_request_and_cache() {
        use minix_types::Endpoint;

        let fchown_msg = |fd: i32, uid: u32, gid: u32| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Fchown as i32,
                ..Message::default()
            };
            // SAFETY: fchown payload name@0, len@8, fd@16, owner@20, group@24.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[16..20].copy_from_slice(&fd.to_le_bytes());
                raw[20..24].copy_from_slice(&uid.to_le_bytes());
                raw[24..28].copy_from_slice(&gid.to_le_bytes());
            }
            m
        };
        // Setup: fd 3 → filp → vnode (owner 1000, group 100, 0644).
        let user_e = Endpoint::from_generation_slot(1, 0);
        let setup = |eff_uid: u32, eff_gid: u32| {
            let mut state = seeded(100);
            state.vmnt_table.get_mut(crate::vmnt::VmntId(0)).unwrap().fs = Endpoint::MFS;
            state.vmnt_table.get_mut(crate::vmnt::VmntId(0)).unwrap().dev = 1;
            // [SD19-gate] Best-effort grant registration (failure takes the existing error reply path)
            #[allow(clippy::let_underscore_must_use)]
            let _ = state.grants.grant_direct(
                &minix_sys::syscall::DirectKernelCallTransport,
                Endpoint::MFS.get(),
                0x1000,
                8,
                minix_types::CpFlags::READ,
            );
            let fid = state.filp_table.alloc_filp(crate::open::W_BIT).unwrap();
            state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap()
                .filps[3] = Some(fid.get());
            let vid = state.vnode_table.alloc().unwrap();
            {
                let v = state.vnode_table.get_mut(vid).unwrap();
                v.fs = Endpoint::MFS;
                v.ino = 0x42;
                v.mode = crate::open::S_IFREG | 0o644;
                v.uid = 1000;
                v.gid = 100;
                v.ref_count = 1;
            }
            state.filp_table.get_mut(fid).unwrap().vnode = Some(vid.get());
            {
                let fp = state
                    .fproc_table
                    .get_mut(minix_types::UserSlot::new(0))
                    .unwrap();
                fp.eff_uid = eff_uid;
                fp.eff_gid = eff_gid;
            }
            let idx = state
                .worker_pool
                .assign_first_fit(
                    minix_types::UserSlot::new(0),
                    crate::worker::WorkerFunc::DoWork,
                    &Message::default(),
                )
                .expect("空闲槽");
            state.current_worker = Some(idx);
            (state, vid, idx)
        };

        // 1) Non-owner (effective id 0 but... use a non-zero one: 2000) → EPERM ("must be the owner").
        let (mut state, _vid, _idx) = setup(2000, 100);
        state.current_message = fchown_msg(3, 1000, 100);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchown),
            SyscallResult::Suspend,
            "收尾路径统一报挂起（槽已由收尾释放）"
        );
        assert!(state.pending_fs.is_none(), "门没过不该发请求");
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((Endpoint::from_generation_slot(1, 0), -minix_types::EPERM))
        );

        // 2) Owner but giving ownership away (request uid ≠ file uid) → EPERM.
        let (mut state, _vid, _idx) = setup(1000, 100);
        state.current_message = fchown_msg(3, 2000, 100);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchown),
            SyscallResult::Suspend
        );
        assert!(state.pending_fs.is_none());
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user_e, -minix_types::EPERM))
        );

        // 3) Owner, not giving away, but the new group is not one of its groups → EPERM.
        let (mut state, _vid, _idx) = setup(1000, 100);
        state.current_message = fchown_msg(3, 1000, 999);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchown),
            SyscallResult::Suspend
        );
        assert!(state.pending_fs.is_none());
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user_e, -minix_types::EPERM))
        );

        // 4) A non-superuser passing `-1` as the new owner is also rejected by the
        // "may not give away" rule (C `do_chown:148` uses the **raw value in the
        // message**: `vp->v_uid != uid`, and `(uid_t)-1` is not any real owner).
        // This C behavior looks odd, but it is the baseline; pin it.
        let (mut state, _vid, _idx) = setup(1000, 100);
        state.current_message = fchown_msg(3, u32::MAX, 100);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchown),
            SyscallResult::Suspend
        );
        assert!(state.pending_fs.is_none());
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user_e, -minix_types::EPERM)),
            "非超级用户 + uid=-1：C 判 EPERM"
        );

        // 5) All gates pass (owner does not give away, changes its own group) → send REQ_CHOWN.
        let (mut state, vid, idx) = setup(1000, 100);
        state.current_message = fchown_msg(3, 1000, 100);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchown),
            SyscallResult::Suspend
        );
        let p = state.pending_fs.as_ref().expect("已登记 REQ_CHOWN");
        assert_eq!(p.req.m_type, minix_types::REQ_CHOWN);
        // SAFETY(test): read the three fields back using chown_req_off.
        unsafe {
            let raw = &p.req.m_u.raw;
            let ino = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let uid = u32::from_le_bytes(raw[8..12].try_into().unwrap());
            let gid = u32::from_le_bytes(raw[12..16].try_into().unwrap());
            assert_eq!(ino, 0x42);
            assert_eq!(uid, 1000);
            assert_eq!(gid, 100);
        }

        // The reply carries the new mode (the FS cleared setuid) → uid/gid/mode all enter the cache.
        state.pending_fs = None;
        let mut reply = Message { m_type: 0, ..Message::default() };
        // SAFETY(test): fill in the new mode using chown_reply_off.
        unsafe {
            reply.m_u.raw[0..4]
                .copy_from_slice(&(crate::open::S_IFREG | 0o600).to_le_bytes());
        }
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.sendrec = Some(reply);
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user_e, 0))
        );
        let v = state.vnode_table.get(vid).unwrap();
        assert_eq!((v.uid, v.gid), (1000, 100), "uid/gid 写回缓存");
        assert_eq!(v.mode, crate::open::S_IFREG | 0o600, "模式取回复里的新值");

        // 6) Superuser passing `-1`: both fields fold to the current values (C
        // `do_chown:154-158`'s `keep_id`), and the three rules are skipped entirely for root.
        let (mut state, _vid, _idx) = setup(crate::link::SU_UID, 100);
        state.current_message = fchown_msg(3, u32::MAX, u32::MAX);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchown),
            SyscallResult::Suspend
        );
        let p = state.pending_fs.as_ref().expect("root 直接过门");
        // SAFETY(test): read back the folded result using chown_req_off.
        unsafe {
            let raw = &p.req.m_u.raw;
            let uid = u32::from_le_bytes(raw[8..12].try_into().unwrap());
            let gid = u32::from_le_bytes(raw[12..16].try_into().unwrap());
            assert_eq!((uid, gid), (1000, 100), "-1 折算成文件现有值");
        }
    }

    /// The first gates of the socket family (C `do_socket`/`do_socketpair`
    /// socket.c:176-273 and `do_shutdown` socket.c:744-762): **no driver for the
    /// domain → EAFNOSUPPORT**, **not enough fd slots → EMFILE**, **fd is not a
    /// socket → ENOTSOCK**, **invalid how value → EINVAL**. All of these are local
    /// decisions; the next step's driver conversation (`sdev_*`) pipeline is not
    /// wired, so after the gates it honestly returns ENOSYS.
    #[test]
    fn test_dispatch_socket_family_local_gates() {
        use minix_types::Endpoint;

        let setup = || {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            let idx = state
                .worker_pool
                .assign_first_fit(
                    minix_types::UserSlot::new(0),
                    crate::worker::WorkerFunc::DoWork,
                    &Message::default(),
                )
                .expect("空闲槽");
            state.current_worker = Some(idx);
            (state, idx)
        };
        let socket_msg = |call: VfsCallNum, domain: i32, ty: i32| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: call as i32,
                ..Message::default()
            };
            // SAFETY: mess_lc_vfs_socket: domain@0, type@4, protocol@8.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&domain.to_le_bytes());
                raw[4..8].copy_from_slice(&ty.to_le_bytes());
            }
            m
        };
        let shutdown_msg = |fd: i32, how: i32| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Shutdown as i32,
                ..Message::default()
            };
            // SAFETY: mess_lc_vfs_shutdown: fd@0, how@4.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[4..8].copy_from_slice(&how.to_le_bytes());
            }
            m
        };

        // 1) No socket driver for the domain → EAFNOSUPPORT (the smap domain table is empty).
        let (mut state, _idx) = setup();
        state.current_message = socket_msg(VfsCallNum::Socket, 2, 1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Socket),
            SyscallResult::Error(minix_types::EAFNOSUPPORT)
        );

        // 2) Domain mapped (pfmap[2] = row 0) → passes the domain gate and really
        // sends `SDEV_SOCKET` to the driver; under the host build the trap is
        // unreachable → EIO (honest boundary).
        let (mut state, _idx) = setup();
        state.smap_table.pfmap[2] = Some(0);
        state.smap_table.entries[0].endpt = Some(Endpoint::from_generation_slot(0, 11));
        state.current_message = socket_msg(VfsCallNum::Socket, 2, 1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Socket),
            SyscallResult::Error(minix_types::EIO),
            "驱动对话要真发（宿主下 trap 不可达）"
        );

        // 3) Not enough fd slots → EMFILE (socketpair needs 2: leave only 1 free slot).
        let (mut state, _idx) = setup();
        state.smap_table.pfmap[2] = Some(0);
        {
            let fp = state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap();
            for i in 1..crate::fproc::OPEN_MAX {
                fp.filps[i] = Some(0);
            }
        }
        state.current_message = socket_msg(VfsCallNum::Socketpair, 2, 1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Socketpair),
            SyscallResult::Error(minix_types::EMFILE)
        );

        // 4) shutdown: invalid fd → EBADF; valid fd but not a socket → ENOTSOCK;
        // a socket but invalid how → EINVAL; all pass → ENOSYS (driver conversation gap).
        let (mut state, _idx) = setup();
        state.current_message = shutdown_msg(9, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Shutdown),
            SyscallResult::Error(minix_types::EBADF)
        );
        // Attach a regular file as fd 3 → ENOTSOCK.
        let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
        state
            .fproc_table
            .get_mut(minix_types::UserSlot::new(0))
            .unwrap()
            .filps[3] = Some(fid.get());
        let vid = state.vnode_table.alloc().unwrap();
        {
            let v = state.vnode_table.get_mut(vid).unwrap();
            v.fs = Endpoint::MFS;
            v.ino = 0x42;
            v.mode = crate::open::S_IFREG | 0o644;
            v.ref_count = 1;
        }
        state.filp_table.get_mut(fid).unwrap().vnode = Some(vid.get());
        state.current_message = shutdown_msg(3, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Shutdown),
            SyscallResult::Error(minix_types::ENOTSOCK)
        );
        // Switch to a socket vnode → passes the type gate; invalid how → EINVAL.
        state.vnode_table.get_mut(vid).unwrap().mode = crate::open::S_IFSOCK | 0o777;
        state.vnode_table.get_mut(vid).unwrap().sdev = 0x1234;
        state.current_message = shutdown_msg(3, 99);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Shutdown),
            SyscallResult::Error(minix_types::EINVAL)
        );
        state.current_message = shutdown_msg(3, 1); // SHUT_WR
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Shutdown),
            SyscallResult::Error(minix_types::EIO),
            "全过 → 真发 SDEV_SHUTDOWN（宿主下 trap 不可达 → EIO）"
        );
    }

    /// `Bind`/`Connect` (**process-level suspension**): gates same as the socket
    /// family; once passed, send `SDEV_BIND`/`SDEV_CONNECT` to the driver (the
    /// address buffer becomes a magic grant) **without waiting for a reply**, mark
    /// the process `FP_BLOCKED_ON_SDEV` and **release the worker slot**; when the
    /// driver's reply arrives, `finish_sdev_blocked` finishes it (revoke the grant +
    /// return the status to the user).
    #[test]
    fn test_dispatch_bind_connect_process_suspension() {
        use minix_types::Endpoint;

        let setup = || {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            let drv = Endpoint::from_generation_slot(0, 11);
            state.smap_table.entries[0].endpt = Some(drv);
            let dev = crate::device_map::make_smap_dev(state.smap_table.entries[0].num, 0x42);
            let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
            state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap()
                .filps[3] = Some(fid.get());
            let vid = state.vnode_table.alloc().unwrap();
            {
                let v = state.vnode_table.get_mut(vid).unwrap();
                v.fs = Endpoint::PFS;
                v.ino = 0x11;
                v.mode = crate::open::S_IFSOCK | 0o777;
                v.sdev = dev;
                v.ref_count = 1;
            }
            state.filp_table.get_mut(fid).unwrap().vnode = Some(vid.get());
            let idx = state
                .worker_pool
                .assign_first_fit(
                    minix_types::UserSlot::new(0),
                    crate::worker::WorkerFunc::DoWork,
                    &Message::default(),
                )
                .expect("空闲槽");
            state.current_worker = Some(idx);
            (state, idx, drv, dev)
        };
        let bind_msg = |call: VfsCallNum, fd: i32| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: call as i32,
                ..Message::default()
            };
            // SAFETY: mess_lc_vfs_sockaddr: fd@0, addr@8, addr_len@16.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[8..16].copy_from_slice(&0x6000u64.to_le_bytes());
                raw[16..24].copy_from_slice(&16u64.to_le_bytes());
            }
            m
        };

        // 1) Gates: negative fd → EBADF; non-socket → ENOTSOCK.
        let (mut state, _idx, _drv, _dev) = setup();
        state.current_message = bind_msg(VfsCallNum::Bind, -1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Bind),
            SyscallResult::Error(minix_types::EBADF)
        );
        // Replace the vnode fd 3 points at with a regular file (don't guess the index: take it from the filp).
        let vnode_idx = {
            let fp = state
                .fproc_table
                .get(minix_types::UserSlot::new(0))
                .unwrap();
            let filp = state
                .filp_table
                .get(crate::filp::FilpId(fp.filps[3].unwrap()))
                .unwrap();
            filp.vnode.unwrap()
        };
        state
            .vnode_table
            .get_mut(crate::vnode::VnodeId(vnode_idx))
            .unwrap()
            .mode = crate::open::S_IFREG | 0o644;
        state.current_message = bind_msg(VfsCallNum::Bind, 3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Bind),
            SyscallResult::Error(minix_types::ENOTSOCK)
        );

        // 2) Under the host build the trap is unreachable → send fails → EIO, with no dangling grant and no suspension.
        let (mut state, _idx, _drv, _dev) = setup();
        state.current_message = bind_msg(VfsCallNum::Bind, 3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Bind),
            SyscallResult::Error(minix_types::EIO)
        );
        assert!(
            matches!(
                state
                    .fproc_table
                    .get(minix_types::UserSlot::new(0))
                    .unwrap()
                    .blocked_on,
                crate::fproc::BlockedOn::None
            ),
            "发不出去就不该挂起"
        );
    }

    /// The socket branch of `Ioctl` (C `do_ioctl`'s `S_IFSOCK` branch + `sdev_ioctl`):
    /// smap driver lookup by device number → `make_ioctl_grant` (access/size decode +
    /// magic grant) → `SDEV_IOCTL` → **process-level suspension**; the reply uses the
    /// generic `SDEV_REPLY` (status in the payload's second slot), finished by
    /// `finish_sdev_blocked`.
    #[test]
    fn test_ioctl_socket_branch_and_reply() {
        use minix_types::Endpoint;

        let mut state = seeded(100);
        crate::main_loop::seed_ready_state(&mut state);
        let drv = Endpoint::from_generation_slot(0, 11);
        state.smap_table.entries[0].endpt = Some(drv);
        let dev = crate::device_map::make_smap_dev(state.smap_table.entries[0].num, 0x42);
        // fd 3 → socket vnode (`v_sdev` = that device number).
        let fid = state.filp_table.alloc_filp(crate::open::R_BIT | crate::open::W_BIT).unwrap();
        state
            .fproc_table
            .get_mut(minix_types::UserSlot::new(0))
            .unwrap()
            .filps[3] = Some(fid.get());
        let vid = state.vnode_table.alloc().unwrap();
        {
            let v = state.vnode_table.get_mut(vid).unwrap();
            v.fs = Endpoint::PFS;
            v.ino = 0x11;
            v.mode = crate::open::S_IFSOCK | 0o777;
            v.sdev = dev;
            v.ref_count = 1;
        }
        state.filp_table.get_mut(fid).unwrap().vnode = Some(vid.get());
        let idx = state
            .worker_pool
            .assign_first_fit(
                minix_types::UserSlot::new(0),
                crate::worker::WorkerFunc::DoWork,
                &Message::default(),
            )
            .expect("空闲槽");
        state.current_worker = Some(idx);

        // `ioctl(fd, FIONBIO-like)`: under the host build the send fails → EIO, with
        // **no suspension** and no dangling grant.
        let mut m = Message {
            m_source: Endpoint::from_generation_slot(1, 0),
            m_type: VfsCallNum::Ioctl as i32,
            ..Message::default()
        };
        // SAFETY: mess_lc_vfs_ioctl: fd@0, req@8, arg@16. The request bits carry
        // IOC_IN (`_IOW`: the driver reads the user buffer), and the size bits are
        // non-zero → a grant will be built.
        unsafe {
            let raw = &mut m.m_u.raw;
            raw[0..4].copy_from_slice(&3i32.to_le_bytes());
            raw[8..16].copy_from_slice(&(0x8000_0000u64 | (4u64 << 16)).to_le_bytes());
            raw[16..24].copy_from_slice(&0x6000u64.to_le_bytes());
        }
        state.current_message = m;
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Ioctl),
            SyscallResult::Error(minix_types::EIO)
        );
        assert!(matches!(
            state
                .fproc_table
                .get(minix_types::UserSlot::new(0))
                .unwrap()
                .blocked_on,
            crate::fproc::BlockedOn::None
        ));

        // Hand-build the "already suspended" site and feed a generic `SDEV_REPLY` →
        // the user gets the status from the payload (second slot) and the suspended
        // state is cleared.
        let grant = state
            .grant_user_buffer(
                drv,
                Endpoint::from_generation_slot(1, 0),
                0x6000,
                4,
                minix_types::CpFlags::READ,
            )
            .expect("grant 表已热身");
        {
            let fp = state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap();
            fp.blocked_on = crate::fproc::BlockedOn::Sdev(crate::fproc::SdevBlock {
                dev,
                call: crate::fproc::SdevCall::Ioctl,
                grants: [Some(grant), None, None],
                aux: crate::fproc::SdevAux::None,
            });
        }
        let mut reply = Message {
            m_type: minix_sockdriver::sdev::SdevReply::Reply as i32,
            ..Message::default()
        };
        reply.m_source = drv;
        // SAFETY(test): `mess_lsockdriver_vfs_reply { req_id@0; status@4; }`.
        unsafe {
            reply.m_u.raw[4..8].copy_from_slice(&(-minix_types::ENOTTY).to_le_bytes());
        }
        assert!(state.finish_sdev_blocked(&reply));
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((Endpoint::from_generation_slot(1, 0), -minix_types::ENOTTY)),
            "ioctl 的状态在通用回复的第二格"
        );
    }

    /// The paired orchestration of `Socketpair` (C `do_socketpair` socket.c:224-266):
    /// the driver's reply carries **two** socket numbers → build two `make_sock_fd`s
    /// serially (the first half carries "the other half's device number", the second
    /// half carries "the first fd") → finally reply `m_vfs_lc_fdpair { fd0, fd1 }`.
    #[test]
    fn test_socketpair_paired_orchestration() {
        use minix_types::Endpoint;

        let mut state = seeded(100);
        crate::main_loop::seed_ready_state(&mut state);
        let drv = Endpoint::from_generation_slot(0, 11);
        state.smap_table.entries[0].endpt = Some(drv);
        state.smap_table.pfmap[2] = Some(0);
        let row = state.smap_table.entries[0].num;
        // PFS mount row (both halves need `REQ_NEWNODE`).
        let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(1)).unwrap();
        v.fs = Endpoint::PFS;
        v.dev = 9;
        let idx = state
            .worker_pool
            .assign_first_fit(
                minix_types::UserSlot::new(0),
                crate::worker::WorkerFunc::DoWork,
                &Message::default(),
            )
            .expect("空闲槽");
        state.current_worker = Some(idx);
        // Set up the slot state "the `SDEV_SOCKETPAIR` reply has arrived" (paired: two sock_ids).
        let mut reply = Message {
            m_type: minix_sockdriver::sdev::SdevReply::SocketReply as i32,
            ..Message::default()
        };
        reply.m_source = drv;
        // SAFETY(test): `mess_lsockdriver_vfs_socket_reply { req_id@0;
        // sock_id@4; sock_id2@8 }`.
        unsafe {
            let raw = &mut reply.m_u.raw;
            raw[4..8].copy_from_slice(&0x10i32.to_le_bytes());
            raw[8..12].copy_from_slice(&0x11i32.to_le_bytes());
        }
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(crate::worker::WorkerCont::SdevSocket {
                pair: true,
                flags: 0,
                smap_num: row,
            });
            wp.sendrec = Some(reply);
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();

        // First half: send PFS the `REQ_NEWNODE` for dev0, with "the other half" in the continuation.
        let p = state.pending_fs.as_ref().expect("第一半的 REQ_NEWNODE");
        assert_eq!(p.req.m_type, minix_types::REQ_NEWNODE);
        let dev0 = crate::device_map::make_smap_dev(row, 0x10);
        // SAFETY(test): `mess_vfs_fs_newnode`: device@0.
        unsafe {
            let raw = &p.req.m_u.raw;
            assert_eq!(u64::from_le_bytes(raw[0..8].try_into().unwrap()), dev0);
        }
        assert!(
            matches!(
                state.worker_pool.get_mut(idx).unwrap().cont,
                Some(crate::worker::WorkerCont::SockFd {
                    pair: Some(crate::worker::PairState { second: false, .. }),
                    ..
                })
            ),
            "第一半的续接带着成对状态"
        );

        // First half's reply arrives → start the second half (dev1).
        let pfs_worker = p.worker;
        state.pending_fs = None;
        let mut r1 = Message { m_type: 0, ..Message::default() };
        // SAFETY(test): the new node details.
        unsafe {
            let raw = &mut r1.m_u.raw;
            raw[minix_types::lookup_reply_off::INODE..minix_types::lookup_reply_off::INODE + 8]
                .copy_from_slice(&0x51u64.to_le_bytes());
            raw[minix_types::lookup_reply_off::MODE..minix_types::lookup_reply_off::MODE + 4]
                .copy_from_slice(&(crate::open::S_IFSOCK | 0o777).to_le_bytes());
        }
        {
            let wp = state.worker_pool.get_mut(pfs_worker).unwrap();
            wp.sendrec = Some(r1);
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        let p = state.pending_fs.as_ref().expect("第二半的 REQ_NEWNODE");
        let dev1 = crate::device_map::make_smap_dev(row, 0x11);
        // SAFETY(test): `mess_vfs_fs_newnode`: device@0.
        unsafe {
            let raw = &p.req.m_u.raw;
            assert_eq!(u64::from_le_bytes(raw[0..8].try_into().unwrap()), dev1);
        }
        assert!(matches!(
            state.worker_pool.get_mut(pfs_worker).unwrap().cont,
            Some(crate::worker::WorkerCont::SockFd {
                pair: Some(crate::worker::PairState { second: true, .. }),
                ..
            })
        ));

        // Second half's reply arrives → reply `m_vfs_lc_fdpair { fd0, fd1 }`.
        state.pending_fs = None;
        let mut r2 = Message { m_type: 0, ..Message::default() };
        // SAFETY(test): new node details.
        unsafe {
            let raw = &mut r2.m_u.raw;
            raw[minix_types::lookup_reply_off::INODE..minix_types::lookup_reply_off::INODE + 8]
                .copy_from_slice(&0x52u64.to_le_bytes());
            raw[minix_types::lookup_reply_off::MODE..minix_types::lookup_reply_off::MODE + 4]
                .copy_from_slice(&(crate::open::S_IFSOCK | 0o777).to_le_bytes());
        }
        {
            let wp = state.worker_pool.get_mut(pfs_worker).unwrap();
            wp.sendrec = Some(r2);
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        let (target, reply) = state.take_reply().expect("回复");
        assert_eq!(target, Endpoint::from_generation_slot(1, 0));
        assert_eq!(reply.m_type, 0, "成功回 0（fd 对在载荷里）");
        // SAFETY(test): `mess_vfs_lc_fdpair { int fd0; int fd1; }`.
        let (fd0, fd1) = unsafe {
            (
                i32::from_le_bytes(reply.m_u.raw[0..4].try_into().unwrap()),
                i32::from_le_bytes(reply.m_u.raw[4..8].try_into().unwrap()),
            )
        };
        assert!(fd0 >= 0 && fd1 >= 0 && fd0 != fd1, "两个 fd 都建好了");
        let fp = state
            .fproc_table
            .get(minix_types::UserSlot::new(0))
            .unwrap();
        assert!(fp.filps[fd0 as usize].is_some() && fp.filps[fd1 as usize].is_some());
        assert!(state.worker_pool.get_mut(pfs_worker).unwrap().is_idle());
    }

    /// The **local half** of `Select` (C `do_select` select.c:94-260): `nfds > OPEN_MAX`
    /// → EINVAL; slot table full → ENOSPC; per-fd validation (bit unset → skip,
    /// invalid fd → EBADF, unknown type → EBADF); regular files and fds whose "mode
    /// bits do not match" are **ready immediately**; if there is nothing to wait for,
    /// return immediately (copying the result bitmap back to the user).
    ///
    /// Select's **wait half**: the full revival chain after a type-1 driver reply
    /// lands with the suspended site (`BlockedOn::Select` + slot + the filp's select
    /// accounting) — `reply1_step` records → `filp_status` broadcasts →
    /// `restart_proc` finishes. Under the host build the result-set copy-back is
    /// unreachable (store goes cross-space), so the user gets the EIO of the failed
    /// copy-back; the ready positive path is tested directly via `select_finish`'s
    /// injectable io.
    #[test]
    fn test_select_wait_half_reply1_resume() {
        use minix_types::Endpoint;

        let mut state = seeded(100);
        crate::main_loop::seed_ready_state(&mut state);
        let drv = Endpoint::from_generation_slot(0, 11);
        state.smap_table.entries[0].endpt = Some(drv);
        let row = state.smap_table.entries[0].num;
        let dev = crate::device_map::make_smap_dev(row, 0x42);
        let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
        state
            .fproc_table
            .get_mut(minix_types::UserSlot::new(0))
            .unwrap()
            .filps[3] = Some(fid.get());
        let vid = state.vnode_table.alloc().unwrap();
        {
            let v = state.vnode_table.get_mut(vid).unwrap();
            v.fs = Endpoint::MFS;
            v.ino = 0x20;
            v.mode = crate::open::S_IFSOCK | 0o777;
            v.sdev = dev;
            v.ref_count = 1;
        }
        state.filp_table.get_mut(fid).unwrap().vnode = Some(vid.get());
        // Hand-build the "suspended select" site (C `suspend(FP_BLOCKED_ON_SELECT)`;
        // the in-flight query is accounted by smap's `sel_busy`/`sel_owner` — a type-1
        // reply claims it through those).
        {
            let sp = state
                .smap_table
                .entries
                .iter_mut()
                .find(|r| r.num == row)
                .unwrap();
            sp.sel_busy = true;
            sp.sel_owner = Some(fid.get());
        }
        let slot_idx = state.select_table.alloc().unwrap();
        {
            let se = state.select_table.get_mut(slot_idx).unwrap();
            se.requestor = Some(minix_types::UserSlot::new(0));
            se.block = true;
            se.nfds = 8;
            se.readfds = vec![1 << 3];
            se.vir_readfds = 0x7000;
            se.ready_readfds = vec![0u8; 1];
            se.ready_writefds = vec![0u8; 1];
            se.ready_errorfds = vec![0u8; 1];
            // C: `se->filps[fd]` is indexed by fd — here fd 3.
            se.filps = vec![
                None,
                None,
                None,
                Some(crate::select::FdEntry {
                    filp: fid.get(),
                    kind: crate::select::FdKind::Sock,
                }),
            ];
        }
        {
            let f = state.filp_table.get_mut(fid).unwrap();
            f.select_dev = dev;
            f.selectors = 1; // C: filp_selectors++ initialized by do_select
        }
        {
            let fp = state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap();
            fp.blocked_on = crate::fproc::BlockedOn::Select;
        }

        // Type-1 driver reply: `SDEV_SELECT_REPLY` (sock_id@0, status@4 = SEL_RD).
        let mut reply = Message {
            m_type: minix_sockdriver::sdev::SdevReply::SelectReply1 as i32,
            ..Message::default()
        };
        reply.m_source = drv;
        // SAFETY(test): `mess_lsockdriver_vfs_select_reply` (ipc.h).
        unsafe {
            let raw = &mut reply.m_u.raw;
            raw[0..4].copy_from_slice(&0x42u32.to_le_bytes());
            raw[4..8].copy_from_slice(&(crate::select::SelOps::RD.to_status()).to_le_bytes());
        }
        state
            .handle_drv_reply(&reply)
            .expect("select 回复被 select 层认领");

        // Bookkeeping: the slot is released by the finish (nready=1 triggers
        // restart_proc → finish) and the suspended state is cleared; under the host
        // build the result-set copy-back is unreachable → the user gets -EIO.
        assert!(state.select_table.get(slot_idx).unwrap().is_free(), "收尾放槽");
        let fp = state
            .fproc_table
            .get(minix_types::UserSlot::new(0))
            .unwrap();
        assert_eq!(fp.blocked_on, crate::fproc::BlockedOn::None, "挂起态已清");
        let (target, m) = state.take_reply().expect("复活要回用户");
        assert_eq!(target, Endpoint::from_generation_slot(1, 0));
        assert_eq!(m.m_type, -minix_types::EIO, "宿主下 store 不可达 → 按错收尾");

        // Ready positive path, tested directly: rebuild by hand + select_finish with MemoryFdSetIo.
        let slot_idx = state.select_table.alloc().unwrap();
        {
            let se = state.select_table.get_mut(slot_idx).unwrap();
            se.requestor = Some(minix_types::UserSlot::new(0));
            se.block = true;
            se.nfds = 8;
            se.readfds = vec![1 << 3];
            se.vir_readfds = 0x7000;
            se.ready_readfds = vec![0u8; 1];
            se.ready_writefds = vec![0u8; 1];
            se.ready_errorfds = vec![0u8; 1];
            se.nready = 1;
            se.ready_readfds = vec![1 << 3];
            // C: `se->filps[fd]` is indexed by fd — here fd 3.
            se.filps = vec![
                None,
                None,
                None,
                Some(crate::select::FdEntry {
                    filp: fid.get(),
                    kind: crate::select::FdKind::Sock,
                }),
            ];
        }
        state.filp_table.get_mut(fid).unwrap().selectors = 1;
        let mut io = crate::select::MemoryFdSetIo {
            read: vec![0u8; 8],
            write: vec![0u8; 8],
            error: vec![0u8; 8],
            stored: core::cell::RefCell::new(Vec::new()),
        };
        assert_eq!(state.select_finish(slot_idx, &io), Ok(1), "就绪数回用户");
        let stored = io.stored.borrow();
        assert_eq!(stored[0].1[0] & (1 << 3), 1 << 3, "fd 3 的读位在结果集里");
    }

    /// **Type-2 (ready notification) revival** of select's wait half: the driver
    /// later reports readable → `reply2_hit` records (clearing the watch bit) →
    /// `restart_proc` finishes.
    #[test]
    fn test_select_wait_half_reply2_resume() {
        use minix_types::Endpoint;

        let mut state = seeded(100);
        crate::main_loop::seed_ready_state(&mut state);
        let drv = Endpoint::from_generation_slot(0, 11);
        state.dmap_table.get_mut(4).unwrap().driver = Some(drv);
        let dev = 0x0407u64; // major 4 / minor 7
        let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
        state
            .fproc_table
            .get_mut(minix_types::UserSlot::new(0))
            .unwrap()
            .filps[3] = Some(fid.get());
        let vid = state.vnode_table.alloc().unwrap();
        {
            let v = state.vnode_table.get_mut(vid).unwrap();
            v.fs = Endpoint::MFS;
            v.ino = 0x21;
            v.mode = crate::open::S_IFCHR | 0o600;
            v.sdev = dev;
            v.ref_count = 1;
        }
        state.filp_table.get_mut(fid).unwrap().vnode = Some(vid.get());
        let slot_idx = state.select_table.alloc().unwrap();
        {
            let se = state.select_table.get_mut(slot_idx).unwrap();
            se.requestor = Some(minix_types::UserSlot::new(0));
            se.block = true;
            se.nfds = 8;
            se.readfds = vec![1 << 3];
            se.vir_readfds = 0x7000;
            se.ready_readfds = vec![0u8; 1];
            se.ready_writefds = vec![0u8; 1];
            se.ready_errorfds = vec![0u8; 1];
            se.filps = vec![
                None,
                None,
                None,
                Some(crate::select::FdEntry {
                    filp: fid.get(),
                    kind: crate::select::FdKind::Char,
                }),
            ];
        }
        {
            let f = state.filp_table.get_mut(fid).unwrap();
            f.select_dev = dev;
            f.selectors = 1; // C: filp_selectors++ initialized by do_select
            // C: a filp on a blocking select carries the BLOCKED + RD_BLOCK watch.
            f.select_flags =
                (crate::filp::FsfFlags::RD_BLOCK | crate::filp::FsfFlags::BLOCKED).bits() as u8;
        }
        {
            let fp = state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap();
            fp.blocked_on = crate::fproc::BlockedOn::Select;
        }

        // Type-2 reply: CDEV_SEL2_REPLY (status@0 = SEL_RD, minor@4 = 7).
        let mut reply = Message {
            m_type: minix_chardriver::protocol::CdevReplyKind::SelectNotify as i32,
            ..Message::default()
        };
        reply.m_source = drv;
        // SAFETY(test): `mess_lchardriver_vfs_sel2 { int status@0; int32
        // minor@4 }` (ipc.h).
        unsafe {
            let raw = &mut reply.m_u.raw;
            raw[0..4].copy_from_slice(&(crate::select::SelOps::RD.to_status()).to_le_bytes());
            raw[4..8].copy_from_slice(&7u32.to_le_bytes());
        }
        state
            .handle_drv_reply(&reply)
            .expect("select 通知被 select 层认领");

        // Bookkeeping: the RD_BLOCK watch is cleared; the slot is released by the finish; the host copy-back fails → -EIO.
        let f = state.filp_table.get(fid).unwrap();
        assert!(
            !crate::filp::FsfFlags::from_bits_truncate(f.select_flags as u32)
                .contains(crate::filp::FsfFlags::RD_BLOCK),
            "可读通知要清 RD_BLOCK 监视"
        );
        assert!(state.select_table.get(slot_idx).unwrap().is_free());
        let (target, m) = state.take_reply().expect("复活要回用户");
        assert_eq!(target, Endpoint::from_generation_slot(1, 0));
        assert_eq!(m.m_type, -minix_types::EIO);
    }

    /// **Timeout revival** of select's wait half: `select_timeout_check` finishes at
    /// the deadline (all bookkeeping is assertable: slot released / suspended state
    /// cleared / user gets the reply / no longer fires once nothing is suspended).
    #[test]
    fn test_select_wait_half_timeout() {
        let mut state = seeded(100);
        crate::main_loop::seed_ready_state(&mut state);
        let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
        state
            .fproc_table
            .get_mut(minix_types::UserSlot::new(0))
            .unwrap()
            .filps[3] = Some(fid.get());
        let vid = state.vnode_table.alloc().unwrap();
        {
            let v = state.vnode_table.get_mut(vid).unwrap();
            v.fs = Endpoint::MFS;
            v.ino = 0x22;
            v.mode = crate::open::S_IFSOCK | 0o777;
            v.ref_count = 1;
        }
        state.filp_table.get_mut(fid).unwrap().vnode = Some(vid.get());
        let slot_idx = state.select_table.alloc().unwrap();
        {
            let se = state.select_table.get_mut(slot_idx).unwrap();
            se.requestor = Some(minix_types::UserSlot::new(0));
            se.block = true;
            se.nfds = 8;
            se.readfds = vec![1 << 3];
            se.expiry = 7;
            // C: `se->filps[fd]` is indexed by fd — here fd 3.
            se.filps = vec![
                None,
                None,
                None,
                Some(crate::select::FdEntry {
                    filp: fid.get(),
                    kind: crate::select::FdKind::Sock,
                }),
            ];
        }
        state.filp_table.get_mut(fid).unwrap().selectors = 1;
        {
            let fp = state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap();
            fp.blocked_on = crate::fproc::BlockedOn::Select;
        }
        assert!(state.select_timeout_check(), "有到期槽要收尾");
        assert!(state.select_table.get(slot_idx).unwrap().is_free());
        let fp = state
            .fproc_table
            .get(minix_types::UserSlot::new(0))
            .unwrap();
        assert_eq!(fp.blocked_on, crate::fproc::BlockedOn::None);
        let (target, m) = state.take_reply().expect("超时复活要回用户");
        assert_eq!(target, Endpoint::from_generation_slot(1, 0));
        assert_eq!(m.m_type, 0, "select 超时 = 就绪数 0（POSIX 语义；vir 指针为 0 无拷回）");
        // No select is suspended anymore: it no longer fires.
        assert!(!state.select_timeout_check());
    }

    /// The two halves of `put_vnode` (C `put_vnode` vnode.c:240-290): the fast path
    /// only decrements the reference count; the slow path queues `REQ_PUTNODE` into
    /// `pending_puts` (the fs/ino/fs_count triple comes from the vnode's
    /// `v_fs_e`/`v_inode_nr`/`v_fs_count`).
    #[test]
    fn test_put_vnode_deferred_fast_and_slow_paths() {
        let mut state = seeded(100);
        let vid = state.vnode_table.alloc().unwrap();
        {
            let v = state.vnode_table.get_mut(vid).unwrap();
            v.fs = Endpoint::MFS;
            v.ino = 0x33;
            v.mode = crate::open::S_IFREG | 0o600;
            v.ref_count = 2;
            v.fs_count = 2;
        }
        // Fast path: ref>1 → only decrement, queue empty.
        assert_eq!(state.put_vnode_deferred(vid), Ok(false));
        assert_eq!(state.vnode_table.get(vid).unwrap().ref_count, 1);
        assert!(state.pending_puts.is_empty());
        // Slow path: ref==1 → slot released + REQ_PUTNODE queued.
        assert_eq!(state.put_vnode_deferred(vid), Ok(true));
        assert_eq!(
            state.pending_puts.as_slice(),
            [crate::vnode::PutNodeReq { fs_e: Endpoint::MFS, ino: 0x33, count: 2 }]
        );
    }

    /// The last-close half of the Close arm (C `filedes.c:496-505`): the last close
    /// must put_vnode the filp's vnode (queued delivery); a non-last close does not send.
    #[test]
    fn test_dispatch_close_last_close_puts_vnode() {
        let mut state = seeded(100);
        crate::main_loop::seed_ready_state(&mut state);
        let slot = minix_types::UserSlot::new(0);
        let vid = state.vnode_table.alloc().unwrap();
        {
            let v = state.vnode_table.get_mut(vid).unwrap();
            v.fs = Endpoint::MFS;
            v.ino = 0x34;
            v.mode = crate::open::S_IFREG | 0o600;
            v.ref_count = 1;
            v.fs_count = 1;
        }
        let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
        state.filp_table.inc_count(fid);
        state.filp_table.get_mut(fid).unwrap().vnode = Some(vid.get());
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.filps[3] = Some(fid.get());
        }
        let fd = Fd::new(3).unwrap();
        state.current_fp_slot = Some(slot);
        state.current_message = Message {
            m_source: Endpoint::from_generation_slot(1, 0),
            m_type: VfsCallNum::Close as i32,
            m_u: MessageUnion {
                m_m7: MessageM7 { m7i1: 3, ..Default::default() },
            },
        };
        assert_eq!(dispatch_syscall(&mut state, VfsCallNum::Close), SyscallResult::Ok(0));
        assert_eq!(
            state.pending_puts.as_slice(),
            [crate::vnode::PutNodeReq { fs_e: Endpoint::MFS, ino: 0x34, count: 1 }],
            "last close 的 put_vnode 排队投递"
        );
    }

    /// Arm-level tests for the Fcntl lock family (C `lock_op`): SETLK takes a lock →
    /// another process's GETLK reports a conflict → another process's SETLKW suspends
    /// → the owner's unlock releases the table entry. flock goes in and out through the
    /// Memory `FlockIo` by calling `do_fcntl_lock` directly (dispatch's payload surface
    /// is covered by the existing gate tests).
    #[test]
    fn test_fcntl_lock_ops_end_to_end() {
        use crate::fcntl::MemoryFlockIo;
        use minix_types::Endpoint;

        let mk = |pid: i32| -> (VfsState, minix_types::UserSlot) {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            let slot = minix_types::UserSlot::new(0);
            {
                let fp = state.fproc_table.get_mut(slot).unwrap();
                fp.endpoint = Endpoint::from_generation_slot(1, 0);
                fp.pid = pid;
            }
            let vid = state.vnode_table.alloc().unwrap();
            {
                let v = state.vnode_table.get_mut(vid).unwrap();
                v.fs = Endpoint::MFS;
                v.ino = 0x40;
                v.mode = crate::open::S_IFREG | 0o600;
                v.ref_count = 1;
            }
            let fid = state
                .filp_table
                .alloc_filp(crate::open::R_BIT | crate::open::W_BIT)
                .unwrap();
            state.filp_table.inc_count(fid);
            state.filp_table.get_mut(fid).unwrap().vnode = Some(vid.get());
            state.fproc_table.get_mut(slot).unwrap().filps[3] = Some(fid.get());
            (state, slot)
        };
        let mut io = MemoryFlockIo::default();
        let call = |state: &mut VfsState,
                    slot: minix_types::UserSlot,
                    fd: usize,
                    cmd: crate::fcntl::FcntlCmd,
                    ty: i32,
                    pid: i32,
                    io: &MemoryFlockIo| {
            let mut src = [0u8; minix_types::FLOCK_SIZE];
            crate::fcntl::encode_flock(&mut src, 10, 20, pid, ty, 0);
            *io.source.borrow_mut() = src;
            do_fcntl_lock(state, slot, fd, cmd, 0x9000, io)
        };

        // 1) Process 100: F_SETLK write lock [10, 30) → enters the table.
        let (mut state, slot0) = mk(100);
        assert_eq!(
            call(&mut state, slot0, 3, crate::fcntl::FcntlCmd::SetLk, minix_types::F_WRLCK, 100, &io),
            SyscallResult::Ok(0)
        );
        assert_eq!(state.lock_table.nr, 1, "锁进表");

        // 2) Process 200 (another fd for the same vnode): F_GETLK → reports a conflict.
        let filp3 = state
            .fproc_table
            .get(minix_types::UserSlot::new(0))
            .unwrap()
            .filps[3];
        {
            let slot1 = minix_types::UserSlot::new(1);
            let fp = state.fproc_table.get_mut(slot1).unwrap();
            fp.endpoint = Endpoint::from_generation_slot(1, 1);
            fp.pid = 200;
            fp.filps[4] = filp3;
        }
        state.current_fp_slot = Some(minix_types::UserSlot::new(1));
        assert_eq!(
            call(&mut state, minix_types::UserSlot::new(1), 4, crate::fcntl::FcntlCmd::GetLk,
                 minix_types::F_RDLCK, 200, &io),
            SyscallResult::Ok(0),
            "GETLK 走 fd 4"
        );
        {
            let stored = io.stored.borrow();
            let (addr, last) = stored.last().expect("GETLK 要拷回");
            let (_s, len, pid, ty, _w) = crate::fcntl::decode_flock(last);
            assert_eq!(*addr, 0x9000);
            assert_eq!(ty, minix_types::F_WRLCK, "报告冲突者的类型");
            assert_eq!(pid, 100, "报告冲突者的 pid");
            assert_eq!(len, 20);
        }

        // 3) Process 200: F_SETLKW for a write lock on the same range → suspends (BlockedOn::Flock).
        {
            let idx = state
                .worker_pool
                .assign_first_fit(
                    minix_types::UserSlot::new(1),
                    crate::worker::WorkerFunc::DoWork,
                    &Message::default(),
                )
                .unwrap();
            state.current_worker = Some(idx);
            assert_eq!(
                call(&mut state, minix_types::UserSlot::new(1), 4, crate::fcntl::FcntlCmd::SetLkw,
                     minix_types::F_WRLCK, 200, &io),
                SyscallResult::Suspend
            );
            let fp = state.fproc_table.get(minix_types::UserSlot::new(1)).unwrap();
            assert!(matches!(
                fp.blocked_on,
                crate::fproc::BlockedOn::Flock(crate::fproc::FlockBlock {
                    cmd: crate::fproc::FlockCmd::SetLkw,
                    ..
                })
            ));
        }

        // 4) Process 100: F_SETLK unlock on the same range → the table is emptied.
        state.current_fp_slot = Some(minix_types::UserSlot::new(0));
        assert_eq!(
            call(&mut state, minix_types::UserSlot::new(0), 3, crate::fcntl::FcntlCmd::SetLk,
                 minix_types::F_UNLCK, 100, &io),
            SyscallResult::Ok(0)
        );
        assert_eq!(state.lock_table.nr, 0, "解锁放表");
    }

    /// The `F_FREESP` arm: non-zero length → `REQ_FTRUNC(start, end)`; zero length
    /// → `REQ_FTRUNC(start, 0)` and `v_size = start` after the reply lands.
    #[test]
    fn test_fcntl_freesp_arm_and_zero_len_resume() {
        use minix_types::Endpoint;

        let mk = |v_size: u64| -> (VfsState, minix_types::UserSlot, usize, usize) {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            let slot = minix_types::UserSlot::new(0);
            let vid = state.vnode_table.alloc().unwrap();
            {
                let v = state.vnode_table.get_mut(vid).unwrap();
                v.fs = Endpoint::MFS;
                v.ino = 0x50;
                v.mode = crate::open::S_IFREG | 0o600;
                v.size = v_size;
                v.ref_count = 1;
            }
            let fid = state
                .filp_table
                .alloc_filp(crate::open::R_BIT | crate::open::W_BIT)
                .unwrap();
            state.filp_table.inc_count(fid);
            state.filp_table.get_mut(fid).unwrap().vnode = Some(vid.get());
            state.fproc_table.get_mut(slot).unwrap().filps[3] = Some(fid.get());
            let idx = state
                .worker_pool
                .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
                .unwrap();
            state.current_worker = Some(idx);
            (state, slot, vid.get(), idx)
        };
        let mut io = crate::fcntl::MemoryFlockIo::default();
        {
            let mut src = [0u8; minix_types::FLOCK_SIZE];
            crate::fcntl::encode_flock(&mut src, 5, 10, 0, minix_types::F_WRLCK, 0);
            *io.source.borrow_mut() = src;
        }

        // 1) Non-zero length: start = 5 (SEEK_SET), end = 15.
        let (mut state, slot, vid, idx) = mk(100);
        assert_eq!(
            do_fcntl_freesp(&mut state, slot, 3, 0x9000, &io),
            SyscallResult::Suspend
        );
        let p = state.pending_fs.expect("FTRUNC 对话挂起");
        assert_eq!(p.req.m_type, minix_types::REQ_FTRUNC);
        // SAFETY(test): read back using ftrunc_req_off (the inode is in the first slot).
        unsafe {
            let raw = &p.req.m_u.raw;
            assert_eq!(
                i64::from_le_bytes(
                    raw[minix_types::ftrunc_req_off::TRC_START
                        ..minix_types::ftrunc_req_off::TRC_START + 8]
                        .try_into()
                        .unwrap()
                ),
                5,
                "trc_start"
            );
            assert_eq!(
                i64::from_le_bytes(
                    raw[minix_types::ftrunc_req_off::TRC_END
                        ..minix_types::ftrunc_req_off::TRC_END + 8]
                        .try_into()
                        .unwrap()
                ),
                15,
                "trc_end"
            );
        }
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(crate::worker::WorkerCont::Freesp {
                vnode: vid,
                zero_len: false,
                start: 5,
            });
            wp.sendrec = Some(Message { m_type: minix_types::OK, ..Message::default() });
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.vnode_table.get(crate::vnode::VnodeId(vid)).unwrap().size,
            100,
            "非零长不动 size"
        );

        // 2) Zero length: truncate to start; after the reply size = start.
        let (mut state, slot, vid, idx) = mk(100);
        {
            let mut src = [0u8; minix_types::FLOCK_SIZE];
            crate::fcntl::encode_flock(&mut src, 40, 0, 0, minix_types::F_WRLCK, 0);
            *io.source.borrow_mut() = src;
        }
        assert_eq!(
            do_fcntl_freesp(&mut state, slot, 3, 0x9000, &io),
            SyscallResult::Suspend
        );
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(crate::worker::WorkerCont::Freesp {
                vnode: vid,
                zero_len: true,
                start: 40,
            });
            wp.sendrec = Some(Message { m_type: minix_types::OK, ..Message::default() });
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.vnode_table.get(crate::vnode::VnodeId(vid)).unwrap().size,
            40,
            "零长截断把 size 收到 start"
        );
    }

    /// The dead-driver classification surface of `send_drv_for_slot`: the host
    /// transport is always `EIO` (the `Fatal` class) → it does **not** unmap (no
    /// collateral damage to a live table); only the `Dead` class (202/215, real
    /// kernel status) unmaps — the classification criterion is pinned by the
    /// `bdev::classify_send` pinned-value test.
    #[test]
    fn test_send_drv_fatal_class_keeps_dmap() {
        let mut state = VfsState::new();
        let drv = Endpoint::from_generation_slot(0, 11);
        state.dmap_table.get_mut(4).unwrap().driver = Some(drv);
        let idx = state
            .worker_pool
            .assign_first_fit(
                minix_types::UserSlot::new(0),
                crate::worker::WorkerFunc::DoWork,
                &Message::default(),
            )
            .unwrap();
        let r = state.send_drv_for_slot(
            idx,
            None,
            drv,
            &Message { m_type: 0x406, ..Message::default() },
        );
        assert_eq!(r, Err(minix_types::EIO), "宿主传输失败折 EIO");
        assert!(
            state.dmap_table.get(4).unwrap().driver.is_some(),
            "Fatal 类（宿主 EIO）不解映射——只有真机的 Dead 类才动表"
        );
    }

    /// The failure half of `ds_fill_label`: when DS is unreachable (the host trap
    /// always fails) it returns `None` and **does not pollute** `driver_labels` —
    /// the local table stays as it was, and the fail-closed chain (empty table →
    /// EINVAL) is unchanged.
    #[test]
    fn test_ds_fill_label_failure_keeps_table_clean() {
        let mut state = VfsState::new();
        state
            .driver_labels
            .push(("keep".to_string(), Endpoint::from_generation_slot(0, 9)));
        assert_eq!(state.ds_fill_label("ghost"), None);
        assert_eq!(state.driver_labels.len(), 1, "失败的查询不写表");
        assert_eq!(state.driver_labels[0].0, "keep");
    }

    /// `flush_pending_puts`: each queue entry is sent as `REQ_PUTNODE` (recording
    /// transport); entries without a mount row are dropped.
    #[test]
    fn test_flush_pending_puts_sends_and_drops() {
        struct RecordingIpc {
            sent: core::cell::RefCell<alloc::vec::Vec<(i32, i32)>>,
        }
        impl minix_sys::ipc::IpcTransport for RecordingIpc {
            fn send(&self, _d: Endpoint, _m: &Message) -> Result<(), minix_sys::ipc::TrapStatus> {
                Err(minix_sys::ipc::TrapStatus(minix_types::EIO))
            }
            fn receive(
                &self,
                _s: Endpoint,
                _m: &mut Message,
            ) -> Result<minix_sys::ipc::IpcStatus, minix_sys::ipc::TrapStatus> {
                Err(minix_sys::ipc::TrapStatus(minix_types::EIO))
            }
            fn sendrec(
                &self,
                d: Endpoint,
                m: &mut Message,
            ) -> Result<(), minix_sys::ipc::TrapStatus> {
                self.sent.borrow_mut().push((d.0, m.m_type));
                Ok(())
            }
            fn notify(&self, _d: Endpoint) -> Result<(), minix_sys::ipc::TrapStatus> {
                Err(minix_sys::ipc::TrapStatus(minix_types::EIO))
            }
            fn sendnb(&self, _d: Endpoint, _m: &Message) -> Result<(), minix_sys::ipc::TrapStatus> {
                Err(minix_sys::ipc::TrapStatus(minix_types::EIO))
            }
            fn senda(&self, _t: &[minix_sys::ipc::AsyncSlot]) -> Result<(), minix_sys::ipc::TrapStatus> {
                Err(minix_sys::ipc::TrapStatus(minix_types::EIO))
            }
            fn query_kerninfo_page(&self) -> Result<u64, minix_sys::ipc::TrapStatus> {
                Err(minix_sys::ipc::TrapStatus(minix_types::EIO))
            }
        }
        let mut state = seeded(100);
        crate::main_loop::seed_ready_state(&mut state);
        state.pending_puts = alloc::vec![
            crate::vnode::PutNodeReq { fs_e: Endpoint::MFS, ino: 7, count: 2 },
            crate::vnode::PutNodeReq { fs_e: Endpoint::NONE, ino: 9, count: 1 },
        ];
        let ipc = RecordingIpc { sent: core::cell::RefCell::new(alloc::vec::Vec::new()) };
        state.flush_pending_puts(&ipc);
        assert_eq!(
            ipc.sent.borrow().as_slice(),
            [(Endpoint::MFS.0, minix_types::REQ_PUTNODE)],
            "只有挂在表上的 FS 收到 REQ_PUTNODE"
        );
        assert!(state.pending_puts.is_empty(), "flush 后队列清空");
    }

    /// Test adapter: `do_select` now returns `SyscallResult` directly (the
    /// suspension semantics cannot fold into Ok/Err); the old assertions stay
    /// readable through this conversion.
    fn do_select_res(
        state: &mut VfsState,
        fp_slot: minix_types::UserSlot,
        nfds: usize,
        r: u64,
        w: u64,
        e: u64,
        t: u64,
        io: &impl crate::select::FdSetIo,
    ) -> Result<i32, i32> {
        use crate::call_table::SyscallResult;
        match do_select(state, fp_slot, nfds, r, w, e, t, io) {
            SyscallResult::Ok(v) => Ok(v),
            SyscallResult::Error(e) => Err(e),
            SyscallResult::Suspend => Err(-1),
            SyscallResult::Nosys => Err(minix_types::ENOSYS),
        }
    }

    #[test]
    fn test_dispatch_select_local_half() {
        use crate::select::MemoryFdSetIo;
        use minix_types::Endpoint;

        let setup = |mode: u32, filp_mode: u32| {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            let fid = state.filp_table.alloc_filp(filp_mode).unwrap();
            state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap()
                .filps[3] = Some(fid.get());
            let vid = state.vnode_table.alloc().unwrap();
            {
                let v = state.vnode_table.get_mut(vid).unwrap();
                v.fs = Endpoint::MFS;
                v.ino = 0x11;
                v.mode = mode;
                v.ref_count = 1;
            }
            state.filp_table.get_mut(fid).unwrap().vnode = Some(vid.get());
            state
        };
        let io_with = |bits: &[u8]| {
            let mut set = [0u8; 8];
            set[..bits.len()].copy_from_slice(bits);
            MemoryFdSetIo {
                read: set.to_vec(),
                write: vec![0u8; 8],
                error: vec![0u8; 8],
                stored: core::cell::RefCell::new(Vec::new()),
            }
        };

        // 1) `nfds > OPEN_MAX` → EINVAL.
        let mut state = setup(crate::open::S_IFREG | 0o644, crate::open::R_BIT);
        let io = io_with(&[]);
        assert_eq!(
            do_select_res(
                &mut state,
                minix_types::UserSlot::new(0),
                crate::fproc::OPEN_MAX + 1,
                0,
                0,
                0,
                0,
                &io
            ),
            Err(minix_types::EINVAL)
        );

        // 2) readfds names an **unopened fd** (fd 5) → EBADF.
        let mut state = setup(crate::open::S_IFREG | 0o644, crate::open::R_BIT);
        let io = io_with(&[1 << 5]);
        assert_eq!(
            do_select_res(
                &mut state,
                minix_types::UserSlot::new(0),
                8,
                1,
                0,
                0,
                0,
                &io
            ),
            Err(minix_types::EBADF)
        );

        // 3) Regular file + read bit → **ready immediately**: returns 1, and the
        // result bitmap is copied back to the user (fd 3's read bit is set).
        let mut state = setup(crate::open::S_IFREG | 0o644, crate::open::R_BIT);
        let io = io_with(&[1 << 3]);
        let r = do_select_res(
            &mut state,
            minix_types::UserSlot::new(0),
            8,
            1,
            0,
            0,
            0,
            &io,
        );
        assert_eq!(r, Ok(1), "常规文件读方向永远就绪");
        let stored = io.stored.borrow();
        assert_eq!(stored.len(), 1, "结果只拷回给了 readfds");
        assert_eq!(stored[0].0, 1, "拷回的是用户给的 readfds 地址");
        assert_eq!(stored[0].1[0] & (1 << 3), 1 << 3, "fd 3 的读位置上了");

        // 4) Empty pipe + read bit + no timeout: probe one byte → WouldBlock → not
        // ready → **blocking suspension** (C `suspend(FP_BLOCKED_ON_SELECT)`; the
        // adapter folds Suspend into Err(-1)): the slot is taken, the filp's pipe
        // interest is parked, and the suspended state is set. The data-arrival half
        // belongs to the wakeup surface of 17-pipe.md.
        let mut state = setup(crate::open::S_IFIFO | 0o600, crate::open::R_BIT);
        let io = io_with(&[1 << 3]);
        assert_eq!(
            do_select_res(
                &mut state,
                minix_types::UserSlot::new(0),
                8,
                1,
                0,
                0,
                0,
                &io
            ),
            Err(-1),
            "阻塞 select 无就绪 = 挂起（Suspend）"
        );
        assert!(
            state.select_table.slots.iter().any(|s| !s.is_free()),
            "挂起的 select 占着槽"
        );
        assert_eq!(
            state.fproc_table.get(minix_types::UserSlot::new(0)).unwrap().blocked_on,
            crate::fproc::BlockedOn::Select,
            "进程级挂起（FP_BLOCKED_ON_SELECT）"
        );
        let f = state.filp_table.get(
            crate::filp::FilpId(
                state.fproc_table.get(minix_types::UserSlot::new(0)).unwrap().filps[3].unwrap(),
            ),
        ).unwrap();
        assert!(
            crate::select::SelOps::from_bits_truncate(f.pipe_select_ops)
                .contains(crate::select::SelOps::RD),
            "阻塞且无就绪：管道兴趣停靠（filp_pipe_select_ops）"
        );

        // 5) Slot table full → ENOSPC (occupy all 25 slots).
        let mut state = setup(crate::open::S_IFREG | 0o644, crate::open::R_BIT);
        for s in state.select_table.slots.iter_mut() {
            s.requestor = Some(minix_types::UserSlot::new(1));
        }
        let io = io_with(&[1 << 3]);
        assert_eq!(
            do_select_res(
                &mut state,
                minix_types::UserSlot::new(0),
                8,
                1,
                0,
                0,
                0,
                &io
            ),
            Err(minix_types::ENOSPC)
        );
    }

    /// `Sendmsg`/`Recvmsg`: gates same as the socket family; fetch the user
    /// `struct msghdr` (the production path copies across spaces, unreachable on
    /// the host → EINVAL). The gates and request shape after fetching are tested
    /// through `do_sockmsg` with a **scripted stand-in**: multi-element iov →
    /// `EMSGSIZE`, `iov_len > SSIZE_MAX` → `EINVAL`, single element → the three
    /// grants (data/control/address).
    #[test]
    fn test_sendmsg_recvmsg_iov_gates_and_shape() {
        use crate::socket::{IoVec, MsgHdr, ScriptedMsgHdr};
        use minix_types::Endpoint;

        let setup = || {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            let drv = Endpoint::from_generation_slot(0, 11);
            state.smap_table.entries[0].endpt = Some(drv);
            let dev = crate::device_map::make_smap_dev(state.smap_table.entries[0].num, 0x42);
            let fid = state.filp_table.alloc_filp(crate::open::R_BIT | crate::open::W_BIT).unwrap();
            state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap()
                .filps[3] = Some(fid.get());
            let vid = state.vnode_table.alloc().unwrap();
            {
                let v = state.vnode_table.get_mut(vid).unwrap();
                v.fs = Endpoint::PFS;
                v.ino = 0x11;
                v.mode = crate::open::S_IFSOCK | 0o777;
                v.sdev = dev;
                v.ref_count = 1;
            }
            state.filp_table.get_mut(fid).unwrap().vnode = Some(vid.get());
            let idx = state
                .worker_pool
                .assign_first_fit(
                    minix_types::UserSlot::new(0),
                    crate::worker::WorkerFunc::DoWork,
                    &Message::default(),
                )
                .expect("空闲槽");
            state.current_worker = Some(idx);
            (state, idx)
        };
        let sockmsg_msg = |call: VfsCallNum, fd: i32| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: call as i32,
                ..Message::default()
            };
            // SAFETY: mess_lc_vfs_sockmsg: fd@0, msgbuf@8, flags@16.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[8..16].copy_from_slice(&0x6000u64.to_le_bytes());
            }
            m
        };

        // 1) Gates: negative fd → EBADF; under the host build the msghdr is unreachable → EINVAL.
        let (mut state, _idx) = setup();
        state.current_message = sockmsg_msg(VfsCallNum::Sendmsg, -1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Sendmsg),
            SyscallResult::Error(minix_types::EBADF)
        );
        state.current_message = sockmsg_msg(VfsCallNum::Sendmsg, 3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Sendmsg),
            SyscallResult::Error(minix_types::EINVAL),
            "宿主下取不到用户的 msghdr"
        );

        // 2) Scripted stand-in through `do_sockmsg`: multi-element vector → EMSGSIZE.
        let (mut state, idx) = setup();
        let scripted = ScriptedMsgHdr {
            msghdr: MsgHdr {
                name: 0x7000,
                namelen: 16,
                iov: 0x8000,
                iovlen: 2,
                control: 0,
                controllen: 0,
                flags: 0,
            },
            iovec: IoVec { base: 0x9000, len: 8 },
        };
        let dev = crate::device_map::make_smap_dev(state.smap_table.entries[0].num, 0x42);
        let e = do_sockmsg(
            &mut state,
            idx,
            Some(minix_types::UserSlot::new(0)),
            dev,
            0,
            0x6000,
            0,
            false,
            &scripted,
        );
        assert_eq!(e, Err(minix_types::EMSGSIZE), "多元素向量不支持");

        // 3) Single element but `iov_len > SSIZE_MAX` → EINVAL.
        let (mut state, idx) = setup();
        let scripted = ScriptedMsgHdr {
            msghdr: MsgHdr {
                iovlen: 1,
                ..scripted.msghdr
            },
            iovec: IoVec {
                base: 0x9000,
                len: u64::MAX,
            },
        };
        let dev = crate::device_map::make_smap_dev(state.smap_table.entries[0].num, 0x42);
        let e = do_sockmsg(
            &mut state,
            idx,
            Some(minix_types::UserSlot::new(0)),
            dev,
            0,
            0x6000,
            0,
            false,
            &scripted,
        );
        assert_eq!(e, Err(minix_types::EINVAL), "iov_len 越界");

        // 4) `iovlen == 0` (empty data): legal, still sends a request to the driver
        // (sending fails on the host → EIO), and builds **no data grant** (C builds
        // no grant for a zero buffer).
        let (mut state, idx) = setup();
        let scripted = ScriptedMsgHdr {
            msghdr: MsgHdr {
                iovlen: 0,
                ..scripted.msghdr
            },
            iovec: IoVec::default(),
        };
        let dev = crate::device_map::make_smap_dev(state.smap_table.entries[0].num, 0x42);
        let e = do_sockmsg(
            &mut state,
            idx,
            Some(minix_types::UserSlot::new(0)),
            dev,
            0,
            0x6000,
            0,
            true,
            &scripted,
        );
        assert_eq!(e, Err(minix_types::EIO), "宿主下发送失败");
        assert!(matches!(
            state
                .fproc_table
                .get(minix_types::UserSlot::new(0))
                .unwrap()
                .blocked_on,
            crate::fproc::BlockedOn::None
        ));
    }

    /// `Accept`: gates same as the socket family + `check_sock_fds(1)` (the new
    /// socket needs an fd slot) → send `SDEV_ACCEPT` to the driver (the address
    /// grant is `CPF_WRITE` — the driver writes the peer address back) →
    /// process-level suspension (the site carries the listening fd).
    ///
    /// Three finish states (C `resume_accept` socket.c:367-465): 1) failure and no
    /// socket created → return only the error; 2) failure but the driver already
    /// created a socket → close it, then return the error; 3) success → the site
    /// **opens another worker** to run `make_sock_fd` (PFS creates the node +
    /// assembles the fd), replying with the fd + the peer address length.
    #[test]
    fn test_dispatch_accept_and_three_state_resume() {
        use minix_types::Endpoint;

        let setup = || {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            let drv = Endpoint::from_generation_slot(0, 11);
            state.smap_table.entries[0].endpt = Some(drv);
            let dev = crate::device_map::make_smap_dev(state.smap_table.entries[0].num, 0x42);
            // PFS mount row (the finish needs `REQ_NEWNODE`).
            let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(1)).unwrap();
            v.fs = Endpoint::PFS;
            v.dev = 9;
            // Listening socket: fd 3.
            let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
            state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap()
                .filps[3] = Some(fid.get());
            let vid = state.vnode_table.alloc().unwrap();
            {
                let v = state.vnode_table.get_mut(vid).unwrap();
                v.fs = Endpoint::PFS;
                v.ino = 0x11;
                v.mode = crate::open::S_IFSOCK | 0o777;
                v.sdev = dev;
                v.ref_count = 1;
            }
            state.filp_table.get_mut(fid).unwrap().vnode = Some(vid.get());
            // The listening socket carries `O_NONBLOCK` (to be inherited by the new socket).
            state.filp_table.get_mut(fid).unwrap().flags = crate::fcntl::O_NONBLOCK as i32;
            let idx = state
                .worker_pool
                .assign_first_fit(
                    minix_types::UserSlot::new(0),
                    crate::worker::WorkerFunc::DoWork,
                    &Message::default(),
                )
                .expect("空闲槽");
            state.current_worker = Some(idx);
            (state, idx, drv, dev)
        };
        let accept_msg = |fd: i32| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Accept as i32,
                ..Message::default()
            };
            // SAFETY: mess_lc_vfs_sockaddr: fd@0, addr@8, addr_len@16.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[8..16].copy_from_slice(&0x6000u64.to_le_bytes());
                raw[16..24].copy_from_slice(&16u64.to_le_bytes());
            }
            m
        };
        let accept_reply = |drv: Endpoint, sock_id: i32, status: i32, addr_len: u32| {
            let mut r = Message {
                m_type: minix_sockdriver::sdev::SdevReply::AcceptReply as i32,
                ..Message::default()
            };
            r.m_source = drv;
            // SAFETY(test): `mess_lsockdriver_vfs_accept_reply { req_id@0;
            // sock_id@4; status@8; len@12 }`.
            unsafe {
                let raw = &mut r.m_u.raw;
                raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
                raw[8..12].copy_from_slice(&status.to_le_bytes());
                raw[12..16].copy_from_slice(&addr_len.to_le_bytes());
            }
            r
        };

        // 1) Gates: negative fd → EBADF; non-socket → ENOTSOCK.
        let (mut state, _idx, _drv, _dev) = setup();
        state.current_message = accept_msg(-1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Accept),
            SyscallResult::Error(minix_types::EBADF)
        );
        // Under the host build the trap is unreachable → send fails → EIO, and no suspension (the listening fd is fine).
        state.current_message = accept_msg(3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Accept),
            SyscallResult::Error(minix_types::EIO)
        );

        // 2) Finish case 1: failure and **no socket created** (sock_id < 0) → return only the error.
        let (mut state, _idx, drv, dev) = setup();
        {
            let fp = state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap();
            fp.blocked_on = crate::fproc::BlockedOn::Sdev(crate::fproc::SdevBlock {
                dev,
                call: crate::fproc::SdevCall::Accept,
                grants: [None, None, None],
                aux: crate::fproc::SdevAux::Fd(3),
            });
        }
        assert!(state.finish_sdev_blocked(&accept_reply(drv, -1, -minix_types::EAGAIN, 0)));
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((Endpoint::from_generation_slot(1, 0), -minix_types::EAGAIN)),
            "没建套接字就只回错误"
        );

        // 3) Finish case 3: success (sock_id ≥ 0) → the site opens a worker to run
        // make_sock_fd: first send PFS `REQ_NEWNODE` (device number = make_smap_dev(row, sock_id)).
        let (mut state, _idx, drv, dev) = setup();
        {
            let fp = state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap();
            fp.blocked_on = crate::fproc::BlockedOn::Sdev(crate::fproc::SdevBlock {
                dev,
                call: crate::fproc::SdevCall::Accept,
                grants: [None, None, None],
                aux: crate::fproc::SdevAux::Fd(3),
            });
        }
        assert!(state.finish_sdev_blocked(&accept_reply(drv, 0x77, 0, 16)));
        let p = state.pending_fs.as_ref().expect("收尾里给 PFS 发了 REQ_NEWNODE");
        assert_eq!(p.req.m_type, minix_types::REQ_NEWNODE);
        assert_eq!(p.fs_e, Endpoint::PFS);
        // SAFETY(test): `mess_vfs_fs_newnode`: device@0, mode@8.
        unsafe {
            let raw = &p.req.m_u.raw;
            let new_dev = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let mode = u32::from_le_bytes(raw[8..12].try_into().unwrap());
            let row = state.smap_table.entries[0].num;
            assert_eq!(new_dev, crate::device_map::make_smap_dev(row, 0x77));
            assert_eq!(mode & crate::open::S_IFMT, crate::open::S_IFSOCK);
        }
        // The reply arrives → return the fd + **peer address length** (`m_vfs_lc_socklen`).
        let pfs_worker = p.worker;
        state.pending_fs = None;
        let mut reply = Message { m_type: 0, ..Message::default() };
        // SAFETY(test): the new node details (same order as the first six fields of lookup_reply_off).
        unsafe {
            let raw = &mut reply.m_u.raw;
            raw[minix_types::lookup_reply_off::INODE..minix_types::lookup_reply_off::INODE + 8]
                .copy_from_slice(&0x88u64.to_le_bytes());
            raw[minix_types::lookup_reply_off::MODE..minix_types::lookup_reply_off::MODE + 4]
                .copy_from_slice(&(crate::open::S_IFSOCK | 0o777).to_le_bytes());
        }
        {
            let wp = state.worker_pool.get_mut(pfs_worker).unwrap();
            wp.sendrec = Some(reply);
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        let (target, reply) = state.take_reply().expect("回复");
        assert_eq!(target, Endpoint::from_generation_slot(1, 0));
        let fd = reply.m_type;
        assert!(fd >= 0, "成功回新套接字的 fd（这里是 {fd}）");
        // SAFETY(test): `mess_vfs_lc_socklen { unsigned int len; }`.
        let len = unsafe { u32::from_le_bytes(reply.m_u.raw[0..4].try_into().unwrap()) };
        assert_eq!(len, 16, "对端地址长度进回复载荷");
        // The new fd holds a socket vnode and **inherited the listening socket's O_NONBLOCK**.
        let fp = state
            .fproc_table
            .get(minix_types::UserSlot::new(0))
            .unwrap();
        let new_filp = fp.filps[fd as usize].expect("新 fd 有 filp");
        let f = state.filp_table.get(crate::filp::FilpId(new_filp)).unwrap();
        assert_eq!(
            f.flags as u32 & crate::fcntl::O_NONBLOCK,
            crate::fcntl::O_NONBLOCK,
            "打开标志按 C 的 `flags &= O_CLOEXEC|O_NONBLOCK|O_NOSIGPIPE` 继承"
        );
    }

    /// `Sendto`/`Recvfrom` (two grants: data + address; process-level suspension):
    /// gates same as the socket family; the send direction's data grant is
    /// `CPF_READ` (the driver reads the user buffer), the receive direction's is
    /// `CPF_WRITE`; open flags (non-blocking / NOSIGPIPE in the write direction)
    /// become message flags OR-ed with the user's flags (C `sdev_readwrite:396-400`).
    ///
    /// Two finish states: the send direction uses `SDEV_REPLY` (the status is the
    /// result), the receive direction uses `SDEV_RECV_REPLY` (which must also put
    /// **addr_len into the reply payload**).
    #[test]
    fn test_dispatch_sendto_recvfrom_and_reply_shapes() {
        use minix_types::Endpoint;

        let setup = || {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            let drv = Endpoint::from_generation_slot(0, 11);
            state.smap_table.entries[0].endpt = Some(drv);
            let dev = crate::device_map::make_smap_dev(state.smap_table.entries[0].num, 0x42);
            let fid = state.filp_table.alloc_filp(crate::open::R_BIT | crate::open::W_BIT).unwrap();
            state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap()
                .filps[3] = Some(fid.get());
            let vid = state.vnode_table.alloc().unwrap();
            {
                let v = state.vnode_table.get_mut(vid).unwrap();
                v.fs = Endpoint::PFS;
                v.ino = 0x11;
                v.mode = crate::open::S_IFSOCK | 0o777;
                v.sdev = dev;
                v.ref_count = 1;
            }
            state.filp_table.get_mut(fid).unwrap().vnode = Some(vid.get());
            let idx = state
                .worker_pool
                .assign_first_fit(
                    minix_types::UserSlot::new(0),
                    crate::worker::WorkerFunc::DoWork,
                    &Message::default(),
                )
                .expect("空闲槽");
            state.current_worker = Some(idx);
            (state, idx, drv, dev)
        };
        let sendrecv_msg = |call: VfsCallNum, fd: i32| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: call as i32,
                ..Message::default()
            };
            // SAFETY: mess_lc_vfs_sendrecv: fd@0, buf@8, len@16, flags@24,
            // addr@32, addr_len@40.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[8..16].copy_from_slice(&0x6000u64.to_le_bytes());
                raw[16..24].copy_from_slice(&64u64.to_le_bytes());
                raw[32..40].copy_from_slice(&0x7000u64.to_le_bytes());
                raw[40..44].copy_from_slice(&16u32.to_le_bytes());
            }
            m
        };

        // 1) Gates: negative fd → EBADF.
        let (mut state, _idx, _drv, _dev) = setup();
        state.current_message = sendrecv_msg(VfsCallNum::Sendto, -1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Sendto),
            SyscallResult::Error(minix_types::EBADF)
        );

        // 2) Under the host build the trap is unreachable → send fails → EIO, no suspension, no dangling grant.
        let (mut state, _idx, _drv, _dev) = setup();
        state.current_message = sendrecv_msg(VfsCallNum::Sendto, 3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Sendto),
            SyscallResult::Error(minix_types::EIO)
        );
        assert!(matches!(
            state
                .fproc_table
                .get(minix_types::UserSlot::new(0))
                .unwrap()
                .blocked_on,
            crate::fproc::BlockedOn::None
        ));

        // 3) Hand-build the "recvfrom suspended" site and feed `SDEV_RECV_REPLY`:
        // the user gets the byte count (status), and **addr_len goes into the reply payload**.
        let (mut state, _idx, drv, dev) = setup();
        let data_grant = state
            .grant_user_buffer(
                drv,
                Endpoint::from_generation_slot(1, 0),
                0x6000,
                64,
                minix_types::CpFlags::WRITE,
            )
            .expect("grant 表已热身");
        {
            let fp = state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap();
            fp.blocked_on = crate::fproc::BlockedOn::Sdev(crate::fproc::SdevBlock {
                dev,
                call: crate::fproc::SdevCall::Recvfrom,
                grants: [Some(data_grant), None, None],
                aux: crate::fproc::SdevAux::None,
            });
        }
        let mut reply = Message {
            m_type: minix_sockdriver::sdev::SdevReply::ReceiveReply as i32,
            ..Message::default()
        };
        reply.m_source = drv;
        // SAFETY(test): `mess_lsockdriver_vfs_recv_reply { int32_t req_id@0;
        // int status@4; unsigned int ctl_len@8; unsigned int addr_len@12;
        // int flags@16 }`.
        unsafe {
            let raw = &mut reply.m_u.raw;
            raw[4..8].copy_from_slice(&12i32.to_le_bytes()); // received 12 bytes
            raw[12..16].copy_from_slice(&16u32.to_le_bytes()); // addr_len
        }
        assert!(state.finish_sdev_blocked(&reply), "认领这条回复");
        let (target, reply) = state.take_reply().expect("回复");
        assert_eq!(target, Endpoint::from_generation_slot(1, 0));
        assert_eq!(reply.m_type, 12, "用户拿到字节数");
        // SAFETY(test): `mess_vfs_lc_socklen { unsigned int len; }`.
        let len = unsafe { u32::from_le_bytes(reply.m_u.raw[0..4].try_into().unwrap()) };
        assert_eq!(len, 16, "addr_len 进回复载荷");
    }

    /// `finish_sdev_blocked`: a driver reply wakes a suspended bind/connect —
    /// revoke the grant left behind, clear the suspended state, and return the
    /// **status in the payload** to the user (C `sdev_reply`'s second branch +
    /// `sdev_finish`'s bind/connect group).
    #[test]
    fn test_finish_sdev_blocked_resumes_process() {
        use minix_types::Endpoint;

        let mut state = seeded(100);
        crate::main_loop::seed_ready_state(&mut state);
        let drv = Endpoint::from_generation_slot(0, 11);
        state.smap_table.entries[0].endpt = Some(drv);
        let dev = crate::device_map::make_smap_dev(state.smap_table.entries[0].num, 0x42);
        // Hand-build the "already suspended" site: grant + SdevBlock.
        let grant = state
            .grant_user_buffer(
                drv,
                Endpoint::from_generation_slot(1, 0),
                0x6000,
                16,
                minix_types::CpFlags::READ,
            )
            .expect("宿主下 grant 表已热身");
        {
            let fp = state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap();
            fp.blocked_on = crate::fproc::BlockedOn::Sdev(crate::fproc::SdevBlock {
                dev,
                call: crate::fproc::SdevCall::Bind,
                grants: [Some(grant), None, None],
                aux: crate::fproc::SdevAux::None,
            });
        }

        // Driver reply: `SDEV_REPLY` + payload status = -EADDRINUSE.
        let mut reply = Message {
            m_type: minix_sockdriver::sdev::SdevReply::Reply as i32,
            ..Message::default()
        };
        reply.m_source = drv;
        // SAFETY(test): `mess_lsockdriver_vfs_reply { int32_t req_id; int
        // status; }` — the status is in the second slot.
        unsafe {
            reply.m_u.raw[4..8].copy_from_slice(&(-minix_types::EADDRINUSE).to_le_bytes());
        }
        assert!(
            state.finish_sdev_blocked(&reply),
            "被挂起的进程认领了这条回复"
        );
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((Endpoint::from_generation_slot(1, 0), -minix_types::EADDRINUSE)),
            "用户拿到载荷里的 status"
        );
        assert!(
            matches!(
                state
                    .fproc_table
                    .get(minix_types::UserSlot::new(0))
                    .unwrap()
                    .blocked_on,
                crate::fproc::BlockedOn::None
            ),
            "挂起态已清"
        );
        // Nobody is waiting + no suspended process → not claimed (soft failure).
        let mut stray = Message {
            m_type: minix_sockdriver::sdev::SdevReply::Reply as i32,
            ..Message::default()
        };
        stray.m_source = drv;
        assert!(!state.finish_sdev_blocked(&stray), "没有挂起者就不认领");
    }

    /// The getset family (`Setsockopt`/`Getsockopt`/`Getsockname`/`Getpeername`):
    /// the first two gates are the same as the socket family (EBADF/ENOTSOCK); once
    /// passed, the user buffer becomes a magic grant handed to the driver, the reply
    /// type is `SDEV_REPLY`, and **the status is in the payload**; for the `get`
    /// direction the status is the **new length**, placed into the reply payload
    /// (`m_vfs_lc_socklen { len }`).
    #[test]
    fn test_dispatch_sockopt_and_sockname() {
        use minix_types::Endpoint;

        let setup = |mode: u32| {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            let drv = Endpoint::from_generation_slot(0, 11);
            state.smap_table.entries[0].endpt = Some(drv);
            let dev = crate::device_map::make_smap_dev(state.smap_table.entries[0].num, 0x42);
            let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
            state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap()
                .filps[3] = Some(fid.get());
            let vid = state.vnode_table.alloc().unwrap();
            {
                let v = state.vnode_table.get_mut(vid).unwrap();
                v.fs = Endpoint::PFS;
                v.ino = 0x11;
                v.mode = mode;
                v.sdev = dev;
                v.ref_count = 1;
            }
            state.filp_table.get_mut(fid).unwrap().vnode = Some(vid.get());
            let idx = state
                .worker_pool
                .assign_first_fit(
                    minix_types::UserSlot::new(0),
                    crate::worker::WorkerFunc::DoWork,
                    &Message::default(),
                )
                .expect("空闲槽");
            state.current_worker = Some(idx);
            (state, idx, drv)
        };
        let sockopt_msg = |call: VfsCallNum, fd: i32| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: call as i32,
                ..Message::default()
            };
            // SAFETY: mess_lc_vfs_sockopt: fd@0, level@4, name@8, buf@16, len@24.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[8..12].copy_from_slice(&7i32.to_le_bytes());
                raw[16..24].copy_from_slice(&0x6000u64.to_le_bytes());
                raw[24..28].copy_from_slice(&32u32.to_le_bytes());
            }
            m
        };
        let sockaddr_msg = |call: VfsCallNum, fd: i32| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: call as i32,
                ..Message::default()
            };
            // SAFETY: mess_lc_vfs_sockaddr: fd@0, addr@8, addr_len@16.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[8..16].copy_from_slice(&0x6000u64.to_le_bytes());
                raw[16..24].copy_from_slice(&32u64.to_le_bytes());
            }
            m
        };

        // 1) Negative fd → EBADF; non-socket → ENOTSOCK (the getset family shares get_sock's gates).
        let (mut state, _idx, _drv) = setup(crate::open::S_IFSOCK | 0o777);
        state.current_message = sockopt_msg(VfsCallNum::Setsockopt, -1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Setsockopt),
            SyscallResult::Error(minix_types::EBADF)
        );
        let (mut state, _idx, _drv) = setup(crate::open::S_IFREG | 0o644);
        state.current_message = sockopt_msg(VfsCallNum::Setsockopt, 3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Setsockopt),
            SyscallResult::Error(minix_types::ENOTSOCK)
        );

        // 2) It is a socket → really sends `SDEV_SETSOCKOPT` (EIO on the host); the grant was built.
        let (mut state, idx, _drv) = setup(crate::open::S_IFSOCK | 0o777);
        state.current_message = sockopt_msg(VfsCallNum::Setsockopt, 3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Setsockopt),
            SyscallResult::Error(minix_types::EIO)
        );
        assert!(matches!(
            state.worker_pool.get_mut(idx).unwrap().cont,
            Some(crate::worker::WorkerCont::SdevGetSet { write_dir: false, .. })
        ));

        // 3) `getsockname`: the reply carries the **new length** (payload status = 16) →
        // the user gets 0, and the length goes into the reply payload.
        let (mut state, idx, drv) = setup(crate::open::S_IFSOCK | 0o777);
        state.current_message = sockaddr_msg(VfsCallNum::Getsockname, 3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getsockname),
            SyscallResult::Error(minix_types::EIO)
        );
        let mut reply = Message {
            m_type: minix_sockdriver::sdev::SdevReply::Reply as i32,
            ..Message::default()
        };
        reply.m_source = drv;
        // SAFETY(test): `mess_lsockdriver_vfs_reply { int32_t req_id; int
        // status; }` — here it is "how many bytes were written", i.e. the new length (second slot).
        unsafe {
            reply.m_u.raw[4..8].copy_from_slice(&16i32.to_le_bytes());
        }
        state.handle_drv_reply(&reply).expect("有槽在等这个驱动");
        state.run_worker_continuations();
        let (target, reply) = state.take_reply().expect("回复");
        assert_eq!(target, Endpoint::from_generation_slot(1, 0));
        assert_eq!(reply.m_type, 0, "成功回 0（长度在载荷里）");
        // SAFETY(test): `mess_vfs_lc_socklen { unsigned int len; }`.
        let len = unsafe { u32::from_le_bytes(reply.m_u.raw[0..4].try_into().unwrap()) };
        assert_eq!(len, 16, "新长度进回复载荷");
        let _ = idx;

        // 4) `getpeername` takes the same path (`level`/`name` are passed as 0).
        let (mut state, _idx, _drv) = setup(crate::open::S_IFSOCK | 0o777);
        state.current_message = sockaddr_msg(VfsCallNum::Getpeername, 3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getpeername),
            SyscallResult::Error(minix_types::EIO)
        );
    }

    /// The two consumers of `sdev_simple` (`Listen`/`Shutdown`) use the same driver
    /// request shape: the reply type must be `SDEV_REPLY`, and **the status is in
    /// the reply payload** (C `sdev_simple` sdev.c:245-276). A reply is fed here to
    /// walk this chain end to end.
    #[test]
    fn test_sdev_simple_reply_status_from_payload() {
        use minix_types::Endpoint;

        let mut state = seeded(100);
        crate::main_loop::seed_ready_state(&mut state);
        // One socket fd: the vnode carries `v_sdev` (smap row 0 + sock_id 0x42).
        let drv = Endpoint::from_generation_slot(0, 11);
        state.smap_table.entries[0].endpt = Some(drv);
        let dev = crate::device_map::make_smap_dev(state.smap_table.entries[0].num, 0x42);
        let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
        state
            .fproc_table
            .get_mut(minix_types::UserSlot::new(0))
            .unwrap()
            .filps[3] = Some(fid.get());
        let vid = state.vnode_table.alloc().unwrap();
        {
            let v = state.vnode_table.get_mut(vid).unwrap();
            v.fs = Endpoint::PFS;
            v.ino = 0x11;
            v.mode = crate::open::S_IFSOCK | 0o777;
            v.sdev = dev;
            v.ref_count = 1;
        }
        state.filp_table.get_mut(fid).unwrap().vnode = Some(vid.get());
        let idx = state
            .worker_pool
            .assign_first_fit(
                minix_types::UserSlot::new(0),
                crate::worker::WorkerFunc::DoWork,
                &Message::default(),
            )
            .expect("空闲槽");
        state.current_worker = Some(idx);

        // `listen(fd, 5)`: under the host build the send fails → EIO, but the continuation is attached.
        let mut m = Message {
            m_source: Endpoint::from_generation_slot(1, 0),
            m_type: VfsCallNum::Listen as i32,
            ..Message::default()
        };
        // SAFETY: mess_lc_vfs_listen: fd@0, backlog@4.
        unsafe {
            let raw = &mut m.m_u.raw;
            raw[0..4].copy_from_slice(&3i32.to_le_bytes());
            raw[4..8].copy_from_slice(&5i32.to_le_bytes());
        }
        state.current_message = m;
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Listen),
            SyscallResult::Error(minix_types::EIO)
        );
        assert!(matches!(
            state.worker_pool.get_mut(idx).unwrap().cont,
            Some(crate::worker::WorkerCont::SdevSimple)
        ));

        // Driver reply: `SDEV_REPLY` + the status in the payload (deliberately -EACCES here).
        let mut reply = Message {
            m_type: minix_sockdriver::sdev::SdevReply::Reply as i32,
            ..Message::default()
        };
        reply.m_source = drv;
        // SAFETY(test): `mess_lsockdriver_vfs_reply { int32_t req_id; int
        // status; }` — the status is in the **second slot** (req_id comes first).
        unsafe {
            reply.m_u.raw[4..8].copy_from_slice(&(-minix_types::EACCES).to_le_bytes());
        }
        state.handle_drv_reply(&reply).expect("有槽在等这个驱动");
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((Endpoint::from_generation_slot(1, 0), -minix_types::EACCES)),
            "用户拿到的是**载荷里的 status**，不是回复号"
        );

        // Reply type is not `SDEV_REPLY` → EIO (C's protocol-error surface).
        let idx2 = state
            .worker_pool
            .assign_first_fit(
                minix_types::UserSlot::new(0),
                crate::worker::WorkerFunc::DoWork,
                &Message::default(),
            )
            .expect("空闲槽");
        state.current_worker = Some(idx2);
        state.current_message = Message {
            m_source: Endpoint::from_generation_slot(1, 0),
            m_type: VfsCallNum::Listen as i32,
            ..Message::default()
        };
        // SAFETY: fd@0, backlog@4 (same as the call above).
        unsafe {
            let raw = &mut state.current_message.m_u.raw;
            raw[0..4].copy_from_slice(&3i32.to_le_bytes());
            raw[4..8].copy_from_slice(&5i32.to_le_bytes());
        }
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Listen),
            SyscallResult::Error(minix_types::EIO)
        );
        // Both select reply types are already claimed by the select layer (the wait
        // half is wired) — use an undefined value inside the SDEV_RS window as the
        // out-of-protocol reply type.
        let mut bad = Message { m_type: 0x1999, ..Message::default() };
        bad.m_source = drv;
        state.handle_drv_reply(&bad).expect("有槽在等这个驱动");
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((Endpoint::from_generation_slot(1, 0), -minix_types::EIO)),
            "回复号不对 → 协议错误"
        );
    }

    /// **Reply landing** in the driver conversation pipeline (the common first half
    /// of C `sdev_reply`): when a driver reply arrives it finds the slot waiting for
    /// it, lands there, wakes it, and the continuation runs immediately (here it
    /// reaches PFS's `REQ_NEWNODE` — the first step of `make_sock_fd`).
    #[test]
    fn test_driver_reply_lands_and_chains_to_pfs() {
        use minix_types::Endpoint;

        let mut state = seeded(100);
        crate::main_loop::seed_ready_state(&mut state);
        // Domain 2 → smap row 0 (endpoint = the driver).
        state.smap_table.pfmap[2] = Some(0);
        let drv = Endpoint::from_generation_slot(0, 11);
        state.smap_table.entries[0].endpt = Some(drv);
        // PFS mount row (`make_sock_fd` needs it).
        let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(1)).unwrap();
        v.fs = Endpoint::PFS;
        v.dev = 9;
        let idx = state
            .worker_pool
            .assign_first_fit(
                minix_types::UserSlot::new(0),
                crate::worker::WorkerFunc::DoWork,
                &Message::default(),
            )
            .expect("空闲槽");
        state.current_worker = Some(idx);

        // The arm: pass the gates → record the continuation (`SdevSocket`) → send fails (host) → EIO.
        let mut m = Message {
            m_source: Endpoint::from_generation_slot(1, 0),
            m_type: VfsCallNum::Socket as i32,
            ..Message::default()
        };
        // SAFETY: mess_lc_vfs_socket: domain@0, type@4, protocol@8.
        unsafe {
            let raw = &mut m.m_u.raw;
            raw[0..4].copy_from_slice(&2i32.to_le_bytes());
            raw[4..8].copy_from_slice(&1i32.to_le_bytes());
        }
        state.current_message = m;
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Socket),
            SyscallResult::Error(minix_types::EIO)
        );
        assert!(
            matches!(
                state.worker_pool.get_mut(idx).unwrap().cont,
                Some(crate::worker::WorkerCont::SdevSocket { pair: false, .. })
            ),
            "续接标识已挂（等驱动回复）"
        );

        // Driver reply arrives: `handle_drv_reply` finds the slot waiting for it and lands it.
        let mut reply = Message {
            m_type: minix_sockdriver::sdev::SdevReply::SocketReply as i32,
            ..Message::default()
        };
        reply.m_source = drv;
        // SAFETY(test): `mess_lsockdriver_vfs_socket_reply`: req_id@0,
        // sock_id@4, sock_id2@8.
        unsafe {
            let raw = &mut reply.m_u.raw;
            raw[0..4].copy_from_slice(&0i32.to_le_bytes());
            raw[4..8].copy_from_slice(&0x42i32.to_le_bytes());
            raw[8..12].copy_from_slice(&(-1i32).to_le_bytes());
        }
        let slot = state.handle_drv_reply(&reply).expect("有槽在等这个驱动");
        assert_eq!(slot, idx);
        state.run_worker_continuations();

        // The continuation reaches the first step of `make_sock_fd`: send PFS
        // `REQ_NEWNODE`, with the device number `make_smap_dev(row, sock_id)` (the
        // high 32 bits are the row).
        let p = state.pending_fs.as_ref().expect("已登记 REQ_NEWNODE");
        assert_eq!(p.req.m_type, minix_types::REQ_NEWNODE);
        assert_eq!(p.fs_e, Endpoint::PFS);
        // SAFETY(test): `mess_vfs_fs_newnode`: device@0, mode@8.
        unsafe {
            let raw = &p.req.m_u.raw;
            let dev = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let mode = u32::from_le_bytes(raw[8..12].try_into().unwrap());
            let row = state.smap_table.entries[0].num;
            assert_eq!(dev, crate::device_map::make_smap_dev(row, 0x42));
            assert_eq!(mode & crate::open::S_IFMT, crate::open::S_IFSOCK, "套接字节点");
        }
    }

    /// The `Pipe2` arm (C `do_pipe2` pipe.c:39-55 + `create_pipe` pipe.c:58-135):
    /// with no PFS mount row → fail-closed EIO (C panics "PFS gone"); with a mount
    /// row it reserves a vnode + claims two fds/filps and sends `REQ_NEWNODE` to PFS;
    /// when the reply arrives it fills the vnode and both filps, and sends
    /// `m_vfs_lc_fdpair { fd0, fd1 }` back as the **reply payload** (the user gets
    /// these two fds).
    #[test]
    fn test_dispatch_pipe2_flow_and_payload() {
        use minix_types::Endpoint;

        let setup = |with_pfs: bool| {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            if with_pfs {
                // PFS mount row (the `find_vmnt(PFS_PROC_NR)` hop).
                let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(1)).unwrap();
                v.fs = Endpoint::PFS;
                v.dev = 9;
            }
            let idx = state
                .worker_pool
                .assign_first_fit(
                    minix_types::UserSlot::new(0),
                    crate::worker::WorkerFunc::DoWork,
                    &Message::default(),
                )
                .expect("空闲槽");
            state.current_worker = Some(idx);
            (state, idx)
        };
        let pipe2_msg = |flags: i32, oflags: i32| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Pipe2 as i32,
                ..Message::default()
            };
            // SAFETY: mess_lc_vfs_pipe2: flags@0, _unused@4, oflags@8.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&flags.to_le_bytes());
                raw[8..12].copy_from_slice(&oflags.to_le_bytes());
            }
            m
        };

        // 1) No PFS mount row → EIO (C panics here; the Rust side fails closed).
        let (mut state, _idx) = setup(false);
        state.current_message = pipe2_msg(0, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Pipe2),
            SyscallResult::Error(minix_types::EIO),
            "PFS 未挂载：不假装建成了管道"
        );

        // 2) PFS present: claim two fds/filps and send REQ_NEWNODE to PFS.
        let (mut state, idx) = setup(true);
        state.current_message = pipe2_msg(0, crate::open::OpenFlags::CLOEXEC.bits() as i32);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Pipe2),
            SyscallResult::Suspend
        );
        let p = state.pending_fs.as_ref().expect("已登记 REQ_NEWNODE");
        assert_eq!(p.req.m_type, minix_types::REQ_NEWNODE);
        assert_eq!(p.fs_e, Endpoint::PFS);
        // SAFETY(test): read back in C's mess_vfs_fs_newnode field order (device@0, mode@8).
        unsafe {
            let raw = &p.req.m_u.raw;
            assert_eq!(u64::from_le_bytes(raw[0..8].try_into().unwrap()), 0);
            let mode = u32::from_le_bytes(raw[8..12].try_into().unwrap());
            assert_eq!(mode & crate::open::S_IFMT, crate::open::S_IFIFO, "管道节点");
        }
        {
            let fp = state
                .fproc_table
                .get(minix_types::UserSlot::new(0))
                .unwrap();
            assert!(fp.filps[0].is_some() && fp.filps[1].is_some(), "两个 fd 已认领");
            // The CLOEXEC bit is not set at this step — C calls `FD_SET` only
            // **after** creation succeeds (pipe.c:126-129), so only fd claiming is
            // asserted here.
            assert!(!fp.cloexec_set.get(0), "CLOEXEC 还没置（建成功才置）");
        }

        // 3) Reply arrives (with the new node details) → fill the vnode/both filps + the reply payload carries the fd pair.
        let mut reply = Message { m_type: 0, ..Message::default() };
        // SAFETY(test): fill in ino/mode/device using lookup_reply_off (same order
        // as the first six fields of mess_fs_vfs_newnode).
        unsafe {
            let raw = &mut reply.m_u.raw;
            raw[minix_types::lookup_reply_off::INODE..minix_types::lookup_reply_off::INODE + 8]
                .copy_from_slice(&0x77u64.to_le_bytes());
            raw[minix_types::lookup_reply_off::MODE..minix_types::lookup_reply_off::MODE + 4]
                .copy_from_slice(&(crate::open::S_IFIFO | 0o600).to_le_bytes());
        }
        state.pending_fs = None;
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.sendrec = Some(reply);
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        let (target, reply) = state.take_reply().expect("回复");
        assert_eq!(target, Endpoint::from_generation_slot(1, 0));
        assert_eq!(reply.m_type, 0, "成功回 0（fd 对在载荷里）");
        // SAFETY(test): `mess_vfs_lc_fdpair { fd0, fd1 }`.
        let (fd0, fd1) = unsafe {
            (
                i32::from_le_bytes(reply.m_u.raw[0..4].try_into().unwrap()),
                i32::from_le_bytes(reply.m_u.raw[4..8].try_into().unwrap()),
            )
        };
        assert_eq!((fd0, fd1), (0, 1), "回复载荷带的是两个 fd");
        // The vnode and both filps are in place.
        let vnode_idx = {
            let fp = state
                .fproc_table
                .get(minix_types::UserSlot::new(0))
                .unwrap();
            let f0 = fp.filps[fd0 as usize].expect("读端 filp");
            state
                .filp_table
                .get(crate::filp::FilpId(f0))
                .and_then(|f| f.vnode)
                .expect("filp 指向 vnode")
        };
        let v = state
            .vnode_table
            .get(crate::vnode::VnodeId(vnode_idx))
            .unwrap();
        assert_eq!(v.ino, 0x77);
        assert_eq!(v.fs, Endpoint::PFS, "管道节点在 PFS 上");
        assert_eq!(v.ref_count, 2, "两端各持一个引用");
        let fp = state
            .fproc_table
            .get(minix_types::UserSlot::new(0))
            .unwrap();
        let f0 = state
            .filp_table
            .get(crate::filp::FilpId(fp.filps[fd0 as usize].unwrap()))
            .unwrap();
        let f1 = state
            .filp_table
            .get(crate::filp::FilpId(fp.filps[fd1 as usize].unwrap()))
            .unwrap();
        assert_eq!(f0.flags as u32 & crate::open::O_ACCMODE, crate::open::O_RDONLY);
        assert_eq!(f1.flags as u32 & crate::open::O_ACCMODE, crate::open::O_WRONLY);
        // CLOEXEC is set only after creation succeeds (C pipe.c:126-129), for both fds.
        assert!(fp.cloexec_set.get(fd0 as usize), "读端 CLOEXEC");
        assert!(fp.cloexec_set.get(fd1 as usize), "写端 CLOEXEC");
    }

    /// The `Copyfd` arm (C `do_copyfd` filedes.c:524-650, driver callback): only the
    /// superuser may call it (EPERM); the remote endpoint must be known (EINVAL); all
    /// three operations are **local table operations** — `TO` installs the caller's
    /// fd in the remote end, `FROM` does the reverse (without CLOEXEC), and `CLOSE`
    /// rolls back one `TO` (allowed only when the count > 1).
    #[test]
    fn test_dispatch_copyfd_three_ops() {
        use minix_types::Endpoint;

        let caller_ep = Endpoint::from_generation_slot(1, 0);
        let remote_ep = Endpoint::from_generation_slot(1, 1);
        let setup = |is_super: bool| {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            // Note that the default `eff_uid` is 0 (= SU_UID), so the "non-superuser"
            // case must **explicitly** be set to non-zero, otherwise the gate is
            // always open.
            state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap()
                .eff_uid = if is_super { crate::link::SU_UID } else { 1000 };
            // Remote process: slot 1.
            {
                let fp = state
                    .fproc_table
                    .get_mut(minix_types::UserSlot::new(1))
                    .unwrap();
                fp.endpoint = remote_ep;
                fp.pid = 101;
            }
            // Caller's fd 3 → filp (count 1).
            let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
            state.filp_table.inc_count(fid);
            state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap()
                .filps[3] = Some(fid.get());
            // The remote end also has its own fd 5 → the same filp (used for FROM/CLOSE).
            state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(1))
                .unwrap()
                .filps[5] = Some(fid.get());
            state.filp_table.inc_count(fid);
            (state, fid)
        };
        let copyfd_msg = |endpt: Endpoint, fd: i32, what: i32| {
            let mut m = Message {
                m_source: caller_ep,
                m_type: VfsCallNum::Copyfd as i32,
                ..Message::default()
            };
            // SAFETY: mess_lsys_vfs_copyfd: endpt@0, fd@4, what@8.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&endpt.0.to_le_bytes());
                raw[4..8].copy_from_slice(&fd.to_le_bytes());
                raw[8..12].copy_from_slice(&what.to_le_bytes());
            }
            m
        };

        // 1) Non-superuser → EPERM (the privilege of the driver callback).
        let (mut state, _fid) = setup(false);
        state.current_message = copyfd_msg(remote_ep, 3, 1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Copyfd),
            SyscallResult::Error(minix_types::EPERM)
        );

        // 2) Unknown remote endpoint → EINVAL.
        let (mut state, _fid) = setup(true);
        state.current_message = copyfd_msg(Endpoint::from_generation_slot(3, 9), 3, 1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Copyfd),
            SyscallResult::Error(minix_types::EINVAL)
        );

        // 3) `COPYFD_TO`: the caller's fd 3 is installed in the remote end's first
        // free slot (0); the CLOEXEC bit follows `what`'s 0x8000.
        let (mut state, fid) = setup(true);
        state.current_message = copyfd_msg(remote_ep, 3, 1 | 0x8000);
        let r = dispatch_syscall(&mut state, VfsCallNum::Copyfd);
        assert_eq!(r, SyscallResult::Ok(0), "远端第一个空闲 fd 是 0");
        {
            let remote = state
                .fproc_table
                .get(minix_types::UserSlot::new(1))
                .unwrap();
            assert_eq!(remote.filps[0], Some(fid.get()), "远端 fd 0 指向同一 filp");
            assert!(remote.cloexec_set.get(0), "CLOEXEC 位跟过来");
        }
        assert_eq!(state.filp_table.get(fid).unwrap().count, 3, "计数 +1");

        // 4) `COPYFD_FROM`: the remote end's fd 5 is installed in the **caller's**
        // table, **without** CLOEXEC (C explicitly does `flags &= ~COPYFD_CLOEXEC`).
        let (mut state, fid) = setup(true);
        state.current_message = copyfd_msg(remote_ep, 5, 0 | 0x8000);
        let r = dispatch_syscall(&mut state, VfsCallNum::Copyfd);
        assert_eq!(r, SyscallResult::Ok(0), "调用方第一个空闲 fd 是 0");
        {
            let caller = state
                .fproc_table
                .get(minix_types::UserSlot::new(0))
                .unwrap();
            assert_eq!(caller.filps[0], Some(fid.get()));
            assert!(!caller.cloexec_set.get(0), "FROM 方向剥掉 CLOEXEC");
        }

        // 5) `COPYFD_CLOSE`: roll back one `TO` — allowed only when count > 1; on success the remote slot is cleared.
        let (mut state, _fid) = setup(true);
        state.current_message = copyfd_msg(remote_ep, 5, 2);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Copyfd),
            SyscallResult::Ok(5),
            "CLOSE 回的是被清掉的 fd 号"
        );
        assert_eq!(
            state
                .fproc_table
                .get(minix_types::UserSlot::new(1))
                .unwrap()
                .filps[5],
            None,
            "远端槽已清"
        );

        // 6) CLOSE when count == 1 → EBADF (C's `filp_count > 1` gate).
        let (mut state, fid) = setup(true);
        // Squeeze the count down to 1 (keep only the caller's reference).
        state.filp_table.dec_count(fid);
        state
            .fproc_table
            .get_mut(minix_types::UserSlot::new(1))
            .unwrap()
            .filps[5] = Some(fid.get());
        state.current_message = copyfd_msg(remote_ep, 5, 2);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Copyfd),
            SyscallResult::Error(minix_types::EBADF)
        );
    }

    /// The `Ioctl` arm's type dispatch (C `do_ioctl` device.c:18-58): fd/vnode gates
    /// → dispatch by **file type** — non-device files are always `ENOTTY` (C's
    /// `default`; this is a local decision and is really installed); the three device
    /// types each need a driver conversation, whose pipeline is not wired → honestly
    /// return ENOSYS and record the gap.
    #[test]
    fn test_dispatch_ioctl_type_dispatch() {
        use minix_types::Endpoint;

        let setup = |mode: u32| {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
            state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap()
                .filps[3] = Some(fid.get());
            let vid = state.vnode_table.alloc().unwrap();
            {
                let v = state.vnode_table.get_mut(vid).unwrap();
                v.fs = Endpoint::MFS;
                v.ino = 0x42;
                v.mode = mode;
                v.ref_count = 1;
            }
            state.filp_table.get_mut(fid).unwrap().vnode = Some(vid.get());
            (state, fid)
        };
        let ioctl_msg = |fd: i32, req: u64| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Ioctl as i32,
                ..Message::default()
            };
            // SAFETY: mess_lc_vfs_ioctl: fd@0, req@8, arg@16.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[8..16].copy_from_slice(&req.to_le_bytes());
                raw[16..24].copy_from_slice(&0x6000u64.to_le_bytes());
            }
            m
        };

        // 1) Negative fd / empty slot → EBADF.
        let (mut state, _fid) = setup(crate::open::S_IFCHR | 0o644);
        state.current_message = ioctl_msg(-1, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Ioctl),
            SyscallResult::Error(minix_types::EBADF)
        );
        state.current_message = ioctl_msg(9, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Ioctl),
            SyscallResult::Error(minix_types::EBADF)
        );

        // 2) Regular files and directories → ENOTTY (C's `default`, a local decision).
        for mode in [
            crate::open::S_IFREG | 0o644,
            crate::open::S_IFDIR | 0o755,
            crate::open::S_IFIFO | 0o644,
        ] {
            let (mut state, _fid) = setup(mode);
            state.current_message = ioctl_msg(3, 0x5401);
            assert_eq!(
                dispatch_syscall(&mut state, VfsCallNum::Ioctl),
                SyscallResult::Error(minix_types::ENOTTY),
                "mode {mode:o} 不是设备文件"
            );
        }

        // 3) Character device: no driver mapped in dmap → **ENXIO** (the "no
        // driver for major" surface of C `cdev_get`).
        let (mut state, _fid) = setup(crate::open::S_IFCHR | 0o644);
        state.current_message = ioctl_msg(3, 0x5401);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Ioctl),
            SyscallResult::Error(minix_types::ENXIO),
            "字符设备要有 dmap 驱动"
        );

        // 4) Character device + a driver → really send `CDEV_IOCTL` (under the host
        // build the trap is unreachable → EIO), and the continuation is attached
        // (waiting for the driver's reply).
        let (mut state, _fid) = setup(crate::open::S_IFCHR | 0o644);
        let idx;
        {
            // The vnode's `sdev` sets major/minor (major 3 → dmap row 3).
            let vid = state
                .vnode_table
                .find_by_ino(Endpoint::MFS, 0x42)
                .expect("刚建的 vnode");
            state.vnode_table.get_mut(vid).unwrap().sdev = (3 << 8) | 7;
            idx = state
                .worker_pool
                .assign_first_fit(
                    minix_types::UserSlot::new(0),
                    crate::worker::WorkerFunc::DoWork,
                    &Message::default(),
                )
                .expect("空闲槽");
            state.current_worker = Some(idx);
            let mut row = crate::device_map::DmapEntry::empty();
            row.driver = Some(Endpoint::from_generation_slot(0, 12));
            state.dmap_table.set(3, row);
        }
        // On the host the grant table's first growth cannot pass `sys_setgrant`:
        // warm it up so the magic grant is really created (otherwise EIO comes from
        // the grant gate and the send is never reached).
        crate::main_loop::warm_grants(&mut state);
        state.current_message = ioctl_msg(3, 0x5401);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Ioctl),
            SyscallResult::Error(minix_types::EIO),
            "有驱动就真发（宿主下发送失败）"
        );
        // The message number of the parked request must be the full base value +
        // ordinal (0x404) — the enum ordinal (4) from `as i32` does not pass the
        // driver-side `is_char_request` mask test.
        let parked = state
            .worker_pool
            .get(idx)
            .and_then(|w| w.sendrec)
            .expect("请求已停槽");
        assert_eq!(
            parked.m_type,
            minix_chardriver::protocol::CdevRequest::Ioctl.message_type(),
            "CDEV_IOCTL 消息号完整（0x404）"
        );

        // 5) Socket: smap has no driver for this device number → EIO (the
        // `get_smap_by_dev` failure surface of C `sdev_ioctl`); with a driver it
        // really sends `SDEV_IOCTL` (send fails on the host → EIO).
        let (mut state, _fid) = setup(crate::open::S_IFSOCK | 0o644);
        state.current_message = ioctl_msg(3, 0x5401);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Ioctl),
            SyscallResult::Error(minix_types::EIO),
            "套接字要有 smap 行"
        );

        // 6) Block device: no driver in dmap → ENXIO (the "no driver for major"
        // surface of C `bdev_ioctl`); with a driver it really sends `BDEV_IOCTL`
        // (send fails on the host → EIO), and the `filp_ioctl_fp` guard must be
        // cleared on the failure path.
        let (mut state, fid) = setup(crate::open::S_IFBLK | 0o644);
        state.current_message = ioctl_msg(3, 0x5401);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Ioctl),
            SyscallResult::Error(minix_types::ENXIO),
            "块设备要有 dmap 驱动"
        );
        {
            let vid = state
                .vnode_table
                .find_by_ino(Endpoint::MFS, 0x42)
                .expect("刚建的 vnode");
            state.vnode_table.get_mut(vid).unwrap().sdev = (4 << 8) | 2;
            let idx = state
                .worker_pool
                .assign_first_fit(
                    minix_types::UserSlot::new(0),
                    crate::worker::WorkerFunc::DoWork,
                    &Message::default(),
                )
                .expect("空闲槽");
            state.current_worker = Some(idx);
            let mut row = crate::device_map::DmapEntry::empty();
            row.driver = Some(Endpoint::from_generation_slot(0, 13));
            state.dmap_table.set(4, row);
        }
        state.current_message = ioctl_msg(3, 0x5401);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Ioctl),
            SyscallResult::Error(minix_types::EIO),
            "有驱动就真发（宿主下发送失败）"
        );
        assert_eq!(
            state.filp_table.get(fid).unwrap().ioctl_holder,
            None,
            "发送失败要清掉 filp 的 ioctl 守卫"
        );
    }

    /// The local subset of the `Fcntl` arm (C `do_fcntl` misc.c:127-300): the
    /// `F_DUPFD` family shares one filp (`filp_count++`, **no** new filp),
    /// `F_GETFD`/`F_SETFD` read/write the close-on-exec bit, `F_GETFL`/`F_SETFL`
    /// read/write the status word (narrow gate), and
    /// `F_GETNOSIGPIPE`/`F_SETNOSIGPIPE` read/write the sentinel bit; unknown cmd → EINVAL.
    #[test]
    fn test_dispatch_fcntl_local_commands() {
        use minix_types::Endpoint;

        let setup = || {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
            {
                let f = state.filp_table.get_mut(fid).unwrap();
                f.flags = (crate::open::O_RDONLY | crate::fcntl::O_APPEND) as i32;
            }
            // In production the filp referenced from the fd table has a count of at
            // least 1 (claimed at open) — the test adds it, otherwise the "count +1
            // after dup" assertion cannot show the difference.
            state.filp_table.inc_count(fid);
            state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap()
                .filps[3] = Some(fid.get());
            (state, fid)
        };
        let fcntl_msg = |fd: i32, cmd: u32, arg: i32| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Fcntl as i32,
                ..Message::default()
            };
            // SAFETY: mess_lc_vfs_fcntl: fd@0, cmd@4, arg_int@8, arg_ptr@16.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[4..8].copy_from_slice(&cmd.to_le_bytes());
                raw[8..12].copy_from_slice(&arg.to_le_bytes());
            }
            m
        };

        // 1) Unknown cmd → EINVAL (C's `default`).
        let (mut state, _fid) = setup();
        state.current_message = fcntl_msg(3, 0x7fff, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fcntl),
            SyscallResult::Error(minix_types::EINVAL)
        );

        // 2) Negative fd / empty slot → EBADF.
        let (mut state, _fid) = setup();
        state.current_message = fcntl_msg(-1, crate::fcntl::F_GETFD, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fcntl),
            SyscallResult::Error(minix_types::EBADF)
        );
        state.current_message = fcntl_msg(9, crate::fcntl::F_GETFD, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fcntl),
            SyscallResult::Error(minix_types::EBADF)
        );

        // 3) `F_DUPFD`: floor gate (negative/out of range → EINVAL); when legal it
        // takes the lowest free fd and **shares the same filp** (count +1, no new filp).
        let (mut state, fid) = setup();
        state.current_message = fcntl_msg(3, crate::fcntl::F_DUPFD, -1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fcntl),
            SyscallResult::Error(minix_types::EINVAL)
        );
        state.current_message = fcntl_msg(3, crate::fcntl::F_DUPFD, 0);
        let r = dispatch_syscall(&mut state, VfsCallNum::Fcntl);
        assert_eq!(r, SyscallResult::Ok(0), "fd 0 是第一个空闲槽");
        {
            let fp = state
                .fproc_table
                .get(minix_types::UserSlot::new(0))
                .unwrap();
            assert_eq!(fp.filps[0], Some(fid.get()), "新 fd 指向同一个 filp");
            assert!(!fp.cloexec_set.get(0), "F_DUPFD 不置 cloexec");
        }
        assert_eq!(
            state.filp_table.get(fid).unwrap().count,
            2,
            "共用 filp：计数 +1"
        );
        // "No new filp allocated": the two fd-table slots point at **the same** filp
        // id (fd 0's target is asserted above), plus count +1 — `FilpTable::len` is
        // the table capacity (NR_FILPS), not the used count, so don't use it as the criterion.
        assert_eq!(
            state.filp_table.get(fid).unwrap().count,
            2,
            "仍然只有这一个 filp 在服务两个 fd"
        );

        // 4) `F_DUPFD_CLOEXEC`: also sets the cloexec bit.
        let (mut state, _fid) = setup();
        state.current_message = fcntl_msg(3, crate::fcntl::F_DUPFD_CLOEXEC, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fcntl),
            SyscallResult::Ok(0)
        );
        assert!(
            state
                .fproc_table
                .get(minix_types::UserSlot::new(0))
                .unwrap()
                .cloexec_set
                .get(0),
            "CLOEXEC 变体置位"
        );

        // 5) `F_GETFD`/`F_SETFD`: read and write the same bit.
        let (mut state, _fid) = setup();
        state.current_message = fcntl_msg(3, crate::fcntl::F_GETFD, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fcntl),
            SyscallResult::Ok(0),
            "未置位时回 0"
        );
        state.current_message = fcntl_msg(3, crate::fcntl::F_SETFD, crate::fcntl::FD_CLOEXEC as i32);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fcntl),
            SyscallResult::Ok(0)
        );
        state.current_message = fcntl_msg(3, crate::fcntl::F_GETFD, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fcntl),
            SyscallResult::Ok(crate::fcntl::FD_CLOEXEC as i32),
            "置位后回 FD_CLOEXEC"
        );

        // 6) `F_GETFL`/`F_SETFL`: status-word read-back and narrow-gate write (only O_NONBLOCK|O_APPEND is admitted).
        let (mut state, fid) = setup();
        state.current_message = fcntl_msg(3, crate::fcntl::F_GETFL, 0);
        let r = dispatch_syscall(&mut state, VfsCallNum::Fcntl);
        assert_eq!(
            r,
            SyscallResult::Ok((crate::open::O_RDONLY | crate::fcntl::O_APPEND) as i32)
        );
        // Try to slip in a bit outside the narrow gate (O_TRUNC): it must not get in.
        state.current_message = fcntl_msg(
            3,
            crate::fcntl::F_SETFL,
            crate::open::OpenFlags::TRUNC.bits() as i32,
        );
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fcntl),
            SyscallResult::Ok(0)
        );
        let flags = state.filp_table.get(fid).unwrap().flags as u32;
        assert_eq!(
            flags & crate::open::OpenFlags::TRUNC.bits(),
            0,
            "窄门外的位进不来"
        );
    }

    /// The `Mapdriver` arm (C `do_mapdriver` dmap.c:106-177): three gates come before
    /// any table lookup (only RS may call → EPERM / the label must fit → EINVAL / the
    /// label must be NUL-terminated → EINVAL); label → endpoint goes through the local
    /// directory (the production gap is `LabelDir`); on a hit, mark it as a service
    /// process and write dmap/smap.
    ///
    /// The label lives in **the caller's memory** (C's `sys_vircopy`), unreachable on
    /// the host → EINVAL — so the half "after fetching the label" is covered directly
    /// by calling the body in `test_finish_mapdriver_*`.
    #[test]
    fn test_dispatch_mapdriver_gates() {
        use minix_types::Endpoint;

        let mapdriver_msg = |major: u32, label_len: u64| {
            let mut m = Message {
                m_source: Endpoint::RS, // only RS may call
                m_type: VfsCallNum::Mapdriver as i32,
                ..Message::default()
            };
            // SAFETY: mess_lsys_vfs_mapdriver: major@0, labellen@8, label@16,
            // ndomains@24, domains@28...
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&major.to_le_bytes());
                raw[8..16].copy_from_slice(&label_len.to_le_bytes());
                raw[16..24].copy_from_slice(&0x6000u64.to_le_bytes());
                raw[24..28].copy_from_slice(&0i32.to_le_bytes());
            }
            m
        };

        // 1) A non-RS caller → EPERM.
        let mut state = seeded(100);
        state.current_message = mapdriver_msg(3, 4);
        state.current_message.m_source = Endpoint::PM;
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Mapdriver),
            SyscallResult::Error(minix_types::EPERM)
        );

        // 2) Label longer than LABEL_MAX → EINVAL.
        state.current_message = mapdriver_msg(3, crate::device_map::LABEL_MAX as u64 + 1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Mapdriver),
            SyscallResult::Error(minix_types::EINVAL)
        );

        // 3) Legal label length but the cross-space fetch fails (host) → EINVAL (the
        // same value as C's failed `sys_vircopy`).
        state.current_message = mapdriver_msg(3, 4);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Mapdriver),
            SyscallResult::Error(minix_types::EINVAL)
        );
    }

    /// The body of `finish_mapdriver` (the half after fetching the label): an
    /// unregistered label → EINVAL; a registered one → mark it as a service process
    /// + write the dmap row (when major is valid) + write the smap row and `pfmap`
    /// (when domains are given); an smap failure must **undo** the dmap entry.
    #[test]
    fn test_finish_mapdriver_registers_and_undoes() {
        use minix_types::Endpoint;

        let mut state = seeded(100);
        let drv = Endpoint::from_generation_slot(0, 9);
        // The driver process occupies an fproc slot (the `isokendpt` hop).
        let drv_slot = drv.to_user_slot().expect("用户槽");
        state.driver_labels.push(("mydrv".to_string(), drv));

        // 1) The label is not registered → EINVAL (fail-closed; don't pretend the lookup succeeded).
        assert_eq!(
            state.finish_mapdriver(Endpoint::RS, "unknown", 3, &[]),
            minix_types::EINVAL
        );

        // 2) A non-RS caller → EPERM.
        assert_eq!(
            state.finish_mapdriver(Endpoint::PM, "mydrv", 3, &[]),
            minix_types::EPERM
        );

        // 3) Only major given: write the dmap row + mark the service process.
        assert_eq!(
            state.finish_mapdriver(Endpoint::RS, "mydrv", 3, &[]),
            0
        );
        assert_eq!(
            state.dmap_table.get(3).and_then(|r| r.driver),
            Some(drv),
            "dmap 行已登记"
        );
        assert!(
            state
                .fproc_table
                .get(drv_slot)
                .map(|fp| fp.flags.contains(crate::fproc::FpFlags::SRV_PROC))
                .unwrap_or(false),
            "驱动进程被标成服务进程"
        );

        // 4) Domains given: write the smap row + `pfmap[domain]`.
        assert_eq!(
            state.finish_mapdriver(Endpoint::RS, "mydrv", 3, &[1, 2]),
            0
        );
        let row = state
            .smap_table
            .entries
            .iter()
            .position(|e| e.endpt == Some(drv))
            .expect("smap 行");
        assert_eq!(state.smap_table.pfmap[1], Some(row as u8));
        assert_eq!(state.smap_table.pfmap[2], Some(row as u8));
        assert_eq!(
            &state.smap_table.entries[row].label[..6],
            b"mydrv\0",
            "标签写进 smap 行"
        );

        // 5) The domain is taken by **another** driver → EBUSY, and the dmap registration must be undone.
        let other = Endpoint::from_generation_slot(0, 10);
        state.driver_labels.push(("other".to_string(), other));
        assert_eq!(
            state.finish_mapdriver(Endpoint::RS, "other", 5, &[1]),
            minix_types::EBUSY,
            "域 1 已被 mydrv 占"
        );
        assert_eq!(
            state.dmap_table.get(5).and_then(|r| r.driver),
            None,
            "smap 失败要把刚写的 dmap 行撤销（C 的 undo）"
        );
    }

    /// The `Getvfsstat` arm (C `do_getvfsstat` stadir.c:330-403): `buf == 0` only
    /// counts (**returns the count**, not 0); with a buffer, the mount list is
    /// truncated to `bufsize / sizeof` and filled one by one at `buf + i*sizeof`
    /// (multi-mount sequence), returning the **count** even if all succeed.
    #[test]
    fn test_dispatch_getvfsstat_count_and_sequence() {
        use minix_types::Endpoint;

        let setup = || {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            // Two rows reportable (dev is not NO_DEV + CANSTAT), one row lacks CANSTAT and must be filtered out.
            {
                let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(0)).unwrap();
                v.flags = crate::vmnt::VmntFlags::CANSTAT;
                v.root = None;
            }
            {
                let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(1)).unwrap();
                v.fs = Endpoint::from_generation_slot(0, 7);
                v.dev = 2;
                v.flags = crate::vmnt::VmntFlags::CANSTAT;
            }
            {
                let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(2)).unwrap();
                v.fs = Endpoint::from_generation_slot(0, 8);
                v.dev = 3; // no CANSTAT → not counted
            }
            let idx = state
                .worker_pool
                .assign_first_fit(
                    minix_types::UserSlot::new(0),
                    crate::worker::WorkerFunc::DoWork,
                    &Message::default(),
                )
                .expect("空闲槽");
            state.current_worker = Some(idx);
            (state, idx)
        };
        let msg = |buf: u64, len: u64, flags: i32| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Getvfsstat as i32,
                ..Message::default()
            };
            // SAFETY: mess_lc_vfs_getvfsstat: buf@0, len@8, flags@16.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..8].copy_from_slice(&buf.to_le_bytes());
                raw[8..16].copy_from_slice(&len.to_le_bytes());
                raw[16..20].copy_from_slice(&flags.to_le_bytes());
            }
            m
        };

        // 1) `buf == 0`: only count → returns 2 (the two reportable rows), sending no request.
        let (mut state, _idx) = setup();
        state.current_message = msg(0, 0, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getvfsstat),
            SyscallResult::Suspend,
            "就地收尾也报挂起（回复已入队）"
        );
        assert!(state.pending_fs.is_none(), "只数个数不该打扰 FS");
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((Endpoint::from_generation_slot(1, 0), 2)),
            "C 返回的是**个数**"
        );

        // 2) A buffer is given (room for two entries): start the sequence — the first entry's REQ_STATVFS is registered.
        let (mut state, _idx) = setup();
        state.current_message = msg(0x5000, 2 * minix_types::STATVFS_SIZE as u64, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getvfsstat),
            SyscallResult::Suspend
        );
        let p = state.pending_fs.as_ref().expect("第一格 REQ_STATVFS");
        assert_eq!(p.req.m_type, minix_types::REQ_STATVFS);
        assert_eq!(p.fs_e, Endpoint::MFS, "先填列表里的第一行");

        // 3) The buffer holds only one entry: the sequence is truncated to 1 (C's `if (bufsize < sizeof) break`).
        let (mut state, _idx) = setup();
        state.current_message = msg(0x5000, minix_types::STATVFS_SIZE as u64, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getvfsstat),
            SyscallResult::Suspend
        );
        let idx = state.current_worker.expect("槽");
        assert!(matches!(
            state.worker_pool.get(idx).map(|w| w.cont),
            Some(Some(crate::worker::WorkerCont::Statvfs { seq_count: 1, .. }))
        ));

        // 4) Sequence progress: the first entry's reply arrives → the second entry's
        // request is sent (the buffer advances by sizeof); the second reply arrives →
        // finish with the **count** 2.
        {
            use minix_types::statvfs_off as off;
            let (mut state, idx) = setup();
            state.current_message = msg(0x5000, 2 * minix_types::STATVFS_SIZE as u64, 0);
            assert_eq!(
                dispatch_syscall(&mut state, VfsCallNum::Getvfsstat),
                SyscallResult::Suspend
            );
            assert_eq!(
                state.pending_fs.as_ref().map(|p| p.fs_e),
                Some(Endpoint::MFS)
            );
            // First entry's reply (the FS wrote the statistics into the buffer).
            state.statvfs_buf.set_u64(off::BSIZE, 1024);
            state.pending_fs = None;
            {
                let wp = state.worker_pool.get_mut(idx).unwrap();
                wp.sendrec = Some(Message { m_type: 0, ..Message::default() });
                wp.task = None;
                wp.state = crate::worker::WorkerState::Busy;
            }
            state.run_worker_continuations();
            // The copy is unreachable on the host → the sequence aborts in place (returns EIO) and never advances to the second entry.
            assert_eq!(
                state.take_reply().map(|(t, m)| (t, m.m_type)),
                Some((Endpoint::from_generation_slot(1, 0), -minix_types::EIO)),
                "拷贝失败即中止序列（C 的 `return r`）"
            );
            assert!(state.pending_fs.is_none(), "中止后不该再发请求");
        }

        // 5) The buffer does not even hold one entry → finish in place returning 0 (C's loop breaks on the first iteration).
        let (mut state, _idx) = setup();
        state.current_message = msg(0x5000, 8, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getvfsstat),
            SyscallResult::Suspend
        );
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((Endpoint::from_generation_slot(1, 0), 0))
        );
    }

    /// The Statvfs family (C `do_statvfs`/`do_fstatvfs` stadir.c:294-442 +
    /// `fill_statvfs`/`update_statvfs`): the fresh path makes a direct grant on the
    /// VFS-side buffer and sends `REQ_STATVFS` (the FS fills the whole statistics
    /// block); `ST_NOWAIT` **sends no request** and uses the mount-row cache directly.
    /// Both paths finally add the local fields (read-only bit, `f_fsid`, the three
    /// names) and copy the whole block to the user — under the host build that copy is
    /// unreachable, so EIO is returned truthfully.
    #[test]
    fn test_dispatch_fstatvfs_fresh_and_nowait() {
        use minix_types::Endpoint;

        let setup = |readonly: bool| {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            {
                let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(0)).unwrap();
                if readonly {
                    v.flags = crate::vmnt::VmntFlags::READONLY;
                }
                v.mount_path = "/".to_string();
                v.mount_dev = "/dev/root".to_string();
                v.fstype = "mfs".to_string();
            }
            let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
            state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap()
                .filps[3] = Some(fid.get());
            let vid = state.vnode_table.alloc().unwrap();
            {
                let v = state.vnode_table.get_mut(vid).unwrap();
                v.fs = Endpoint::MFS;
                v.ino = 0x42;
                v.mode = crate::open::S_IFREG | 0o644;
                v.dev = 1;
                v.ref_count = 1;
            }
            state.filp_table.get_mut(fid).unwrap().vnode = Some(vid.get());
            let idx = state
                .worker_pool
                .assign_first_fit(
                    minix_types::UserSlot::new(0),
                    crate::worker::WorkerFunc::DoWork,
                    &Message::default(),
                )
                .expect("空闲槽");
            state.current_worker = Some(idx);
            (state, idx)
        };
        let fstatvfs_msg = |fd: i32, flags: i32, buf: u64| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Fstatvfs1 as i32,
                ..Message::default()
            };
            // SAFETY: mess_lc_vfs_statvfs1: fd@0, flags@4, len@8, name@16, buf@24.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[4..8].copy_from_slice(&flags.to_le_bytes());
                raw[24..32].copy_from_slice(&buf.to_le_bytes());
            }
            m
        };

        // 1) Negative fd → EBADF.
        let (mut state, _idx) = setup(false);
        state.current_message = fstatvfs_msg(-1, 0, 0x5000);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fstatvfs1),
            SyscallResult::Error(minix_types::EBADF)
        );

        // 2) Fresh path: send REQ_STATVFS with the grant pointing at the VFS-side buffer.
        let (mut state, idx) = setup(false);
        state.current_message = fstatvfs_msg(3, 0, 0x5000);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fstatvfs1),
            SyscallResult::Suspend
        );
        let p = state.pending_fs.as_ref().expect("已登记 REQ_STATVFS");
        assert_eq!(p.req.m_type, minix_types::REQ_STATVFS);
        // SAFETY(test): the grant is in the first slot of the payload area.
        let grant = unsafe { i32::from_le_bytes(p.req.m_u.raw[0..4].try_into().unwrap()) };
        assert_eq!(grant, p.grant);
        assert!(matches!(
            state.worker_pool.get_mut(idx).unwrap().cont,
            Some(crate::worker::WorkerCont::Statvfs { user_buf: 0x5000, .. })
        ));

        // 3) Reply arrives: the FS has already written the statistics into the buffer
        // (the test simulates it by writing the buffer directly) → the cache is written
        // back to the mount row + local fields filled in; copying to the user is
        // unreachable on the host → EIO.
        {
            use minix_types::statvfs_off as off;
            let b = &mut state.statvfs_buf;
            b.set_u64(off::BSIZE, 4096);
            b.set_u64(off::BLOCKS, 1000);
            b.set_u64(off::NAMEMAX, 60);
        }
        state.pending_fs = None;
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.sendrec = Some(Message { m_type: 0, ..Message::default() });
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        let v = state.vmnt_table.get(crate::vmnt::VmntId(0)).unwrap();
        assert_eq!(v.stats.f_bsize, 4096, "FS 的统计量进了挂载行缓存");
        assert_eq!(v.stats.f_blocks, 1000);
        assert_eq!(v.stats.f_namemax, 60);
        {
            use minix_types::statvfs_off as off;
            let b = state.statvfs_buf;
            assert_eq!(b.get_u64(off::FSID), 1, "f_fsid 是挂载设备号");
            assert_eq!(b.get_name(off::FSTYPENAME), "mfs");
            assert_eq!(b.get_name(off::MNTONNAME), "/");
            assert_eq!(b.get_name(off::MNTFROMNAME), "/dev/root");
        }
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((Endpoint::from_generation_slot(1, 0), -minix_types::EIO)),
            "宿主下 sys_datacopy 不可达 → EIO（如实，不假装拷成功）"
        );

        // 4) ST_NOWAIT: send no request, use the cache directly (and add the read-only bit into f_flag).
        let (mut state, _idx) = setup(true);
        {
            let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(0)).unwrap();
            v.stats.f_bsize = 512;
            v.stats.f_blocks = 77;
        }
        state.current_message = fstatvfs_msg(3, minix_types::ST_NOWAIT, 0x5000);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fstatvfs1),
            SyscallResult::Suspend,
            "缓存分支就地收尾，也报挂起（回复已入队）"
        );
        assert!(state.pending_fs.is_none(), "ST_NOWAIT 不该打扰 FS");
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((Endpoint::from_generation_slot(1, 0), -minix_types::EIO)),
            "拷贝在宿主下不可达 → EIO"
        );
        {
            use minix_types::statvfs_off as off;
            let b = state.statvfs_buf;
            assert_eq!(b.get_u64(off::BSIZE), 512, "统计量来自缓存");
            assert_eq!(b.get_u64(off::BLOCKS), 77);
            assert_eq!(
                b.get_u64(off::FLAG) & minix_types::ST_RDONLY,
                minix_types::ST_RDONLY,
                "只读挂载位补进 f_flag"
            );
        }
    }

    /// The `Chdir`/`Chroot`/`Fchdir` arm (C stadir.c:32-107, all reduce to
    /// `change_into`): chroot is superuser-only; fchdir takes the vnode from the fd;
    /// the three directory-change gates (same vnode → success / non-directory →
    /// ENOTDIR / not searchable → EACCES) are decided locally, **with no FS round trip**.
    #[test]
    fn test_dispatch_chdir_fchdir_chroot() {
        use minix_types::Endpoint;

        let setup = |eff_uid: u32, dir_mode: u32, dir_uid: u32| {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            {
                let fp = state
                    .fproc_table
                    .get_mut(minix_types::UserSlot::new(0))
                    .unwrap();
                fp.eff_uid = eff_uid;
                fp.eff_gid = 100;
                fp.real_uid = eff_uid;
                fp.real_gid = 100;
            }
            // fd 3 → the target directory vnode.
            let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
            state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap()
                .filps[3] = Some(fid.get());
            let vid = state.vnode_table.alloc().unwrap();
            {
                let v = state.vnode_table.get_mut(vid).unwrap();
                v.fs = Endpoint::MFS;
                v.ino = 0x77;
                v.mode = dir_mode;
                v.uid = dir_uid;
                v.gid = 100;
                v.ref_count = 1;
            }
            state.filp_table.get_mut(fid).unwrap().vnode = Some(vid.get());
            (state, vid)
        };
        let fchdir_msg = |fd: i32| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Fchdir as i32,
                ..Message::default()
            };
            // SAFETY: `mess_lc_vfs_fchdir { int fd; }` (ipc.h:640-644).
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
            }
            m
        };

        // 1) chroot by a non-superuser → EPERM (before any path fetch).
        let (mut state, _vid) = setup(1000, crate::open::S_IFDIR | 0o755, 1000);
        state.current_message = Message {
            m_source: Endpoint::from_generation_slot(1, 0),
            m_type: VfsCallNum::Chroot as i32,
            ..Message::default()
        };
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Chroot),
            SyscallResult::Error(minix_types::EPERM)
        );

        // 2) fchdir with negative fd / empty slot → EBADF.
        let (mut state, _vid) = setup(1000, crate::open::S_IFDIR | 0o755, 1000);
        state.current_message = fchdir_msg(-1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchdir),
            SyscallResult::Error(minix_types::EBADF)
        );
        state.current_message = fchdir_msg(9);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchdir),
            SyscallResult::Error(minix_types::EBADF)
        );

        // 3) fchdir pointing at a regular file → ENOTDIR (`change_into`'s type gate).
        let (mut state, _vid) = setup(1000, crate::open::S_IFREG | 0o644, 1000);
        state.current_message = fchdir_msg(3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchdir),
            SyscallResult::Ok(minix_types::ENOTDIR)
        );

        // 4) fchdir pointing at a 0700 directory owned by the caller → succeeds, wd points at the new vnode.
        let (mut state, vid) = setup(1000, crate::open::S_IFDIR | 0o700, 1000);
        state.current_message = fchdir_msg(3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchdir),
            SyscallResult::Ok(0)
        );
        assert_eq!(
            state
                .fproc_table
                .get(minix_types::UserSlot::new(0))
                .unwrap()
                .work_dir,
            Some(vid.get()),
            "当前目录已换"
        );

        // 5) The same directory again → immediate success (C's `if (*result == vp)`).
        state.current_message = fchdir_msg(3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchdir),
            SyscallResult::Ok(0)
        );

        // 6) fchdir pointing at **someone else's** 0700 directory → EACCES (the search-bit gate).
        let (mut state, _vid) = setup(2000, crate::open::S_IFDIR | 0o700, 1000);
        state.current_message = fchdir_msg(3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchdir),
            SyscallResult::Ok(minix_types::EACCES)
        );
    }

    /// The `Sync`/`Fsync` arm (C `do_sync` misc.c:276-296 / `do_fsync`
    /// misc.c:229-267): fsync takes the vnode's device number through the fd and
    /// filters mounts by device; with no mount to sync it **succeeds directly** (C's
    /// loop sent nothing); otherwise it starts the sequence (each REQ_SYNC suspends
    /// its segment).
    #[test]
    fn test_dispatch_sync_and_fsync() {
        use minix_types::Endpoint;

        let setup = |with_mount: bool| {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            let vid = state.vnode_table.find_by_ino(Endpoint::MFS, 1).unwrap();
            if with_mount {
                let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(0)).unwrap();
                v.root = Some(vid.get());
            }
            let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
            state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap()
                .filps[3] = Some(fid.get());
            let vfid = state.vnode_table.alloc().unwrap();
            {
                let v = state.vnode_table.get_mut(vfid).unwrap();
                v.fs = Endpoint::MFS;
                v.ino = 0x42;
                v.mode = crate::open::S_IFREG | 0o644;
                v.dev = 1; // same as the mount row's dev
                v.ref_count = 1;
            }
            state.filp_table.get_mut(fid).unwrap().vnode = Some(vfid.get());
            let idx = state
                .worker_pool
                .assign_first_fit(
                    minix_types::UserSlot::new(0),
                    crate::worker::WorkerFunc::DoWork,
                    &Message::default(),
                )
                .expect("空闲槽");
            state.current_worker = Some(idx);
            (state, idx)
        };
        let fsync_msg = |fd: i32| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Fsync as i32,
                ..Message::default()
            };
            // SAFETY: `mess_lc_vfs_fsync { int fd; }` (ipc.h:673-677).
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
            }
            m
        };

        // 1) fsync with a negative fd → EBADF.
        let (mut state, _idx) = setup(true);
        state.current_message = fsync_msg(-1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fsync),
            SyscallResult::Error(minix_types::EBADF)
        );

        // 2) fsync with a valid fd but no mount for that device → succeeds directly (C's loop sent nothing).
        let (mut state, _idx) = setup(false);
        state.current_message = fsync_msg(3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fsync),
            SyscallResult::Ok(0)
        );
        assert!(state.pending_fs.is_none());

        // 3) fsync with a mount → start the sequence (suspend; the first REQ_SYNC is registered).
        let (mut state, _idx) = setup(true);
        state.current_message = fsync_msg(3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fsync),
            SyscallResult::Suspend
        );
        let p = state.pending_fs.as_ref().expect("第一条 REQ_SYNC");
        assert_eq!(p.req.m_type, minix_types::REQ_SYNC);
        assert_eq!(p.fs_e, Endpoint::MFS);

        // 4) sync (all mounts), same shape: only one valid mount row here.
        let (mut state, _idx) = setup(true);
        state.current_message = Message {
            m_source: Endpoint::from_generation_slot(1, 0),
            m_type: VfsCallNum::Sync as i32,
            ..Message::default()
        };
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Sync),
            SyscallResult::Suspend
        );
        assert_eq!(
            state.pending_fs.as_ref().map(|p| p.req.m_type),
            Some(minix_types::REQ_SYNC)
        );
    }

    /// The `Symlink` arm: two length gates (`<= 1` → ENOENT, `>= 255` →
    /// ENAMETOOLONG, C link.c:400-401) before any path fetch; after the gates it
    /// walks the parent directory.
    #[test]
    fn test_dispatch_symlink_length_gates() {
        use minix_types::Endpoint;

        let slink_msg = |target_len: u64| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Symlink as i32,
                ..Message::default()
            };
            // SAFETY: mess_lc_vfs_link: name1@0 (target), name2@8 (link path),
            // len1@16, len2@24. Both the target string and the link path live in user memory (cross-space fetch).
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..8].copy_from_slice(&0x6000u64.to_le_bytes());
                raw[8..16].copy_from_slice(&0x7000u64.to_le_bytes());
                raw[16..24].copy_from_slice(&target_len.to_le_bytes());
                raw[24..32].copy_from_slice(&4u64.to_le_bytes());
            }
            m
        };

        let mut state = seeded(100);
        // 1) Target length 0 and 1 → ENOENT.
        for len in [0u64, 1] {
            state.current_message = slink_msg(len);
            assert_eq!(
                dispatch_syscall(&mut state, VfsCallNum::Symlink),
                SyscallResult::Error(minix_types::ENOENT),
                "len={len} 太短"
            );
        }
        // 2) Target length >= 255 → ENAMETOOLONG.
        state.current_message = slink_msg(crate::link::POSIX_SYMLINK_MAX as u64);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Symlink),
            SyscallResult::Error(minix_types::ENAMETOOLONG)
        );
        // 3) Legal length → passes the gates, stops at the cross-space fetch of the link path (unreachable on the host → EINVAL).
        state.current_message = slink_msg(10);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Symlink),
            SyscallResult::Error(minix_types::EINVAL)
        );
    }

    /// The gates and time conversion of the `Utimens` arm (C `do_utimens` time.c:44-160):
    /// the fd half requires flags 0, the path half accepts only `AT_SYMLINK_NOFOLLOW`;
    /// the ownership gate (EPERM) **degrades to a write-permission check** when both
    /// nanoseconds are `UTIME_NOW`; nanoseconds >= 1e9 is EINVAL (except the
    /// `UTIME_NOW`/`UTIME_OMIT` sentinels).
    #[test]
    fn test_dispatch_utimens_gates_and_nsec_validation() {
        use minix_types::Endpoint;

        let utimens_msg = |fd: i32, name: u64, flags: u32, ansec: i64, mnsec: i64| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Utimens as i32,
                ..Message::default()
            };
            // SAFETY: mess_vfs_utimens: atime@0, mtime@8, ansec@16, mnsec@24,
            // len@32, name@40, fd@48, flags@52.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..8].copy_from_slice(&1000i64.to_le_bytes());
                raw[8..16].copy_from_slice(&2000i64.to_le_bytes());
                raw[16..24].copy_from_slice(&ansec.to_le_bytes());
                raw[24..32].copy_from_slice(&mnsec.to_le_bytes());
                raw[40..48].copy_from_slice(&name.to_le_bytes());
                raw[48..52].copy_from_slice(&fd.to_le_bytes());
                raw[52..56].copy_from_slice(&flags.to_le_bytes());
            }
            m
        };
        // Setup: fd 3 → filp → vnode (owner 1000).
        let setup = |eff_uid: u32, umask: u32| {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            let fid = state.filp_table.alloc_filp(crate::open::W_BIT).unwrap();
            state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap()
                .filps[3] = Some(fid.get());
            let vid = state.vnode_table.alloc().unwrap();
            {
                let v = state.vnode_table.get_mut(vid).unwrap();
                v.fs = Endpoint::MFS;
                v.ino = 0x99;
                v.mode = crate::open::S_IFREG | 0o644;
                v.uid = 1000;
                v.gid = 100;
                v.ref_count = 1;
            }
            state.filp_table.get_mut(fid).unwrap().vnode = Some(vid.get());
            {
                let fp = state
                    .fproc_table
                    .get_mut(minix_types::UserSlot::new(0))
                    .unwrap();
                fp.eff_uid = eff_uid;
                fp.eff_gid = 100;
                fp.umask = umask;
            }
            let idx = state
                .worker_pool
                .assign_first_fit(
                    minix_types::UserSlot::new(0),
                    crate::worker::WorkerFunc::DoWork,
                    &Message::default(),
                )
                .expect("空闲槽");
            state.current_worker = Some(idx);
            (state, idx)
        };

        // 1) fd half + non-zero flags → EINVAL (C time.c:103-104).
        let (mut state, _idx) = setup(1000, 0);
        state.current_message = utimens_msg(3, 0, 1, 0, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Utimens),
            SyscallResult::Error(minix_types::EINVAL)
        );

        // 2) Owner + explicit times → REQ_UTIME (seconds and nanoseconds carried separately).
        let (mut state, idx) = setup(1000, 0);
        state.current_message = utimens_msg(3, 0, 0, 111, 222);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Utimens),
            SyscallResult::Suspend
        );
        let p = state.pending_fs.as_ref().expect("已登记 REQ_UTIME");
        assert_eq!(p.req.m_type, minix_types::REQ_UTIME);
        // SAFETY(test): read back the five fields using utime_req_off.
        unsafe {
            let raw = &p.req.m_u.raw;
            let ino = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let actime = i64::from_le_bytes(raw[8..16].try_into().unwrap());
            let modtime = i64::from_le_bytes(raw[16..24].try_into().unwrap());
            let acnsec = u32::from_le_bytes(raw[24..28].try_into().unwrap());
            let modnsec = u32::from_le_bytes(raw[28..32].try_into().unwrap());
            assert_eq!((ino, actime, modtime), (0x99, 1000, 2000));
            assert_eq!((acnsec, modnsec), (111, 222));
        }
        // The reply is only a status.
        state.pending_fs = None;
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.sendrec = Some(Message { m_type: 0, ..Message::default() });
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((Endpoint::from_generation_slot(1, 0), 0))
        );

        // 3) Non-owner + both explicit times → EPERM (only the owner or the superuser may change times).
        let (mut state, _idx) = setup(2000, 0);
        state.current_message = utimens_msg(3, 0, 0, 111, 222);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Utimens),
            SyscallResult::Suspend
        );
        assert!(state.pending_fs.is_none());
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((Endpoint::from_generation_slot(1, 0), -minix_types::EPERM))
        );

        // 4) Nanoseconds out of range (>= 1e9 and not a sentinel) → EINVAL.
        let (mut state, _idx) = setup(1000, 0);
        state.current_message = utimens_msg(3, 0, 0, 1_000_000_000, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Utimens),
            SyscallResult::Suspend
        );
        assert!(state.pending_fs.is_none());
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((Endpoint::from_generation_slot(1, 0), -minix_types::EINVAL))
        );

        // 5) Non-owner + both UTIME_NOW (touch) → degrades to the write-permission
        // check: 0644's other has no write bit → EACCES (not EPERM).
        let (mut state, _idx) = setup(2000, 0);
        state.current_message = utimens_msg(
            3,
            0,
            0,
            crate::open::UTIME_NOW,
            crate::open::UTIME_NOW,
        );
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Utimens),
            SyscallResult::Suspend
        );
        assert!(state.pending_fs.is_none());
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((Endpoint::from_generation_slot(1, 0), -minix_types::EACCES)),
            "touch 的门是写权限，不是属主"
        );

        // 6) Path half + unknown flag → EINVAL (C time.c:78-79).
        let (mut state, _idx) = setup(1000, 0);
        state.current_message = utimens_msg(3, 0x5000, 0x4, 0, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Utimens),
            SyscallResult::Error(minix_types::EINVAL)
        );
    }

    /// The `Mknod` arm's gates and mode narrowing (C `do_mknod` open.c:514-556):
    /// only the superuser may create **non-FIFO** nodes (otherwise EPERM); mode =
    /// type bits | (permission bits & `!umask`).
    #[test]
    fn test_dispatch_mknod_superuser_gate_and_mode_narrowing() {
        use minix_types::Endpoint;

        let mknod_msg = |mode: u32, dev: u64| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Mknod as i32,
                ..Message::default()
            };
            // SAFETY: mknod payload device@0, name@8, len@16, mode@24
            // (ipc.h:736-744); the path is fetched across spaces via SysPathFetcher
            // (unreachable on the host), so only the gates and mode are tested here —
            // passing them stops at the path fetch with EINVAL.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..8].copy_from_slice(&dev.to_le_bytes());
                raw[16..24].copy_from_slice(&3u64.to_le_bytes());
                raw[24..28].copy_from_slice(&mode.to_le_bytes());
            }
            m
        };

        // 1) Non-superuser + regular file → EPERM (before any path fetch).
        let mut state = seeded(100);
        {
            let fp = state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap();
            fp.eff_uid = 1000;
        }
        state.current_message = mknod_msg(crate::open::S_IFREG | 0o644, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Mknod),
            SyscallResult::Error(minix_types::EPERM)
        );

        // 2) Non-superuser + FIFO → passes the gate (anyone may create a FIFO); under
        // the host build the path fetch is unreachable → EINVAL (`SysPathFetcher`'s failure surface).
        state.current_message = mknod_msg(crate::open::S_IFIFO | 0o644, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Mknod),
            SyscallResult::Error(minix_types::EINVAL),
            "FIFO 过超级用户门，停在跨空间取路径"
        );

        // 3) Superuser + character device: mode narrowing follows umask, dev rides
        // along (here only "not blocked by EPERM" is verifiable — the full shape is
        // pinned by the Done arm's test).
        let mut state = seeded(100);
        {
            let fp = state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap();
            fp.eff_uid = crate::link::SU_UID;
            fp.umask = 0o022;
        }
        state.current_message = mknod_msg(crate::open::S_IFCHR | 0o666, 0x0301);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Mknod),
            SyscallResult::Error(minix_types::EINVAL),
            "root 过门，停在跨空间取路径"
        );
    }

    /// **umask narrowing** of the node-creation mode (creat at C `open.c:109`,
    /// mkdir at `do_mkdir:583`): the Rust side stores `fproc.umask` as the **raw
    /// mask** and the formula is `mode & 0777 & !umask` (C stores the complement and
    /// writes `& fp_umask` — the two conventions differ; the Rust side's result is
    /// pinned here). The default umask 0 must **keep everything** (it was once
    /// written as `& umask`, which made newly created files/directories have
    /// permission bits of 0).
    #[test]
    fn test_create_and_mkdir_apply_umask_as_narrowing() {
        use minix_types::Endpoint;

        let path_msg = |call: VfsCallNum, mode: u32| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: call as i32,
                ..Message::default()
            };
            // SAFETY: mess_lc_vfs_path: name@0, len@8, flags@16, mode@20,
            // inline path@24. Creat must carry O_CREAT (C `do_creat`'s gate; absent
            // is EINVAL); Mkdir ignores flags.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[8..16].copy_from_slice(&3u64.to_le_bytes());
                raw[16..20].copy_from_slice(
                    &crate::open::OpenFlags::CREAT.bits().to_le_bytes(),
                );
                raw[20..24].copy_from_slice(&mode.to_le_bytes());
                raw[24] = b'/';
                raw[25] = b'x';
                raw[26] = 0;
            }
            m
        };
        let follow_mode = |state: &VfsState| -> u32 {
            let wp = state
                .worker_pool
                .get(state.current_worker.unwrap())
                .expect("槽");
            match wp.path.as_ref().map(|p| &p.follow) {
                Some(crate::worker::PathFollow::Mkdir { mode, .. }) => *mode,
                Some(crate::worker::PathFollow::Creat { mode, .. }) => *mode,
                other => panic!("follow 不对：{other:?}"),
            }
        };
        let run = |call: VfsCallNum, mode: u32, umask: u32| -> u32 {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            {
                let fp = state
                    .fproc_table
                    .get_mut(minix_types::UserSlot::new(0))
                    .unwrap();
                fp.umask = umask;
            }
            let idx = state
                .worker_pool
                .assign_first_fit(
                    minix_types::UserSlot::new(0),
                    crate::worker::WorkerFunc::DoWork,
                    &Message::default(),
                )
                .expect("空闲槽");
            state.current_worker = Some(idx);
            state.current_message = path_msg(call, mode);
            assert_eq!(
                dispatch_syscall(&mut state, call),
                SyscallResult::Suspend,
                "{call:?} 起走：挂起等 FS"
            );
            follow_mode(&state)
        };

        // umask 0 (the default for a new process): keep everything.
        assert_eq!(run(VfsCallNum::Mkdir, 0o777, 0), crate::open::S_IFDIR | 0o777);
        // umask 022: narrow bitwise (not "keep only the bits in the mask").
        assert_eq!(run(VfsCallNum::Mkdir, 0o777, 0o022), crate::open::S_IFDIR | 0o755);
        // creat's narrowing formula is isomorphic to mkdir's (`S_IFREG | mode & 0o777 & !umask`,
        // open.c:109), but the creat arm always fetches the path (C do_creat:72-74; the
        // wire has no inline field) — the continuation is unreachable on the host, and
        // the earlier "inline forged path reaching the continuation" was fake coverage
        // (on the real wire the client's CreatePayload never writes buf, so the arm
        // would decode a garbage NUL name). What the host can observe is the fetch
        // failure surface EINVAL; the real-hardware half of narrowing is deferred to T2.
        let mut state = seeded(100);
        crate::main_loop::seed_ready_state(&mut state);
        let idx = state
            .worker_pool
            .assign_first_fit(
                minix_types::UserSlot::new(0),
                crate::worker::WorkerFunc::DoWork,
                &Message::default(),
            )
            .expect("空闲槽");
        state.current_worker = Some(idx);
        let mut m = path_msg(VfsCallNum::Creat, 0o666);
        // SAFETY: name@0 lane (the fetch source address; no kernel reply on the host).
        unsafe {
            m.m_u.raw[0..8].copy_from_slice(&(0x7000u64).to_le_bytes());
        }
        state.current_message = m;
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Creat),
            SyscallResult::Error(minix_types::EINVAL),
            "creat 恒 fetch：宿主失败面"
        );
    }

    /// The reply payload on the **position-unchanged** path of `Lseek` must not be
    /// overwritten by the unified finish: C `do_lseek` writes the new position into
    /// `m_vfs_lc_lseek.offset` (open.c:665-666) and returns OK, and both exits (with
    /// or without the read-ahead inhibit) must carry it. This goes through the full
    /// `run_once` (not just dispatch), because the overwrite happens in `run_once`'s
    /// trailing unified `queue_reply`.
    #[test]
    fn test_lseek_unchanged_position_keeps_payload_through_run_once() {
        use minix_types::Endpoint;

        let mut state = seeded(100);
        // filp: position 100, a regular file.
        let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
        state
            .fproc_table
            .get_mut(minix_types::UserSlot::new(0))
            .unwrap()
            .filps[3] = Some(fid.get());
        let vid = state.vnode_table.alloc().unwrap();
        {
            let v = state.vnode_table.get_mut(vid).unwrap();
            v.fs = Endpoint::MFS;
            v.ino = 9;
            v.mode = crate::open::S_IFREG | 0o644;
            v.size = 500;
            v.ref_count = 1;
        }
        {
            let f = state.filp_table.get_mut(fid).unwrap();
            f.vnode = Some(vid.get());
            f.pos = 100;
        }
        // lseek(3, 100, SEEK_SET): new position == current position → don't disturb the FS.
        let mut m = Message {
            m_source: Endpoint::from_generation_slot(1, 0),
            m_type: VfsCallNum::Lseek as i32,
            ..Message::default()
        };
        // SAFETY: lseek payload offset@0, fd@8, whence@12 (the m_m7 overlay).
        unsafe {
            let raw = &mut m.m_u.raw;
            raw[0..8].copy_from_slice(&100i64.to_le_bytes());
            raw[8..12].copy_from_slice(&3i32.to_le_bytes());
            raw[12..16].copy_from_slice(&0i32.to_le_bytes());
        }
        let codec = crate::main_loop::VfsTransIdCodec;
        let _ = state.run_once(&m, &codec);
        let (target, reply) = state.take_reply().expect("回复");
        assert_eq!(target, Endpoint::from_generation_slot(1, 0));
        assert_eq!(reply.m_type, 0, "C 的 do_lseek 回 OK");
        // SAFETY(test): the offset is in the first slot of the payload area (mess_vfs_lc_lseek).
        let offset = unsafe { i64::from_le_bytes(reply.m_u.raw[0..8].try_into().unwrap()) };
        assert_eq!(offset, 100, "新位置必须留在回复载荷里");
    }

    /// The `Fchmod` arm (fd half): fd gate (only fd validity is checked, **not the
    /// open mode** — unlike ftruncate's W-bit gate, C `do_chmod:88-93`) → ownership
    /// gate (EPERM) → read-only gate (EROFS) → clear setgid → `REQ_CHMOD`; the reply
    /// carries the full mode word and it is written back to the cache.
    #[test]
    fn test_dispatch_fchmod_gates_request_and_cache() {
        use minix_types::Endpoint;

        let fchmod_msg = |fd: i32, mode: u32| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Fchmod as i32,
                ..Message::default()
            };
            // SAFETY: fchmod payload fd@0, mode@4 (ipc.h:647-652). @8 is filled with
            // a decoy value: mode may only be taken from @4, and a misaligned read
            // would make case 4's mode assertion fail on the spot.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[4..8].copy_from_slice(&mode.to_le_bytes());
                raw[8..12].copy_from_slice(&0o777u32.to_le_bytes());
            }
            m
        };
        let user_e = Endpoint::from_generation_slot(1, 0);
        // Setup: fd 3 → filp (opened read-only) → vnode (owner 1000, group 100, 0644).
        let setup = |eff_uid: u32, eff_gid: u32, readonly_fs: bool| {
            let mut state = seeded(100);
            {
                let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(0)).unwrap();
                v.fs = Endpoint::MFS;
                v.dev = 1;
                if readonly_fs {
                    v.flags = crate::vmnt::VmntFlags::READONLY;
                }
            }
            // [SD19-gate] Best-effort grant registration (failure takes the existing error reply path)
            #[allow(clippy::let_underscore_must_use)]
            let _ = state.grants.grant_direct(
                &minix_sys::syscall::DirectKernelCallTransport,
                Endpoint::MFS.get(),
                0x1000,
                8,
                minix_types::CpFlags::READ,
            );
            let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
            state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap()
                .filps[3] = Some(fid.get());
            let vid = state.vnode_table.alloc().unwrap();
            {
                let v = state.vnode_table.get_mut(vid).unwrap();
                v.fs = Endpoint::MFS;
                v.ino = 0x21;
                v.mode = crate::open::S_IFREG | 0o644;
                v.uid = 1000;
                v.gid = 100;
                v.ref_count = 1;
            }
            state.filp_table.get_mut(fid).unwrap().vnode = Some(vid.get());
            {
                let fp = state
                    .fproc_table
                    .get_mut(minix_types::UserSlot::new(0))
                    .unwrap();
                fp.eff_uid = eff_uid;
                fp.eff_gid = eff_gid;
            }
            let idx = state
                .worker_pool
                .assign_first_fit(
                    minix_types::UserSlot::new(0),
                    crate::worker::WorkerFunc::DoWork,
                    &Message::default(),
                )
                .expect("空闲槽");
            state.current_worker = Some(idx);
            (state, vid, idx)
        };

        // 1) Negative fd and empty slot → EBADF.
        let (mut state, _vid, _idx) = setup(1000, 100, false);
        state.current_message = fchmod_msg(-1, 0o600);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchmod),
            SyscallResult::Error(minix_types::EBADF)
        );
        state.current_message = fchmod_msg(9, 0o600);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchmod),
            SyscallResult::Error(minix_types::EBADF)
        );

        // 2) Non-owner → EPERM (the finish path always reports suspension; the reply is queued).
        let (mut state, _vid, _idx) = setup(2000, 100, false);
        state.current_message = fchmod_msg(3, 0o600);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchmod),
            SyscallResult::Suspend
        );
        assert!(state.pending_fs.is_none());
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user_e, -minix_types::EPERM))
        );

        // 3) Owner but read-only mount → EROFS.
        let (mut state, _vid, _idx) = setup(1000, 100, true);
        state.current_message = fchmod_msg(3, 0o600);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchmod),
            SyscallResult::Suspend
        );
        assert!(state.pending_fs.is_none());
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user_e, -minix_types::EROFS))
        );

        // 4) Owner + writable: the setgid bit is cleared (file group 100 == effective
        // group 100 → **not cleared**; make the effective group differ here so it is cleared).
        let (mut state, vid, idx) = setup(1000, 999, false);
        state.current_message = fchmod_msg(3, 0o2664);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchmod),
            SyscallResult::Suspend
        );
        let p = state.pending_fs.as_ref().expect("已登记 REQ_CHMOD");
        assert_eq!(p.req.m_type, minix_types::REQ_CHMOD);
        // SAFETY(test): read back inode/mode using chmod_req_off.
        unsafe {
            let raw = &p.req.m_u.raw;
            let ino = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let mode = u32::from_le_bytes(raw[8..12].try_into().unwrap());
            assert_eq!(ino, 0x21);
            assert_eq!(mode, 0o664, "不同组 → setgid 被清");
        }

        // The reply carries the full mode word (including the type bits) → written back to the cache.
        state.pending_fs = None;
        let mut reply = Message { m_type: 0, ..Message::default() };
        // SAFETY(test): fill in the new mode using chmod_reply_off.
        unsafe {
            reply.m_u.raw[0..4]
                .copy_from_slice(&(crate::open::S_IFREG | 0o600).to_le_bytes());
        }
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.sendrec = Some(reply);
            wp.task = None;
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(state.take_reply().map(|(t, m)| (t, m.m_type)), Some((user_e, 0)));
        assert_eq!(
            state.vnode_table.get(vid).unwrap().mode,
            crate::open::S_IFREG | 0o600
        );
    }

    /// The window gate of the `Readlink` arm (C link.c:486): `bufsize > SSIZE_MAX` is
    /// EINVAL, and this gate takes effect before the path fetch; after it, the path is
    /// always fetched (link.c:494) — under the host build the caller's memory is
    /// unreachable → EINVAL. The REQ_SHAPE observation for the `PATH_RET_SYMLINK` walk
    /// flag is deferred with the continuation to real hardware (T2).
    #[test]
    fn test_dispatch_readlink_buffer_gate() {
        use minix_types::Endpoint;

        let readlink_msg = |name_len: u64, buf_size: u64| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Readlink as i32,
                ..Message::default()
            };
            // SAFETY: readlink payload name@0, namelen@8, buf@16, bufsize@24
            // (ipc.h:785-792) — no inline field; the path is delivered only through the pointer lane.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..8].copy_from_slice(&(0x7000u64).to_le_bytes());
                raw[8..16].copy_from_slice(&name_len.to_le_bytes());
                raw[16..24].copy_from_slice(&0x5000u64.to_le_bytes());
                raw[24..32].copy_from_slice(&buf_size.to_le_bytes());
            }
            m
        };

        let mut state = seeded(100);
        state.current_message = readlink_msg(3, u64::MAX);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Readlink),
            SyscallResult::Error(minix_types::EINVAL)
        );
        // Legal window → reaches fetch: unreachable on the host → EINVAL.
        state.current_message = readlink_msg(3, 128);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Readlink),
            SyscallResult::Error(minix_types::EINVAL),
            "恒 fetch：宿主失败面"
        );
        assert!(state.pending_fs.is_none());
    }

    /// The `Readlink` arm has no inline window: the path length has no local gate (the
    /// earlier self-invented 24-byte window `READLINK_INLINE_MAX` has been removed,
    /// matching C's wire, which has no buf field) — a namelen within PATH_MAX always
    /// reaches fetch (host failure surface EINVAL), and only a length over PATH_MAX
    /// hits fetch's first gate, ENAMETOOLONG (matching utility.c:62-65).
    #[test]
    fn test_dispatch_readlink_no_inline_window_leniency() {
        use minix_types::Endpoint;

        let readlink_msg = |name_len: u64| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Readlink as i32,
                ..Message::default()
            };
            // SAFETY: readlink payload name@0, namelen@8, buf@16, bufsize@24.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..8].copy_from_slice(&(0x7000u64).to_le_bytes());
                raw[8..16].copy_from_slice(&name_len.to_le_bytes());
                raw[16..24].copy_from_slice(&0x5000u64.to_le_bytes());
                raw[24..32].copy_from_slice(&128u64.to_le_bytes());
            }
            m
        };

        // namelen=25 (the old window gate would return ENAMETOOLONG here): now it proceeds to fetch.
        let mut state = seeded(100);
        state.current_message = readlink_msg(25);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Readlink),
            SyscallResult::Error(minix_types::EINVAL),
            "超旧窗不再本地拒，fetch 宿主失败面"
        );
        assert!(state.pending_fs.is_none());

        // namelen > PATH_MAX: fetch's first gate ENAMETOOLONG.
        state.current_message = readlink_msg(crate::path::PATH_MAX as u64 + 1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Readlink),
            SyscallResult::Error(minix_types::ENAMETOOLONG)
        );
        assert!(state.pending_fs.is_none());
    }

    /// The bit-mask validation of the `Access` arm (C protect.c:216-217): apart from
    /// `F_OK`(0) only combinations of `R_OK|W_OK|X_OK` (4|2|1) are accepted — any
    /// extra bit is EINVAL, and this gate takes effect **before** the path fetch and
    /// the walk (don't put it after the permission check).
    #[test]
    fn test_dispatch_access_mode_gate() {
        use minix_types::Endpoint;

        let access_msg = |mode: u32| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Access as i32,
                ..Message::default()
            };
            // SAFETY: access payload has the same shape as open (name@0, len@8, mode@20,
            // inline path@24) — give a two-byte path so the path fetch and the walk
            // after the bit-mask gate are really executed.
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[8..16].copy_from_slice(&3u64.to_le_bytes());
                raw[20..24].copy_from_slice(&mode.to_le_bytes());
                raw[24] = b'/';
                raw[25] = b'x';
                raw[26] = 0; // fetch_name's length includes the trailing NUL (utility.c:60-90)
            }
            m
        };

        let mut state = seeded(100);
        // 0o10 = 8: a bit outside R/W/X → EINVAL.
        state.current_message = access_msg(0o10);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Access),
            SyscallResult::Error(minix_types::EINVAL)
        );
        // 0o7 (all of R|W|X) and 0 (F_OK) both pass the gate → start the walk
        // (suspend, waiting for the FS). The baseline must be laid out first: a mount
        // row + the caller's root-directory vnode + a warmed grant table (missing any
        // one ends with EIO, and the assertion would then have the wrong reason).
        crate::main_loop::seed_ready_state(&mut state);
        let idx = state
            .worker_pool
            .assign_first_fit(
                minix_types::UserSlot::new(0),
                crate::worker::WorkerFunc::DoWork,
                &Message::default(),
            )
            .expect("空闲槽");
        state.current_worker = Some(idx);
        for mode in [0o7u32, 0] {
            state.current_message = access_msg(mode);
            assert_eq!(
                dispatch_syscall(&mut state, VfsCallNum::Access),
                SyscallResult::Suspend,
                "mode={mode} 过位掩码门后起走遍历"
            );
            assert_eq!(
                state.pending_fs.as_ref().map(|p| p.req.m_type),
                Some(minix_types::REQ_LOOKUP),
                "首条 lookup 已登记"
            );
            state.pending_fs = None;
        }
    }

    /// The over-inline branch of the six path-family arms (mkdir/chmod/access/chdir/
    /// unlink/chroot) reaches fetch: under the host build the caller's memory is
    /// unreachable → EINVAL (`SysPathFetcher` failure surface); a length > PATH_MAX
    /// hits fetch's first gate, ENAMETOOLONG (matching utility.c:62-65). chroot also
    /// has the superuser gate before the fetch, so eff_uid=0 is set up here first.
    #[test]
    fn test_dispatch_path_family_long_name_fetch_faces() {
        use minix_types::Endpoint;

        let path_msg = |call: VfsCallNum, mode: u32, name_len: u64| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: call as i32,
                ..Message::default()
            };
            // SAFETY: all six families share `mess_lc_vfs_path` — name@0, len@8,
            // flags@16, mode@20; an over-inline name writes no buf (pointer lane, server-side fetch).
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..8].copy_from_slice(&(0x7000u64).to_le_bytes());
                raw[8..16].copy_from_slice(&name_len.to_le_bytes());
                raw[20..24].copy_from_slice(&mode.to_le_bytes());
            }
            m
        };

        // mkdir: 33 bytes (over the inline limit, within PATH_MAX) → fetch failure surface EINVAL.
        let mut state = seeded(100);
        state.current_message = path_msg(VfsCallNum::Mkdir, 0o755, 33);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Mkdir),
            SyscallResult::Error(minix_types::EINVAL)
        );
        assert!(state.pending_fs.is_none());

        // chmod: also reaches fetch.
        state.current_message = path_msg(VfsCallNum::Chmod, 0o644, 33);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Chmod),
            SyscallResult::Error(minix_types::EINVAL)
        );

        // unlink: > PATH_MAX hits fetch's first gate ENAMETOOLONG.
        state.current_message = path_msg(VfsCallNum::Unlink, 0, crate::path::PATH_MAX as u64 + 1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Unlink),
            SyscallResult::Error(minix_types::ENAMETOOLONG)
        );

        // chdir: same two branches.
        state.current_message = path_msg(VfsCallNum::Chdir, 0, 33);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Chdir),
            SyscallResult::Error(minix_types::EINVAL)
        );

        // access: the bit-mask gate (protect.c:216-217) still takes effect before the
        // path fetch, and fetch comes after it.
        state.current_message = path_msg(VfsCallNum::Access, 0o644, 33);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Access),
            SyscallResult::Error(minix_types::EINVAL),
            "位掩码门先行"
        );
        state.current_message = path_msg(VfsCallNum::Access, 4 | 2, 33);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Access),
            SyscallResult::Error(minix_types::EINVAL)
        );

        // chroot: the superuser gate comes before the path fetch (the do_chroot gate
        // order in stadir.c); after setting eff_uid=0 it reaches fetch.
        {
            let fp = state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap();
            fp.eff_uid = crate::link::SU_UID;
        }
        state.current_message = path_msg(VfsCallNum::Chroot, 0, 33);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Chroot),
            SyscallResult::Error(minix_types::EINVAL)
        );
    }

    /// The `Fstat` arm (conversation arm template): an invalid fd/empty slot returns
    /// EBADF **before any I/O** (C `get_filp`'s gate); a missing mount window returns
    /// EIO (C `find_vmnt`); only after all gates does it reach the grant (under the
    /// host build `grant_magic` is unreachable → honest EIO, the same value as C's
    /// panic-on-grant-failure "internal error to the outside").
    #[test]
    fn test_dispatch_fstat_gates_then_grant() {
        use minix_types::Endpoint;

        let fstat_msg = |fd: i32, buf: u64| {
            let mut m = Message::default();
            m.m_source = Endpoint::from_generation_slot(1, 0);
            m.m_type = VfsCallNum::Fstat as i32;
            // SAFETY: fstat payload fd@0, buf@8 (same layout as minix-sys fstat_via).
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[8..16].copy_from_slice(&buf.to_le_bytes());
            }
            m
        };

        // Negative fd: C's `get_filp` returns EBADF.
        let mut state = seeded(100);
        state.current_message = fstat_msg(-1, 0x5000);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fstat),
            SyscallResult::Error(minix_types::EBADF)
        );

        // The fd has no corresponding filp → EBADF.
        state.current_message = fstat_msg(3, 0x5000);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fstat),
            SyscallResult::Error(minix_types::EBADF)
        );

        // Has a filp but the vnode is empty → EBADF (C's filp always has a vnode;
        // here it corresponds to "the thing the fd points at is already invalid").
        let fid = state.filp_table.alloc_filp(0o644).unwrap();
        state
            .fproc_table
            .get_mut(minix_types::UserSlot::new(0))
            .unwrap()
            .filps[3] = Some(fid.get());
        state.current_message = fstat_msg(3, 0x5000);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fstat),
            SyscallResult::Error(minix_types::EBADF)
        );

        // filp + vnode present: reaches the mount window and the grant — no vmnt gives EIO.
        let vid = state.vnode_table.alloc().unwrap();
        {
            let v = state.vnode_table.get_mut(vid).unwrap();
            v.fs = Endpoint::MFS;
            v.ino = 0x1234;
        }
        state.filp_table.get_mut(crate::filp::FilpId(fid.get())).unwrap().vnode = Some(vid.get());
        state.current_message = fstat_msg(3, 0x5000);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fstat),
            SyscallResult::Error(minix_types::EIO),
            "无 vmnt → C find_vmnt 失败的 EIO"
        );
    }

    /// The `VFS_GETSYSINFO` arm (the server-internal arm of the reply half): its
    /// three parts match C `do_getsysinfo` (misc.c:59-113) — the root gate
    /// (`fp_effuid == 0`, C's `super_user` macro glo.h:33) precedes the size gate,
    /// an unknown `what` returns `EINVAL`, and only after all gates does it reach the
    /// copy-out seam (under the host build `sys_datacopy` honestly returns `EFAULT`,
    /// see `SysCopyToUser`).
    #[test]
    fn test_dispatch_getsysinfo_gates_and_copy_stage() {
        use minix_types::{SI_DMAP_TAB, SI_PROC_TAB};

        let getsysinfo_msg = |what: i32, size: u64| {
            let mut m = Message::default();
            m.m_source = Endpoint::from_generation_slot(1, 0);
            m.m_type = VfsCallNum::Getsysinfo as i32;
            // SAFETY: m_lsys_getsysinfo is the payload area of VFS_GETSYSINFO.
            unsafe {
                let g = &mut m.m_u.m_lsys_getsysinfo;
                g.what = what;
                g.where_ = 0x4000;
                g.size = size;
            }
            m
        };
        let dmap_bytes =
            (core::mem::size_of::<minix_types::DmapSnap>() * minix_types::NR_DEVICES) as u64;
        let fproc_bytes =
            (core::mem::size_of::<minix_types::FProcSnap>() * minix_types::NR_PROCS) as u64;

        // Non-root (eff_uid != 0) → EPERM, and this happens before any copy.
        let mut state = seeded(100);
        state
            .fproc_table
            .get_mut(minix_types::UserSlot::new(0))
            .unwrap()
            .eff_uid = 1000;
        state.current_message = getsysinfo_msg(SI_PROC_TAB, fproc_bytes);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getsysinfo),
            SyscallResult::Error(minix_types::EPERM)
        );

        // Unknown what → EINVAL (C's default arm).
        state
            .fproc_table
            .get_mut(minix_types::UserSlot::new(0))
            .unwrap()
            .eff_uid = 0;
        state.current_message = getsysinfo_msg(99, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getsysinfo),
            SyscallResult::Error(minix_types::EINVAL)
        );

        // root + wrong size → EINVAL (the exact-size gate).
        state.current_message = getsysinfo_msg(SI_DMAP_TAB, dmap_bytes - 1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getsysinfo),
            SyscallResult::Error(minix_types::EINVAL)
        );

        // root + all gates pass → reaches the copy-out seam; under the host build
        // sys_datacopy is unreachable, so EFAULT (Fault) is honestly surfaced, not faked.
        state.current_message = getsysinfo_msg(SI_DMAP_TAB, dmap_bytes);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getsysinfo),
            SyscallResult::Error(minix_types::EFAULT)
        );
    }

    #[test]
    fn test_dispatch_close_releases_fd() {
        let mut state = seeded(100);
        let fid = state.filp_table.alloc_filp(0o644).unwrap();
        state.filp_table.inc_count(fid);
        state.fproc_table.get_mut(minix_types::UserSlot::new(0)).unwrap().filps[3] = Some(fid.get());

        let call = VfsCallNum::Close;
        state.current_message = msg_of(call, 3, 0, 0, 0);

        let r = dispatch_syscall(&mut state, call);
        assert_eq!(r, SyscallResult::Ok(0));
        assert!(state.fproc_table.get(minix_types::UserSlot::new(0)).unwrap().filps[3].is_none());
    }

    #[test]
    fn test_dispatch_umask_swaps() {
        let mut state = seeded(100);
        // C: fork inherits cmask (on the mproc/PM side); the VFS-side test seeds the old value 022 directly.
        state.fproc_table.get_mut(minix_types::UserSlot::new(0)).unwrap().umask = 0o022;
        let call = VfsCallNum::Umask;
        state.current_message = msg_of(call, 0o027, 0, 0, 0);
        let r = dispatch_syscall(&mut state, call);
        assert_eq!(r, SyscallResult::Ok(0o022));
        let fp = state.fproc_table.get(minix_types::UserSlot::new(0)).unwrap();
        assert_eq!(fp.umask, 0o027);
    }

    #[test]
    fn test_dispatch_lseek_set_cur_end() {
        let mut state = seeded(100);
        let fid = state.filp_table.alloc_filp(0o644).unwrap();
        state.filp_table.inc_count(fid);
        {
            let fp = state.fproc_table.get_mut(minix_types::UserSlot::new(0)).unwrap();
            fp.filps[4] = Some(fid.get());
        }
        {
            let v = state.vnode_table.get_mut(VnodeId(0)).unwrap();
            v.mode = 0o100644;
            v.size = 1000;
            v.fs = Endpoint::MFS;
        }
        {
            let f = state.filp_table.get_mut(fid).unwrap();
            f.vnode = Some(0);
            f.pos = 10;
        }
        // The read-ahead inhibit goes through the mount window: create a vmnt row for
        // this vnode's FS (dev must be a real device number — a `NO_DEV` row is
        // treated as an empty row).
        let vmnt = state.vmnt_table.alloc().unwrap();
        {
            let m = state.vmnt_table.get_mut(vmnt).unwrap();
            m.fs = Endpoint::MFS;
            m.dev = 7;
        }
        let call = VfsCallNum::Lseek;
        // SEEK_SET(0) to 40.
        state.current_message = Message {
            m_source: Endpoint::from_generation_slot(1, 0),
            m_type: call as i32,
            m_u: MessageUnion {
                m_m7: MessageM7 {
                    m7i1: 40,
                    m7i2: 0,
                    m7i3: 4,
                    m7i4: 0,
                    ..Default::default()
                },
            },
        };
        // This test is the first arm-level test to reach the **suspension phase**:
        // calling dispatch directly has no run_once slot-binding layer, so bind a slot
        // by hand (C's work always runs on a slot).
        let worker = state
            .worker_pool
            .assign_first_fit(
                minix_types::UserSlot::new(0),
                crate::worker::WorkerFunc::DoWork,
                &state.current_message,
            )
            .unwrap();
        state.current_worker = Some(worker);
        let r = dispatch_syscall(&mut state, call);
        let f = state.filp_table.get(fid).unwrap();
        assert_eq!(f.pos, 40, "位置已改（C actual_lseek:640）");
        // Position changed → send `REQ_INHIBREAD` and suspend (C :642-645's req_inhibread).
        assert_eq!(r, SyscallResult::Suspend);
        let pending = state.pending_fs.expect("抑制预读已登记");
        assert_eq!(pending.req.m_type, minix_types::REQ_INHIBREAD);
        assert_eq!(pending.user, Endpoint::from_generation_slot(1, 0));

        // Contrast: position **unchanged** → no FS request, reply directly with the new position (C open.c:639).
        let mut state2 = seeded(100);
        let fid2 = state2.filp_table.alloc_filp(0o644).unwrap();
        state2
            .fproc_table
            .get_mut(minix_types::UserSlot::new(0))
            .unwrap()
            .filps[4] = Some(fid2.get());
        {
            let v = state2.vnode_table.get_mut(VnodeId(0)).unwrap();
            v.mode = 0o100644;
            v.size = 1000;
        }
        {
            let f = state2.filp_table.get_mut(fid2).unwrap();
            f.vnode = Some(0);
            f.pos = 40;
        }
        state2.current_message = Message {
            m_source: Endpoint::from_generation_slot(1, 0),
            m_type: call as i32,
            m_u: MessageUnion {
                // SEEK_CUR(1) + offset 0 → newpos == cur (position unchanged).
                m_m7: MessageM7 { m7i1: 0, m7i2: 0, m7i3: 4, m7i4: 1, ..Default::default() },
            },
        };
        assert_eq!(dispatch_syscall(&mut state2, call), SyscallResult::Ok(0));
        assert!(state2.pending_fs.is_none(), "位置没变不打扰 FS");
        let (target, reply) = state2.take_reply().expect("回复已入队");
        assert_eq!(target, Endpoint::from_generation_slot(1, 0));
        assert_eq!(reply.m_type, minix_types::OK);
        // SAFETY(test): the first slot of the reply payload is the new position.
        let off = unsafe {
            let raw = &reply.m_u.raw;
            let mut b8 = [0u8; 8];
            b8.copy_from_slice(&raw[0..8]);
            i64::from_le_bytes(b8)
        };
        assert_eq!(off, 40, "回复带新位置（C do_lseek:665-666）");
    }

    /// The `InhibRead` continuation: when the read-ahead inhibit reply arrives, the
    /// new position is carried back to the user; a failed request returns the error
    /// (the position has already changed — that is C's semantics).
    #[test]
    fn test_worker_continuation_inhibread_replies_with_offset() {
        use crate::worker::WorkerCont;
        let user = Endpoint::from_generation_slot(1, 0);
        let mut state = VfsState::new();
        let slot = minix_types::UserSlot::new(0);
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.endpoint = user;
            fp.pid = 100;
        }
        let idx = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        {
            let wp = state.worker_pool.get_mut(idx).unwrap();
            wp.cont = Some(WorkerCont::InhibRead { offset: 0x99 });
            wp.sendrec = Some(Message { m_type: 0, ..Message::default() });
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        let (target, reply) = state.take_reply().expect("回复");
        assert_eq!(target, user);
        assert_eq!(reply.m_type, 0);
        // SAFETY(test): the first slot is the new position.
        let off = unsafe {
            let raw = &reply.m_u.raw;
            let mut b8 = [0u8; 8];
            b8.copy_from_slice(&raw[0..8]);
            i64::from_le_bytes(b8)
        };
        assert_eq!(off, 0x99);

        // Failure: the status is passed through (with no payload).
        let idx2 = state
            .worker_pool
            .assign_first_fit(slot, crate::worker::WorkerFunc::DoWork, &Message::default())
            .unwrap();
        {
            let wp = state.worker_pool.get_mut(idx2).unwrap();
            wp.cont = Some(WorkerCont::InhibRead { offset: 0x99 });
            wp.sendrec = Some(Message { m_type: minix_types::EIO, ..Message::default() });
            wp.state = crate::worker::WorkerState::Busy;
        }
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user, minix_types::EIO))
        );
    }

    #[test]
    fn test_dispatch_fs_dialogue_arms_nosys() {
        // FS/driver conversation arms: the unwired ones still fail closed with Nosys
        // (the honest contract). Already wired by the template: `Read`/`Write`/
        // `Fstat`/`Stat`/`Lstat`/`Ftruncate`/`Lseek`/`Open` (including `O_TRUNC`)/
        // `Mkdir`/`Creat`/`Getdents`/`Access`/`Readlink`/`Chmod`/`Fchmod`/`Chown`/
        // `Fchown`/`Unlink`/`Rmdir`/`Mknod`/`Symlink`/`Link`/`Rename`/`Utimens`/
        // `Sync`/`Fsync`/`Chdir`/`Fchdir`/`Chroot`. `Mount` is taken here as the
        // representative unwired one — it belongs to the same "FS conversation family"
        // and is the only arm in it that must **create a mount row**.
        let mut state = seeded(100);
        let call = VfsCallNum::Mount;
        state.current_message = Message {
            m_source: Endpoint::from_generation_slot(1, 0),
            m_type: call as i32,
            m_u: MessageUnion { m_m7: MessageM7::default() },
        };
        let r = dispatch_syscall(&mut state, call);
        assert_eq!(r, SyscallResult::Nosys);
    }

    #[test]
    fn test_exhaustive_arms_smoke() {
        // Exhaustiveness smoke: all 64 call numbers can be dispatched one by one (most Nosys, no panic).
        let mut state = seeded(100);
        for raw in 0..64u32 {
            let call = VfsCallNum::from_raw(VFS_BASE + raw).unwrap();
            let state_msg = Message { m_type: call as i32, ..Message::default() };
            state.current_message = state_msg;
            let _ = dispatch_syscall(&mut state, call);
        }
    }
    #[test]
    fn test_dispatch_close_releases_locks() {
        // C `close_fd:700-713`: closing a file releases this process's record locks on that vnode.
        let mut state = seeded(100);
        let slot = minix_types::UserSlot::new(0);
        let pid: u32 = 100;

        // Populate the vnode table: ino 5 = the file being closed
        {
            let v = state.vnode_table.get_mut(VnodeId(0)).unwrap();
            v.fs = Endpoint::from_generation_slot(0, 10);
            v.ino = 5;
            v.mode = 0o100644;
            v.dev = 100;
        }
        // Populate the filp: fd 3 → vnode 0
        let fid = state.filp_table.alloc_filp(0o644).unwrap();
        state.filp_table.inc_count(fid);
        {
            let f = state.filp_table.get_mut(fid).unwrap();
            f.vnode = Some(0);
        }
        {
            let fp = state.fproc_table.get_mut(slot).unwrap();
            fp.filps[3] = Some(fid.get());
        }
        // Add a lock: pid 100 holds a record lock on vnode (fs 10, ino 5)
        let key = crate::fcntl::VnodeKey { fs: Endpoint::from_generation_slot(0, 10), ino: 5 };
        state.lock_table.release_for(key, pid); // clear test residue
        // Manipulate lock_table directly to add the lock (simulating an effective fcntl F_SETLK)
        {
            let fl = crate::fcntl::FileLock {
                lock_type: crate::fcntl::LockType::Write,
                pid,
                vnode: key,
                first: 0,
                last: 100,
            };
            state.lock_table.slots[0] = Some(fl);
            state.lock_table.nr += 1;
        }
        assert_eq!(state.lock_table.nr, 1);

        // Close fd 3 → the lock should be released
        let fd = Fd::new(3).unwrap();
        state.current_message = Message {
            m_source: Endpoint::from_generation_slot(1, 0),
            m_type: VfsCallNum::Close as i32,
            m_u: MessageUnion {
                m_m7: MessageM7 { m7i1: 3, ..Default::default() },
            },
        };
        let r = dispatch_syscall(&mut state, VfsCallNum::Close);
        assert_eq!(r, SyscallResult::Ok(0));
        assert!(state.fproc_table.get(slot).unwrap().filps[3].is_none());
        assert_eq!(state.lock_table.nr, 0);
        assert!(state.lock_table.slots[0].is_none());
    }

}
