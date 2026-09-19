//! Terminal attributes: the `struct termios` ABI and the flag bits the
//! `stty` deciding half names.
//!
//! C correspondence: `minix3/sys/sys/termios.h` — the struct at lines
//! 192 to 200 (four mode words, twenty control characters, two speed
//! words; 44 bytes on LP64, no internal padding) and the flag values at
//! lines 90 to 176. Control-character slots (`VMIN` 16, `VTIME` 17, and
//! friends) live at lines 44 to 74.
//!
//! Every constant is the C value verbatim; the pin tests at the bottom
//! lock each number and the struct layout, so a drift breaks the build
//! here instead of on a terminal.

/// Input flag: strip eighth bit (`termios.h:95`, `ISTRIP`).
pub const ISTRIP: u32 = 0x0000_0020;
/// Input flag: map CR to NL (`termios.h:98`, `ICRNL`).
pub const ICRNL: u32 = 0x0000_0100;
/// Input flag: enable output flow control (`termios.h:99`, `IXON`).
pub const IXON: u32 = 0x0000_0200;
/// Input flag: enable input flow control (`termios.h:100`, `IXOFF`).
pub const IXOFF: u32 = 0x0000_0400;

/// Output flag: enable output processing (`termios.h:111`, `OPOST`).
pub const OPOST: u32 = 0x0000_0001;
/// Output flag: map NL to CR-NL (`termios.h:113`, `ONLCR`).
pub const ONLCR: u32 = 0x0000_0002;

/// Control flag: character size mask (`termios.h:131`, `CSIZE`).
pub const CSIZE: u32 = 0x0000_0300;
/// Control flag: eight bits (`termios.h:135`, `CS8`).
pub const CS8: u32 = 0x0000_0300;
/// Control flag: two stop bits (`termios.h:136`, `CSTOPB`).
pub const CSTOPB: u32 = 0x0000_0400;
/// Control flag: enable receiver (`termios.h:137`, `CREAD`).
pub const CREAD: u32 = 0x0000_0800;
/// Control flag: parity enable (`termios.h:138`, `PARENB`).
pub const PARENB: u32 = 0x0000_1000;
/// Control flag: odd parity (`termios.h:139`, `PARODD`).
pub const PARODD: u32 = 0x0000_2000;
/// Control flag: hang up on last close (`termios.h:140`, `HUPCL`).
pub const HUPCL: u32 = 0x0000_4000;

/// Local flag: visually erase characters (`termios.h:163`, `ECHOE`).
pub const ECHOE: u32 = 0x0000_0002;
/// Local flag: echo NL after line kill (`termios.h:164`, `ECHOK`).
pub const ECHOK: u32 = 0x0000_0004;
/// Local flag: enable echoing (`termios.h:165`, `ECHO`).
pub const ECHO: u32 = 0x0000_0008;
/// Local flag: enable signals (`termios.h:171`, `ISIG`).
pub const ISIG: u32 = 0x0000_0080;
/// Local flag: canonical input (`termios.h:172`, `ICANON`).
pub const ICANON: u32 = 0x0000_0100;
/// Local flag: enable DISCARD/LNEXT (`termios.h:176`, `IEXTEN`).
pub const IEXTEN: u32 = 0x0000_0400;

/// Control character slot: end of file (`termios.h:47`, `VEOF`).
pub const VEOF: usize = 0;
/// Control character slot: erase (`termios.h:52`, `VERASE`).
pub const VERASE: usize = 3;
/// Control character slot: intr (`termios.h:61`, `VINTR`).
pub const VINTR: usize = 8;
/// Control character slot: quit (`termios.h:62`, `VQUIT`).
pub const VQUIT: usize = 9;
/// Control character slot: susp (`termios.h:63`, `VSUSP`).
pub const VSUSP: usize = 10;
/// Control character slot: start (`termios.h:67`, `VSTART`).
pub const VSTART: usize = 12;
/// Control character slot: stop (`termios.h:68`, `VSTOP`).
pub const VSTOP: usize = 13;
/// Control character slot: min (`termios.h:73`, `VMIN`).
pub const VMIN: usize = 16;
/// Control character slot: time (`termios.h:74`, `VTIME`).
pub const VTIME: usize = 17;
/// Control character slots in the struct (`termios.h:79`, `NCCS 20`).
pub const NCCS: usize = 20;

/// The terminal attribute record (`termios.h:192-200`): four mode words,
/// twenty control characters, input and output speed. 44 bytes, no
/// internal padding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Termios {
    /// `c_iflag` — input modes.
    pub c_iflag: u32,
    /// `c_oflag` — output modes.
    pub c_oflag: u32,
    /// `c_cflag` — control modes.
    pub c_cflag: u32,
    /// `c_lflag` — local modes.
    pub c_lflag: u32,
    /// `c_cc` — control characters.
    pub c_cc: [u8; NCCS],
    /// `c_ispeed` — input speed.
    pub c_ispeed: i32,
    /// `c_ospeed` — output speed.
    pub c_ospeed: i32,
}

impl Termios {
    /// A blank record (all flags clear, control characters `0`, speeds 0).
    pub fn new() -> Self {
        Termios {
            c_iflag: 0,
            c_oflag: 0,
            c_cflag: 0,
            c_lflag: 0,
            c_cc: [0; NCCS],
            c_ispeed: 0,
            c_ospeed: 0,
        }
    }

    /// Serialize in memory order (`termios.h:192-200`): the disk/wire
    /// shape the ioctl face moves as one blob.
    pub fn to_bytes(&self, out: &mut [u8]) -> Option<usize> {
        if out.len() < 44 {
            return None;
        }
        out[0..4].copy_from_slice(&self.c_iflag.to_le_bytes());
        out[4..8].copy_from_slice(&self.c_oflag.to_le_bytes());
        out[8..12].copy_from_slice(&self.c_cflag.to_le_bytes());
        out[12..16].copy_from_slice(&self.c_lflag.to_le_bytes());
        out[16..36].copy_from_slice(&self.c_cc);
        out[36..40].copy_from_slice(&self.c_ispeed.to_le_bytes());
        out[40..44].copy_from_slice(&self.c_ospeed.to_le_bytes());
        Some(44)
    }

    /// Parse from memory order; short input is rejected.
    pub fn from_bytes(input: &[u8]) -> Option<Self> {
        if input.len() < 44 {
            return None;
        }
        Some(Termios {
            c_iflag: u32::from_le_bytes(input[0..4].try_into().ok()?),
            c_oflag: u32::from_le_bytes(input[4..8].try_into().ok()?),
            c_cflag: u32::from_le_bytes(input[8..12].try_into().ok()?),
            c_lflag: u32::from_le_bytes(input[12..16].try_into().ok()?),
            c_cc: input[16..36].try_into().ok()?,
            c_ispeed: i32::from_le_bytes(input[36..40].try_into().ok()?),
            c_ospeed: i32::from_le_bytes(input[40..44].try_into().ok()?),
        })
    }
}

impl Default for Termios {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// C 值逐位钉死：任何一枚旗标位与 `termios.h` 不符都在这里爆。
    #[test]
    fn test_flag_bits_match_c_header() {
        // 输入旗标（termios.h:90-100）。
        assert_eq!(ISTRIP, 0x0000_0020);
        assert_eq!(ICRNL, 0x0000_0100);
        assert_eq!(IXON, 0x0000_0200);
        assert_eq!(IXOFF, 0x0000_0400);
        // 输出旗标（termios.h:111-113）。
        assert_eq!(OPOST, 0x0000_0001);
        assert_eq!(ONLCR, 0x0000_0002);
        // 控制旗标（termios.h:131-140）。
        assert_eq!(CSIZE, 0x0000_0300);
        assert_eq!(CS8, 0x0000_0300);
        assert_eq!(CSTOPB, 0x0000_0400);
        assert_eq!(CREAD, 0x0000_0800);
        assert_eq!(PARENB, 0x0000_1000);
        assert_eq!(PARODD, 0x0000_2000);
        assert_eq!(HUPCL, 0x0000_4000);
        // 本地旗标（termios.h:161-176）。
        assert_eq!(ECHOE, 0x0000_0002);
        assert_eq!(ECHOK, 0x0000_0004);
        assert_eq!(ECHO, 0x0000_0008);
        assert_eq!(ISIG, 0x0000_0080);
        assert_eq!(ICANON, 0x0000_0100);
        assert_eq!(IEXTEN, 0x0000_0400);
    }

    /// 槽位与 `NCCS` 钉值（termios.h:44-79）。
    #[test]
    fn test_control_char_slots() {
        assert_eq!(VEOF, 0);
        assert_eq!(VERASE, 3);
        assert_eq!(VINTR, 8);
        assert_eq!(VQUIT, 9);
        assert_eq!(VSUSP, 10);
        assert_eq!(VSTART, 12);
        assert_eq!(VSTOP, 13);
        assert_eq!(VMIN, 16);
        assert_eq!(VTIME, 17);
        assert_eq!(NCCS, 20);
    }

    /// 结构体 44 字节且序列化按域序回读（LP64 无内垫）。
    #[test]
    fn test_round_trip_layout() {
        let mut t = Termios::new();
        t.c_iflag = ICRNL | IXON;
        t.c_oflag = OPOST | ONLCR;
        t.c_cflag = CS8 | CREAD | HUPCL;
        t.c_lflag = ISIG | ICANON | ECHO;
        t.c_cc[VEOF] = 4;
        t.c_cc[VINTR] = 3;
        t.c_ispeed = 9600;
        t.c_ospeed = 9600;

        let mut bytes = [0u8; 44];
        assert_eq!(t.to_bytes(&mut bytes), Some(44));
        assert_eq!(bytes.len(), 44);
        let back = Termios::from_bytes(&bytes).unwrap();
        assert_eq!(back, t);

        // 短输入拒绝。
        assert!(Termios::from_bytes(&bytes[..43]).is_none());
        let mut small = [0u8; 43];
        assert!(t.to_bytes(&mut small).is_none());
    }
}
