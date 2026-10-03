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

// ── NK4-C 续-294 a2d-leaf 探针 sink（riscv-only；结案随本模块一起滚除）────
// arch walk_read 叶/大页分支抓到的五元组在这里格式化并经 SYS_DIAGCTL 上台
// （消息 <DIAGBUFSIZE=128；fail-silent 同上）。

/// 注册进 arch 探针（须先于任何子进程 walk；对齐 pt_alloc::register
/// 的调用时机，见 vm_server.rs 注册块）。
#[cfg(all(target_arch = "riscv64", not(test)))]
pub(crate) fn register_a2d_sink() {
    minix_arch::riscv64::paging::a2d::register_sink(a2d_leaf_sink);
}

#[cfg(all(target_arch = "riscv64", not(test)))]
fn a2d_leaf_sink(walked_root: u64, level: u8, idx: usize, raw: u64, leaf_pa: u64) {
    mark(&alloc::format!(
        "nk4c: a2dlf root={walked_root:#x} lvl={level} idx={idx:#x} raw={raw:#x} pa={leaf_pa:#x}\n"
    ));
}
