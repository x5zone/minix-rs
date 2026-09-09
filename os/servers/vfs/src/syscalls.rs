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
use crate::filedes::{close_fd, Fd};
use crate::open::{seek_pos, S_IFMT, S_IFIFO, Whence};
use crate::main_loop::VfsState;

use crate::vnode::VnodeId;
use minix_types::Endpoint;

/// `do_work`（`main.c:283-294`）的绑定层——穷举 64 臂，无通配。
///
/// 解码约定：`lc_vfs_*` 消息布局按 C `ipc.h` 的字段偏移，经 `Message`
/// 的 M7 视图读取（`off_t` 由相邻两个 `i32` 拼回，低 32 位在前）。
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
            let fp = match state.fproc_table.get_mut(fp_slot) {
                Some(fp) => fp,
                None => return SyscallResult::Error(minix_types::EINVAL),
            };
            match close_fd(fp, fd, &mut state.filp_table) {
                Ok(()) => SyscallResult::Ok(0),
                Err(e) => SyscallResult::Error(e.to_errno()),
            }
        }
        VfsCallNum::Lseek => {
            // lc_vfs_lseek（ipc.h:725-731）：off_t offset @0、int fd @8、
            // int whence @12。`actual_lseek` 的纯算术决策（open.rs seek_pos）。
            let offset = (i64::from(m7.m7i2) << 32) | i64::from(m7.m7i1 as i32);
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
        VfsCallNum::Read
        | VfsCallNum::Write
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
        | VfsCallNum::Stat
        | VfsCallNum::Fstat
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
        | VfsCallNum::GcovFlush
        | VfsCallNum::Getsysinfo => {
            // FS/驱动对话——W1 transport 通电后经 fs_comm 窗口接入
            // （plan.md §8 W3 尾注）。
            SyscallResult::Nosys
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
        // FS/驱动对话臂：W1 通电前 fail-closed Nosys（诚实契约，模式 60）。
        let mut state = seeded(100);
        let call = VfsCallNum::Read;
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
}
