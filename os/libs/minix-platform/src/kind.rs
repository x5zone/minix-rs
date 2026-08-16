//! Platform descriptor parse dispatch.
//!
//! Re-exports the kind constants (`DTB`, `RSDP`) from `minix-boot` (where the
//! handoff protocol contract lives) and defines the [`parse_by_kind`]
//! function that dispatches on the kind tag to the appropriate parser.
//!
//! # Layering
//!
//! | Concern | Location |
//! |---------|----------|
//! | Opaque tag type + constant values | `minix_boot::platform` (handoff layer) |
//! | Tag → parser dispatch | this module (`minix_platform::kind`) |
//! | Concrete parsers | `minix_platform::device_tree` / `minix_platform::acpi` |
//!
//! Boot-shim uses `minix_boot::{DTB, RSDP}` to construct sources; the kernel
//! calls `parse_by_kind()` to dispatch. Neither side needs to know about the
//! other's internal structure.
//!
//! # Adding a new firmware table format
//!
//! 1. Add a new `pub const` kind tag in `minix_boot::platform`.
//! 2. Add a match arm in [`parse_by_kind`].
//! 3. Implement the parser in a new module.
//!
//! Upper layers (`KernelInfo`, `boot-shim`, `global::init_from_kinfo`) require
//! **no changes** — they already handle `PlatformDescSource` generically.

pub use minix_boot::platform::{DTB, RSDP};
use minix_boot::{PlatformDescSource, PlatformParseError};

use crate::global::PlatformDescEnum;

/// Parse a platform descriptor source by dispatching on its kind tag.
///
/// # Dispatch table
///
/// | Kind | Parser | Result variant |
/// |------|--------|----------------|
/// | `DTB` | [`DeviceTreeDesc::parse`](crate::device_tree::DeviceTreeDesc) | `PlatformDescEnum::DeviceTree` |
/// | `RSDP` | [`AcpiDesc::parse`](crate::acpi::AcpiDesc) | `PlatformDescEnum::Acpi` |
/// | other | — | `Err(PlatformParseError::UnknownKind)` |
///
/// # Safety
///
/// Delegates to `DeviceTreeDesc::parse` / `AcpiDesc::parse` which dereference
/// raw physical pointers. Caller must guarantee `source.phys_addr()` points
/// to a valid firmware table that remains readable for the duration of this
/// call.
pub unsafe fn parse_by_kind(source: PlatformDescSource) -> Result<PlatformDescEnum, PlatformParseError> {
    match source.kind() {
        #[cfg(any(target_arch = "riscv64", target_arch = "aarch64"))]
        DTB => {
            // SAFETY: caller guarantees phys_addr points to a valid FDT blob.
            match unsafe {
                crate::device_tree::DeviceTreeDesc::parse(source.phys_addr().0 as usize)
            } {
                Ok(d) => Ok(PlatformDescEnum::DeviceTree(d)),
                Err(_e) => Err(PlatformParseError::DtbParse),
            }
        }
        #[cfg(target_arch = "x86_64")]
        RSDP => {
            // SAFETY: caller guarantees phys_addr points to a valid RSDP.
            match unsafe { crate::acpi::AcpiDesc::parse(source.phys_addr().0 as usize) } {
                Ok(d) => Ok(PlatformDescEnum::Acpi(d)),
                Err(_e) => Err(PlatformParseError::AcpiParse),
            }
        }
        k => Err(PlatformParseError::UnknownKind(k.raw())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_boot::PlatformDescKind;
    use minix_types::PhysBytes;

    #[test]
    fn test_parse_by_kind_unknown_returns_error() {
        let unknown = PlatformDescSource::new(PlatformDescKind::new(999), PhysBytes(0));
        // SAFETY: phys_addr=0 is not dereferenced because the kind is unknown
        // and the dispatch returns early with UnknownKind.
        let r = unsafe { parse_by_kind(unknown) };
        assert!(matches!(r, Err(PlatformParseError::UnknownKind(999))));
    }
}
