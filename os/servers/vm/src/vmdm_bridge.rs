//! VM 页表遍历的内核代读旁路（NK4-C 续-311，riscv 专用）。
//!
//! walk_read 的 VmDm 槽读/写/清零改经 SYS_VMCTL 内核调用由内核 KDM 直达，
//! 绕开 QEMU 平移层对 DM 窗虚拟地址的时序性误导（§续-291..310 穷举收口）。
//! 原与启动路标探针同居 bootmark 模块，T8 探针滚除时拆出独立成模块——
//! 路标族已随结案清除，本机制是生产功能，去留裁决=保留。

// ── NK4-C 续-311 内核代读旁路 read-fn（riscv-only 生产机制）──
// walk_read 的 VmDm 槽读改经本 fn：SYS_VMCTL(43) + VmCtlParam::PteRead(20)，
// pa 走 m1p1，内核 KDM 直读回填 m1p1。绕开 QEMU 平移层对 DM 窗 VA 的
// 时序性误导（§续-291..310 穷举收口）。

/// 注册进 arch 钩子（时机对齐 pt_alloc::register——先于任何 walk）。
#[cfg(all(target_arch = "riscv64", not(test)))]
pub(crate) fn register_vmdm_read() {
    minix_arch::riscv64::paging::vmdm::register_read(vmdm_read_via_kernel);
    minix_arch::riscv64::paging::vmdm::register_write(vmdm_write_via_kernel);
    minix_arch::riscv64::paging::vmdm::register_zero(vmdm_zero_via_kernel);
}

#[cfg(all(target_arch = "riscv64", not(test)))]
fn vmdm_write_via_kernel(paddr: u64, val: u64) {
    let mut msg = minix_types::Message::default();
    {
        // SAFETY: SYS_VMCTL wire——m1i1=VM、m1i2=21(PteWrite)、m1p1=pa、
        // m1p2=val；内核 KDM 直写+sfence。
        let m1 = unsafe { &mut msg.m_u.m_m1 };
        m1.m1i1 = minix_types::Endpoint::VM.0;
        m1.m1i2 = 21;
        m1.m1p1 = paddr;
        m1.m1p2 = val;
    }
    let _ = minix_sys::syscall::perform_kernel_call(
        &minix_sys::syscall::DirectKernelCallTransport,
        crate::kernel_gateway::SYS_VMCTL_CALL,
        &mut msg,
        |_| {},
    );
}

#[cfg(all(target_arch = "riscv64", not(test)))]
fn vmdm_zero_via_kernel(paddr: u64) {
    let mut msg = minix_types::Message::default();
    {
        // SAFETY: SYS_VMCTL wire——m1i1=VM、m1i2=22(PteZero)、m1p1=pa；
        // 内核 KDM 写 4096 零+sfence。
        let m1 = unsafe { &mut msg.m_u.m_m1 };
        m1.m1i1 = minix_types::Endpoint::VM.0;
        m1.m1i2 = 22;
        m1.m1p1 = paddr;
    }
    let _ = minix_sys::syscall::perform_kernel_call(
        &minix_sys::syscall::DirectKernelCallTransport,
        crate::kernel_gateway::SYS_VMCTL_CALL,
        &mut msg,
        |_| {},
    );
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
