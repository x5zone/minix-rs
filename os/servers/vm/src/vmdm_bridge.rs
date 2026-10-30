//! Kernel-proxied read bypass for VM page-table walks (riscv only).
//!
//! walk_read's reads, writes, and zeroing of VmDm slots go through the
//! SYS_VMCTL kernel call and reach the kernel KDM directly, bypassing the
//! QEMU translation layer's timing-dependent misdirection of DM-window
//! virtual addresses.
//! This used to share the bootmark module with the boot route-marker probes;
//! it was split out into its own module when those probes were rolled out.
//! The marker family is gone, but this mechanism is production functionality
//! and is kept.

// ── Kernel-proxied read bypass read-fn (riscv-only production mechanism) ──
// walk_read's VmDm slot reads go through this fn: SYS_VMCTL(43) +
// VmCtlParam::PteRead(20); pa travels in m1p1 and the kernel KDM reads it
// directly and writes it back into m1p1. This bypasses the QEMU translation
// layer's timing-dependent misdirection of DM-window VAs.

/// Register into the arch hooks (timing aligned with pt_alloc::register —
/// before any walk).
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
        // SAFETY: SYS_VMCTL wire — m1i1=VM, m1i2=21 (PteWrite), m1p1=pa,
        // m1p2=val; the kernel KDM writes directly and then issues sfence.
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
        // SAFETY: SYS_VMCTL wire — m1i1=VM, m1i2=22 (PteZero), m1p1=pa;
        // the kernel KDM writes 4096 zero bytes and then issues sfence.
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
        // SAFETY: SYS_VMCTL wire — m1i1=VM, m1i2=20 (PteRead), m1p1=pa;
        // the kernel's PteRead arm reads through KDM and writes the value
        // back into m1p1 (kernel/src/syscall.rs).
        let m1 = unsafe { &mut msg.m_u.m_m1 };
        m1.m1i1 = minix_types::Endpoint::VM.0;
        m1.m1i2 = 20; // wire number of VmCtlParam::PteRead (kernel/src/vm.rs TryFrom)
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
