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
    /// A token outside the five the format defines.
    InvalidToken,
    /// The token stream does not form one tree: no root, more than one root,
    /// or unbalanced node tokens.
    BadTreeShape,
    /// Nesting deeper than the parser this project uses can hold.
    DepthLimitExceeded,
    /// A property appears after a child node of the same node. `fdt 0.1.5`
    /// stops iterating a node's children at that token, so the subtree after
    /// it would vanish silently instead of failing.
    PropertyAfterChild,
    /// A node name or property value/name runs past its block.
    StructureOutOfRange,
    /// A property name is not NUL-terminated inside the strings block.
    BadPropertyName,
    /// `FDT_END` is not the last token of the structure block.
    EndTokenNotLast,
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
    let end = start
        .checked_add(size)
        .ok_or(FdtInputError::BlockOverflow)?;
    if end > total {
        return Err(FdtInputError::BlockOutOfRange);
    }
    Ok((start, end))
}

fn overlaps(a: (u32, u32), b: (u32, u32)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

/// Deepest node nesting the parser this project uses can hold. `fdt 0.1.5`
/// keeps a 64-slot parent stack but stores the root at index 1, so the deepest
/// tree its `all_nodes()` can walk is 63 levels (root counted).
const MAX_DEPTH: u32 = 63;

/// Walk the structure block once and prove the invariants the parser relies
/// on: only defined tokens, one root, balanced nesting, every name/property
/// inside its block, names terminated in the strings block, nesting within the
/// parser's limit, properties before child nodes within a node, and `FDT_END`
/// exactly once as the last token.
///
/// The property-before-child order is load-bearing, not stylistic: `fdt 0.1.5`
/// stops `properties()` at the first non-property token and stops `children()`
/// and `all_nodes()` at a property that follows a child, so such a blob parses
/// into a silently truncated tree instead of failing.
///
/// This runs before any block of the image is guaranteed reachable, so its
/// dispatch must not lower to a jump table; the target is built with
/// `-C jump-tables=no` for that reason.
#[inline(never)]
fn validate_structure(
    bytes: &[u8],
    structure: (u32, u32),
    strings: (u32, u32),
) -> Result<(), FdtInputError> {
    const BEGIN_NODE: u32 = 1;
    const END_NODE: u32 = 2;
    const PROP: u32 = 3;
    const NOP: u32 = 4;
    const END: u32 = 9;

    let (start, end) = structure;
    let mut pos = start;
    let mut depth: u32 = 0;
    let mut seen_root = false;
    let mut seen_end = false;
    // Bit `d` is set once the node currently open at depth `d` has seen a
    // child, i.e. its property list is closed. `MAX_DEPTH` is 63, so one
    // `u64` covers every depth the walk can reach.
    let mut props_closed: u64 = 0;

    while pos < end {
        if end - pos < 4 {
            return Err(FdtInputError::StructureOutOfRange);
        }
        let token = be_u32(bytes, pos as usize);
        pos += 4;
        if seen_end {
            return Err(FdtInputError::EndTokenNotLast);
        }
        if token == BEGIN_NODE {
            if depth == 0 && seen_root {
                return Err(FdtInputError::BadTreeShape);
            }
            if depth > 0 {
                props_closed |= 1u64 << depth;
            }
            depth += 1;
            if depth > MAX_DEPTH {
                return Err(FdtInputError::DepthLimitExceeded);
            }
            props_closed &= !(1u64 << depth);
            if depth == 1 {
                seen_root = true;
            }
            let name_start = pos;
            let mut cursor = pos;
            loop {
                if cursor >= end {
                    return Err(FdtInputError::StructureOutOfRange);
                }
                if bytes[cursor as usize] == 0 {
                    break;
                }
                cursor += 1;
            }
            pos = (cursor + 1 + 3) & !3;
            if pos > end || pos < name_start {
                return Err(FdtInputError::StructureOutOfRange);
            }
        } else if token == END_NODE {
            if depth == 0 {
                return Err(FdtInputError::BadTreeShape);
            }
            depth -= 1;
        } else if token == PROP {
            if depth == 0 {
                return Err(FdtInputError::BadTreeShape);
            }
            if props_closed & (1u64 << depth) != 0 {
                return Err(FdtInputError::PropertyAfterChild);
            }
            if end - pos < 8 {
                return Err(FdtInputError::StructureOutOfRange);
            }
            let len = be_u32(bytes, pos as usize);
            let nameoff = be_u32(bytes, pos as usize + 4);
            pos += 8;
            let value_end = pos
                .checked_add(len)
                .ok_or(FdtInputError::StructureOutOfRange)?;
            let padded = (value_end + 3) & !3;
            if value_end > end || padded > end {
                return Err(FdtInputError::StructureOutOfRange);
            }
            pos = padded;
            if nameoff >= strings.1.saturating_sub(strings.0) {
                return Err(FdtInputError::BadPropertyName);
            }
            let name_start = strings.0 + nameoff;
            if name_start >= strings.1 {
                return Err(FdtInputError::BadPropertyName);
            }
            let mut found = false;
            let mut i = name_start;
            while i < strings.1 {
                if bytes[i as usize] == 0 {
                    found = true;
                    break;
                }
                i += 1;
            }
            if !found {
                return Err(FdtInputError::BadPropertyName);
            }
        } else if token == NOP {
            // No-op token; nothing to check.
        } else if token == END {
            if depth != 0 || pos != end {
                return Err(FdtInputError::EndTokenNotLast);
            }
            seen_end = true;
        } else {
            return Err(FdtInputError::InvalidToken);
        }
    }

    if !seen_root || depth != 0 || !seen_end {
        return Err(FdtInputError::BadTreeShape);
    }
    Ok(())
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
    if !(HEADER_LEN..=FDT_MAX_LEN).contains(&totalsize) || totalsize as usize > bytes.len() {
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
    if !off_struct.is_multiple_of(4) {
        return Err(FdtInputError::BlockMisaligned);
    }
    // The structure block must end with the FDT_END token (0x9): a truncated
    // or foreign tail is refused before any token walk starts.
    if size_struct < 4 || be_u32(bytes, (struct_block.1 - 4) as usize) != 9 {
        return Err(FdtInputError::MissingEndToken);
    }

    // The reservation block must start after the header, be 8-byte aligned,
    // and end (with its terminator) before the next block or the object end.
    if !off_mrb.is_multiple_of(8) {
        return Err(FdtInputError::MrbMisaligned);
    }
    if off_mrb < HEADER_LEN {
        return Err(FdtInputError::BlockOutOfRange);
    }
    // The block always carries at least its terminator; a start that cannot
    // even hold one record is not a block inside this object.
    if off_mrb
        .checked_add(MRB_ENTRY)
        .is_none_or(|end| end > totalsize)
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
        if (pos - off_mrb) / MRB_ENTRY >= MRB_MAX_ENTRIES {
            return Err(FdtInputError::TooManyReservations);
        }
        pos = end;
    };

    let mrb = block(off_mrb, mrb_end - off_mrb, totalsize)?;
    if overlaps(mrb, struct_block) || overlaps(mrb, strings_block) || overlaps(mrb, header) {
        return Err(FdtInputError::BlockOverlap);
    }
    if overlaps(header, struct_block) || overlaps(header, strings_block) {
        return Err(FdtInputError::BlockOverlap);
    }

    validate_structure(bytes, struct_block, strings_block)?;

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

    const BE: u32 = 4;

    fn token(t: u32) -> Vec<u8> {
        t.to_be_bytes().to_vec()
    }

    fn begin(name: &str) -> Vec<u8> {
        let mut out = token(1);
        out.extend_from_slice(name.as_bytes());
        out.push(0);
        while out.len() % 4 != 0 {
            out.push(0);
        }
        out
    }

    fn end_node() -> Vec<u8> {
        token(2)
    }

    fn prop(len: u32, nameoff: u32, value: &[u8]) -> Vec<u8> {
        let mut out = token(3);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&nameoff.to_be_bytes());
        out.extend_from_slice(value);
        while out.len() % 4 != 0 {
            out.push(0);
        }
        out
    }

    fn end() -> Vec<u8> {
        token(9)
    }

    /// Blob with explicit structure bytes and strings payload.
    fn blob_ex(
        off_mrb: u32,
        struct_start: u32,
        structure: &[u8],
        strings_start: u32,
        strings: &[u8],
    ) -> Vec<u8> {
        let total = strings_start + strings.len() as u32;
        let mut out = vec![0u8; total as usize];
        out[0..4].copy_from_slice(&MAGIC.to_be_bytes());
        out[4..8].copy_from_slice(&total.to_be_bytes());
        out[8..12].copy_from_slice(&struct_start.to_be_bytes());
        out[12..16].copy_from_slice(&strings_start.to_be_bytes());
        out[16..20].copy_from_slice(&off_mrb.to_be_bytes());
        out[20..24].copy_from_slice(&17u32.to_be_bytes());
        out[24..28].copy_from_slice(&16u32.to_be_bytes());
        out[32..36].copy_from_slice(&(strings.len() as u32).to_be_bytes());
        out[36..40].copy_from_slice(&(structure.len() as u32).to_be_bytes());
        out[struct_start as usize..(struct_start as usize + structure.len())]
            .copy_from_slice(structure);
        out[strings_start as usize..(strings_start as usize + strings.len())]
            .copy_from_slice(strings);
        out
    }

    /// Smallest valid tree: root with no properties or children.
    fn minimal_tree(nameoff: u32) -> Vec<u8> {
        let mut structure = begin("");
        structure.extend_from_slice(&end_node());
        structure.extend_from_slice(&end());
        let _ = nameoff;
        structure
    }

    /// Canonical layout: mrb 40..56, struct at 56, strings after the structure.
    fn valid_blob() -> Vec<u8> {
        let structure = minimal_tree(0);
        blob_ex(40, 56, &structure, 56 + structure.len() as u32, b"reg\0")
    }

    #[test]
    fn accepts_a_canonical_layout() {
        let bytes = valid_blob();
        let layout = validate(0x8000_0000, &bytes).expect("valid blob");
        assert_eq!(layout.mrb_start, 40);
        assert_eq!(layout.mrb_end, 56);
        assert_eq!(layout.struct_start, 56);
    }

    #[test]
    fn accepts_an_unaligned_strings_block() {
        // The strings block has no alignment requirement; start it off a
        // 4-byte boundary and keep everything else canonical.
        let structure = minimal_tree(0);
        let struct_end = 56 + structure.len() as u32;
        let bytes = blob_ex(40, 56, &structure, struct_end + 1, b"reg\0");
        assert!(validate(0x8000_0000, &bytes).is_ok());
    }

    #[test]
    fn accepts_a_property_before_children() {
        let mut structure = begin("");
        structure.extend_from_slice(&prop(4, 0, &[0, 0, 0, 2]));
        structure.extend_from_slice(&begin("child"));
        structure.extend_from_slice(&end_node());
        structure.extend_from_slice(&end_node());
        structure.extend_from_slice(&end());
        let bytes = blob_ex(40, 56, &structure, 56 + structure.len() as u32, b"reg\0");
        assert!(validate(0x8000_0000, &bytes).is_ok());
    }

    #[test]
    fn rejects_a_property_after_a_child() {
        // `fdt 0.1.5` stops `children()`/`all_nodes()` at a property that
        // follows a child, so this shape would parse into a truncated tree.
        let mut structure = begin("");
        structure.extend_from_slice(&begin("child"));
        structure.extend_from_slice(&end_node());
        structure.extend_from_slice(&prop(4, 0, &[0, 0, 0, 2]));
        structure.extend_from_slice(&end_node());
        structure.extend_from_slice(&end());
        let bytes = blob_ex(40, 56, &structure, 56 + structure.len() as u32, b"reg\0");
        assert_eq!(
            validate(0x8000_0000, &bytes),
            Err(FdtInputError::PropertyAfterChild)
        );
    }

    #[test]
    fn rejects_an_unknown_token() {
        let mut structure = begin("");
        structure.extend_from_slice(&token(7));
        structure.extend_from_slice(&end_node());
        structure.extend_from_slice(&end());
        let bytes = blob_ex(40, 56, &structure, 56 + structure.len() as u32, b"reg\0");
        assert_eq!(
            validate(0x8000_0000, &bytes),
            Err(FdtInputError::InvalidToken)
        );
    }

    #[test]
    fn rejects_a_second_root() {
        let mut structure = begin("");
        structure.extend_from_slice(&end_node());
        structure.extend_from_slice(&begin(""));
        structure.extend_from_slice(&end_node());
        structure.extend_from_slice(&end());
        let bytes = blob_ex(40, 56, &structure, 56 + structure.len() as u32, b"reg\0");
        assert_eq!(
            validate(0x8000_0000, &bytes),
            Err(FdtInputError::BadTreeShape)
        );
    }

    #[test]
    fn rejects_unbalanced_end_nodes() {
        let mut structure = begin("");
        structure.extend_from_slice(&end_node());
        structure.extend_from_slice(&end_node());
        structure.extend_from_slice(&end());
        let bytes = blob_ex(40, 56, &structure, 56 + structure.len() as u32, b"reg\0");
        assert_eq!(
            validate(0x8000_0000, &bytes),
            Err(FdtInputError::BadTreeShape)
        );
    }

    #[test]
    fn rejects_an_end_token_in_the_middle() {
        let mut structure = begin("");
        structure.extend_from_slice(&end_node());
        structure.extend_from_slice(&end());
        structure.extend_from_slice(&begin("late"));
        structure.extend_from_slice(&end_node());
        structure.extend_from_slice(&end());
        let bytes = blob_ex(40, 56, &structure, 56 + structure.len() as u32, b"reg\0");
        assert_eq!(
            validate(0x8000_0000, &bytes),
            Err(FdtInputError::EndTokenNotLast)
        );
    }

    #[test]
    fn rejects_a_property_running_past_the_block() {
        let mut structure = begin("");
        structure.extend_from_slice(&prop(0x40, 0, &[]));
        structure.extend_from_slice(&end_node());
        structure.extend_from_slice(&end());
        let bytes = blob_ex(40, 56, &structure, 56 + structure.len() as u32, b"reg\0");
        assert_eq!(
            validate(0x8000_0000, &bytes),
            Err(FdtInputError::StructureOutOfRange)
        );
    }

    #[test]
    fn rejects_a_property_name_outside_the_strings_block() {
        let mut structure = begin("");
        structure.extend_from_slice(&prop(4, 0x40, &[0, 0, 0, 2]));
        structure.extend_from_slice(&end_node());
        structure.extend_from_slice(&end());
        let bytes = blob_ex(40, 56, &structure, 56 + structure.len() as u32, b"reg\0");
        assert_eq!(
            validate(0x8000_0000, &bytes),
            Err(FdtInputError::BadPropertyName)
        );
    }

    #[test]
    fn rejects_an_unterminated_property_name() {
        let mut structure = begin("");
        structure.extend_from_slice(&prop(4, 0, &[0, 0, 0, 2]));
        structure.extend_from_slice(&end_node());
        structure.extend_from_slice(&end());
        let bytes = blob_ex(40, 56, &structure, 56 + structure.len() as u32, b"reg");
        assert_eq!(
            validate(0x8000_0000, &bytes),
            Err(FdtInputError::BadPropertyName)
        );
    }

    #[test]
    fn rejects_nesting_beyond_the_parser_limit() {
        let mut structure = Vec::new();
        for _ in 0..=MAX_DEPTH {
            structure.extend_from_slice(&begin("n"));
        }
        for _ in 0..=MAX_DEPTH {
            structure.extend_from_slice(&end_node());
        }
        structure.extend_from_slice(&end());
        let bytes = blob_ex(40, 56, &structure, 56 + structure.len() as u32, b"reg\0");
        assert_eq!(
            validate(0x8000_0000, &bytes),
            Err(FdtInputError::DepthLimitExceeded)
        );
    }

    #[test]
    fn the_parser_consumes_the_deepest_tree_this_gate_accepts() {
        // The gate's limit is only meaningful if the parser really walks the
        // deepest accepted tree: `fdt 0.1.5` stores the root at parent-stack
        // index 1 of 64, so one more level would index out of bounds.
        let mut structure = Vec::new();
        for _ in 0..MAX_DEPTH {
            structure.extend_from_slice(&begin("n"));
        }
        for _ in 0..MAX_DEPTH {
            structure.extend_from_slice(&end_node());
        }
        structure.extend_from_slice(&end());
        let bytes = blob_ex(40, 56, &structure, 56 + structure.len() as u32, b"reg\0");
        assert!(validate(0x8000_0000, &bytes).is_ok());
        let fdt = fdt::Fdt::new(&bytes).expect("parser accepts the gated blob");
        assert_eq!(fdt.all_nodes().count(), MAX_DEPTH as usize);
    }

    #[test]
    fn rejects_misaligned_blob_address() {
        let bytes = valid_blob();
        assert_eq!(
            validate(0x8000_0001, &bytes),
            Err(FdtInputError::BlobMisaligned)
        );
    }

    #[test]
    fn rejects_misaligned_mrb_offset() {
        let structure = minimal_tree(0);
        let mut bytes = blob_ex(49, 56, &structure, 56 + structure.len() as u32, b"reg\0");
        bytes[16..20].copy_from_slice(&49u32.to_be_bytes());
        assert_eq!(
            validate(0x8000_0000, &bytes),
            Err(FdtInputError::MrbMisaligned)
        );
    }

    #[test]
    fn rejects_mrb_outside_the_object() {
        let structure = minimal_tree(0);
        let mut bytes = blob_ex(40, 56, &structure, 56 + structure.len() as u32, b"reg\0");
        bytes[16..20].copy_from_slice(&0x1000u32.to_be_bytes());
        assert_eq!(
            validate(0x8000_0000, &bytes),
            Err(FdtInputError::BlockOutOfRange)
        );
    }

    #[test]
    fn rejects_missing_terminator_before_the_struct_block() {
        let structure = minimal_tree(0);
        let mut bytes = blob_ex(40, 56, &structure, 56 + structure.len() as u32, b"reg\0");
        bytes[40..56].copy_from_slice(&[0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0]);
        assert_eq!(
            validate(0x8000_0000, &bytes),
            Err(FdtInputError::MrbNoTerminator)
        );
    }

    #[test]
    fn rejects_a_terminator_only_beyond_totalsize() {
        let structure = minimal_tree(0);
        let mut bytes = blob_ex(40, 56, &structure, 56 + structure.len() as u32, b"reg\0");
        bytes[40..56].copy_from_slice(&[0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0]);
        bytes.extend_from_slice(&[0u8; 16]);
        assert_eq!(
            validate(0x8000_0000, &bytes),
            Err(FdtInputError::MrbNoTerminator)
        );
    }

    #[test]
    fn rejects_more_reservations_than_the_collector_accepts() {
        let structure = minimal_tree(0);
        let mut entries = Vec::new();
        for i in 0..MRB_MAX_ENTRIES + 1 {
            entries.extend_from_slice(&(0x8000_0000u64 + i as u64 * 0x1000).to_be_bytes());
            entries.extend_from_slice(&0x1000u64.to_be_bytes());
        }
        entries.extend_from_slice(&[0u8; 16]);
        let struct_start = 40 + entries.len() as u32;
        let mut bytes = blob_ex(
            40,
            struct_start,
            &structure,
            struct_start + structure.len() as u32,
            b"reg\0",
        );
        bytes[40..40 + entries.len()].copy_from_slice(&entries);
        assert_eq!(
            validate(0x8000_0000, &bytes),
            Err(FdtInputError::TooManyReservations)
        );
    }

    /// The card is 16 entries (the collector's own cap), so the 16th entry
    /// must pass and only the 17th is refused.
    #[test]
    fn accepts_exactly_the_collector_reservation_bound() {
        let structure = minimal_tree(0);
        let mut entries = Vec::new();
        for i in 0..MRB_MAX_ENTRIES {
            entries.extend_from_slice(&(0x8000_0000u64 + i as u64 * 0x1000).to_be_bytes());
            entries.extend_from_slice(&0x1000u64.to_be_bytes());
        }
        entries.extend_from_slice(&[0u8; 16]);
        let struct_start = 40 + entries.len() as u32;
        let mut bytes = blob_ex(
            40,
            struct_start,
            &structure,
            struct_start + structure.len() as u32,
            b"reg\0",
        );
        bytes[40..40 + entries.len()].copy_from_slice(&entries);
        assert!(validate(0x8000_0000, &bytes).is_ok());
    }

    #[test]
    fn rejects_struct_block_past_totalsize() {
        let structure = minimal_tree(0);
        let mut bytes = blob_ex(40, 56, &structure, 56 + structure.len() as u32, b"reg\0");
        bytes[36..40].copy_from_slice(&0x100u32.to_be_bytes());
        assert_eq!(
            validate(0x8000_0000, &bytes),
            Err(FdtInputError::BlockOutOfRange)
        );
    }

    #[test]
    fn rejects_overlapping_struct_and_strings_blocks() {
        // struct 56..72, strings 64..72: both in the object, overlapping.
        let structure = minimal_tree(0);
        let bytes = blob_ex(40, 56, &structure, 64, b"reg\0\0\0\0\0");
        assert_eq!(
            validate(0x8000_0000, &bytes),
            Err(FdtInputError::BlockOverlap)
        );
    }

    #[test]
    fn rejects_totalsize_larger_than_the_buffer() {
        let mut bytes = valid_blob();
        bytes[4..8].copy_from_slice(&0x1000u32.to_be_bytes());
        assert_eq!(
            validate(0x8000_0000, &bytes),
            Err(FdtInputError::BadTotalsize)
        );
    }

    #[test]
    fn rejects_a_structure_block_without_the_end_token() {
        let mut structure = begin("");
        structure.extend_from_slice(&end_node());
        let structure_len = structure.len();
        let mut bytes = blob_ex(40, 56, &structure, 56 + structure_len as u32, b"reg\0");
        bytes[56 + structure_len as usize - 4..56 + structure_len as usize]
            .copy_from_slice(&0u32.to_be_bytes());
        assert_eq!(
            validate(0x8000_0000, &bytes),
            Err(FdtInputError::MissingEndToken)
        );
    }

    #[test]
    fn rejects_unbacked_structure_alignment() {
        let structure = minimal_tree(0);
        let bytes = blob_ex(40, 57, &structure, 57 + structure.len() as u32, b"reg\0");
        assert_eq!(
            validate(0x8000_0000, &bytes),
            Err(FdtInputError::BlockMisaligned)
        );
    }

    #[test]
    fn rejects_unsupported_fdt_versions() {
        let mut old = valid_blob();
        old[20..24].copy_from_slice(&16u32.to_be_bytes());
        assert_eq!(
            validate(0x8000_0000, &old),
            Err(FdtInputError::UnsupportedVersion)
        );

        let mut future = valid_blob();
        future[24..28].copy_from_slice(&18u32.to_be_bytes());
        assert_eq!(
            validate(0x8000_0000, &future),
            Err(FdtInputError::UnsupportedVersion)
        );
    }

    #[test]
    fn rejects_a_layout_whose_mrb_follows_the_struct_block() {
        let structure = minimal_tree(0);
        let mut bytes = blob_ex(64, 40, &structure, 56, b"reg\0");
        bytes.resize(80, 0);
        bytes[4..8].copy_from_slice(&80u32.to_be_bytes());
        assert_eq!(
            validate(0x8000_0000, &bytes),
            Err(FdtInputError::UnsupportedBlockOrder)
        );
    }

    #[test]
    fn rejects_an_mrb_that_starts_inside_the_header() {
        let structure = minimal_tree(0);
        let mut bytes = blob_ex(40, 56, &structure, 56 + structure.len() as u32, b"reg\0");
        bytes[16..20].copy_from_slice(&32u32.to_be_bytes());
        assert_eq!(
            validate(0x8000_0000, &bytes),
            Err(FdtInputError::BlockOutOfRange)
        );
    }

    #[test]
    fn rejects_strings_before_the_structure_block() {
        let structure = minimal_tree(0);
        let mut bytes = blob_ex(40, 64, &structure, 80, b"reg\0");
        bytes[12..16].copy_from_slice(&56u32.to_be_bytes());
        assert_eq!(
            validate(0x8000_0000, &bytes),
            Err(FdtInputError::UnsupportedBlockOrder)
        );
    }
}
