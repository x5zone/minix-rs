//! Filter checksum policy: checksum kinds, group layout, mirror fallback.
//!
//! C correspondence: the checksum kinds (`ST_NIL`, `ST_XOR`, `ST_CRC`,
//! `ST_MD5`, `inc.h:25-27`), the feature switches (`USE_CHECKSUM`,
//! `BAD_SUM_ERROR`, `NR_SUM_SEC`, `main.c:16-22`, `main.c:52-53`),
//! the sector and group checksum routines (`compute/checksum`,
//! `make/check_group_sum`, `sum.c:39-158`), the interleaved extended
//! buffer layout (`sum.c:506-537`), the read-back verification
//! (`sum.c:491-493`), and the mirror fallback (`USE_MIRROR`,
//! `main.c:17`, `bad_driver` returning `RET_REDO`, `driver.c:242-252`,
//! with `RET_REDO` at `inc.h:57`).
//!
//! Digest math (CRC in `crc.c`, MD5 in `md5.c`) and lower-driver
//! traffic stay in the service binary; this module owns the policy
//! half: which checksum kind applies, how group sums lay out, and when
//! a mirror member is dropped.

/// Checksum kind protecting one sector group (`inc.h:25-27`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumKind {
    /// No checksum stored.
    None,
    /// Byte-wise exclusive-or across the group.
    Xor,
    /// Cyclic redundancy check.
    Crc,
    /// MD5 digest.
    Md5,
}

/// Sectors covered by one group checksum (`NR_SUM_SEC`, `sum.c`).
pub const SECTORS_PER_GROUP: u32 = 8;

/// Retry-the-other-mirror marker (`RET_REDO`, `inc.h:57`).
pub const RETRY_OTHER_MIRROR: i32 = 1;

/// Whether a failed checksum must fail the request (`BAD_SUM_ERROR`,
/// `main.c:19`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadSumPolicy {
    /// Report the mismatch as an error.
    ReportError,
    /// Log the mismatch and return the data anyway.
    ReturnAnyway,
}

/// Group checksum layout: data sectors followed by their sum sector,
/// interleaved in the extended buffer (`dd..C` layout, `sum.c:506-537`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupLayout {
    /// Data sectors per group.
    pub data_sectors: u32,
}

impl GroupLayout {
    /// Standard layout: eight data sectors plus one sum sector.
    pub fn standard() -> Self {
        GroupLayout { data_sectors: SECTORS_PER_GROUP }
    }

    /// Total sectors stored per group (data plus the sum sector).
    pub fn stored_sectors(&self) -> u32 {
        self.data_sectors + 1
    }

    /// Whether `sector` (0-based inside the group store) is the sum sector.
    pub fn is_sum_sector(&self, sector: u32) -> bool {
        sector == self.data_sectors
    }
}

/// Restart budget per lower driver before mirroring gives up
/// (`NR_RESTARTS`, `main.c:28`).
pub const NR_RESTARTS: u32 = 3;

/// Which half of the mirror pair a restart belongs to
/// (`DRIVER_MAIN` / `DRIVER_BACKUP`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorMember {
    /// The primary driver.
    Main,
    /// The mirror driver.
    Backup,
}

/// What the service should do after one lower driver was restarted.
///
/// C: `bad_driver` (`driver.c:384-408`): under the budget the answer is
/// "retry the request" (`EAGAIN`); crossing the budget with mirroring on
/// switches mirroring off and promotes the survivor (`OK`); crossing it
/// without a mirror left gives up (`EIO`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorOutcome {
    /// Keep going: the restart is still inside the budget (`EAGAIN`).
    Retry,
    /// Mirroring is now off and the survivor is the new main (`OK`).
    FailOver,
    /// The last driver crossed the budget with no mirror left (`EIO`).
    GiveUp,
}

/// Mirror pair state: per-member restart budgets plus the global
/// mirroring switch.
///
/// C: `driver[which].kills` counts restarts of each lower driver
/// (`driver.c:384-388`); crossing `NR_RESTARTS` turns `USE_MIRROR` off
/// for the whole filter and promotes the surviving member to main
/// (`driver.c:390-405`). Failures are counted per *driver restart*, not
/// per checksum mismatch, and the response is fail-over plus a global
/// switch, never a per-member removal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MirrorState {
    mirroring: bool,
    main_kills: u32,
    backup_kills: u32,
}

impl MirrorState {
    /// A pair that came up mirroring (or not, per the command line).
    pub fn new(mirroring: bool) -> Self {
        MirrorState {
            mirroring,
            main_kills: 0,
            backup_kills: 0,
        }
    }

    /// Record that `member` was restarted; returns the continuation.
    pub fn record_restart(&mut self, member: MirrorMember) -> MirrorOutcome {
        let kills = match member {
            MirrorMember::Main => &mut self.main_kills,
            MirrorMember::Backup => &mut self.backup_kills,
        };
        *kills += 1;
        if *kills < NR_RESTARTS {
            return MirrorOutcome::Retry;
        }
        if self.mirroring {
            // Threshold reached with a mirror left: mirroring goes off
            // and the survivor becomes the main driver.
            self.mirroring = false;
            return MirrorOutcome::FailOver;
        }
        MirrorOutcome::GiveUp
    }

    /// Whether mirroring is still on.
    pub const fn is_mirroring(&self) -> bool {
        self.mirroring
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group_layout_appends_one_sum_sector() {
        let layout = GroupLayout::standard();
        assert_eq!(layout.data_sectors, SECTORS_PER_GROUP);
        assert_eq!(layout.stored_sectors(), SECTORS_PER_GROUP + 1);
        assert!(!layout.is_sum_sector(0));
        assert!(layout.is_sum_sector(SECTORS_PER_GROUP));
    }

    #[test]
    fn test_mirror_failover_after_restart_budget() {
        // Two restarts are inside the budget (EAGAIN); the third crossing
        // switches mirroring off and promotes the survivor (driver.c:
        // 384-405).
        let mut mirror = MirrorState::new(true);
        assert_eq!(
            mirror.record_restart(MirrorMember::Main),
            MirrorOutcome::Retry
        );
        assert_eq!(
            mirror.record_restart(MirrorMember::Main),
            MirrorOutcome::Retry
        );
        assert_eq!(
            mirror.record_restart(MirrorMember::Main),
            MirrorOutcome::FailOver
        );
        assert!(!mirror.is_mirroring());
    }

    #[test]
    fn test_last_driver_crossing_budget_gives_up() {
        // With mirroring already off, a member crossing the budget has no
        // survivor to promote: give up (EIO).
        let mut mirror = MirrorState::new(false);
        assert_eq!(
            mirror.record_restart(MirrorMember::Main),
            MirrorOutcome::Retry
        );
        assert_eq!(
            mirror.record_restart(MirrorMember::Main),
            MirrorOutcome::Retry
        );
        assert_eq!(
            mirror.record_restart(MirrorMember::Main),
            MirrorOutcome::GiveUp
        );
    }

    #[test]
    fn test_members_count_restarts_independently() {
        // Budgets are per member: the main and the backup each get their
        // own NR_RESTARTS allowance.
        let mut mirror = MirrorState::new(true);
        assert_eq!(
            mirror.record_restart(MirrorMember::Backup),
            MirrorOutcome::Retry
        );
        assert_eq!(
            mirror.record_restart(MirrorMember::Main),
            MirrorOutcome::Retry
        );
        assert!(mirror.is_mirroring());
    }

    #[test]
    fn test_checksum_kinds_cover_all_four_options() {
        let kinds = [
            ChecksumKind::None,
            ChecksumKind::Xor,
            ChecksumKind::Crc,
            ChecksumKind::Md5,
        ];
        assert_eq!(kinds.len(), 4);
        assert_eq!(RETRY_OTHER_MIRROR, 1);
        let _ = BadSumPolicy::ReportError;
        let _ = BadSumPolicy::ReturnAnyway;
    }
}
