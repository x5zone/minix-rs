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
    ARENA_CAP, BOOT_HANDOFF_V1_MAGIC, BOOT_HANDOFF_V1_VERSION, BootHandoffV1, FREE_UPPER_IDX_NONE,
    WIRE_MEMMAP_CAP, WIRE_MEMMAP_OFFSET, WIRE_MODULE_CAP, WIRE_MODULES_OFFSET, WIRE_PARAM_CAP,
    WIRE_PARAM_OFFSET, WIRE_PLATFORM_CAP, WIRE_PLATFORM_OFFSET, WIRE_RESERVED_CAP,
    WIRE_RESERVED_OFFSET, WIRE_SOURCE_RECORD_SIZE, WIRE_SOURCES_CAP, WIRE_SOURCES_OFFSET,
    WIRE_STARTUP_CAP, WIRE_STARTUP_OFFSET, WireBootModule, WireMemoryRegion,
    WirePlatformDescSource, WireSourceRecord,
};
use crate::kernel_info::{BootModule, MemoryRegion};
use crate::memmap_source::SourceRecord;
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
    pub const RAM_AUTHORITY: usize = 12;
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
    pub const SOURCES_BYTE_LEN: usize = 168;
    pub const STARTUP_BYTE_LEN: usize = 176;
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
    /// `root_page` is not a 4 KiB-aligned physical page.
    BadRootPage,
    /// The bump triple is not a self-consistent half-open range.
    BadBumpRange,
    /// A source record carries a tag or payload value the encoder cannot
    /// produce (corrupted or foreign wire image).
    BadSourceRecord,
    /// The declared RAM authority is not one this kernel understands.
    BadRamAuthority,
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
    pub sources_byte_len: u64,
    pub startup_byte_len: u64,
    pub ram_authority: crate::memmap_source::InputObjectId,
}

fn read_u32(arena: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(arena[off..off + 4].try_into().unwrap())
}

fn read_u64(arena: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(arena[off..off + 8].try_into().unwrap())
}

/// Validate the protocol header (magic/version/envelope size/arena length).
fn check_header(arena: &[u8]) -> Result<(), HandoffV1ReadError> {
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
    Ok(())
}

/// Payload descriptor lengths, read by the sender after the pairing loop has
/// classified the final map (the sender still needs the semantic partition to
/// build the in-process `KernelInfo`, and the runtime fields are not patched
/// until the root is live).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandoffDescriptors {
    pub memmap_byte_len: u64,
    pub reserved_byte_len: u64,
    pub modules_byte_len: u64,
    pub platform_byte_len: u64,
    pub param_byte_len: u64,
    pub sources_byte_len: u64,
    pub startup_byte_len: u64,
}

/// Validate the header and return the five payload descriptors only.
///
/// This is the sender-side read: it deliberately does not touch the runtime
/// fields, which are patched later in the boot sequence.
pub fn decode_descriptors(arena: &[u8]) -> Result<HandoffDescriptors, HandoffV1ReadError> {
    check_header(arena)?;
    Ok(HandoffDescriptors {
        memmap_byte_len: read_u64(arena, offsets::MEMMAP_BYTE_LEN),
        reserved_byte_len: read_u64(arena, offsets::RESERVED_BYTE_LEN),
        modules_byte_len: read_u64(arena, offsets::MODULES_BYTE_LEN),
        platform_byte_len: read_u64(arena, offsets::PLATFORM_BYTE_LEN),
        param_byte_len: read_u64(arena, offsets::PARAM_BYTE_LEN),
        sources_byte_len: read_u64(arena, offsets::SOURCES_BYTE_LEN),
        startup_byte_len: read_u64(arena, offsets::STARTUP_BYTE_LEN),
    })
}

/// Validate the protocol header and copy the envelope scalars by value.
///
/// No payload byte is touched here; a caller that does not get `Ok` must not
/// interpret the arena any further.
pub fn decode_envelope(arena: &[u8]) -> Result<HandoffScalars, HandoffV1ReadError> {
    check_header(arena)?;
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
    // The runtime fields are written by the sender after the root switch, so
    // the receiver can only trust their shape here. `root_page` must be a real
    // page; the bump triple must be a self-consistent half-open range (N-1:
    // an inverted or out-of-range cursor would let `BootAlloc::alloc` hand out
    // pages outside the declared region).
    let ram_authority = match read_u32(arena, offsets::RAM_AUTHORITY) {
        0 => crate::memmap_source::InputObjectId::DtbBootInput,
        1 => crate::memmap_source::InputObjectId::UefiMemoryMap,
        _ => return Err(HandoffV1ReadError::BadRamAuthority),
    };
    let root_page = read_u64(arena, offsets::ROOT_PAGE);
    if root_page == 0 || !root_page.is_multiple_of(4096) {
        return Err(HandoffV1ReadError::BadRootPage);
    }
    let bump_base = read_u64(arena, offsets::BUMP_BASE);
    let bump_next = read_u64(arena, offsets::BUMP_NEXT);
    let bump_end = read_u64(arena, offsets::BUMP_END);
    if !(bump_base < bump_end && bump_base <= bump_next && bump_next <= bump_end) {
        return Err(HandoffV1ReadError::BadBumpRange);
    }
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
        root_page,
        bump_base,
        bump_next,
        bump_end,
        memmap_byte_len: read_u64(arena, offsets::MEMMAP_BYTE_LEN),
        reserved_byte_len: read_u64(arena, offsets::RESERVED_BYTE_LEN),
        modules_byte_len: read_u64(arena, offsets::MODULES_BYTE_LEN),
        platform_byte_len: read_u64(arena, offsets::PLATFORM_BYTE_LEN),
        param_byte_len: read_u64(arena, offsets::PARAM_BYTE_LEN),
        sources_byte_len: read_u64(arena, offsets::SOURCES_BYTE_LEN),
        startup_byte_len: read_u64(arena, offsets::STARTUP_BYTE_LEN),
        ram_authority,
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

/// Decode the memory-map source records into `out`, returning the count.
///
/// Every record is re-validated by [`WireSourceRecord::decode`]: an unknown
/// tag or a payload word the encoder always zeroes is a corrupted wire image
/// and fails closed. The landed records back the kernel's `KernelInfo`
/// slice, so — like every other payload — nothing here aliases the arena.
pub fn decode_sources(
    arena: &[u8],
    scalars: &HandoffScalars,
    out: &mut [SourceRecord],
) -> Result<usize, HandoffV1ReadError> {
    let count = element_count(
        scalars.sources_byte_len,
        WIRE_SOURCE_RECORD_SIZE,
        WIRE_SOURCES_CAP,
    )?;
    if count > out.len() {
        return Err(HandoffV1ReadError::OutputTooSmall);
    }
    for (i, slot) in out.iter_mut().enumerate().take(count) {
        let off = WIRE_SOURCES_OFFSET + i * WIRE_SOURCE_RECORD_SIZE;
        let wire = WireSourceRecord {
            start: read_u64(arena, off),
            end_lo: read_u64(arena, off + 8),
            end_hi: read_u64(arena, off + 16),
            attribute: read_u64(arena, off + 24),
            virtual_start: read_u64(arena, off + 32),
            snapshot: read_u64(arena, off + 40),
            cut: read_u64(arena, off + 48),
            id: u16::from_le_bytes(arena[off + 56..off + 58].try_into().unwrap()),
            local: u16::from_le_bytes(arena[off + 58..off + 60].try_into().unwrap()),
            a: u16::from_le_bytes(arena[off + 60..off + 62].try_into().unwrap()),
            b: u16::from_le_bytes(arena[off + 62..off + 64].try_into().unwrap()),
            kind: arena[off + 64],
            origin_tag: arena[off + 65],
            input_object: arena[off + 66],
            address_domain: arena[off + 67],
            raw_type: read_u32(arena, off + 68),
        };
        *slot = wire.decode().ok_or(HandoffV1ReadError::BadSourceRecord)?;
    }
    Ok(count)
}

/// Decode the startup-object records into `out`, returning the count.
///
/// They live in their own partition with their own capacity, so the memory
/// map's 256 slots can never crowd them out.
pub fn decode_startup_objects(
    arena: &[u8],
    scalars: &HandoffScalars,
    out: &mut [SourceRecord],
) -> Result<usize, HandoffV1ReadError> {
    let count = element_count(
        scalars.startup_byte_len,
        WIRE_SOURCE_RECORD_SIZE,
        WIRE_STARTUP_CAP,
    )?;
    if count > out.len() {
        return Err(HandoffV1ReadError::OutputTooSmall);
    }
    for (i, slot) in out.iter_mut().enumerate().take(count) {
        let off = WIRE_STARTUP_OFFSET + i * WIRE_SOURCE_RECORD_SIZE;
        let wire = WireSourceRecord {
            start: read_u64(arena, off),
            end_lo: read_u64(arena, off + 8),
            end_hi: read_u64(arena, off + 16),
            attribute: read_u64(arena, off + 24),
            virtual_start: read_u64(arena, off + 32),
            snapshot: read_u64(arena, off + 40),
            cut: read_u64(arena, off + 48),
            id: u16::from_le_bytes(arena[off + 56..off + 58].try_into().unwrap()),
            local: u16::from_le_bytes(arena[off + 58..off + 60].try_into().unwrap()),
            a: u16::from_le_bytes(arena[off + 60..off + 62].try_into().unwrap()),
            b: u16::from_le_bytes(arena[off + 62..off + 64].try_into().unwrap()),
            kind: arena[off + 64],
            origin_tag: arena[off + 65],
            input_object: arena[off + 66],
            address_domain: arena[off + 67],
            raw_type: read_u32(arena, off + 68),
        };
        *slot = wire.decode().ok_or(HandoffV1ReadError::BadSourceRecord)?;
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
pub fn wire_module_to_boot_module(wire: &WireBootModule, name: &'static str) -> BootModule {
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
    use crate::handoff_v1_write::{HandoffV1WriteError, RuntimeFields, Serializer};
    use crate::kernel_info::KernelInfo;
    use crate::memmap_source::{
        SourceKind, SourceOrigin, SourceRange, SourceRecord, startup_record,
    };
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
    static RECORDS: [SourceRecord; 3] = [
        SourceRecord {
            input_object: crate::memmap_source::InputObjectId::DtbBootInput,
            address_domain: crate::memmap_source::AddressDomain::Physical,
            local_source: 0,
            snapshot_id: 0,
            attribute: 0,
            virtual_start: 0,
            id: 0,
            raw_type: 0,
            kind: SourceKind::Memory,
            origin: SourceOrigin::MemoryNode { node: 0, reg: 0 },
            range: SourceRange::new(0x10_0000, 0x20_0000),
        },
        SourceRecord {
            input_object: crate::memmap_source::InputObjectId::DtbBootInput,
            address_domain: crate::memmap_source::AddressDomain::Physical,
            local_source: 0,
            snapshot_id: 0,
            attribute: 0,
            virtual_start: 0,
            id: 1,
            raw_type: 0,
            kind: SourceKind::Memory,
            origin: SourceOrigin::MemoryNode { node: 0, reg: 1 },
            range: SourceRange::new(0x40_0000, 0x60_0000),
        },
        SourceRecord {
            input_object: crate::memmap_source::InputObjectId::DtbBootInput,
            address_domain: crate::memmap_source::AddressDomain::Physical,
            local_source: 0,
            snapshot_id: 0,
            attribute: 0,
            virtual_start: 0,
            id: 2,
            raw_type: 0,
            kind: SourceKind::ReservedOrdinary,
            origin: SourceOrigin::ReservedMemoryChild { node: 0, reg: 0 },
            range: SourceRange::new(0x1DA0_0000, 0x1DA0_3000),
        },
    ];

    /// Kernel image + two modules, in the startup-object class.
    static STARTUP: [SourceRecord; 3] = [
        startup_record(0, 0x20_0000, 0x60_0000),
        startup_record(1, 0x20_0000, 0x21_0000),
        startup_record(2, 0x21_0000, 0x23_0000),
    ];

    const ROOT: u64 = 0x1000;
    const BUMP_BASE: u64 = 0x20_0000;
    const BUMP_NEXT: u64 = 0x21_0000;
    const BUMP_END: u64 = 0x30_0000;

    fn info() -> KernelInfo {
        KernelInfo {
            memmap: &MEMMAP,
            reserved_regions: &RESERVED,
            memmap_sources: &[],
            startup_objects: &[],
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
            s.write_sources(&RECORDS).unwrap();
            s.write_startup(&STARTUP).unwrap();
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
        // Version 1 is the pre-source-partition envelope: an exact match is
        // required, so an older sender is rejected rather than read with the
        // new offsets.
        put_u32(&mut arena, 4, 1);
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
    fn descriptor_reader_ignores_unpatched_runtime_fields() {
        let mut arena = [0u8; ARENA_CAP];
        {
            let mut s = Serializer::new(&mut arena).unwrap();
            let info = info();
            s.write_envelope(&info).unwrap();
            s.push_memmap(MEMMAP[0]).unwrap();
            s.finish_memmap();
            s.write_param_buf(PARAM).unwrap();
        }
        let d = decode_descriptors(&arena).unwrap();
        assert_eq!(d.memmap_byte_len, 16);
        assert_eq!(d.reserved_byte_len, 0);
        assert_eq!(d.param_byte_len, PARAM.len() as u64);
        // The full envelope read still rejects the unpatched runtime fields.
        assert_eq!(
            decode_envelope(&arena).unwrap_err(),
            HandoffV1ReadError::BadRootPage
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
    fn rejects_bad_root_and_bump_triple() {
        let mut arena = build_arena();
        put_u64(&mut arena, 96, 0x1234);
        assert_eq!(
            decode_envelope(&arena).unwrap_err(),
            HandoffV1ReadError::BadRootPage
        );

        let mut arena = build_arena();
        put_u64(&mut arena, 96, 0);
        assert_eq!(
            decode_envelope(&arena).unwrap_err(),
            HandoffV1ReadError::BadRootPage
        );

        // Inverted range: base above end.
        let mut arena = build_arena();
        put_u64(&mut arena, 104, BUMP_END + 0x1000);
        assert_eq!(
            decode_envelope(&arena).unwrap_err(),
            HandoffV1ReadError::BadBumpRange
        );

        // Cursor before the base.
        let mut arena = build_arena();
        put_u64(&mut arena, 112, BUMP_BASE - 0x1000);
        assert_eq!(
            decode_envelope(&arena).unwrap_err(),
            HandoffV1ReadError::BadBumpRange
        );

        // Cursor past the end.
        let mut arena = build_arena();
        put_u64(&mut arena, 112, BUMP_END + 0x1000);
        assert_eq!(
            decode_envelope(&arena).unwrap_err(),
            HandoffV1ReadError::BadBumpRange
        );

        // The empty region is not a valid bump pool for this protocol.
        let mut arena = build_arena();
        put_u64(&mut arena, 104, BUMP_BASE);
        put_u64(&mut arena, 112, BUMP_BASE);
        put_u64(&mut arena, 120, BUMP_BASE);
        assert_eq!(
            decode_envelope(&arena).unwrap_err(),
            HandoffV1ReadError::BadBumpRange
        );
    }

    #[test]
    fn round_trip_sources() {
        let arena = build_arena();
        let s = decode_envelope(&arena).unwrap();
        assert_eq!(
            s.sources_byte_len,
            (RECORDS.len() * WIRE_SOURCE_RECORD_SIZE) as u64
        );
        let mut out = [SourceRecord {
            input_object: crate::memmap_source::InputObjectId::DtbBootInput,
            address_domain: crate::memmap_source::AddressDomain::Physical,
            local_source: 0,
            snapshot_id: 0,
            attribute: 0,
            virtual_start: 0,
            id: 0,
            raw_type: 0,
            kind: SourceKind::Memory,
            origin: SourceOrigin::MrbEntry(0),
            range: SourceRange::new(0, 0),
        }; WIRE_SOURCES_CAP];
        let n = decode_sources(&arena, &s, &mut out).unwrap();
        assert_eq!(n, RECORDS.len());
        assert_eq!(&out[..n], &RECORDS[..]);
    }

    /// The startup-object partition is decoded on its own and never eats into
    /// the memory-map partition (or vice versa).
    #[test]
    fn round_trip_startup_objects() {
        let arena = build_arena();
        let s = decode_envelope(&arena).unwrap();
        assert_eq!(
            s.startup_byte_len,
            (STARTUP.len() * WIRE_SOURCE_RECORD_SIZE) as u64
        );
        let mut out = [STARTUP[0]; WIRE_STARTUP_CAP];
        let n = decode_startup_objects(&arena, &s, &mut out).unwrap();
        assert_eq!(n, STARTUP.len());
        assert_eq!(&out[..n], &STARTUP[..]);
        let mut mem = [RECORDS[0]; WIRE_SOURCES_CAP];
        let n = decode_sources(&arena, &s, &mut mem).unwrap();
        assert_eq!(n, RECORDS.len());
        assert_eq!(&mem[..n], &RECORDS[..]);
    }

    /// Exactly `WIRE_STARTUP_CAP` startup records are accepted while the
    /// memory partition is simultaneously full at its own cap; the 17th
    /// startup record is refused by the startup capacity alone.
    #[test]
    fn startup_partition_accepts_exactly_its_capacity() {
        let mut arena = [0u8; ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        s.write_envelope(&info()).unwrap();
        s.patch_runtime_fields(RuntimeFields {
            root_page: ROOT,
            bump_base: BUMP_BASE,
            bump_next: BUMP_NEXT,
            bump_end: BUMP_END,
        });
        let memmap_full = [RECORDS[0]; WIRE_SOURCES_CAP];
        s.write_sources(&memmap_full).unwrap();
        let startup_full = [STARTUP[0]; WIRE_STARTUP_CAP];
        s.write_startup(&startup_full).unwrap();
        let startup_over = [STARTUP[0]; WIRE_STARTUP_CAP + 1];
        assert_eq!(
            s.write_startup(&startup_over).unwrap_err(),
            HandoffV1WriteError::TooManySourceRecords
        );
        let scalars = decode_envelope(&arena).unwrap();
        assert_eq!(
            scalars.sources_byte_len,
            (WIRE_SOURCES_CAP * WIRE_SOURCE_RECORD_SIZE) as u64
        );
        assert_eq!(
            scalars.startup_byte_len,
            (WIRE_STARTUP_CAP * WIRE_SOURCE_RECORD_SIZE) as u64
        );
    }

    #[test]
    fn rejects_startup_over_capacity() {
        let mut arena = build_arena();
        // Startup descriptor is the u64 at offset 176.
        put_u64(
            &mut arena,
            176,
            ((WIRE_STARTUP_CAP as u64) + 1) * WIRE_SOURCE_RECORD_SIZE as u64,
        );
        let s = decode_envelope(&arena).unwrap();
        let mut out = [STARTUP[0]; WIRE_STARTUP_CAP];
        assert_eq!(
            decode_startup_objects(&arena, &s, &mut out).unwrap_err(),
            HandoffV1ReadError::DescriptorOverflowsPartition
        );
    }

    /// The EBS retry clears the classification snapshot only; startup-object
    /// records describe the image and must survive it.
    #[test]
    fn reset_map_partitions_keeps_the_startup_partition() {
        let mut arena = [0u8; ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        s.write_envelope(&info()).unwrap();
        s.write_sources(&RECORDS).unwrap();
        s.write_startup(&STARTUP).unwrap();
        s.patch_runtime_fields(RuntimeFields {
            root_page: ROOT,
            bump_base: BUMP_BASE,
            bump_next: BUMP_NEXT,
            bump_end: BUMP_END,
        });
        s.reset_map_partitions();
        let scalars = decode_envelope(&arena).unwrap();
        assert_eq!(scalars.sources_byte_len, 0);
        assert_eq!(
            scalars.startup_byte_len,
            (STARTUP.len() * WIRE_SOURCE_RECORD_SIZE) as u64
        );
        let mut out = [STARTUP[0]; WIRE_STARTUP_CAP];
        let n = decode_startup_objects(&arena, &scalars, &mut out).unwrap();
        assert_eq!(&out[..n], &STARTUP[..]);
    }

    #[test]
    fn rejects_sources_over_capacity() {
        let mut arena = build_arena();
        put_u64(
            &mut arena,
            168,
            ((WIRE_SOURCES_CAP as u64) + 1) * WIRE_SOURCE_RECORD_SIZE as u64,
        );
        let s = decode_envelope(&arena).unwrap();
        let mut out = [SourceRecord {
            input_object: crate::memmap_source::InputObjectId::DtbBootInput,
            address_domain: crate::memmap_source::AddressDomain::Physical,
            local_source: 0,
            snapshot_id: 0,
            attribute: 0,
            virtual_start: 0,
            id: 0,
            raw_type: 0,
            kind: SourceKind::Memory,
            origin: SourceOrigin::MrbEntry(0),
            range: SourceRange::new(0, 0),
        }; WIRE_SOURCES_CAP];
        assert_eq!(
            decode_sources(&arena, &s, &mut out).unwrap_err(),
            HandoffV1ReadError::DescriptorOverflowsPartition
        );
    }

    /// A record word the encoder always zeroes (the reserved tail) means the
    /// image did not come from [`WireSourceRecord::encode`]; the receiver
    /// fails closed instead of carrying a guessed record.
    /// A non-physical record (EFI type 12) is provenance only: it decodes as
    /// `UefiExcluded`, and a non-physical domain paired with any other kind is
    /// rejected rather than reinterpreted as a physical range.
    #[test]
    fn non_physical_domain_is_only_legal_on_excluded_records() {
        let mut arena = build_arena();
        let off = crate::handoff_v1::WIRE_SOURCES_OFFSET;
        // `build_arena` writes DTB records (address_domain = 0). Patch the
        // first record's domain to UefiMmioPortSpace (2) and its kind to
        // `UefiExcluded` (7); decoding must succeed.
        arena[off + 67] = 2;
        arena[off + 64] = 7;
        let s = decode_envelope(&arena).unwrap();
        let mut out = [SourceRecord {
            input_object: crate::memmap_source::InputObjectId::DtbBootInput,
            address_domain: crate::memmap_source::AddressDomain::Physical,
            local_source: 0,
            snapshot_id: 0,
            attribute: 0,
            virtual_start: 0,
            id: 0,
            raw_type: 0,
            kind: SourceKind::Memory,
            origin: SourceOrigin::MrbEntry(0),
            range: SourceRange::new(0, 0),
        }; WIRE_SOURCES_CAP];
        let n = decode_sources(&arena, &s, &mut out).unwrap();
        assert!(n >= 1);
        assert_eq!(
            out[0].address_domain,
            crate::memmap_source::AddressDomain::UefiMmioPortSpace
        );
        // Same domain with a physical kind is a corrupt image.
        let mut arena = build_arena();
        arena[off + 67] = 2; // domain = UefiMmioPortSpace, kind stays Memory
        let s = decode_envelope(&arena).unwrap();
        assert_eq!(
            decode_sources(&arena, &s, &mut out).unwrap_err(),
            HandoffV1ReadError::BadSourceRecord
        );
    }

    #[test]
    fn round_trips_the_raw_firmware_type_and_accepts_any_value() {
        // The trailing word used to be a must-be-zero reserved slot; it now
        // carries the firmware's own Type, so every value is legal and must
        // survive the round trip.
        let mut arena = build_arena();
        let wire_off = crate::handoff_v1::WIRE_SOURCES_OFFSET + 68;
        put_u32(&mut arena, wire_off, 0xDEAD_BEEF);
        let s = decode_envelope(&arena).unwrap();
        let mut out = [SourceRecord {
            input_object: crate::memmap_source::InputObjectId::DtbBootInput,
            address_domain: crate::memmap_source::AddressDomain::Physical,
            local_source: 0,
            snapshot_id: 0,
            attribute: 0,
            virtual_start: 0,
            id: 0,
            raw_type: 0,
            kind: SourceKind::Memory,
            origin: SourceOrigin::MrbEntry(0),
            range: SourceRange::new(0, 0),
        }; WIRE_SOURCES_CAP];
        let n = decode_sources(&arena, &s, &mut out).unwrap();
        assert!(n >= 1);
        assert_eq!(out[0].raw_type, 0xDEAD_BEEF);
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

    /// The arena holds nothing that outlives the decode.
    ///
    /// `arch_boot_resume_v1` runs this exact sequence — envelope, then the
    /// five payloads into kernel-owned landing buffers — and the `KernelInfo`
    /// the kernel keeps afterwards is built from those buffers, never from
    /// arena bytes. So the arena's last read happens before the kernel's
    /// first VM allocation, and the pages it occupies (loader data on the
    /// UEFI legs, the next bump pages on the U-Boot leg) need no reservation
    /// of their own. Poisoning the arena right after the decode is the
    /// executable form of that claim: any surviving alias into arena memory
    /// would flip a landing byte to the poison pattern and fail here.
    #[test]
    fn round_trip_identity_fields() {
        use crate::memmap_source::InputObjectId;
        let record = SourceRecord {
            id: 7,
            raw_type: 0,
            kind: SourceKind::Memory,
            origin: SourceOrigin::UefiDescriptor { index: 7 },
            range: SourceRange::new(0x1000, 0x3000),
            input_object: InputObjectId::UefiMemoryMap,
            address_domain: crate::memmap_source::AddressDomain::Physical,
            local_source: 7,
            snapshot_id: 0xABCD_EF01,
            attribute: 0x8000_0000_0000_000F,
            virtual_start: 0xFFFF_8000_0000_1000,
        };
        let mut arena = [0u8; ARENA_CAP];
        {
            let mut s = Serializer::new(&mut arena).unwrap();
            s.write_envelope(&info()).unwrap();
            s.write_sources(&[record]).unwrap();
            s.patch_runtime_fields(RuntimeFields {
                root_page: ROOT,
                bump_base: BUMP_BASE,
                bump_next: BUMP_NEXT,
                bump_end: BUMP_END,
            });
        }
        let scalars = decode_envelope(&arena).unwrap();
        let mut out = [record; 1];
        let n = decode_sources(&arena, &scalars, &mut out).unwrap();
        assert_eq!(n, 1);
        assert_eq!(out[0].input_object, InputObjectId::UefiMemoryMap);
        assert_eq!(out[0].local_source, 7);
        assert_eq!(out[0].snapshot_id, 0xABCD_EF01);
        assert_eq!(out[0].attribute, 0x8000_0000_0000_000F);
        assert_eq!(out[0].virtual_start, 0xFFFF_8000_0000_1000);
    }

    #[test]
    fn rejects_nonzero_snapshot_on_a_dtb_record() {
        let mut arena = [0u8; ARENA_CAP];
        {
            let mut s = Serializer::new(&mut arena).unwrap();
            s.write_envelope(&info()).unwrap();
            s.write_sources(&RECORDS[..1]).unwrap();
            s.patch_runtime_fields(RuntimeFields {
                root_page: ROOT,
                bump_base: BUMP_BASE,
                bump_next: BUMP_NEXT,
                bump_end: BUMP_END,
            });
        }
        let off = WIRE_SOURCES_OFFSET + 40;
        arena[off..off + 8].copy_from_slice(&1u64.to_le_bytes());
        let scalars = decode_envelope(&arena).unwrap();
        let mut out = [RECORDS[0]; 1];
        assert!(matches!(
            decode_sources(&arena, &scalars, &mut out),
            Err(HandoffV1ReadError::BadSourceRecord)
        ));
    }

    #[test]
    fn decoded_payloads_survive_arena_poison() {
        let mut arena = build_arena();
        let s = decode_envelope(&arena).unwrap();

        let mut free = [MemoryRegion {
            base: PhysBytes(0),
            len: 0,
        }; WIRE_MEMMAP_CAP];
        let n_free = decode_memmap(&arena, &s, &mut free).unwrap();
        let mut reserved = [MemoryRegion {
            base: PhysBytes(0),
            len: 0,
        }; WIRE_RESERVED_CAP];
        let n_reserved = decode_reserved_regions(&arena, &s, &mut reserved).unwrap();
        let mut wire_modules = [WireBootModule {
            name: [0; 16],
            start: 0,
            len: 0,
        }; WIRE_MODULE_CAP];
        let n_modules = decode_modules(&arena, &s, &mut wire_modules).unwrap();
        let mut sources = [PlatformDescSource::new(DTB, PhysBytes(0)); WIRE_PLATFORM_CAP];
        let n_sources = decode_platform_sources(&arena, &s, &mut sources).unwrap();
        let mut param = [0u8; WIRE_PARAM_CAP];
        let n_param = decode_param_buf(&arena, &s, &mut param).unwrap();
        let mut records = [SourceRecord {
            input_object: crate::memmap_source::InputObjectId::DtbBootInput,
            address_domain: crate::memmap_source::AddressDomain::Physical,
            local_source: 0,
            snapshot_id: 0,
            attribute: 0,
            virtual_start: 0,
            id: 0,
            raw_type: 0,
            kind: SourceKind::Memory,
            origin: SourceOrigin::MrbEntry(0),
            range: SourceRange::new(0, 0),
        }; WIRE_SOURCES_CAP];
        let n_records = decode_sources(&arena, &s, &mut records).unwrap();

        arena.fill(0xA5);

        assert_eq!(
            (n_free, n_reserved, n_modules, n_sources, n_param, n_records),
            (2, 1, 2, 2, PARAM.len(), RECORDS.len())
        );
        for (i, landed) in free[..n_free].iter().enumerate() {
            assert_eq!(
                (landed.base.0, landed.len),
                (MEMMAP[i].base.0, MEMMAP[i].len)
            );
        }
        for (i, landed) in reserved[..n_reserved].iter().enumerate() {
            assert_eq!(
                (landed.base.0, landed.len),
                (RESERVED[i].base.0, RESERVED[i].len)
            );
        }
        for (wire, module) in wire_modules[..n_modules].iter().zip(MODULES.iter()) {
            assert_eq!(wire.start, module.start.0);
            assert_eq!(wire.len, module.len as u64);
            let mut name = [0u8; 16];
            name[..module.name.len()].copy_from_slice(module.name.as_bytes());
            assert_eq!(wire.name, name);
        }
        for (i, landed) in sources[..n_sources].iter().enumerate() {
            assert_eq!(landed.kind(), SOURCES[i].kind());
            assert_eq!(landed.phys_addr(), SOURCES[i].phys_addr());
        }
        assert_eq!(&param[..n_param], PARAM);
        assert_eq!(n_records, RECORDS.len());
        assert_eq!(&records[..n_records], &RECORDS[..]);
    }
}
