//! The FBD block face: the rule table and the transfer path with its three
//! interception points.
//!
//! C correspondence: `fbd.c`'s `fbd_transfer` chooser (`fbd.c:396-442`)
//! and `rule.c`'s three hooks (`rule.c:151-215`), with the per-action
//! arithmetic in [`crate::rules`]. The control requests and the
//! open/close forwarding live in [`crate::service`]: they need the
//! downstream endpoint, which the service resolves at startup.
//!
//! One deliberate difference: C picks a zero-copy *direct* path when no
//! rule wants the I/O hook (`fbd_transfer_direct`, indirect grants,
//! `fbd.c:228-270`); the rewrite always goes through the staging buffer
//! (`fbd_transfer_copy`'s shape), because forwarding a caller's grant
//! needs an indirect-grant verb the transport does not have. The lower
//! driver sees the same bytes; one copy is added.

use minix_bdev::client::{BdevClient, Device, Major, TransferDirection};
use minix_bdev::driver::PlaneLink;
use minix_blockdriver::driver::BlockDriver;
use minix_blockdriver::protocol::{BlockDriverType, DeviceMinor};
use minix_driver_rt::plane::TransferPlane;
use minix_types::E2BIG;

use crate::protocol::{FLAG_READ, FLAG_WRITE, MAX_RULES};
use crate::rules::{
    CorruptKind, FaultAction, FaultRule, IO_HOOK, POST_HOOK, PRE_HOOK, corrupt_data,
    error_prefix_size, find_rules, misdirect_pos, post_error, torn_limit, torn_result,
};

/// The staging buffer (`BUF_SIZE = NR_IOREQS * CLICK_SIZE`, `fbd.c:15`,
/// "256k": 64 requests of one click each).
pub const BUF_SIZE: u64 = 64 * 4096;

/// The FBD face: a rule table, the downstream binding, and the buffer the
/// copy path interposes on.
pub struct FbdFace {
    /// The lower driver's endpoint (`driver_endpt`), set by the service
    /// once the label resolves.
    downstream: i32,
    /// The lower driver's minor (`driver_minor`).
    minor: u32,
    /// The rule table; a `Some` slot is a live rule (`rules[]` with C's
    /// `num` as occupancy).
    rules: [Option<FaultRule>; MAX_RULES],
    /// The intermediate buffer (`fbd_buf`).
    buf: [u8; BUF_SIZE as usize],
    /// Entropy for the random corruption flavor and the misdirect choice
    /// (C seeds `srand48(getticks())`; the rewrite keeps a small state so
    /// tests are reproducible).
    entropy: u64,
}

impl FbdFace {
    /// A face over the given lower-driver minor, with no rules and no
    /// downstream binding yet.
    pub fn new(minor: u32) -> Self {
        FbdFace {
            downstream: -1,
            minor,
            rules: [None; MAX_RULES],
            buf: [0; BUF_SIZE as usize],
            entropy: 0x1234_5678_9abc_def0,
        }
    }

    /// Bind the lower driver's endpoint (`ds_retrieve_label_endpt`).
    pub fn set_downstream(&mut self, endpoint: i32) {
        self.downstream = endpoint;
    }

    /// The lower driver's endpoint, if resolved.
    pub fn downstream(&self) -> i32 {
        self.downstream
    }

    /// The lower driver's minor.
    pub fn minor(&self) -> u32 {
        self.minor
    }

    /// The number of live rules.
    pub fn rule_count(&self) -> usize {
        self.rules.iter().filter(|slot| slot.is_some()).count()
    }

    /// One draw from the face's entropy (xorshift64).
    fn draw(&mut self) -> u64 {
        let mut x = self.entropy;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.entropy = x;
        x
    }

    /// Add a rule, returning its 1-based number (`FBDCADDRULE`,
    /// `rule.c:25-42`).
    pub fn add_rule(&mut self, rule: FaultRule) -> Result<i32, i32> {
        for (index, slot) in self.rules.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(rule);
                return Ok(index as i32 + 1);
            }
        }
        Err(minix_types::ENOMEM)
    }

    /// Delete a rule by number (`FBDCDELRULE`, `rule.c:44-61`).
    pub fn delete_rule(&mut self, num: i32) -> Result<(), i32> {
        if num <= 0 || num as usize > MAX_RULES {
            return Err(minix_types::EINVAL);
        }
        if self.rules[num as usize - 1].is_none() {
            return Err(minix_types::ENOENT);
        }
        self.rules[num as usize - 1] = None;
        Ok(())
    }

    /// The rule with the given number (`FBDCGETRULE`, `rule.c:63-...`).
    pub fn get_rule(&self, num: i32) -> Result<FaultRule, i32> {
        if num <= 0 || num as usize > MAX_RULES {
            return Err(minix_types::EINVAL);
        }
        self.rules[num as usize - 1].ok_or(minix_types::ENOENT)
    }

    /// The transfer path (`fbd_transfer`, `fbd.c:396-442`).
    pub fn transfer(
        &mut self,
        plane: &mut dyn TransferPlane,
        do_write: bool,
        position: u64,
        count: u64,
        flags: i32,
    ) -> i64 {
        let osize = count;
        let flag = if do_write { FLAG_WRITE } else { FLAG_READ };

        // One pass over the live table: `find_rules` spends skips and
        // lifetimes exactly once (`rule_find`, `rule.c:123-148`), and the
        // updated table is written back before the hook passes.
        let mut table: alloc::vec::Vec<FaultRule> =
            self.rules.iter().filter_map(|slot| *slot).collect();
        let (_matched, hooks) = find_rules(&mut table, position, count, flag);
        let mut cursor = 0usize;
        for slot in self.rules.iter_mut() {
            if slot.is_some() {
                // A rule that spent its last trigger frees its slot
                // (`rule_find` resets `num` to zero, `rule.c:136-142`).
                *slot = if table[cursor].is_retired() {
                    None
                } else {
                    Some(table[cursor])
                };
                cursor += 1;
            }
        }

        let mut position = position;
        let mut size = count;

        // Pre hook (`rule_pre_hook`, `rule.c:151-160`).
        if hooks & PRE_HOOK != 0 {
            for rule in table.iter() {
                if rule.hook_mask() & PRE_HOOK == 0 || !fires(rule, position, size, flag) {
                    continue;
                }
                match rule.action {
                    FaultAction::Error(_) => {
                        size = error_prefix_size(rule, position, size).min(size);
                    }
                    FaultAction::Misdirect { start, end, align } => {
                        let choice = self.draw() as u32;
                        position = misdirect_pos(start, end, align, choice);
                    }
                    FaultAction::LostTorn { lead } => size = torn_limit(size, lead),
                    FaultAction::Corrupt(_) => {}
                }
            }
        }

        if size == 0 {
            return 0;
        }
        if size > BUF_SIZE {
            return -(E2BIG as i64);
        }

        let bytes = size as u32;
        let device = Device::from_parts(0, self.minor);
        let direction = if do_write {
            TransferDirection::Write
        } else {
            TransferDirection::Read
        };
        let buffer_address = self.buf.as_ptr() as u64;
        let rule_snapshot = table.clone();

        // The staging buffer's grant is issued on the plane before the
        // client takes the plane over for the forward.
        let grant = client_grant(plane, self.downstream, buffer_address, bytes, do_write);
        if grant < 0 {
            return -(minix_types::ENOMEM as i64);
        }
        // Write requests copy the caller's bytes in first
        // (`fbd_transfer_copy`, `fbd.c:305-330`).
        if do_write && let Err(status) = plane.copy_from_caller(0, &mut self.buf[..bytes as usize])
        {
            return -(status.abs() as i64);
        }
        if do_write {
            self.io_hook(&rule_snapshot, position, size, flag);
        }

        let status: i32 = {
            let mut client = BdevClient::new(PlaneLink::new(plane));
            client.bind(Major(0), self.downstream);
            client.transfer(device, direction, position, bytes, grant)
        };

        // Read requests corrupt the answer and copy it out afterwards
        // (`fbd.c:340-375`).
        if !do_write && status >= 0 {
            self.io_hook(&rule_snapshot, position, size, flag);
            if let Err(copy) = plane.copy_to_caller(0, &self.buf[..bytes as usize]) {
                return -(copy.abs() as i64);
            }
        }
        let result: i32 = status;
        let _ = flags;

        // Post hook (`rule_post_hook`, `rule.c:186-215`).
        let mut result = result;
        if hooks & POST_HOOK != 0 {
            for rule in table.iter() {
                if rule.hook_mask() & POST_HOOK == 0 {
                    continue;
                }
                match rule.action {
                    // The rule's code is a positive errno as userland writes
                    // it; the transfer's return convention is negative.
                    FaultAction::Error(code) => {
                        let signed = if code > 0 { -code } else { code };
                        result = post_error(result, signed);
                    }
                    FaultAction::LostTorn { lead: _ } => result = torn_result(result, osize),
                    _ => {}
                }
            }
        }
        result as i64
    }

    /// The I/O hook (`rule_io_hook`, `rule.c:168-176`): corrupt actions
    /// rewrite the staging buffer in place.
    fn io_hook(&mut self, table: &[FaultRule], position: u64, size: u64, flag: u32) {
        for rule in table.iter() {
            if rule.hook_mask() & IO_HOOK == 0 || !fires(rule, position, size, flag) {
                continue;
            }
            if let FaultAction::Corrupt(kind) = rule.action {
                match kind {
                    CorruptKind::Random => {
                        let len = size as usize;
                        for index in 0..len {
                            self.buf[index] = self.draw() as u8;
                        }
                    }
                    other => corrupt_data(other, position, &mut self.buf[..size as usize]),
                }
            }
        }
    }
}

/// Issue a grant over the staging buffer through the plane (the client's
/// grant issuer hands the id back to the transfer).
fn client_grant(
    plane: &mut dyn TransferPlane,
    endpoint: i32,
    address: u64,
    bytes: u32,
    write: bool,
) -> i32 {
    plane
        .grant_direct(
            minix_types::Endpoint(endpoint),
            address,
            bytes as usize,
            write,
        )
        .unwrap_or(-1)
}

/// Whether a rule fires for this request, without the lifetime side
/// effects `rule_match` performs (the hooks run after `find_rules` has
/// already spent them, `rule.c:151-215`).
fn fires(rule: &FaultRule, pos: u64, size: u64, flag: u32) -> bool {
    let until = if rule.end == 0 { pos + size } else { rule.end };
    rule.start < pos + size && until > pos && (rule.flags & flag) != 0
}

impl BlockDriver for FbdFace {
    fn driver_type(&self) -> BlockDriverType {
        // C: `BLOCKDRIVER_TYPE_OTHER` (`fbd.c:34`) — no partition requests.
        BlockDriverType::Other
    }

    fn open(&mut self, _minor: DeviceMinor, _access: i32) -> i32 {
        // The service forwards opens (it holds the downstream binding);
        // reaching this hook means the interception was bypassed.
        -(minix_types::EIO)
    }

    fn close(&mut self, _minor: DeviceMinor) -> i32 {
        -(minix_types::EIO)
    }

    fn transfer(
        &mut self,
        plane: &mut dyn TransferPlane,
        _minor: DeviceMinor,
        do_write: bool,
        position: u64,
        count: u64,
        flags: i32,
        _id: minix_blockdriver::protocol::RequestId,
        vectored: bool,
    ) -> i64 {
        if vectored {
            // The vectored pair carries an iovec array (`E-BLK-VEC`); this
            // leg answers the contiguous forms.
            return -(minix_types::ENOSYS as i64);
        }
        self.transfer(plane, do_write, position, count, flags)
    }

    fn ioctl(&mut self, _minor: DeviceMinor, _request: u64, _grant: u64, _user: i64) -> i32 {
        // The FBD controls are decoded in the service; any other control is
        // forwarded downstream by C through an indirect grant, which the
        // transport cannot express (registered seam).
        minix_types::ENOTTY
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    use minix_blockdriver::wire;
    use minix_driver_rt::plane::{GrantIpc, NullPlane, TransferPlane};
    use minix_types::{Endpoint, Message};

    /// A plane over a scratch caller buffer that answers the forwarded
    /// request with a scripted status.
    struct FakePlane {
        from_bytes: Vec<u8>,
        to_bytes: Vec<u8>,
        copied: Vec<(u64, usize)>,
        status: i32,
        forwarded: Vec<(u64, u32, i32)>,
        grants: usize,
    }

    impl FakePlane {
        fn new() -> Self {
            FakePlane {
                from_bytes: Vec::new(),
                to_bytes: Vec::new(),
                copied: Vec::new(),
                status: 0,
                forwarded: Vec::new(),
                grants: 0,
            }
        }
    }

    impl GrantIpc for FakePlane {
        fn grant_direct(
            &mut self,
            _dest: Endpoint,
            _address: u64,
            _len: usize,
            _write: bool,
        ) -> Result<i32, i32> {
            self.grants += 1;
            Ok(5)
        }
        fn grant_revoke(&mut self, _grant: i32) -> Result<(), i32> {
            Ok(())
        }
        fn send_rec(&mut self, _dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.forwarded.push((
                wire::read_u64(msg, wire::request::POS),
                wire::read_i32(msg, wire::request::COUNT) as u32,
                wire::read_i32(msg, wire::request::GRANT),
            ));
            let id = wire::read_i32(msg, wire::request::ID);
            let count = wire::read_i32(msg, wire::request::COUNT);
            msg.m_type = minix_bdev::transport::BDEV_REPLY;
            // A block driver's transfer reply carries the byte count; a
            // scripted non-zero status stands in for an error.
            // A failure is a negative errno in the reply lane, like the
            // framework's own replies.
            let status = if self.status != 0 {
                -self.status
            } else {
                count
            };
            wire::write_i32(msg, wire::reply::STATUS, status);
            wire::write_i32(msg, wire::reply::ID, id);
            Ok(())
        }
    }

    impl TransferPlane for FakePlane {
        fn caller(&self) -> Endpoint {
            Endpoint(20)
        }
        fn copy_from_caller(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), i32> {
            self.copied.push((offset, buf.len()));
            buf.copy_from_slice(&self.from_bytes[..buf.len()]);
            Ok(())
        }
        fn copy_to_caller(&mut self, offset: u64, buf: &[u8]) -> Result<(), i32> {
            self.copied.push((offset, buf.len()));
            self.to_bytes.extend_from_slice(buf);
            Ok(())
        }
    }

    fn bound_face() -> FbdFace {
        let mut face = FbdFace::new(3);
        face.set_downstream(6);
        face
    }

    #[test]
    fn test_rule_table_add_delete_get_and_full() {
        let mut face = bound_face();
        let rule = FaultRule::new(
            0x1000,
            0x2000,
            FLAG_READ,
            FaultAction::Error(minix_types::EIO),
        );
        assert_eq!(face.add_rule(rule), Ok(1));
        assert_eq!(
            face.add_rule(FaultRule::new(
                0,
                0,
                FLAG_WRITE,
                FaultAction::Corrupt(CorruptKind::Zero)
            )),
            Ok(2)
        );
        assert_eq!(face.rule_count(), 2);
        assert_eq!(face.get_rule(1), Ok(rule));
        assert_eq!(face.get_rule(3), Err(minix_types::ENOENT));
        assert_eq!(face.get_rule(0), Err(minix_types::EINVAL));
        assert_eq!(face.delete_rule(1), Ok(()));
        assert_eq!(face.delete_rule(1), Err(minix_types::ENOENT));
        let mut full = bound_face();
        for _ in 0..MAX_RULES {
            assert!(full.add_rule(rule).is_ok());
        }
        assert_eq!(full.add_rule(rule), Err(minix_types::ENOMEM));
    }

    #[test]
    fn test_transfer_forwards_contiguously_with_the_staging_grant() {
        let mut face = bound_face();
        let mut plane = FakePlane::new();
        // A read: forward, then copy the answer out.
        assert_eq!(face.transfer(&mut plane, false, 0x100, 64, 0), 64);
        assert_eq!(plane.forwarded, vec![(0x100, 64, 5)]);
        assert_eq!(plane.grants, 1);
        assert_eq!(plane.to_bytes.len(), 64);
        // A write: copy in, then forward.
        let mut plane = FakePlane::new();
        plane.from_bytes = vec![7u8; 32];
        assert_eq!(face.transfer(&mut plane, true, 0x200, 32, 0), 32);
        assert_eq!(plane.forwarded, vec![(0x200, 32, 5)]);
        assert_eq!(plane.copied, vec![(0, 32)]);
    }

    #[test]
    fn test_a_failing_downstream_keeps_the_caller_buffer_untouched() {
        let mut face = bound_face();
        let mut plane = FakePlane::new();
        plane.status = minix_types::EIO;
        assert_eq!(
            face.transfer(&mut plane, false, 0, 16, 0),
            -(minix_types::EIO as i64)
        );
        assert!(plane.to_bytes.is_empty(), "no copy out on failure");
    }

    #[test]
    fn test_the_error_rule_serves_only_the_prefix_and_reports_its_code() {
        let mut face = bound_face();
        assert_eq!(
            face.add_rule(FaultRule::new(
                0x100,
                0x200,
                FLAG_READ,
                FaultAction::Error(minix_types::EIO)
            )),
            Ok(1)
        );
        let mut plane = FakePlane::new();
        assert_eq!(
            face.transfer(&mut plane, false, 0, 0x200, 0),
            -(minix_types::EIO as i64)
        );
        assert_eq!(
            plane.forwarded,
            vec![(0, 0x100, 5)],
            "only the healthy prefix"
        );
    }

    #[test]
    fn test_the_corrupt_rule_rewrites_the_staging_buffer() {
        let mut face = bound_face();
        assert_eq!(
            face.add_rule(FaultRule::new(
                0,
                0,
                FLAG_READ,
                FaultAction::Corrupt(CorruptKind::Zero)
            )),
            Ok(1)
        );
        let mut plane = FakePlane::new();
        assert_eq!(face.transfer(&mut plane, false, 0, 16, 0), 16);
        assert!(plane.to_bytes.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn test_the_lost_torn_rule_pretends_full_completion() {
        let mut face = bound_face();
        assert_eq!(
            face.add_rule(FaultRule::new(
                0,
                0,
                FLAG_WRITE,
                FaultAction::LostTorn { lead: 8 }
            )),
            Ok(1)
        );
        let mut plane = FakePlane::new();
        plane.from_bytes = vec![1u8; 32];
        // Eight bytes reach the lower driver; the caller is told 32.
        assert_eq!(face.transfer(&mut plane, true, 0, 32, 0), 32);
        assert_eq!(plane.forwarded, vec![(0, 8, 5)]);
    }

    #[test]
    fn test_the_misdirect_rule_relocates_the_request() {
        let mut face = bound_face();
        assert_eq!(
            face.add_rule(FaultRule::new(
                0,
                0,
                FLAG_WRITE,
                FaultAction::Misdirect {
                    start: 0x1000,
                    end: 0x2000,
                    align: 512
                }
            )),
            Ok(1)
        );
        let mut plane = FakePlane::new();
        plane.from_bytes = vec![2u8; 16];
        assert_eq!(face.transfer(&mut plane, true, 0x40, 16, 0), 16);
        let position = plane.forwarded[0].0;
        assert!(
            (0x1000..=0x2000).contains(&position),
            "relocated: {position:#x}"
        );
        assert_eq!(position % 512, 0, "aligned");
    }

    #[test]
    fn test_a_finite_rule_lifetime_is_spent_once_per_match() {
        let mut face = bound_face();
        let mut rule = FaultRule::new(0, 0, FLAG_READ, FaultAction::Corrupt(CorruptKind::Zero));
        rule.count = 1;
        assert_eq!(face.add_rule(rule), Ok(1));
        // The first read fires the rule; the second finds it retired.
        let mut plane = FakePlane::new();
        assert_eq!(face.transfer(&mut plane, false, 0, 8, 0), 8);
        assert!(plane.to_bytes.iter().all(|byte| *byte == 0));
        assert_eq!(face.rule_count(), 0, "the rule spent its one trigger");
        let mut plane = FakePlane::new();
        assert_eq!(face.transfer(&mut plane, false, 0, 8, 0), 8);
        assert!(
            plane.to_bytes.iter().all(|byte| *byte == 0),
            "the buffer is not rewritten"
        );
    }

    #[test]
    fn test_skips_delay_a_rule_without_retiring_it() {
        let mut face = bound_face();
        let mut rule = FaultRule::new(0, 0, FLAG_READ, FaultAction::Corrupt(CorruptKind::Zero));
        rule.skip = 1;
        assert_eq!(face.add_rule(rule), Ok(1));
        // First match consumes the skip; the second fires.
        let mut plane = FakePlane::new();
        face.transfer(&mut plane, false, 0, 8, 0);
        assert_eq!(face.rule_count(), 1, "the rule is still live");
        let mut plane = FakePlane::new();
        face.transfer(&mut plane, false, 0, 8, 0);
        assert_eq!(face.rule_count(), 1);
    }

    #[test]
    fn test_vectored_refusal_and_the_bypassed_hooks() {
        let mut face = bound_face();
        let mut null = NullPlane;
        assert_eq!(
            BlockDriver::transfer(
                &mut face,
                &mut null,
                DeviceMinor(0),
                false,
                0,
                8,
                0,
                minix_blockdriver::protocol::RequestId(1),
                true
            ),
            -(minix_types::ENOSYS as i64)
        );
        assert_eq!(face.open(DeviceMinor(0), 0), -(minix_types::EIO));
        assert_eq!(face.close(DeviceMinor(0)), -(minix_types::EIO));
        assert_eq!(face.ioctl(DeviceMinor(0), 0, 0, 0), minix_types::ENOTTY);
        assert_eq!(face.driver_type(), BlockDriverType::Other);
        assert_eq!(face.downstream(), 6);
        assert_eq!(face.minor(), 3);
    }
}
