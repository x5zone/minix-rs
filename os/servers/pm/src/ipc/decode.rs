//! 入站消息解码单点——wire union 到 handler 参数的唯一通道。
//!
//! `MessageUnion` 是 C `message` union 的 `#[repr(C)]` 镜像
//! （`minix3/minix/include/minix/ipc.h`），按 `m_type` 选取成员的读取
//! 天然是 unsafe。本模块把这份 unsafe 收敛为每调用一个解码函数：
//! unsafe 面限制在单个 Copy 结构的按值读取，逐点附 SAFETY 论证与
//! ipc.h 锚点；`calls.rs` 的 dispatch 臂只消费这里的安全函数，
//! 不再出现裸 union 读。
//!
//! C 侧对应物是各 handler 开头的 `m->m_lc_pm_xxx` 字段选取
//! （如 `forkexit.c`、`signal.c`、`trace.c`）；解码即 C 里
//! "从 message 联合体取出本调用参数" 这一步的 Rust 对应物。

use minix_types::{
    MessLcPmExit, MessLcPmKill, MessLcPmPtrace, MessLcPmWait4, MessLsysPmProceventmask,
    MessLsysPmSrvFork, MessRsPmSrvKill, Message,
};

// 布局断言：涉及的 wire 结构必须留在 IPC 载荷内（C 侧对应
// ipc.h 各结构后的 `_ASSERT_MSG_SIZE`）。
const _: () = assert!(size_of::<MessLsysPmSrvFork>() <= minix_types::MESSAGE_PAYLOAD_SIZE);
const _: () = assert!(size_of::<MessLcPmExit>() <= minix_types::MESSAGE_PAYLOAD_SIZE);
const _: () = assert!(size_of::<MessLcPmWait4>() <= minix_types::MESSAGE_PAYLOAD_SIZE);
const _: () = assert!(size_of::<MessLcPmKill>() <= minix_types::MESSAGE_PAYLOAD_SIZE);
const _: () = assert!(size_of::<MessRsPmSrvKill>() <= minix_types::MESSAGE_PAYLOAD_SIZE);
const _: () = assert!(size_of::<MessLsysPmProceventmask>() <= minix_types::MESSAGE_PAYLOAD_SIZE);
const _: () = assert!(size_of::<MessLcPmPtrace>() <= minix_types::MESSAGE_PAYLOAD_SIZE);

/// srv_fork 参数 (uid, gid)。RS → PM。
///
/// C: `mess_lsys_pm_srv_fork` — ipc.h:1422-1427（`do_srv_fork` 消费
/// uid/gid，forkexit.c:206-211）。
pub(crate) fn srv_fork(msg: &Message) -> (u32, u32) {
    // SAFETY: 调用契约由 dispatch 保证——m_type 已匹配 SRV_FORK 才会
    // 到达本函数，发送方按同一成员写入。成员是全 Copy 的 #[repr(C)]
    // 结构，按值读取不产生引用，无别名风险。
    let pl = unsafe { msg.m_u.m_lsys_pm_srv_fork };
    (pl.uid, pl.gid)
}

/// exit 状态。user → PM。
///
/// C: `mess_lc_pm_exit` — ipc.h:446-450（`do_exit` 消费 status，
/// forkexit.c:259）。
pub(crate) fn exit(msg: &Message) -> i32 {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路，Copy 按值读。
    let pl = unsafe { msg.m_u.m_lc_pm_exit };
    pl.status
}

/// wait4 参数 (pid, options, addr)。user → PM。
///
/// C: `mess_lc_pm_wait4` — ipc.h:585-591（`do_wait` 消费 pid 等，
/// forkexit.c:490）；`addr` 是用户态 `struct rusage*` 的 VirBytes 视图。
pub(crate) fn wait4(msg: &Message) -> (i32, i32, u64) {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路，Copy 按值读。
    let pl = unsafe { msg.m_u.m_lc_pm_wait4 };
    (pl.pid, pl.options, pl.addr)
}

/// kill 参数 (pid, signo)。user → PM。
///
/// C 无独立 kill 成员——`do_kill` 直接读 `m_in.m_lc_pm_sig`
/// （signal.c:201，与 sigaction 共用 `mess_lc_pm_sig`，ipc.h:531-540）。
/// Rust wire 为 kill 单列 `MessLcPmKill`（内部建模选择）：前 8 字节
/// (pid, signo) 与 `mess_lc_pm_sig` 的 (pid, nr) 同位同型，读取等价。
pub(crate) fn kill(msg: &Message) -> (i32, i32) {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路，Copy 按值读。
    let pl = unsafe { msg.m_u.m_lc_pm_kill };
    (pl.pid, pl.signo)
}

/// srv_kill 参数 (pid, signo)。RS → PM。
///
/// C: `mess_rs_pm_srv_kill` — ipc.h:1879-1884（`do_srv_kill` 消费
/// pid/nr，signal.c:219）。
pub(crate) fn srv_kill(msg: &Message) -> (i32, i32) {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路，Copy 按值读。
    let pl = unsafe { msg.m_u.m_rs_pm_srv_kill };
    (pl.pid, pl.signo)
}

/// proceventmask 掩码。subscriber → PM。
///
/// C: `mess_lsys_pm_proceventmask` — ipc.h:1415-1419（`do_proceventmask`
/// 消费 mask，event.c:179）。
pub(crate) fn proceventmask(msg: &Message) -> u32 {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路，Copy 按值读。
    let pl = unsafe { msg.m_u.m_lsys_pm_proceventmask };
    pl.mask
}

/// ptrace 参数 (pid, req, addr, data)。tracer → PM。
///
/// C: `mess_lc_pm_ptrace` — ipc.h:493-500（`do_trace` 消费，trace.c）。
pub(crate) fn ptrace(msg: &Message) -> (i32, i32, u64, i64) {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路，Copy 按值读。
    let pl = unsafe { msg.m_u.m_lc_pm_ptrace };
    (pl.pid, pl.req, pl.addr, pl.data)
}

/// itimer 参数 (which, value 指针, ovalue 指针)。user → PM。
///
/// C: `mess_lc_pm_itimer`（minix-types `MessLcPmItimer`，repr(C) 域序
/// which@0/value@8/ovalue@16；value/ovalue 是 `struct itimerval` 的
/// 用户态指针，字节搬运经网关 copy 缝）。MessageUnion 无专属臂——
/// 按字节读前 24 字节（vm.rs wrapper 的 raw 读取先例）。
pub(crate) fn itimer(msg: &Message) -> (i32, u64, u64) {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路；raw 臂按字节
    // 读取，无类型重解释，域序与 `MessLcPmItimer` 的 repr(C) 排布一致。
    let raw = unsafe { msg.m_u.raw };
    let u64_at = |off: usize| u64::from_le_bytes(raw[off..off + 8].try_into().unwrap());
    let which = i32::from_le_bytes(raw[0..4].try_into().unwrap());
    (which, u64_at(8), u64_at(16))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造只含指定成员载荷的消息（m_type 由 dispatch 契约决定，与解码无关）。
    fn msg_with(f: impl FnOnce(&mut Message)) -> Message {
        let mut m = Message::default();
        f(&mut m);
        m
    }

    #[test]
    fn srv_fork_decodes_uid_gid() {
        let m = msg_with(|m| {
            m.m_u.m_lsys_pm_srv_fork.uid = 1000;
            m.m_u.m_lsys_pm_srv_fork.gid = 100;
        });
        assert_eq!(srv_fork(&m), (1000, 100));
    }

    #[test]
    fn exit_decodes_status() {
        let m = msg_with(|m| m.m_u.m_lc_pm_exit.status = 7);
        assert_eq!(exit(&m), 7);
    }

    #[test]
    fn wait4_decodes_pid_options_addr() {
        let m = msg_with(|m| {
            m.m_u.m_lc_pm_wait4.pid = -1;
            m.m_u.m_lc_pm_wait4.options = 0x3;
            m.m_u.m_lc_pm_wait4.addr = 0x7000;
        });
        assert_eq!(wait4(&m), (-1, 0x3, 0x7000));
    }

    #[test]
    fn kill_decodes_pid_signo() {
        let m = msg_with(|m| {
            m.m_u.m_lc_pm_kill.pid = 42;
            m.m_u.m_lc_pm_kill.signo = 9;
        });
        assert_eq!(kill(&m), (42, 9));
    }

    #[test]
    fn srv_kill_decodes_pid_signo() {
        let m = msg_with(|m| {
            m.m_u.m_rs_pm_srv_kill.pid = 5;
            m.m_u.m_rs_pm_srv_kill.signo = 15;
        });
        assert_eq!(srv_kill(&m), (5, 15));
    }

    #[test]
    fn proceventmask_decodes_mask() {
        let m = msg_with(|m| m.m_u.m_lsys_pm_proceventmask.mask = 0b101);
        assert_eq!(proceventmask(&m), 0b101);
    }

    #[test]
    fn itimer_decodes_which_and_pointers() {
        let m = msg_with(|m| unsafe {
            // 按域序写 raw:which@0/value@8/ovalue@16(repr(C) 字节排布)。
            // 切片访问即对 union 字段的读借用,需 unsafe 块包裹。
            m.m_u.raw[0..4].copy_from_slice(&0i32.to_le_bytes());
            m.m_u.raw[8..16].copy_from_slice(&0x5000u64.to_le_bytes());
            m.m_u.raw[16..24].copy_from_slice(&0x6000u64.to_le_bytes());
        });
        assert_eq!(itimer(&m), (0, 0x5000, 0x6000));
    }

    #[test]
    fn ptrace_decodes_full_tuple() {
        let m = msg_with(|m| {
            m.m_u.m_lc_pm_ptrace.pid = 3;
            m.m_u.m_lc_pm_ptrace.req = 2;
            m.m_u.m_lc_pm_ptrace.addr = 0x4000;
            m.m_u.m_lc_pm_ptrace.data = 0x1234_5678;
        });
        assert_eq!(ptrace(&m), (3, 2, 0x4000, 0x1234_5678));
    }
}
