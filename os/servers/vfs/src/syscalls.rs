//! 系统调用绑定层（R2-P1-2 W3）——`VfsCallNum` 的 64 臂穷举 match。
//!
//! C 的 `do_work`（main.c）以 `call_vec[call_index]()` 函数指针直达各
//! `do_xxx` 处理器；Rust 的对应物是本模块的穷举 [`dispatch_syscall`]：
//! 每臂解码 `current_message` 的字段（`lc_vfs_*` 布局按 `ipc.h` 偏移，
//! 经 M7 视图读取）、调用所属模块的决策函数、产出 [`SyscallResult`]。
//! 无通配臂——新增调用号必须显式表态。
//!
//! 忠实移植边界：需要 FS/驱动对话的调用，在 W1 transport 通电前以
//! `SyscallResult::Nosys` 显式拒绝（fail-closed），逐臂注明依赖；
//! 通电后按同臂位接入对话（模式 60 诚实契约）。

use crate::call_table::{SyscallResult, VfsCallNum};
use crate::path::PathFetcher as _;
use crate::filedes::{close_fd, Fd};
use crate::open::{seek_pos, S_IFMT, S_IFIFO, Whence};
use crate::main_loop::VfsState;

use crate::vnode::VnodeId;

/// `do_work`（`main.c:283-294`）的绑定层——穷举 64 臂，无通配。
///
/// 解码约定：`lc_vfs_*` 消息布局按 C `ipc.h` 的字段偏移，经 `Message`
/// 的 M7 视图读取（`off_t` 由相邻两个 `i32` 拼回，低 32 位在前）。
/// 传输的起始位置：`O_APPEND` 时取文件大小（C read.c:234），否则取 filp
/// 位置（:145 的 `position = f->filp_pos`）。
///
/// 抽成纯函数是因为它在本拍的两条门（grant/挂载）之前生效，宿主构建下
/// 走不到后面的登记，只有这里能测——同时把 C 的两个来源摆在一处。
pub(crate) const fn transfer_position(flags: i32, filp_pos: i64, vnode_size: u64) -> i64 {
    if (flags as u32) & crate::fcntl::O_APPEND != 0 {
        vnode_size as i64
    } else {
        filp_pos
    }
}

pub fn dispatch_syscall(state: &mut VfsState, call: VfsCallNum) -> SyscallResult {
    let msg = state.current_message;
    // 联合体读取与 C 的 union 语义一致（VfsPmInit decode 同款惯例）。
    let m7 = unsafe { &msg.m_u.m_m7 };

    match call {
        // ── 表本地语义：决策齐，直接执行 ──
        VfsCallNum::Close => {
            // lc_vfs_close: fd @0（ipc.h:620-627）。
            let fd = match Fd::new(m7.m7i1.max(0) as usize) {
                Some(fd) => fd,
                None => return SyscallResult::Error(minix_types::EBADF),
            };
            let fp_slot = match state.current_fp_slot {
                Some(slot) => slot,
                None => return SyscallResult::Error(minix_types::EINVAL),
            };

            // 预取锁释放所需身份（C `close_fd:702` 的 `vp = rfilp->filp_vno`）。
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

            let fp = match state.fproc_table.get_mut(fp_slot) {
                Some(fp) => fp,
                None => return SyscallResult::Error(minix_types::EINVAL),
            };
            match close_fd(fp, fd, &mut state.filp_table) {
                Ok(()) => {}
                Err(e) => return SyscallResult::Error(e.to_errno()),
            }

            // POSIX 记录锁释放（C `close_fd:700-713`）：关闭文件的 vnode 上
            // 属于本进程的锁全部释放并触发 lock_revive。
            if let Some(vid) = filp_vid
                && let Some(v) = state.vnode_table.get(vid)
            {
                let key = crate::fcntl::VnodeKey { fs: v.fs, ino: v.ino };
                let pid = fp_pid as u32;
                let _released = state.lock_table.release_for(key, pid);
                // lock_revive 的复活广播归 17 号（select/lock 等待者）。
            }

            SyscallResult::Ok(0)
        }
        VfsCallNum::Lseek => {
            // lc_vfs_lseek（ipc.h:725-731）：off_t offset @0、int fd @8、
            // int whence @12。`actual_lseek` 的纯算术决策（open.rs seek_pos）。
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
            // SEEK_END 需要 vnode size；非 FIFO 才允许（open.rs seek_pos）。
            let (cur, size, is_fifo) = match filp.vnode
                .and_then(|vid| state.vnode_table.get(VnodeId(vid)))
            {
                Some(v) => (f_pos_of(filp), v.size as i64, (v.mode & S_IFMT) == S_IFIFO),
                None => return SyscallResult::Error(minix_types::EBADF),
            };
            match seek_pos(whence, cur, size, offset, is_fifo) {
                Ok((new_pos, inhibit)) => {
                    filp.pos = new_pos;
                    // C `do_lseek` 把新位置写进回复载荷
                    // （`m_vfs_lc_lseek.offset`，open.c:665-666）——两条
                    // 出口（发不发抑制预读）都要带。
                    let mut reply = minix_types::Message {
                        m_type: minix_types::OK,
                        ..minix_types::Message::default()
                    };
                    // SAFETY: `mess_vfs_lc_lseek { off_t offset; }`
                    // （ipc.h:2206-2210）在负载区首字。
                    unsafe {
                        reply.m_u.raw[0..8].copy_from_slice(&new_pos.to_le_bytes());
                    }
                    let target = state
                        .fproc_table
                        .get(fp_slot)
                        .map(|fp| fp.endpoint)
                        .unwrap_or(minix_types::Endpoint::NONE);
                    if !inhibit {
                        // 位置没变：不打扰 FS（C open.c:639）。
                        state.queue_reply_msg(target, reply);
                        return SyscallResult::Ok(0);
                    }
                    // 位置变了：发 `REQ_INHIBREAD`（抑制预读）后回信——
                    // 续接体 `InhibRead` 负责把新位置带上（C :642-645）。
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
                        grant: 0, // 无数据面
                        user: target,
                        req: crate::request::encode_inhibread(fs_e.1),
                    });
                    SyscallResult::Suspend
                }
                Err(e) => SyscallResult::Error(e.to_errno()),
            }
        }
        VfsCallNum::Umask => {
            // `do_umask`（protect.c:186-190）：`mask &= 0777` 后交换旧值。
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
        VfsCallNum::Getrusage => SyscallResult::Ok(0), // 废弃调用恒 OK（misc.c:1005）

        // ── FS 对话族：req_* 经 fs_comm 窗口投递，W1 transport 通电后启用 ──
        | VfsCallNum::Mknod
        | VfsCallNum::Link
        | VfsCallNum::Unlink
        | VfsCallNum::Rename
        | VfsCallNum::Rmdir
        | VfsCallNum::Symlink
        | VfsCallNum::Readlink
        | VfsCallNum::Truncate
        | VfsCallNum::Getdents
        | VfsCallNum::Chmod
        | VfsCallNum::Fchmod
        | VfsCallNum::Chown
        | VfsCallNum::Fchown
        | VfsCallNum::Lstat
        | VfsCallNum::Statvfs1
        | VfsCallNum::Fstatvfs1
        | VfsCallNum::Mount
        | VfsCallNum::Umount
        | VfsCallNum::Sync
        | VfsCallNum::Fsync
        | VfsCallNum::Access
        | VfsCallNum::Chdir
        | VfsCallNum::Fchdir
        | VfsCallNum::Chroot
        | VfsCallNum::Pipe2
        | VfsCallNum::Select
        | VfsCallNum::Socket
        | VfsCallNum::Socketpair
        | VfsCallNum::Bind
        | VfsCallNum::Connect
        | VfsCallNum::Listen
        | VfsCallNum::Accept
        | VfsCallNum::Sendto
        | VfsCallNum::Sendmsg
        | VfsCallNum::Recvfrom
        | VfsCallNum::Recvmsg
        | VfsCallNum::Setsockopt
        | VfsCallNum::Getsockopt
        | VfsCallNum::Getsockname
        | VfsCallNum::Getpeername
        | VfsCallNum::Shutdown
        | VfsCallNum::Svrctl
        | VfsCallNum::Utimens
        | VfsCallNum::Vmcall
        | VfsCallNum::Mapdriver
        | VfsCallNum::Copyfd
        | VfsCallNum::Socketpath
        | VfsCallNum::Ioctl
        | VfsCallNum::Fcntl
        | VfsCallNum::Getvfsstat
        | VfsCallNum::GcovFlush => {
            // FS/驱动对话——W1 transport 通电后经 fs_comm 窗口接入
            // （plan.md §8 W3 尾注）。
            SyscallResult::Nosys
        }

        // ── 对话臂模板：read（常规文件；管道/字符/块各有其臂）──
        VfsCallNum::Read => {
            // C `do_read` → `read_write(READING)`（read.c:141-265）的
            // **常规文件**分支：位置取 filp（`position = f->filp_pos`，
            // :145）→ `req_readwrite`（grant 是 FS 往用户缓冲写的 magic
            // grant，`CPF_WRITE|CPF_TRY`）→ 回复带新位置与实际字节数。
            // 本节拍只接常规文件；`S_ISFIFO`/`S_ISCHR`/`S_ISBLK` 三个分支
            // 各有其臂（pipe.c/cdev.c/bdev.c），未接线前保持 ENOSYS。
            let (fd, buf, len) = {
                // 用户载荷（minix-sys `read_via`：fd@0、buf@8、len@16、
                // 保留的累计字段@24 恒零 — C `mess_lc_vfs_readwrite`）。
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
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
            // C read.c:150 —— `if (size > SSIZE_MAX) return EINVAL;`
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
            // C 按 `vp->v_mode` 分派（read.c:154-231）：本拍只服务常规文件。
            if mode & crate::open::S_IFMT != crate::open::S_IFREG {
                return SyscallResult::Nosys;
            }
            let vmnt_id = match state.vmnt_table.find_by_fs(fs_e) {
                Some(v) => v,
                None => return SyscallResult::Error(minix_types::EIO),
            };
            // C request.c:860-862 —— FS 未声明 64 位能力且位置越过 INT_MAX
            // 即 EINVAL（`!(vmp->m_fs_flags & RES_64BIT) && pos > INT_MAX`）。
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
            // magic grant：读方向＝FS 往用户缓冲写。
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

        // ── 对话臂模板：write（常规文件；管道/字符/块各有其臂）──
        VfsCallNum::Write => {
            // C `do_write` → `read_write(WRITING)`（read.c:141-265）：位置取
            // filp（:145），**`O_APPEND` 时位置改取 vnode 大小**（:234）→
            // `req_readwrite`（grant 是 FS 从用户缓冲读的 magic grant，
            // `CPF_READ|CPF_TRY`）→ 回复带新位置与实际字节数，并且写方向
            // 要按新位置抬高 vnode 大小（:255-259，续接体里做）。
            let (fd, buf, len) = {
                // 载荷与读同形（minix-sys `write_via` 用同一个
                // ReadWritePayload：fd@0、buf@8、len@16）。
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
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
            // C read.c:150 的同一条门（读写在 C 里共用一个函数）。
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
                // C read.c:234 —— `O_APPEND` 时位置改取当前文件大小。
                let pos = transfer_position(filp.flags, filp.pos, vnode.size);
                (filp_idx, vnode.fs, vnode.ino, vnode.mode, pos, vnode_idx)
            };
            // 类型分派同读（本拍只服务常规文件）。
            if mode & crate::open::S_IFMT != crate::open::S_IFREG {
                return SyscallResult::Nosys;
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
            // magic grant：写方向＝FS 从用户缓冲读（`CPF_READ`）。
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

        // ── 对话臂模板：ftruncate（fd 版截断；无数据面）──
        VfsCallNum::Ftruncate => {
            // C `do_ftruncate`（link.c:331-359）：fd + 新长度 → `get_filp(fd,
            // VNODE_WRITE)` → 写位门（`filp_mode & W_BIT`）→ 大小不变则
            // **不发请求**（POSIX：保住文件时间）→ `truncate_vnode` →
            // 类型门（REG/FIFO）→ `req_ftrunc(fs_e, ino, newsize, 0)`。
            let (length, fd) = {
                // 用户载荷（C `mess_lc_vfs_truncate`：offset@0、fd@8；
                // minix-sys 暂无 wrapper，按 C 结构序的 LP64 换算）。
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[0..8]);
                let len = i64::from_le_bytes(b8);
                let fd = i32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]);
                (len, fd)
            };
            // C link.c:341-342 —— 负长度即 EINVAL。
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
                // C link.c:346-347 —— 写位门（`!(filp_mode & W_BIT)`；
                // 位值权威在 `open::W_BIT`，与 `filp.mode` 同为 `Mode`）。
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
            // C link.c:349-354 —— 大小不变则不打扰 FS（POSIX 文件时间）。
            if mode & crate::open::S_IFMT == crate::open::S_IFREG && size == length as u64 {
                return SyscallResult::Ok(length as i32);
            }
            // C `truncate_vnode:375-376` —— 只服务常规文件与管道。
            let ftype = mode & crate::open::S_IFMT;
            if ftype != crate::open::S_IFREG && ftype != crate::open::S_IFIFO {
                return SyscallResult::Error(minix_types::EINVAL);
            }
            let vmnt_id = match state.vmnt_table.find_by_fs(fs_e) {
                Some(v) => v,
                None => return SyscallResult::Error(minix_types::EIO),
            };
            // C request.c:274-278 —— 未声明 64 位且长度越过 INT_MAX 即 EINVAL。
            let fs_flags = state
                .vmnt_table
                .get(vmnt_id)
                .map(|v| v.fs_flags)
                .unwrap_or(0);
            if fs_flags & crate::request::FsFlags::IS64BIT.bits() == 0 && length > i32::MAX as i64 {
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
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Ftrunc {
                    vnode: vnode_idx,
                    newsize: length,
                });
            }
            state.pending_fs = Some(crate::main_loop::PendingFs {
                vmnt: vmnt_id.0,
                fs_e,
                worker,
                grant: 0, // 无数据面，不发 grant
                user: user_e,
                req: crate::request::encode_ftrunc(ino, length, 0),
            });
            SyscallResult::Suspend
        }

        // ── 路径族臂：creat（O_CREAT；三段续接链）──
        VfsCallNum::Creat => {
            // C `do_creat`（open.c:58-78）→ `common_open` 的 O_CREAT 支：
            // ①`O_CREAT` 必须在场（否则 EINVAL）②取路径 ③模式位按 umask
            // 收窄 ④走整条路径——走通即"文件已存在"，ENOENT 则转走父目录
            // 并发 `REQ_CREATE`（阶段 2/3 在续接体里）。
            let (path, oflags, mode) = {
                // 载荷（C `mess_lc_vfs_creat`：name/len/flags/mode；Rust 侧
                // 与 open 同形：name@0、len@8、flags@16、mode@20、buf@24）。
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[8..16]);
                let len = u64::from_le_bytes(b8);
                let flags = u32::from_le_bytes([raw[16], raw[17], raw[18], raw[19]]);
                let mode = u32::from_le_bytes([raw[20], raw[21], raw[22], raw[23]]);
                let inline = &raw[24..];
                if len as usize > minix_sys::vfs::OPEN_PATH_INLINE_MAX {
                    return SyscallResult::Error(minix_types::ENAMETOOLONG);
                }
                let n = (len as usize).min(inline.len());
                match crate::path::decode_name(&inline[..n], n) {
                    Ok(p) => (p, flags, mode),
                    Err(e) => return SyscallResult::Error(e.to_errno()),
                }
            };
            // C `do_creat:71-72` —— `O_CREAT` 必须在场。
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
            // C `common_open` 的建节点模式：`bits & RWX & umask`。
            let umask = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.umask)
                .unwrap_or(0o022);
            let create_bits = (mode & 0o777 & umask) | crate::open::S_IFREG;
            let resolve = match crate::path::Lookup::new(path.clone(), crate::path::LookupFlags::NOFLAGS)
            {
                Ok(l) => l,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
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

        // ── 路径族臂：mkdir（走父目录 → REQ_MKDIR）──
        VfsCallNum::Mkdir => {
            // C `do_mkdir`（open.c:564-598）：取路径 → `last_dir`（**走父
            // 目录**）→ 目录门 + 权限门 → `req_mkdir(父 ino, lastc, uid,
            // gid, bits)`。路径拆成"父目录 + 最后组件"用 `last_dir_split`
            // （C `last_dir` 的同一件事，含 NAME_MAX 门）。
            let (path, mode) = {
                // 载荷与 open 同形（`mess_lc_vfs_path`：name@0、len@8、
                // flags@16、mode@20、buf@24）。
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[8..16]);
                let len = u64::from_le_bytes(b8);
                let mode = u32::from_le_bytes([raw[20], raw[21], raw[22], raw[23]]);
                let inline = &raw[24..];
                if len as usize > minix_sys::vfs::OPEN_PATH_INLINE_MAX {
                    return SyscallResult::Error(minix_types::ENAMETOOLONG);
                }
                let n = (len as usize).min(inline.len());
                match crate::path::decode_name(&inline[..n], n) {
                    Ok(p) => (p, mode),
                    Err(e) => return SyscallResult::Error(e.to_errno()),
                }
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let split = match crate::path::last_dir_split(&path) {
                Ok(sp) => sp,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            // C `do_mkdir:583` —— 权限位 = I_DIRECTORY | (mode & RWX & umask)。
            let umask = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.umask)
                .unwrap_or(0o022);
            let bits = crate::open::S_IFDIR | (mode & 0o777 & umask);
            let resolve = match crate::path::Lookup::new(
                split.dir_path.clone(),
                crate::path::LookupFlags::NOFLAGS,
            ) {
                Ok(l) => l,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
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

        // ── 路径族臂：open（open-existing；O_CREAT 归 creat 臂）──
        VfsCallNum::Open => {
            // C `do_open`（open.c:38-53）：`O_CREAT` 必须缺席（libc 把
            // open() 拆成 OPEN 与 CREAT 两个调用）→ `copy_path` 取路径 →
            // `common_open` 走遍历 + 本地装配。本臂做取路径 + 起走，
            // 走完之后的本地半由 `PathFollow::Open` 的续接体做。
            let (name_addr, name_len, flags, inline) = {
                // 用户载荷（minix-sys `OpenPathPayload`：name@0、len@8、
                // flags@16、mode@20、buf@24 —— C `mess_lc_vfs_path` 的
                // LP64 换算；≤ OPEN_PATH_INLINE_MAX 的路径**内联在 buf**）。
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
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
            // `do_open` 的 O_CREAT 门（C open.c:46-47）。
            let args = crate::open::OpenArgs {
                oflags: crate::open::OpenFlags::from_bits_truncate(flags),
                mode: 0,
            };
            if args.validate_for_open().is_err() {
                return SyscallResult::Error(minix_types::EINVAL);
            }
            // 路径取回：≤ 内联上限的走载荷内联（minix-rs 的 wire 裁决，
            // minix-sys `OPEN_PATH_INLINE_MAX`），超长即 ENAMETOOLONG。
            let path = if name_len as usize <= minix_sys::vfs::OPEN_PATH_INLINE_MAX {
                let n = (name_len as usize).min(inline.len());
                match crate::path::decode_name(&inline[..n], n) {
                    Ok(p) => p,
                    Err(e) => return SyscallResult::Error(e.to_errno()),
                }
            } else {
                return SyscallResult::Error(minix_types::ENAMETOOLONG);
            };
            let _ = name_addr;
            let resolve = match crate::path::Lookup::new(path, crate::path::LookupFlags::NOFLAGS)
            {
                Ok(l) => l,
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
            // 起点：路径首字符 `/` 取进程根，否则工作目录（C `eat_path`）。
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
                    grant: 0, // 由 send_lookup_for_slot 覆写
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

        // ── 路径族臂：stat（path 版；首个走 W7 遍历的臂）──
        VfsCallNum::Stat => {
            // C `do_stat`（stadir.c:140-165）：取路径 → `eat_path` 走遍历
            // （每步一条 REQ_LOOKUP）→ `req_stat(fs_e, ino, who_e, buf)`。
            // 单线程模型里遍历的每一步都要挂起-续走，故本臂只做三件事：
            // ①取路径（`PathFetcher`）②开状态机（`LookupWalk::begin`）
            // ③登记现场与续接、把首条 lookup 交给循环发；后续由
            // `WorkerCont::Path` 的续接体推进（含相位 2 的 REQ_STAT）。
            let (name_addr, name_len, statbuf) = {
                // 用户载荷（minix-sys `stat_via_path`：len@0、name@8、
                // buf@16 — C `mess_lc_vfs_stat` 的 LP64 换算）。
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
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
            // 取路径：C `copy_path` 的两支（消息内联 vs 跨空间），Rust 侧
            // 生产取数件在 `path::SysPathFetcher`。
            let fetcher = crate::path::SysPathFetcher { who: user_e };
            let path = match fetcher.fetch(name_addr, name_len as usize) {
                Ok(p) => p,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let resolve = match crate::path::Lookup::new(path, crate::path::LookupFlags::NOFLAGS)
            {
                Ok(l) => l,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            // 起点：路径首字符 `/` 取进程根，否则取工作目录（C `eat_path`
            // 的同一选择）。
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
                // `begin` 永不直接完成（首步必是 Send），防御性返回。
                return SyscallResult::Error(minix_types::EIO);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            // 登记现场：状态机 + follow（相位 2）+ 续接标识；路径 grant 由
            // `send_lookup_for_slot` 现开（槽内 scratch 装路径 + NUL）。
            if let Some(wp) = state.worker_pool.get_mut(worker) {
                wp.cont = Some(crate::worker::WorkerCont::Path);
                wp.path = Some(crate::worker::PathPending {
                    walk,
                    grant: 0, // 由 send_lookup_for_slot 覆写
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

        // ── 对话臂模板：fstat（fd 版 stat）──
        VfsCallNum::Fstat => {
            // C `do_fstat`（stadir.c:173-192）：`fd`/`buf` 取用户载荷 →
            // `get_filp(fd, VNODE_READ)`（fd 有效 + 可读门）→
            // `req_stat(v_fs_e, v_inode_nr, who_e, buf)`：
            // magic grant 把用户 `struct stat` 缓冲授权给 FS 直写
            // （`cpf_grant_magic(fs_e, user_e, buf, sizeof(struct stat),
            // CPF_WRITE|CPF_TRY)` — request.c:1087）→ `fs_sendrec` 挂起。
            // 用户载荷（minix-sys `fstat_via` 的编码面：fd@0、buf@8 —
            // C `mess_lc_vfs_fstat` 的 LP64 换算）。
            let (fd, statbuf) = {
                // SAFETY: 该调用号的载荷按上述两域写在消息负载区。
                let raw = unsafe { &msg.m_u.raw };
                let fd = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[8..16]);
                (fd, u64::from_le_bytes(b8))
            };
            // 用户载荷里没有 fd 时（`fd < 0`）C 的 `get_filp` 走 EBADF。
            if fd < 0 {
                return SyscallResult::Error(minix_types::EBADF);
            }
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            // filp → vnode → (fs_e, ino)；顺带过 fd 有效性与可读门
            // （C `get_filp(..., VNODE_READ)`）。
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
                // C `find_vmnt` 失败即 EIO（comm.c:137-140）。
                None => return SyscallResult::Error(minix_types::EIO),
            };
            let Some(worker) = state.current_worker else {
                // 无槽（非 run_once 路径或已被释放）——按 C 的
                // `worker_available()==0` 同面回 EAGAIN。
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            let user_e = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);
            // magic grant：FS 往用户 stat 缓冲写（`CPF_WRITE | CPF_TRY`）。
            const STRUCT_STAT_SIZE: u64 = 88; // sys/stat.h 的 LP64 struct stat
            let grant = match state.grant_user_buffer(
                fs_e,
                user_e,
                statbuf,
                STRUCT_STAT_SIZE,
                minix_types::CpFlags::WRITE | minix_types::CpFlags::TRY,
            ) {
                Ok(g) => g,
                // C 在这里 panic（"cpf_grant_* failed"，request.c:1090）；
                // Rust 按 EIO 回用户（内部错误的对外面）。
                Err(_) => return SyscallResult::Error(minix_types::EIO),
            };
            let req = crate::request::encode_stat(ino, grant);
            // 登记续接 + 待发（发送由主循环的 flush_pending_fs 做）。
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
            // C 的 `worker_wait()`：单线程模型里＝臂返回 Suspend、槽停在
            // WaitingForFs（由 flush_pending_fs 经 fs_sendrec 置位）。
            SyscallResult::Suspend
        }

        // ── 服务器自用臂：getsysinfo（VFS_GETSYSINFO = +48）──
        VfsCallNum::Getsysinfo => {
            // C `do_getsysinfo`（misc.c:59-113）三段：root 门 → `what`
            // 分类 → 精确长度门 + `sys_datacopy(SELF → 调用方)`。
            // root 判据是 C 的 `super_user` 宏（glo.h:33）——当前 fproc
            // 的 `fp_effuid == SU_UID (0)`。
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
                // C `default: return(EINVAL)`（misc.c:104-105）。
                None => return SyscallResult::Error(minix_types::EINVAL),
            };
            let target = msg.m_source;
            let mut cpy = crate::misc::SysCopyToUser { target };
            match crate::misc::do_getsysinfo(
                &state.fproc_table,
                &state.dmap_table,
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

/// filp 的当前读位置（`filp_pos`，`file.h:7`）。
fn f_pos_of(f: &crate::filp::Filp) -> i64 {
    f.pos
}

// `OPEN_MAX` 由 `fproc.rs` 定义；表本地臂的 fd 边界经 `Fd::new` 检查。
#[allow(unused_imports)]
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

    /// `transfer_position`：C 的两个位置来源（read.c:145/:234）。
    #[test]
    fn test_transfer_position_sources() {
        // 无 O_APPEND：取 filp 位置。
        assert_eq!(transfer_position(0, 0x1000, 0x9000), 0x1000);
        // 有 O_APPEND：取文件大小（追加写）。
        assert_eq!(
            transfer_position(
                crate::fcntl::O_APPEND as i32,
                0x1000,
                0x9000
            ),
            0x9000
        );
        // 空文件 + O_APPEND：位置 0。
        assert_eq!(transfer_position(crate::fcntl::O_APPEND as i32, 0x1000, 0), 0);
    }

    /// `Write` 臂（模板的第三个）：门序与类型分支同 Read；追加位置见
    /// `transfer_position`，写入后的文件大小更新在续接体（见 main_loop 的
    /// 续接测试）。
    #[test]
    fn test_dispatch_write_gates_and_type_branch() {
        use minix_types::Endpoint;

        let write_msg = |fd: i32, buf: u64, len: u64| {
            let mut m = Message::default();
            m.m_source = Endpoint::from_generation_slot(1, 0);
            m.m_type = VfsCallNum::Write as i32;
            // SAFETY: write 载荷 fd@0、buf@8、len@16（minix-sys write_via 同布局）。
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
        state.filp_table.get_mut(crate::filp::FilpId(fid.get())).unwrap().vnode = Some(vid.get());
        state.current_message = write_msg(3, 0x5000, 16);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Write),
            SyscallResult::Nosys,
            "字符设备分支未接线"
        );

        state.vnode_table.get_mut(vid).unwrap().mode = crate::open::S_IFREG | 0o644;
        state.current_message = write_msg(3, 0x5000, 16);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Write),
            SyscallResult::Error(minix_types::EIO),
            "常规文件：无 vmnt → EIO"
        );
    }

    /// `Creat` 臂的门：`O_CREAT` 缺席即 EINVAL（C `do_creat:71-72`）；超内联
    /// 上限 ENAMETOOLONG；合法路径走到遍历（宿主下 grant 不可达 → EIO）。
    #[test]
    fn test_dispatch_creat_gates() {
        use minix_types::Endpoint;

        let creat_msg = |flags: u32, mode: u32, path: &[u8]| {
            let mut m = Message::default();
            m.m_source = Endpoint::from_generation_slot(1, 0);
            m.m_type = VfsCallNum::Creat as i32;
            // SAFETY: creat 载荷 name@0/len@8/flags@16/mode@20/buf@24。
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[8..16].copy_from_slice(&(path.len() as u64).to_le_bytes());
                raw[16..20].copy_from_slice(&flags.to_le_bytes());
                raw[20..24].copy_from_slice(&mode.to_le_bytes());
                let n = path.len().min(raw.len() - 24);
                raw[24..24 + n].copy_from_slice(&path[..n]);
            }
            m
        };

        let mut state = seeded(100);
        // O_CREAT 缺席 → EINVAL。
        state.current_message = creat_msg(0, 0o644, b"/x\0");
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Creat),
            SyscallResult::Error(minix_types::EINVAL)
        );
        // 超内联 → ENAMETOOLONG。
        let long = [b'a'; 33];
        state.current_message = creat_msg(0x200, 0o644, &long);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Creat),
            SyscallResult::Error(minix_types::ENAMETOOLONG)
        );
        // 合法：走到遍历（绑槽后宿主 grant 不可达 → EIO）。
        let worker = state
            .worker_pool
            .assign_first_fit(
                minix_types::UserSlot::new(0),
                crate::worker::WorkerFunc::DoWork,
                &Message::default(),
            )
            .unwrap();
        state.current_worker = Some(worker);
        state.current_message = creat_msg(0x200, 0o644, b"/x\0");
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Creat),
            SyscallResult::Error(minix_types::EIO)
        );
    }

    /// `Open` 臂的门：`O_CREAT` 在场即 EINVAL（C `do_open:46-47`，libc 把
    /// open() 拆成 OPEN/CREAT 两调用）；超内联上限的路径 ENAMETOOLONG
    /// （minix-rs 的 wire 裁决）；其余走到遍历（宿主下首条 lookup 的
    /// grant 不可达 → EIO）。
    #[test]
    fn test_dispatch_open_gates() {
        use minix_types::Endpoint;

        let open_msg = |flags: u32, path: &[u8]| {
            let mut m = Message::default();
            m.m_source = Endpoint::from_generation_slot(1, 0);
            m.m_type = VfsCallNum::Open as i32;
            // SAFETY: open 载荷 name@0/len@8/flags@16/buf@24（OpenPathPayload）。
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[8..16].copy_from_slice(&(path.len() as u64).to_le_bytes());
                raw[16..20].copy_from_slice(&flags.to_le_bytes());
                let n = path.len().min(raw.len() - 24);
                raw[24..24 + n].copy_from_slice(&path[..n]);
            }
            m
        };

        // O_CREAT 在场 → EINVAL（位值取 `OpenFlags::CREAT` = 0x200）。
        let mut state = seeded(100);
        state.current_message = open_msg(0x0000_0200, b"/x\0");
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Open),
            SyscallResult::Error(minix_types::EINVAL),
            "O_CREAT 归 creat 臂"
        );

        // 超内联上限（含 NUL 的 33 字节）→ ENAMETOOLONG。
        let long = [b'a'; 33];
        state.current_message = open_msg(0, &long);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Open),
            SyscallResult::Error(minix_types::ENAMETOOLONG)
        );

        // 合法内联路径：走到遍历（宿主下 grant 不可达 → EIO）。直调
        // dispatch 没有 run_once 的绑槽层，故手工绑槽（臂的挂起阶段需要）。
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

    /// `Ftruncate` 臂（fd 版截断）：门序照 C `do_ftruncate` —— 负长度 →
    /// EINVAL；无 filp → EBADF；**写位门**（`filp_mode & W_BIT`）→ EBADF；
    /// 大小不变 → 直接 Ok（不打扰 FS）；非 REG/FIFO → EINVAL；其余到挂载
    /// 窗口与请求编码。
    #[test]
    fn test_dispatch_ftruncate_gates() {
        use minix_types::Endpoint;

        let ftrunc_msg = |fd: i32, length: i64| {
            let mut m = Message::default();
            m.m_source = Endpoint::from_generation_slot(1, 0);
            m.m_type = VfsCallNum::Ftruncate as i32;
            // SAFETY: ftruncate 载荷 offset@0、fd@8（C mess_lc_vfs_truncate）。
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

        // 有 filp 但只读（mode 无写位）→ EBADF（C link.c:346-347）。
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

        // 给写位后：**大小不变 → 直接 Ok（不打扰 FS）**（C link.c:349-354）。
        state.filp_table.get_mut(crate::filp::FilpId(fid.get())).unwrap().mode =
            crate::open::R_BIT | crate::open::W_BIT;
        state.current_message = ftrunc_msg(3, 50);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Ftruncate),
            SyscallResult::Ok(50)
        );

        // 类型门：目录 → EINVAL（C truncate_vnode:375-376）。
        state.vnode_table.get_mut(vid).unwrap().mode = crate::open::S_IFDIR | 0o755;
        state.current_message = ftrunc_msg(3, 10);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Ftruncate),
            SyscallResult::Error(minix_types::EINVAL)
        );

        // 常规文件 + 新长度不同：走到挂载窗口（无 vmnt → EIO）。
        state.vnode_table.get_mut(vid).unwrap().mode = crate::open::S_IFREG | 0o644;
        state.current_message = ftrunc_msg(3, 10);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Ftruncate),
            SyscallResult::Error(minix_types::EIO)
        );
    }

    /// `Ftrunc` 续接：成功时按 C `truncate_vnode:382` 更新 vnode 大小；
    /// 失败不动；两者都把状态回给用户并释放槽。
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

        // 失败：大小不动。
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
            Some((user, minix_types::EACCES))
        );
    }

    /// `Read` 臂（模板的第二个）：门序照 C `read_write` —— 负 fd / 无 filp
    /// → EBADF；`size > SSIZE_MAX` → EINVAL；**非常规文件**（管道/字符/块）
    /// 仍回 Nosys（各自的臂未接线）；常规文件走到挂载窗口与 grant。
    #[test]
    fn test_dispatch_read_gates_and_type_branch() {
        use minix_types::Endpoint;

        let read_msg = |fd: i32, buf: u64, len: u64| {
            let mut m = Message::default();
            m.m_source = Endpoint::from_generation_slot(1, 0);
            m.m_type = VfsCallNum::Read as i32;
            // SAFETY: read 载荷 fd@0、buf@8、len@16（minix-sys read_via 同布局）。
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

        // filp + 非常规 vnode（管道）→ 类型分支未接线：Nosys。
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

        // 常规文件：走到挂载窗口（无 vmnt → EIO）。
        state.vnode_table.get_mut(vid).unwrap().mode = crate::open::S_IFREG | 0o644;
        state.current_message = read_msg(3, 0x5000, 16);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Read),
            SyscallResult::Error(minix_types::EIO)
        );

        // 超 SSIZE_MAX 的长度门（C read.c:150）在解析之前生效。
        state.current_message = read_msg(3, 0x5000, u64::MAX);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Read),
            SyscallResult::Error(minix_types::EINVAL)
        );
    }

    /// `Fstat` 臂（对话臂模板）：fd 无效/空槽在**任何 I/O 之前**就回
    /// EBADF（C `get_filp` 的门）；挂载窗口缺失回 EIO（C `find_vmnt`）；
    /// 门全过才走 grant（宿主构建下 `grant_magic` 不可达 → 诚实回 EIO，
    /// 与 C 在 grant 失败处 panic 的"内部错误对外面"同值）。
    #[test]
    fn test_dispatch_fstat_gates_then_grant() {
        use minix_types::Endpoint;

        let fstat_msg = |fd: i32, buf: u64| {
            let mut m = Message::default();
            m.m_source = Endpoint::from_generation_slot(1, 0);
            m.m_type = VfsCallNum::Fstat as i32;
            // SAFETY: fstat 载荷 fd@0、buf@8（minix-sys fstat_via 同布局）。
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[8..16].copy_from_slice(&buf.to_le_bytes());
            }
            m
        };

        // 负 fd：C 的 `get_filp` 走 EBADF。
        let mut state = seeded(100);
        state.current_message = fstat_msg(-1, 0x5000);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fstat),
            SyscallResult::Error(minix_types::EBADF)
        );

        // fd 没有对应 filp → EBADF。
        state.current_message = fstat_msg(3, 0x5000);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fstat),
            SyscallResult::Error(minix_types::EBADF)
        );

        // 有 filp 但 vnode 为空 → EBADF（C 的 filp 总有 vnode；这里对应
        // "fd 指向的东西已失效"）。
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

        // filp + vnode 齐：走到挂载窗口与 grant —— 无 vmnt 时 EIO。
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

    /// `VFS_GETSYSINFO` 臂（W3 回复半的服务器自用臂）：三段与 C
    /// `do_getsysinfo`（misc.c:59-113）一致——root 门（`fp_effuid == 0`，
    /// C 的 `super_user` 宏 glo.h:33）先于长度门，`what` 分类失败回
    /// `EINVAL`，门全过才走拷出缝（宿主构建下 `sys_datacopy` 诚实回
    /// `EFAULT`，见 `SysCopyToUser`）。
    #[test]
    fn test_dispatch_getsysinfo_gates_and_copy_stage() {
        use minix_types::{SI_DMAP_TAB, SI_PROC_TAB};

        let getsysinfo_msg = |what: i32, size: u64| {
            let mut m = Message::default();
            m.m_source = Endpoint::from_generation_slot(1, 0);
            m.m_type = VfsCallNum::Getsysinfo as i32;
            // SAFETY: m_lsys_getsysinfo 是 VFS_GETSYSINFO 的载荷域。
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

        // 非 root（eff_uid != 0）→ EPERM，且发生在任何拷贝之前。
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

        // 未知 what → EINVAL（C 的 default 臂）。
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

        // root + 尺寸不符 → EINVAL（精确长度门）。
        state.current_message = getsysinfo_msg(SI_DMAP_TAB, dmap_bytes - 1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getsysinfo),
            SyscallResult::Error(minix_types::EINVAL)
        );

        // root + 门全过 → 到达拷出缝；宿主构建下 sys_datacopy 不可达，
        // 诚实上浮 EFAULT（Fault），不假装成功。
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
        // C：fork 继承 cmask（mproc/PM 侧），VFS 侧测试直接播种旧值 022。
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
        // 抑制预读要经挂载窗口：给该 vnode 的 FS 建一行 vmnt（dev 必须是
        // 真设备号——`NO_DEV` 行按空行处理）。
        let vmnt = state.vmnt_table.alloc().unwrap();
        {
            let m = state.vmnt_table.get_mut(vmnt).unwrap();
            m.fs = Endpoint::MFS;
            m.dev = 7;
        }
        let call = VfsCallNum::Lseek;
        // SEEK_SET(0) 到 40。
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
        // 本测试是首个走到**挂起阶段**的臂级测试：直调 dispatch 时没有
        // run_once 的绑槽层，故手工绑一个槽（C 的作业总在槽上跑）。
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
        // 位置变了 → 发 `REQ_INHIBREAD` 并挂起（C :642-645 的 req_inhibread）。
        assert_eq!(r, SyscallResult::Suspend);
        let pending = state.pending_fs.expect("抑制预读已登记");
        assert_eq!(pending.req.m_type, minix_types::REQ_INHIBREAD);
        assert_eq!(pending.user, Endpoint::from_generation_slot(1, 0));

        // 位置**不变**的对照：不发 FS、直接回带新位置的回复（C open.c:639）。
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
                // SEEK_CUR(1) + offset 0 → newpos == cur（位置不变）。
                m_m7: MessageM7 { m7i1: 0, m7i2: 0, m7i3: 4, m7i4: 1, ..Default::default() },
            },
        };
        assert_eq!(dispatch_syscall(&mut state2, call), SyscallResult::Ok(0));
        assert!(state2.pending_fs.is_none(), "位置没变不打扰 FS");
        let (target, reply) = state2.take_reply().expect("回复已入队");
        assert_eq!(target, Endpoint::from_generation_slot(1, 0));
        assert_eq!(reply.m_type, minix_types::OK);
        // SAFETY(test): 回复载荷首字是新位置。
        let off = unsafe {
            let raw = &reply.m_u.raw;
            let mut b8 = [0u8; 8];
            b8.copy_from_slice(&raw[0..8]);
            i64::from_le_bytes(b8)
        };
        assert_eq!(off, 40, "回复带新位置（C do_lseek:665-666）");
    }

    /// `InhibRead` 续接：抑制预读的回复到达后，把新位置带上回给用户；
    /// 请求失败则按错误回（位置已改，C 的语义如此）。
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
        // SAFETY(test): 首字即新位置。
        let off = unsafe {
            let raw = &reply.m_u.raw;
            let mut b8 = [0u8; 8];
            b8.copy_from_slice(&raw[0..8]);
            i64::from_le_bytes(b8)
        };
        assert_eq!(off, 0x99);

        // 失败：状态透传（不带载荷）。
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
        // FS/驱动对话臂：未接线的仍 fail-closed Nosys（诚实契约，模式 60）。
        // 已按模板接线的：`Read`/`Write`/`Fstat`/`Stat`/`Ftruncate`/`Lseek`/
        // `Open`/`Mkdir`。这里取还没接的 `Unlink` 作代表——它同属"FS 对话族"。
        let mut state = seeded(100);
        let call = VfsCallNum::Unlink;
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
        // 穷举性 smoke：64 个调用号逐个可分发（多数 Nosys，不 panic）。
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
        // C `close_fd:700-713`：关闭文件时释放该进程在该 vnode 上的记录锁。
        let mut state = seeded(100);
        let slot = minix_types::UserSlot::new(0);
        let pid: u32 = 100;

        // 填充 vnode 表：ino 5 = 被关文件
        {
            let v = state.vnode_table.get_mut(VnodeId(0)).unwrap();
            v.fs = Endpoint::from_generation_slot(0, 10);
            v.ino = 5;
            v.mode = 0o100644;
            v.dev = 100;
        }
        // 填充 filp：fd 3 → vnode 0
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
        // 加锁：pid 100 在 vnode (fs 10, ino 5) 上持有记录锁
        let key = crate::fcntl::VnodeKey { fs: Endpoint::from_generation_slot(0, 10), ino: 5 };
        state.lock_table.release_for(key, pid); // 清空测试残留
        // 直接操纵 lock_table 加锁（模拟 fcntl F_SETLK 已生效）
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

        // Close fd 3 → 锁应被释放
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
