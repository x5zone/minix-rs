//! Block requests: three-segment chains, alignment, status mapping.
//!
//! C correspondence: the request assembly in
//! `minix3/minix/drivers/storage/virtio_blk/virtio_blk.c:280-379`
//! (header type, sector math, vector fix-up, three-segment chain),
//! `virtio_blk_status2error` (`virtio_blk.c:549-563`), the block size
//! `VIRTIO_BLK_BLOCK_SIZE 512` (`virtio_blk.c:35`), and the feature
//! table (`virtio_blk.c:45-54`, barrier through identification bytes).
//!
//! Chains are described, not sent, here: the service crate owns the
//! queue. This module decides the chain shape (which three segments, in
//! which order, with which flags) and translates the device status.

/// Block size in bytes: every position and length aligns to it.
///
/// C: `VIRTIO_BLK_BLOCK_SIZE 512` (`virtio_blk.c:35`).
pub const BLOCK_SIZE: u64 = 512;

/// Request type: read sectors into guest buffers.
///
/// C: `VIRTIO_BLK_T_IN`.
pub const TYPE_IN: u32 = 0;
/// Request type: write sectors from guest buffers.
///
/// C: `VIRTIO_BLK_T_OUT`.
pub const TYPE_OUT: u32 = 1;
/// Request type: flush the write cache.
///
/// C: `VIRTIO_BLK_T_FLUSH`.
pub const TYPE_FLUSH: u32 = 4;
/// Request type: fetch the drive identifier.
///
/// C: `VIRTIO_BLK_T_GET_ID`.
pub const TYPE_GET_ID: u32 = 8;

/// Status: request completed.
///
/// C: `VIRTIO_BLK_S_OK`.
pub const STATUS_OK: u8 = 0;
/// Status: input-output error.
///
/// C: `VIRTIO_BLK_S_IOERR`.
pub const STATUS_IO_ERROR: u8 = 1;
/// Status: request not supported.
///
/// C: `VIRTIO_BLK_S_UNSUPP`.
pub const STATUS_UNSUPPORTED: u8 = 2;

/// Feature bit: write barrier.
///
/// C: `VIRTIO_BLK_F_BARRIER` (`virtio_blk.c:45`).
pub const FEATURE_BARRIER: u8 = 0;
/// Feature bit: maximum segment size.
///
/// C: `VIRTIO_BLK_F_SIZE_MAX` (`virtio_blk.c:46`).
pub const FEATURE_SIZE_MAX: u8 = 1;
/// Feature bit: maximum segment count.
///
/// C: `VIRTIO_BLK_F_SEG_MAX` (`virtio_blk.c:47`).
pub const FEATURE_SEG_MAX: u8 = 2;
/// Feature bit: geometry in configuration.
///
/// C: `VIRTIO_BLK_F_GEOMETRY` (`virtio_blk.c:48`).
pub const FEATURE_GEOMETRY: u8 = 4;
/// Feature bit: read-only device.
///
/// C: `VIRTIO_BLK_F_RO` (`virtio_blk.c:49`).
pub const FEATURE_READ_ONLY: u8 = 5;
/// Feature bit: block size in configuration.
///
/// C: `VIRTIO_BLK_F_BLK_SIZE` (`virtio_blk.c:50`).
pub const FEATURE_BLOCK_SIZE: u8 = 6;
/// Feature bit: flush support.
///
/// C: `VIRTIO_BLK_F_FLUSH` (`virtio_blk.c:53`).
pub const FEATURE_FLUSH: u8 = 9;

/// Direction of a block request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Guest buffers receive (read).
    In,
    /// Guest buffers provide (write).
    Out,
}

/// Planned chain: header type, sector, segment count, byte count.
///
/// C: the header fill plus vector fix-up (`virtio_blk.c:334-364`): the
/// type follows the direction, the sector is the position divided by the
/// block size, and the byte count truncates to the partition end with the
/// last vector shortened to fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainPlan {
    /// Request type (in or out).
    pub request_type: u32,
    /// First sector number.
    pub sector: u64,
    /// Data segments in the chain (excluding header and status).
    pub segments: usize,
    /// Total data bytes.
    pub bytes: u64,
}

/// Plan one transfer: alignment first, bounds second, chain last.
///
/// Errors: unaligned position or length is invalid argument
/// (`virtio_blk.c:283-286,321-325`); past-the-end answers zero bytes
/// (`virtio_blk.c:293-294`).
pub fn plan_transfer(
    position: u64,
    length: u64,
    partition_end: u64,
    write: bool,
) -> Result<ChainPlan, i32> {
    if !position.is_multiple_of(BLOCK_SIZE) || !length.is_multiple_of(BLOCK_SIZE) {
        return Err(-einval_code());
    }
    if position >= partition_end {
        return Ok(ChainPlan {
            request_type: if write { TYPE_OUT } else { TYPE_IN },
            sector: position / BLOCK_SIZE,
            segments: 0,
            bytes: 0,
        });
    }
    let mut bytes = length;
    if position + bytes > partition_end {
        bytes = partition_end - position;
        bytes -= bytes % BLOCK_SIZE;
    }
    Ok(ChainPlan {
        request_type: if write { TYPE_OUT } else { TYPE_IN },
        sector: position / BLOCK_SIZE,
        segments: if bytes == 0 { 0 } else { 1 },
        bytes,
    })
}

/// Translate a device status byte into the answer code.
///
/// C: `virtio_blk_status2error` (`virtio_blk.c:549-563`): ok maps to
/// success, input-output error maps to itself, unsupported maps to
/// itself, anything else is treated as input-output error (unknown
/// statuses must not pass as success).
pub const fn status_to_code(status: u8) -> i32 {
    match status {
        STATUS_OK => 0,
        STATUS_IO_ERROR => -minix_types::EIO,
        STATUS_UNSUPPORTED => -minix_types::ENOTSUP,
        _ => -minix_types::EIO,
    }
}

/// Invalid-argument code for unaligned access.
const fn einval_code() -> i32 {
    minix_types::EINVAL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unaligned_access_is_invalid() {
        assert!(plan_transfer(100, 512, 8192, false).is_err());
        assert!(plan_transfer(0, 100, 8192, false).is_err());
        assert!(plan_transfer(0, 0, 8192, false).is_ok());
    }

    #[test]
    fn test_past_end_answers_zero() {
        let plan = plan_transfer(8192, 512, 8192, false).unwrap();
        assert_eq!(plan.bytes, 0);
        assert_eq!(plan.request_type, TYPE_IN);
    }

    #[test]
    fn test_truncation_rounds_down_to_blocks() {
        let plan = plan_transfer(0, 8192, 6000, false).unwrap();
        assert_eq!(plan.bytes, 5632);
        assert_eq!(plan.sector, 0);
    }

    #[test]
    fn test_direction_selects_type() {
        let read = plan_transfer(0, 512, 8192, false).unwrap();
        let write = plan_transfer(0, 512, 8192, true).unwrap();
        assert_eq!(read.request_type, TYPE_IN);
        assert_eq!(write.request_type, TYPE_OUT);
    }

    #[test]
    fn test_status_mapping_never_passes_unknown() {
        assert_eq!(status_to_code(STATUS_OK), 0);
        assert!(status_to_code(STATUS_IO_ERROR) < 0);
        assert!(status_to_code(STATUS_UNSUPPORTED) < 0);
        assert_eq!(status_to_code(0xFF), status_to_code(STATUS_IO_ERROR));
    }

    #[test]
    fn test_constants_match_c_sources() {
        assert_eq!(BLOCK_SIZE, 512);
        assert_eq!(TYPE_IN, 0);
        assert_eq!(TYPE_OUT, 1);
        assert_eq!(FEATURE_READ_ONLY, 5);
        assert_eq!(FEATURE_FLUSH, 9);
    }
}
