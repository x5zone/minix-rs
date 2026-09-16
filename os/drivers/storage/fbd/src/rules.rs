//! Fault-injection proxy rules: matching, lifetimes, hooks, actions.
//!
//! C correspondence: the rule record (`struct fbd_rule`,
//! `ioc_fbd.h:12-46`), the match rule with its side effects
//! (`rule_match`, `rule.c:88-121`), the all-matches walk
//! (`rule_find`, `rule.c:123-148`), the hook mask per action type
//! (`action_mask`, `action.c:229-246`), and the action bodies
//! (`action.c:105-216`).
//!
//! The faulty block device is a proxy, not a disk: every request is
//! forwarded to a lower driver, and rules decide which requests come
//! back damaged. Forwarding, message traffic, and the random source
//! stay in the service binary; this module owns the pure policy half:
//! does a rule fire, how long does it live, and what the matched
//! actions do to positions, sizes, and results.

use alloc::vec::Vec;

/// Match flag: fire on read requests (`FBD_FLAG_READ`, `ioc_fbd.h:43`).
pub const FLAG_READ: u32 = 0x1;
/// Match flag: fire on write requests (`FBD_FLAG_WRITE`, `ioc_fbd.h:44`).
pub const FLAG_WRITE: u32 = 0x2;

/// Hook bit: before the lower driver sees the request
/// (`PRE_HOOK`, `rule.h:16`).
pub const PRE_HOOK: u32 = 0x1;
/// Hook bit: instead of the lower driver (`IO_HOOK`, `rule.h:17`).
pub const IO_HOOK: u32 = 0x2;
/// Hook bit: after the lower driver answers (`POST_HOOK`, `rule.h:18`).
pub const POST_HOOK: u32 = 0x4;

/// Corruption flavor for the corrupt action (`ioc_fbd.h:55-59`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorruptKind {
    /// Zeroed data.
    Zero,
    /// The same wrong data every time, derived from the position.
    Persist,
    /// New random data every time (the random source stays in the
    /// service; supply a byte source to [`corrupt_data`]).
    Random,
}

/// What a triggered rule does (`FBD_ACTION_*`, `ioc_fbd.h:61-66`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultAction {
    /// Write or return corrupt data.
    Corrupt(CorruptKind),
    /// Return this error code instead of touching the lower driver.
    Error(i32),
    /// Send the transfer to a random aligned position in the target
    /// range (the service supplies the random choice).
    Misdirect { start: u64, end: u64, align: u32 },
    /// Process only this many bytes normally; pretend full completion.
    LostTorn { lead: u32 },
}

/// One fault rule: an address condition, a lifetime, and one action
/// (`struct fbd_rule`, `ioc_fbd.h:12-46`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FaultRule {
    /// First byte position the rule covers.
    pub start: u64,
    /// End position (exclusive); zero means up to end of disk.
    pub end: u64,
    /// Which request directions fire the rule (`FLAG_READ`/`FLAG_WRITE`).
    pub flags: u32,
    /// Matching attempts to skip before activating.
    pub skip: u32,
    /// Times left to trigger after skipping (zero means no limit).
    pub count: u32,
    /// The action to perform when the rule triggers.
    pub action: FaultAction,
    /// Lifetime spent (C resets `num` to zero for a dead rule).
    retired: bool,
}

impl FaultRule {
    /// A live rule over `[start, end)` firing on `flags`.
    ///
    /// `end == 0` means up to end of disk; `count == 0` means unlimited.
    pub fn new(start: u64, end: u64, flags: u32, action: FaultAction) -> Self {
        FaultRule {
            start,
            end,
            flags,
            skip: 0,
            count: 0,
            action,
            retired: false,
        }
    }

    /// Whether the rule fires for this request, with C's side effects:
    /// ranges must overlap (`rule->start < pos+size && (end == 0 ||
    /// end > pos)`, `rule.c:96-99`), flags must intersect, a pending skip
    /// is consumed instead of matching, and a finite `count` is spent by
    /// a real match (`rule.c:136-142`, spent rules disable themselves).
    pub fn rule_match(&mut self, pos: u64, size: u64, flag: u32) -> bool {
        if self.retired {
            return false;
        }
        if self.start >= pos.saturating_add(size) {
            return false;
        }
        if self.end != 0 && self.end <= pos {
            return false;
        }
        if self.flags & flag == 0 {
            return false;
        }
        if self.skip > 0 {
            self.skip -= 1;
            return false;
        }
        if self.count > 0 {
            self.count -= 1;
            if self.count == 0 {
                self.retired = true;
            }
        }
        true
    }

    /// Whether the rule's lifetime ran out (C resets `num` to zero).
    pub const fn is_retired(&self) -> bool {
        self.retired
    }

    /// Which hooks this rule's action fires at (`action_mask`,
    /// `action.c:229-246`).
    pub const fn hook_mask(&self) -> u32 {
        match self.action {
            FaultAction::Corrupt(_) => IO_HOOK,
            FaultAction::Error(_) => PRE_HOOK | POST_HOOK,
            FaultAction::Misdirect { .. } => PRE_HOOK,
            FaultAction::LostTorn { .. } => PRE_HOOK | POST_HOOK,
        }
    }
}

/// Find every rule that fires for this request, with lifetimes spent and
/// skips consumed; returns the matching rule indices and the union of
/// their hook masks.
///
/// C: `rule_find` (`rule.c:123-148`) aggregates ALL matches — not just
/// the first — because two rules may both want a say about one request.
pub fn find_rules(
    rules: &mut [FaultRule],
    pos: u64,
    size: u64,
    flag: u32,
) -> (Vec<usize>, u32) {
    let mut matched = Vec::new();
    let mut hooks = 0;
    for (index, rule) in rules.iter_mut().enumerate() {
        if rule.rule_match(pos, size, flag) {
            hooks |= rule.hook_mask();
            matched.push(index);
        }
    }
    (matched, hooks)
}

/// Bytes of the request that fall inside the rule range, plus how far
/// into the rule they start (`get_range`, `action.c`).
pub fn covered_bytes(rule: &FaultRule, pos: u64, size: u64) -> u64 {
    let from = rule.start.max(pos);
    let until = if rule.end == 0 {
        pos + size
    } else {
        rule.end.min(pos + size)
    };
    until.saturating_sub(from)
}

/// Corrupt a data buffer in place (`action_io_corrupt`,
/// `action.c:105-145`). `offset` is the matched range's distance from the
/// rule start (C's `skip` out-parameter) and seeds the persistent
/// pattern so the same range always corrupts the same way. The random
/// kind needs an entropy source and stays with the service.
pub fn corrupt_data(kind: CorruptKind, offset: u64, buf: &mut [u8]) {
    match kind {
        CorruptKind::Zero => {
            for byte in buf.iter_mut() {
                *byte = 0;
            }
        }
        CorruptKind::Persist => {
            // Dword-aligned positions and sizes only (action.c:119-122).
            if !offset.is_multiple_of(4) || !buf.len().is_multiple_of(4) {
                return;
            }
            let mut val = (offset as u32) ^ 0xDEAD_BEEF;
            for chunk in buf.chunks_exact_mut(4) {
                chunk.copy_from_slice(&val.to_ne_bytes());
                val = val.wrapping_add(4);
            }
        }
        CorruptKind::Random => {
            // Service-side: needs the entropy source.
        }
    }
}

/// Pre-error: limit the request to the part that precedes the rule range
/// (`action_pre_error`, `action.c:151-157`): the lower driver then only
/// sees the healthy prefix.
pub fn error_prefix_size(rule: &FaultRule, pos: u64, size: u64) -> u64 {
    if rule.start > pos {
        (rule.start - pos).min(size)
    } else {
        0
    }
}

/// Post-error: on success, replace the result with the rule's error code
/// (`action_post_error`, `action.c:164-170`); OK codes pass through.
pub const fn post_error(result: i32, code: i32) -> i32 {
    if result >= 0 && code != 0 {
        code
    } else {
        result
    }
}

/// Pre-misdirect: relocate the request to `choice`-th aligned slot inside
/// the target range (`action_pre_misdir`, `action.c:179-199`). The
/// service draws the random choice.
pub const fn misdirect_pos(start: u64, end: u64, align: u32, choice: u32) -> u64 {
    let slots = ((end - start) + 1) / align as u64;
    if slots == 0 {
        return start;
    }
    let chosen = (choice as u64) % slots;
    start + chosen * align as u64
}

/// Pre-lost-torn: process only the lead bytes normally
/// (`action_pre_losttorn`, `action.c:205-214`).
pub const fn torn_limit(size: u64, lead: u32) -> u64 {
    if size > lead as u64 {
        lead as u64
    } else {
        size
    }
}

/// Post-lost-torn: on success, pretend the whole request completed
/// (`action_post_losttorn`, `action.c:220-226`).
pub const fn torn_result(result: i32, original_size: u64) -> i32 {
    if result < 0 {
        result
    } else {
        original_size as i32
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use minix_types::EIO;
    use super::*;

    fn rule(start: u64, end: u64, action: FaultAction) -> FaultRule {
        FaultRule::new(start, end, FLAG_READ | FLAG_WRITE, action)
    }

    #[test]
    fn test_overlap_matches_partial_requests() {
        // C matches on RANGE OVERLAP, not on the start block alone
        // (rule.c:96-99): a rule over [100,200) fires for a request that
        // merely touches the range.
        let mut rule = rule(100, 200, FaultAction::Corrupt(CorruptKind::Zero));
        assert!(!rule.rule_match(50, 40, FLAG_READ)); // ends before 100
        assert!(rule.rule_match(50, 60, FLAG_READ)); // straddles 100
        assert!(rule.rule_match(150, 100, FLAG_READ)); // straddles 200
        assert!(!rule.rule_match(200, 50, FLAG_READ)); // starts at the end
    }

    #[test]
    fn test_open_ended_rule_matches_to_eof() {
        let mut rule = rule(100, 0, FaultAction::Corrupt(CorruptKind::Zero));
        assert!(rule.rule_match(1_000_000, 8, FLAG_READ));
    }

    #[test]
    fn test_flags_gate_direction() {
        let mut read_rule = rule(0, 0, FaultAction::Corrupt(CorruptKind::Zero));
        read_rule.flags = FLAG_READ;
        assert!(read_rule.rule_match(0, 8, FLAG_READ));
        assert!(!read_rule.rule_match(0, 8, FLAG_WRITE));
    }

    #[test]
    fn test_skip_consumes_then_fires() {
        let mut rule = rule(0, 0, FaultAction::Corrupt(CorruptKind::Zero));
        rule.skip = 2;
        assert!(!rule.rule_match(0, 8, FLAG_READ));
        assert!(!rule.rule_match(0, 8, FLAG_READ));
        assert!(rule.rule_match(0, 8, FLAG_READ));
    }

    #[test]
    fn test_count_spends_and_retires() {
        let mut rule = rule(0, 0, FaultAction::Corrupt(CorruptKind::Zero));
        rule.count = 2;
        assert!(rule.rule_match(0, 8, FLAG_READ));
        assert!(rule.rule_match(0, 8, FLAG_READ));
        // Lifetime spent: the rule is dead, like C zeroing its number.
        assert!(!rule.rule_match(0, 8, FLAG_READ));
        assert!(rule.is_retired());
    }

    #[test]
    fn test_find_rules_aggregates_all_matches() {
        let mut rules = [
            rule(0, 100, FaultAction::Corrupt(CorruptKind::Zero)),
            rule(50, 150, FaultAction::Error(EIO)),
            rule(500, 600, FaultAction::Corrupt(CorruptKind::Zero)),
        ];
        // A request inside the [50,150) overlap fires BOTH first rules
        // and their hook masks union (rule.c:129-146).
        let (matched, hooks) = find_rules(&mut rules, 60, 8, FLAG_READ);
        assert_eq!(matched, vec![0, 1]);
        assert_eq!(hooks, IO_HOOK | PRE_HOOK | POST_HOOK);
        // A miss fires nothing.
        let (matched, hooks) = find_rules(&mut rules, 900, 8, FLAG_READ);
        assert!(matched.is_empty());
        assert_eq!(hooks, 0);
    }

    #[test]
    fn test_corrupt_zero_and_persist_patterns() {
        let mut buf = [0xFFu8; 8];
        corrupt_data(CorruptKind::Zero, 0, &mut buf);
        assert_eq!(buf, [0u8; 8]);
        // Persist: deterministic per offset, dword strides.
        let mut buf = [0u8; 8];
        corrupt_data(CorruptKind::Persist, 8, &mut buf);
        let first = u32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]);
        assert_eq!(first, 8u32 ^ 0xDEAD_BEEF);
        let second = u32::from_ne_bytes([buf[4], buf[5], buf[6], buf[7]]);
        assert_eq!(second, first.wrapping_add(4));
    }

    #[test]
    fn test_error_prefix_and_post_codes() {
        let rule = rule(100, 200, FaultAction::Error(EIO));
        // Request starts before the range: only the healthy prefix passes.
        assert_eq!(error_prefix_size(&rule, 0, 150), 100);
        // Request starts inside the range: nothing healthy precedes it.
        assert_eq!(error_prefix_size(&rule, 120, 50), 0);
        // Post-error: success is replaced by the rule's code, failures
        // and explicit OK pass through.
        assert_eq!(post_error(0, EIO), EIO);
        assert_eq!(post_error(-5, EIO), -5);
        assert_eq!(post_error(0, 0), 0);
    }

    #[test]
    fn test_misdirect_positions_on_alignment_grid() {
        // Slots: (300-100+1)/100 = 2 (positions 100 and 200).
        assert_eq!(misdirect_pos(100, 300, 100, 0), 100);
        assert_eq!(misdirect_pos(100, 300, 100, 1), 200);
        // Choices wrap onto the grid: 5 % 2 = 1 is the second slot.
        assert_eq!(misdirect_pos(100, 300, 100, 5), 200);
        assert_eq!(misdirect_pos(100, 300, 100, 2), 100);
        // Degenerate range parks at the start.
        assert_eq!(misdirect_pos(100, 100, 100, 3), 100);
    }

    #[test]
    fn test_torn_write_limits_then_claims_completion() {
        assert_eq!(torn_limit(512, 128), 128);
        assert_eq!(torn_limit(64, 128), 64);
        // After a successful short write, the caller sees full completion.
        assert_eq!(torn_result(64, 512), 512);
        assert_eq!(torn_result(-5, 512), -5);
    }

    #[test]
    fn test_hook_masks_follow_action_types() {
        assert_eq!(
            rule(0, 0, FaultAction::Corrupt(CorruptKind::Zero)).hook_mask(),
            IO_HOOK
        );
        assert_eq!(
            rule(0, 0, FaultAction::Error(EIO)).hook_mask(),
            PRE_HOOK | POST_HOOK
        );
        assert_eq!(
            rule(0, 0, FaultAction::Misdirect {
                start: 0,
                end: 100,
                align: 4
            })
            .hook_mask(),
            PRE_HOOK
        );
        assert_eq!(
            rule(0, 0, FaultAction::LostTorn { lead: 16 }).hook_mask(),
            PRE_HOOK | POST_HOOK
        );
    }
}
