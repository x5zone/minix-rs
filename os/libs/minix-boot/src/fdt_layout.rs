//! Devicetree container validation, done before any parser sees the blob.
//!
//! The `fdt` crate accepts a blob after checking only the magic and
//! `data.len() >= totalsize`; block offsets, the memory-reservation block and
//! the region boundaries are not validated there. This module checks the
//! container layout once, so the parser and the reservation reader only ever
//! see a shape whose blocks are inside the object, non-overlapping and
//! terminated where the specification says.
//!
//! The entry point takes the physical address because the specification also
//! requires the blob itself to be 8-byte aligned — a slice cannot express that.

/// The validated layout of one devicetree blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FdtLayout {
    pub totalsize: u32,
    /// Start of the memory reservation block (8-byte aligned).
    pub mrb_start: u32,
    /// End of the `{0, 0}` terminator.
    pub mrb_end: u32,
    pub struct_start: u32,
    pub struct_end: u32,
    pub strings_start: u32,
    pub strings_end: u32,
}

/// Why a blob was refused. Every variant stops the boot; the parser is never
/// handed a partially validated object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FdtInputError {
    /// Shorter than the fixed 40-byte header.
    TooSmall,
    /// The magic word is not `0xd00dfeed`.
    BadMagic,
    /// The blob's physical address is not 8-byte aligned.
    BlobMisaligned,
    /// `totalsize` is smaller than the header, larger than the accepted
    /// project bound, or larger than the provided buffer.
    BadTotalsize,
    /// A block offset plus its size overflows 32 bits.
    BlockOverflow,
    /// A block reaches past `totalsize`.
    BlockOutOfRange,
    /// Two blocks overlap.
    BlockOverlap,
    /// `off_mem_rsvmap` is not 8-byte aligned.
    MrbMisaligned,
    /// The memory-reservation block has no `{0, 0}` terminator before the
    /// next block or the end of the object.
    MrbNoTerminator,
    /// A reservation record would cross the scan boundary.
    MrbRecordTruncated,
    /// A block that must be 4-byte aligned is not (the structure block; the
    /// strings block has no alignment requirement).
    BlockMisaligned,
    /// The structure block does not end with the `FDT_END` token.
    MissingEndToken,
    /// The memory reservation block holds more entries than the collector
    /// accepts.
    TooManyReservations,
    /// The header's `version` / `last_comp_version` is outside the set this
    /// project accepts.
    UnsupportedVersion,
    /// The blocks are not in the order this project accepts: header, then the
    /// memory reservation block, then the structure and strings blocks. The
    /// specification only recommends this order, so rejecting other layouts is
    /// a project input restriction, not a specification requirement.
    UnsupportedBlockOrder,
}

const HEADER_LEN: u32 = 40;
const MAGIC: u32 = 0xd00d_feed;
const MRB_ENTRY: u32 = 16;
/// Maximum reservation entries the collector accepts; enforced here so a huge
/// but "valid" MRB cannot make the boot scan for an unbounded time.
const MRB_MAX_ENTRIES: u32 = 16;
/// Binary structure versions this project accepts: `version = 17` with
/// `last_comp_version = 16`, the pair the current format description defines.
/// The specification asks clients to accept every blob backward-compatible
/// with the current version, so this is a **project input restriction**: boot
/// paths are pinned to this pair and a future `version > 17` blob is refused
/// rather than guessed at.
const FDT_VERSION: u32 = 17;
const FDT_LAST_COMP_VERSION: u32 = 16;
/// Upper bound this project accepts for a devicetree blob.
pub const FDT_MAX_LEN: u32 = crate::DTB_MAX_LEN;

fn be_u32(bytes: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
}

fn block(start: u32, size: u32, total: u32) -> Result<(u32, u32), FdtInputError> {
    let end = start.checked_add(size).ok_or(FdtInputError::BlockOverflow)?;
    if end > total {
        return Err(FdtInputError::BlockOutOfRange);
    }
    Ok((start, end))
}

fn overlaps(a: (u32, u32), b: (u32, u32)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

/// Validate one blob at physical address `addr`.
pub fn validate(addr: u64, bytes: &[u8]) -> Result<FdtLayout, FdtInputError> {
    if bytes.len() < HEADER_LEN as usize {
        return Err(FdtInputError::TooSmall);
    }
    if be_u32(bytes, 0) != MAGIC {
        return Err(FdtInputError::BadMagic);
    }
    if !crate::dtb_blob_aligned(addr) {
        return Err(FdtInputError::BlobMisaligned);
    }
    let version = be_u32(bytes, 20);
    let last_comp = be_u32(bytes, 24);
    if version != FDT_VERSION || last_comp != FDT_LAST_COMP_VERSION {
        return Err(FdtInputError::UnsupportedVersion);
    }
    let totalsize = be_u32(bytes, 4);
    if totalsize < HEADER_LEN
        || totalsize > FDT_MAX_LEN
        || totalsize as usize > bytes.len()
    {
        return Err(FdtInputError::BadTotalsize);
    }

    let off_struct = be_u32(bytes, 8);
    let off_strings = be_u32(bytes, 12);
    let off_mrb = be_u32(bytes, 16);
    let size_strings = be_u32(bytes, 32);
    let size_struct = be_u32(bytes, 36);

    let header = (0, HEADER_LEN);
    let struct_block = block(off_struct, size_struct, totalsize)?;
    let strings_block = block(off_strings, size_strings, totalsize)?;
    if overlaps(struct_block, strings_block) {
        return Err(FdtInputError::BlockOverlap);
    }
    if off_struct % 4 != 0 {
        return Err(FdtInputError::BlockMisaligned);
    }
    // The structure block must end with the FDT_END token (0x9): a truncated
    // or foreign tail is refused before any token walk starts.
    if size_struct < 4 || be_u32(bytes, (struct_block.1 - 4) as usize) != 9 {
        return Err(FdtInputError::MissingEndToken);
    }

    // The reservation block must start after the header, be 8-byte aligned,
    // and end (with its terminator) before the next block or the object end.
    if off_mrb % 8 != 0 {
        return Err(FdtInputError::MrbMisaligned);
    }
    if off_mrb < HEADER_LEN {
        return Err(FdtInputError::BlockOutOfRange);
    }
    // The block always carries at least its terminator; a start that cannot
    // even hold one record is not a block inside this object.
    if off_mrb
        .checked_add(MRB_ENTRY)
        .map_or(true, |end| end > totalsize)
    {
        return Err(FdtInputError::BlockOutOfRange);
    }
    // Project-accepted layout: reservation block, then structure block, then
    // strings block. The specification only recommends this order.
    if off_mrb >= off_struct || struct_block.1 > off_strings {
        return Err(FdtInputError::UnsupportedBlockOrder);
    }
    let scan_limit = off_struct;
    let mut pos = off_mrb;
    let mrb_end = loop {
        let end = pos
            .checked_add(MRB_ENTRY)
            .ok_or(FdtInputError::BlockOverflow)?;
        if end > scan_limit {
            return Err(FdtInputError::MrbNoTerminator);
        }
        if end as usize > bytes.len() {
            return Err(FdtInputError::MrbRecordTruncated);
        }
        let address = be_u32(bytes, pos as usize) as u64;
        let high_addr = be_u32(bytes, pos as usize + 4) as u64;
        let size = be_u32(bytes, pos as usize + 8) as u64;
        let high_size = be_u32(bytes, pos as usize + 12) as u64;
        if address == 0 && high_addr == 0 && size == 0 && high_size == 0 {
            break end;
        }
        if (pos - off_mrb) / MRB_ENTRY + 1 >= MRB_MAX_ENTRIES {
            return Err(FdtInputError::TooManyReservations);
        }
        pos = end;
    };

    let mrb = block(off_mrb, mrb_end - off_mrb, totalsize)?;
    if overlaps(mrb, struct_block)
        || overlaps(mrb, strings_block)
        || overlaps(mrb, header)
    {
        return Err(FdtInputError::BlockOverlap);
    }
    if overlaps(header, struct_block) || overlaps(header, strings_block) {
        return Err(FdtInputError::BlockOverlap);
    }

    Ok(FdtLayout {
        totalsize,
        mrb_start: off_mrb,
        mrb_end,
        struct_start: struct_block.0,
        struct_end: struct_block.1,
        strings_start: strings_block.0,
        strings_end: strings_block.1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{vec, vec::Vec};

    /// Minimal canonical blob: header + one `{0,0}` terminator + one-byte
    /// struct/strings blocks. `patch` may rewrite header fields afterwards.
    fn blob(off_mrb: u32, struct_start: u32, struct_size: u32, strings_start: u32, strings_size: u32) -> Vec<u8> {
        let total = strings_start + strings_size;
        let mut out = vec![0u8; total as usize];
        out[0..4].copy_from_slice(&MAGIC.to_be_bytes());
        out[4..8].copy_from_slice(&total.to_be_bytes());
        out[8..12].copy_from_slice(&struct_start.to_be_bytes());
        out[12..16].copy_from_slice(&strings_start.to_be_bytes());
        out[16..20].copy_from_slice(&off_mrb.to_be_bytes());
        out[20..24].copy_from_slice(&17u32.to_be_bytes());
        out[24..28].copy_from_slice(&16u32.to_be_bytes());
        out[32..36].copy_from_slice(&strings_size.to_be_bytes());
        out[36..40].copy_from_slice(&struct_size.to_be_bytes());
        if struct_size >= 4 {
            let end = struct_start + struct_size - 4;
            out[end as usize..(end + 4) as usize].copy_from_slice(&9u32.to_be_bytes());
        }
        out
    }

    #[test]
    fn accepts_an_unaligned_strings_block() {
        // The strings block has no alignment requirement.
        let bytes = blob(40, 56, 16, 73, 4);
        assert!(validate(0x8000_0000, &bytes).is_ok());
    }

    #[test]
    fn rejects_more_reservations_than_the_collector_accepts() {
        let mut entries = Vec::new();
        for i in 0..MRB_MAX_ENTRIES + 1 {
            entries.extend_from_slice(&(0x8000_0000u64 + i as u64 * 0x1000).to_be_bytes());
            entries.extend_from_slice(&0x1000u64.to_be_bytes());
        }
        entries.extend_from_slice(&[0u8; 16]);
        let struct_start = 40 + entries.len() as u32;
        let mut bytes = blob(40, struct_start, 16, struct_start + 16, 4);
        bytes[40..40 + entries.len()].copy_from_slice(&entries);
        assert_eq!(validate(0x8000_0000, &bytes), Err(FdtInputError::TooManyReservations));
    }

    #[test]
    fn accepts_a_canonical_layout() {
        let bytes = blob(40, 56, 8, 64, 4);
        let layout = validate(0x8000_0000, &bytes).expect("valid blob");
        assert_eq!(layout.totalsize, 68);
        assert_eq!((layout.mrb_start, layout.mrb_end), (40, 56));
        assert_eq!((layout.struct_start, layout.struct_end), (56, 64));
        assert_eq!((layout.strings_start, layout.strings_end), (64, 68));
    }

    #[test]
    fn rejects_misaligned_blob_address() {
        let bytes = blob(40, 56, 8, 64, 4);
        assert_eq!(validate(0x8000_0001, &bytes), Err(FdtInputError::BlobMisaligned));
    }

    #[test]
    fn rejects_misaligned_mrb_offset() {
        let mut bytes = blob(48, 64, 8, 72, 4);
        bytes[16..20].copy_from_slice(&49u32.to_be_bytes());
        assert_eq!(validate(0x8000_0000, &bytes), Err(FdtInputError::MrbMisaligned));
    }

    #[test]
    fn rejects_mrb_outside_the_object() {
        let bytes = blob(40, 56, 8, 64, 4);
        let mut bad = bytes.clone();
        bad[16..20].copy_from_slice(&0x1000u32.to_be_bytes());
        assert_eq!(validate(0x8000_0000, &bad), Err(FdtInputError::BlockOutOfRange));
    }

    #[test]
    fn rejects_missing_terminator_before_the_struct_block() {
        let mut bytes = blob(40, 56, 8, 64, 4);
        // Make every record look real; there is no room for a terminator.
        bytes[40..56].copy_from_slice(&[0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0]);
        assert_eq!(validate(0x8000_0000, &bytes), Err(FdtInputError::MrbNoTerminator));
    }

    #[test]
    fn rejects_a_terminator_only_beyond_totalsize() {
        // A larger buffer with a fake terminator after the object must not be
        // read: the scan stops at the struct block.
        let mut bytes = blob(40, 56, 8, 64, 4);
        bytes[40..56].copy_from_slice(&[0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0]);
        bytes.extend_from_slice(&[0u8; 16]);
        assert_eq!(validate(0x8000_0000, &bytes), Err(FdtInputError::MrbNoTerminator));
    }

    #[test]
    fn rejects_struct_block_past_totalsize() {
        let mut bytes = blob(40, 56, 8, 64, 4);
        bytes[36..40].copy_from_slice(&0x100u32.to_be_bytes());
        assert_eq!(validate(0x8000_0000, &bytes), Err(FdtInputError::BlockOutOfRange));
    }

    #[test]
    fn rejects_overlapping_struct_and_strings_blocks() {
        // struct 56..64, strings 60..72: both inside the object, overlapping.
        let bytes = blob(40, 56, 8, 60, 12);
        assert_eq!(validate(0x8000_0000, &bytes), Err(FdtInputError::BlockOverlap));
    }

    #[test]
    fn rejects_totalsize_larger_than_the_buffer() {
        let mut bytes = blob(40, 56, 8, 64, 4);
        bytes[4..8].copy_from_slice(&0x1000u32.to_be_bytes());
        assert_eq!(validate(0x8000_0000, &bytes), Err(FdtInputError::BadTotalsize));
    }

    #[test]
    fn rejects_unbacked_structure_alignment() {
        let bytes = blob(40, 57, 8, 68, 4);
        assert_eq!(validate(0x8000_0000, &bytes), Err(FdtInputError::BlockMisaligned));
    }

    #[test]
    fn rejects_a_structure_block_without_the_end_token() {
        let mut bytes = blob(40, 56, 8, 64, 4);
        bytes[60..64].copy_from_slice(&0u32.to_be_bytes());
        assert_eq!(validate(0x8000_0000, &bytes), Err(FdtInputError::MissingEndToken));
    }

    #[test]
    fn rejects_strings_before_the_structure_block() {
        let mut bytes = blob(40, 64, 8, 72, 4);
        bytes[12..16].copy_from_slice(&56u32.to_be_bytes());
        assert_eq!(validate(0x8000_0000, &bytes), Err(FdtInputError::UnsupportedBlockOrder));
    }

    #[test]
    fn rejects_unsupported_fdt_versions() {
        let mut old = blob(40, 56, 8, 64, 4);
        old[20..24].copy_from_slice(&16u32.to_be_bytes());
        assert_eq!(validate(0x8000_0000, &old), Err(FdtInputError::UnsupportedVersion));

        let mut future = blob(40, 56, 8, 64, 4);
        future[24..28].copy_from_slice(&18u32.to_be_bytes());
        assert_eq!(validate(0x8000_0000, &future), Err(FdtInputError::UnsupportedVersion));

        let mut lower = blob(40, 56, 8, 64, 4);
        lower[24..28].copy_from_slice(&15u32.to_be_bytes());
        assert_eq!(validate(0x8000_0000, &lower), Err(FdtInputError::UnsupportedVersion));
    }

    #[test]
    fn rejects_a_layout_whose_mrb_follows_the_struct_block() {
        // header 0..40, struct 40..48, strings 48..52, mrb 56..72.
        let mut bytes = blob(56, 40, 8, 48, 8);
        bytes.resize(72, 0);
        bytes[4..8].copy_from_slice(&72u32.to_be_bytes());
        assert_eq!(validate(0x8000_0000, &bytes), Err(FdtInputError::UnsupportedBlockOrder));
    }

    #[test]
    fn rejects_an_mrb_that_starts_inside_the_header() {
        let mut bytes = blob(40, 56, 8, 64, 4);
        bytes[16..20].copy_from_slice(&32u32.to_be_bytes());
        assert_eq!(validate(0x8000_0000, &bytes), Err(FdtInputError::BlockOutOfRange));
    }

    #[test]
    fn rejects_a_record_that_would_cross_the_scan_limit() {
        // Terminator starts 8 bytes before the struct block: truncated record.
        let bytes = blob(40, 48, 8, 56, 4);
        assert_eq!(validate(0x8000_0000, &bytes), Err(FdtInputError::MrbNoTerminator));
    }
}
