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

/// How many failed verifications drop a mirror member before the
/// driver kills it (mirror kill threshold, `driver.c:331-392`).
pub const MIRROR_KILL_THRESHOLD: u32 = 3;

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

/// Mirror health: counts failures and drops the member at the threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MirrorHealth {
    failures: u32,
    dropped: bool,
}

impl MirrorHealth {
    /// A freshly attached mirror member.
    pub fn new() -> Self {
        MirrorHealth { failures: 0, dropped: false }
    }

    /// Record one failed verification; true once the member is dropped.
    pub fn record_failure(&mut self) -> bool {
        if self.dropped {
            return true;
        }
        self.failures += 1;
        if self.failures >= MIRROR_KILL_THRESHOLD {
            self.dropped = true;
        }
        self.dropped
    }

    /// Whether this member still serves requests.
    pub fn is_live(&self) -> bool {
        !self.dropped
    }
}

impl Default for MirrorHealth {
    fn default() -> Self {
        Self::new()
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
    fn test_mirror_drops_at_threshold() {
        let mut health = MirrorHealth::new();
        assert!(health.is_live());
        assert!(!health.record_failure());
        assert!(!health.record_failure());
        assert!(health.record_failure());
        assert!(!health.is_live());
        assert!(health.record_failure());
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
