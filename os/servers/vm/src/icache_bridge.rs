//! Instruction-cache synchronization for pages the VM loader wrote itself
//! (SD-20, `[ARCH: cache-sync]`).
//!
//! Why this bridge exists: the kernel's ELF loaders synchronize the
//! instruction stream at their tail (`os/arch/src/arch/boot.rs`, the
//! `CurrentCacheSyncArch` call), but the **production boot loader is this
//! server**: `exec_bootproc` copies every boot process's image through the
//! VM Direct Map and maps the frames into the new root. Those stores sit in
//! the data side; on aarch64 the I-cache is not coherent with them, and on
//! riscv64 `FENCE.I` is required before the first fetch — QEMU never shows
//! the fault, so no gate can catch a missing sync (the register calls this
//! the highest-certainty real-hardware time bomb).
//!
//! Shape: the VM reports one freshly written **physical** page per call
//! (`SYS_VMCTL` sub-command `PteSyncIcache`, wire 23); the kernel applies the
//! same H5 gate as the KDM arms (caller must be VM, page must be physical
//! RAM) and runs the architecture sequence over its own Direct Map view of
//! the same physical lines (aarch64 `dc cvau` + `ic ivau`, riscv64 `fence.i`
//! + remote broadcast, x86_64 a serializing no-op). One call per executable
//! page at exec time; x86_64 and host builds compile the call out (nothing
//! to synchronize / no kernel to ask).

/// Synchronize the instruction stream for one executable page the loader just
/// wrote. `paddr` is the page's physical base (page-aligned by construction).
#[cfg(all(any(target_arch = "aarch64", target_arch = "riscv64"), not(test)))]
pub(crate) fn sync_written_page(paddr: u64) {
    const PAGE_SIZE: u64 = crate::region::PAGE_SIZE;
    let mut msg = minix_types::Message::default();
    {
        // SAFETY: SYS_VMCTL wire — m1i1 = who (VM), m1i2 = 23
        // (PteSyncIcache), m1p1 = page PA, m1p2 = whole page; the kernel
        // gates caller and RAM bound before touching the caches.
        let m1 = unsafe { &mut msg.m_u.m_m1 };
        m1.m1i1 = minix_types::Endpoint::VM.0;
        m1.m1i2 = 23;
        m1.m1p1 = paddr;
        m1.m1p2 = PAGE_SIZE;
    }
    let _ = minix_sys::syscall::perform_kernel_call(
        &minix_sys::syscall::DirectKernelCallTransport,
        crate::kernel_gateway::SYS_VMCTL_CALL,
        &mut msg,
        |_| {},
    );
}

/// x86_64 and test builds: I and D caches are coherent (or there is no kernel
/// to ask), so the loader has nothing to call.
#[cfg(not(all(any(target_arch = "aarch64", target_arch = "riscv64"), not(test))))]
pub(crate) fn sync_written_page(_paddr: u64) {}
