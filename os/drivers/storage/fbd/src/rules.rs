//! Fault-injection proxy rules: address matching plus three hooks.
//!
//! C correspondence: the rule lookup `rule_find` (`rule.c:115`), the
//! three interception points `PRE`, `IO`, `POST` (`fbd.c:420-435`),
//! and the fault actions (`action.c:105-229`: corrupt data, report an
//! error before the transfer, misdirect the transfer, lose a torn
//! write, plus the action mask at `action.c:229`).
//!
//! The faulty block device is a proxy, not a disk: every request is
//! forwarded to a lower driver found through the device directory
//! service (`driver_label`, `fbd.c:28`, resolved with
//! `ds_retrieve_label_endpt`, `fbd.c:61`, then `ipc_sendrec`,
//! e.g. `fbd.c:151`). Rules decide which requests come back damaged.
//! Forwarding itself stays in the service binary; this module owns the
//! pure matching half: does a rule cover this address, and which hook
//! fires.

/// Where a rule intercepts a transfer (`fbd.c:420-435`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hook {
    /// Before the lower driver sees the request.
    Pre,
    /// Instead of the lower driver (replaces the data path).
    Io,
    /// After the lower driver answers.
    Post,
}

/// What a matched rule does to the transfer (`action.c:105-216`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultAction {
    /// Flip bytes in the transferred data.
    CorruptData,
    /// Fail before touching the lower driver.
    ReportError,
    /// Send the transfer to the wrong address.
    Misdirect,
    /// Drop half of a torn write.
    LoseTornWrite,
    /// Let the transfer pass untouched.
    PassThrough,
}

/// One fault rule: an address range plus the action per hook.
///
/// The C rule carries an address condition (`FBDCADDRULE`,
/// `fbd.c:197`); a transfer matches when its start address falls
/// inside the rule range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FaultRule {
    /// First block the rule covers.
    pub first_block: u64,
    /// One past the last block the rule covers.
    pub end_block: u64,
    /// Action when the PRE hook fires.
    pub pre: FaultAction,
    /// Action when the IO hook fires.
    pub io: FaultAction,
    /// Action when the POST hook fires.
    pub post: FaultAction,
}

impl FaultRule {
    /// Whether a transfer starting at `block` falls under this rule.
    pub fn matches(&self, block: u64) -> bool {
        block >= self.first_block && block < self.end_block
    }

    /// The action for one hook point.
    pub fn action_for(&self, hook: Hook) -> FaultAction {
        match hook {
            Hook::Pre => self.pre,
            Hook::Io => self.io,
            Hook::Post => self.post,
        }
    }
}

/// Find the first rule covering `block`, if any (`rule_find`, `rule.c:115`).
pub fn find_rule(rules: &[FaultRule], block: u64) -> Option<&FaultRule> {
    rules.iter().find(|rule| rule.matches(block))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_rule() -> FaultRule {
        FaultRule {
            first_block: 100,
            end_block: 200,
            pre: FaultAction::PassThrough,
            io: FaultAction::CorruptData,
            post: FaultAction::ReportError,
        }
    }

    #[test]
    fn test_rule_matches_inside_range_only() {
        let rule = sample_rule();
        assert!(!rule.matches(99));
        assert!(rule.matches(100));
        assert!(rule.matches(199));
        assert!(!rule.matches(200));
    }

    #[test]
    fn test_hooks_select_their_own_actions() {
        let rule = sample_rule();
        assert_eq!(rule.action_for(Hook::Pre), FaultAction::PassThrough);
        assert_eq!(rule.action_for(Hook::Io), FaultAction::CorruptData);
        assert_eq!(rule.action_for(Hook::Post), FaultAction::ReportError);
    }

    #[test]
    fn test_lookup_returns_first_covering_rule() {
        let rules = [sample_rule(), sample_rule()];
        let found = find_rule(&rules, 150);
        assert!(found.is_some());
        assert!(find_rule(&rules, 50).is_none());
        assert!(find_rule(&[], 150).is_none());
    }
}
