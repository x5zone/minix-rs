//! Boot-time architecture identity.
//!
//! `BootArch` binds the three mechanism families the boot page-size policy
//! consumes — page-table services, the Direct Map window constants, and the
//! bootstrap-root leaf installer — into one named architecture instance.
//! Policies carry `PhantomData<A>` so a policy produced for one architecture
//! cannot be executed against another's mechanism implementations.
//!
//! This is deliberately narrower than [`crate::arch::current::Arch`]: the
//! aggregate `Arch` describes every runtime mechanism family, while boot
//! policy only needs these three. The concrete boot bundles below reuse the
//! same implementors the `Current*` aliases select, so there is still one
//! implementation per mechanism.

use crate::arch::direct_map::DirectMapArch;
use crate::arch::boot_dm::BootDmInstaller;
use crate::paging_ext::HugePages;

/// Architecture identity for boot page-size planning and DM installation.
pub trait BootArch: Sized {
    /// Page-table service and hardware huge-page capabilities.
    type Paging: HugePages;
    /// Direct Map window constants and PA↔VA arithmetic.
    type DirectMap: DirectMapArch;
    /// Bootstrap-root leaf installer (boot-time table surgery).
    type DmInstaller: BootDmInstaller;
}

#[cfg(target_arch = "x86_64")]
pub struct X86_64BootArch;

#[cfg(target_arch = "x86_64")]
impl BootArch for X86_64BootArch {
    type Paging = crate::x86_64::paging::X86_64Paging;
    type DirectMap = crate::arch::direct_map::X86_64DirectMap;
    type DmInstaller = crate::x86_64::paging::X86_64BootDmInstaller;
}

#[cfg(target_arch = "aarch64")]
pub struct AArch64BootArch;

#[cfg(target_arch = "aarch64")]
impl BootArch for AArch64BootArch {
    type Paging = crate::arm64::paging::AArch64Paging;
    type DirectMap = crate::arch::direct_map::AArch64DirectMap;
    type DmInstaller = crate::arm64::paging::AArch64BootDmInstaller;
}

#[cfg(target_arch = "riscv64")]
pub struct Riscv64BootArch;

#[cfg(target_arch = "riscv64")]
impl BootArch for Riscv64BootArch {
    type Paging = crate::riscv64::paging::Riscv64Paging;
    type DirectMap = crate::arch::direct_map::Riscv64DirectMap;
    type DmInstaller = crate::riscv64::paging::Riscv64BootDmInstaller;
}

/// Host-test bundle: mock paging and DM installer with the mock Direct Map
/// layout, mirroring what the `Current*` aliases select under
/// `runtime-window`.
#[cfg(feature = "runtime-window")]
pub struct MockBootArch;

#[cfg(feature = "runtime-window")]
impl BootArch for MockBootArch {
    type Paging = crate::arch::paging::mock::MockPaging;
    type DirectMap = crate::arch::direct_map::MockDirectMap;
    type DmInstaller = crate::arch::boot_dm::mock::MockBootDmInstaller;
}

/// Boot-architecture instance for the current build target.
#[cfg(feature = "runtime-window")]
pub type CurrentBootArch = MockBootArch;
#[cfg(all(not(feature = "runtime-window"), target_arch = "x86_64"))]
pub type CurrentBootArch = X86_64BootArch;
#[cfg(all(not(feature = "runtime-window"), target_arch = "aarch64"))]
pub type CurrentBootArch = AArch64BootArch;
#[cfg(all(not(feature = "runtime-window"), target_arch = "riscv64"))]
pub type CurrentBootArch = Riscv64BootArch;
