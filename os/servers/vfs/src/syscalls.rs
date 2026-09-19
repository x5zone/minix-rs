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
        | VfsCallNum::Rename
        | VfsCallNum::Statvfs1
        | VfsCallNum::Fstatvfs1
        | VfsCallNum::Mount
        | VfsCallNum::Umount
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
                // 大小不变：C 回 `r = OK`（不是新长度）。
                Ok(false) => SyscallResult::Ok(0),
                Err(e) => SyscallResult::Error(e),
            }
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
            // C `common_open` 的建节点模式：`omode & ALLPERMS & fp_umask`
            // （open.c:109）。**存储约定与 C 不同**：C 的 `fp_umask` 是
            // `~mask`（protect.c:190），Rust 的 `fproc.umask` 是原始掩码
            // （默认 0 = 全保留），所以这里是 `& !umask`。
            let umask = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| fp.umask)
                .unwrap_or(0o022);
            let create_bits = (mode & 0o777 & !umask) | crate::open::S_IFREG;
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
            // C `do_mkdir:583` —— 权限位 = `I_DIRECTORY | (dirmode & RWX_MODES
            // & fp_umask)`；同 Creat，`fp_umask` 的存储约定在 Rust 侧是原始
            // 掩码，所以取 `& !umask`。
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
            let resolve = match crate::path::Lookup::new(
                path,
                crate::path::LookupFlags::NOFLAGS,
            ) {
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
        VfsCallNum::Stat | VfsCallNum::Lstat => {
            // C `do_stat`（stadir.c:140-165）与 `do_lstat`（stadir.c:405-434）
            // 的**唯一差别**是遍历标志：stat 用 `PATH_NOFLAGS`、lstat 用
            // `PATH_RET_SYMLINK`（末组件是符号链接就带链接本身回来，不跟进）。
            // 其余（`req_stat` 的目标、权限、锁）逐字相同，所以两条调用号
            // 共用一个臂。
            //
            // 宿主可测性边界：`mess_lc_vfs_stat` 没有内联路径字段，路径要经
            // `SysPathFetcher` 跨空间取（C `fetch_name`），宿主构建下取不到
            // ——所以"lstat 的 flags 是 RET_SYMLINK"这条在宿主下观测不到，
            // 挂真机 E5/T4；同一条 flags 管线的宿主断言在 Unlink 的粘滞位
            // 子遍历那条 REQ_LOOKUP 上（那里 flags 真的落在消息里）。
            let retain_symlink = matches!(call, VfsCallNum::Lstat);
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
        // ── 对话臂模板：getdents（目录读；只有目录能过类型门）──
        // ── 路径臂模板：access（走完即判，无 FS 往返）──
        // ── 路径臂模板：readlink（走末组件符号链接本身，不跟进）──
        // ── 路径臂模板：chmod（走完过权限门，回复带实际模式）──
        // ── 路径臂模板：unlink / rmdir（父目录遍历 + 粘滞位子遍历）──
        // ── chown / fchown（C `do_chown`，protect.c:24-110，两条调用号共用
        // 一个函数体：path 半走遍历，fd 半从 filp 取 vnode）──
        // ── fchmod（C `do_chmod`，protect.c:62-133 的 fd 半：从 filp 取
        // vnode，之后与 path 半共用同一个体）──
        VfsCallNum::Fchmod => {
            // 载荷 `mess_lc_vfs_fchmod`（ipc.h:647-652）：fd@0、mode@8。
            let (fd, mode) = {
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
                let raw = unsafe { &msg.m_u.raw };
                let fd = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let mode = u32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]);
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
            // C `do_chmod:88-93`：`get_filp(rfd, VNODE_WRITE)`——只要 fd 有效，
            // 不查打开模式（与 ftruncate 的 W 位门不同）。
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
            // 共用体自己决定挂起还是收尾，统一报挂起（同 Fchown）。
            SyscallResult::Suspend
        }

        // ── 路径臂模板：truncate（与 ftruncate 共用发送半）──
        VfsCallNum::Truncate => {
            // C `do_truncate`（link.c:277-326）：载荷 `mess_lc_vfs_truncate`
            // （offset@0、fd@8、name@16、len@24）→ 负长度即 EINVAL → 取路径
            // （跨空间）→ 走遍历（NOFLAGS）→ 走完过 W 位门后发 `REQ_FTRUNC`。
            let (length, name_len, name_addr) = {
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[0..8]);
                let length = i64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[16..24]);
                let name_addr = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[24..32]);
                (length, u64::from_le_bytes(b8), name_addr)
            };
            // C link.c:299-300 —— 负长度即 EINVAL（在任何取路径之前）。
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
                    grant: 0, // 由 send_lookup_for_slot 覆写
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

        // ── 路径臂模板：mknod（父目录遍历 + 名字 direct grant）──
        // ── 路径臂模板：utimens（path/fd 两半共用一个体）──
        // ── 路径臂模板：symlink（父目录遍历 + 名字 direct grant + 目标 magic grant）──
        // ── 路径臂模板：link（**两段遍历**：先源文件、再新名的父目录）──
        // ── sync / fsync（多挂载序列：每个匹配挂载一条 REQ_SYNC）──
        VfsCallNum::Sync | VfsCallNum::Fsync => {
            // C `do_sync`（misc.c:276-296，全部挂载）与 `do_fsync`
            // （misc.c:229-267，按文件所在设备的 `v_dev` 过滤）。
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            let Some(worker) = state.current_worker else {
                return SyscallResult::Error(minix_types::EAGAIN);
            };
            let dev = if matches!(call, VfsCallNum::Fsync) {
                // C misc.c:236-243：`get_filp(fd, VNODE_READ)` 取 vnode 的
                // `v_dev`——fsync 只同步该设备所在的挂载。
                let fd = {
                    // SAFETY: `mess_lc_vfs_fsync { int fd; }`（ipc.h:673-677）。
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
                    // 没有要同步的挂载：C 的循环一条都没发 → 直接成功。
                    if state.pending_fs.is_none() {
                        return SyscallResult::Ok(0);
                    }
                    SyscallResult::Suspend
                }
                Err(e) => SyscallResult::Error(e),
            }
        }

        VfsCallNum::Link => {
            // C `do_link`（link.c:170-230）：载荷 `mess_lc_vfs_link`
            // （name1@0 = 源文件路径、name2@8 = 新链接路径、len1@16、len2@24）。
            // 阶段 1 走整条 name1（源文件必须存在），阶段 2 走 name2 的父目录。
            let (src_addr, dst_addr, src_len, dst_len) = {
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
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
            // 两条路径都在用户内存里（跨空间取，宿主取不到）。
            let src_path = match fetcher.fetch(src_addr, src_len as usize) {
                Ok(p) => p,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            let dst_path = match fetcher.fetch(dst_addr, dst_len as usize) {
                Ok(p) => p,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            // C link.c:186 的 `lookup_init(..., PATH_NOFLAGS, ...)`。
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
                    grant: 0, // 由 send_lookup_for_slot 覆写
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
            // C `do_slink`（link.c:386-424）：载荷 `mess_lc_vfs_link`
            // （name1@0 = 目标串、name2@8 = 链接路径、len1@16、len2@24）。
            // 两道长度门（`<= 1` → ENOENT、`>= _POSIX_SYMLINK_MAX` →
            // ENAMETOOLONG）在任何取路径之前。
            let (target_addr, link_addr, target_len, link_len) = {
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
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
            // C link.c:400-401 的两道门（决策函数 `link::check_slink_len`
            // 同时把"不含 NUL 的长度"算出来）。
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
            // C link.c:396 的 `lookup_init(..., PATH_NOFLAGS, ...)`。
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
                    grant: 0, // 由 send_lookup_for_slot 覆写
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
            // C `do_utimens`（time.c:44-160）：载荷 `mess_vfs_utimens`
            // （atime@0、mtime@8、ansec@16、mnsec@24、len@32、name@40、
            // fd@48、flags@52）。`name != NULL` 走路径半（此时 flags 只允许
            // `AT_SYMLINK_NOFOLLOW`），否则走 fd 半（flags 必须为 0）。
            let (atime, mtime, ansec, mnsec, name_len, name_addr, fd, flags) = {
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
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
                // fd 半（C time.c:98-108）：flags 必须为 0。
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
            // 路径半（C time.c:76-96）：未知标志即 EINVAL；`AT_SYMLINK_NOFOLLOW`
            // 选遍历标志（不跟进末组件符号链接）。
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
                    grant: 0, // 由 send_lookup_for_slot 覆写
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
            // C `do_mknod`（open.c:514-556）：载荷 `mess_lc_vfs_mknod`
            // （device@0、name@8、len@16、mode@24）→ 只有超级用户能建非 FIFO
            // 节点（否则 EPERM）→ 模式按 umask 收窄 → `last_dir` 走父目录。
            let (dev, name_len, name_addr, mode_bits) = {
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
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
            // C open.c:530-531 —— 只有超级用户能建非 FIFO 节点。
            if eff_uid != crate::link::SU_UID
                && mode_bits & crate::open::S_IFMT != crate::open::S_IFIFO
            {
                return SyscallResult::Error(minix_types::EPERM);
            }
            // C open.c:541 —— `(mode & S_IFMT) | (mode & ACCESSPERMS & fp_umask)`；
            // Rust 侧 umask 存原始掩码，故取 `& !umask`（见 `fproc::FProc`）。
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
            // 目录前缀的符号链接要跟进（`path.c:231-235` 清 RET_SYMLINK）。
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
                    grant: 0, // 由 send_lookup_for_slot 覆写
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
            // 载荷 `mess_lc_vfs_chown`（ipc.h:611-619）：name@0、len@8、fd@16、
            // owner@20、group@24——path 半用 name/len，fd 半用 fd。
            let (name_len, name_addr, fd, uid, gid) = {
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
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
                // C `do_chown` 的 fd 半（protect.c:53-63）：从 filp 取 vnode，
                // 后面与 path 半共用同一个体。
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
                // 共用体自己决定"已挂起"还是"已收尾"：两种情况都不该在这里
                // 释放槽（收尾时 `finish_worker_job` 已经释放并清了
                // `current_worker`），所以统一报挂起。
                return SyscallResult::Suspend;
            }
            // C `do_chown` 的 path 半：`fetch_name` 取路径（跨空间，宿主取不到）
            // → `eat_path`（NOFLAGS）→ 共用体。
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
                    grant: 0, // 由 send_lookup_for_slot 覆写
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
            // C `do_unlink`（link.c:94-163）同时服务 unlink 与 rmdir（table.c:25
            // 与 :36 都指向它），差别只在最后发哪个请求号（link.c:156-159）。
            let rmdir = matches!(call, VfsCallNum::Rmdir);
            let (name_len, inline) = {
                // 载荷与 access/chmod 同形（`mess_lc_vfs_path`：name@0、
                // len@8、flags@16、mode@20、buf@24 内联路径）——unlink 不用 mode。
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[8..16]);
                (u64::from_le_bytes(b8), raw[24..].to_vec())
            };
            let path = if name_len as usize <= minix_sys::vfs::OPEN_PATH_INLINE_MAX {
                let n = (name_len as usize).min(inline.len());
                match crate::path::decode_name(&inline[..n], n) {
                    Ok(p) => p,
                    Err(e) => return SyscallResult::Error(e.to_errno()),
                }
            } else {
                return SyscallResult::Error(minix_types::ENAMETOOLONG);
            };
            // C `last_dir`（path.c:146-380）：切出目录前缀与最后组件，**只走
            // 目录前缀**（前缀里的符号链接要跟进——`path.c:231-235` 把
            // RET_SYMLINK 清掉正是这个意思，所以这里用 NOFLAGS）。
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
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
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
                    grant: 0, // 由 send_lookup_for_slot 覆写
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
            // C `do_chmod`（protect.c:62-133）的路径半：`m_lc_vfs_path.mode`
            // → 走路径（VNODE_WRITE 锁，此处不建模）→ 属主/超级用户门 +
            // 只读挂载门 + setgid 清位 → `REQ_CHMOD`。
            let (name_len, mode, inline) = {
                // 载荷与 access 同形（`mess_lc_vfs_path`：name@0、len@8、
                // flags@16、mode@20、buf@24 内联路径）。
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[8..16]);
                let len = u64::from_le_bytes(b8);
                let mode = u32::from_le_bytes([raw[20], raw[21], raw[22], raw[23]]);
                (len, mode, raw[24..].to_vec())
            };
            let path = if name_len as usize <= minix_sys::vfs::OPEN_PATH_INLINE_MAX {
                let n = (name_len as usize).min(inline.len());
                match crate::path::decode_name(&inline[..n], n) {
                    Ok(p) => p,
                    Err(e) => return SyscallResult::Error(e.to_errno()),
                }
            } else {
                return SyscallResult::Error(minix_types::ENAMETOOLONG);
            };
            // C `lookup_init(..., PATH_NOFLAGS, ...)`（protect.c:76）。
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
            // C `do_rdlink`（link.c:473-507）：载荷
            // `mess_lc_vfs_readlink { name, namelen, buf, bufsize }` →
            // `bufsize > SSIZE_MAX` 即 EINVAL → 带 `PATH_RET_SYMLINK` 走路径
            // → 不是符号链接即 EINVAL → `REQ_RDLINK`（grant 往用户缓冲写）。
            let (name_len, buf, buf_size, inline) = {
                // 用户载荷：name@0、namelen@8、buf@16、bufsize@24（ipc.h:785-792）。
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[8..16]);
                let name_len = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[16..24]);
                let buf = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[24..32]);
                let buf_size = u64::from_le_bytes(b8);
                (name_len, buf, buf_size, raw[32..].to_vec())
            };
            // C link.c:486 —— 窗口大于 SSIZE_MAX 即 EINVAL。
            if buf_size > i64::MAX as u64 {
                return SyscallResult::Error(minix_types::EINVAL);
            }
            let path = if name_len as usize <= minix_sys::vfs::OPEN_PATH_INLINE_MAX {
                let n = (name_len as usize).min(inline.len());
                match crate::path::decode_name(&inline[..n], n) {
                    Ok(p) => p,
                    Err(e) => return SyscallResult::Error(e.to_errno()),
                }
            } else {
                return SyscallResult::Error(minix_types::ENAMETOOLONG);
            };
            // C link.c:489 —— `PATH_RET_SYMLINK`：末组件是符号链接就带回
            // 链接本身（该位的语义在 FS 侧，见 `encode_lookup`）。
            let resolve = match crate::path::Lookup::new(
                path,
                crate::path::LookupFlags::RET_SYMLINK,
            ) {
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
            // C `do_access`（protect.c:199-233）：先验 mode（只允许
            // `R_OK|W_OK|X_OK` 的组合或 `F_OK`）→ `copy_path` → `eat_path`
            // → `forbidden(fp, vp, access)`（**真实** uid/gid）。
            let (name_len, mode, inline) = {
                // 用户载荷与 open 同形（`mess_lc_vfs_path`：name@0、len@8、
                // flags@16、mode@20、buf@24 内联路径）——access 取 `mode`。
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
                let raw = unsafe { &msg.m_u.raw };
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[8..16]);
                let len = u64::from_le_bytes(b8);
                let mode = u32::from_le_bytes([raw[20], raw[21], raw[22], raw[23]]);
                (len, mode, raw[24..].to_vec())
            };
            // C protect.c:216-217 —— 位掩码先验：`F_OK`(0) 之外只认
            // `R_OK|W_OK|X_OK`（4|2|1）；别的位就是 EINVAL，不做权限判断。
            if mode & !0o7 != 0 && mode != 0 {
                return SyscallResult::Error(minix_types::EINVAL);
            }
            let path = if name_len as usize <= minix_sys::vfs::OPEN_PATH_INLINE_MAX {
                let n = (name_len as usize).min(inline.len());
                match crate::path::decode_name(&inline[..n], n) {
                    Ok(p) => p,
                    Err(e) => return SyscallResult::Error(e.to_errno()),
                }
            } else {
                return SyscallResult::Error(minix_types::ENAMETOOLONG);
            };
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
            let rd = state.root_dir_of(Some(fp_slot));
            let start = if resolve.path.starts_with('/') {
                crate::path::LookupStart { fs: rd.fs, ino: rd.ino, dev: rd.dev }
            } else {
                let wd = state.work_dir_of(fp_slot);
                crate::path::LookupStart { fs: wd.fs, ino: wd.ino, dev: wd.dev }
            };
            // 走路径时的 uid/gid 只用于 `advance` 的搜索权限判断（C
            // `advance` 内部用有效 id）；access 自己的判断在走完之后用真实
            // id 重做一遍（protect.c:255-256）。
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
            // C `do_getdents`（read.c:282-317）：载荷与 read/write 同形
            // （`mess_lc_vfs_readwrite`，但 `cum_io` 这一格**必须为 0**）
            // → fd 门（R_BIT + 必须是目录）→ `req_getdents`（grant 是 FS
            // 往用户缓冲写目录项的 magic grant）→ 回复带下一趟位置与实际
            // 字节数，**位置只在 `nbytes > 0` 时推进**。
            let (fd, buf, len, cum_io) = {
                // 用户载荷：fd@0、buf@8、len@16、cum_io@24
                // （ipc.h:795-802 的 `mess_lc_vfs_readwrite`）。
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
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
            // C read.c:290-292 —— `cum_io` 是内部保留格，非零即 EINVAL
            // （用户态不该填它；填了说明调用方用错了入口）。
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
            // C read.c:294-297 —— 两个 EBADF 门：filp 的打开模式要含读位、
            // 且节点必须是目录（`getdents` 不是"读文件"的通用入口）。
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
            // C request.c:316-319 —— FS 未声明 64 位能力且位置越过 INT_MAX
            // 即 EINVAL。**顺序与 C 有意不同**：C 在这条早退里把刚建的
            // grant 漏掉了（`cpf_revoke` 只在正常路径调），本臂把能力门
            // 提到建 grant 之前——正常路径的返回值与 C 一致，早退路径不
            // 留悬空 grant。
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
            // magic grant：FS 往用户缓冲写目录项。C 的 `req_getdents`
            // 包装（request.c:341-357）首趟带 `CPF_TRY`（用户页没驻留时
            // 不 panic，回 ERESTART 由 `vm_vfs_procctl_handlemem` 补页后
            // 重试）——补页重试这一环与读/写臂同样待接，此处先按 C 的首趟
            // 形态带 `CPF_TRY`。
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

        // 给写位后：**大小不变 → 直接 Ok(0)（不打扰 FS）**（C link.c:349-354
        // 的 `r = OK`；这里曾经错写成"回新长度"，测试也跟着错——修的时候
        // 对着 C 行核，而不是对着实现核）。
        state.filp_table.get_mut(crate::filp::FilpId(fid.get())).unwrap().mode =
            crate::open::R_BIT | crate::open::W_BIT;
        state.current_message = ftrunc_msg(3, 50);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Ftruncate),
            SyscallResult::Ok(0)
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

    /// `Getdents` 臂的门与顺序：`cum_io != 0` 先拒（C read.c:290-292，这是
    /// 内部保留格）；fd 门与类型门是两个**不同的** EBADF（C read.c:294-297
    /// 的 `filp_mode & R_BIT` 与 `S_ISDIR`）；能力门（FS 未声明 64 位且位置
    /// 越过 `INT_MAX`）在建 grant 之前生效。
    #[test]
    fn test_dispatch_getdents_gates_and_type_branch() {
        use minix_types::Endpoint;

        let getdents_msg = |fd: i32, buf: u64, len: u64, cum_io: u64| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Getdents as i32,
                ..Message::default()
            };
            // SAFETY: getdents 载荷 fd@0、buf@8、len@16、cum_io@24
            // （ipc.h:795-802 的 mess_lc_vfs_readwrite）。
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[8..16].copy_from_slice(&buf.to_le_bytes());
                raw[16..24].copy_from_slice(&len.to_le_bytes());
                raw[24..32].copy_from_slice(&cum_io.to_le_bytes());
            }
            m
        };

        // `cum_io` 非零 → EINVAL（先于一切 fd 检查）。
        let mut state = seeded(100);
        state.current_message = getdents_msg(-1, 0x5000, 16, 1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getdents),
            SyscallResult::Error(minix_types::EINVAL)
        );

        // 负 fd 与空槽 → EBADF。
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

        // 挂上 filp + 目录 vnode，但打开模式只写 → 第一个 EBADF 门。
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
            f.mode = crate::open::W_BIT; // 只写打开
        }
        state.current_message = getdents_msg(3, 0x5000, 16, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getdents),
            SyscallResult::Error(minix_types::EBADF),
            "只写 fd 不能 getdents"
        );

        // 读位有了但节点是常规文件 → 第二个 EBADF 门。
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

        // 目录了：无挂载窗口 → EIO（C `find_vmnt` 的失败面）。
        state.vnode_table.get_mut(vid).unwrap().mode = crate::open::S_IFDIR | 0o755;
        state.current_message = getdents_msg(3, 0x5000, 16, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getdents),
            SyscallResult::Error(minix_types::EIO)
        );

        // 有挂载窗口但 FS 未声明 64 位能力 + 位置越过 INT_MAX → EINVAL
        // （C request.c:316-319）。
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

        // 声明了 64 位能力：能力门放行，走到 worker 槽与 grant（宿主构建下
        // grant 不可达 → 诚实回 EIO，与 C 在 grant 失败处 panic 的"内部
        // 错误对外面"同值）。先绑一个 worker 槽——没槽时臂回 EAGAIN
        // （C `handle_work:150-151`），那是另一条门，不在这条断言里。
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

    /// `Fchown` 臂（fd 半，C `do_chown` protect.c:53-63 + 共用体）：
    /// fd 门 → 只读门 → 三条属主规则（非超级用户：必须是属主、不能送人、
    /// 新组必须是自己所在组）→ `-1` 折算 → 界检查 → `REQ_CHOWN`；
    /// 回复带**新的模式**，uid/gid 由续接体写回 vnode 缓存。
    #[test]
    fn test_dispatch_fchown_gates_request_and_cache() {
        use minix_types::Endpoint;

        let fchown_msg = |fd: i32, uid: u32, gid: u32| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Fchown as i32,
                ..Message::default()
            };
            // SAFETY: fchown 载荷 name@0、len@8、fd@16、owner@20、group@24。
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[16..20].copy_from_slice(&fd.to_le_bytes());
                raw[20..24].copy_from_slice(&uid.to_le_bytes());
                raw[24..28].copy_from_slice(&gid.to_le_bytes());
            }
            m
        };
        // 现场：fd 3 → filp → vnode（属主 1000、属组 100、0644）。
        let user_e = Endpoint::from_generation_slot(1, 0);
        let setup = |eff_uid: u32, eff_gid: u32| {
            let mut state = seeded(100);
            state.vmnt_table.get_mut(crate::vmnt::VmntId(0)).unwrap().fs = Endpoint::MFS;
            state.vmnt_table.get_mut(crate::vmnt::VmntId(0)).unwrap().dev = 1;
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

        // ① 非属主（有效 id 0 但…换个非 0 的：2000）→ EPERM（"必须是属主"）。
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
            Some((Endpoint::from_generation_slot(1, 0), minix_types::EPERM))
        );

        // ② 是属主但把属主"送人"（请求 uid ≠ 文件 uid）→ EPERM。
        let (mut state, _vid, _idx) = setup(1000, 100);
        state.current_message = fchown_msg(3, 2000, 100);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchown),
            SyscallResult::Suspend
        );
        assert!(state.pending_fs.is_none());
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user_e, minix_types::EPERM))
        );

        // ③ 是属主、不送人，但新组不是自己所在组 → EPERM。
        let (mut state, _vid, _idx) = setup(1000, 100);
        state.current_message = fchown_msg(3, 1000, 999);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchown),
            SyscallResult::Suspend
        );
        assert!(state.pending_fs.is_none());
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user_e, minix_types::EPERM))
        );

        // ④ 非超级用户给 `-1` 当新属主 → 也会在"不能送人"那条上被拒
        // （C `do_chown:148` 用的是**消息里的原始值**：`vp->v_uid != uid`，
        // 而 `(uid_t)-1` 不是任何真实属主）。这条 C 行为看着别扭，但它就是
        // 基准，钉住它。
        let (mut state, _vid, _idx) = setup(1000, 100);
        state.current_message = fchown_msg(3, u32::MAX, 100);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchown),
            SyscallResult::Suspend
        );
        assert!(state.pending_fs.is_none());
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user_e, minix_types::EPERM)),
            "非超级用户 + uid=-1：C 判 EPERM"
        );

        // ⑤ 全过（属主不送人、改自己的组）→ 发 REQ_CHOWN。
        let (mut state, vid, idx) = setup(1000, 100);
        state.current_message = fchown_msg(3, 1000, 100);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchown),
            SyscallResult::Suspend
        );
        let p = state.pending_fs.as_ref().expect("已登记 REQ_CHOWN");
        assert_eq!(p.req.m_type, minix_types::REQ_CHOWN);
        // SAFETY(test): 按 chown_req_off 读回三域。
        unsafe {
            let raw = &p.req.m_u.raw;
            let ino = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let uid = u32::from_le_bytes(raw[8..12].try_into().unwrap());
            let gid = u32::from_le_bytes(raw[12..16].try_into().unwrap());
            assert_eq!(ino, 0x42);
            assert_eq!(uid, 1000);
            assert_eq!(gid, 100);
        }

        // 回复带新模式（FS 清掉 setuid）→ uid/gid/模式三样都进缓存。
        state.pending_fs = None;
        let mut reply = Message { m_type: 0, ..Message::default() };
        // SAFETY(test): 按 chown_reply_off 填新模式。
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

        // ⑥ 超级用户给 `-1`：两条都折算成现有值（C `do_chown:154-158` 的
        // `keep_id`），三条规则对 root 直接跳过。
        let (mut state, _vid, _idx) = setup(crate::link::SU_UID, 100);
        state.current_message = fchown_msg(3, u32::MAX, u32::MAX);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchown),
            SyscallResult::Suspend
        );
        let p = state.pending_fs.as_ref().expect("root 直接过门");
        // SAFETY(test): 按 chown_req_off 读回折算结果。
        unsafe {
            let raw = &p.req.m_u.raw;
            let uid = u32::from_le_bytes(raw[8..12].try_into().unwrap());
            let gid = u32::from_le_bytes(raw[12..16].try_into().unwrap());
            assert_eq!((uid, gid), (1000, 100), "-1 折算成文件现有值");
        }
    }

    /// `Sync`/`Fsync` 臂（C `do_sync` misc.c:276-296 / `do_fsync`
    /// misc.c:229-267）：fsync 走 fd 取 vnode 的设备号再按设备过滤挂载；
    /// 没有要同步的挂载时**直接成功**（C 的循环一条都没发）；有则起序列
    /// （每条 REQ_SYNC 逐段挂起）。
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
                v.dev = 1; // 与挂载行的 dev 相同
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
            // SAFETY: `mess_lc_vfs_fsync { int fd; }`（ipc.h:673-677）。
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
            }
            m
        };

        // ① fsync 负 fd → EBADF。
        let (mut state, _idx) = setup(true);
        state.current_message = fsync_msg(-1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fsync),
            SyscallResult::Error(minix_types::EBADF)
        );

        // ② fsync 有效 fd 但没有该设备的挂载 → 直接成功（C 的循环没发东西）。
        let (mut state, _idx) = setup(false);
        state.current_message = fsync_msg(3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fsync),
            SyscallResult::Ok(0)
        );
        assert!(state.pending_fs.is_none());

        // ③ fsync 有挂载 → 起序列（挂起，第一条 REQ_SYNC 已登记）。
        let (mut state, _idx) = setup(true);
        state.current_message = fsync_msg(3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fsync),
            SyscallResult::Suspend
        );
        let p = state.pending_fs.as_ref().expect("第一条 REQ_SYNC");
        assert_eq!(p.req.m_type, minix_types::REQ_SYNC);
        assert_eq!(p.fs_e, Endpoint::MFS);

        // ④ sync（全部挂载）同款：这里只有一行有效挂载。
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

    /// `Symlink` 臂：两道长度门（`<= 1` → ENOENT、`>= 255` →
    /// ENAMETOOLONG，C link.c:400-401）在任何取路径之前；过门后走父目录。
    #[test]
    fn test_dispatch_symlink_length_gates() {
        use minix_types::Endpoint;

        let slink_msg = |target_len: u64| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Symlink as i32,
                ..Message::default()
            };
            // SAFETY: mess_lc_vfs_link：name1@0（目标）、name2@8（链接路径）、
            // len1@16、len2@24。目标串与链接路径都在用户内存里（跨空间取）。
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
        // ① 目标长度 0 与 1 → ENOENT。
        for len in [0u64, 1] {
            state.current_message = slink_msg(len);
            assert_eq!(
                dispatch_syscall(&mut state, VfsCallNum::Symlink),
                SyscallResult::Error(minix_types::ENOENT),
                "len={len} 太短"
            );
        }
        // ② 目标长度 >= 255 → ENAMETOOLONG。
        state.current_message = slink_msg(crate::link::POSIX_SYMLINK_MAX as u64);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Symlink),
            SyscallResult::Error(minix_types::ENAMETOOLONG)
        );
        // ③ 长度合法 → 过门，停在跨空间取链接路径（宿主取不到 → EINVAL）。
        state.current_message = slink_msg(10);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Symlink),
            SyscallResult::Error(minix_types::EINVAL)
        );
    }

    /// `Utimens` 臂的门与时间折算（C `do_utimens` time.c:44-160）：
    /// fd 半要求 flags 为 0、路径半只认 `AT_SYMLINK_NOFOLLOW`；属主门
    /// （EPERM）在两个纳秒都是 `UTIME_NOW` 时**退化成写权限检查**；
    /// 纳秒 >= 1e9 即 EINVAL（`UTIME_NOW`/`UTIME_OMIT` 哨兵除外）。
    #[test]
    fn test_dispatch_utimens_gates_and_nsec_validation() {
        use minix_types::Endpoint;

        let utimens_msg = |fd: i32, name: u64, flags: u32, ansec: i64, mnsec: i64| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Utimens as i32,
                ..Message::default()
            };
            // SAFETY: mess_vfs_utimens：atime@0、mtime@8、ansec@16、mnsec@24、
            // len@32、name@40、fd@48、flags@52。
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
        // 现场：fd 3 → filp → vnode（属主 1000）。
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

        // ① fd 半 + flags 非 0 → EINVAL（C time.c:103-104）。
        let (mut state, _idx) = setup(1000, 0);
        state.current_message = utimens_msg(3, 0, 1, 0, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Utimens),
            SyscallResult::Error(minix_types::EINVAL)
        );

        // ② 属主 + 显式时间 → REQ_UTIME（秒/纳秒分开带）。
        let (mut state, idx) = setup(1000, 0);
        state.current_message = utimens_msg(3, 0, 0, 111, 222);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Utimens),
            SyscallResult::Suspend
        );
        let p = state.pending_fs.as_ref().expect("已登记 REQ_UTIME");
        assert_eq!(p.req.m_type, minix_types::REQ_UTIME);
        // SAFETY(test): 按 utime_req_off 读回五域。
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
        // 回复只有状态。
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

        // ③ 非属主 + 两个显式时间 → EPERM（只有属主或超级用户能改时间）。
        let (mut state, _idx) = setup(2000, 0);
        state.current_message = utimens_msg(3, 0, 0, 111, 222);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Utimens),
            SyscallResult::Suspend
        );
        assert!(state.pending_fs.is_none());
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((Endpoint::from_generation_slot(1, 0), minix_types::EPERM))
        );

        // ④ 纳秒越界（>= 1e9 且不是哨兵）→ EINVAL。
        let (mut state, _idx) = setup(1000, 0);
        state.current_message = utimens_msg(3, 0, 0, 1_000_000_000, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Utimens),
            SyscallResult::Suspend
        );
        assert!(state.pending_fs.is_none());
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((Endpoint::from_generation_slot(1, 0), minix_types::EINVAL))
        );

        // ⑤ 非属主 + 两个 UTIME_NOW（touch）→ 退化成写权限检查：0644 的
        // other 无写位 → EACCES（不是 EPERM）。
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
            Some((Endpoint::from_generation_slot(1, 0), minix_types::EACCES)),
            "touch 的门是写权限，不是属主"
        );

        // ⑥ 路径半 + 未知标志 → EINVAL（C time.c:78-79）。
        let (mut state, _idx) = setup(1000, 0);
        state.current_message = utimens_msg(3, 0x5000, 0x4, 0, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Utimens),
            SyscallResult::Error(minix_types::EINVAL)
        );
    }

    /// `Mknod` 臂的门与模式收窄（C `do_mknod` open.c:514-556）：
    /// 只有超级用户能建**非 FIFO** 节点（否则 EPERM）；模式 = 类型位 |
    /// （权限位 & `!umask`）。
    #[test]
    fn test_dispatch_mknod_superuser_gate_and_mode_narrowing() {
        use minix_types::Endpoint;

        let mknod_msg = |mode: u32, dev: u64| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Mknod as i32,
                ..Message::default()
            };
            // SAFETY: mknod 载荷 device@0、name@8、len@16、mode@24
            // （ipc.h:736-744）；路径经 SysPathFetcher 跨空间取（宿主取不到），
            // 所以这里只测门与模式——能过门的话会停在取路径的 EINVAL。
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..8].copy_from_slice(&dev.to_le_bytes());
                raw[16..24].copy_from_slice(&3u64.to_le_bytes());
                raw[24..28].copy_from_slice(&mode.to_le_bytes());
            }
            m
        };

        // ① 非超级用户 + 常规文件 → EPERM（在任何取路径之前）。
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

        // ② 非超级用户 + FIFO → 过门（FIFO 人人可建）；宿主下取路径不可达
        // → EINVAL（`SysPathFetcher` 的失败面）。
        state.current_message = mknod_msg(crate::open::S_IFIFO | 0o644, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Mknod),
            SyscallResult::Error(minix_types::EINVAL),
            "FIFO 过超级用户门，停在跨空间取路径"
        );

        // ③ 超级用户 + 字符设备：模式收窄按 umask，dev 随行（这里只能验到
        // "没被 EPERM 挡"——完整形状由 Done 臂那条测试钉）。
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

    /// 建节点模式的 **umask 收窄**（C `open.c:109` 的 creat、
    /// `do_mkdir:583` 的 mkdir）：Rust 侧 `fproc.umask` 存**原始掩码**，
    /// 公式是 `mode & 0777 & !umask`（C 存反码所以写 `& fp_umask`——两侧
    /// 约定不同，这里钉住 Rust 侧的结果）。默认 umask 0 必须**全保留**
    /// （曾经写成 `& umask`，新建的文件/目录权限位会变成 0）。
    #[test]
    fn test_create_and_mkdir_apply_umask_as_narrowing() {
        use minix_types::Endpoint;

        let path_msg = |call: VfsCallNum, mode: u32| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: call as i32,
                ..Message::default()
            };
            // SAFETY: mess_lc_vfs_path：name@0、len@8、flags@16、mode@20、
            // 内联路径@24。Creat 必须带 O_CREAT（C `do_creat` 的门，缺席即
            // EINVAL），Mkdir 不看 flags。
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

        // umask 0（新进程默认）：全保留。
        assert_eq!(run(VfsCallNum::Mkdir, 0o777, 0), crate::open::S_IFDIR | 0o777);
        assert_eq!(run(VfsCallNum::Creat, 0o666, 0), crate::open::S_IFREG | 0o666);
        // umask 022：按位收窄（不是"只留掩码里的位"）。
        assert_eq!(run(VfsCallNum::Mkdir, 0o777, 0o022), crate::open::S_IFDIR | 0o755);
        assert_eq!(run(VfsCallNum::Creat, 0o666, 0o022), crate::open::S_IFREG | 0o644);
    }

    /// `Lseek` 位置不变那条路的**回复载荷不能被统一收尾覆盖**：C `do_lseek`
    /// 把新位置写进 `m_vfs_lc_lseek.offset`（open.c:665-666）并返回 OK，两条
    /// 出口（发不发抑制预读）都要带。这里走完整的 `run_once`（不是只调
    /// dispatch），因为覆盖发生在 `run_once` 尾部的统一 `queue_reply`。
    #[test]
    fn test_lseek_unchanged_position_keeps_payload_through_run_once() {
        use minix_types::Endpoint;

        let mut state = seeded(100);
        // filp：位置 100，常规文件。
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
        // lseek(3, 100, SEEK_SET)：新位置 == 当前位置 → 不打扰 FS。
        let mut m = Message {
            m_source: Endpoint::from_generation_slot(1, 0),
            m_type: VfsCallNum::Lseek as i32,
            ..Message::default()
        };
        // SAFETY: lseek 载荷 offset@0、fd@8、whence@12（M7 覆盖层）。
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
        // SAFETY(test): 偏移在负载区首字（mess_vfs_lc_lseek）。
        let offset = unsafe { i64::from_le_bytes(reply.m_u.raw[0..8].try_into().unwrap()) };
        assert_eq!(offset, 100, "新位置必须留在回复载荷里");
    }

    /// `Fchmod` 臂（fd 半）：fd 门（只要 fd 有效，**不查打开模式**——与
    /// ftruncate 的 W 位门不同，C `do_chmod:88-93`）→ 属主门（EPERM）→
    /// 只读门（EROFS）→ setgid 清位 → `REQ_CHMOD`；回复带整字模式并写回缓存。
    #[test]
    fn test_dispatch_fchmod_gates_request_and_cache() {
        use minix_types::Endpoint;

        let fchmod_msg = |fd: i32, mode: u32| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Fchmod as i32,
                ..Message::default()
            };
            // SAFETY: fchmod 载荷 fd@0、mode@8（ipc.h:647-652）。
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[8..12].copy_from_slice(&mode.to_le_bytes());
            }
            m
        };
        let user_e = Endpoint::from_generation_slot(1, 0);
        // 现场：fd 3 → filp（只读打开）→ vnode（属主 1000、属组 100、0644）。
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

        // ① 负 fd 与空槽 → EBADF。
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

        // ② 非属主 → EPERM（收尾路径统一报挂起，回复已入队）。
        let (mut state, _vid, _idx) = setup(2000, 100, false);
        state.current_message = fchmod_msg(3, 0o600);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchmod),
            SyscallResult::Suspend
        );
        assert!(state.pending_fs.is_none());
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user_e, minix_types::EPERM))
        );

        // ③ 属主但只读挂载 → EROFS。
        let (mut state, _vid, _idx) = setup(1000, 100, true);
        state.current_message = fchmod_msg(3, 0o600);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchmod),
            SyscallResult::Suspend
        );
        assert!(state.pending_fs.is_none());
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((user_e, minix_types::EROFS))
        );

        // ④ 属主 + 可写：setgid 位被清（文件组 100 == 有效组 100 → **不清**，
        // 这里让有效组不同才清）。
        let (mut state, vid, idx) = setup(1000, 999, false);
        state.current_message = fchmod_msg(3, 0o2664);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchmod),
            SyscallResult::Suspend
        );
        let p = state.pending_fs.as_ref().expect("已登记 REQ_CHMOD");
        assert_eq!(p.req.m_type, minix_types::REQ_CHMOD);
        // SAFETY(test): 按 chmod_req_off 读回 inode/mode。
        unsafe {
            let raw = &p.req.m_u.raw;
            let ino = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let mode = u32::from_le_bytes(raw[8..12].try_into().unwrap());
            assert_eq!(ino, 0x21);
            assert_eq!(mode, 0o664, "不同组 → setgid 被清");
        }

        // 回复带整字模式（含类型位）→ 写回缓存。
        state.pending_fs = None;
        let mut reply = Message { m_type: 0, ..Message::default() };
        // SAFETY(test): 按 chmod_reply_off 填新模式。
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

    /// `Readlink` 臂的窗口门（C link.c:486）：`bufsize > SSIZE_MAX` 即
    /// EINVAL，且这条门在取路径与走遍历之前生效。
    #[test]
    fn test_dispatch_readlink_buffer_gate() {
        use minix_types::Endpoint;

        let readlink_msg = |name_len: u64, buf_size: u64| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Readlink as i32,
                ..Message::default()
            };
            // SAFETY: readlink 载荷 name@0、namelen@8、buf@16、bufsize@24
            // （ipc.h:785-792）——内联路径从 @32 起。
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[8..16].copy_from_slice(&name_len.to_le_bytes());
                raw[16..24].copy_from_slice(&0x5000u64.to_le_bytes());
                raw[24..32].copy_from_slice(&buf_size.to_le_bytes());
                raw[32] = b'/';
                raw[33] = b'x';
                raw[34] = 0;
            }
            m
        };

        let mut state = seeded(100);
        state.current_message = readlink_msg(3, u64::MAX);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Readlink),
            SyscallResult::Error(minix_types::EINVAL)
        );
        // 窗口合法则过门 → 起走遍历（挂起）。基线要先铺好：挂载行 + 调用方
        // 根目录 vnode + 热身过的 grant 表，缺任一项都会以 EIO 收场。
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
        state.current_message = readlink_msg(3, 128);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Readlink),
            SyscallResult::Suspend
        );
        // 末组件符号链接要带回：遍历标志必须是 RET_SYMLINK（C link.c:489）。
        // SAFETY(test): 按 lookup_req_off 读 FLAGS 域。
        let p = state.pending_fs.as_ref().expect("已登记 REQ_LOOKUP");
        unsafe {
            let raw = &p.req.m_u.raw;
            let flags = u32::from_le_bytes(raw[16..20].try_into().unwrap());
            assert_eq!(flags, minix_types::PATH_RET_SYMLINK);
        }
    }

    /// `Access` 臂的位掩码先验（C protect.c:216-217）：`F_OK`(0) 之外只认
    /// `R_OK|W_OK|X_OK`（4|2|1）的组合——多出别的位就是 EINVAL，且这条门
    /// 在取路径与走遍历**之前**生效（别把它放到权限判断之后）。
    #[test]
    fn test_dispatch_access_mode_gate() {
        use minix_types::Endpoint;

        let access_msg = |mode: u32| {
            let mut m = Message {
                m_source: Endpoint::from_generation_slot(1, 0),
                m_type: VfsCallNum::Access as i32,
                ..Message::default()
            };
            // SAFETY: access 载荷与 open 同形（name@0、len@8、mode@20、
            // 内联路径@24）——给一条两字节路径，让位掩码门之后的取路径与
            // 走遍历真的被执行到。
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[8..16].copy_from_slice(&3u64.to_le_bytes());
                raw[20..24].copy_from_slice(&mode.to_le_bytes());
                raw[24] = b'/';
                raw[25] = b'x';
                raw[26] = 0; // fetch_name 的长度含结尾 NUL（utility.c:60-90）
            }
            m
        };

        let mut state = seeded(100);
        // 0o10 = 8：多出 R/W/X 之外的位 → EINVAL。
        state.current_message = access_msg(0o10);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Access),
            SyscallResult::Error(minix_types::EINVAL)
        );
        // 0o7（R|W|X 全给）与 0（F_OK）都过门 → 起走遍历（挂起，等 FS）。
        // 基线要先铺好：挂载行 + 调用方根目录 vnode + 热身过的 grant 表
        // （缺任一项都会以 EIO 收场，断言就"理由不对"了）。
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
        // `Open`（含 `O_TRUNC`）/`Mkdir`/`Creat`/`Getdents`/`Access`/`Readlink`/
        // `Chmod`/`Unlink`/`Rmdir`。这里取还没接的 `Rename` 作代表——它同属
        // "FS 对话族"，而且是同族里唯一还需要"两个父目录"的臂。
        let mut state = seeded(100);
        let call = VfsCallNum::Rename;
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
