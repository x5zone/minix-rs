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
extern crate alloc;


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
        | VfsCallNum::Mount
        | VfsCallNum::Umount
        | VfsCallNum::Svrctl
        | VfsCallNum::Vmcall
        | VfsCallNum::Socketpath
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
        // ── 路径臂模板：chdir / chroot / fchdir（走完本地改目录）──
        VfsCallNum::Chdir | VfsCallNum::Chroot | VfsCallNum::Fchdir => {
            // C `do_chdir`/`do_chroot`（stadir.c:48-107）与 `do_fchdir`
            // （stadir.c:32-46）：都归结到 `change_into`（走完即判即改，
            // **没有 FS 往返**）。chroot 多一道"只有超级用户"的门。
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
                // C stadir.c:38-42：fd → filp → vnode，然后同一个 `change_into`。
                let fd = {
                    // SAFETY: `mess_lc_vfs_fchdir { int fd; }`（ipc.h:640-644）。
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
            // path 半：与 access/chmod 同形（内联路径），走完本地改目录。
            let (name_len, inline) = {
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
            // C stadir.c:65 的 `lookup_init(..., PATH_NOFLAGS, ...)`。
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
                    grant: 0, // 由 send_lookup_for_slot 覆写
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

        // ── 路径臂模板：rename（**三段链**：旧父目录 → [粘滞位子遍历] → 新父目录）──
        // ── statvfs 族（Statvfs1 / Fstatvfs1）：取挂载行 → fill_statvfs ──
        // ── getvfsstat（多挂载序列 + 用户缓冲按 i*sizeof 偏移 + 返回个数）──
        // ── mapdriver（只有 RS 能调；标签 → 端点 → dmap/smap 登记）──
        // ── fcntl（大部分是本地 fd 表操作；锁与 F_FREESP 要跨空间拷 flock）──
        // ── ioctl（按文件类型分派；设备对话管线未接线，见下面的注记）──
        // ── copyfd（驱动回调用：在调用方与远端之间搬/关一个 fd）──
        // ── pipe2（向 PFS 要一个新 inode，再装配 fd 对）──
        // ── socket 族（共同的本地门：域 → 驱动表、fd → socket、资源够不够）──
        VfsCallNum::Socket | VfsCallNum::Socketpair => {
            // C `do_socket`（socket.c:176-216）/ `do_socketpair`
            // （socket.c:224-273）：载荷都是 `mess_lc_vfs_socket`
            // （domain@0、type@4、protocol@8）。
            let (domain, ty, _protocol) = {
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
                let raw = unsafe { &msg.m_u.raw };
                let domain = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let ty = i32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]);
                let protocol = i32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]);
                (domain, ty, protocol)
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            // C socket.c:185-187 —— 这个域有没有套接字驱动？没有就是
            // `EAFNOSUPPORT`（本地判定：查 smap 的域映射表）。
            if crate::device_map::smap_by_domain(&state.smap_table, domain).is_none() {
                return SyscallResult::Error(minix_types::EAFNOSUPPORT);
            }
            // C socket.c:204-205 / :242-243 —— 先确认进程有足够的 fd 槽
            // （socket 要 1 个、socketpair 要 2 个）。
            let want = if matches!(call, VfsCallNum::Socketpair) { 2 } else { 1 };
            let enough = state
                .fproc_table
                .get(fp_slot)
                .map(|fp| crate::filedes::check_fds(fp, want).is_ok())
                .unwrap_or(false);
            if !enough {
                return SyscallResult::Error(minix_types::EMFILE);
            }
            // C 的 `sock_type = type & ~SOCK_FLAGS_MASK` 与 `get_sock_flags`
            // （决策函数 `socket::strip_sock_type`）：类型位与打开标志分开。
            let (_sock_type, _flags) = crate::socket::strip_sock_type(ty as u32);
            // C socket.c:207-214 —— `sdev_socket(domain, sock_type, protocol,
            // &dev, pair)`：给该域的套接字驱动发 `SDEV_SOCKET`，回复带新的
            // 套接字号。驱动对话管线（smap 取端点 + 发送 + 回复落槽）已就位，
            // 所以这里真发；回复到达后由 `WorkerCont::SdevSocket` 接着做
            // `make_sock_fd`（PFS 建节点 + 装配 fd）。
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
            // user_endpt }`（ipc.h:2329-2337）；`m_type` 是
            // `SDEV_SOCKET`/`SDEV_SOCKETPAIR`（`pair` 决定）。
            let mut req = minix_types::Message {
                m_type: if matches!(call, VfsCallNum::Socketpair) {
                    minix_sockdriver::sdev::SdevRequest::SocketPair as i32
                } else {
                    minix_sockdriver::sdev::SdevRequest::Socket as i32
                },
                ..minix_types::Message::default()
            };
            // SAFETY: 该请求的载荷按上述域序写在消息负载区。
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
            // C `do_shutdown`（socket.c:744-762）：`get_sock(fd)` 的两道门
            // （不是 fd → EBADF；不是套接字 → ENOTSOCK）→ `how` 的取值门
            // （EINVAL）→ `sdev_shutdown`（驱动对话，缺口）。
            let (fd, how) = {
                // SAFETY: `mess_lc_vfs_shutdown { int fd; int how; }`
                // （ipc.h:828-833）。
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
            // C socket.c:761 —— `sdev_shutdown(dev, how)`：走 `sdev_simple`
            // （`SDEV_SHUTDOWN` + `param = how`），回复号 `SDEV_REPLY`、状态在
            // 载荷里。管线已就位，所以这里真发。
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

        // ── getset 族（Setsockopt/Getsockopt/Getsockname/Getpeername）──
        VfsCallNum::Setsockopt | VfsCallNum::Getsockopt => {
            // C `do_setsockopt`/`do_getsockopt`（socket.c:657-698）：载荷
            // `mess_lc_vfs_sockopt`（fd@0、level@4、name@8、buf@16、len@24）。
            let (fd, level, name, buf, len) = {
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
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
            // C `do_getsockname`/`do_getpeername`（socket.c:701-733）：载荷
            // `mess_lc_vfs_sockaddr`（fd@0、addr@8、addr_len@16）；`level`/
            // `name` 传 0（C 里是 `sdev_get(dev, type, 0, 0, addr, &len)`）。
            let (fd, addr, addr_len) = {
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
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

        // ── bind / connect（**进程级挂起**：C 的 sdev_suspend 形状）──
        // ── sendto / recvfrom（数据 + 地址两张 grant，进程级挂起）──
        // ── sendmsg / recvmsg（msghdr + 单个 iovec；与 sendto/recvfrom 同一
        //    条 sdev_readwrite，多一个控制缓冲与 msg_buf）──
        // ── select（本地半：门、fd 校验、就绪判定与立即返回）──
        VfsCallNum::Select => {
            // C `do_select`（select.c:94-260）：载荷 `mess_lc_vfs_select`
            // （nfds@0、readfds@8、writefds@16、errorfds@24、timeout@32）。
            let (nfds, readfds, writefds, errorfds, timeout) = {
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
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
            match do_select(
                state,
                fp_slot,
                nfds as usize,
                readfds,
                writefds,
                errorfds,
                timeout,
                &io,
            ) {
                Ok(count) => SyscallResult::Ok(count),
                Err(e) => SyscallResult::Error(e),
            }
        }

        VfsCallNum::Sendmsg | VfsCallNum::Recvmsg => {
            // C `do_sockmsg`（socket.c:538-595）：载荷 `mess_lc_vfs_sockmsg`
            // （fd@0、msgbuf@8、flags@16）。取用户 `struct msghdr`（跨空间）→
            // iov 门（多元素向量不支持，> 1 即 EMSGSIZE；libc 会合并）→ 单条
            // `iovec` → `sdev_readwrite(dev, data, ctl, addr, flags, ...)`。
            let (fd, msgbuf, flags) = {
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
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
            // 取用户 msghdr（生产件是跨空间拷贝；宿主下 → EINVAL）。
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
            // C `do_sendto`/`do_recvfrom`（socket.c:483-518）+ `sdev_readwrite`
            // （sdev.c:336-410）：载荷 `mess_lc_vfs_sendrecv`
            // （fd@0、buf@8、len@16、flags@24、addr@32、addr_len@40）。
            let (fd, buf, len, flags, addr, addr_len) = {
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
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
            // C sdev.c:396-400 —— 打开标志翻成消息标志（非阻塞 + 写方向的
            // NOSIGPIPE），与用户给的 flags 按位或。
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

        // ── accept（进程级挂起 + 三态收尾；见 `finish_sdev_blocked`）──
        VfsCallNum::Accept => {
            // C `do_accept`（socket.c:363-378）：`get_sock` → `check_sock_fds(1)`
            // → `sdev_accept(dev, addr, addr_len, flags, fd)`（socket.c 的
            // `sdev_accept` 用与 bind/connect 同一条 `mess_vfs_lsockdriver_addr`
            // 载荷，但地址 grant 是 `CPF_WRITE`——驱动要写对端地址回来）。
            let (fd, addr, addr_len) = {
                // SAFETY: mess_lc_vfs_sockaddr：fd@0、addr@8、addr_len@16。
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
            // C socket.c:373-374 —— 新套接字要占一个 fd 槽，先确认有。
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
            // C `sdev_accept`：地址缓冲**非零**才建 grant（`CPF_WRITE`）。
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
            // SAFETY: `mess_vfs_lsockdriver_addr`（与 bind/connect 同一条）。
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
                // listen_fd, 0)`：监听 fd 随现场带下去（收尾要读它的标志）。
                aux: crate::fproc::SdevAux::Fd(fd as usize),
            };
            match state.suspend_on_sdev(Some(fp_slot), worker, block) {
                Ok(()) => SyscallResult::Suspend,
                Err(e) => SyscallResult::Error(e),
            }
        }

        VfsCallNum::Bind | VfsCallNum::Connect => {
            // C `do_bind`/`do_connect`（socket.c:308-333）+ `sdev_bindconn`
            // （sdev.c:179-215）：载荷 `mess_lc_vfs_sockaddr`
            // （fd@0、addr@8、addr_len@16）。
            let (fd, addr, addr_len) = {
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
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
            // C sdev.c:192-194 —— 地址缓冲的 magic grant（驱动只读它）。
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
            // user_endpt; int sflags; }`（ipc.c:2060-2070 一类）。
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
            // C `asynsend3`：发出去**不等**（这类调用可能等很久）。
            if minix_sys::ipc::IpcTransport::send(&minix_sys::ipc::DirectTrapTransport, drv_e, &req)
                .is_err()
            {
                let _ = state.revoke_grant(grant);
                return SyscallResult::Error(minix_types::EIO);
            }
            // 进程级挂起：记下现场 + 释放 worker 槽（C `sdev_suspend`）。
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
            // C `do_listen`（socket.c:334-357）：`get_sock` → backlog 负值归零
            // （`socket::clamp_backlog`）→ `sdev_listen(dev, backlog)`（
            // `SDEV_LISTEN` + `param = backlog`，走 `sdev_simple`）。
            let (fd, backlog) = {
                // SAFETY: `mess_lc_vfs_listen { int fd; int backlog; }`
                // （ipc.h:851-856 一类）。
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
            // C `do_pipe2`（pipe.c:39-55）+ `create_pipe`（pipe.c:58-135）：
            // 载荷 `mess_lc_vfs_pipe2`（flags@0、_unused@4、oflags@8）——两个
            // flags 字段按位或（向后兼容）；回复载荷是 `m_vfs_lc_fdpair`。
            let (flags, oflags) = {
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
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
            // C pipe.c:70-71 —— `find_vmnt(PFS_PROC_NR)`，拿不到就 panic
            // （"PFS gone"）。Rust 侧**不 panic**：PFS 是另一条线的服务器，
            // 未挂载时这里 fail-closed 回 EIO 并记缺口（不假装建成了管道）。
            let Some(vmnt_id) = state.vmnt_table.find_by_fs(minix_types::Endpoint::PFS) else {
                return SyscallResult::Error(minix_types::EIO);
            };
            // C pipe.c:75-78 —— 预留一个 vnode（`get_free_vnode`）。
            let vnode = match state.vnode_table.alloc() {
                Ok(v) => v,
                Err(_) => return SyscallResult::Error(minix_types::ENFILE),
            };
            // C pipe.c:81-110 —— 两个 fd + 两个 filp（读端 R、写端 W）；第二步
            // 失败要按 `rollback_for` 把第一步拆掉。
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
                        // `rollback_for(FdWrite)`：拆掉读端与 vnode。
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
                grant: 0, // 无数据面
                user: user_e,
                // C pipe.c:112-113：`req_newnode(PFS, effuid, effgid,
                // I_NAMED_PIPE, NO_DEV, &res)`。
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
            // C `do_copyfd`（filedes.c:524-650）：载荷 `mess_lsys_vfs_copyfd`
            // （endpt@0、fd@4、what@8）。**全程本地表操作**（没有 FS/驱动
            // 往返），决策与执行都在 `filedes::copy_fd` 里（V1 轮已备）。
            let (endpt_raw, fd, what) = {
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
                let raw = unsafe { &msg.m_u.raw };
                let endpt_raw = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let fd = i32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]);
                let what = i32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]);
                (endpt_raw, fd, what)
            };
            let Some(fp_slot) = state.current_fp_slot else {
                return SyscallResult::Error(minix_types::EINVAL);
            };
            // C filedes.c:540-541 —— `flags = what & COPYFD_FLAGS; what &=
            // ~COPYFD_FLAGS`：低四位是操作种类、`0x8000` 是 CLOEXEC 位。
            let flags = what & 0xF000;
            let op = what & !0xF000;
            let kind = match op {
                0 => crate::filedes::CopyKind::From,
                1 => crate::filedes::CopyKind::To,
                2 => crate::filedes::CopyKind::Close,
                _ => return SyscallResult::Error(minix_types::EINVAL),
            };
            let cloexec = flags & 0x8000 != 0;
            // C filedes.c:543-545 —— `isokendpt(endpt)`：远端必须是**活着的**
            // 进程。注意不能只判"槽在范围内"：`FProcTable` 的槽是预分配的，
            // 空槽也 `get()` 得到——要用 `is_ok_endpoint`（端点与槽里记的
            // 端点相符，正是 C 的 generation 校验）。
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
            // 两个槽同时借出（`get_two_mut`）——`copy_fd` 要同时拿调用方与
            // 远端的 fd 表；其余表经 `CopyFdCtx` 借。
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
            // C `do_ioctl`（device.c:18-58）：载荷 `mess_lc_vfs_ioctl`
            // （fd@0、req@8、arg@16）。取 filp/vnode 后按**文件类型**分派：
            // 块设备走 `bdev_ioctl`、字符设备走 `cdev_io(CDEV_IOCTL, ...)`、
            // 套接字走 `sdev_ioctl`，其余一律 `ENOTTY`（C 的 `default`）。
            let (fd, req, arg) = {
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
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
            // `filp_flags`：C `cdev_io(..., f->filp_flags)` 用它翻
            // `CDEV_NONBLOCK`。
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
            // 类型分派（决策函数 `device_map::ioctl_route`）：非设备文件
            // `ENOTTY`——这条是**本地判定**，与设备对话无关，所以先接上。
            let target = match crate::device_map::ioctl_route(crate::open::FileType::from(mode)) {
                Ok(t) => t,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            match target {
                crate::device_map::IoctlTarget::Char => {
                    // C `do_ioctl` 的 `S_IFCHR` 支（device.c:44-46）→
                    // `cdev_io(CDEV_IOCTL, vp->v_sdev, who_e, arg, 0, request,
                    // filp_flags)`（cdev.c:277-340）：dmap 按 major 找驱动 →
                    // `make_ioctl_grant`（access/size 由请求位解出来）→
                    // `CDEV_IOCTL` 消息 → 等回复。
                    // C `major(x)`/`minor(x)`（sys/types.h:290-292）：
                    // `major = (dev & 0x000fff00) >> 8`；
                    // `minor = ((dev & 0xfff00000) >> 12) | (dev & 0xff)`。
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
                    // C `make_ioctl_grant`：请求位解出 access 与 size，做一张
                    // magic grant（`CPF_WRITE`/`CPF_READ` 由方向位定）。
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
                        m_type: minix_chardriver::protocol::CdevRequest::Ioctl as i32,
                        ..minix_types::Message::default()
                    };
                    // SAFETY: `mess_vfs_lchardriver_readwrite { off_t pos@0;
                    // cp_grant_id_t grant@8; size_t count@16; unsigned long
                    // request@24; int flags@32; endpoint_t id@36; endpoint_t
                    // user@40; devminor_t minor@44 }`（ipc.h:2238-2249）。
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
                                let _ = state.revoke_grant(grant);
                            }
                            SyscallResult::Error(e)
                        }
                    };
                }
                crate::device_map::IoctlTarget::Sock => {
                    // C `do_ioctl` 的 `S_IFSOCK` 支（device.c:48-50）→
                    // `sdev_ioctl(dev, request, buf, filp_flags)`
                    // （sdev.c:415-448）：smap 按设备号找驱动 → `make_ioctl_grant`
                    // → `SDEV_IOCTL` 消息 → **进程级挂起**（套接字调用可能等）。
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
                    // C `make_ioctl_grant`（与字符设备那条同一套解码）。
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
                    // int sflags@24 }`（ipc.h:2284-2293）。
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
                    // C `do_ioctl` 的 `S_IFBLK` 支（device.c:36-42）→
                    // `bdev_ioctl(dev, who_e, request, arg)`（bdev.c:144-186）：
                    // dmap 按 major 找驱动（没有 → ENXIO）→ `make_ioctl_grant`
                    // → `BDEV_IOCTL` → **worker 等待**（块驱动的回复通常很快）。
                    // C 还在调用前后置/清 `filp_ioctl_fp`（防 ioctl 期间改同一
                    // 设备节点导致死锁）——本模型里是 `Filp::ioctl_holder`。
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
                    // C `f->filp_ioctl_fp = fp`：记下"这个 filp 正被谁的 ioctl
                    // 占着"（`copyfd` 用它挡死锁）。
                    if let Some(f) = state.filp_table.get_mut(crate::filp::FilpId(filp_idx)) {
                        f.ioctl_holder = Some(fp_slot);
                    }
                    let mut m = minix_types::Message {
                        m_type: crate::bdev::BdevOp::Ioctl.msg_type() as i32,
                        ..minix_types::Message::default()
                    };
                    // SAFETY: `mess_lbdev_lblockdriver_msg { int minor@0; int
                    // id@4; int access@8; int count@12; cp_grant_id_t grant@16;
                    // int flags@20; endpoint_t user@24; unsigned long
                    // request@32 }`（ipc.h:331-353）。
                    unsafe {
                        let raw = &mut m.m_u.raw;
                        raw[0..4].copy_from_slice(&(minor as i32).to_le_bytes());
                        raw[16..20].copy_from_slice(&grant.to_le_bytes());
                        raw[24..28].copy_from_slice(&user_e.0.to_le_bytes());
                        raw[32..40].copy_from_slice(&req.to_le_bytes());
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
            // C `do_fcntl`（misc.c:127-300）：载荷 `mess_lc_vfs_fcntl`
            // （fd@0、cmd@4、arg_int@8、arg_ptr@16）。未知 cmd → EINVAL
            // （C 的 `default: r = EINVAL`）。
            // `arg_ptr` 是锁类命令的 `struct flock *`（本批未接线，见下面
            // 的分支）；解析出来但不用，前缀下划线明示。
            let (fd, cmd_raw, arg_int, _arg_ptr) = {
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
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
            // C misc.c:141-143 —— fd 门（锁类命令要写锁，其余读锁）。
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
                // C misc.c:147-163 —— `dup` 家族的替代：floor 门 → 拿新 fd →
                // `filp_count++` → 写 fd 表（CLOEXEC 变体顺带置位）。
                C::DupFd | C::DupFdCloexec => {
                    if let Err(e) = crate::fcntl::dupfd_arg_check(arg_int) {
                        return SyscallResult::Error(e.to_errno());
                    }
                    // 与 open 不同：dup 家族**共用**同一个 filp（C 的
                    // `f->filp_count++` + `fp_filp[new_fd] = f`），所以只占一个
                    // fd 槽、不再分配 filp——直接问分配策略要最低空闲槽。
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
                // C misc.c:165-172 —— 读/写 close-on-exec 位。
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
                // C misc.c:174-187 —— 状态字（只让 `O_NONBLOCK|O_APPEND|O_ACCMODE`
                // 这类位过门）。
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
                // C misc.c:250-256 —— `O_NOSIGPIPE` 哨兵。
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
                // C misc.c:258-275 —— 只有超级用户能刷缓存；目标由文件类型定
                // （块设备刷自己的设备块、常规/目录刷宿主 FS）。
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
                    // 目标端点与设备号：块设备走 `v_bfs_e`/`v_sdev`（块驱动
                    // 的 FS 与设备），常规/目录走 `v_fs_e`/`v_dev`（宿主 FS）。
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
                        grant: 0, // 无数据面
                        user: user_e,
                        req: crate::request::encode_flush(dev),
                    });
                    return SyscallResult::Suspend;
                }
                // 锁类（`lock_op`）与 `F_FREESP` 都要跨空间拷用户的
                // `struct flock`（进/出两个方向），本批未接线 → 诚实拒绝。
                C::GetLk | C::SetLk | C::SetLkw | C::FreeSp => {
                    return SyscallResult::Error(minix_types::ENOSYS);
                }
            }
        }

        VfsCallNum::Mapdriver => {
            // C `do_mapdriver`（dmap.c:106-177）：载荷
            // `mess_lsys_vfs_mapdriver`（major@0、labellen@8、label@16、
            // ndomains@24、domains@28..，NR_DOMAIN = 8）。三道门在任何查表
            // 之前：只有 RS 能调（EPERM）、标签放得下（EINVAL）、标签以 NUL
            // 结尾（EINVAL）。
            let caller = msg.m_source;
            if crate::device_map::check_mapper(caller).is_err() {
                return SyscallResult::Error(minix_types::EPERM);
            }
            let (major, label_len, label_addr, ndomains, domains) = {
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
                let raw = unsafe { &msg.m_u.raw };
                let major = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                let mut b8 = [0u8; 8];
                b8.copy_from_slice(&raw[8..16]);
                let label_len = u64::from_le_bytes(b8);
                b8.copy_from_slice(&raw[16..24]);
                let label_addr = u64::from_le_bytes(b8);
                let ndomains = i32::from_le_bytes([raw[24], raw[25], raw[26], raw[27]]);
                // 域数组从 28 起，本重写的载荷区是 56 字节（`MESSAGE_PAYLOAD_SIZE`，
                // 与 32 位 Minix 的 message 对齐）——只能放 7 个 `int`，而 C 的
                // `NR_DOMAIN` 是 8。多出来的域读不到，所以 `ndomains > 7` 一律
                // EINVAL（**不静默丢域**：少注册一个域会让驱动半残）。
                let mut doms = [0i32; 7];
                for (i, d) in doms.iter_mut().enumerate() {
                    let at = 28 + i * 4;
                    *d = i32::from_le_bytes([raw[at], raw[at + 1], raw[at + 2], raw[at + 3]]);
                }
                (major, label_len, label_addr, ndomains, doms)
            };
            // C dmap.c:129-133 —— 标签放不下即 EINVAL（`LABEL_MAX` = 16）。
            if label_len as usize > crate::device_map::LABEL_MAX {
                return SyscallResult::Error(minix_types::EINVAL);
            }
            // 载荷区容量门（见上面域数组的注记）：C 允许到 `NR_DOMAIN` = 8，
            // 本重写的 56 字节载荷区只放得下 7 个域——多的拒掉而不是丢。
            if ndomains > 7 {
                return SyscallResult::Error(minix_types::EINVAL);
            }
            // 本臂**不需要 worker 槽**：登记全是本地表操作（dmap/smap），没有
            // FS/驱动往返，所以不像别的对话臂那样要等回复。
            // C dmap.c:134-139 —— `sys_vircopy(who_e, label_vir, SELF, label,
            // label_len, CP_FLAG_TRY)`：标签在**调用方内存**里（跨空间取，
            // 宿主不可达 → EINVAL，与 C 的 `r != OK → EINVAL` 同值）。
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
            // C dmap.c:140-144 —— 必须以 NUL 结尾。
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
            // C `do_getvfsstat`（stadir.c:330-403）：载荷
            // `mess_lc_vfs_getvfsstat { buf@0, len@8, flags@16 }`。`buf == 0`
            // 只数个数；否则按 `bufsize / sizeof(struct statvfs)` 截断挂载
            // 列表，逐个填到 `buf + i*sizeof`，返回**个数**（不是 0）。
            let (buf_addr, bufsize, flags) = {
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
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
            // 两种结局（挂起 / 就地收尾）入口统一报挂起——同 Fstatvfs1。
            match state.begin_getvfsstat(worker, Some(fp_slot), buf_addr, bufsize, flags) {
                Ok(()) => SyscallResult::Suspend,
                Err(e) => SyscallResult::Error(e),
            }
        }

        VfsCallNum::Statvfs1 | VfsCallNum::Fstatvfs1 => {
            // C `do_statvfs`（stadir.c:294-326，路径半）与 `do_fstatvfs`
            // （stadir.c:419-442，fd 半）：都归结到 `fill_statvfs(vp->v_vmnt,
            // who_e, statbuf, flags)`——载荷 `mess_lc_vfs_statvfs1`
            // （fd@0、flags@4、len@8、name@16、buf@24）。
            let (fd, flags, name_len, name_addr, statbuf) = {
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
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
                // C stadir.c:428-431：fd → filp → vnode 的 `v_vmnt`。
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
                // `begin_statvfs` 可能"已挂起"（新鲜路径）也可能"已收尾"
                // （`ST_NOWAIT` 走缓存、就地补字段并回信）——两种结局都报挂起：
                // 收尾时槽已被释放、回复已入队，`run_once` 见 Suspend 不再释放，
                // `queue_reply(Suspend)` 也不会覆盖已入队的回复。
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
            // 路径半：C stadir.c:305-313 的 `eat_path`（NOFLAGS）。
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
                    grant: 0, // 由 send_lookup_for_slot 覆写
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
            // C `do_rename`（link.c:166-280）：载荷与 link 同形
            // （name1@0、name2@8、len1@16、len2@24）。阶段 1 走 name1 的父目录
            // （`last_dir`），阶段 2 走 name2 的父目录，中间可能插一段粘滞位
            // 子遍历（旧父目录带 `S_ISVTX` 时）。
            let (old_addr, new_addr, old_len, new_len) = {
                // SAFETY: 该调用号的载荷按上述域序写在消息负载区。
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
            // C `do_rename:196`：`last_dir` 切出旧名的最后组件——它要被**保存**
            // 下来（阶段 2 发请求时用），所以随 follow 带过去。
            let split = match crate::path::last_dir_split(&old_path) {
                Ok(sp) => sp,
                Err(e) => return SyscallResult::Error(e.to_errno()),
            };
            // C `do_rename:188` 的 `lookup_init(..., PATH_RET_SYMLINK, ...)`；
            // 目录前缀里的符号链接要跟进（`path.c:231-235` 清 RET_SYMLINK）。
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
/// `do_sockmsg` 的共用体（C `do_sockmsg` socket.c:538-595）：取用户
/// `struct msghdr`（经 `MsgHdrFetcher` 缝）→ iov 门（`> 1` 即 `EMSGSIZE`）→
/// 单条 `iovec` 的长度门（`> SSIZE_MAX` → `EINVAL`）→ 与 sendto/recvfrom 同一条
/// `send_sdev_readwrite`（`recvmsg` 还要把 `msg_buf` 带下去——收尾要回写
/// msghdr 的几个字段）。
///
/// 抽成自由函数是为了**可注入取数件**：宿主下生产件取不到 msghdr，但"取到之后"
/// 的 iov 门与请求形状可以用脚本替身测。
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
    // `recvmsg` 的收尾要回写 msghdr，所以 `msg_buf` 随现场带下去
    // （C `sdev_readwrite(..., user_buf)` 的最后一参）。
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

/// `do_select` 的**本地半**（C `do_select` select.c:94-260）：`nfds` 门 → 取一个
/// select 槽（满即 `ENOSPC`）→ 拷三张 fd 集 → 逐 fd 校验（`tab2ops` 空位跳过、
/// `get_filp` 失败 `EBADF`、类型不认识 `EBADF`）→ 就绪判定（模式位不符与常规
/// 文件立刻算就绪）→ 立即返回（`should_return`）。
///
/// **未接的一半**：字符/套接字驱动的 `select_request` 与管道探测、以及"没有就绪
/// 就挂起等驱动/超时"那条路——它们要驱动对话（管线已就位）与定时器，是下一步。
/// 走到那一步时诚实回 `ENOSYS` 并把槽放回。
///
/// 抽成自由函数是为了可注入取存件（宿主下生产件取不到用户 fd_set）。
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
) -> Result<i32, i32> {
    // C select.c:110-111 —— `nfds < 0 || nfds > OPEN_MAX` 即 EINVAL。
    if nfds > crate::fproc::OPEN_MAX {
        return Err(minix_types::EINVAL);
    }
    // C select.c:118-124 —— 找一个空槽；满了 ENOSPC。
    let Some(slot_idx) = state.select_table.alloc() else {
        return Err(minix_types::ENOSPC);
    };
    let bytes = crate::select::fdset_bytes(nfds).ok_or(minix_types::EINVAL)?;
    // C `copy_fdsets(se, nfds, FROM_PROC)`：三张集分别拷进来（指针为 0 的
    // 那几张跳过——`select` 允许只关心其中一部分）。
    let mut read_set = alloc::vec::Vec::new();
    let mut write_set = alloc::vec::Vec::new();
    let mut error_set = alloc::vec::Vec::new();
    if readfds != 0 {
        read_set = io.fetch(readfds, bytes)?;
    }
    if writefds != 0 {
        write_set = io.fetch(writefds, bytes)?;
    }
    if errorfds != 0 {
        error_set = io.fetch(errorfds, bytes)?;
    }
    // 超时：C `plan_timeout`（无 timeval = 永远等；(0,0) = poll）。
    let plan = if timeout == 0 {
        crate::select::TimeoutPlan::Forever
    } else {
        // 用户 `struct timeval { time_t tv_sec; suseconds_t tv_usec; }`（LP64
        // 两个 8 字节域）——取它也要跨空间（同一个缝不覆盖，暂用宿主不可达的
        // 直取，失败即 EINVAL）。
        let mut raw = [0u8; 16];
        minix_sys::syscall::sys_datacopy(
            &minix_sys::syscall::DirectKernelCallTransport,
            io_who(state, fp_slot),
            timeout,
            minix_types::Endpoint::SELF.0,
            raw.as_mut_ptr() as u64,
            16,
        )
        .map_err(|e| -e)?;
        let sec = i64::from_le_bytes(raw[0..8].try_into().unwrap());
        let usec = i64::from_le_bytes(raw[8..16].try_into().unwrap());
        crate::select::plan_timeout(true, sec, usec, 60).map_err(|e| e.to_errno())?
    };
    let block = crate::select::block_of(plan);
    {
        let Some(se) = state.select_table.get_mut(slot_idx) else {
            return Err(minix_types::EIO);
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
    let bit = |set: &[u8], fd: usize| -> bool {
        let byte = fd / 8;
        let mask = 1u8 << (fd % 8);
        set.get(byte).is_some_and(|b| b & mask != 0)
    };
    let mut nready = 0usize;
    let mut pending = 0usize;
    for fd in 0..nfds {
        let (rd, wr, er) = {
            let Some(se) = state.select_table.get(slot_idx) else {
                return Err(minix_types::EIO);
            };
            (
                bit(&se.readfds, fd),
                bit(&se.writefds, fd),
                bit(&se.errorfds, fd),
            )
        };
        let ops = crate::select::tab2ops(rd, wr, er);
        if ops.is_empty() {
            continue; // C：这一位没设，跳过
        }
        // C `get_filp(fd, VNODE_READ)`：拿不到就是 EBADF。
        let Some(fp) = state.fproc_table.get(fp_slot) else {
            return Err(minix_types::EINVAL);
        };
        let Some(filp_idx) = fp.filps.get(fd).copied().flatten() else {
            state.select_table.release(slot_idx);
            return Err(minix_types::EBADF);
        };
        let Some(filp) = state.filp_table.get(crate::filp::FilpId(filp_idx)) else {
            state.select_table.release(slot_idx);
            return Err(minix_types::EBADF);
        };
        let mode = filp.mode;
        let Some(vnode_idx) = filp.vnode else {
            state.select_table.release(slot_idx);
            return Err(minix_types::EBADF);
        };
        let Some(v) = state.vnode_table.get(crate::vnode::VnodeId(vnode_idx)) else {
            state.select_table.release(slot_idx);
            return Err(minix_types::EBADF);
        };
        let ft = crate::open::FileType::from(v.mode);
        // C `fdtypes[type].type_match` 找不到类型 → EBADF。
        let Some(kind) = crate::select::classify(
            ft == crate::open::FileType::Char,
            ft == crate::open::FileType::Socket,
            ft == crate::open::FileType::Regular,
            ft == crate::open::FileType::Fifo,
        ) else {
            state.select_table.release(slot_idx);
            return Err(minix_types::EBADF);
        };
        // C select.c:213-226 —— 打开模式与请求方向不符的**立刻算就绪**
        // （随后的读写必然失败）；常规文件永远就绪（`FdKind::File`）。
        if rd && mode & crate::open::R_BIT == 0 {
            nready += 1;
        }
        if wr && mode & crate::open::W_BIT == 0 {
            nready += 1;
        }
        match kind {
            crate::select::FdKind::File => {
                // `select_request_file`：永远就绪。
                if rd {
                    nready += 1;
                }
                if wr {
                    nready += 1;
                }
            }
            // 字符/套接字/管道要问驱动或探测——**未接**（见函数注记）。
            _ => pending += 1,
        }
    }
    if pending > 0 {
        state.select_table.release(slot_idx);
        return Err(minix_types::ENOSYS);
    }
    // 立即返回（C `should_return`）：没有要等的，就把就绪集拷回去并回个数。
    let should = crate::select::should_return(nready, false, block, false);
    if !should {
        state.select_table.release(slot_idx);
        return Err(minix_types::ENOSYS);
    }
    let Some(se) = state.select_table.get(slot_idx) else {
        return Err(minix_types::EIO);
    };
    // 结果集：C `ops2tab` 把**就绪位**写进副本，最后整块拷回用户。三张集各
    // 有各的结果（`errorfds` 这一轮没有会置位的来源——驱动那一半未接）。
    let (mut out_rd, mut out_wr) = (alloc::vec![0u8; bytes], alloc::vec![0u8; bytes]);
    for fd in 0..nfds {
        let (rd, wr) = (bit(&se.readfds, fd), bit(&se.writefds, fd));
        if !rd && !wr {
            continue;
        }
        let Some(fp) = state.fproc_table.get(fp_slot) else {
            return Err(minix_types::EINVAL);
        };
        let Some(filp_idx) = fp.filps.get(fd).copied().flatten() else {
            continue;
        };
        let Some(filp) = state.filp_table.get(crate::filp::FilpId(filp_idx)) else {
            continue;
        };
        let Some(vnode_idx) = filp.vnode else { continue };
        let Some(v) = state.vnode_table.get(crate::vnode::VnodeId(vnode_idx)) else {
            continue;
        };
        let ft = crate::open::FileType::from(v.mode);
        // C select.c:213-226：打开模式与请求方向不符 → 立刻就绪；常规文件
        // （`FdKind::File`）两个方向都就绪。
        if rd && (filp.mode & crate::open::R_BIT == 0 || ft == crate::open::FileType::Regular) {
            out_rd[fd / 8] |= 1u8 << (fd % 8);
        }
        if wr && (filp.mode & crate::open::W_BIT == 0 || ft == crate::open::FileType::Regular) {
            out_wr[fd / 8] |= 1u8 << (fd % 8);
        }
    }
    let count = nready as i32;
    let (vir_read, vir_write, vir_err) = (se.vir_readfds, se.vir_writefds, se.vir_errorfds);
    state.select_table.release(slot_idx);
    if vir_read != 0 {
        io.store(vir_read, &out_rd)?;
    }
    if vir_write != 0 {
        io.store(vir_write, &out_wr)?;
    }
    if vir_err != 0 {
        io.store(vir_err, &alloc::vec![0u8; bytes])?;
    }
    Ok(count)
}

/// 调用方端点（超时结构体的跨空间取要用）。
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

    /// 套接字族的第一批门（C `do_socket`/`do_socketpair` socket.c:176-273 与
    /// `do_shutdown` socket.c:744-762）：**域没有驱动 → EAFNOSUPPORT**、
    /// **fd 槽不够 → EMFILE**、**fd 不是套接字 → ENOTSOCK**、**how 取值非法 →
    /// EINVAL**。这些都是本地判定；下一步的驱动对话（`sdev_*`）管线未接线，
    /// 所以过门之后诚实回 ENOSYS。
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
            // SAFETY: mess_lc_vfs_socket：domain@0、type@4、protocol@8。
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
            // SAFETY: mess_lc_vfs_shutdown：fd@0、how@4。
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[4..8].copy_from_slice(&how.to_le_bytes());
            }
            m
        };

        // ① 域没有套接字驱动 → EAFNOSUPPORT（smap 的域映射表空着）。
        let (mut state, _idx) = setup();
        state.current_message = socket_msg(VfsCallNum::Socket, 2, 1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Socket),
            SyscallResult::Error(minix_types::EAFNOSUPPORT)
        );

        // ② 域映射了（pfmap[2] = 行 0）→ 过域门，真给驱动发 `SDEV_SOCKET`；
        // 宿主构建下 trap 不可达 → EIO（诚实边界）。
        let (mut state, _idx) = setup();
        state.smap_table.pfmap[2] = Some(0);
        state.smap_table.entries[0].endpt = Some(Endpoint::from_generation_slot(0, 11));
        state.current_message = socket_msg(VfsCallNum::Socket, 2, 1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Socket),
            SyscallResult::Error(minix_types::EIO),
            "驱动对话要真发（宿主下 trap 不可达）"
        );

        // ③ fd 槽不够 → EMFILE（socketpair 要 2 个：只留 1 个空槽）。
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

        // ④ shutdown：fd 无效 → EBADF；fd 有效但不是套接字 → ENOTSOCK；
        // 是套接字但 how 非法 → EINVAL；全过 → ENOSYS（驱动对话缺口）。
        let (mut state, _idx) = setup();
        state.current_message = shutdown_msg(9, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Shutdown),
            SyscallResult::Error(minix_types::EBADF)
        );
        // 挂一个常规文件的 fd 3 → ENOTSOCK。
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
        // 换成套接字 vnode → 过类型门；how 非法 → EINVAL。
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

    /// `Bind`/`Connect`（**进程级挂起**）：门同 socket 族；过了就给驱动发
    /// `SDEV_BIND`/`SDEV_CONNECT`（地址缓冲做成 magic grant）**不等回复**，
    /// 把进程标成 `FP_BLOCKED_ON_SDEV` 并**释放 worker 槽**；驱动回复到达时
    /// 由 `finish_sdev_blocked` 收尾（撤 grant + 状态回用户）。
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
            // SAFETY: mess_lc_vfs_sockaddr：fd@0、addr@8、addr_len@16。
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[8..16].copy_from_slice(&0x6000u64.to_le_bytes());
                raw[16..24].copy_from_slice(&16u64.to_le_bytes());
            }
            m
        };

        // ① 门：负 fd → EBADF；非套接字 → ENOTSOCK。
        let (mut state, _idx, _drv, _dev) = setup();
        state.current_message = bind_msg(VfsCallNum::Bind, -1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Bind),
            SyscallResult::Error(minix_types::EBADF)
        );
        // 把 fd 3 指的那个 vnode 换成常规文件（别猜下标：从 filp 取）。
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

        // ② 宿主下 trap 不可达 → 发送失败 → EIO，且不留悬空 grant、不挂起。
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

    /// `Ioctl` 的套接字分支（C `do_ioctl` 的 `S_IFSOCK` 支 + `sdev_ioctl`）：
    /// smap 按设备号找驱动 → `make_ioctl_grant`（access/size 解码 + magic grant）
    /// → `SDEV_IOCTL` → **进程级挂起**；回复走通用 `SDEV_REPLY`（状态在载荷
    /// 第二格），由 `finish_sdev_blocked` 收尾。
    #[test]
    fn test_ioctl_socket_branch_and_reply() {
        use minix_types::Endpoint;

        let mut state = seeded(100);
        crate::main_loop::seed_ready_state(&mut state);
        let drv = Endpoint::from_generation_slot(0, 11);
        state.smap_table.entries[0].endpt = Some(drv);
        let dev = crate::device_map::make_smap_dev(state.smap_table.entries[0].num, 0x42);
        // fd 3 → 套接字 vnode（`v_sdev` = 那个设备号）。
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

        // `ioctl(fd, FIONBIO 一类)`：宿主下发送失败 → EIO，且**不挂起**、
        // 不留悬空 grant。
        let mut m = Message {
            m_source: Endpoint::from_generation_slot(1, 0),
            m_type: VfsCallNum::Ioctl as i32,
            ..Message::default()
        };
        // SAFETY: mess_lc_vfs_ioctl：fd@0、req@8、arg@16。请求位带 IOC_IN
        // （`_IOW`：驱动读用户缓冲），size 位非零 → 会建 grant。
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

        // 手工摆出"已挂起"的现场，喂通用 `SDEV_REPLY` → 用户拿到载荷里的
        // status（第二格），挂起态清掉。
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
        // SAFETY(test): `mess_lsockdriver_vfs_reply { req_id@0; status@4; }`。
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

    /// `Socketpair` 的成对编排（C `do_socketpair` socket.c:224-266）：驱动回复
    /// 带回**两个**套接字号 → 串行建两个 `make_sock_fd`（第一半带着"另一半的
    /// 设备号"、第二半带着"第一个 fd"）→ 最后回 `m_vfs_lc_fdpair { fd0, fd1 }`。
    #[test]
    fn test_socketpair_paired_orchestration() {
        use minix_types::Endpoint;

        let mut state = seeded(100);
        crate::main_loop::seed_ready_state(&mut state);
        let drv = Endpoint::from_generation_slot(0, 11);
        state.smap_table.entries[0].endpt = Some(drv);
        state.smap_table.pfmap[2] = Some(0);
        let row = state.smap_table.entries[0].num;
        // PFS 挂载行（两半都要 `REQ_NEWNODE`）。
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
        // 摆出"`SDEV_SOCKETPAIR` 的回复已到"的槽态（成对：两个 sock_id）。
        let mut reply = Message {
            m_type: minix_sockdriver::sdev::SdevReply::SocketReply as i32,
            ..Message::default()
        };
        reply.m_source = drv;
        // SAFETY(test): `mess_lsockdriver_vfs_socket_reply { req_id@0;
        // sock_id@4; sock_id2@8 }`。
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

        // 第一半：给 PFS 发 dev0 的 `REQ_NEWNODE`，且续接里带着"另一半"。
        let p = state.pending_fs.as_ref().expect("第一半的 REQ_NEWNODE");
        assert_eq!(p.req.m_type, minix_types::REQ_NEWNODE);
        let dev0 = crate::device_map::make_smap_dev(row, 0x10);
        // SAFETY(test): `mess_vfs_fs_newnode`：device@0。
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

        // 第一半回复到达 → 接着起第二半（dev1）。
        let pfs_worker = p.worker;
        state.pending_fs = None;
        let mut r1 = Message { m_type: 0, ..Message::default() };
        // SAFETY(test): 新节点 details。
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
        // SAFETY(test): `mess_vfs_fs_newnode`：device@0。
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

        // 第二半回复到达 → 回 `m_vfs_lc_fdpair { fd0, fd1 }`。
        state.pending_fs = None;
        let mut r2 = Message { m_type: 0, ..Message::default() };
        // SAFETY(test): 新节点 details。
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
        // SAFETY(test): `mess_vfs_lc_fdpair { int fd0; int fd1; }`。
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

    /// `Select` 的**本地半**（C `do_select` select.c:94-260）：`nfds > OPEN_MAX`
    /// → EINVAL；槽满 → ENOSPC；逐 fd 校验（位没设跳过、fd 无效 EBADF、类型
    /// 不认识 EBADF）；常规文件与"模式位不符"的 fd **立刻就绪**；没有要等的就
    /// 立刻返回（把结果位图拷回用户）。
    ///
    /// 走"要等驱动/超时"那条路的（字符/套接字/管道）诚实回 ENOSYS——那一半
    /// 要驱动对话与定时器，是下一步。
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

        // ① `nfds > OPEN_MAX` → EINVAL。
        let mut state = setup(crate::open::S_IFREG | 0o644, crate::open::R_BIT);
        let io = io_with(&[]);
        assert_eq!(
            do_select(
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

        // ② readfds 里指了一个**没开的 fd**（fd 5）→ EBADF。
        let mut state = setup(crate::open::S_IFREG | 0o644, crate::open::R_BIT);
        let io = io_with(&[1 << 5]);
        assert_eq!(
            do_select(
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

        // ③ 常规文件 + 读位 → **立刻就绪**：回 1，且结果位图拷回用户（fd 3 的
        // 读位被置上）。
        let mut state = setup(crate::open::S_IFREG | 0o644, crate::open::R_BIT);
        let io = io_with(&[1 << 3]);
        let r = do_select(
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

        // ④ 套接字 + 读位 → 要走驱动 `select_request`（未接）→ ENOSYS，且
        // **槽要放回**（下次还能用）。
        let mut state = setup(crate::open::S_IFSOCK | 0o777, crate::open::R_BIT);
        let io = io_with(&[1 << 3]);
        assert_eq!(
            do_select(
                &mut state,
                minix_types::UserSlot::new(0),
                8,
                1,
                0,
                0,
                0,
                &io
            ),
            Err(minix_types::ENOSYS),
            "套接字要问驱动，那一半未接"
        );
        assert!(
            state.select_table.slots.iter().all(|s| s.is_free()),
            "未接路径要把槽放回去（否则 25 次之后 ENOSPC）"
        );

        // ⑤ 槽满 → ENOSPC（把 25 个槽全占上）。
        let mut state = setup(crate::open::S_IFREG | 0o644, crate::open::R_BIT);
        for s in state.select_table.slots.iter_mut() {
            s.requestor = Some(minix_types::UserSlot::new(1));
        }
        let io = io_with(&[1 << 3]);
        assert_eq!(
            do_select(
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

    /// `Sendmsg`/`Recvmsg`：门同 socket 族；取用户 `struct msghdr`（生产件是
    /// 跨空间拷贝，宿主不可达 → EINVAL）。取到之后的门与请求形状用**脚本替身**
    /// 走 `do_sockmsg` 测：iov 多元素 → `EMSGSIZE`、`iov_len > SSIZE_MAX` →
    /// `EINVAL`、单元素 → 数据/控制/地址三张 grant。
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
            // SAFETY: mess_lc_vfs_sockmsg：fd@0、msgbuf@8、flags@16。
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[8..16].copy_from_slice(&0x6000u64.to_le_bytes());
            }
            m
        };

        // ① 门：负 fd → EBADF；宿主下取 msghdr 不可达 → EINVAL。
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

        // ② 脚本替身走 `do_sockmsg`：多元素向量 → EMSGSIZE。
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

        // ③ 单元素但 `iov_len > SSIZE_MAX` → EINVAL。
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

        // ④ `iovlen == 0`（空数据）：合法，照常给驱动发请求（宿主下发送失败
        // → EIO），且**不建数据 grant**（C 对零缓冲不建 grant）。
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

    /// `Accept`：门同 socket 族 + `check_sock_fds(1)`（新套接字要占一个 fd 槽）
    /// → 给驱动发 `SDEV_ACCEPT`（地址 grant 是 `CPF_WRITE`——驱动写对端地址
    /// 回来）→ 进程级挂起（现场带上监听 fd）。
    ///
    /// 收尾三态（C `resume_accept` socket.c:367-465）：① 失败且没建套接字 →
    /// 只回错误；② 失败但驱动已建套接字 → 关掉它再回错误；③ 成功 → 现场**再开
    /// 一个 worker** 做 `make_sock_fd`（PFS 建节点 + 装配 fd），回 fd + 对端
    /// 地址长度。
    #[test]
    fn test_dispatch_accept_and_three_state_resume() {
        use minix_types::Endpoint;

        let setup = || {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            let drv = Endpoint::from_generation_slot(0, 11);
            state.smap_table.entries[0].endpt = Some(drv);
            let dev = crate::device_map::make_smap_dev(state.smap_table.entries[0].num, 0x42);
            // PFS 挂载行（收尾要 `REQ_NEWNODE`）。
            let v = state.vmnt_table.get_mut(crate::vmnt::VmntId(1)).unwrap();
            v.fs = Endpoint::PFS;
            v.dev = 9;
            // 监听套接字：fd 3。
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
            // 监听套接字带 `O_NONBLOCK`（要被新套接字继承）。
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
            // SAFETY: mess_lc_vfs_sockaddr：fd@0、addr@8、addr_len@16。
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
            // sock_id@4; status@8; len@12 }`。
            unsafe {
                let raw = &mut r.m_u.raw;
                raw[4..8].copy_from_slice(&sock_id.to_le_bytes());
                raw[8..12].copy_from_slice(&status.to_le_bytes());
                raw[12..16].copy_from_slice(&addr_len.to_le_bytes());
            }
            r
        };

        // ① 门：负 fd → EBADF；非套接字 → ENOTSOCK。
        let (mut state, _idx, _drv, _dev) = setup();
        state.current_message = accept_msg(-1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Accept),
            SyscallResult::Error(minix_types::EBADF)
        );
        // 宿主下 trap 不可达 → 发送失败 → EIO，且不挂起（监听 fd 正常）。
        state.current_message = accept_msg(3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Accept),
            SyscallResult::Error(minix_types::EIO)
        );

        // ② 收尾 case ①：失败且**没建套接字**（sock_id < 0）→ 只回错误。
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

        // ③ 收尾 case ③：成功（sock_id ≥ 0）→ 现场开 worker 做 make_sock_fd：
        // 先给 PFS 发 `REQ_NEWNODE`（设备号 = make_smap_dev(行号, sock_id)）。
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
        // SAFETY(test): `mess_vfs_fs_newnode`：device@0、mode@8。
        unsafe {
            let raw = &p.req.m_u.raw;
            let new_dev = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let mode = u32::from_le_bytes(raw[8..12].try_into().unwrap());
            let row = state.smap_table.entries[0].num;
            assert_eq!(new_dev, crate::device_map::make_smap_dev(row, 0x77));
            assert_eq!(mode & crate::open::S_IFMT, crate::open::S_IFSOCK);
        }
        // 回复到达 → 回 fd + **对端地址长度**（`m_vfs_lc_socklen`）。
        let pfs_worker = p.worker;
        state.pending_fs = None;
        let mut reply = Message { m_type: 0, ..Message::default() };
        // SAFETY(test): 新节点 details（与 lookup_reply_off 前六域同序）。
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
        // SAFETY(test): `mess_vfs_lc_socklen { unsigned int len; }`。
        let len = unsafe { u32::from_le_bytes(reply.m_u.raw[0..4].try_into().unwrap()) };
        assert_eq!(len, 16, "对端地址长度进回复载荷");
        // 新 fd 上挂着套接字 vnode，且**继承了监听套接字的 O_NONBLOCK**。
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

    /// `Sendto`/`Recvfrom`（数据 + 地址两张 grant，进程级挂起）：门同 socket 族；
    /// 发送方向的数据 grant 是 `CPF_READ`（驱动读用户缓冲）、接收方向是
    /// `CPF_WRITE`；打开标志（非阻塞/写方向的 NOSIGPIPE）翻成消息标志与用户
    /// flags 按位或（C `sdev_readwrite:396-400`）。
    ///
    /// 收尾两态：发送方向用 `SDEV_REPLY`（状态即结果）、接收方向用
    /// `SDEV_RECV_REPLY`（还要把 **addr_len 放进回复载荷**）。
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
            // SAFETY: mess_lc_vfs_sendrecv：fd@0、buf@8、len@16、flags@24、
            // addr@32、addr_len@40。
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

        // ① 门：负 fd → EBADF。
        let (mut state, _idx, _drv, _dev) = setup();
        state.current_message = sendrecv_msg(VfsCallNum::Sendto, -1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Sendto),
            SyscallResult::Error(minix_types::EBADF)
        );

        // ② 宿主下 trap 不可达 → 发送失败 → EIO，不挂起、不留悬空 grant。
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

        // ③ 手工摆出"recvfrom 已挂起"的现场，喂 `SDEV_RECV_REPLY`：
        // 用户拿到字节数（status），**addr_len 进回复载荷**。
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
        // int flags@16 }`。
        unsafe {
            let raw = &mut reply.m_u.raw;
            raw[4..8].copy_from_slice(&12i32.to_le_bytes()); // 收到 12 字节
            raw[12..16].copy_from_slice(&16u32.to_le_bytes()); // addr_len
        }
        assert!(state.finish_sdev_blocked(&reply), "认领这条回复");
        let (target, reply) = state.take_reply().expect("回复");
        assert_eq!(target, Endpoint::from_generation_slot(1, 0));
        assert_eq!(reply.m_type, 12, "用户拿到字节数");
        // SAFETY(test): `mess_vfs_lc_socklen { unsigned int len; }`。
        let len = unsafe { u32::from_le_bytes(reply.m_u.raw[0..4].try_into().unwrap()) };
        assert_eq!(len, 16, "addr_len 进回复载荷");
    }

    /// `finish_sdev_blocked`：驱动回复唤醒被挂起的 bind/connect——撤掉留下的
    /// grant、清挂起态、把**载荷里的 status** 回给用户（C `sdev_reply` 第二条
    /// 分支 + `sdev_finish` 的 bind/connect 组）。
    #[test]
    fn test_finish_sdev_blocked_resumes_process() {
        use minix_types::Endpoint;

        let mut state = seeded(100);
        crate::main_loop::seed_ready_state(&mut state);
        let drv = Endpoint::from_generation_slot(0, 11);
        state.smap_table.entries[0].endpt = Some(drv);
        let dev = crate::device_map::make_smap_dev(state.smap_table.entries[0].num, 0x42);
        // 手工摆出"已挂起"的现场：grant + SdevBlock。
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

        // 驱动回复：`SDEV_REPLY` + 载荷 status = -EADDRINUSE。
        let mut reply = Message {
            m_type: minix_sockdriver::sdev::SdevReply::Reply as i32,
            ..Message::default()
        };
        reply.m_source = drv;
        // SAFETY(test): `mess_lsockdriver_vfs_reply { int32_t req_id; int
        // status; }`——状态在第二格。
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
        // 没人在等 + 没有挂起的进程 → 不认领（软失败）。
        let mut stray = Message {
            m_type: minix_sockdriver::sdev::SdevReply::Reply as i32,
            ..Message::default()
        };
        stray.m_source = drv;
        assert!(!state.finish_sdev_blocked(&stray), "没有挂起者就不认领");
    }

    /// getset 族（`Setsockopt`/`Getsockopt`/`Getsockname`/`Getpeername`）：
    /// 前两道门同 socket 族（EBADF/ENOTSOCK）；过了就把用户缓冲做成 magic
    /// grant 交给驱动，回复号 `SDEV_REPLY`、**状态在载荷里**；`get` 方向的
    /// 状态就是**新长度**，进回复载荷（`m_vfs_lc_socklen { len }`）。
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
            // SAFETY: mess_lc_vfs_sockopt：fd@0、level@4、name@8、buf@16、len@24。
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
            // SAFETY: mess_lc_vfs_sockaddr：fd@0、addr@8、addr_len@16。
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[8..16].copy_from_slice(&0x6000u64.to_le_bytes());
                raw[16..24].copy_from_slice(&32u64.to_le_bytes());
            }
            m
        };

        // ① 负 fd → EBADF；非套接字 → ENOTSOCK（getset 族共用 get_sock 的门）。
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

        // ② 是套接字 → 真发 `SDEV_SETSOCKOPT`（宿主下 EIO），grant 已建。
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

        // ③ `getsockname`：回复带**新长度**（载荷 status = 16）→ 用户拿到 0，
        // 长度进回复载荷。
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
        // status; }`——这里是"写进去多少字节"，即新长度（第二格）。
        unsafe {
            reply.m_u.raw[4..8].copy_from_slice(&16i32.to_le_bytes());
        }
        state.handle_drv_reply(&reply).expect("有槽在等这个驱动");
        state.run_worker_continuations();
        let (target, reply) = state.take_reply().expect("回复");
        assert_eq!(target, Endpoint::from_generation_slot(1, 0));
        assert_eq!(reply.m_type, 0, "成功回 0（长度在载荷里）");
        // SAFETY(test): `mess_vfs_lc_socklen { unsigned int len; }`。
        let len = unsafe { u32::from_le_bytes(reply.m_u.raw[0..4].try_into().unwrap()) };
        assert_eq!(len, 16, "新长度进回复载荷");
        let _ = idx;

        // ④ `getpeername` 走同一条路（`level`/`name` 传 0）。
        let (mut state, _idx, _drv) = setup(crate::open::S_IFSOCK | 0o777);
        state.current_message = sockaddr_msg(VfsCallNum::Getpeername, 3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getpeername),
            SyscallResult::Error(minix_types::EIO)
        );
    }

    /// `sdev_simple` 的两个消费者（`Listen`/`Shutdown`）走的是同一条驱动
    /// 请求形状：回复号必须是 `SDEV_REPLY`，**状态在回复载荷里**
    /// （C `sdev_simple` sdev.c:245-276）。这里喂一条回复把这条链走完。
    #[test]
    fn test_sdev_simple_reply_status_from_payload() {
        use minix_types::Endpoint;

        let mut state = seeded(100);
        crate::main_loop::seed_ready_state(&mut state);
        // 一个套接字 fd：vnode 带 `v_sdev`（smap 行 0 + sock_id 0x42）。
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

        // `listen(fd, 5)`：宿主下发送失败 → EIO，但续接标识已挂。
        let mut m = Message {
            m_source: Endpoint::from_generation_slot(1, 0),
            m_type: VfsCallNum::Listen as i32,
            ..Message::default()
        };
        // SAFETY: mess_lc_vfs_listen：fd@0、backlog@4。
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

        // 驱动回复：`SDEV_REPLY` + 载荷里的 status（这里故意带 -EACCES）。
        let mut reply = Message {
            m_type: minix_sockdriver::sdev::SdevReply::Reply as i32,
            ..Message::default()
        };
        reply.m_source = drv;
        // SAFETY(test): `mess_lsockdriver_vfs_reply { int32_t req_id; int
        // status; }`——状态在**第二格**（req_id 在前）。
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

        // 回复号不是 `SDEV_REPLY` → EIO（C 的协议错误面）。
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
        // SAFETY: fd@0、backlog@4（同上面那条）。
        unsafe {
            let raw = &mut state.current_message.m_u.raw;
            raw[0..4].copy_from_slice(&3i32.to_le_bytes());
            raw[4..8].copy_from_slice(&5i32.to_le_bytes());
        }
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Listen),
            SyscallResult::Error(minix_types::EIO)
        );
        let mut bad = Message {
            m_type: minix_sockdriver::sdev::SdevReply::SelectReply1 as i32,
            ..Message::default()
        };
        bad.m_source = drv;
        state.handle_drv_reply(&bad).expect("有槽在等这个驱动");
        state.run_worker_continuations();
        assert_eq!(
            state.take_reply().map(|(t, m)| (t, m.m_type)),
            Some((Endpoint::from_generation_slot(1, 0), minix_types::EIO)),
            "回复号不对 → 协议错误"
        );
    }

    /// 驱动对话管线的**回复落槽**（C `sdev_reply` 的公共前半）：驱动回复到达
    /// 时找到等它的槽、落槽、唤醒，续接体随即跑（这里跑到 PFS 的
    /// `REQ_NEWNODE`——`make_sock_fd` 的第一步）。
    #[test]
    fn test_driver_reply_lands_and_chains_to_pfs() {
        use minix_types::Endpoint;

        let mut state = seeded(100);
        crate::main_loop::seed_ready_state(&mut state);
        // 域 2 → smap 行 0（端点 = 驱动）。
        state.smap_table.pfmap[2] = Some(0);
        let drv = Endpoint::from_generation_slot(0, 11);
        state.smap_table.entries[0].endpt = Some(drv);
        // PFS 挂载行（`make_sock_fd` 要它）。
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

        // 臂：过门 → 记下续接（`SdevSocket`）→ 发送失败（宿主）→ EIO。
        let mut m = Message {
            m_source: Endpoint::from_generation_slot(1, 0),
            m_type: VfsCallNum::Socket as i32,
            ..Message::default()
        };
        // SAFETY: mess_lc_vfs_socket：domain@0、type@4、protocol@8。
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

        // 驱动回复到达：`handle_drv_reply` 找等它的槽并落槽。
        let mut reply = Message {
            m_type: minix_sockdriver::sdev::SdevReply::SocketReply as i32,
            ..Message::default()
        };
        reply.m_source = drv;
        // SAFETY(test): `mess_lsockdriver_vfs_socket_reply`：req_id@0、
        // sock_id@4、sock_id2@8。
        unsafe {
            let raw = &mut reply.m_u.raw;
            raw[0..4].copy_from_slice(&0i32.to_le_bytes());
            raw[4..8].copy_from_slice(&0x42i32.to_le_bytes());
            raw[8..12].copy_from_slice(&(-1i32).to_le_bytes());
        }
        let slot = state.handle_drv_reply(&reply).expect("有槽在等这个驱动");
        assert_eq!(slot, idx);
        state.run_worker_continuations();

        // 续接体跑到了 `make_sock_fd` 的第一步：给 PFS 发 `REQ_NEWNODE`，
        // 且设备号是 `make_smap_dev(行号, sock_id)`（高 32 位是行号）。
        let p = state.pending_fs.as_ref().expect("已登记 REQ_NEWNODE");
        assert_eq!(p.req.m_type, minix_types::REQ_NEWNODE);
        assert_eq!(p.fs_e, Endpoint::PFS);
        // SAFETY(test): `mess_vfs_fs_newnode`：device@0、mode@8。
        unsafe {
            let raw = &p.req.m_u.raw;
            let dev = u64::from_le_bytes(raw[0..8].try_into().unwrap());
            let mode = u32::from_le_bytes(raw[8..12].try_into().unwrap());
            let row = state.smap_table.entries[0].num;
            assert_eq!(dev, crate::device_map::make_smap_dev(row, 0x42));
            assert_eq!(mode & crate::open::S_IFMT, crate::open::S_IFSOCK, "套接字节点");
        }
    }

    /// `Pipe2` 臂（C `do_pipe2` pipe.c:39-55 + `create_pipe` pipe.c:58-135）：
    /// 没有 PFS 挂载行 → fail-closed EIO（C 会 panic "PFS gone"）；有挂载行时
    /// 预留 vnode + 认领两个 fd/filp，向 PFS 发 `REQ_NEWNODE`；回复到达后填
    /// vnode 与两个 filp，并把 `m_vfs_lc_fdpair { fd0, fd1 }` 作为**回复载荷**
    /// 发回（用户拿到的就是这两个 fd）。
    #[test]
    fn test_dispatch_pipe2_flow_and_payload() {
        use minix_types::Endpoint;

        let setup = |with_pfs: bool| {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            if with_pfs {
                // PFS 挂载行（`find_vmnt(PFS_PROC_NR)` 那一跳）。
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
            // SAFETY: mess_lc_vfs_pipe2：flags@0、_unused@4、oflags@8。
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&flags.to_le_bytes());
                raw[8..12].copy_from_slice(&oflags.to_le_bytes());
            }
            m
        };

        // ① 没有 PFS 挂载行 → EIO（C 在这里 panic；Rust 侧 fail-closed）。
        let (mut state, _idx) = setup(false);
        state.current_message = pipe2_msg(0, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Pipe2),
            SyscallResult::Error(minix_types::EIO),
            "PFS 未挂载：不假装建成了管道"
        );

        // ② 有 PFS：认领两个 fd/filp 并向 PFS 发 REQ_NEWNODE。
        let (mut state, idx) = setup(true);
        state.current_message = pipe2_msg(0, crate::open::OpenFlags::CLOEXEC.bits() as i32);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Pipe2),
            SyscallResult::Suspend
        );
        let p = state.pending_fs.as_ref().expect("已登记 REQ_NEWNODE");
        assert_eq!(p.req.m_type, minix_types::REQ_NEWNODE);
        assert_eq!(p.fs_e, Endpoint::PFS);
        // SAFETY(test): 按 C 的 mess_vfs_fs_newnode 域序读回（device@0、mode@8）。
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
            // CLOEXEC 位不在这一步置位——C 是**建成功之后**才 `FD_SET`
            // （pipe.c:126-129），所以这里只断言 fd 认领。
            assert!(!fp.cloexec_set.get(0), "CLOEXEC 还没置（建成功才置）");
        }

        // ③ 回复到达（带新节点 details）→ 填 vnode/两个 filp + 回复载荷带 fd 对。
        let mut reply = Message { m_type: 0, ..Message::default() };
        // SAFETY(test): 按 lookup_reply_off（与 mess_fs_vfs_newnode 前六域同序）
        // 填 ino/mode/device。
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
        // SAFETY(test): `mess_vfs_lc_fdpair { fd0, fd1 }`。
        let (fd0, fd1) = unsafe {
            (
                i32::from_le_bytes(reply.m_u.raw[0..4].try_into().unwrap()),
                i32::from_le_bytes(reply.m_u.raw[4..8].try_into().unwrap()),
            )
        };
        assert_eq!((fd0, fd1), (0, 1), "回复载荷带的是两个 fd");
        // vnode 与两个 filp 都落位了。
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
        // 建成功之后 CLOEXEC 才置位（C pipe.c:126-129），两个 fd 都要。
        assert!(fp.cloexec_set.get(fd0 as usize), "读端 CLOEXEC");
        assert!(fp.cloexec_set.get(fd1 as usize), "写端 CLOEXEC");
    }

    /// `Copyfd` 臂（C `do_copyfd` filedes.c:524-650，驱动回调用）：只有超级
    /// 用户能调（EPERM）；远端端点必须已知（EINVAL）；三种操作都是**本地表
    /// 操作**——`TO` 把调用方的 fd 装进远端、`FROM` 反过来（且不带 CLOEXEC）、
    /// `CLOSE` 回滚一次 `TO`（计数 > 1 才允许）。
    #[test]
    fn test_dispatch_copyfd_three_ops() {
        use minix_types::Endpoint;

        let caller_ep = Endpoint::from_generation_slot(1, 0);
        let remote_ep = Endpoint::from_generation_slot(1, 1);
        let setup = |is_super: bool| {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            // 注意默认 `eff_uid` 就是 0（= SU_UID），所以"非超级用户"那条要
            // **显式**改成非 0，否则门恒开。
            state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap()
                .eff_uid = if is_super { crate::link::SU_UID } else { 1000 };
            // 远端进程：槽 1。
            {
                let fp = state
                    .fproc_table
                    .get_mut(minix_types::UserSlot::new(1))
                    .unwrap();
                fp.endpoint = remote_ep;
                fp.pid = 101;
            }
            // 调用方的 fd 3 → filp（计数 1）。
            let fid = state.filp_table.alloc_filp(crate::open::R_BIT).unwrap();
            state.filp_table.inc_count(fid);
            state
                .fproc_table
                .get_mut(minix_types::UserSlot::new(0))
                .unwrap()
                .filps[3] = Some(fid.get());
            // 远端也有一个自己的 fd 5 → 同一个 filp（给 FROM/CLOSE 用）。
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
            // SAFETY: mess_lsys_vfs_copyfd：endpt@0、fd@4、what@8。
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&endpt.0.to_le_bytes());
                raw[4..8].copy_from_slice(&fd.to_le_bytes());
                raw[8..12].copy_from_slice(&what.to_le_bytes());
            }
            m
        };

        // ① 非超级用户 → EPERM（驱动回调用的特权）。
        let (mut state, _fid) = setup(false);
        state.current_message = copyfd_msg(remote_ep, 3, 1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Copyfd),
            SyscallResult::Error(minix_types::EPERM)
        );

        // ② 远端端点未知 → EINVAL。
        let (mut state, _fid) = setup(true);
        state.current_message = copyfd_msg(Endpoint::from_generation_slot(3, 9), 3, 1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Copyfd),
            SyscallResult::Error(minix_types::EINVAL)
        );

        // ③ `COPYFD_TO`：调用方的 fd 3 装进远端的第一个空闲槽（0），
        // CLOEXEC 位跟着 `what` 的 0x8000 走。
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

        // ④ `COPYFD_FROM`：远端的 fd 5 装进**调用方**的表，且**不带** CLOEXEC
        // （C 明确 `flags &= ~COPYFD_CLOEXEC`）。
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

        // ⑤ `COPYFD_CLOSE`：回滚一次 `TO`——计数 > 1 才允许，成功清掉远端槽。
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

        // ⑥ 计数 == 1 时 CLOSE → EBADF（C 的 `filp_count > 1` 门）。
        let (mut state, fid) = setup(true);
        // 把计数压到 1（只留调用方那一个引用）。
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

    /// `Ioctl` 臂的类型分派（C `do_ioctl` device.c:18-58）：fd/vnode 门 →
    /// 按**文件类型**分派——非设备文件一律 `ENOTTY`（C 的 `default`，这条是
    /// 本地判定，已经真装）；三种设备各要一次驱动对话，管线未接线 → 诚实回
    /// ENOSYS 并登记缺口。
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
            // SAFETY: mess_lc_vfs_ioctl：fd@0、req@8、arg@16。
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[8..16].copy_from_slice(&req.to_le_bytes());
                raw[16..24].copy_from_slice(&0x6000u64.to_le_bytes());
            }
            m
        };

        // ① 负 fd / 空槽 → EBADF。
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

        // ② 常规文件与目录 → ENOTTY（C 的 `default`，本地判定）。
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

        // ③ 字符设备：dmap 里没映射驱动 → **ENXIO**（C `cdev_get` 的
        // "major 无驱动"面）。
        let (mut state, _fid) = setup(crate::open::S_IFCHR | 0o644);
        state.current_message = ioctl_msg(3, 0x5401);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Ioctl),
            SyscallResult::Error(minix_types::ENXIO),
            "字符设备要有 dmap 驱动"
        );

        // ④ 字符设备 + 有驱动 → 真发 `CDEV_IOCTL`（宿主下 trap 不可达 →
        // EIO），且续接标识挂上（等驱动的回复）。
        let (mut state, _fid) = setup(crate::open::S_IFCHR | 0o644);
        {
            // vnode 的 `sdev` 定 major/minor（major 3 → dmap 行 3）。
            let vid = state
                .vnode_table
                .find_by_ino(Endpoint::MFS, 0x42)
                .expect("刚建的 vnode");
            state.vnode_table.get_mut(vid).unwrap().sdev = (3 << 8) | 7;
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
            row.driver = Some(Endpoint::from_generation_slot(0, 12));
            state.dmap_table.set(3, row);
        }
        state.current_message = ioctl_msg(3, 0x5401);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Ioctl),
            SyscallResult::Error(minix_types::EIO),
            "有驱动就真发（宿主下发送失败）"
        );

        // ⑤ 套接字：smap 里没这个设备号的驱动 → EIO（C `sdev_ioctl` 的
        // `get_smap_by_dev` 失败面）；有驱动则真发 `SDEV_IOCTL`（宿主下发送
        // 失败 → EIO）。
        let (mut state, _fid) = setup(crate::open::S_IFSOCK | 0o644);
        state.current_message = ioctl_msg(3, 0x5401);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Ioctl),
            SyscallResult::Error(minix_types::EIO),
            "套接字要有 smap 行"
        );

        // ⑥ 块设备：dmap 里没驱动 → ENXIO（C `bdev_ioctl` 的 "no driver for
        // major" 面）；有驱动则真发 `BDEV_IOCTL`（宿主下发送失败 → EIO），
        // 且 `filp_ioctl_fp` 守卫在失败路径上要清掉。
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

    /// `Fcntl` 臂的本地子集（C `do_fcntl` misc.c:127-300）：`F_DUPFD` 家族
    /// 共用同一个 filp（`filp_count++`，**不**新分配 filp）、`F_GETFD`/`F_SETFD`
    /// 读写 close-on-exec 位、`F_GETFL`/`F_SETFL` 读写状态字（窄门）、
    /// `F_GETNOSIGPIPE`/`F_SETNOSIGPIPE` 读写哨兵位；未知 cmd → EINVAL。
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
            // 生产里 fd 表上的 filp 计数至少是 1（open 时认领的）——测试补上，
            // 否则"dup 之后计数 +1"这条断言看不出差别。
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
            // SAFETY: mess_lc_vfs_fcntl：fd@0、cmd@4、arg_int@8、arg_ptr@16。
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[4..8].copy_from_slice(&cmd.to_le_bytes());
                raw[8..12].copy_from_slice(&arg.to_le_bytes());
            }
            m
        };

        // ① 未知 cmd → EINVAL（C 的 `default`）。
        let (mut state, _fid) = setup();
        state.current_message = fcntl_msg(3, 0x7fff, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fcntl),
            SyscallResult::Error(minix_types::EINVAL)
        );

        // ② 负 fd / 空槽 → EBADF。
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

        // ③ `F_DUPFD`：floor 门（负数/越界 → EINVAL）；合法时拿到最低空闲 fd
        // 且**共用同一个 filp**（计数 +1、不新增 filp）。
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
        // "没有新分配 filp"：fd 表里两个槽指向**同一个** filp id（上面已断言
        // fd 0 的指向），再加上计数 +1——`FilpTable::len` 是表容量（NR_FILPS），
        // 不是已用数，别拿它当判据。
        assert_eq!(
            state.filp_table.get(fid).unwrap().count,
            2,
            "仍然只有这一个 filp 在服务两个 fd"
        );

        // ④ `F_DUPFD_CLOEXEC`：顺带置 cloexec 位。
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

        // ⑤ `F_GETFD`/`F_SETFD`：读写同一位。
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

        // ⑥ `F_GETFL`/`F_SETFL`：状态字读回与窄门写入（只放 O_NONBLOCK|O_APPEND）。
        let (mut state, fid) = setup();
        state.current_message = fcntl_msg(3, crate::fcntl::F_GETFL, 0);
        let r = dispatch_syscall(&mut state, VfsCallNum::Fcntl);
        assert_eq!(
            r,
            SyscallResult::Ok((crate::open::O_RDONLY | crate::fcntl::O_APPEND) as i32)
        );
        // 试着塞一个不在窄门里的位（O_TRUNC）：不该进去。
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

    /// `Mapdriver` 臂（C `do_mapdriver` dmap.c:106-177）：三道门在任何查表
    /// 之前（只有 RS 能调 EPERM / 标签放得下 EINVAL / 标签以 NUL 结尾
    /// EINVAL）；标签 → 端点走本地目录（生产缺口见 `LabelDir`），命中后标成
    /// 服务进程并写 dmap/smap。
    ///
    /// 标签在**调用方内存**里（C 的 `sys_vircopy`），宿主下取不到 → EINVAL
    /// ——所以"取标签之后"的半在 `test_finish_mapdriver_*` 那两条里直接调
    /// 主体覆盖。
    #[test]
    fn test_dispatch_mapdriver_gates() {
        use minix_types::Endpoint;

        let mapdriver_msg = |major: u32, label_len: u64| {
            let mut m = Message {
                m_source: Endpoint::RS, // 只有 RS 能调
                m_type: VfsCallNum::Mapdriver as i32,
                ..Message::default()
            };
            // SAFETY: mess_lsys_vfs_mapdriver：major@0、labellen@8、label@16、
            // ndomains@24、domains@28..。
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&major.to_le_bytes());
                raw[8..16].copy_from_slice(&label_len.to_le_bytes());
                raw[16..24].copy_from_slice(&0x6000u64.to_le_bytes());
                raw[24..28].copy_from_slice(&0i32.to_le_bytes());
            }
            m
        };

        // ① 非 RS 调用 → EPERM。
        let mut state = seeded(100);
        state.current_message = mapdriver_msg(3, 4);
        state.current_message.m_source = Endpoint::PM;
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Mapdriver),
            SyscallResult::Error(minix_types::EPERM)
        );

        // ② 标签超过 LABEL_MAX → EINVAL。
        state.current_message = mapdriver_msg(3, crate::device_map::LABEL_MAX as u64 + 1);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Mapdriver),
            SyscallResult::Error(minix_types::EINVAL)
        );

        // ③ 标签长度合法但跨空间取不到（宿主）→ EINVAL（与 C 的
        // `sys_vircopy` 失败同值）。
        state.current_message = mapdriver_msg(3, 4);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Mapdriver),
            SyscallResult::Error(minix_types::EINVAL)
        );
    }

    /// `finish_mapdriver` 的主体（取标签之后那半）：标签未登记 → EINVAL；
    /// 登记了 → 标成服务进程 + 写 dmap 行（major 有效时）+ 写 smap 行与
    /// `pfmap`（有域时）；smap 失败要**撤销** dmap。
    #[test]
    fn test_finish_mapdriver_registers_and_undoes() {
        use minix_types::Endpoint;

        let mut state = seeded(100);
        let drv = Endpoint::from_generation_slot(0, 9);
        // 驱动进程占一个 fproc 槽（`isokendpt` 那一跳）。
        let drv_slot = drv.to_user_slot().expect("用户槽");
        state.driver_labels.push(("mydrv".to_string(), drv));

        // ① 标签没登记 → EINVAL（fail-closed，不假装解析成功）。
        assert_eq!(
            state.finish_mapdriver(Endpoint::RS, "unknown", 3, &[]),
            minix_types::EINVAL
        );

        // ② 非 RS 调用 → EPERM。
        assert_eq!(
            state.finish_mapdriver(Endpoint::PM, "mydrv", 3, &[]),
            minix_types::EPERM
        );

        // ③ 只给 major：写 dmap 行 + 标服务进程。
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

        // ④ 给域：写 smap 行 + `pfmap[domain]`。
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

        // ⑤ 域被**别的**驱动占了 → EBUSY，且 dmap 的登记要撤销。
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

    /// `Getvfsstat` 臂（C `do_getvfsstat` stadir.c:330-403）：`buf == 0` 只数
    /// 个数（**返回个数**，不是 0）；给了缓冲就按 `bufsize / sizeof` 截断挂载
    /// 列表，逐个填到 `buf + i*sizeof`（多挂载序列），全部成功也返回**个数**。
    #[test]
    fn test_dispatch_getvfsstat_count_and_sequence() {
        use minix_types::Endpoint;

        let setup = || {
            let mut state = seeded(100);
            crate::main_loop::seed_ready_state(&mut state);
            // 两行可报（dev 非 NO_DEV + CANSTAT），一行缺 CANSTAT 要被过滤。
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
                v.dev = 3; // 没有 CANSTAT → 不计
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
            // SAFETY: mess_lc_vfs_getvfsstat：buf@0、len@8、flags@16。
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..8].copy_from_slice(&buf.to_le_bytes());
                raw[8..16].copy_from_slice(&len.to_le_bytes());
                raw[16..20].copy_from_slice(&flags.to_le_bytes());
            }
            m
        };

        // ① `buf == 0`：只数个数 → 回 2（可报的两行），不发任何请求。
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

        // ② 给了缓冲（够两格）：起序列——第一格的 REQ_STATVFS 已登记。
        let (mut state, _idx) = setup();
        state.current_message = msg(0x5000, 2 * minix_types::STATVFS_SIZE as u64, 0);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Getvfsstat),
            SyscallResult::Suspend
        );
        let p = state.pending_fs.as_ref().expect("第一格 REQ_STATVFS");
        assert_eq!(p.req.m_type, minix_types::REQ_STATVFS);
        assert_eq!(p.fs_e, Endpoint::MFS, "先填列表里的第一行");

        // ③ 缓冲只够一格：序列截断到 1（C 的 `if (bufsize < sizeof) break`）。
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

        // ④ 序列推进：第一格回复到达 → 第二格请求发出（缓冲按 sizeof 推进）；
        // 第二格回复到达 → 收尾回**个数** 2。
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
            // 第一格回复（FS 已把统计量写进缓冲）。
            state.statvfs_buf.set_u64(off::BSIZE, 1024);
            state.pending_fs = None;
            {
                let wp = state.worker_pool.get_mut(idx).unwrap();
                wp.sendrec = Some(Message { m_type: 0, ..Message::default() });
                wp.task = None;
                wp.state = crate::worker::WorkerState::Busy;
            }
            state.run_worker_continuations();
            // 拷贝在宿主下不可达 → 序列就地中止（回 EIO），不会推进到第二格。
            assert_eq!(
                state.take_reply().map(|(t, m)| (t, m.m_type)),
                Some((Endpoint::from_generation_slot(1, 0), minix_types::EIO)),
                "拷贝失败即中止序列（C 的 `return r`）"
            );
            assert!(state.pending_fs.is_none(), "中止后不该再发请求");
        }

        // ⑤ 缓冲一格都不够 → 就地收尾回 0（C 的循环第一轮就 break）。
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

    /// Statvfs 族（C `do_statvfs`/`do_fstatvfs` stadir.c:294-442 +
    /// `fill_statvfs`/`update_statvfs`）：新鲜路径给 VFS 侧缓冲做 direct grant
    /// 发 `REQ_STATVFS`（统计量由 FS 整块回填）；`ST_NOWAIT` **不发请求**，直接
    /// 用挂载行缓存。两条路最后都补本地字段（只读位、`f_fsid`、三个名字）再
    /// 整块拷给用户——宿主下拷贝不可达，如实回 EIO。
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
            // SAFETY: mess_lc_vfs_statvfs1：fd@0、flags@4、len@8、name@16、buf@24。
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
                raw[4..8].copy_from_slice(&flags.to_le_bytes());
                raw[24..32].copy_from_slice(&buf.to_le_bytes());
            }
            m
        };

        // ① 负 fd → EBADF。
        let (mut state, _idx) = setup(false);
        state.current_message = fstatvfs_msg(-1, 0, 0x5000);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fstatvfs1),
            SyscallResult::Error(minix_types::EBADF)
        );

        // ② 新鲜路径：发 REQ_STATVFS，grant 指向 VFS 侧缓冲。
        let (mut state, idx) = setup(false);
        state.current_message = fstatvfs_msg(3, 0, 0x5000);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fstatvfs1),
            SyscallResult::Suspend
        );
        let p = state.pending_fs.as_ref().expect("已登记 REQ_STATVFS");
        assert_eq!(p.req.m_type, minix_types::REQ_STATVFS);
        // SAFETY(test): grant 在负载区首字。
        let grant = unsafe { i32::from_le_bytes(p.req.m_u.raw[0..4].try_into().unwrap()) };
        assert_eq!(grant, p.grant);
        assert!(matches!(
            state.worker_pool.get_mut(idx).unwrap().cont,
            Some(crate::worker::WorkerCont::Statvfs { user_buf: 0x5000, .. })
        ));

        // ③ 回复到达：FS 已经把统计量写进缓冲（测试直接写缓冲模拟）→ 缓存
        // 写回挂载行 + 本地字段补齐；拷给用户宿主不可达 → EIO。
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
            Some((Endpoint::from_generation_slot(1, 0), minix_types::EIO)),
            "宿主下 sys_datacopy 不可达 → EIO（如实，不假装拷成功）"
        );

        // ④ ST_NOWAIT：不发请求，直接用缓存（并把只读位补进 f_flag）。
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
            Some((Endpoint::from_generation_slot(1, 0), minix_types::EIO)),
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

    /// `Chdir`/`Chroot`/`Fchdir` 臂（C stadir.c:32-107，都归结到
    /// `change_into`）：chroot 只有超级用户；fchdir 走 fd 取 vnode；
    /// 换目录的三道（同一个 vnode 直接成功 / 非目录 ENOTDIR / 不可搜索
    /// EACCES）在本地判定，**没有 FS 往返**。
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
            // fd 3 → 目标目录 vnode。
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
            // SAFETY: `mess_lc_vfs_fchdir { int fd; }`（ipc.h:640-644）。
            unsafe {
                let raw = &mut m.m_u.raw;
                raw[0..4].copy_from_slice(&fd.to_le_bytes());
            }
            m
        };

        // ① chroot 非超级用户 → EPERM（在任何取路径之前）。
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

        // ② fchdir 负 fd / 空槽 → EBADF。
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

        // ③ fchdir 指向常规文件 → ENOTDIR（`change_into` 的类型门）。
        let (mut state, _vid) = setup(1000, crate::open::S_IFREG | 0o644, 1000);
        state.current_message = fchdir_msg(3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchdir),
            SyscallResult::Ok(minix_types::ENOTDIR)
        );

        // ④ fchdir 指向 0700 的目录、调用方是属主 → 换成功，wd 指向新 vnode。
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

        // ⑤ 同一个目录再来一次 → 直接成功（C 的 `if (*result == vp)`）。
        state.current_message = fchdir_msg(3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchdir),
            SyscallResult::Ok(0)
        );

        // ⑥ fchdir 指向**别人的** 0700 目录 → EACCES（搜索位门）。
        let (mut state, _vid) = setup(2000, crate::open::S_IFDIR | 0o700, 1000);
        state.current_message = fchdir_msg(3);
        assert_eq!(
            dispatch_syscall(&mut state, VfsCallNum::Fchdir),
            SyscallResult::Ok(minix_types::EACCES)
        );
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
        // 已按模板接线的：`Read`/`Write`/`Fstat`/`Stat`/`Lstat`/`Ftruncate`/
        // `Lseek`/`Open`（含 `O_TRUNC`）/`Mkdir`/`Creat`/`Getdents`/`Access`/
        // `Readlink`/`Chmod`/`Fchmod`/`Chown`/`Fchown`/`Unlink`/`Rmdir`/`Mknod`/
        // `Symlink`/`Link`/`Rename`/`Utimens`/`Sync`/`Fsync`/`Chdir`/`Fchdir`/
        // `Chroot`。这里取还没接的 `Mount` 作代表——它同属"FS 对话族"，而且是
        // 同族里唯一要**新建挂载行**的臂。
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
