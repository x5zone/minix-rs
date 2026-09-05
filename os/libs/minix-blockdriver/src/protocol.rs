//! Block request protocol: numbers, sector math, extents, and queues.
//!
//! C correspondence: the message layout comment at the top of
//! `minix3/minix/lib/libblockdriver/driver.c:1-40`, the request constants
//! in `minix3/minix/include/minix/com.h:963-987`, the sector and DMA
//! constants in `minix3/minix/include/minix/blockdriver.h:50-60`, the queue
//! bounds in `minix3/minix/lib/libblockdriver/mq.c:24` (`MQ_SIZE 128`), and
//! the partition geometry in `minix3/minix/include/minix/partition.h`.

use minix_types::{EBADF, EINVAL, EIO, ENOTTY, OK};

/// Base of the block request range.
///
/// C: `BDEV_RQ_BASE 0x500` (`com.h:963`).
pub const BDEV_REQUEST_BASE: i32 = 0x500;

/// Maximum number of minor devices remembered as opened.
///
/// C: `MAX_NR_OPEN_DEVICES 256` (`driver.h:41`), shared with the character
/// framework; both frameworks keep their own table.
pub const MAX_OPEN_DEVICES: usize = 256;

/// Bytes in one physical sector.
///
/// C: `SECTOR_SIZE 512` (`blockdriver.h:53`).
pub const SECTOR_SIZE: u64 = 512;

/// Shift for dividing by the sector size.
///
/// C: `SECTOR_SHIFT 9` (`blockdriver.h:54`).
pub const SECTOR_SHIFT: u32 = 9;

/// Mask for the remainder of a division by the sector size.
///
/// C: `SECTOR_MASK 511` (`blockdriver.h:55`).
pub const SECTOR_MASK: u64 = 511;

/// Bytes in one compact-disc sector.
///
/// C: `CD_SECTOR_SIZE 2048` (`blockdriver.h:57`).
pub const CD_SECTOR_SIZE: u64 = 2048;

/// Number of sectors in the DMA staging buffer.
///
/// C: `DMA_SECTORS 1` (`config.h:39`); the byte size follows as
/// `DMA_BUF_SIZE = DMA_SECTORS * SECTOR_SIZE` (`blockdriver.h:60`).
pub const DMA_SECTORS: u64 = 1;

/// Bytes in the DMA staging buffer.
pub const DMA_BUFFER_SIZE: u64 = DMA_SECTORS * SECTOR_SIZE;

/// Open access flag: read access requested.
///
/// C: `BDEV_R_BIT 0x01` (`com.h:982`).
pub const BDEV_READ_ACCESS: i32 = 0x01;

/// Open access flag: write access requested.
///
/// C: `BDEV_W_BIT 0x02` (`com.h:983`).
pub const BDEV_WRITE_ACCESS: i32 = 0x02;

/// Transfer flag: force the write through to the medium immediately.
///
/// C: `BDEV_FORCEWRITE 0x01` (`com.h:987`).
pub const BDEV_FORCE_WRITE: i32 = 0x01;

/// Bound on queued messages in the shared message-queue module.
///
/// C: `MQ_SIZE 128` (`mq.c:24`). The C queues are per-device free-list
/// cells; this bound is preserved so a flood of requests fails fast instead
/// of growing without limit.
pub const MESSAGE_QUEUE_BOUND: usize = 128;

/// Block request kind, one variant per request number.
///
/// C: `BDEV_OPEN` through `BDEV_IOCTL` (`com.h:970-976`): open, close, read,
/// write, gather (vectored read), scatter (vectored write), control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BdevRequest {
    /// Open a minor device.
    Open,
    /// Close a minor device.
    Close,
    /// Read contiguous bytes into one grant.
    Read,
    /// Write contiguous bytes from one grant.
    Write,
    /// Read into a scatter-gather vector of grants.
    Gather,
    /// Write from a scatter-gather vector of grants.
    Scatter,
    /// Device-specific control operation.
    Ioctl,
}

impl BdevRequest {
    /// Small index of the request (zero for open through six for control).
    pub const fn index(self) -> i32 {
        match self {
            BdevRequest::Open => 0,
            BdevRequest::Close => 1,
            BdevRequest::Read => 2,
            BdevRequest::Write => 3,
            BdevRequest::Gather => 4,
            BdevRequest::Scatter => 5,
            BdevRequest::Ioctl => 6,
        }
    }

    /// Full message type of the request (base plus index).
    pub const fn message_type(self) -> i32 {
        BDEV_REQUEST_BASE + self.index()
    }

    /// Decode a raw message type; `None` means "not a block request".
    pub const fn decode(message_type: i32) -> Option<BdevRequest> {
        match message_type - BDEV_REQUEST_BASE {
            0 => Some(BdevRequest::Open),
            1 => Some(BdevRequest::Close),
            2 => Some(BdevRequest::Read),
            3 => Some(BdevRequest::Write),
            4 => Some(BdevRequest::Gather),
            5 => Some(BdevRequest::Scatter),
            6 => Some(BdevRequest::Ioctl),
            _ => None,
        }
    }

    /// True for the four data-moving requests (read, write, gather,
    /// scatter), which share the transfer callback and the position field.
    pub const fn is_transfer(self) -> bool {
        matches!(
            self,
            BdevRequest::Read | BdevRequest::Write | BdevRequest::Gather | BdevRequest::Scatter
        )
    }
}

/// Returns true when a raw message type is a block request.
///
/// C: `IS_BDEV_RQ(type)` (`com.h:966`).
pub const fn is_block_request(message_type: i32) -> bool {
    (message_type & !0x7f) == BDEV_REQUEST_BASE
}

/// Minor device number of a block device.
///
/// C: `devminor_t` as used by the block adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceMinor(pub u32);

/// Opaque device identifier used by the queue layer.
///
/// C: `device_id_t` (`blockdriver.h`), an integer naming one device inside
/// the multi-device queue module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceId(pub i32);

/// Opaque request identifier echoed back in every block reply.
///
/// C: the `id` field of the block messages; synchronous callers use `NO_ID`
/// (`libbdev/const.h`), asynchronous callers use a call index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequestId(pub i32);

/// Byte extent of a partition: base offset plus size in bytes.
///
/// C: `struct device` with `dv_base` and `dv_size` (`driver.h:28-31`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceExtent {
    /// Byte offset of the first byte of the extent.
    pub base: u64,
    /// Number of bytes in the extent.
    pub size: u64,
}

impl DeviceExtent {
    /// True when the half-open byte range sits inside this extent.
    pub fn contains(&self, offset: u64, length: u64) -> bool {
        offset
            .checked_add(length)
            .map(|end| offset >= self.base && end <= self.base + self.size)
            .unwrap_or(false)
    }
}

/// Geometry of a partition for the geometry callback.
///
/// C: `struct part_geom` (`partition.h`): byte base and size plus cylinder,
/// head, and sector counts for legacy addressing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartitionGeometry {
    /// Byte offset of the partition start.
    pub base: u64,
    /// Number of bytes in the partition.
    pub size: u64,
    /// Disk cylinder count.
    pub cylinders: u32,
    /// Disk head count.
    pub heads: u32,
    /// Sectors per track.
    pub sectors: u32,
}

/// Partitioning style passed to the partition parser.
///
/// C: the `style` parameter of `partition()` (`drvlib.c`): floppy, primary,
/// or sub-partition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionStyle {
    /// Floppy layout: no table, whole medium is one partition.
    Floppy,
    /// Primary table shared with other operating systems (sorted).
    Primary,
    /// Sub-partition inside an extended partition.
    Sub,
}

/// Driver kind: whether the driver answers partition requests.
///
/// C: `blockdriver_type_t` (`blockdriver.h:16-19`): `BLOCKDRIVER_TYPE_DISK`
/// handles partition requests, `BLOCKDRIVER_TYPE_OTHER` does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockDriverType {
    /// Disk-like device: partition table requests are served.
    Disk,
    /// Other device: partition requests are refused.
    Other,
}

/// Bounded first-in first-out queue of pending request identifiers.
///
/// Models the `MQ_SIZE`-bounded per-device queues of `mq.c` without owning
/// any message buffer: the service crate owns the messages, this type owns
/// the admission policy (fail fast when full, preserve order).
#[derive(Debug, Clone)]
pub struct PendingQueue {
    slots: [i32; MESSAGE_QUEUE_BOUND],
    head: usize,
    len: usize,
}

impl PendingQueue {
    /// Empty queue.
    pub const fn new() -> PendingQueue {
        PendingQueue {
            slots: [0; MESSAGE_QUEUE_BOUND],
            head: 0,
            len: 0,
        }
    }

    /// Number of queued entries.
    pub fn len(&self) -> usize {
        self.len
    }

    /// True when nothing is queued.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// True when no further entry fits.
    pub fn is_full(&self) -> bool {
        self.len >= MESSAGE_QUEUE_BOUND
    }

    /// Append an entry; returns false when the queue is full.
    ///
    /// C: `mq_enqueue` (`mq.c:49`) returns false when the free list is
    /// empty. Callers treat a full queue as a transient refusal, not as a
    /// fatal error.
    pub fn push(&mut self, id: i32) -> bool {
        if self.is_full() {
            return false;
        }
        let slot = (self.head + self.len) % MESSAGE_QUEUE_BOUND;
        self.slots[slot] = id;
        self.len += 1;
        true
    }

    /// Remove the oldest entry; `None` when empty.
    ///
    /// C: `mq_dequeue` (`mq.c:89`).
    pub fn pop(&mut self) -> Option<i32> {
        if self.is_empty() {
            return None;
        }
        let id = self.slots[self.head];
        self.head = (self.head + 1) % MESSAGE_QUEUE_BOUND;
        self.len -= 1;
        Some(id)
    }
}

impl Default for PendingQueue {
    fn default() -> Self {
        PendingQueue::new()
    }
}

/// Set of minor devices opened since the last announce.
///
/// Same 256-slot policy as the character framework (each framework keeps
/// its own table; see `driver.c` open helpers).
#[derive(Debug, Clone)]
pub struct OpenDeviceSet {
    slots: [u32; MAX_OPEN_DEVICES],
    len: usize,
}

impl OpenDeviceSet {
    /// Empty set, as right after an announce.
    pub const fn new() -> OpenDeviceSet {
        OpenDeviceSet {
            slots: [0; MAX_OPEN_DEVICES],
            len: 0,
        }
    }

    /// Forget every recorded device (fresh start or restart).
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Number of recorded devices.
    pub fn len(&self) -> usize {
        self.len
    }

    /// True when the set holds no device.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// True when the raw minor value was recorded before.
    pub fn contains_raw(&self, minor: u32) -> bool {
        self.slots[..self.len].contains(&minor)
    }

    /// True when the device was recorded before.
    pub fn contains(&self, minor: DeviceMinor) -> bool {
        self.contains_raw(minor.0)
    }

    /// Record a raw minor value; returns false when the table is full.
    pub fn insert_raw(&mut self, minor: u32) -> bool {
        if self.contains_raw(minor) {
            return true;
        }
        if self.len >= MAX_OPEN_DEVICES {
            return false;
        }
        self.slots[self.len] = minor;
        self.len += 1;
        true
    }

    /// Record a device; returns false when the table is full.
    pub fn insert(&mut self, minor: DeviceMinor) -> bool {
        self.insert_raw(minor.0)
    }
}

impl Default for OpenDeviceSet {
    fn default() -> Self {
        OpenDeviceSet::new()
    }
}

/// Default result when a device provides no transfer callback.
///
/// C: the transfer path reports `EIO` when the hook is missing. A block
/// device that cannot move blocks is an input-output error.
pub const NO_TRANSFER_HOOK: i32 = EIO;

/// Default result when a device provides no control callback.
///
/// C: `ENOTTY`, shared with the character framework.
pub const NO_IOCTL_HOOK: i32 = ENOTTY;

/// Default result when a non-disk device receives a partition request.
///
/// C: partition handling is skipped unless the type is disk; the Rust side
/// reports "bad file descriptor", matching the C refusal to serve geometry
/// for non-disk types.
pub const NOT_DISK: i32 = EBADF;

/// Error for a request with no usable minor number.
pub const BAD_MINOR: i32 = EINVAL;

/// Success code.
pub const SUCCESS: i32 = OK;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_indices_match_c_offsets() {
        assert_eq!(BdevRequest::Open.message_type(), 0x500);
        assert_eq!(BdevRequest::Close.message_type(), 0x501);
        assert_eq!(BdevRequest::Read.message_type(), 0x502);
        assert_eq!(BdevRequest::Write.message_type(), 0x503);
        assert_eq!(BdevRequest::Gather.message_type(), 0x504);
        assert_eq!(BdevRequest::Scatter.message_type(), 0x505);
        assert_eq!(BdevRequest::Ioctl.message_type(), 0x506);
    }

    #[test]
    fn test_decode_round_trips_all_seven_requests() {
        let all = [
            BdevRequest::Open,
            BdevRequest::Close,
            BdevRequest::Read,
            BdevRequest::Write,
            BdevRequest::Gather,
            BdevRequest::Scatter,
            BdevRequest::Ioctl,
        ];
        for request in all {
            assert_eq!(BdevRequest::decode(request.message_type()), Some(request));
        }
    }

    #[test]
    fn test_decode_rejects_character_range() {
        assert_eq!(BdevRequest::decode(0x400), None);
        assert_eq!(BdevRequest::decode(0x507), None);
    }

    #[test]
    fn test_transfer_classification_covers_vectored_pair() {
        assert!(BdevRequest::Read.is_transfer());
        assert!(BdevRequest::Write.is_transfer());
        assert!(BdevRequest::Gather.is_transfer());
        assert!(BdevRequest::Scatter.is_transfer());
        assert!(!BdevRequest::Open.is_transfer());
        assert!(!BdevRequest::Ioctl.is_transfer());
    }

    #[test]
    fn test_sector_constants_match_c_headers() {
        assert_eq!(SECTOR_SIZE, 512);
        assert_eq!(SECTOR_SHIFT, 9);
        assert_eq!(SECTOR_MASK, 511);
        assert_eq!(CD_SECTOR_SIZE, 2048);
        assert_eq!(DMA_BUFFER_SIZE, 512);
        assert_eq!(1u64 << SECTOR_SHIFT, SECTOR_SIZE);
    }

    #[test]
    fn test_extent_contains_checks_bounds() {
        let extent = DeviceExtent {
            base: 1024,
            size: 4096,
        };
        assert!(extent.contains(1024, 512));
        assert!(extent.contains(1024, 4096));
        assert!(!extent.contains(512, 512));
        assert!(!extent.contains(4096, 2048));
        assert!(!extent.contains(u64::MAX, 16));
    }

    #[test]
    fn test_queue_preserves_order_and_fails_fast_when_full() {
        let mut queue = PendingQueue::new();
        assert!(queue.is_empty());
        assert!(queue.push(1));
        assert!(queue.push(2));
        assert_eq!(queue.pop(), Some(1));
        assert_eq!(queue.pop(), Some(2));
        assert_eq!(queue.pop(), None);
        for id in 0..MESSAGE_QUEUE_BOUND as i32 {
            assert!(queue.push(id));
        }
        assert!(queue.is_full());
        assert!(!queue.push(-1));
    }
}
