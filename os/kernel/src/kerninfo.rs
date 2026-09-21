//! Kernel information page — build, user-map, and publish (`MINIX_KERNINFO`).
//!
//! C ground truth: the `MINIX_KERNINFO` IPC call (= 6, `ipcconst.h:12`)
//! hands the caller a *user-readable* kernel information page address
//! through the secondary IPC return channel (`minix3/minix/kernel/proc.c:685-693`).
//! In C the page is the `.usermapped`-section `struct minix_kerninfo`
//! (`usermapped_data.c:4`); the kernel fixes up its interior pointers to
//! the user-visible mapping and publishes the address in
//! `minix_kerninfo_user` the first time the section gets its user mapping
//! (`arch/i386/memory.c:874-925`). Until that happens the call returns
//! EBADCALL ("It might not be initialized yet", proc.c:687-689).
//!
//! This rewrite does not port the `.usermapped` section (design doc
//! `01-stage-kernel/28-usermapped-data.md` D1): no user-space IPC
//! trampolines, and the non-ABI sub-structures (`kinfo`, `machine`,
//! `kmessages`, `loadinfo`, `kclockinfo`) stay unreachable through this
//! page — their slots hold 0 and the matching `ki_flags` bits stay clear,
//! which is how C treats not-yet-initialized content. What IS preserved is
//! the call's observable contract: a page whose `kerninfo_magic` validates
//! and whose `kuserinfo` sub-structure is present (the only userland-ABI
//! field pair besides the magic). See `minix_types::types::kerninfo` for
//! the wire layout and the `MINIX_KIF_*` semantics.
//!
//! Mapping responsibility differs from C by bootstrap stage, not by
//! contract. C's VM maps the section into every process and tells the
//! kernel where; this kernel bootstrap has no VM yet, so
//! [`init_kerninfo`] maps one reserved page itself into the active
//! bootstrap root at a fixed user VA — the same shape as the `VmBootHandoff`
//! page (`vm_handoff`, lib.rs `init_proc_and_boot`). When VM takes over
//! per-process address spaces, replicating this mapping into each new root
//! moves to VM together with the rest of the user PTE work; the publish
//! mechanism (this module + the dispatch arm reading
//! `MINIX_KERNINFO_USER`) is unchanged by that transition.

use core::mem::size_of;
use core::sync::atomic::Ordering;

use minix_arch::paging::PageFlags;
use minix_arch::{CurrentPaging, paging::Paging as _};
use minix_boot::KernelInfo;
use minix_types::{KuserInfo, MinixKerninfo, KERNINFO_MAGIC, MINIX_KIF_USERINFO, VirBytes};

use crate::globals::{SyncUnsafeCell, MINIX_KERNINFO_USER};

/// Fixed user-space VA of the kernel information page.
///
/// Placement policy, same constraints as `minix_types::VM_BOOT_HANDOFF_VA`:
/// above the 4 GiB identity window (a VA below it would alias the boot
/// identity mapping instead of getting fresh PTEs), below every supported
/// architecture's user-VA limit (Sv39 caps user VA at 2^38), and far from
/// the handoff page at 0x1_0000_0000 so the two never share a PD entry.
/// Consumers never hardcode this value — they receive it from the
/// `MINIX_KERNINFO` call — so moving it later is a one-line change.
pub const KERNINFO_USER_VA: u64 = 0x2_0000_0000;

/// Offset of the `kuserinfo` sub-structure inside the page.
///
/// C keeps `minix_kerninfo` and `kuserinfo` as separate `.usermapped`
/// symbols; the pointer relation is all that matters, so both live on one
/// page here and the pointer is `KERNINFO_USER_VA + KUSERINFO_OFFSET`.
const KUSERINFO_OFFSET: usize = 0x800;

/// The one-page kernel information image shared with user mode.
///
/// Write-once: filled by [`init_kerninfo`] during boot (BKL held,
/// single-threaded), never mutated afterwards — the `BklProtected`
/// "write-once-read-only after boot" rationale, same audit class as
/// `KernelInfo` itself.
#[repr(C, align(4096))]
pub(crate) struct KerninfoPage {
    /// Top-level structure at offset 0 — the address users receive.
    pub info: MinixKerninfo,
    _pad0: [u8; KUSERINFO_OFFSET - size_of::<MinixKerninfo>()],
    /// `kuserinfo` sub-structure, reached through `info.kuserinfo`.
    pub user: KuserInfo,
    _pad1: [u8; 0x1000 - KUSERINFO_OFFSET - size_of::<KuserInfo>()],
}

impl KerninfoPage {
    /// Page image before boot init: magic stamped, everything else absent.
    pub(crate) const fn new() -> Self {
        Self {
            info: MinixKerninfo::new_uninitialized(),
            _pad0: [0; KUSERINFO_OFFSET - size_of::<MinixKerninfo>()],
            user: KuserInfo {
                kui_size: 0,
                kui_user_sp: 0,
            },
            _pad1: [0; 0x1000 - KUSERINFO_OFFSET - size_of::<KuserInfo>()],
        }
    }
}

static KERNINFO_PAGE: SyncUnsafeCell<KerninfoPage> = SyncUnsafeCell::new(KerninfoPage::new());

/// Fill the page content from boot information.
///
/// C parity: `kuserinfo` is filled at cstart from `kinfo.user_sp`
/// (`main.c:438-440`: memset, `kui_size = sizeof(kuserinfo)`,
/// `kui_user_sp = kinfo.user_sp`); the interior pointer fixups, the magic
/// stamp, and the publication happen together when the user mapping
/// exists (`memory.c:878-925`). Here that order is fill → map → fixups →
/// publish ([`init_kerninfo`]); the kernel-side higher-half alias stays
/// writable, so ordering relative to `map` is a readability choice, not a
/// constraint — users cannot touch the page before the publish anyway,
/// since the VA only escapes through [`MINIX_KERNINFO_USER`].
fn fill_kerninfo_page(kernel_info: &KernelInfo) {
    let page = unsafe { &mut *KERNINFO_PAGE.get() };
    page.user = KuserInfo {
        kui_size: size_of::<KuserInfo>() as u64,
        kui_user_sp: kernel_info.user_sp().0,
    };
}

/// Map the page user read-only into the active bootstrap root and publish
/// its user VA.
///
/// The physical address comes from translating the static's runtime VA
/// through the live root, not from `kern_phys_base + (va - kern_virt_base)`
/// arithmetic: that span identity holds for the higher-half production
/// image, but a UEFI-staged test kernel executes from wherever the firmware
/// loaded it (inside the identity window), where the arithmetic underflows.
/// The active root is the single source of truth for VA→PA in both shapes.
///
/// `PageFlags::read_only()` = PRESENT | USER_ACCESSIBLE (NX implied — the
/// page is data), the same flag set the `VmBootHandoff` page maps with.
/// The fresh VA has no stale TLB entry to invalidate: it was never mapped
/// before, and the bootstrap root is the only root in existence at boot
/// time.
fn map_and_publish() {
    let page_virt = core::ptr::addr_of!(KERNINFO_PAGE) as usize as u64;
    let root = crate::current_root_phys()
        .expect("init_kerninfo: bootstrap root not set — arch_boot_impl must run first");
    let mut paging = CurrentPaging::from_active_root(root);
    let (page_phys, _flags) = paging
        .query(VirBytes(page_virt))
        .expect("init_kerninfo: kerninfo page VA not mapped in the active root");
    paging
        .map(VirBytes(KERNINFO_USER_VA), page_phys, PageFlags::read_only())
        .expect("init_kerninfo: failed to map the kernel info page user read-only");

    // Interior fixups + magic, C memory.c:878-919 order (pointers, then
    // magic as the final content write before publication).
    let page = unsafe { &mut *KERNINFO_PAGE.get() };
    page.info.kuserinfo = KERNINFO_USER_VA + KUSERINFO_OFFSET as u64;
    page.info.ki_flags = MINIX_KIF_USERINFO;
    page.info.kerninfo_magic = KERNINFO_MAGIC;

    // Publication is the release point: from this store on, the
    // MINIX_KERNINFO dispatch arm returns OK with this address instead of
    // EBADCALL (proc.c:687-692). Relaxed matches the dispatch-side load;
    // the store happens once under the boot-time BKL before any user code
    // runs, so no cross-CPU ordering hazard exists at write time.
    MINIX_KERNINFO_USER.store(KERNINFO_USER_VA, Ordering::Relaxed);
}

/// Build, user-map, and publish the kernel information page.
///
/// Boot-order contract: after `arch_boot_impl` (paging live, Direct Map
/// coverage established, `pt_alloc` registered — `map` may allocate the
/// PDP/PD/PT descent for the fresh VA) and before any user process becomes
/// runnable. `kmain` calls this right before `init_proc_and_boot`, mirroring
/// C where the cstart `kuserinfo` fill precedes `proc_init`.
pub fn init_kerninfo(kernel_info: &KernelInfo) {
    fill_kerninfo_page(kernel_info);
    map_and_publish();
}

#[cfg(test)]
mod kerninfo_tests {
    use super::*;
    use core::mem::{offset_of, size_of};
    use minix_types::PhysBytes;

    /// Minimal KernelInfo fixture — same shape as the lib.rs boot tests
    /// (`test_boot_empty_memmap` et al.); only `user_sp` is semantically
    /// load-bearing for the fill contract.
    fn test_kernel_info() -> KernelInfo {
        KernelInfo {
            memmap: &[],
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200_000),
            kern_size: 0x200000,
            free_upper_idx: None,
            user_sp: VirBytes(0x7fff_ffff_f000),
            kern_stack_top: VirBytes(0xFFFF_8000_0040_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: &[],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
            platform_sources: &[],
            param_buf: &[],
            reserved_regions: &[],
        }
    }

    /// Layout witness: one 4 KiB page, `info` at 0, `kuserinfo` at 0x800.
    #[test]
    fn test_kerninfo_page_layout() {
        assert_eq!(size_of::<KerninfoPage>(), 0x1000);
        assert_eq!(offset_of!(KerninfoPage, info), 0);
        assert_eq!(offset_of!(KerninfoPage, user), KUSERINFO_OFFSET);
    }

    /// Content contract: magic + kuserinfo advertised (ki_flags), user_sp
    /// from KernelInfo, everything else absent (C: only USERINFO is set —
    /// no IPCVECS in the 64-bit rewrite, no other sub-structures published).
    #[test]
    fn test_fill_kerninfo_page_content_contract() {
        let info = test_kernel_info();
        fill_kerninfo_page(&info);
        let page = unsafe { &*KERNINFO_PAGE.get() };

        // kuserinfo is filled from boot info (C main.c:438-440)...
        assert_eq!(page.user.kui_size, size_of::<KuserInfo>() as u64);
        assert_eq!(page.user.kui_user_sp, info.user_sp().0);

        // ...while the top-level struct still shows the pre-publish state:
        // magic stamped (const init), but no pointer/flag fixups yet. The
        // publication flip itself is pinned by the dispatch tests in
        // syscall.rs (they store/reset the atomic) and end-to-end by the
        // test-user-trap QEMU run — this test deliberately does not touch
        // that shared atomic, so it stays race-free under parallel test
        // execution.
        assert_eq!(page.info.kerninfo_magic, KERNINFO_MAGIC);
        assert_eq!(page.info.ki_flags, 0);
        assert_eq!(page.info.kuserinfo, 0);
        assert_eq!(page.info.kinfo, 0);
        assert_eq!(page.info.kmessages, 0);
        assert_eq!(page.info.minix_ipcvecs, 0);
    }
}
