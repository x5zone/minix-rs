//! Rust-leg translation of minix3/minix/tests — the terminal domain (the
//! pinnable surface of test74/77).
//!
//! # Translation mapping
//!
//! The bulk of C test77 (termios/pty/setsid) and test74 (ioctl/select/mmap)
//! needs the tty/pty servers and real terminal devices; this file pins the
//! **contract surface that already exists**: the byte layout of the termios
//! record (the 44-byte block an ioctl moves in one go, C `termios.h:192-200`)
//! and the tty ioctl request numbers (the `_IOR/_IOW` expansions of C
//! `ttycom.h:88-89`). The pty session, setsid, and terminal
//! suspend/wakeup chain are follow-up entries for after the tty server is
//! brought up.
//!
//! | C test | Semantic home |
//! |---|---|
//! | test77 (termios record transfer) | [`termios_byte_layout_roundtrip_preserves_record`] |
//! | test77 (ioctl request-number ABI) | [`tty_ioctl_request_numbers_match_c_ttycom`] |
//! | test77 (pty session/setsid), test74 (device select/mmap) | Out of scope; follow-up once the tty surface is brought up |
//!
//! Delivery gate = compilation; every test is `#[ignore]`d, and the
//! precondition for enabling each one is given in its attribute.

use minix_sys::{TIOCGETA, TIOCSETA};

// ---------------------------------------------------------------------------
// test77 — termios record byte layout
// ---------------------------------------------------------------------------

/// C semantics (in-memory-order serialization, `termios.h:192-200`): the
/// termios record travels as one 44-byte block (the ioctl argument), and
/// serialization followed by deserialization restores all seven fields.
#[test]
#[ignore = "点亮前提:tty 服务器 ioctl 面随载体点亮后端到端复核"]
fn termios_byte_layout_roundtrip_preserves_record() {
    use minix_types::types::termios::{
        CREAD, CS8, ECHO, ICANON, ICRNL, ISIG, OPOST, Termios, VEOF, VINTR,
    };

    let mut record = Termios::new();
    record.c_iflag = ICRNL;
    record.c_oflag = OPOST;
    record.c_cflag = CS8 | CREAD;
    record.c_lflag = ISIG | ICANON | ECHO;
    record.c_cc[VEOF] = 0x04; // Ctrl-D
    record.c_cc[VINTR] = 0x03; // Ctrl-C
    record.c_ispeed = 9600;
    record.c_ospeed = 9600;

    let mut bytes = [0u8; 64];
    let written = record.to_bytes(&mut bytes).expect("44 字节记录可写入");
    let parsed = Termios::from_bytes(&bytes[..written]).expect("字节可反序列化");

    assert_eq!(parsed.c_iflag, ICRNL, "输入标志位还原");
    assert_eq!(parsed.c_oflag, OPOST, "输出标志位还原");
    assert_eq!(parsed.c_cflag, CS8 | CREAD, "控制标志位还原");
    assert_eq!(parsed.c_lflag, ISIG | ICANON | ECHO, "本地标志位还原");
    assert_eq!(
        parsed.c_cc[VEOF], 0x04,
        "EOF 控制字符还原(C test77 的 Raw/Canonical 面)"
    );
    assert_eq!(parsed.c_cc[VINTR], 0x03, "INTR 控制字符还原");
    assert_eq!((parsed.c_ispeed, parsed.c_ospeed), (9600, 9600), "速率还原");
}

// ---------------------------------------------------------------------------
// test77 — tty ioctl request-number ABI
// ---------------------------------------------------------------------------

/// C semantics (`ttycom.h:88-89`): TIOCGETA = `_IOR('t', 19, struct
/// termios)` = IOC_OUT(0x4000_0000) | 44-byte argument | group 't' | number
/// 19; TIOCSETA uses the same form with IOC_IN(0x8000_0000) and number 20.
/// The request numbers are the shared ABI between userland and the tty
/// server, pinned byte by byte.
#[test]
#[ignore = "点亮前提:tty 服务器 ioctl 面随载体点亮后端到端复核"]
fn tty_ioctl_request_numbers_match_c_ttycom() {
    // _IOR('t', 19, 44 bytes): direction out (0x4000_0000), length 44
    // (<<16), group code 't' (0x74), number 19 (0x13).
    assert_eq!(
        TIOCGETA,
        0x4000_0000 | (44 << 16) | (0x74 << 8) | 19,
        "TIOCGETA 逐位对账 C ttycom.h:88"
    );
    // _IOW('t', 20, 44 bytes): direction in (0x8000_0000), number 20
    // (0x14).
    assert_eq!(
        TIOCSETA,
        0x8000_0000 | (44 << 16) | (0x74 << 8) | 20,
        "TIOCSETA 逐位对账 C ttycom.h:89"
    );
    // The direction bits are mutually exclusive: GETA carries out-direction,
    // SETA carries in-direction.
    assert_eq!(TIOCGETA & 0xC000_0000, 0x4000_0000);
    assert_eq!(TIOCSETA & 0xC000_0000, 0x8000_0000);
}
