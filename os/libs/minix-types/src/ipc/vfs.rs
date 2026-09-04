//! VFS service IPC message types.
//!
//! Defines the messages exchanged between VFS and other services (PM, Kernel).

use crate::{
    EAGAIN, EINVAL, EIO, EMFILE, ENOSYS, ESRCH, Endpoint, Message, MessageM7, MessageUnion,
    NR_PROCS, Pid,
};

/// VFS request message types.
///
/// These are the requests that VFS receives from other services.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsRequest {
    /// Fork request from PM.
    ///
    /// PM sends this to duplicate the parent's file descriptor table.
    Fork {
        /// Parent process endpoint.
        parent_endpoint: Endpoint,
        /// Child process endpoint.
        child_endpoint: Endpoint,
    },
}

/// VFS response message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsResponse {
    /// Fork succeeded.
    ForkOk,
    /// Operation failed.
    Error(VfsError),
}

/// VFS error types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsError {
    /// Process table is full.
    ProcTableFull,
    /// Invalid endpoint.
    InvalidEndpoint,
    /// Slot is already in use.
    SlotInUse,
    /// Too many open files.
    TooManyOpenFiles,
    /// Internal error.
    InternalError,
    /// Operation not implemented.
    NotImplemented,
}

impl VfsError {
    /// Converts error to errno value.
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::ProcTableFull => EAGAIN,
            Self::InvalidEndpoint => ESRCH,
            Self::SlotInUse => EINVAL,
            Self::TooManyOpenFiles => EMFILE,
            Self::InternalError => EIO,
            Self::NotImplemented => ENOSYS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vfs_fork_request() {
        let req = VfsRequest::Fork {
            parent_endpoint: Endpoint::PM,
            child_endpoint: Endpoint::from_generation_slot(1, 1),
        };

        match req {
            VfsRequest::Fork {
                parent_endpoint,
                child_endpoint,
            } => {
                assert_eq!(parent_endpoint, Endpoint::PM);
                assert_eq!(child_endpoint, Endpoint::from_generation_slot(1, 1));
            }
        }
    }

    #[test]
    fn test_vfs_response_fork_ok() {
        let resp = VfsResponse::ForkOk;
        assert!(matches!(resp, VfsResponse::ForkOk));
    }

    #[test]
    fn test_vfs_error_to_errno() {
        assert_eq!(VfsError::ProcTableFull.to_errno(), EAGAIN);
        assert_eq!(VfsError::TooManyOpenFiles.to_errno(), EMFILE);
        assert_eq!(VfsError::NotImplemented.to_errno(), ENOSYS);
    }
}

// ── VFS_PM 协议（PM → VFS）──
//
// C: include/minix/com.h:513-531（VFS_PM_RQ_BASE / VFS_PM_* 消息族）+
//    com.h:547-551（VFS_PM_INIT 字段 m7_i1/m7_i2/m7_i3，mess_7 布局）。
// PM 侧发送方见 os/servers/pm/src/init.rs（04-stage-pm/01-pm-init-main.md §2.1 第 6 步）；
// VFS 侧接收方见 05-stage-vfs/01-vfs-init-main.md §2.3。

/// VFS_PM request message type base（com.h:513）。
pub const VFS_PM_RQ_BASE: i32 = 0x900;

/// VFS_PM_INIT — process table exchange（com.h:520）。
///
/// PM 逐条发送 boot 进程信息（槽号 + PID + endpoint），末条
/// endpoint = NONE 表示"没有更多系统进程"并同步等待 VFS 回复 OK。
#[allow(clippy::identity_op)] // 保持与 C `(VFS_PM_RQ_BASE + 0)` 同形。
pub const VFS_PM_INIT: i32 = VFS_PM_RQ_BASE + 0;

// ── PM → VFS 运行时请求（com.h:521-531，不含启动期 VFS_PM_INIT）──
/// VFS_PM_SETUID — 替换有效/真实 UID（com.h:521）。
pub const VFS_PM_SETUID: i32 = VFS_PM_RQ_BASE + 1;
/// VFS_PM_SETGID — 替换有效/真实 GID（com.h:522）。
pub const VFS_PM_SETGID: i32 = VFS_PM_RQ_BASE + 2;
/// VFS_PM_SETSID — 创建新会话（com.h:523）。
pub const VFS_PM_SETSID: i32 = VFS_PM_RQ_BASE + 3;
/// VFS_PM_EXIT — 进程退出（com.h:524）。
pub const VFS_PM_EXIT: i32 = VFS_PM_RQ_BASE + 4;
/// VFS_PM_DUMPCORE — 进程 core dump（com.h:525）。
pub const VFS_PM_DUMPCORE: i32 = VFS_PM_RQ_BASE + 5;
/// VFS_PM_EXEC — 装载可执行镜像（com.h:526）。
pub const VFS_PM_EXEC: i32 = VFS_PM_RQ_BASE + 6;
/// VFS_PM_FORK — 复制父进程 fd 表（com.h:527）。
pub const VFS_PM_FORK: i32 = VFS_PM_RQ_BASE + 7;
/// VFS_PM_SRV_FORK — 服务进程 fork（com.h:528）。
pub const VFS_PM_SRV_FORK: i32 = VFS_PM_RQ_BASE + 8;
/// VFS_PM_UNPAUSE — 解除进程挂起（com.h:529）。
pub const VFS_PM_UNPAUSE: i32 = VFS_PM_RQ_BASE + 9;
/// VFS_PM_REBOOT — 系统重启（com.h:530）。
pub const VFS_PM_REBOOT: i32 = VFS_PM_RQ_BASE + 10;
/// VFS_PM_SETGROUPS — 替换补充组（com.h:531）。
pub const VFS_PM_SETGROUPS: i32 = VFS_PM_RQ_BASE + 11;

// ── VFS → PM 回复（com.h:534-544）──
/// VFS_PM response message type base（com.h:514）。
pub const VFS_PM_RS_BASE: i32 = 0x980;

/// VFS_PM_SETUID_REPLY（com.h:534）。
pub const VFS_PM_SETUID_REPLY: i32 = VFS_PM_RS_BASE + 1;
/// VFS_PM_SETGID_REPLY（com.h:535）。
pub const VFS_PM_SETGID_REPLY: i32 = VFS_PM_RS_BASE + 2;
/// VFS_PM_SETSID_REPLY（com.h:536）。
pub const VFS_PM_SETSID_REPLY: i32 = VFS_PM_RS_BASE + 3;
/// VFS_PM_EXIT_REPLY（com.h:537）。
pub const VFS_PM_EXIT_REPLY: i32 = VFS_PM_RS_BASE + 4;
/// VFS_PM_CORE_REPLY（com.h:538）。
pub const VFS_PM_CORE_REPLY: i32 = VFS_PM_RS_BASE + 5;
/// VFS_PM_EXEC_REPLY（com.h:539）。
pub const VFS_PM_EXEC_REPLY: i32 = VFS_PM_RS_BASE + 6;
/// VFS_PM_FORK_REPLY（com.h:540）。
pub const VFS_PM_FORK_REPLY: i32 = VFS_PM_RS_BASE + 7;
/// VFS_PM_SRV_FORK_REPLY（com.h:541）。
pub const VFS_PM_SRV_FORK_REPLY: i32 = VFS_PM_RS_BASE + 8;
/// VFS_PM_UNPAUSE_REPLY（com.h:542）。
pub const VFS_PM_UNPAUSE_REPLY: i32 = VFS_PM_RS_BASE + 9;
/// VFS_PM_REBOOT_REPLY（com.h:543）。
pub const VFS_PM_REBOOT_REPLY: i32 = VFS_PM_RS_BASE + 10;
/// VFS_PM_SETGROUPS_REPLY（com.h:544）。
pub const VFS_PM_SETGROUPS_REPLY: i32 = VFS_PM_RS_BASE + 11;

/// C: `IS_VFS_PM_RQ(type)` — com.h:516 的 `((type) & ~0x7f) == VFS_PM_RQ_BASE`。
///
/// 注意 `VFS_PM_INIT`（0x900）同属请求族。
pub const fn is_vfs_pm_rq(m_type: i32) -> bool {
    (m_type & !0x7f) == VFS_PM_RQ_BASE
}

/// C: `IS_VFS_PM_RS(type)` — com.h:517 的 `((type) & ~0x7f) == VFS_PM_RS_BASE`。
pub const fn is_vfs_pm_rs(m_type: i32) -> bool {
    (m_type & !0x7f) == VFS_PM_RS_BASE
}

/// PM → VFS 的 `VFS_PM_INIT` 消息负载。
///
/// C: com.h:520（m_type）+ com.h:547-551（VFS_PM_ENDPT=m7_i1 /
/// VFS_PM_SLOT=m7_i2 / VFS_PM_PID=m7_i3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VfsPmInit {
    /// 进程槽号（boot image 的 proc_nr）。
    pub slot: i32,
    /// 进程 PID。
    pub pid: Pid,
    /// 进程 endpoint。
    pub endpoint: Endpoint,
}

impl VfsPmInit {
    /// 编码为 IPC 消息（`MessageM7`，与 C mess_7 字段对齐）。
    pub fn encode(&self) -> Message {
        let m7 = MessageM7 {
            m7i1: self.endpoint.get(),
            m7i2: self.slot,
            m7i3: self.pid,
            m7i4: 0,
            m7i5: 0,
            m7p1: 0,
            m7p2: 0,
            _padding: [0; 20],
        };
        // 联合体构造（仅指定一个成员）与成员写入均为安全操作。
        Message {
            m_source: Endpoint::NONE,
            m_type: VFS_PM_INIT,
            m_u: MessageUnion { m_m7: m7 },
        }
    }

    /// 从 IPC 消息解码。
    ///
    /// 仅验证 `m_type` 与槽号范围；endpoint 原样保留（NONE 终止符
    /// 由握手逻辑判别，等价 C 的 `if (NONE == mess.VFS_PM_ENDPT) break;`）。
    pub fn decode(msg: &Message) -> Result<Self, VfsPmInitError> {
        if msg.m_type != VFS_PM_INIT {
            return Err(VfsPmInitError::WrongMessageType(msg.m_type));
        }
        // m_type 已在上面验证；联合体读取与 C 的 union 语义一致。
        let m7 = unsafe { &msg.m_u.m_m7 };
        let slot = m7.m7i2;
        if slot < 0 || slot as usize >= NR_PROCS {
            return Err(VfsPmInitError::SlotOutOfRange(slot));
        }
        Ok(Self {
            slot,
            pid: m7.m7i3,
            endpoint: Endpoint(m7.m7i1),
        })
    }
}

/// `VFS_PM_INIT` 解码错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsPmInitError {
    /// `m_type` 不是 `VFS_PM_INIT`。
    WrongMessageType(i32),
    /// 槽号超出 `[0, NR_PROCS)`。
    SlotOutOfRange(i32),
}

#[cfg(test)]
mod vfs_pm_init_tests {
    use super::*;

    #[test]
    fn test_vfs_pm_init_encode_decode_roundtrip() {
        let init = VfsPmInit {
            slot: 11,
            pid: 42,
            endpoint: Endpoint::from_generation_slot(1, 11),
        };
        let msg = init.encode();
        assert_eq!(msg.m_type, VFS_PM_INIT);

        let decoded = VfsPmInit::decode(&msg).unwrap();
        assert_eq!(decoded, init);
    }

    #[test]
    fn test_vfs_pm_init_decode_none_terminator() {
        // 末条屏障：endpoint = NONE（com.h:428-431 对端语义）。
        let msg = VfsPmInit {
            slot: 0,
            pid: 0,
            endpoint: Endpoint::NONE,
        }
        .encode();
        let decoded = VfsPmInit::decode(&msg).unwrap();
        assert!(decoded.endpoint.is_none());
    }

    #[test]
    fn test_vfs_pm_init_decode_wrong_type() {
        let msg = Message {
            m_type: VFS_PM_INIT + 1,
            ..Message::default()
        };
        let err = VfsPmInit::decode(&msg).unwrap_err();
        assert_eq!(err, VfsPmInitError::WrongMessageType(VFS_PM_INIT + 1));
    }

    #[test]
    fn test_vfs_pm_init_decode_slot_out_of_range() {
        let msg = VfsPmInit {
            slot: NR_PROCS as i32 + 5,
            pid: 1,
            endpoint: Endpoint::PM,
        }
        .encode();
        let err = VfsPmInit::decode(&msg).unwrap_err();
        assert_eq!(err, VfsPmInitError::SlotOutOfRange(NR_PROCS as i32 + 5));
    }

    #[test]
    fn test_vfs_pm_init_constants() {
        assert_eq!(VFS_PM_RQ_BASE, 0x900);
        assert_eq!(VFS_PM_INIT, 0x900);
    }
}

// ── PM → VFS 运行时请求（不含启动期 VFS_PM_INIT）──
//
// C: com.h:521-531（11 种请求）+ com.h:547-583（mess_7 字段布局）。
// 同一 union 槽位在不同请求间复用（如 VFS_PM_STATUS / VFS_PM_PENDPT /
// VFS_PM_GROUP_NO / VFS_PM_EID / VFS_PM_SLOT / VFS_PM_TERM_SIG 同为 m7_i2），
// 这里用类型化字段在编译期排除错配（见 04-stage-pm/05-design.v1.md D2）。

/// PM → VFS 的运行时请求。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsCall {
    /// VFS_PM_SETUID — 替换进程的有效/真实 UID（getset.c:121）。
    SetUid {
        /// 目标进程 endpoint（VFS_PM_ENDPT = m7_i1）。
        endpoint: Endpoint,
        /// 有效 UID（VFS_PM_EID = m7_i2）。
        eid: i32,
        /// 真实 UID（VFS_PM_RID = m7_i3）。
        rid: i32,
    },
    /// VFS_PM_SETGID — 替换有效/真实 GID（getset.c:136）。
    SetGid {
        /// 目标进程 endpoint（VFS_PM_ENDPT = m7_i1）。
        endpoint: Endpoint,
        /// 有效 GID（VFS_PM_EID = m7_i2）。
        eid: i32,
        /// 真实 GID（VFS_PM_RID = m7_i3）。
        rid: i32,
    },
    /// VFS_PM_SETGROUPS — 替换补充组（getset.c:151）。
    SetGroups {
        /// 目标进程 endpoint（VFS_PM_ENDPT = m7_i1）。
        endpoint: Endpoint,
        /// 组数量（VFS_PM_GROUP_NO = m7_i2）。
        group_no: i32,
        /// 组数组地址（VFS_PM_GROUP_ADDR = m7_p1）。
        group_addr: u64,
    },
    /// VFS_PM_SETSID — 创建新会话（getset.c:199）。
    SetSid {
        /// 目标进程 endpoint（VFS_PM_ENDPT = m7_i1）。
        endpoint: Endpoint,
    },
    /// VFS_PM_EXEC — 装载可执行镜像（exec.c:44）。
    Exec {
        /// 目标进程 endpoint（VFS_PM_ENDPT = m7_i1）。
        endpoint: Endpoint,
        /// 可执行路径（VFS_PM_PATH = m7_p1）。
        path: u64,
        /// 路径长度（含 NUL，VFS_PM_PATH_LEN = m7_i2）。
        path_len: i32,
        /// 参数与环境帧（VFS_PM_FRAME = m7_p2）。
        frame: u64,
        /// 帧大小（VFS_PM_FRAME_LEN = m7_i3）。
        frame_len: i32,
        /// ps_strings 指针（VFS_PM_PS_STR = m7_i5）。
        ps_str: i32,
    },
    /// VFS_PM_EXIT — 进程退出（forkexit.c:351）。
    Exit {
        /// 目标进程 endpoint（VFS_PM_ENDPT = m7_i1）。
        endpoint: Endpoint,
    },
    /// VFS_PM_DUMPCORE — 进程 core dump（forkexit.c:351）。
    DumpCore {
        /// 目标进程 endpoint（VFS_PM_ENDPT = m7_i1）。
        endpoint: Endpoint,
        /// 终止信号（VFS_PM_TERM_SIG = m7_i2）。
        term_sig: i32,
        /// core 文件路径（VFS_PM_PATH = m7_p1）。
        path: u64,
    },
    /// VFS_PM_FORK — 复制父进程 fd 表（forkexit.c:123）。
    Fork {
        /// **子**进程 endpoint（VFS_PM_ENDPT = m7_i1）。
        child: Endpoint,
        /// 父进程 endpoint（VFS_PM_PENDPT = m7_i2）。
        parent: Endpoint,
        /// 子进程 PID（VFS_PM_CPID = m7_i3）。
        child_pid: Pid,
    },
    /// VFS_PM_SRV_FORK — 服务进程 fork（forkexit.c:223）。
    SrvFork {
        /// **子**进程 endpoint（VFS_PM_ENDPT = m7_i1）。
        child: Endpoint,
        /// 父进程 endpoint（VFS_PM_PENDPT = m7_i2）。
        parent: Endpoint,
        /// 子进程 PID（VFS_PM_CPID = m7_i3）。
        child_pid: Pid,
        /// 真实/有效 UID（VFS_PM_REUID = m7_i4）。
        reuid: i32,
        /// 真实/有效 GID（VFS_PM_REGID = m7_i5）。
        regid: i32,
    },
    /// VFS_PM_UNPAUSE — 解除进程挂起（signal.c:764）。
    Unpause {
        /// 目标进程 endpoint（VFS_PM_ENDPT = m7_i1）。
        endpoint: Endpoint,
    },
    /// VFS_PM_REBOOT — 系统重启（misc.c:228）。
    ///
    /// 唯一不与任何用户进程关联的请求（com.h:301-302 注释）。
    Reboot,
}

impl VfsCall {
    /// 消息类型（C: `m_type`）。
    pub const fn m_type(&self) -> i32 {
        match self {
            Self::SetUid { .. } => VFS_PM_SETUID,
            Self::SetGid { .. } => VFS_PM_SETGID,
            Self::SetGroups { .. } => VFS_PM_SETGROUPS,
            Self::SetSid { .. } => VFS_PM_SETSID,
            Self::Exec { .. } => VFS_PM_EXEC,
            Self::Exit { .. } => VFS_PM_EXIT,
            Self::DumpCore { .. } => VFS_PM_DUMPCORE,
            Self::Fork { .. } => VFS_PM_FORK,
            Self::SrvFork { .. } => VFS_PM_SRV_FORK,
            Self::Unpause { .. } => VFS_PM_UNPAUSE,
            Self::Reboot => VFS_PM_REBOOT,
        }
    }

    /// 目标进程 endpoint；`Reboot` 不与任何进程关联，返回 `None`。
    pub const fn endpoint(&self) -> Option<Endpoint> {
        match self {
            Self::SetUid { endpoint, .. }
            | Self::SetGid { endpoint, .. }
            | Self::SetGroups { endpoint, .. }
            | Self::SetSid { endpoint }
            | Self::Exec { endpoint, .. }
            | Self::Exit { endpoint }
            | Self::DumpCore { endpoint, .. }
            | Self::Unpause { endpoint } => Some(*endpoint),
            Self::Fork { child, .. } | Self::SrvFork { child, .. } => Some(*child),
            Self::Reboot => None,
        }
    }

    /// 编码为 IPC 消息（`MessageM7`，字段布局与 C mess_7 对齐）。
    ///
    /// `m_source` 置 `Endpoint::NONE`：发送目标由 `IpcTransport::send` 的
    /// 目标参数决定，与 C `asynsend3(VFS_PROC_NR, ...)` 等价。`Fork` 的
    /// `REUID`/`REGID` 槽位显式置 `-1`（forkexit.c:129-130 注释
    /// "Not used by VFS_PM_FORK"），其余未使用槽位置 0。
    pub fn encode(&self) -> Message {
        let (i1, i2, i3, i4, i5, p1, p2) = match *self {
            Self::SetUid { endpoint, eid, rid } => (endpoint.get(), eid, rid, 0, 0, 0, 0),
            Self::SetGid { endpoint, eid, rid } => (endpoint.get(), eid, rid, 0, 0, 0, 0),
            Self::SetGroups {
                endpoint,
                group_no,
                group_addr,
            } => (endpoint.get(), group_no, 0, 0, 0, group_addr, 0),
            Self::SetSid { endpoint } => (endpoint.get(), 0, 0, 0, 0, 0, 0),
            Self::Exec {
                endpoint,
                path,
                path_len,
                frame,
                frame_len,
                ps_str,
            } => (endpoint.get(), path_len, frame_len, 0, ps_str, path, frame),
            Self::Exit { endpoint } => (endpoint.get(), 0, 0, 0, 0, 0, 0),
            Self::DumpCore {
                endpoint,
                term_sig,
                path,
            } => (endpoint.get(), term_sig, 0, 0, 0, path, 0),
            Self::Fork {
                child,
                parent,
                child_pid,
            } => (child.get(), parent.get(), child_pid, -1, -1, 0, 0),
            Self::SrvFork {
                child,
                parent,
                child_pid,
                reuid,
                regid,
            } => (child.get(), parent.get(), child_pid, reuid, regid, 0, 0),
            Self::Unpause { endpoint } => (endpoint.get(), 0, 0, 0, 0, 0, 0),
            Self::Reboot => (0, 0, 0, 0, 0, 0, 0),
        };
        let m7 = MessageM7 {
            m7i1: i1,
            m7i2: i2,
            m7i3: i3,
            m7i4: i4,
            m7i5: i5,
            m7p1: p1,
            m7p2: p2,
            _padding: [0; 20],
        };
        Message {
            m_source: Endpoint::NONE,
            m_type: self.m_type(),
            m_u: MessageUnion { m_m7: m7 },
        }
    }
}

/// VFS → PM 的回复。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsReply {
    /// VFS_PM_SETUID_REPLY（main.c:335 分组）。
    SetUid,
    /// VFS_PM_SETGID_REPLY。
    SetGid,
    /// VFS_PM_SETGROUPS_REPLY。
    SetGroups,
    /// VFS_PM_SETSID_REPLY。
    SetSid,
    /// VFS_PM_EXEC_REPLY — 携带新程序入口与栈（main.c:398-407）。
    Exec {
        /// 执行状态（OK 或失败，VFS_PM_STATUS = m7_i2）。
        status: i32,
        /// 程序计数器（VFS_PM_PC = m7_p1）。
        pc: u64,
        /// 可能更新的栈指针（VFS_PM_NEWSP = m7_p2）。
        newsp: u64,
        /// 可能更新的 ps_strings 指针（VFS_PM_NEWPS_STR = m7_i5）。
        newps_str: i32,
    },
    /// VFS_PM_CORE_REPLY — 携带 core dump 状态（main.c:409-415）。
    Core {
        /// 状态（OK 或失败，VFS_PM_STATUS = m7_i2）。
        status: i32,
    },
    /// VFS_PM_EXIT_REPLY（main.c:419-421）。
    Exit,
    /// VFS_PM_FORK_REPLY（main.c:372-396）。
    Fork,
    /// VFS_PM_SRV_FORK_REPLY（main.c:372-396）。
    SrvFork,
    /// VFS_PM_UNPAUSE_REPLY（main.c:417 后走尾部）。
    Unpause,
    /// VFS_PM_REBOOT_REPLY（main.c:304-312 特例）。
    Reboot,
}

impl VfsReply {
    /// 消息类型（C: `m_type`）。
    pub const fn m_type(&self) -> i32 {
        match self {
            Self::SetUid => VFS_PM_SETUID_REPLY,
            Self::SetGid => VFS_PM_SETGID_REPLY,
            Self::SetGroups => VFS_PM_SETGROUPS_REPLY,
            Self::SetSid => VFS_PM_SETSID_REPLY,
            Self::Exec { .. } => VFS_PM_EXEC_REPLY,
            Self::Core { .. } => VFS_PM_CORE_REPLY,
            Self::Exit => VFS_PM_EXIT_REPLY,
            Self::Fork => VFS_PM_FORK_REPLY,
            Self::SrvFork => VFS_PM_SRV_FORK_REPLY,
            Self::Unpause => VFS_PM_UNPAUSE_REPLY,
            Self::Reboot => VFS_PM_REBOOT_REPLY,
        }
    }

    /// 从 IPC 消息解码。
    ///
    /// 先校验落于 RS 族（C: `main.c:84` 的 `IS_VFS_PM_RS` 族判定保证进入
    /// `handle_vfs_reply` 的必是 RS 族）；`NotAReply` 覆盖族外 `m_type`，
    /// `UnknownReply` 覆盖族内未定义编号（对应 C `default: panic("unknown
    /// reply code")`）。
    pub fn decode(msg: &Message) -> Result<Self, VfsReplyError> {
        if !is_vfs_pm_rs(msg.m_type) {
            return Err(VfsReplyError::NotAReply(msg.m_type));
        }
        // m_type 已在上面验证属于 RS 族；联合体读取与 C 的 union 语义一致。
        let m7 = unsafe { &msg.m_u.m_m7 };
        Ok(match msg.m_type {
            VFS_PM_SETUID_REPLY => Self::SetUid,
            VFS_PM_SETGID_REPLY => Self::SetGid,
            VFS_PM_SETGROUPS_REPLY => Self::SetGroups,
            VFS_PM_SETSID_REPLY => Self::SetSid,
            VFS_PM_EXEC_REPLY => Self::Exec {
                status: m7.m7i2,
                pc: m7.m7p1,
                newsp: m7.m7p2,
                newps_str: m7.m7i5,
            },
            VFS_PM_CORE_REPLY => Self::Core { status: m7.m7i2 },
            VFS_PM_EXIT_REPLY => Self::Exit,
            VFS_PM_FORK_REPLY => Self::Fork,
            VFS_PM_SRV_FORK_REPLY => Self::SrvFork,
            VFS_PM_UNPAUSE_REPLY => Self::Unpause,
            VFS_PM_REBOOT_REPLY => Self::Reboot,
            _ => return Err(VfsReplyError::UnknownReply(msg.m_type)),
        })
    }
}

/// `VfsReply::decode` / `handle_vfs_reply` 错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsReplyError {
    /// `m_type` 不在 RS 族（见 `is_vfs_pm_rs`）。
    NotAReply(i32),
    /// `m_type` 在 RS 族但无对应 variant。
    UnknownReply(i32),
    /// 回复中的 endpoint 无法解析为有效槽位（C: `pm_isokendpt` 失败）。
    BadEndpoint(Endpoint),
}

#[cfg(test)]
mod vfs_call_reply_tests {
    use super::*;

    /// 逐路请求编码 → 解码往返，并校验 m_type 与族判定。
    fn roundtrip(call: VfsCall) {
        assert!(is_vfs_pm_rq(call.m_type()));
        assert!(!is_vfs_pm_rs(call.m_type()));
        let msg = call.encode();
        assert_eq!(msg.m_type, call.m_type());
        // REBOOT 之外，目标进程 endpoint 落于 m7_i1。
        if let Some(ep) = call.endpoint() {
            let m7 = unsafe { &msg.m_u.m_m7 };
            assert_eq!(m7.m7i1, ep.get(), "endpoint 必须编码到 m7_i1");
        }
    }

    #[test]
    fn test_vfs_call_roundtrip_all() {
        roundtrip(VfsCall::SetUid {
            endpoint: Endpoint::from_generation_slot(1, 5),
            eid: 100,
            rid: 100,
        });
        roundtrip(VfsCall::SetGid {
            endpoint: Endpoint::from_generation_slot(1, 5),
            eid: 200,
            rid: 200,
        });
        roundtrip(VfsCall::SetGroups {
            endpoint: Endpoint::from_generation_slot(1, 5),
            group_no: 3,
            group_addr: 0x8000_1000,
        });
        roundtrip(VfsCall::SetSid {
            endpoint: Endpoint::from_generation_slot(1, 5),
        });
        roundtrip(VfsCall::Exec {
            endpoint: Endpoint::from_generation_slot(1, 5),
            path: 0x4000_0000,
            path_len: 12,
            frame: 0x4000_1000,
            frame_len: 256,
            ps_str: 0x7fff_0000i32,
        });
        roundtrip(VfsCall::Exit {
            endpoint: Endpoint::from_generation_slot(1, 5),
        });
        roundtrip(VfsCall::DumpCore {
            endpoint: Endpoint::from_generation_slot(1, 5),
            term_sig: 11,
            path: 0x5000_0000,
        });
        roundtrip(VfsCall::Fork {
            child: Endpoint::from_generation_slot(1, 6),
            parent: Endpoint::from_generation_slot(1, 5),
            child_pid: 1234,
        });
        roundtrip(VfsCall::SrvFork {
            child: Endpoint::from_generation_slot(1, 6),
            parent: Endpoint::from_generation_slot(1, 5),
            child_pid: 1234,
            reuid: 7,
            regid: 8,
        });
        roundtrip(VfsCall::Unpause {
            endpoint: Endpoint::from_generation_slot(1, 5),
        });
        roundtrip(VfsCall::Reboot);
    }

    #[test]
    fn test_vfs_call_fork_sentinels() {
        // VFS_PM_FORK 的 REUID/REGID 槽位必须 -1（forkexit.c:129-130）。
        let msg = VfsCall::Fork {
            child: Endpoint::from_generation_slot(1, 6),
            parent: Endpoint::from_generation_slot(1, 5),
            child_pid: 1234,
        }
        .encode();
        let m7 = unsafe { &msg.m_u.m_m7 };
        assert_eq!(m7.m7i4, -1);
        assert_eq!(m7.m7i5, -1);
        // 父 endpoint 在 m7_i2，子 pid 在 m7_i3。
        assert_eq!(m7.m7i2, Endpoint::from_generation_slot(1, 5).get());
        assert_eq!(m7.m7i3, 1234);
    }

    #[test]
    fn test_vfs_call_exec_layout() {
        let msg = VfsCall::Exec {
            endpoint: Endpoint::from_generation_slot(1, 5),
            path: 0x4000_0000,
            path_len: 12,
            frame: 0x4000_1000,
            frame_len: 256,
            ps_str: 0x7fff_0000i32,
        }
        .encode();
        let m7 = unsafe { &msg.m_u.m_m7 };
        assert_eq!(m7.m7i2, 12);
        assert_eq!(m7.m7i3, 256);
        assert_eq!(m7.m7p1, 0x4000_0000);
        assert_eq!(m7.m7p2, 0x4000_1000);
        assert_eq!(m7.m7i5, 0x7fff_0000i32);
    }

    #[test]
    fn test_vfs_call_reboot_has_no_endpoint() {
        assert_eq!(VfsCall::Reboot.endpoint(), None);
        let msg = VfsCall::Reboot.encode();
        assert_eq!(msg.m_type, VFS_PM_REBOOT);
        let m7 = unsafe { &msg.m_u.m_m7 };
        assert_eq!(m7.m7i1, 0);
    }

    #[test]
    fn test_vfs_reply_decode_all() {
        let mk = |m_type: i32| Message {
            m_type,
            ..Message::default()
        };
        assert_eq!(
            VfsReply::decode(&mk(VFS_PM_SETUID_REPLY)).unwrap(),
            VfsReply::SetUid
        );
        assert_eq!(
            VfsReply::decode(&mk(VFS_PM_SETGID_REPLY)).unwrap(),
            VfsReply::SetGid
        );
        assert_eq!(
            VfsReply::decode(&mk(VFS_PM_SETGROUPS_REPLY)).unwrap(),
            VfsReply::SetGroups
        );
        assert_eq!(
            VfsReply::decode(&mk(VFS_PM_SETSID_REPLY)).unwrap(),
            VfsReply::SetSid
        );
        assert_eq!(
            VfsReply::decode(&mk(VFS_PM_EXIT_REPLY)).unwrap(),
            VfsReply::Exit
        );
        assert_eq!(
            VfsReply::decode(&mk(VFS_PM_FORK_REPLY)).unwrap(),
            VfsReply::Fork
        );
        assert_eq!(
            VfsReply::decode(&mk(VFS_PM_SRV_FORK_REPLY)).unwrap(),
            VfsReply::SrvFork
        );
        assert_eq!(
            VfsReply::decode(&mk(VFS_PM_UNPAUSE_REPLY)).unwrap(),
            VfsReply::Unpause
        );
        assert_eq!(
            VfsReply::decode(&mk(VFS_PM_REBOOT_REPLY)).unwrap(),
            VfsReply::Reboot
        );
    }

    #[test]
    fn test_vfs_reply_decode_exec_payload() {
        let mut msg = Message {
            m_type: VFS_PM_EXEC_REPLY,
            ..Message::default()
        };
        let m7 = unsafe { &mut msg.m_u.m_m7 };
        m7.m7i2 = 0; // status OK
        m7.m7p1 = 0x1000;
        m7.m7p2 = 0x2000;
        m7.m7i5 = 0x3000;
        assert_eq!(
            VfsReply::decode(&msg).unwrap(),
            VfsReply::Exec {
                status: 0,
                pc: 0x1000,
                newsp: 0x2000,
                newps_str: 0x3000,
            }
        );
    }

    #[test]
    fn test_vfs_reply_decode_core_status() {
        let mut msg = Message {
            m_type: VFS_PM_CORE_REPLY,
            ..Message::default()
        };
        let m7 = unsafe { &mut msg.m_u.m_m7 };
        m7.m7i2 = -1;
        assert_eq!(
            VfsReply::decode(&msg).unwrap(),
            VfsReply::Core { status: -1 }
        );
    }

    #[test]
    fn test_vfs_reply_decode_not_a_reply() {
        // 请求族与未知编号都不应通过 decode。
        assert_eq!(
            VfsReply::decode(&Message {
                m_type: VFS_PM_EXEC,
                ..Message::default()
            })
            .unwrap_err(),
            VfsReplyError::NotAReply(VFS_PM_EXEC)
        );
        let bogus = VFS_PM_RS_BASE + 12; // 族内但无 variant
        assert!(
            is_vfs_pm_rs(bogus),
            "bogus 必须落在 RS 族内以测试 UnknownReply 分支"
        );
        assert_eq!(
            VfsReply::decode(&Message {
                m_type: bogus,
                ..Message::default()
            })
            .unwrap_err(),
            VfsReplyError::UnknownReply(bogus)
        );
    }

    #[test]
    fn test_vfs_pm_family_predicates() {
        assert!(is_vfs_pm_rq(VFS_PM_INIT));
        assert!(is_vfs_pm_rq(VFS_PM_EXEC));
        assert!(is_vfs_pm_rs(VFS_PM_EXEC_REPLY));
        // 边界：0x980 + 0x80 = 0xA00 不应命中 RS 族（低 7 位被掩去）。
        assert!(!is_vfs_pm_rs(VFS_PM_RS_BASE + 0x80));
        // 普通 PM 调用（如 PM_GETPROCNR = 0~）不在任一族。
        assert!(!is_vfs_pm_rq(0));
    }

    #[test]
    fn test_vfs_pm_constants_match_com_h() {
        // 锁定线协议数值与 C com.h:520-544 完全一致，防止偏移错位
        // （枚举顺序若与 C 宏编号不同会导致线值错误，属 P0-fact）。
        // RQ 族（com.h:521-531）
        assert_eq!(VFS_PM_SETUID, 0x901);
        assert_eq!(VFS_PM_SETGID, 0x902);
        assert_eq!(VFS_PM_SETSID, 0x903);
        assert_eq!(VFS_PM_EXIT, 0x904);
        assert_eq!(VFS_PM_DUMPCORE, 0x905);
        assert_eq!(VFS_PM_EXEC, 0x906);
        assert_eq!(VFS_PM_FORK, 0x907);
        assert_eq!(VFS_PM_SRV_FORK, 0x908);
        assert_eq!(VFS_PM_UNPAUSE, 0x909);
        assert_eq!(VFS_PM_REBOOT, 0x90a);
        assert_eq!(VFS_PM_SETGROUPS, 0x90b);
        // RS 族（com.h:534-544）
        assert_eq!(VFS_PM_SETUID_REPLY, 0x981);
        assert_eq!(VFS_PM_SETGID_REPLY, 0x982);
        assert_eq!(VFS_PM_SETSID_REPLY, 0x983);
        assert_eq!(VFS_PM_EXIT_REPLY, 0x984);
        assert_eq!(VFS_PM_CORE_REPLY, 0x985);
        assert_eq!(VFS_PM_EXEC_REPLY, 0x986);
        assert_eq!(VFS_PM_FORK_REPLY, 0x987);
        assert_eq!(VFS_PM_SRV_FORK_REPLY, 0x988);
        assert_eq!(VFS_PM_UNPAUSE_REPLY, 0x989);
        assert_eq!(VFS_PM_REBOOT_REPLY, 0x98a);
        assert_eq!(VFS_PM_SETGROUPS_REPLY, 0x98b);
    }
}
