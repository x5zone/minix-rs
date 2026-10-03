//! NK4-A 临时启动路标（fix22 二分取证）。
//!
//! VM 在真机上 `scheduling live` 之后完全静默——panic 通道（minix-rt
//! panic-handler → SYS_DIAGCTL）没有触发，说明 VM 不是死亡而是**卡在
//! 某阶段内部**。SYS_DIAGCTL 是已验证可达的串口腿（fix20c 起 panic
//! 消息直达串口），本模块复用它把 init/run 时间线切成可观测的段。
//!
//! 所有消息统一 `nk4a:` 前缀；task1-close 时按 HANDOFF 做去留裁决
//! （code-excellence 死代码消除：定位完成后整模块删除，不留残余）。

/// 向内核诊断通道发一条路标消息（fail-silent：通道本身故障时
/// 卡点信息由后续观察决定，不在诊断代码里再叠一层故障处理）。
pub fn mark(msg: &str) {
    #[cfg(not(test))]
    {
        let _ = minix_sys::syscall::sys_diagctl_write(
            &minix_sys::syscall::DirectKernelCallTransport,
            msg,
        );
    }
    #[cfg(test)]
    {
        let _ = msg;
    }
}
