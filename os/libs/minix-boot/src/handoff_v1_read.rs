//! Receiver side of the v1 cross-image handoff protocol.
//!
//! The controlled-copy sequence (design §1.4) is split into:
//! 1. [`decode_envelope`] — bounded raw read + protocol header validation;
//!    no wire reference exists before this succeeds;
//! 2. the `decode_*` payload functions — each re-validates its descriptor
//!    (`byte_len` ≤ partition capacity, element-size divisibility, caller
//!    output capacity) and copies element-by-element into kernel-owned
//!    storage (read one, copy one, drop).
//!
//! Nothing here fabricates a `'static` reference into the arena: callers pass
//! their own landing buffers (kernel `.bss`), and only those buffers are
//! allowed to back the final `KernelInfo` slices. The byte order is
//! little-endian, mirroring [`crate::handoff_v1_write`].

use crate::handoff_v1::{
    ARENA_CAP, BOOT_HANDOFF_V1_MAGIC, BOOT_HANDOFF_V1_VERSION, BootHandoffV1,
    FREE_UPPER_IDX_NONE, WIRE_MEMMAP_CAP, WIRE_MEMMAP_OFFSET,
    WIRE_MODULE_CAP, WIRE_MODULES_OFFSET, WIRE_PARAM_CAP, WIRE_PARAM_OFFSET,
    WIRE_PLATFORM_CAP, WIRE_PLATFORM_OFFSET, WIRE_RESERVED_CAP, WIRE_RESERVED_OFFSET,
    WireBootModule, WireMemoryRegion, WirePlatformDescSource,
};
use crate::kernel_info::{BootModule, MemoryRegion};
use crate::platform::{PlatformDescKind, PlatformDescSource};
use core::mem::size_of;
use minix_types::PhysBytes;

/// Page-directory index ceiling shared by the supported 64-bit targets
/// (4-level tables, 512 entries per level). The wire field is a legacy
/// `free_upper_idx`; `u64::MAX` is the "no value" sentinel.
pub const MAX_PAGE_DIR_INDEX: usize = 512;

/// Envelope field offsets (mirror of the producer).
mod offsets {
    pub const MAGIC: usize = 0;
    pub const VERSION: usize = 4;
    pub const SIZE: usize = 8;
    pub const ARENA_LEN: usize = 16;
    pub const KERN_VIRT_BASE: usize = 24;
    pub const KERN_PHYS_BASE: usize = 32;
    pub const KERN_SIZE: usize = 40;
    pub const FREE_UPPER_IDX: usize = 48;
    pub const USER_SP: usize = 56;
    pub const KERN_STACK_TOP: usize = 64;
    pub const SYSCALL_ENTRY: usize = 72;
    pub const BOOTSTRAP_START: usize = 80;
    pub const BOOTSTRAP_LEN: usize = 88;
    pub const ROOT_PAGE: usize = 96;
    pub const BUMP_BASE: usize = 104;
    pub const BUMP_NEXT: usize = 112;
    pub const BUMP_END: usize = 120;
    pub const MEMMAP_BYTE_LEN: usize = 128;
    pub const RESERVED_BYTE_LEN: usize = 136;
    pub const MODULES_BYTE_LEN: usize = 144;
    pub const PLATFORM_BYTE_LEN: usize = 152;
    pub const PARAM_BYTE_LEN: usize = 160;
}

/// Decoder failure. Every variant is fail-closed: the receiver must not fall
/// back to a partially decoded handoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandoffV1ReadError {
    /// The provided arena slice is smaller than [`ARENA_CAP`].
    ArenaTooSmall,
    BadMagic,
    UnsupportedVersion,
    BadEnvelopeSize,
    BadArenaLen,
    /// `free_upper_idx` is neither the sentinel nor a plausible index.
    BadFreeUpperIdx,
    /// A payload descriptor exceeds its partition capacity.
    DescriptorOverflowsPartition,
    /// A payload descriptor is not a whole number of elements.
    DescriptorNotDivisible,
    /// The caller's landing buffer cannot hold the payload.
    OutputTooSmall,
    /// A wire value does not fit the internal type (e.g. `len` > `usize`).
    ValueOutOfRange,
}

/// Verified envelope scalars, copied out of the arena by value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandoffScalars {
    pub kern_virt_base: u64,
    pub kern_phys_base: u64,
    pub kern_size: u64,
    pub free_upper_idx: Option<usize>,
    pub user_sp: u64,
    pub kern_stack_top: u64,
    pub syscall_entry: u64,
    pub bootstrap_start: u64,
    pub bootstrap_len: u64,
    pub root_page: u64,
    pub bump_base: u64,
    pub bump_next: u64,
    pub bump_end: u64,
    pub memmap_byte_len: u64,
    pub reserved_byte_len: u64,
    pub modules_byte_len: u64,
    pub platform_byte_len: u64,
    pub param_byte_len: u64,
}

fn read_u32(arena: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(arena[off..off + 4].try_into().unwrap())
}

fn read_u64(arena: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(arena[off..off + 8].try_into().unwrap())
}

/// Validate the protocol header and copy the envelope scalars by value.
///
/// No payload byte is touched here; a caller that does not get `Ok` must not
/// interpret the arena any further.
pub fn decode_envelope(arena: &[u8]) -> Result<HandoffScalars, HandoffV1ReadError> {
    if arena.len() < ARENA_CAP {
        return Err(HandoffV1ReadError::ArenaTooSmall);
    }
    if read_u32(arena, offsets::MAGIC) != BOOT_HANDOFF_V1_MAGIC {
        return Err(HandoffV1ReadError::BadMagic);
    }
    if read_u32(arena, offsets::VERSION) != BOOT_HANDOFF_V1_VERSION {
        return Err(HandoffV1ReadError::UnsupportedVersion);
    }
    if read_u32(arena, offsets::SIZE) as usize != size_of::<BootHandoffV1>() {
        return Err(HandoffV1ReadError::BadEnvelopeSize);
    }
    if read_u64(arena, offsets::ARENA_LEN) != ARENA_CAP as u64 {
        return Err(HandoffV1ReadError::BadArenaLen);
    }
    let raw_free_upper = read_u64(arena, offsets::FREE_UPPER_IDX);
    let free_upper_idx = if raw_free_upper == FREE_UPPER_IDX_NONE {
        None
    } else {
        let idx =
            usize::try_from(raw_free_upper).map_err(|_| HandoffV1ReadError::ValueOutOfRange)?;
        if idx >= MAX_PAGE_DIR_INDEX {
            return Err(HandoffV1ReadError::BadFreeUpperIdx);
        }
        Some(idx)
    };
    Ok(HandoffScalars {
        kern_virt_base: read_u64(arena, offsets::KERN_VIRT_BASE),
        kern_phys_base: read_u64(arena, offsets::KERN_PHYS_BASE),
        kern_size: read_u64(arena, offsets::KERN_SIZE),
        free_upper_idx,
        user_sp: read_u64(arena, offsets::USER_SP),
        kern_stack_top: read_u64(arena, offsets::KERN_STACK_TOP),
        syscall_entry: read_u64(arena, offsets::SYSCALL_ENTRY),
        bootstrap_start: read_u64(arena, offsets::BOOTSTRAP_START),
        bootstrap_len: read_u64(arena, offsets::BOOTSTRAP_LEN),
        root_page: read_u64(arena, offsets::ROOT_PAGE),
        bump_base: read_u64(arena, offsets::BUMP_BASE),
        bump_next: read_u64(arena, offsets::BUMP_NEXT),
        bump_end: read_u64(arena, offsets::BUMP_END),
        memmap_byte_len: read_u64(arena, offsets::MEMMAP_BYTE_LEN),
        reserved_byte_len: read_u64(arena, offsets::RESERVED_BYTE_LEN),
        modules_byte_len: read_u64(arena, offsets::MODULES_BYTE_LEN),
        platform_byte_len: read_u64(arena, offsets::PLATFORM_BYTE_LEN),
        param_byte_len: read_u64(arena, offsets::PARAM_BYTE_LEN),
    })
}

/// Validate one payload descriptor against its partition capacity and return
/// the element count.
fn element_count(
    byte_len: u64,
    element_size: usize,
    cap: usize,
) -> Result<usize, HandoffV1ReadError> {
    let cap_bytes = (cap as u64) * (element_size as u64);
    if byte_len > cap_bytes {
        return Err(HandoffV1ReadError::DescriptorOverflowsPartition);
    }
    if !byte_len.is_multiple_of(element_size as u64) {
        return Err(HandoffV1ReadError::DescriptorNotDivisible);
    }
    Ok((byte_len / element_size as u64) as usize)
}

fn decode_regions(
    arena: &[u8],
    byte_len: u64,
    offset: usize,
    cap: usize,
    out: &mut [MemoryRegion],
) -> Result<usize, HandoffV1ReadError> {
    let count = element_count(byte_len, size_of::<WireMemoryRegion>(), cap)?;
    if count > out.len() {
        return Err(HandoffV1ReadError::OutputTooSmall);
    }
    for (i, slot) in out.iter_mut().enumerate().take(count) {
        let off = offset + i * size_of::<WireMemoryRegion>();
        let base = read_u64(arena, off);
        let len = read_u64(arena, off + 8);
        let len = usize::try_from(len).map_err(|_| HandoffV1ReadError::ValueOutOfRange)?;
        *slot = MemoryRegion {
            base: PhysBytes(base),
            len,
        };
    }
    Ok(count)
}

/// Decode the free memory map into `out`, returning the entry count.
pub fn decode_memmap(
    arena: &[u8],
    scalars: &HandoffScalars,
    out: &mut [MemoryRegion],
) -> Result<usize, HandoffV1ReadError> {
    decode_regions(
        arena,
        scalars.memmap_byte_len,
        WIRE_MEMMAP_OFFSET,
        WIRE_MEMMAP_CAP,
        out,
    )
}

/// Decode the reserved-region list into `out`, returning the entry count.
pub fn decode_reserved_regions(
    arena: &[u8],
    scalars: &HandoffScalars,
    out: &mut [MemoryRegion],
) -> Result<usize, HandoffV1ReadError> {
    decode_regions(
        arena,
        scalars.reserved_byte_len,
        WIRE_RESERVED_OFFSET,
        WIRE_RESERVED_CAP,
        out,
    )
}

/// Decode the boot modules into `out` (wire form; the caller turns the name
/// slots into its own storage-backed `BootModule` values).
pub fn decode_modules(
    arena: &[u8],
    scalars: &HandoffScalars,
    out: &mut [WireBootModule],
) -> Result<usize, HandoffV1ReadError> {
    let count = element_count(
        scalars.modules_byte_len,
        size_of::<WireBootModule>(),
        WIRE_MODULE_CAP,
    )?;
    if count > out.len() {
        return Err(HandoffV1ReadError::OutputTooSmall);
    }
    for (i, slot) in out.iter_mut().enumerate().take(count) {
        let off = WIRE_MODULES_OFFSET + i * size_of::<WireBootModule>();
        let mut name = [0u8; 16];
        name.copy_from_slice(&arena[off..off + 16]);
        *slot = WireBootModule {
            name,
            start: read_u64(arena, off + 16),
            len: read_u64(arena, off + 24),
        };
    }
    Ok(count)
}

/// Decode the platform descriptor sources into `out`, returning the count.
pub fn decode_platform_sources(
    arena: &[u8],
    scalars: &HandoffScalars,
    out: &mut [PlatformDescSource],
) -> Result<usize, HandoffV1ReadError> {
    let count = element_count(
        scalars.platform_byte_len,
        size_of::<WirePlatformDescSource>(),
        WIRE_PLATFORM_CAP,
    )?;
    if count > out.len() {
        return Err(HandoffV1ReadError::OutputTooSmall);
    }
    for (i, slot) in out.iter_mut().enumerate().take(count) {
        let off = WIRE_PLATFORM_OFFSET + i * size_of::<WirePlatformDescSource>();
        let kind = PlatformDescKind::new(read_u32(arena, off));
        let addr = read_u64(arena, off + 8);
        *slot = PlatformDescSource::new(kind, PhysBytes(addr));
    }
    Ok(count)
}

/// Decode the `param_buf` bytes into `out`, returning the byte count.
pub fn decode_param_buf(
    arena: &[u8],
    scalars: &HandoffScalars,
    out: &mut [u8],
) -> Result<usize, HandoffV1ReadError> {
    let byte_len = scalars.param_byte_len;
    if byte_len > WIRE_PARAM_CAP as u64 {
        return Err(HandoffV1ReadError::DescriptorOverflowsPartition);
    }
    let count = usize::try_from(byte_len).map_err(|_| HandoffV1ReadError::ValueOutOfRange)?;
    if count > out.len() {
        return Err(HandoffV1ReadError::OutputTooSmall);
    }
    out[..count].copy_from_slice(&arena[WIRE_PARAM_OFFSET..WIRE_PARAM_OFFSET + count]);
    Ok(count)
}

/// Convert a wire module plus its landed name into the kernel's internal
/// `BootModule` value.
///
/// `name` must be the NUL-terminated name borrowed from the caller's landing
/// pool (kernel `.bss`), so the resulting `&'static str` does not point into
/// the arena.
pub fn wire_module_to_boot_module(
    wire: &WireBootModule,
    name: &'static str,
) -> BootModule {
    BootModule {
        name,
        start: PhysBytes(wire.start),
        len: usize::try_from(wire.len).unwrap_or(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handoff_v1::ARENA_CAP;
    use crate::handoff_v1_write::{RuntimeFields, Serializer};
    use crate::kernel_info::KernelInfo;
    use crate::platform::{DTB, RSDP};
    use minix_types::{USER_STACK_TOP, VirBytes};

    static MEMMAP: [MemoryRegion; 2] = [
        MemoryRegion {
            base: PhysBytes(0x10_0000),
            len: 0x10_0000,
        },
        MemoryRegion {
            base: PhysBytes(0x40_0000),
            len: 0x20_0000,
        },
    ];
    static RESERVED: [MemoryRegion; 1] = [MemoryRegion {
        base: PhysBytes(0x1DA0_0000),
        len: 0x3000,
    }];
    static MODULES: [BootModule; 2] = [
        BootModule {
            name: "vm",
            start: PhysBytes(0x20_0000),
            len: 0x1000,
        },
        BootModule {
            name: "memory",
            start: PhysBytes(0x21_0000),
            len: 0x2000,
        },
    ];
    static SOURCES: [PlatformDescSource; 2] = [
        PlatformDescSource::new(DTB, PhysBytes(0x4000_0000)),
        PlatformDescSource::new(RSDP, PhysBytes(0xE0000)),
    ];
    static PARAM: &[u8] = b"ARCH=x86_64\0BOARD=qemu\0";

    const ROOT: u64 = 0x1000;
    const BUMP_BASE: u64 = 0x20_0000;
    const BUMP_NEXT: u64 = 0x21_0000;
    const BUMP_END: u64 = 0x30_0000;

    fn info() -> KernelInfo {
        KernelInfo {
            memmap: &MEMMAP,
            reserved_regions: &RESERVED,
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x20_0000),
            kern_size: 0x20_0000,
            free_upper_idx: Some(256),
            user_sp: VirBytes(USER_STACK_TOP as u64),
            kern_stack_top: VirBytes(0xFFFF_8000_0040_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: &MODULES,
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
            platform_sources: &SOURCES,
            param_buf: PARAM,
        }
    }

    fn build_arena() -> [u8; ARENA_CAP] {
        let mut arena = [0u8; ARENA_CAP];
        {
            let mut s = Serializer::new(&mut arena).unwrap();
            let info = info();
            s.write_envelope(&info).unwrap();
            s.write_reserved_regions(info.reserved_regions).unwrap();
            s.write_boot_modules(info.boot_modules).unwrap();
            s.write_platform_sources(info.platform_sources).unwrap();
            s.write_param_buf(info.param_buf).unwrap();
            for region in info.memmap {
                s.push_memmap(*region).unwrap();
            }
            s.finish_memmap();
            s.patch_runtime_fields(RuntimeFields {
                root_page: ROOT,
                bump_base: BUMP_BASE,
                bump_next: BUMP_NEXT,
                bump_end: BUMP_END,
            });
        }
        arena
    }

    fn put_u32(arena: &mut [u8], off: usize, value: u32) {
        arena[off..off + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u64(arena: &mut [u8], off: usize, value: u64) {
        arena[off..off + 8].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn round_trip_scalars_and_runtime_fields() {
        let arena = build_arena();
        let s = decode_envelope(&arena).unwrap();
        assert_eq!(s.kern_virt_base, 0xFFFF_8000_0000_0000);
        assert_eq!(s.kern_phys_base, 0x20_0000);
        assert_eq!(s.kern_size, 0x20_0000);
        assert_eq!(s.free_upper_idx, Some(256));
        assert_eq!(s.bootstrap_len, 0);
        assert_eq!(s.root_page, ROOT);
        assert_eq!(s.bump_base, BUMP_BASE);
        assert_eq!(s.bump_next, BUMP_NEXT);
        assert_eq!(s.bump_end, BUMP_END);
        assert_eq!(s.memmap_byte_len, 32);
        assert_eq!(s.reserved_byte_len, 16);
        assert_eq!(s.modules_byte_len, 64);
        assert_eq!(s.platform_byte_len, 32);
        assert_eq!(s.param_byte_len, PARAM.len() as u64);
    }

    #[test]
    fn round_trip_memmap_and_reserved_regions() {
        let arena = build_arena();
        let s = decode_envelope(&arena).unwrap();
        let mut free = [MemoryRegion {
            base: PhysBytes(0),
            len: 0,
        }; WIRE_MEMMAP_CAP];
        let n = decode_memmap(&arena, &s, &mut free).unwrap();
        assert_eq!(n, 2);
        assert_eq!(free[0].base.0, 0x10_0000);
        assert_eq!(free[0].len, 0x10_0000);
        assert_eq!(free[1].base.0, 0x40_0000);
        let mut reserved = [MemoryRegion {
            base: PhysBytes(0),
            len: 0,
        }; WIRE_RESERVED_CAP];
        let n = decode_reserved_regions(&arena, &s, &mut reserved).unwrap();
        assert_eq!(n, 1);
        assert_eq!(reserved[0].base.0, 0x1DA0_0000);
        assert_eq!(reserved[0].len, 0x3000);
    }

    #[test]
    fn round_trip_modules_platform_and_param() {
        let arena = build_arena();
        let s = decode_envelope(&arena).unwrap();
        let zero = WireBootModule {
            name: [0; 16],
            start: 0,
            len: 0,
        };
        let mut modules = [zero; WIRE_MODULE_CAP];
        let n = decode_modules(&arena, &s, &mut modules).unwrap();
        assert_eq!(n, 2);
        assert_eq!(&modules[0].name[..2], b"vm");
        assert!(modules[0].name[2..].iter().all(|&b| b == 0));
        assert_eq!(modules[1].start, 0x21_0000);
        let vm = wire_module_to_boot_module(&modules[0], "vm");
        assert_eq!(vm.name, "vm");
        assert_eq!(vm.start.0, 0x20_0000);

        let empty = PlatformDescSource::new(DTB, PhysBytes(0));
        let mut sources = [empty; WIRE_PLATFORM_CAP];
        let n = decode_platform_sources(&arena, &s, &mut sources).unwrap();
        assert_eq!(n, 2);
        assert_eq!(sources[0].kind().raw(), DTB.raw());
        assert_eq!(sources[0].phys_addr().0, 0x4000_0000);
        assert_eq!(sources[1].kind().raw(), RSDP.raw());

        let mut param = [0u8; WIRE_PARAM_CAP];
        let n = decode_param_buf(&arena, &s, &mut param).unwrap();
        assert_eq!(&param[..n], PARAM);
    }

    #[test]
    fn rejects_bad_header_fields() {
        let mut arena = build_arena();
        put_u32(&mut arena, 0, 0);
        assert_eq!(
            decode_envelope(&arena).unwrap_err(),
            HandoffV1ReadError::BadMagic
        );
        let mut arena = build_arena();
        put_u32(&mut arena, 4, 2);
        assert_eq!(
            decode_envelope(&arena).unwrap_err(),
            HandoffV1ReadError::UnsupportedVersion
        );
        let mut arena = build_arena();
        put_u32(&mut arena, 8, 1);
        assert_eq!(
            decode_envelope(&arena).unwrap_err(),
            HandoffV1ReadError::BadEnvelopeSize
        );
        let mut arena = build_arena();
        put_u64(&mut arena, 16, 4096);
        assert_eq!(
            decode_envelope(&arena).unwrap_err(),
            HandoffV1ReadError::BadArenaLen
        );
        let small = [0u8; 16];
        assert_eq!(
            decode_envelope(&small).unwrap_err(),
            HandoffV1ReadError::ArenaTooSmall
        );
    }

    #[test]
    fn free_upper_idx_sentinel_and_range() {
        let mut arena = build_arena();
        put_u64(&mut arena, 48, FREE_UPPER_IDX_NONE);
        assert_eq!(decode_envelope(&arena).unwrap().free_upper_idx, None);
        let mut arena = build_arena();
        put_u64(&mut arena, 48, MAX_PAGE_DIR_INDEX as u64);
        assert_eq!(
            decode_envelope(&arena).unwrap_err(),
            HandoffV1ReadError::BadFreeUpperIdx
        );
    }

    #[test]
    fn rejects_descriptor_over_capacity() {
        let mut arena = build_arena();
        put_u64(&mut arena, 128, (WIRE_MEMMAP_CAP as u64 + 1) * 16);
        let s = decode_envelope(&arena).unwrap();
        let mut out = [MemoryRegion {
            base: PhysBytes(0),
            len: 0,
        }; WIRE_MEMMAP_CAP];
        assert_eq!(
            decode_memmap(&arena, &s, &mut out).unwrap_err(),
            HandoffV1ReadError::DescriptorOverflowsPartition
        );
    }

    #[test]
    fn rejects_non_divisible_descriptor() {
        let mut arena = build_arena();
        put_u64(&mut arena, 128, 24);
        let s = decode_envelope(&arena).unwrap();
        let mut out = [MemoryRegion {
            base: PhysBytes(0),
            len: 0,
        }; WIRE_MEMMAP_CAP];
        assert_eq!(
            decode_memmap(&arena, &s, &mut out).unwrap_err(),
            HandoffV1ReadError::DescriptorNotDivisible
        );
    }

    #[test]
    fn rejects_small_output_buffers() {
        let arena = build_arena();
        let s = decode_envelope(&arena).unwrap();
        let mut out = [MemoryRegion {
            base: PhysBytes(0),
            len: 0,
        }; 1];
        assert_eq!(
            decode_memmap(&arena, &s, &mut out).unwrap_err(),
            HandoffV1ReadError::OutputTooSmall
        );
        let mut param = [0u8; 4];
        assert_eq!(
            decode_param_buf(&arena, &s, &mut param).unwrap_err(),
            HandoffV1ReadError::OutputTooSmall
        );
    }

    #[test]
    fn rejects_param_over_capacity() {
        let mut arena = build_arena();
        put_u64(&mut arena, 160, WIRE_PARAM_CAP as u64 + 1);
        let s = decode_envelope(&arena).unwrap();
        let mut param = [0u8; WIRE_PARAM_CAP + 1];
        assert_eq!(
            decode_param_buf(&arena, &s, &mut param).unwrap_err(),
            HandoffV1ReadError::DescriptorOverflowsPartition
        );
    }
}
