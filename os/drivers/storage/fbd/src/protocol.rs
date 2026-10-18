//! The FBD rule wire format: the ioctl numbers and the `struct fbd_rule`
//! codec.
//!
//! C correspondence: `minix3/minix/include/sys/ioc_fbd.h` — the rule
//! structure (num, start, end, flags, skip, count, action, and the action
//! parameter union), the match flags, the action and corruption
//! enumerations, and the three control numbers built with
//! `sys/ioccom.h`'s `_IOC`.

use crate::rules::{CorruptKind, FaultAction, FaultRule};

/// Rule slots (`MAX_RULES`, `rule.h:4`).
pub const MAX_RULES: usize = 16;
/// Match flag: read requests (`FBD_FLAG_READ`).
pub const FLAG_READ: u32 = 0x1;
/// Match flag: write requests (`FBD_FLAG_WRITE`).
pub const FLAG_WRITE: u32 = 0x2;

/// Action: write or return corrupt data (`FBD_ACTION_CORRUPT`).
pub const ACTION_CORRUPT: i32 = 0;
/// Action: return an I/O error (`FBD_ACTION_ERROR`).
pub const ACTION_ERROR: i32 = 1;
/// Action: misdirected write (`FBD_ACTION_MISDIR`).
pub const ACTION_MISDIR: i32 = 2;
/// Action: lost or torn write (`FBD_ACTION_LOSTTORN`).
pub const ACTION_LOSTTORN: i32 = 3;

/// Corruption: zeroed data (`FBD_CORRUPT_ZERO`).
pub const CORRUPT_ZERO: i32 = 0;
/// Corruption: the same bad data every time (`FBD_CORRUPT_PERSIST`).
pub const CORRUPT_PERSIST: i32 = 1;
/// Corruption: new random data every time (`FBD_CORRUPT_RANDOM`).
pub const CORRUPT_RANDOM: i32 = 2;

/// Direction bits and parameter length (`_IOC`).
const IOC_IN: u64 = 0x8000_0000;
const IOC_OUT: u64 = 0x4000_0000;
const IOCPARM_MASK: u64 = 0xfff;

/// Builds one ioctl number (`_IOC(inout, group, num, len)`).
pub const fn ioc(inout: u64, group: u8, num: u64, len: u64) -> u64 {
    inout | ((len & IOCPARM_MASK) << 16) | ((group as u64) << 8) | num
}

/// `sizeof(struct fbd_rule)` on LP64: 4 + 4 pad + 8 + 8 + 4 + 4 + 4 + 4 +
/// the 24-byte parameter union.
pub const RULE_SIZE: usize = 64;

/// Add a rule (`FBDCADDRULE`, `ioc_fbd.h:62`).
pub const FBDCADDRULE: u64 = ioc(IOC_IN, b'B', 1, RULE_SIZE as u64);
/// Delete a rule by number (`FBDCDELRULE`, `ioc_fbd.h:63`).
pub const FBDCDELRULE: u64 = ioc(IOC_IN, b'B', 2, 4);
/// Retrieve a rule (`FBDCGETRULE`, `ioc_fbd.h:64`).
pub const FBDCGETRULE: u64 = ioc(IOC_IN | IOC_OUT, b'B', 3, RULE_SIZE as u64);

/// `struct fbd_rule` field offsets.
pub mod wire {
    /// The rule number (1-based; zero means "slot free").
    pub const NUM: usize = 0;
    /// First byte position the rule covers.
    pub const START: usize = 8;
    /// End position, exclusive (zero = to end of disk).
    pub const END: usize = 16;
    /// Match flags.
    pub const FLAGS: usize = 24;
    /// Matches to skip before activating.
    pub const SKIP: usize = 28;
    /// Times left to trigger (zero = no limit).
    pub const COUNT: usize = 32;
    /// Action type.
    pub const ACTION: usize = 36;
    /// The action parameter union.
    pub const PARAMS: usize = 40;
    /// Corruption type (`params.corrupt.type`).
    pub const CORRUPT_TYPE: usize = 40;
    /// Error code (`params.error.code`).
    pub const ERROR_CODE: usize = 40;
    /// Misdirect target start (`params.misdir.start`).
    pub const MISDIR_START: usize = 40;
    /// Misdirect target end (`params.misdir.end`).
    pub const MISDIR_END: usize = 48;
    /// Misdirect alignment (`params.misdir.align`).
    pub const MISDIR_ALIGN: usize = 56;
    /// Lost/torn lead bytes (`params.losttorn.lead`).
    pub const LOSTTORN_LEAD: usize = 40;
}

/// Decode one `struct fbd_rule` into the rule model.
///
/// The number is ignored here: the service owns slot placement (`rule.c`'s
/// `num` is the slot's occupancy marker, not part of the match).
pub fn decode_rule(raw: &[u8; RULE_SIZE]) -> Result<FaultRule, i32> {
    let read_u32 =
        |offset: usize| -> u32 { u32::from_le_bytes(raw[offset..offset + 4].try_into().unwrap()) };
    let read_u64 =
        |offset: usize| -> u64 { u64::from_le_bytes(raw[offset..offset + 8].try_into().unwrap()) };
    let flags = read_u32(wire::FLAGS);
    let skip = read_u32(wire::SKIP);
    let count = read_u32(wire::COUNT);
    let action = match read_u32(wire::ACTION) as i32 {
        ACTION_CORRUPT => FaultAction::Corrupt(match read_u32(wire::CORRUPT_TYPE) as i32 {
            CORRUPT_ZERO => CorruptKind::Zero,
            CORRUPT_PERSIST => CorruptKind::Persist,
            CORRUPT_RANDOM => CorruptKind::Random,
            _ => return Err(minix_types::EINVAL),
        }),
        ACTION_ERROR => FaultAction::Error(read_u32(wire::ERROR_CODE) as i32),
        ACTION_MISDIR => FaultAction::Misdirect {
            start: read_u64(wire::MISDIR_START),
            end: read_u64(wire::MISDIR_END),
            align: read_u32(wire::MISDIR_ALIGN),
        },
        ACTION_LOSTTORN => FaultAction::LostTorn {
            lead: read_u32(wire::LOSTTORN_LEAD),
        },
        _ => return Err(minix_types::EINVAL),
    };
    let mut rule = FaultRule::new(read_u64(wire::START), read_u64(wire::END), flags, action);
    rule.skip = skip;
    rule.count = count;
    Ok(rule)
}

/// Encode one rule for `FBDCGETRULE`, with the given 1-based number.
pub fn encode_rule(number: i32, rule: &FaultRule) -> [u8; RULE_SIZE] {
    let mut raw = [0u8; RULE_SIZE];
    fn write_u32(raw: &mut [u8; RULE_SIZE], offset: usize, value: u32) {
        raw[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn write_u64(raw: &mut [u8; RULE_SIZE], offset: usize, value: u64) {
        raw[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    let write_u32 =
        |raw: &mut [u8; RULE_SIZE], offset: usize, value: u32| write_u32(raw, offset, value);
    let write_u64 =
        |raw: &mut [u8; RULE_SIZE], offset: usize, value: u64| write_u64(raw, offset, value);
    write_u32(&mut raw, wire::NUM, number as u32);
    write_u64(&mut raw, wire::START, rule.start);
    write_u64(&mut raw, wire::END, rule.end);
    write_u32(&mut raw, wire::FLAGS, rule.flags);
    write_u32(&mut raw, wire::SKIP, rule.skip);
    write_u32(&mut raw, wire::COUNT, rule.count);
    match rule.action {
        FaultAction::Corrupt(kind) => {
            write_u32(&mut raw, wire::ACTION, ACTION_CORRUPT as u32);
            write_u32(
                &mut raw,
                wire::CORRUPT_TYPE,
                match kind {
                    CorruptKind::Zero => CORRUPT_ZERO,
                    CorruptKind::Persist => CORRUPT_PERSIST,
                    CorruptKind::Random => CORRUPT_RANDOM,
                } as u32,
            );
        }
        FaultAction::Error(code) => {
            write_u32(&mut raw, wire::ACTION, ACTION_ERROR as u32);
            write_u32(&mut raw, wire::ERROR_CODE, code as u32);
        }
        FaultAction::Misdirect { start, end, align } => {
            write_u32(&mut raw, wire::ACTION, ACTION_MISDIR as u32);
            write_u64(&mut raw, wire::MISDIR_START, start);
            write_u64(&mut raw, wire::MISDIR_END, end);
            write_u32(&mut raw, wire::MISDIR_ALIGN, align);
        }
        FaultAction::LostTorn { lead } => {
            write_u32(&mut raw, wire::ACTION, ACTION_LOSTTORN as u32);
            write_u32(&mut raw, wire::LOSTTORN_LEAD, lead);
        }
    }
    raw
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_size_and_offsets_match_ioc_fbd_header() {
        assert_eq!(RULE_SIZE, 64);
        assert_eq!(wire::NUM, 0);
        assert_eq!(wire::START, 8);
        assert_eq!(wire::END, 16);
        assert_eq!(wire::FLAGS, 24);
        assert_eq!(wire::SKIP, 28);
        assert_eq!(wire::COUNT, 32);
        assert_eq!(wire::ACTION, 36);
        assert_eq!(wire::PARAMS, 40);
        assert_eq!(wire::MISDIR_ALIGN, 56);
    }

    #[test]
    fn test_ioctl_numbers_derive_from_the_ioc_macro() {
        assert_eq!(FBDCADDRULE, 0x8040_4201);
        assert_eq!(FBDCDELRULE, 0x8004_4202);
        assert_eq!(FBDCGETRULE, 0xC040_4203);
        assert_eq!((FBDCADDRULE >> 8) & 0xff, b'B' as u64);
        assert_eq!(MAX_RULES, 16);
        assert_eq!(FLAG_READ, 0x1);
        assert_eq!(FLAG_WRITE, 0x2);
    }

    #[test]
    fn test_rule_round_trips_through_the_wire() {
        let cases = [
            FaultRule::new(
                0x1000,
                0x2000,
                FLAG_READ,
                FaultAction::Error(minix_types::EIO),
            ),
            FaultRule::new(0, 0, FLAG_WRITE, FaultAction::Corrupt(CorruptKind::Persist)),
            FaultRule::new(
                0,
                0,
                FLAG_WRITE,
                FaultAction::Misdirect {
                    start: 0x1000,
                    end: 0x2000,
                    align: 512,
                },
            ),
            FaultRule::new(0, 0, FLAG_WRITE, FaultAction::LostTorn { lead: 8 }),
        ];
        for rule in cases {
            let mut live = rule;
            live.skip = 2;
            live.count = 3;
            let raw = encode_rule(4, &live);
            assert_eq!(u32::from_le_bytes(raw[0..4].try_into().unwrap()), 4);
            assert_eq!(decode_rule(&raw), Ok(live));
        }
        // An unknown action or corruption type is refused.
        let mut raw = encode_rule(1, &cases[0]);
        raw[wire::ACTION..wire::ACTION + 4].copy_from_slice(&9u32.to_le_bytes());
        assert_eq!(decode_rule(&raw), Err(minix_types::EINVAL));
    }
}
