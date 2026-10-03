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

// ── NK4-C 续-311 内核代读旁路 read-fn（riscv-only；随 (A) 结案裁决去留）──
// walk_read 的 VmDm 槽读改经本 fn：SYS_VMCTL(43) + VmCtlParam::PteRead(20)，
// pa 走 m1p1，内核 KDM 直读回填 m1p1。绕开 QEMU 平移层对 DM 窗 VA 的
// 时序性误导（§续-291..310 穷举收口）。

/// 注册进 arch 钩子（时机对齐 pt_alloc::register——先于任何 walk）。
#[cfg(all(target_arch = "riscv64", not(test)))]
pub(crate) fn register_vmdm_read() {
    minix_arch::riscv64::paging::vmdm::register_read(vmdm_read_via_kernel);
}

#[cfg(all(target_arch = "riscv64", not(test)))]
fn vmdm_read_via_kernel(paddr: u64) -> u64 {
    let mut msg = minix_types::Message::default();
    {
        // SAFETY: SYS_VMCTL wire——m1i1=VM、m1i2=20(PteRead)、m1p1=pa；
        // 内核 PteRead 臂 KDM 直读后回填 m1p1（kernel/src/syscall.rs）。
        let m1 = unsafe { &mut msg.m_u.m_m1 };
        m1.m1i1 = minix_types::Endpoint::VM.0;
        m1.m1i2 = 20; // VmCtlParam::PteRead wire 号（kernel/src/vm.rs TryFrom）
        m1.m1p1 = paddr;
    }
    let reply = minix_sys::syscall::perform_kernel_call(
        &minix_sys::syscall::DirectKernelCallTransport,
        crate::kernel_gateway::SYS_VMCTL_CALL,
        &mut msg,
        |_| {},
    );
    if reply < 0 {
        return 0;
    }
    unsafe { msg.m_u.m_m1.m1p1 as u64 }
}
