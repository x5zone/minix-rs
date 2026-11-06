//! Boot-time page-size policy.
//!
//! Two layers are deliberately separate:
//!
//! - capability facts (`BootPageSupport<A>`, [`minix_arch::paging_ext`])
//!   answer what the platform supports;
//! - policies here and in [`minix_arch::arch::boot_dm`] record what this
//!   boot chose, together with the layout that choice was validated against.
//!
//! Planning is pure: no hardware query, no page-table access. The execution
//! paths take the policy's stored layout, never a second copy assembled at
//! the call site.

use minix_arch::arch::boot_arch::BootArch;
use minix_arch::arch::boot_dm::{DmPagePolicy, DmPolicyError, plan_dm_policy};
use minix_arch::arch::direct_map::DirectMapLayout;
use minix_arch::paging::Paging;
use minix_arch::paging_ext::{BootPageSupport, PageSize, PageSupportError};

/// Kernel image layout used as planning input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KernelLayout {
    pub virtual_base: u64,
    pub physical_base: u64,
    pub size: u64,
}

/// Kernel mapping planning errors; this layer answers only kernel-layout
/// questions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelMapError {
    /// The capability snapshot's static configuration is invalid.
    InvalidSupport(PageSupportError),
    /// Layout arithmetic overflowed, the VA interval is illegal, or the
    /// physical end exceeds the platform bound.
    InvalidKernelLayout,
    /// The fallback size is required but unsupported.
    FallbackUnsupported(PageSize),
    /// Neither preferred nor fallback satisfies the layout.
    NoKernelPageSize,
}

impl core::fmt::Display for KernelMapError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidSupport(e) => write!(f, "invalid page-size support: {e}"),
            Self::InvalidKernelLayout => write!(f, "invalid kernel layout"),
            Self::FallbackUnsupported(size) => {
                write!(f, "fallback page size {} is not supported", size.bytes())
            }
            Self::NoKernelPageSize => write!(f, "no kernel page size satisfies the layout"),
        }
    }
}

/// Validated kernel mapping decision.
pub struct KernelMapPolicy<A: BootArch> {
    layout: KernelLayout,
    page_size: PageSize,
    _arch: core::marker::PhantomData<fn() -> A>,
}

impl<A: BootArch> Copy for KernelMapPolicy<A> {}

impl<A: BootArch> Clone for KernelMapPolicy<A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A: BootArch> core::fmt::Debug for KernelMapPolicy<A> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("KernelMapPolicy")
            .field("layout", &self.layout)
            .field("page_size", &self.page_size)
            .finish()
    }
}

impl<A: BootArch> KernelMapPolicy<A> {
    pub const fn layout(&self) -> KernelLayout {
        self.layout
    }

    pub const fn page_size(&self) -> PageSize {
        self.page_size
    }
}

fn layout_satisfies(size: PageSize, layout: KernelLayout) -> bool {
    let bytes = size.bytes();
    layout.virtual_base.is_multiple_of(bytes)
        && layout.physical_base.is_multiple_of(bytes)
        && layout.size.is_multiple_of(bytes)
}

/// Choose the kernel mapping page size: capability first, then layout.
pub fn plan_kernel_mapping<A: BootArch>(
    support: &BootPageSupport<A>,
    layout: KernelLayout,
) -> Result<KernelMapPolicy<A>, KernelMapError> {
    support.validate().map_err(KernelMapError::InvalidSupport)?;

    if layout.size == 0 {
        return Err(KernelMapError::InvalidKernelLayout);
    }
    let virtual_end = layout
        .virtual_base
        .checked_add(layout.size)
        .ok_or(KernelMapError::InvalidKernelLayout)?;
    let physical_end = layout
        .physical_base
        .checked_add(layout.size)
        .ok_or(KernelMapError::InvalidKernelLayout)?;
    if A::Paging::validate_virtual_range(layout.virtual_base, layout.size).is_err()
        || physical_end > support.phys_addr_limit()
    {
        return Err(KernelMapError::InvalidKernelLayout);
    }
    let _ = virtual_end;

    let preferred = support.preferred();
    let fallback = support.fallback();

    let page_size = if preferred.supported() && layout_satisfies(preferred.size(), layout) {
        preferred.size()
    } else if fallback.supported() && layout_satisfies(fallback.size(), layout) {
        fallback.size()
    } else if !fallback.supported() {
        return Err(KernelMapError::FallbackUnsupported(fallback.size()));
    } else {
        return Err(KernelMapError::NoKernelPageSize);
    };

    Ok(KernelMapPolicy {
        layout,
        page_size,
        _arch: core::marker::PhantomData,
    })
}

/// Combined boot policy: one kernel mapping plus one Direct Map decision.
pub struct BootPagePolicy<A: BootArch> {
    kernel: KernelMapPolicy<A>,
    dm: DmPagePolicy<A>,
}

impl<A: BootArch> Copy for BootPagePolicy<A> {}

impl<A: BootArch> Clone for BootPagePolicy<A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A: BootArch> core::fmt::Debug for BootPagePolicy<A> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BootPagePolicy")
            .field("kernel", &self.kernel)
            .field("dm", &self.dm)
            .finish()
    }
}

impl<A: BootArch> BootPagePolicy<A> {
    pub const fn kernel(&self) -> KernelMapPolicy<A> {
        self.kernel
    }

    pub const fn dm(&self) -> DmPagePolicy<A> {
        self.dm
    }
}

/// Combined planning error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootPagePolicyError {
    Support(PageSupportError),
    Kernel(KernelMapError),
    Dm(DmPolicyError),
}

impl core::fmt::Display for BootPagePolicyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Support(e) => write!(f, "invalid boot page support: {e}"),
            Self::Kernel(e) => write!(f, "kernel mapping policy: {e}"),
            Self::Dm(e) => write!(f, "direct-map policy: {e}"),
        }
    }
}

impl From<KernelMapError> for BootPagePolicyError {
    fn from(value: KernelMapError) -> Self {
        Self::Kernel(value)
    }
}

impl From<DmPolicyError> for BootPagePolicyError {
    fn from(value: DmPolicyError) -> Self {
        Self::Dm(value)
    }
}

/// Plan both consumers of the capability facts in one place.
pub fn plan_boot_pages<A: BootArch>(
    support: &BootPageSupport<A>,
    kernel_layout: KernelLayout,
    dm_layout: &DirectMapLayout<A>,
) -> Result<BootPagePolicy<A>, BootPagePolicyError> {
    support.validate().map_err(BootPagePolicyError::Support)?;
    let kernel = plan_kernel_mapping::<A>(support, kernel_layout)?;
    let dm = plan_dm_policy::<A>(support, dm_layout)?;
    Ok(BootPagePolicy { kernel, dm })
}

#[cfg(all(test, feature = "mock"))]
mod policy_tests {
    use super::*;
    use minix_arch::DirectMapArch;
    use minix_arch::arch::boot_arch::MockBootArch;
    use minix_arch::arch::direct_map::MockDirectMap;
    use minix_arch::paging_ext::{BootPageSupport, PageSupport};

    fn support(preferred: bool, fallback: bool, phys_limit: u64) -> BootPageSupport<MockBootArch> {
        BootPageSupport::<MockBootArch>::from_parts(
            PageSize::KIB_4,
            phys_limit,
            PageSupport::new(PageSize::GIB_1, preferred),
            PageSupport::new(PageSize::MIB_2, fallback),
        )
    }

    fn layout(virtual_base: u64, physical_base: u64, size: u64) -> KernelLayout {
        KernelLayout {
            virtual_base,
            physical_base,
            size,
        }
    }

    fn dm_layout() -> DirectMapLayout<MockBootArch> {
        DirectMapLayout::<MockBootArch>::from_arch()
    }

    #[test]
    fn picks_the_preferred_size_when_supported_and_aligned() {
        let policy = plan_kernel_mapping::<MockBootArch>(
            &support(true, true, 1 << 52),
            layout(0xFFFF_8000_0000_0000, 0x4000_0000, 0x4000_0000),
        )
        .unwrap();
        assert_eq!(policy.page_size(), PageSize::GIB_1);
    }

    #[test]
    fn falls_back_when_the_preferred_size_is_unsupported() {
        // The regression this layer pins: an aligned layout on a CPU without
        // 1 GiB pages must choose 2 MiB, not fail and not choose 1 GiB.
        let policy = plan_kernel_mapping::<MockBootArch>(
            &support(false, true, 1 << 52),
            layout(0xFFFF_8000_0000_0000, 0x4000_0000, 0x4000_0000),
        )
        .unwrap();
        assert_eq!(policy.page_size(), PageSize::MIB_2);
    }

    #[test]
    fn fails_when_the_required_fallback_is_unsupported() {
        let err = plan_kernel_mapping::<MockBootArch>(
            &support(false, false, 1 << 52),
            layout(0xFFFF_8000_0000_0000, 0x200_000, 0x200_000),
        )
        .unwrap_err();
        assert_eq!(err, KernelMapError::FallbackUnsupported(PageSize::MIB_2));
    }

    #[test]
    fn physical_end_above_the_platform_bound_is_rejected() {
        let err = plan_kernel_mapping::<MockBootArch>(
            &support(true, true, 0x1000),
            layout(0xFFFF_8000_0000_0000, 0x200_000, 0x200_000),
        )
        .unwrap_err();
        assert_eq!(err, KernelMapError::InvalidKernelLayout);
    }

    #[test]
    fn boot_policy_uses_one_layout_for_both_consumers() {
        let dm = dm_layout();
        let policy = plan_boot_pages::<MockBootArch>(
            &support(true, true, 1 << 52),
            layout(0xFFFF_8000_0000_0000, 0x4000_0000, 0x4000_0000),
            &dm,
        )
        .unwrap();
        assert_eq!(policy.kernel().page_size(), PageSize::GIB_1);
        assert_eq!(
            policy.dm().kernel_window().va_base(),
            MockDirectMap::KERNEL_DIRECT_MAP_BASE
        );
        assert_eq!(
            policy.dm().vm_window().va_base(),
            MockDirectMap::VM_DIRECT_MAP_BASE
        );
        assert_eq!(policy.dm().layout().kernel_base(), dm.kernel_base());
    }
}
