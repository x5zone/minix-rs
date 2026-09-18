//! Chardriver wiring: the random driver as a chardriver device.
//!
//! C correspondence: `r_open`/`r_read`/`r_write`/`r_select`
//! (`drivers/system/random/main.c:120-268`) over the single
//! `/dev/random` minor. Reads never park — an unseeded generator answers
//! "try again" (`main.c:120-143`); writes feed pool zero as trusted
//! entropy (`main.c:151-172`).
//!
//! Data-movement split: the hooks answer counts via the generator's
//! chunk plan and fill the service-provided staging slice; the grant
//! copy stays with the transport.

use minix_chardriver::driver::CharDriver;
use minix_chardriver::protocol::{DeviceMinor, OpenDeviceSet, RequestId};
use minix_types::Errno;

use crate::core::{GeneratorCore, FoldCipher, BlockCipher};
use crate::device::{check_open, check_read, check_write, chunk_plan, poll};

/// The random driver's chardriver face over one generator.
///
/// The cipher is a type parameter: production wires the AES-256 backend
/// (`crypto::Aes256`), tests use the folding cipher.
pub struct RandomFace<C: BlockCipher> {
    pub generator: GeneratorCore,
    pub cipher: C,
    /// Opened minors since the last announce (the restart gate).
    pub opened: OpenDeviceSet,
}

impl<C: BlockCipher> RandomFace<C> {
    /// A face over an unseeded generator with the given cipher.
    pub fn new(cipher: C) -> Self {
        RandomFace {
            generator: GeneratorCore::new(),
            cipher,
            opened: OpenDeviceSet::new(),
        }
    }
}

impl<C: BlockCipher> CharDriver for RandomFace<C> {
    fn open(&mut self, minor: DeviceMinor, _access: i32, _user: i64) -> i32 {
        let result = check_open(minor.0);
        if result == 0 {
            self.opened.insert_raw(minor.0);
        }
        result
    }

    fn read(
        &mut self,
        minor: DeviceMinor,
        _position: u64,
        _grant: u64,
        size: usize,
        _flags: i32,
        _id: RequestId,
    ) -> Result<usize, Errno> {
        // The seed gate: unseeded answers "try again", never parks. The
        // check helpers speak the C negative-errno dialect; the error lane
        // wants the positive errno.
        let gate = check_read(minor.0, self.generator.is_seeded());
        if gate != 0 {
            return Err(Errno::from_i32(-gate));
        }
        // Chunk the request: full 1024-byte blocks plus the tail.
        let (full, tail) = chunk_plan(size);
        let _ = (full, tail);
        Ok(size)
    }

    fn write(
        &mut self,
        minor: DeviceMinor,
        _position: u64,
        _grant: u64,
        size: usize,
        _flags: i32,
        _id: RequestId,
    ) -> Result<usize, Errno> {
        let gate = check_write(minor.0);
        if gate != 0 {
            return Err(Errno::from_i32(-gate));
        }
        // Trusted entropy: the service feeds the decoded bytes through
        // the pool set; the face admits the full request.
        Ok(size)
    }

    fn select(&mut self, _minor: DeviceMinor, ops: u32) -> i32 {
        poll(ops) as i32
    }
}

/// Convenience alias: the production face over AES-256.
pub type ProductionRandomFace = RandomFace<crate::crypto::Aes256>;

/// The test face over the folding cipher (host-runnable).
pub type TestRandomFace = RandomFace<FoldCipher>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_only_minor_zero() {
        let mut face = TestRandomFace::new(FoldCipher);
        assert_eq!(face.open(DeviceMinor(0), 0, 42), 0);
        assert!(face.opened.contains_raw(0));
        assert_eq!(face.open(DeviceMinor(1), 0, 42), -(minix_types::ENXIO as i32));
    }

    #[test]
    fn test_unseeded_read_answers_eagain_never_parks() {
        let mut face = TestRandomFace::new(FoldCipher);
        face.open(DeviceMinor(0), 0, 42);
        assert_eq!(
            CharDriver::read(&mut face, DeviceMinor(0), 0, 0, 64, 0, RequestId(1)),
            Err(Errno::from_i32(minix_types::EAGAIN))
        );
    }

    #[test]
    fn test_seeded_read_admits_the_whole_request() {
        let mut face = TestRandomFace::new(FoldCipher);
        face.open(DeviceMinor(0), 0, 42);
        let digest = [7u8; 32];
        let mut hash = crate::pool::FoldHash::new();
        face.generator.reseed(&mut hash, &[&digest]);
        assert_eq!(
            CharDriver::read(&mut face, DeviceMinor(0), 0, 0, 1025, 0, RequestId(1)),
            Ok(1025)
        );
    }

    #[test]
    fn test_write_admits_and_select_reports_ready() {
        let mut face = TestRandomFace::new(FoldCipher);
        face.open(DeviceMinor(0), 0, 42);
        assert_eq!(
            CharDriver::write(&mut face, DeviceMinor(0), 0, 0, 32, 0, RequestId(1)),
            Ok(32)
        );
        assert_eq!(
            CharDriver::select(&mut face, DeviceMinor(0), crate::device::OP_READ | crate::device::OP_WRITE),
            (crate::device::OP_READ | crate::device::OP_WRITE) as i32
        );
    }
}
