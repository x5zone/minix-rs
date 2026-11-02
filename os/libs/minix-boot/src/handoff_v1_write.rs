//! Producer side of the v1 cross-image handoff protocol.
//!
//! A [`Serializer`] owns a caller-provided arena (`&mut [u8]`) and writes the
//! v1 wire image into it at the fixed offsets from [`crate::handoff_v1`]:
//! envelope first, then the wire partition (memmap / reserved regions / boot
//! modules / platform sources / param bytes), with every memory-map entry
//! dual-written into the semantic work partition (`MemoryRegion`, shim-side
//! supply; the receiver ignores it).
//!
//! The arena is a plain byte slice here on purpose: the boot legs construct it
//! from their `AllocatePages` result (the only `unsafe` step, documented at
//! the leg), while host tests exercise the exact same serializer against a
//! stack buffer. Byte order is pinned little-endian by the protocol.
//!
//! Timing (design §1.5 (6).5): the envelope and all non-memory-map payloads
//! are written before `ExitBootServices`; memory-map entries are pushed from
//! the EBS pairing loop; the runtime fields (`root_page`, bump triple) are
//! patched in place after the root switch.

use crate::handoff_v1::{
    ARENA_CAP, BOOT_HANDOFF_V1_MAGIC, BOOT_HANDOFF_V1_VERSION, BootHandoffV1,
    FREE_UPPER_IDX_NONE, SEMANTIC_END, SEMANTIC_MEMMAP_OFFSET, SEMANTIC_RESERVED_OFFSET,
    WIRE_MEMMAP_CAP, WIRE_MEMMAP_OFFSET, WIRE_MODULE_CAP,
    WIRE_MODULES_OFFSET, WIRE_PARAM_CAP, WIRE_PARAM_OFFSET, WIRE_PLATFORM_CAP,
    WIRE_PLATFORM_OFFSET, WIRE_RESERVED_CAP, WIRE_RESERVED_OFFSET,
};
use crate::kernel_info::{BootModule, KernelInfo, MemoryRegion};
use crate::platform::PlatformDescSource;
use core::mem::size_of;

/// Errors the producer can report. All of them are fail-fast: the sender is
/// the only party that can still print, and an over-capacity payload is a
/// build/contract bug rather than something to truncate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandoffV1WriteError {
    /// The supplied buffer is smaller than [`ARENA_CAP`].
    ArenaTooSmall,
    /// More memory-map entries than the wire partition can hold.
    TooManyMemmap,
    /// More reserved regions than the wire partition can hold.
    TooManyReservedRegions,
    /// More boot modules than the wire partition can hold.
    TooManyBootModules,
    /// More platform sources than the wire partition can hold.
    TooManyPlatformSources,
    /// A module name does not fit the 15-byte + NUL protocol slot.
    ModuleNameTooLong,
    /// `param_buf` bytes exceed the 1024-byte protocol capacity.
    ParamTooLong,
}

/// Runtime handoff fields, patched in after the root switch (the bump cursor
/// is only known then).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeFields {
    pub root_page: u64,
    pub bump_base: u64,
    pub bump_next: u64,
    pub bump_end: u64,
}

/// Envelope field offsets (kept next to the writer so a layout change breaks
/// the compile-time gate in [`crate::handoff_v1`] and this file together).
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

fn put_u32(arena: &mut [u8], off: usize, value: u32) {
    arena[off..off + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(arena: &mut [u8], off: usize, value: u64) {
    arena[off..off + 8].copy_from_slice(&value.to_le_bytes());
}

/// Patch the runtime fields into an arena that is already populated.
///
/// Unlike [`Serializer::new`], this does not zero the arena and does not
/// require producer state: the sender-side entry uses it after the bootstrap
/// root is live, when only the four runtime fields remain to be filled.
pub fn patch_runtime_fields_in(arena: &mut [u8], fields: RuntimeFields) {
    assert!(
        arena.len() >= ARENA_CAP,
        "handoff arena smaller than ARENA_CAP"
    );
    put_u64(arena, offsets::ROOT_PAGE, fields.root_page);
    put_u64(arena, offsets::BUMP_BASE, fields.bump_base);
    put_u64(arena, offsets::BUMP_NEXT, fields.bump_next);
    put_u64(arena, offsets::BUMP_END, fields.bump_end);
}

/// Arena writer. Construction zeroes the whole protocol region so stale bytes
/// cannot leak into any partition.
pub struct Serializer<'a> {
    arena: &'a mut [u8],
    wire_memmap_n: usize,
    reserved_n: usize,
    modules_n: usize,
    platform_n: usize,
    param_n: usize,
}

impl<'a> Serializer<'a> {
    /// Wrap an arena buffer. The buffer must be at least [`ARENA_CAP`] bytes;
    /// only the first `ARENA_CAP` bytes are used.
    pub fn new(arena: &'a mut [u8]) -> Result<Self, HandoffV1WriteError> {
        if arena.len() < ARENA_CAP {
            return Err(HandoffV1WriteError::ArenaTooSmall);
        }
        for byte in &mut arena[..ARENA_CAP] {
            *byte = 0;
        }
        Ok(Self {
            arena,
            wire_memmap_n: 0,
            reserved_n: 0,
            modules_n: 0,
            platform_n: 0,
            param_n: 0,
        })
    }

    /// Write the protocol header and the kernel scalars. Runtime fields and
    /// payload descriptors stay zero until written (valid intermediate states:
    /// empty payloads are legal).
    pub fn write_envelope(&mut self, info: &KernelInfo) -> Result<(), HandoffV1WriteError> {
        put_u32(self.arena, offsets::MAGIC, BOOT_HANDOFF_V1_MAGIC);
        put_u32(self.arena, offsets::VERSION, BOOT_HANDOFF_V1_VERSION);
        put_u32(
            self.arena,
            offsets::SIZE,
            size_of::<BootHandoffV1>() as u32,
        );
        put_u64(self.arena, offsets::ARENA_LEN, ARENA_CAP as u64);
        put_u64(self.arena, offsets::KERN_VIRT_BASE, info.kern_virt_base.0);
        put_u64(self.arena, offsets::KERN_PHYS_BASE, info.kern_phys_base.0);
        put_u64(self.arena, offsets::KERN_SIZE, info.kern_size);
        let free_upper = match info.free_upper_idx {
            Some(idx) => idx as u64,
            None => FREE_UPPER_IDX_NONE,
        };
        put_u64(self.arena, offsets::FREE_UPPER_IDX, free_upper);
        put_u64(self.arena, offsets::USER_SP, info.user_sp.0);
        put_u64(self.arena, offsets::KERN_STACK_TOP, info.kern_stack_top.0);
        put_u64(self.arena, offsets::SYSCALL_ENTRY, info.syscall_entry.0);
        put_u64(self.arena, offsets::BOOTSTRAP_START, info.bootstrap_start.0);
        put_u64(self.arena, offsets::BOOTSTRAP_LEN, info.bootstrap_len);
        Ok(())
    }

    /// Patch the runtime fields in place (after the root switch, before the
    /// jump). Idempotent.
    pub fn patch_runtime_fields(&mut self, fields: RuntimeFields) {
        patch_runtime_fields_in(self.arena, fields);
    }

    /// Serialize the reserved regions into the wire partition and dual-write
    /// them into the semantic work partition (shim-side supply).
    pub fn write_reserved_regions(
        &mut self,
        regions: &[MemoryRegion],
    ) -> Result<(), HandoffV1WriteError> {
        if regions.len() > WIRE_RESERVED_CAP {
            return Err(HandoffV1WriteError::TooManyReservedRegions);
        }
        for (i, region) in regions.iter().enumerate() {
            self.write_wire_region(WIRE_RESERVED_OFFSET + i * 16, region);
            self.write_semantic_region(SEMANTIC_RESERVED_OFFSET + i * 16, region);
        }
        self.reserved_n = regions.len();
        put_u64(
            self.arena,
            offsets::RESERVED_BYTE_LEN,
            (regions.len() * 16) as u64,
        );
        Ok(())
    }

    /// Serialize the boot modules (fixed 16-byte name slot, NUL padded).
    pub fn write_boot_modules(
        &mut self,
        modules: &[BootModule],
    ) -> Result<(), HandoffV1WriteError> {
        if modules.len() > WIRE_MODULE_CAP {
            return Err(HandoffV1WriteError::TooManyBootModules);
        }
        for (i, module) in modules.iter().enumerate() {
            let name = module.name.as_bytes();
            if name.len() > 15 {
                return Err(HandoffV1WriteError::ModuleNameTooLong);
            }
            let off = WIRE_MODULES_OFFSET + i * 32;
            for byte in &mut self.arena[off..off + 16] {
                *byte = 0;
            }
            self.arena[off..off + name.len()].copy_from_slice(name);
            put_u64(self.arena, off + 16, module.start.0);
            put_u64(self.arena, off + 24, module.len as u64);
        }
        self.modules_n = modules.len();
        put_u64(
            self.arena,
            offsets::MODULES_BYTE_LEN,
            (modules.len() * 32) as u64,
        );
        Ok(())
    }

    /// Serialize the platform descriptor sources (kind as a bare `u32`).
    pub fn write_platform_sources(
        &mut self,
        sources: &[PlatformDescSource],
    ) -> Result<(), HandoffV1WriteError> {
        if sources.len() > WIRE_PLATFORM_CAP {
            return Err(HandoffV1WriteError::TooManyPlatformSources);
        }
        for (i, source) in sources.iter().enumerate() {
            let off = WIRE_PLATFORM_OFFSET + i * 16;
            put_u32(self.arena, off, source.kind().raw());
            put_u32(self.arena, off + 4, 0);
            put_u64(self.arena, off + 8, source.phys_addr().0);
        }
        self.platform_n = sources.len();
        put_u64(
            self.arena,
            offsets::PLATFORM_BYTE_LEN,
            (sources.len() * 16) as u64,
        );
        Ok(())
    }

    /// Serialize the `param_buf` bytes verbatim.
    pub fn write_param_buf(&mut self, bytes: &[u8]) -> Result<(), HandoffV1WriteError> {
        if bytes.len() > WIRE_PARAM_CAP {
            return Err(HandoffV1WriteError::ParamTooLong);
        }
        // The partition is fixed-size; a second, shorter write must not leave
        // the previous tail visible past `param_n` (defensive: current
        // callers write once).
        self.arena[WIRE_PARAM_OFFSET..WIRE_PARAM_OFFSET + WIRE_PARAM_CAP].fill(0);
        self.arena[WIRE_PARAM_OFFSET..WIRE_PARAM_OFFSET + bytes.len()].copy_from_slice(bytes);
        self.param_n = bytes.len();
        put_u64(self.arena, offsets::PARAM_BYTE_LEN, bytes.len() as u64);
        Ok(())
    }

    /// Dual-write one memory-map entry: `WireMemoryRegion` into the wire
    /// partition and `MemoryRegion` into the semantic work partition. Called
    /// from the EBS pairing loop once per classified descriptor.
    pub fn push_memmap(&mut self, region: MemoryRegion) -> Result<(), HandoffV1WriteError> {
        if self.wire_memmap_n >= WIRE_MEMMAP_CAP {
            return Err(HandoffV1WriteError::TooManyMemmap);
        }
        let off = self.wire_memmap_n * 16;
        self.write_wire_region(WIRE_MEMMAP_OFFSET + off, &region);
        self.write_semantic_region(SEMANTIC_MEMMAP_OFFSET + off, &region);
        self.wire_memmap_n += 1;
        Ok(())
    }

    /// Finalize the memory-map descriptor after the pairing loop.
    pub fn finish_memmap(&mut self) {
        put_u64(
            self.arena,
            offsets::MEMMAP_BYTE_LEN,
            (self.wire_memmap_n * 16) as u64,
        );
    }

    /// Dual-write one classified reserved range (non-conventional map
    /// entries). Same partition discipline as [`Self::push_memmap`].
    pub fn push_reserved(&mut self, region: MemoryRegion) -> Result<(), HandoffV1WriteError> {
        if self.reserved_n >= WIRE_RESERVED_CAP {
            return Err(HandoffV1WriteError::TooManyReservedRegions);
        }
        let off = self.reserved_n * 16;
        self.write_wire_region(WIRE_RESERVED_OFFSET + off, &region);
        self.write_semantic_region(SEMANTIC_RESERVED_OFFSET + off, &region);
        self.reserved_n += 1;
        Ok(())
    }

    /// Finalize the reserved descriptor after the classification loop.
    pub fn finish_reserved(&mut self) {
        put_u64(
            self.arena,
            offsets::RESERVED_BYTE_LEN,
            (self.reserved_n * 16) as u64,
        );
    }

    /// Clear both map partitions and their descriptors. The EBS pairing loop
    /// calls this before re-classifying after a failed `ExitBootServices`:
    /// only the classification paired with a successful MapKey may survive,
    /// so stale entries must not linger.
    pub fn reset_map_partitions(&mut self) {
        for byte in &mut self.arena
            [WIRE_MEMMAP_OFFSET..WIRE_RESERVED_OFFSET + WIRE_RESERVED_CAP * 16]
        {
            *byte = 0;
        }
        for byte in &mut self.arena[SEMANTIC_MEMMAP_OFFSET..SEMANTIC_END] {
            *byte = 0;
        }
        self.wire_memmap_n = 0;
        self.reserved_n = 0;
        put_u64(self.arena, offsets::MEMMAP_BYTE_LEN, 0);
        put_u64(self.arena, offsets::RESERVED_BYTE_LEN, 0);
    }

    /// The raw arena bytes (tests and diagnostics).
    pub fn as_bytes(&self) -> &[u8] {
        self.arena
    }

    fn write_wire_region(&mut self, off: usize, region: &MemoryRegion) {
        put_u64(self.arena, off, region.base.0);
        put_u64(self.arena, off + 8, region.len as u64);
    }

    fn write_semantic_region(&mut self, off: usize, region: &MemoryRegion) {
        // The semantic partition stores the internal type verbatim; its
        // layout is pinned by the compile-time gate in `handoff_v1` (16 B on
        // every supported 64-bit target, element-aligned offset).
        let bytes = unsafe {
            // SAFETY: `MemoryRegion` is `#[repr(C)]` with two 8-byte fields;
            // the const gate above pins size/alignment, and the partition
            // offset is element-aligned by the same gate.
            core::slice::from_raw_parts(
                region as *const MemoryRegion as *const u8,
                size_of::<MemoryRegion>(),
            )
        };
        self.arena[off..off + bytes.len()].copy_from_slice(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handoff_v1::{
        ENVELOPE_CAP, SEMANTIC_MEMMAP_CAP, SEMANTIC_MEMMAP_OFFSET, WIRE_MEMMAP_OFFSET,
        WIRE_MODULES_OFFSET, WIRE_PARAM_OFFSET, WIRE_PLATFORM_OFFSET, WIRE_RESERVED_OFFSET,
    };
    use crate::kernel_info::MemoryRegion;
    use crate::platform::{PlatformDescSource, DTB, RSDP};
    use minix_types::{PhysBytes, USER_STACK_TOP, VirBytes};

    fn get_u32(arena: &[u8], off: usize) -> u32 {
        u32::from_le_bytes(arena[off..off + 4].try_into().unwrap())
    }

    fn get_u64(arena: &[u8], off: usize) -> u64 {
        u64::from_le_bytes(arena[off..off + 8].try_into().unwrap())
    }

    fn base_info() -> KernelInfo {
        KernelInfo {
            memmap: &[],
            reserved_regions: &[],
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x20_0000),
            kern_size: 0x20_0000,
            free_upper_idx: None,
            user_sp: VirBytes(USER_STACK_TOP as u64),
            kern_stack_top: VirBytes(0xFFFF_8000_0040_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: &[],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
            platform_sources: &[],
            param_buf: &[],
        }
    }

    fn region(base: u64, len: u64) -> MemoryRegion {
        MemoryRegion {
            base: PhysBytes(base),
            len: len as usize,
        }
    }

    #[test]
    fn envelope_carries_header_and_scalars() {
        let mut arena = [0u8; ARENA_CAP];
        {
            let mut s = Serializer::new(&mut arena).unwrap();
            s.write_envelope(&base_info()).unwrap();
        }
        assert_eq!(get_u32(&arena, 0), BOOT_HANDOFF_V1_MAGIC);
        assert_eq!(get_u32(&arena, 4), BOOT_HANDOFF_V1_VERSION);
        assert_eq!(
            get_u32(&arena, 8),
            core::mem::size_of::<BootHandoffV1>() as u32
        );
        assert_eq!(get_u64(&arena, 16), ARENA_CAP as u64);
        assert_eq!(get_u64(&arena, 24), 0xFFFF_8000_0000_0000);
        assert_eq!(get_u64(&arena, 32), 0x20_0000);
        assert_eq!(get_u64(&arena, 40), 0x20_0000);
        assert_eq!(get_u64(&arena, 48), FREE_UPPER_IDX_NONE);
        // Descriptors start empty.
        for off in [128usize, 136, 144, 152, 160] {
            assert_eq!(get_u64(&arena, off), 0);
        }
        assert!(ENVELOPE_CAP >= core::mem::size_of::<BootHandoffV1>());
    }

    #[test]
    fn runtime_fields_are_patched_in_place() {
        let mut arena = [0u8; ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        s.write_envelope(&base_info()).unwrap();
        s.patch_runtime_fields(RuntimeFields {
            root_page: 0x1000,
            bump_base: 0x20_0000,
            bump_next: 0x21_0000,
            bump_end: 0x30_0000,
        });
        assert_eq!(get_u64(s.as_bytes(), 96), 0x1000);
        assert_eq!(get_u64(s.as_bytes(), 104), 0x20_0000);
        assert_eq!(get_u64(s.as_bytes(), 112), 0x21_0000);
        assert_eq!(get_u64(s.as_bytes(), 120), 0x30_0000);
    }

    /// The free function patches a fully serialized arena without zeroing it:
    /// every other byte (header, scalars, payload descriptors) must survive.
    #[test]
    fn patch_runtime_fields_in_preserves_the_rest_of_the_arena() {
        let mut arena = [0u8; ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        s.write_envelope(&base_info()).unwrap();
        s.write_param_buf(b"ARCH=test\0").unwrap();
        let mut before = [0u8; ARENA_CAP];
        before.copy_from_slice(s.as_bytes());
        drop(s);
        patch_runtime_fields_in(
            &mut arena,
            RuntimeFields {
                root_page: 0x9000,
                bump_base: 0x40_0000,
                bump_next: 0x40_1000,
                bump_end: 0x50_0000,
            },
        );
        assert_eq!(get_u32(&arena, 0), BOOT_HANDOFF_V1_MAGIC);
        assert_eq!(get_u64(&arena, 24), 0xFFFF_8000_0000_0000);
        assert_eq!(get_u64(&arena, 160), 10);
        for (i, (old, new)) in before.iter().zip(arena.iter()).enumerate() {
            if (96..128).contains(&i) {
                continue;
            }
            assert_eq!(old, new, "byte {i} outside the runtime fields changed");
        }
        assert_eq!(get_u64(&arena, 96), 0x9000);
        assert_eq!(get_u64(&arena, 104), 0x40_0000);
        assert_eq!(get_u64(&arena, 112), 0x40_1000);
        assert_eq!(get_u64(&arena, 120), 0x50_0000);
    }

    #[test]
    fn memmap_is_dual_written_wire_and_semantic() {
        let mut arena = [0u8; ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        s.write_envelope(&base_info()).unwrap();
        s.push_memmap(region(0x10_0000, 0x1000)).unwrap();
        s.push_memmap(region(0x20_0000, 0x2000)).unwrap();
        s.finish_memmap();
        let b = s.as_bytes();
        // Wire partition: (base, len) pairs.
        assert_eq!(get_u64(b, WIRE_MEMMAP_OFFSET), 0x10_0000);
        assert_eq!(get_u64(b, WIRE_MEMMAP_OFFSET + 8), 0x1000);
        assert_eq!(get_u64(b, WIRE_MEMMAP_OFFSET + 16), 0x20_0000);
        assert_eq!(get_u64(b, WIRE_MEMMAP_OFFSET + 24), 0x2000);
        // Semantic partition: same values in the internal type layout.
        assert_eq!(get_u64(b, SEMANTIC_MEMMAP_OFFSET), 0x10_0000);
        assert_eq!(get_u64(b, SEMANTIC_MEMMAP_OFFSET + 8), 0x1000);
        // Descriptor = count × 16.
        assert_eq!(get_u64(b, 128), 32);
    }

    #[test]
    fn reserved_regions_fill_both_partitions_and_descriptor() {
        let mut arena = [0u8; ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        s.write_envelope(&base_info()).unwrap();
        s.write_reserved_regions(&[region(0x1DA0_0000, 0x3000)])
            .unwrap();
        let b = s.as_bytes();
        assert_eq!(get_u64(b, WIRE_RESERVED_OFFSET), 0x1DA0_0000);
        assert_eq!(get_u64(b, WIRE_RESERVED_OFFSET + 8), 0x3000);
        assert_eq!(get_u64(b, SEMANTIC_RESERVED_OFFSET), 0x1DA0_0000);
        assert_eq!(get_u64(b, 136), 16);
    }

    #[test]
    fn modules_use_fixed_name_slot_and_nul_padding() {
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
        let mut arena = [0u8; ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        s.write_envelope(&base_info()).unwrap();
        s.write_boot_modules(&MODULES).unwrap();
        let b = s.as_bytes();
        assert_eq!(&b[WIRE_MODULES_OFFSET..WIRE_MODULES_OFFSET + 2], b"vm");
        assert!(b[WIRE_MODULES_OFFSET + 2..WIRE_MODULES_OFFSET + 16]
            .iter()
            .all(|&x| x == 0));
        assert_eq!(get_u64(b, WIRE_MODULES_OFFSET + 16), 0x20_0000);
        assert_eq!(get_u64(b, WIRE_MODULES_OFFSET + 24), 0x1000);
        let second = WIRE_MODULES_OFFSET + 32;
        assert_eq!(&b[second..second + 6], b"memory");
        assert_eq!(get_u64(b, 144), 64);
    }

    #[test]
    fn platform_sources_carry_raw_kind_and_address() {
        static SOURCES: [PlatformDescSource; 2] = [
            PlatformDescSource::new(DTB, PhysBytes(0x4000_0000)),
            PlatformDescSource::new(RSDP, PhysBytes(0xE0000)),
        ];
        let mut arena = [0u8; ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        s.write_envelope(&base_info()).unwrap();
        s.write_platform_sources(&SOURCES).unwrap();
        let b = s.as_bytes();
        assert_eq!(get_u32(b, WIRE_PLATFORM_OFFSET), DTB.raw());
        assert_eq!(get_u32(b, WIRE_PLATFORM_OFFSET + 4), 0);
        assert_eq!(get_u64(b, WIRE_PLATFORM_OFFSET + 8), 0x4000_0000);
        assert_eq!(get_u32(b, WIRE_PLATFORM_OFFSET + 16), RSDP.raw());
        assert_eq!(get_u64(b, 152), 32);
    }

    #[test]
    fn param_buf_is_copied_verbatim() {
        let payload = b"ARCH=x86_64\0BOARD=qemu\0";
        let mut arena = [0u8; ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        s.write_param_buf(payload).unwrap();
        assert_eq!(
            &s.as_bytes()[WIRE_PARAM_OFFSET..WIRE_PARAM_OFFSET + payload.len()],
            payload
        );
        assert_eq!(get_u64(s.as_bytes(), 160), payload.len() as u64);
    }

    #[test]
    fn over_capacity_payloads_fail_fast() {
        let mut arena = [0u8; ARENA_CAP];
        let mut s = Serializer::new(&mut arena).unwrap();
        // param > 1024
        let big = [0u8; WIRE_PARAM_CAP + 1];
        assert_eq!(
            s.write_param_buf(&big),
            Err(HandoffV1WriteError::ParamTooLong)
        );
        // 13 modules
        let modules = [BootModule {
            name: "m",
            start: PhysBytes(0),
            len: 0,
        }; WIRE_MODULE_CAP + 1];
        assert_eq!(
            s.write_boot_modules(&modules),
            Err(HandoffV1WriteError::TooManyBootModules)
        );
        // 16-byte name (no room for NUL)
        let long = [BootModule {
            name: "0123456789abcdef",
            start: PhysBytes(0),
            len: 0,
        }; 1];
        assert_eq!(
            s.write_boot_modules(&long),
            Err(HandoffV1WriteError::ModuleNameTooLong)
        );
        // 129 memmap entries
        for _ in 0..WIRE_MEMMAP_CAP {
            s.push_memmap(region(0, 0x1000)).unwrap();
        }
        assert_eq!(
            s.push_memmap(region(0, 0x1000)),
            Err(HandoffV1WriteError::TooManyMemmap)
        );
    }

    #[test]
    fn short_arena_is_rejected() {
        let mut arena = [0u8; ARENA_CAP - 1];
        assert!(matches!(
            Serializer::new(&mut arena),
            Err(HandoffV1WriteError::ArenaTooSmall)
        ));
    }

    #[test]
    fn semantic_partition_capacity_matches_wire_count() {
        // Both partitions hold the same entry count; the compile-time gate in
        // `handoff_v1` pins the byte capacities, this test pins the coupling.
        assert_eq!(SEMANTIC_MEMMAP_CAP, WIRE_MEMMAP_CAP);
    }
}
