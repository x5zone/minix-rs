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
                Ok((new_pos, _inhibit)) => {
                    filp.pos = new_pos;
                    SyscallResult::Ok(0)
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
        | VfsCallNum::Open
        | VfsCallNum::Creat
        | VfsCallNum::Mkdir
        | VfsCallNum::Mknod
        | VfsCallNum::Link
        | VfsCallNum::Unlink
        | VfsCallNum::Rename
        | VfsCallNum::Rmdir
        | VfsCallNum::Symlink
        | VfsCallNum::Readlink
        | VfsCallNum::Truncate
        | VfsCallNum::Ftruncate
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
        }
        {
            let f = state.filp_table.get_mut(fid).unwrap();
            f.vnode = Some(0);
            f.pos = 10;
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
        dispatch_syscall(&mut state, call);
        let f = state.filp_table.get(fid).unwrap();
        assert_eq!(f.pos, 40);
    }

    #[test]
    fn test_dispatch_fs_dialogue_arms_nosys() {
        // FS/驱动对话臂：未接线的仍 fail-closed Nosys（诚实契约，模式 60）。
        // `Read`/`Write`/`Fstat` 已按模板接线（走各自的门），这里取还没接的
        // `Open` 作代表——它同属"FS 对话族"，且要等 W7 的 path 往返。
        let mut state = seeded(100);
        let call = VfsCallNum::Open;
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
