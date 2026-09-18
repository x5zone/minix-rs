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
// 凭证族三 wire(MessLcPmSetid/Groups/Getsid)的 MessageUnion 无专属臂,
// 解码按字节读前 24 域(vm.rs wrapper 的 raw 读取先例)。

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

/// exec 参数 (path 指针, path_len, frame 指针, framelen, ps_str)。
/// user/VFS → PM。
///
/// C: `mess_lc_pm_exec` — ipc.h:435-443(name@0/namelen@8/frame@16/
/// framelen@24/ps_str@32,LP64 各域 8 字节)。
pub(crate) fn exec(msg: &Message) -> (u64, u64, u64, u64, u64) {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路;五域各 8 字节。
    let raw = unsafe { msg.m_u.raw };
    let u64_at = |off: usize| u64::from_le_bytes(raw[off..off + 8].try_into().unwrap());
    (u64_at(0), u64_at(8), u64_at(16), u64_at(24), u64_at(32))
}

/// EXEC_NEW 参数 (endpt, exec_info 指针)。VFS/RS → PM。
///
/// C: `mess_lexec_pm_exec_new` — ipc.h:966-973(endpt@0/ptr@8);ptr 指向
/// VFS 内存中的 `struct exec_info`(拷入尺寸见 calls.rs EXEC_INFO_COPY_SIZE)。
pub(crate) fn exec_new(msg: &Message) -> (i32, u64) {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路;endpt@0/ptr@8。
    let raw = unsafe { msg.m_u.raw };
    let endpt = i32::from_le_bytes(raw[0..4].try_into().unwrap());
    let ptr = u64::from_le_bytes(raw[8..16].try_into().unwrap());
    (endpt, ptr)
}

/// EXEC_RESTART 参数 (endpt, result, pc, ps_str)。RS → PM。
///
/// C: `mess_rs_pm_exec_restart`(minix-types `MessRsPmExecRestart`——
/// endpt@0/result@4/pc@8/ps_str@16,ipc.h:665-673)。
pub(crate) fn exec_restart(msg: &Message) -> (i32, i32, u64, u64) {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路，Copy 按值读。
    let pl = unsafe { msg.m_u.m_rs_pm_exec_restart };
    (pl.endpt, pl.result, pl.pc, pl.ps_str)
}

/// sysuname 参数 (req, field, len, value 指针)。user → PM。
///
/// C: `mess_lc_pm_sysuname` — ipc.h:565-571(req@0/field@4/len@8/
/// value@16,raw 字节读,无专属臂)。
pub(crate) fn sysuname(msg: &Message) -> (i32, i32, u64, u64) {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路。
    let raw = unsafe { msg.m_u.raw };
    let req = i32::from_le_bytes(raw[0..4].try_into().unwrap());
    let field = i32::from_le_bytes(raw[4..8].try_into().unwrap());
    let len = u64::from_le_bytes(raw[8..16].try_into().unwrap());
    let value = u64::from_le_bytes(raw[16..24].try_into().unwrap());
    (req, field, len, value)
}

/// svrctl 参数 (request, arg)。user → PM。
///
/// C: `mess_lc_svrctl` — ipc.h:603-608(request@0/arg@8)。
pub(crate) fn svrctl(msg: &Message) -> (u64, u64) {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路。
    let raw = unsafe { msg.m_u.raw };
    let request = u64::from_le_bytes(raw[0..8].try_into().unwrap());
    let arg = u64::from_le_bytes(raw[8..16].try_into().unwrap());
    (request, arg)
}

/// sprof 参数 (action, freq, intr_type, ctl_ptr, mem_ptr, mem_size)。
/// user → PM。
///
/// C: `mess_lc_pm_sprof` — ipc.h:550-558(LP64 六域:action@0/freq@4/
/// intr_type@8/ctl_ptr@16/mem_ptr@24/mem_size@32)。
pub(crate) fn sprof(msg: &Message) -> (i32, i32, i32, u64, u64, u64) {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路。
    let raw = unsafe { msg.m_u.raw };
    let i32_at = |o: usize| i32::from_le_bytes(raw[o..o + 4].try_into().unwrap());
    let u64_at = |o: usize| u64::from_le_bytes(raw[o..o + 8].try_into().unwrap());
    (
        i32_at(0),
        i32_at(4),
        i32_at(8),
        u64_at(16),
        u64_at(24),
        u64_at(32),
    )
}

/// getsysinfo 参数 (what, where, size)。RS → PM。
///
/// C: `mess_lsys_getsysinfo` — ipc.h:1062-1070(what@0/where@8/size@16)。
pub(crate) fn getsysinfo(msg: &Message) -> (i32, u64, u64) {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路。
    let raw = unsafe { msg.m_u.raw };
    let what = i32::from_le_bytes(raw[0..4].try_into().unwrap());
    let where_ = u64::from_le_bytes(raw[8..16].try_into().unwrap());
    let size = u64::from_le_bytes(raw[16..24].try_into().unwrap());
    (what, where_, size)
}

/// mcontext 参数 (endpt, ctx 用户态指针)。user → PM。
///
/// C: `mess_lc_pm_mcontext`(minix-types `MessLcPmMcontext` —
/// endpt@0/ctx@8;MessageUnion 无专属臂,raw 字节读)。
pub(crate) fn mcontext(msg: &Message) -> (i32, u64) {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路;endpt@0/ctx@8。
    let raw = unsafe { msg.m_u.raw };
    let endpt = i32::from_le_bytes(raw[0..4].try_into().unwrap());
    let ctx = u64::from_le_bytes(raw[8..16].try_into().unwrap());
    (endpt, ctx)
}

/// getprocnr 参数 (pid)。RS → PM。
///
/// C: `mess_lc_pm_getprocnr`(pid@0,raw 字节读,无专属臂)。
pub(crate) fn getprocnr(msg: &Message) -> i32 {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路;pid@0。
    let raw = unsafe { msg.m_u.raw };
    i32::from_le_bytes(raw[0..4].try_into().unwrap())
}

/// getepinfo 参数 (endpt, caller 组缓冲指针, caller 组容量)。RS → PM。
///
/// C: `mess_lsys_pm_getepinfo` — ipc.h:503-510(endpt@0/groups@8/
/// ngroups@16,LP64)。
pub(crate) fn getepinfo(msg: &Message) -> (i32, u64, i32) {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路;endpt@0/
    // groups@8/ngroups@16。
    let raw = unsafe { msg.m_u.raw };
    let endpt = i32::from_le_bytes(raw[0..4].try_into().unwrap());
    let groups = u64::from_le_bytes(raw[8..16].try_into().unwrap());
    let ngroups = i32::from_le_bytes(raw[16..20].try_into().unwrap());
    (endpt, groups, ngroups)
}

/// 时间族参数 (clk_id, now, sec, nsec)。user → PM。
///
/// C: `mess_lc_pm_time`(minix-types `MessLcPmTime` — sec@0/clk_id@8/
/// now@12/nsec@16;stime 只用 sec,clock 族用 clk_id/now/sec/nsec)。
pub(crate) fn time(msg: &Message) -> (i32, bool, u64, i64) {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路;sec@0(u64)/
    // clk_id@8(i32)/now@12(i32)/nsec@16(i64),raw 字节读(MessageUnion
    // 无专属臂)。
    let raw = unsafe { msg.m_u.raw };
    let clk_id = i32::from_le_bytes(raw[8..12].try_into().unwrap());
    let now = i32::from_le_bytes(raw[12..16].try_into().unwrap());
    let sec = u64::from_le_bytes(raw[0..8].try_into().unwrap());
    let nsec = i64::from_le_bytes(raw[16..24].try_into().unwrap());
    (clk_id, now != 0, sec, nsec)
}

/// setuid/seteuid/setgid/setegid 族参数 (id)。user → PM。
///
/// C: `mess_lc_pm_setid`（minix-types `MessLcPmSetid`，ipc.h:528-533:
/// id u32@0）。四调用共用同一 wire。
pub(crate) fn setid(msg: &Message) -> u32 {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路;raw@0 = id(u32)。
    let raw = unsafe { msg.m_u.raw };
    u32::from_le_bytes(raw[0..4].try_into().unwrap())
}

/// groups 参数 (num, gid 数组指针)。user → PM。
///
/// C: `mess_lc_pm_groups` — ipc.h:459-465（num@0/ptr@8，LP64 pad@4）。
pub(crate) fn groups(msg: &Message) -> (i32, u64) {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路;num@0(i32)、
    // ptr@8(u64,LP64 对齐)。
    let raw = unsafe { msg.m_u.raw };
    let num = i32::from_le_bytes(raw[0..4].try_into().unwrap());
    let ptr = u64::from_le_bytes(raw[8..16].try_into().unwrap());
    (num, ptr)
}

/// getsid 参数 (pid;0 = 自身)。user → PM。
///
/// C: `mess_lc_pm_getsid`（minix-types `MessLcPmGetsid`，pid@0）。
pub(crate) fn getsid(msg: &Message) -> i32 {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路;raw@0 = pid(i32)。
    let raw = unsafe { msg.m_u.raw };
    i32::from_le_bytes(raw[0..4].try_into().unwrap())
}

/// itimer 参数 (which, value 指针, ovalue 指针)。user → PM。
///
/// C: `mess_lc_pm_itimer`（minix-types `MessLcPmItimer`，repr(C) 域序
/// which@0/value@8/ovalue@16；value/ovalue 是 `struct itimerval` 的
/// 用户态指针，字节搬运经网关 copy 缝）。MessageUnion 无专属臂——
/// 按字节读前 24 字节（vm.rs wrapper 的 raw 读取先例）。
/// sigaction 参数 (nr, act 指针, oact 指针, sigreturn 桩)。user → PM。
///
/// C: `mess_lc_pm_sig`（minix-types `MessLcPmSig`——nr@4? 域序
/// pid/nr/act/oact/ret;PM 只消费 nr 起的三域 + ret）。act/oact 是
/// `struct sigaction` 用户态指针,字节搬运经网关 copy 缝。
pub(crate) fn sigaction(msg: &Message) -> (i32, u64, u64, u64) {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路，Copy 按值读。
    let pl = unsafe { msg.m_u.m_lc_pm_sig };
    (pl.nr, pl.act, pl.oact, pl.ret)
}

/// 信号集族参数 (how, ctx 桩, set 掩码)。user → PM。
///
/// C: `mess_lc_pm_sigset`——how@0/ctx@8/set@16(`sigset_t` = 4×u32,
/// sigtypes.h:57-62)。Rust `SigSet` 是 u64:取低两个 u32 拼 u64
/// (信号号 >64 的位在 C 侧即内核已裁决的 no-op 半,见 proc_table.rs)。
pub(crate) fn sigset(msg: &Message) -> (i32, u64, u64) {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路，Copy 按值读。
    let pl = unsafe { msg.m_u.m_lc_pm_sigset };
    let set = (pl.set[0] as u64) | ((pl.set[1] as u64) << 32);
    (pl.how, pl.ctx, set)
}

/// getrusage 参数 (who, addr)。user → PM。
///
/// C: `mess_lc_pm_rusage` — ipc.h:510-515（who@endpoint_t@0/addr@8，
/// raw 字节读，无专属 union 臂）；`who` 取 `RUSAGE_SELF(0)/
/// RUSAGE_CHILDREN(-1)`（sys/resource.h:54-55），`addr` 是用户态
/// `struct rusage*` 的 VirBytes 视图（do_getrusage 最终
/// sys_datacopy 的目的地，misc.c:446-447）。
pub(crate) fn rusage(msg: &Message) -> (i32, u64) {
    // SAFETY: 同 srv_fork——dispatch 已按 m_type 选路;who@0/addr@8,
    // 按字节读取无类型重解释。
    let raw = unsafe { msg.m_u.raw };
    let who = i32::from_le_bytes(raw[0..4].try_into().unwrap());
    let addr = u64::from_le_bytes(raw[8..16].try_into().unwrap());
    (who, addr)
}

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
    fn rusage_decodes_who_and_addr_roundtrip() {
        // ipc.h:510-515 布局见证:who@endpoint_t@0(-1 = RUSAGE_CHILDREN
        // 走负数通路),addr@8。写入端按同域序组包,读回一致。
        for who in [0i32, -1i32, 7i32] {
            let m = msg_with(|m| {
                // SAFETY: 测试构造——按解码域序写 raw 字节。
                let raw = unsafe { &mut m.m_u.raw };
                raw[0..4].copy_from_slice(&who.to_le_bytes());
                raw[8..16].copy_from_slice(&0x2000u64.to_le_bytes());
            });
            assert_eq!(rusage(&m), (who, 0x2000));
        }
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
    fn sigaction_decodes_nr_and_pointers() {
        let m = msg_with(|m| {
            m.m_u.m_lc_pm_sig.nr = 9;
            m.m_u.m_lc_pm_sig.act = 0x5000;
            m.m_u.m_lc_pm_sig.oact = 0x6000;
            m.m_u.m_lc_pm_sig.ret = 0x7000;
        });
        assert_eq!(sigaction(&m), (9, 0x5000, 0x6000, 0x7000));
    }

    #[test]
    fn sigset_decodes_how_ctx_and_mask_low64() {
        let m = msg_with(|m| {
            m.m_u.m_lc_pm_sigset.how = 2;
            m.m_u.m_lc_pm_sigset.ctx = 0x4000;
            m.m_u.m_lc_pm_sigset.set = [0x11, 0x22, 0x33, 0x44];
        });
        // 高 64 位(信号号 >64,C 侧 no-op 半)被丢弃。
        assert_eq!(sigset(&m), (2, 0x4000, 0x22_0000_0011));
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
