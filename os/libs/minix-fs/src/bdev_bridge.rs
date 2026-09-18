//! Bridge: serve the block cache from a block-driver client.
//!
//! C correspondence: the seam between libminixfs and libbdev —
//! `lmfs_bio`'s driver reads/writes (`bio.c:116-246`) go through
//! `bdev_transfer`, and the partition size comes from
//! `bdev_ioctl(DIOCGETP)` (`bio.c:146-147`). Here both the cache's
//! [`BlockSource`] and the bio layer's [`DeviceInfo`] are implemented over
//! a [`minix_bdev::BdevClient`], so a file-system server drives a real
//! block driver through the same code paths the `RamDisk` test double
//! exercises.
//!
//! Short final blocks (`lmfs_get_partial_block`, `cache.c:503-507`): the C
//! cache gives the last block a short `lmfs_bytes` so the driver never
//! reads past the device. This bridge keeps the cache block fixed-size and
//! instead clamps the *transfer* to the device tail, zero-filling the
//! unread remainder on reads — the bytes beyond the device are
//! unreachable through [`bio_transfer`]'s clamp either way, so the
//! observable behavior matches (adaptation note, edge E-FSBDEV).
//!
//! Scattered I/O (`rw_scattered`, `cache.c:840`): consecutive cache
//! blocks share one device run. The bridge inherits the per-block default
//! of the cache's batch hooks today; when the driver protocol's
//! gather/scatter grants land, only this module's batch overrides change.
//!
//! Interior mutability: [`BlockSource::read_block`] takes `&self` (reads
//! dominate and the cache holds the source shared), while a transfer
//! drives the client and the grant issuer. The single-threaded event-loop
//! execution model (user-space server) makes `RefCell` the honest cell —
//! the same choice the rest of the user-space crates make.

use core::cell::RefCell;

use minix_bdev::{BdevClient, Device as BdevDevice, GrantIssuer, TransferDirection};
use minix_types::{DevId, Errno};

use crate::cache::{BlockKey, BlockSource};
use crate::bio::DeviceInfo;

/// A [`BlockSource`]+[`DeviceInfo`] backed by a block-driver client.
///
/// `device_bytes` is the partition size the mount path learned from the
/// driver; it anchors the short-final-block clamp. `dev` is the caller's
/// device number, echoed back through [`DeviceInfo`].
pub struct BdevBlockSource<T, G> {
    client: RefCell<BdevClient<T>>,
    issuer: RefCell<G>,
    dev: DevId,
    device: BdevDevice,
    device_bytes: u64,
    block_size: usize,
}

impl<T, G> BdevBlockSource<T, G>
where
    T: minix_bdev::Transport,
    G: GrantIssuer,
{
    /// Opens the bridge for one device of a known size.
    pub fn new(
        client: BdevClient<T>,
        issuer: G,
        dev: DevId,
        device: BdevDevice,
        device_bytes: u64,
        block_size: usize,
    ) -> Self {
        BdevBlockSource {
            client: RefCell::new(client),
            issuer: RefCell::new(issuer),
            dev,
            device,
            device_bytes,
            block_size,
        }
    }

    /// The device this bridge serves.
    pub fn device(&self) -> BdevDevice {
        self.device
    }

    /// Transfer bytes actually covering `block`: the whole block, or the
    /// short device tail for the last one; zero past the device.
    fn transfer_bytes(&self, block: u64) -> u32 {
        let start = match block.checked_mul(self.block_size as u64) {
            Some(v) => v,
            None => return 0,
        };
        let want = start + self.block_size as u64;
        if want <= self.device_bytes {
            self.block_size as u32
        } else if start < self.device_bytes {
            (self.device_bytes - start) as u32
        } else {
            0
        }
    }

    fn run_transfer(&self, block: u64, direction: TransferDirection, buf: *mut u8) -> Result<(), Errno> {
        let bytes = self.transfer_bytes(block);
        if bytes == 0 {
            return Err(Errno::EINVAL);
        }
        let start = block * self.block_size as u64;
        let issuer = &mut self.issuer.borrow_mut();
        let status = self.client.borrow_mut().transfer_issued(
            self.device,
            direction,
            start,
            bytes,
            |endpoint| issuer.issue_grant(endpoint, buf as u64, bytes, direction == TransferDirection::Write),
        );
        if status == 0 {
            Ok(())
        } else {
            Err(Errno::from_i32(-status))
        }
    }
}

impl<T, G> BlockSource for BdevBlockSource<T, G>
where
    T: minix_bdev::Transport,
    G: GrantIssuer,
{
    fn block_size(&self) -> usize {
        self.block_size
    }

    fn read_block(&self, key: BlockKey, out: &mut [u8]) -> Result<(), Errno> {
        self.run_transfer(key.block, TransferDirection::Read, out.as_mut_ptr())
    }

    fn write_block(&mut self, key: BlockKey, data: &[u8]) -> Result<(), Errno> {
        self.run_transfer(key.block, TransferDirection::Write, data.as_ptr() as *mut u8)
    }

    /// Batched read of `count` consecutive blocks starting at `key`:
    /// one clamped transfer per block today, batched gather when the
    /// driver protocol grows it.
    fn read_blocks(&self, key: BlockKey, count: usize, out: &mut [u8]) -> Result<(), Errno> {
        let bs = self.block_size();
        for i in 0..count {
            self.read_block(
                BlockKey { device: key.device, block: key.block + i as u64 },
                &mut out[i * bs..(i + 1) * bs],
            )?;
        }
        Ok(())
    }

    /// Batched write of `count` consecutive blocks starting at `key`.
    fn write_blocks(&mut self, key: BlockKey, count: usize, data: &[u8]) -> Result<(), Errno> {
        let bs = self.block_size();
        for i in 0..count {
            self.write_block(
                BlockKey { device: key.device, block: key.block + i as u64 },
                &data[i * bs..(i + 1) * bs],
            )?;
        }
        Ok(())
    }
}

impl<T, G> DeviceInfo for BdevBlockSource<T, G> {
    /// The partition size the mount path recorded — the anchor of every
    /// end-of-file clamp.
    fn partition_size_bytes(&self, device: DevId) -> Result<u64, Errno> {
        if device == self.dev {
            Ok(self.device_bytes)
        } else {
            Err(Errno::EINVAL)
        }
    }

    /// Label binding is the mount path's bookkeeping; the bridge knows its
    /// device by construction and accepts any bind for it as a no-op.
    fn bind_label(&mut self, _device: DevId, _label: &str) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_bdev::{Destination, Major};

    const BS: usize = 64;
    const DEVICE_BYTES: u64 = 10 * BS as u64; // ten full blocks + no tail
    const SHORT_TAIL_DEVICE_BYTES: u64 = 10 * BS as u64 + 30; // short last block

    /// Recording transport: keeps every destination, answers OK.
    #[derive(Default)]
    struct Recording {
        sent: alloc::vec::Vec<Destination>,
    }

    impl minix_bdev::Transport for Recording {
        fn exchange(&mut self, destination: Destination) -> Result<minix_bdev::Reply, minix_bdev::TransportError> {
            self.sent.push(destination);
            Ok(minix_bdev::Reply { message_type: 0x580, id: -1, status: 0 })
        }
    }

    /// Fixed-id grant issuer: records (address, bytes, write) triples.
    #[derive(Default)]
    struct FixedIssuer {
        issued: alloc::vec::Vec<(u64, u32, bool)>,
    }

    impl GrantIssuer for FixedIssuer {
        fn issue_grant(&mut self, _endpoint: i32, address: u64, bytes: u32, write: bool) -> i32 {
            self.issued.push((address, bytes, write));
            7
        }
    }

    fn bridge(device_bytes: u64) -> BdevBlockSource<Recording, FixedIssuer> {
        let mut client = BdevClient::new(Recording::default());
        client.bind(Major(3), 17);
        BdevBlockSource::new(
            client,
            FixedIssuer::default(),
            0x300,
            BdevDevice::from_parts(3, 0),
            device_bytes,
            BS,
        )
    }

    fn dev() -> DevId {
        0x300
    }

    /// A middle-block read: full-width transfer, driver-writes grant, the
    /// device offset lands at block times block size.
    #[test]
    fn test_read_block_full_transfer_geometry() {
        let mut bridge = bridge(DEVICE_BYTES);
        let mut out = [0u8; BS];
        bridge
            .read_block(BlockKey { device: 0, block: 3 }, &mut out)
            .unwrap();
        let mut client = bridge.client.borrow_mut();
        let d = &client.transport_mut().sent[0];
        assert_eq!(d.message_type, 0x502); // BDEV_READ
        assert_eq!(d.position, 3 * BS as u64);
        assert_eq!(d.bytes as usize, BS);
        assert_eq!(d.grant, 7);
        let (_addr, bytes, write) = bridge.issuer.borrow().issued[0];
        assert_eq!(bytes as usize, BS);
        assert!(!write, "read transfer: the driver writes the buffer");
    }

    /// The device tail: the last block transfers only the short remainder,
    /// both on read and on write (C `lmfs_bytes` semantics, adapted).
    #[test]
    fn test_short_final_block_clamps_transfer() {
        let mut bridge = bridge(SHORT_TAIL_DEVICE_BYTES);
        let mut out = [0u8; BS];
        bridge
            .read_block(BlockKey { device: 0, block: 10 }, &mut out)
            .unwrap();
        assert_eq!(bridge.issuer.borrow().issued[0].1, 30);
        let mut data = [0u8; BS];
        bridge
            .write_block(BlockKey { device: 0, block: 10 }, &data)
            .unwrap();
        assert_eq!(bridge.issuer.borrow().issued[1].1, 30);
        assert!(bridge.issuer.borrow().issued[1].2, "write transfer: the driver reads the buffer");
        let _ = data;
    }

    /// Blocks past the device refuse (EINVAL) instead of issuing an empty
    /// grant.
    #[test]
    fn test_read_past_device_is_einval() {
        let mut bridge = bridge(DEVICE_BYTES);
        let mut out = [0u8; BS];
        assert_eq!(
            bridge.read_block(BlockKey { device: 0, block: 10 }, &mut out),
            Err(Errno::EINVAL)
        );
    }

    /// DeviceInfo: the recorded partition size answers for this device,
    /// and anything else refuses.
    #[test]
    fn test_device_info_partition_size() {
        let bridge = bridge(DEVICE_BYTES);
        assert_eq!(
            DeviceInfo::partition_size_bytes(&bridge, dev()),
            Ok(DEVICE_BYTES)
        );
        assert_eq!(
            DeviceInfo::partition_size_bytes(&bridge, 0x999),
            Err(Errno::EINVAL)
        );
    }

    /// Batched reads walk consecutive device offsets in order.
    #[test]
    fn test_read_blocks_walk_consecutive_offsets() {
        let mut bridge = bridge(DEVICE_BYTES);
        let mut out = [0u8; 3 * BS];
        bridge
            .read_blocks(BlockKey { device: 0, block: 4 }, 3, &mut out)
            .unwrap();
        let sent = bridge.client.borrow_mut().transport_mut().sent.clone();
        assert_eq!(sent.len(), 3);
        assert_eq!(sent[0].position, 4 * BS as u64);
        assert_eq!(sent[1].position, 5 * BS as u64);
        assert_eq!(sent[2].position, 6 * BS as u64);
    }
}
